package guihost

import (
	"bufio"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net"
	"net/http"
	"net/http/cookiejar"
	"net/http/httptest"
	"net/url"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"testing"
	"testing/fstest"
	"time"

	"github.com/coder/websocket"

	"github.com/aqilputi/nekuroddo/internal/rpc/proto"
)

const upToken = "upstream-secret"

// seen is what the fake core saw of the last /v1/hello.
type seen struct {
	mu     sync.Mutex
	auth   string
	cookie string
	query  string
}

func (s *seen) set(r *http.Request) {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.auth, s.cookie, s.query = r.Header.Get("Authorization"), r.Header.Get("Cookie"), r.URL.RawQuery
}

func (s *seen) get() (auth, cookie, query string) {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.auth, s.cookie, s.query
}

// fakeCore is a protocol stand-in: /v1/hello, an SSE stream that sends one
// event, waits for release and sends another, and a WebSocket echo. With
// token set it demands it like rpc.Server on TCP.
func fakeCore(t *testing.T, token string, release <-chan struct{}) (http.Handler, *seen) {
	t.Helper()
	sn := &seen{}
	m := http.NewServeMux()
	m.HandleFunc("GET /v1/hello", func(w http.ResponseWriter, r *http.Request) {
		sn.set(r)
		w.Header().Set("Content-Type", "application/json")
		fmt.Fprint(w, `{"version":"fake"}`)
	})
	m.HandleFunc("GET /v1/events", func(w http.ResponseWriter, _ *http.Request) {
		w.Header().Set("Content-Type", "text/event-stream")
		w.WriteHeader(http.StatusOK)
		fmt.Fprint(w, "id: 1\nevent: one\ndata: {}\n\n")
		_ = http.NewResponseController(w).Flush()
		<-release
		fmt.Fprint(w, "id: 2\nevent: two\ndata: {}\n\n")
	})
	m.HandleFunc("GET /v1/panes/p1/ws", func(w http.ResponseWriter, r *http.Request) {
		c, err := websocket.Accept(w, r, &websocket.AcceptOptions{InsecureSkipVerify: true})
		if err != nil {
			return
		}
		defer c.CloseNow()
		for {
			typ, b, err := c.Read(r.Context())
			if err != nil {
				return
			}
			if err := c.Write(r.Context(), typ, append([]byte("echo:"), b...)); err != nil {
				return
			}
		}
	})
	var h http.Handler = m
	if token != "" {
		h = http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			if r.Header.Get("Authorization") != "Bearer "+token {
				writeErr(w, http.StatusUnauthorized, proto.CodeUnauthorized, "no")
				return
			}
			m.ServeHTTP(w, r)
		})
	}
	return h, sn
}

var dist = fstest.MapFS{
	"index.html":       {Data: []byte("<!doctype html><html><HEAD><title>nk</title></HEAD><body></body></html>")},
	"assets/app.js":    {Data: []byte("console.log('nk')")},
	"assets/style.css": {Data: []byte("body{}")},
}

func serve(t *testing.T, o Options) *Server {
	t.Helper()
	ctx, cancel := context.WithCancel(context.Background())
	s, err := Serve(ctx, o)
	if err != nil {
		cancel()
		t.Fatal(err)
	}
	t.Cleanup(func() {
		cancel()
		<-s.Done()
	})
	return s
}

// tcpCore serves h on a loopback port and returns its tcp:// address.
func tcpCore(t *testing.T, h http.Handler) string {
	t.Helper()
	ts := httptest.NewServer(h)
	t.Cleanup(ts.Close)
	return "tcp://" + strings.TrimPrefix(ts.URL, "http://")
}

// browser is a client with a cookie jar, like a page.
func browser(t *testing.T) *http.Client {
	t.Helper()
	jar, _ := cookiejar.New(nil)
	return &http.Client{Jar: jar, Timeout: 5 * time.Second}
}

// reply is the part of a response the tests look at (the body is read and
// closed by get).
type reply struct {
	StatusCode int
	Header     http.Header
	URL        *url.URL // after redirects
	cookies    []*http.Cookie
}

func (r reply) Cookies() []*http.Cookie { return r.cookies }

func get(t *testing.T, c *http.Client, url string, hdr ...string) (reply, string) {
	t.Helper()
	req, _ := http.NewRequest(http.MethodGet, url, nil)
	for i := 0; i+1 < len(hdr); i += 2 {
		req.Header.Set(hdr[i], hdr[i+1])
	}
	resp, err := c.Do(req)
	if err != nil {
		t.Fatal(err)
	}
	defer resp.Body.Close()
	b, _ := io.ReadAll(resp.Body)
	return reply{resp.StatusCode, resp.Header, resp.Request.URL, resp.Cookies()}, string(b)
}

func TestLaunchCookieAndIndexInjection(t *testing.T) {
	core, _ := fakeCore(t, "", nil)
	s := serve(t, Options{Assets: dist, Upstream: Upstream{Handler: core}, Token: "page-tok"})
	c := browser(t)

	// nothing without the launch key or the token
	if resp, _ := get(t, c, s.URL()+"/"); resp.StatusCode != http.StatusUnauthorized {
		t.Fatalf("/ without cookie = %d, want 401", resp.StatusCode)
	}
	launch := s.LaunchURL()
	if !strings.HasPrefix(launch, s.URL()+"/?nk=") {
		t.Fatalf("LaunchURL = %q", launch)
	}
	resp, body := get(t, c, launch)
	if resp.StatusCode != http.StatusOK || resp.URL.RawQuery != "" {
		t.Fatalf("launch: %d at %s", resp.StatusCode, resp.URL)
	}
	want := `<HEAD><script>window.nkEndpoint=()=>({url:location.origin,token:"page-tok"});</script><title>`
	if !strings.Contains(body, want) {
		t.Fatalf("index not injected:\n%s", body)
	}
	if resp.Header.Get("Cache-Control") != "no-store" {
		t.Errorf("index Cache-Control = %q", resp.Header.Get("Cache-Control"))
	}
	// the cookie now opens everything: assets, client routes, index.html
	if resp, body := get(t, c, s.URL()+"/assets/app.js"); resp.StatusCode != 200 || body != "console.log('nk')" {
		t.Errorf("asset: %d %q", resp.StatusCode, body)
	}
	for _, p := range []string{"/index.html", "/some/route"} {
		if resp, body := get(t, c, s.URL()+p); resp.StatusCode != 200 || !strings.Contains(body, "nkEndpoint") {
			t.Errorf("%s: %d", p, resp.StatusCode)
		}
	}
	if resp, _ := get(t, c, s.URL()+"/assets/missing.js"); resp.StatusCode != http.StatusNotFound {
		t.Errorf("missing asset = %d, want 404", resp.StatusCode)
	}
	if resp, _ := get(t, c, s.URL()+"/v1/hello"); resp.StatusCode != 200 {
		t.Errorf("/v1/hello with cookie = %d", resp.StatusCode)
	}
	// a reload of the launch URL with the cookie just drops the key
	if resp, _ := get(t, c, launch); resp.StatusCode != 200 || resp.URL.RawQuery != "" {
		t.Errorf("relaunch with cookie: %d at %s", resp.StatusCode, resp.URL)
	}
	// the key is one-time: another browser cannot use it
	other := browser(t)
	if resp, _ := get(t, other, launch); resp.StatusCode != http.StatusForbidden {
		t.Errorf("second launch = %d, want 403", resp.StatusCode)
	}
	if got := s.LaunchURL(); !strings.HasSuffix(got, "?nk=") {
		t.Errorf("LaunchURL after use = %q", got)
	}
	// a HEAD of the index has no body
	req, _ := http.NewRequest(http.MethodHead, s.URL()+"/", nil)
	req.Header.Set("Authorization", "Bearer page-tok")
	hr, err := http.DefaultClient.Do(req)
	if err != nil {
		t.Fatal(err)
	}
	hr.Body.Close()
	if hr.StatusCode != 200 || hr.Header.Get("Content-Type") != "text/html; charset=utf-8" {
		t.Errorf("HEAD / = %d %q", hr.StatusCode, hr.Header.Get("Content-Type"))
	}
}

func TestLaunchKeepsOtherQuery(t *testing.T) {
	core, _ := fakeCore(t, "", nil)
	s := serve(t, Options{Assets: dist, Upstream: Upstream{Handler: core}})
	c := browser(t)
	c.CheckRedirect = func(*http.Request, []*http.Request) error { return http.ErrUseLastResponse }
	resp, _ := get(t, c, s.LaunchURL()+"&view=flat")
	if resp.StatusCode != http.StatusSeeOther || resp.Header.Get("Location") != "/?view=flat" {
		t.Fatalf("launch = %d → %q", resp.StatusCode, resp.Header.Get("Location"))
	}
	ck := resp.Cookies()
	if len(ck) != 1 || !ck[0].HttpOnly || ck[0].SameSite != http.SameSiteStrictMode || ck[0].Value != s.Token() {
		t.Fatalf("cookie = %+v", ck)
	}
	if !strings.HasPrefix(ck[0].Name, "nk_session_") {
		t.Errorf("cookie name %q", ck[0].Name)
	}
}

func TestBadLaunchKey(t *testing.T) {
	core, _ := fakeCore(t, "", nil)
	s := serve(t, Options{Assets: dist, Upstream: Upstream{Handler: core}})
	resp, body := get(t, browser(t), s.URL()+"/?nk=wrong")
	if resp.StatusCode != http.StatusForbidden || !strings.Contains(body, proto.CodeUnauthorized) {
		t.Fatalf("bad key = %d %s", resp.StatusCode, body)
	}
}

func TestRejectsNonLoopbackHost(t *testing.T) {
	core, _ := fakeCore(t, "", nil)
	s := serve(t, Options{Assets: dist, Upstream: Upstream{Handler: core}, Token: "t"})
	req, _ := http.NewRequest(http.MethodGet, s.URL()+"/v1/hello", nil)
	req.Host = "evil.example:80"
	req.Header.Set("Authorization", "Bearer t")
	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		t.Fatal(err)
	}
	resp.Body.Close()
	if resp.StatusCode != http.StatusForbidden {
		t.Fatalf("rebinding Host = %d, want 403", resp.StatusCode)
	}
	// localhost is fine
	req.Host = "localhost"
	resp, err = http.DefaultClient.Do(req)
	if err != nil {
		t.Fatal(err)
	}
	resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		t.Fatalf("Host localhost = %d", resp.StatusCode)
	}
}

func TestAPIRequiresToken(t *testing.T) {
	core, _ := fakeCore(t, "", nil)
	s := serve(t, Options{Upstream: Upstream{Handler: core}, Token: "t"})
	resp, body := get(t, http.DefaultClient, s.URL()+"/v1/hello")
	if resp.StatusCode != http.StatusUnauthorized || resp.Header.Get("WWW-Authenticate") != "Bearer" {
		t.Fatalf("no token = %d", resp.StatusCode)
	}
	var e proto.Error
	if json.Unmarshal([]byte(body), &e) != nil || e.Code != proto.CodeUnauthorized {
		t.Fatalf("body %q", body)
	}
	if resp, _ := get(t, http.DefaultClient, s.URL()+"/v1/hello", "Authorization", "Bearer nope"); resp.StatusCode != 401 {
		t.Errorf("wrong token = %d", resp.StatusCode)
	}
	if resp, _ := get(t, http.DefaultClient, s.URL()+"/v1/hello?token=t"); resp.StatusCode != 200 {
		t.Errorf("?token= = %d", resp.StatusCode)
	}
	// no assets: the rest is 404 even with the token
	if resp, _ := get(t, http.DefaultClient, s.URL()+"/", "Authorization", "Bearer t"); resp.StatusCode != 404 {
		t.Errorf("/ without assets = %d", resp.StatusCode)
	}
}

func TestAssetsWithoutIndex(t *testing.T) {
	core, _ := fakeCore(t, "", nil)
	s := serve(t, Options{Assets: fstest.MapFS{"a.txt": {Data: []byte("a")}}, Upstream: Upstream{Handler: core}, Token: "t"})
	if resp, _ := get(t, http.DefaultClient, s.URL()+"/", "Authorization", "Bearer t"); resp.StatusCode != 404 {
		t.Errorf("/ without index = %d", resp.StatusCode)
	}
	if resp, body := get(t, http.DefaultClient, s.URL()+"/a.txt", "Authorization", "Bearer t"); resp.StatusCode != 200 || body != "a" {
		t.Errorf("a.txt = %d %q", resp.StatusCode, body)
	}
}

// The proxy hands the core its own token and never the page's.
func TestProxyScrubsCredentials(t *testing.T) {
	core, sn := fakeCore(t, upToken, nil)
	for name, up := range map[string]Upstream{
		"tcp":  {Addr: tcpCore(t, core), Token: upToken},
		"http": {Addr: "http://" + strings.TrimPrefix(tcpCore(t, core), "tcp://") + "/", Token: upToken},
	} {
		t.Run(name, func(t *testing.T) {
			s := serve(t, Options{Upstream: up, Token: "page"})
			req, _ := http.NewRequest(http.MethodGet, s.URL()+"/v1/hello?token=page&x=1", nil)
			req.AddCookie(&http.Cookie{Name: "nk_session_1", Value: "c"})
			resp, err := http.DefaultClient.Do(req)
			if err != nil {
				t.Fatal(err)
			}
			b, _ := io.ReadAll(resp.Body)
			resp.Body.Close()
			if resp.StatusCode != 200 || !strings.Contains(string(b), "fake") {
				t.Fatalf("proxied hello = %d %s", resp.StatusCode, b)
			}
			auth, cookie, query := sn.get()
			if auth != "Bearer "+upToken || cookie != "" || query != "x=1" {
				t.Fatalf("core saw auth=%q cookie=%q query=%q", auth, cookie, query)
			}
		})
	}
}

func TestInProcessScrubs(t *testing.T) {
	core, sn := fakeCore(t, "", nil)
	s := serve(t, Options{Upstream: Upstream{Handler: core}, Token: "page"})
	resp, _ := get(t, http.DefaultClient, s.URL()+"/v1/hello?token=page", "Cookie", "a=b")
	if resp.StatusCode != 200 {
		t.Fatalf("hello = %d", resp.StatusCode)
	}
	if auth, cookie, query := sn.get(); auth != "" || cookie != "" || query != "" {
		t.Fatalf("core saw auth=%q cookie=%q query=%q", auth, cookie, query)
	}
}

// A core that wants a token the host does not have answers 401 through
// the proxy.
func TestProxyUpstream401(t *testing.T) {
	core, _ := fakeCore(t, upToken, nil)
	s := serve(t, Options{Upstream: Upstream{Addr: tcpCore(t, core)}, Token: "page"})
	resp, body := get(t, http.DefaultClient, s.URL()+"/v1/hello", "Authorization", "Bearer page")
	if resp.StatusCode != http.StatusUnauthorized || !strings.Contains(body, proto.CodeUnauthorized) {
		t.Fatalf("upstream without token = %d %s", resp.StatusCode, body)
	}
}

func TestProxyUnixSocket(t *testing.T) {
	core, _ := fakeCore(t, "", nil)
	sock := filepath.Join(t.TempDir(), "nk.sock")
	l, err := net.Listen("unix", sock)
	if err != nil {
		t.Fatal(err)
	}
	hs := &http.Server{Handler: core, ReadHeaderTimeout: time.Second}
	go func() { _ = hs.Serve(l) }()
	t.Cleanup(func() { hs.Close() })
	s := serve(t, Options{Upstream: Upstream{Addr: "unix://" + sock}, Token: "page"})
	resp, body := get(t, http.DefaultClient, s.URL()+"/v1/hello", "Authorization", "Bearer page")
	if resp.StatusCode != 200 || !strings.Contains(body, "fake") {
		t.Fatalf("unix hello = %d %s", resp.StatusCode, body)
	}
}

func TestProxyCoreDown(t *testing.T) {
	var logged []string
	var mu sync.Mutex
	logf := func(f string, a ...any) {
		mu.Lock()
		defer mu.Unlock()
		logged = append(logged, fmt.Sprintf(f, a...))
	}
	sock := filepath.Join(t.TempDir(), "none.sock")
	s := serve(t, Options{Upstream: Upstream{Addr: "unix://" + sock}, Token: "page", Logf: logf})
	resp, body := get(t, http.DefaultClient, s.URL()+"/v1/hello?token=page")
	if resp.StatusCode != http.StatusBadGateway || !strings.Contains(body, proto.CodeUnavailable) {
		t.Fatalf("core down = %d %s", resp.StatusCode, body)
	}
	mu.Lock()
	defer mu.Unlock()
	if len(logged) != 1 || strings.Contains(logged[0], "page") || !strings.Contains(logged[0], "GET /v1/hello") {
		t.Fatalf("logged %q", logged)
	}
}

// SSE goes through unbuffered: the first event arrives while the core is
// still holding the stream open.
func TestProxySSEStreams(t *testing.T) {
	release := make(chan struct{})
	core, _ := fakeCore(t, upToken, release)
	cases := map[string]Upstream{
		"proxy": {Addr: tcpCore(t, core), Token: upToken},
		"in-process": {Handler: http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			r.Header.Set("Authorization", "Bearer "+upToken)
			core.ServeHTTP(w, r)
		})},
	}
	for name, up := range cases {
		t.Run(name, func(t *testing.T) {
			s := serve(t, Options{Upstream: up, Token: "page"})
			ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
			defer cancel()
			req, _ := http.NewRequestWithContext(ctx, http.MethodGet, s.URL()+"/v1/events?token=page", nil)
			resp, err := http.DefaultClient.Do(req)
			if err != nil {
				t.Fatal(err)
			}
			defer resp.Body.Close()
			if ct := resp.Header.Get("Content-Type"); ct != "text/event-stream" {
				t.Fatalf("content-type %q", ct)
			}
			rd := bufio.NewReader(resp.Body)
			readEvent := func() string {
				for {
					line, err := rd.ReadString('\n')
					if err != nil {
						t.Fatalf("read: %v", err)
					}
					if ev, ok := strings.CutPrefix(line, "event: "); ok {
						return strings.TrimSpace(ev)
					}
				}
			}
			if ev := readEvent(); ev != "one" {
				t.Fatalf("first event %q", ev)
			}
			release <- struct{}{}
			if ev := readEvent(); ev != "two" {
				t.Fatalf("second event %q", ev)
			}
		})
	}
}

// Pane WebSockets go through the proxy (Upgrade passthrough) with the
// token in the query, as browsers send it.
func TestProxyWebSocketEcho(t *testing.T) {
	core, _ := fakeCore(t, upToken, nil)
	s := serve(t, Options{Upstream: Upstream{Addr: tcpCore(t, core), Token: upToken}, Token: "page"})
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	ws := "ws" + strings.TrimPrefix(s.URL(), "http") + "/v1/panes/p1/ws?token=page"
	c, resp, err := websocket.Dial(ctx, ws, nil)
	if resp != nil && resp.Body != nil {
		resp.Body.Close()
	}
	if err != nil {
		t.Fatal(err)
	}
	defer c.CloseNow()
	for _, msg := range []string{"hi", "ls\r"} {
		if err := c.Write(ctx, websocket.MessageBinary, []byte(msg)); err != nil {
			t.Fatal(err)
		}
		typ, b, err := c.Read(ctx)
		if err != nil {
			t.Fatal(err)
		}
		if typ != websocket.MessageBinary || string(b) != "echo:"+msg {
			t.Fatalf("echo = %v %q", typ, b)
		}
	}
	// without the page's token the upgrade is refused
	_, resp, err = websocket.Dial(ctx, strings.TrimSuffix(ws, "?token=page"), nil)
	if resp != nil && resp.Body != nil {
		resp.Body.Close()
	}
	if err == nil || resp == nil || resp.StatusCode != http.StatusUnauthorized {
		t.Fatalf("ws without token: %v %v", err, resp)
	}
	// Close ends an open socket
	if err := s.Close(); err != nil {
		t.Fatal(err)
	}
	if _, _, err := c.Read(ctx); err == nil {
		t.Fatal("socket still open after Close")
	}
}

func TestServeOptionErrors(t *testing.T) {
	ctx := context.Background()
	for name, o := range map[string]Options{
		"not loopback": {Listen: "0.0.0.0:0", Upstream: Upstream{Addr: "tcp://127.0.0.1:1"}},
		"bad listen":   {Listen: "nope", Upstream: Upstream{Addr: "tcp://127.0.0.1:1"}},
		"no upstream":  {},
		"bad scheme":   {Upstream: Upstream{Addr: "ftp://x"}},
		"empty unix":   {Upstream: Upstream{Addr: "unix://"}},
		"empty tcp":    {Upstream: Upstream{Addr: "tcp://"}},
	} {
		t.Run(name, func(t *testing.T) {
			if s, err := Serve(ctx, o); err == nil {
				s.Close()
				t.Fatal("Serve accepted bad options")
			}
		})
	}
	if _, err := Serve(ctx, Options{Listen: "10.1.2.3:0", Upstream: Upstream{Addr: "tcp://127.0.0.1:1"}}); !errors.Is(err, ErrNotLoopback) {
		t.Errorf("err = %v, want ErrNotLoopback", err)
	}
}

func TestListenBusyPort(t *testing.T) {
	l, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	defer l.Close()
	if s, err := Serve(context.Background(), Options{Listen: l.Addr().String(), Upstream: Upstream{Addr: "tcp://127.0.0.1:1"}}); err == nil {
		s.Close()
		t.Fatal("listened on a busy port")
	}
}

func TestContextEndsServer(t *testing.T) {
	ctx, cancel := context.WithCancel(context.Background())
	s, err := Serve(ctx, Options{Listen: "localhost:0", Upstream: Upstream{Addr: "tcp://127.0.0.1:1"}})
	if err != nil {
		t.Fatal(err)
	}
	cancel()
	select {
	case <-s.Done():
	case <-time.After(5 * time.Second):
		t.Fatal("server still running after ctx ended")
	}
	if err := s.Err(); err != nil {
		t.Errorf("Err after cancel = %v", err)
	}
	if !strings.HasPrefix(s.URL(), "http://127.0.0.1:") || s.Token() == "" {
		t.Errorf("URL %q token %q", s.URL(), s.Token())
	}
}

func TestInjectEndpoint(t *testing.T) {
	for _, tc := range []struct{ in, want string }{
		{"<html><head><title>x</title></head></html>", `<html><head><script>window.nkEndpoint=()=>({url:location.origin,token:"a\"b"});</script><title>`},
		{"<p>no head</p>", `<script>window.nkEndpoint=()=>({url:location.origin,token:"a\"b"});</script><p>no head</p>`},
	} {
		if got := string(injectEndpoint([]byte(tc.in), `a"b`)); !strings.HasPrefix(got, tc.want) {
			t.Errorf("inject(%q) = %q", tc.in, got)
		}
	}
}

func TestHostname(t *testing.T) {
	for in, want := range map[string]string{
		"127.0.0.1:80": "127.0.0.1", "[::1]:8": "::1", "localhost": "localhost", "[::1]": "::1",
	} {
		if got := hostname(in); got != want {
			t.Errorf("hostname(%q) = %q, want %q", in, got, want)
		}
	}
	for h, want := range map[string]bool{"localhost": true, "::1": true, "127.0.0.2": true, "example.com": false, "10.0.0.1": false} {
		if loopbackHost(h) != want {
			t.Errorf("loopbackHost(%q) != %v", h, want)
		}
	}
}

// The in-process upstream also serves a real directory (nkg --dev).
func TestDirFSAssets(t *testing.T) {
	dir := t.TempDir()
	if err := os.WriteFile(filepath.Join(dir, "index.html"), []byte("<head></head>"), 0o600); err != nil {
		t.Fatal(err)
	}
	core, _ := fakeCore(t, "", nil)
	s := serve(t, Options{Assets: os.DirFS(dir), Upstream: Upstream{Handler: core}, Token: "t"})
	if resp, body := get(t, http.DefaultClient, s.URL()+"/", "Authorization", "Bearer t"); resp.StatusCode != 200 || !strings.Contains(body, `token:"t"`) {
		t.Fatalf("dir index = %d %q", resp.StatusCode, body)
	}
}
