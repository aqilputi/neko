// Package guihost serves the GUI's frontend and the core's protocol from one
// loopback origin, so the page never makes a cross-origin call (the core's
// RPC server sends no CORS headers). It is plan §4.9's host for nkg: the
// built frontend (gui/web/dist) at /, and /v1/** — HTTP, SSE and the pane
// WebSockets — handed to the core, either an in-process handler
// (rpc.Server.Handler) or a reverse proxy to a running `nk serve` on a unix
// socket or a loopback TCP port.
//
// Access: the host makes its own token. The window (or browser) opens the
// launch URL once, with a one-time key that is traded for an HttpOnly,
// SameSite=Strict cookie; index.html is then served with
// window.nkEndpoint() (the frontend's endpoint hook) returning the page's
// origin and the token, which the client sends as a bearer token (or
// ?token= on EventSource/WebSocket URLs). Requests whose Host is not a
// loopback name are refused (DNS rebinding). The proxy strips the page's
// token and cookie and adds the upstream's own token, so neither leaks into
// the core or its logs.
//
// guihost is pure Go (no Wails, no cgo): the shell only loads URL().
package guihost

import (
	"bytes"
	"context"
	"crypto/rand"
	"crypto/subtle"
	"encoding/json"
	"errors"
	"fmt"
	"io/fs"
	"net"
	"net/http"
	"net/http/httputil"
	"net/url"
	"path"
	"strings"
	"sync"
	"time"

	"github.com/aqilputi/nekuroddo/internal/rpc/proto"
)

// LaunchParam is the query parameter of the launch URL that carries the
// one-time key.
const LaunchParam = "nk"

// Upstream is where /v1/** goes: Handler when set, else Addr.
type Upstream struct {
	// Handler serves the protocol in this process with no authentication
	// of its own (rpc.Server.Handler()).
	Handler http.Handler
	// Addr is a running server: unix:///path, tcp://127.0.0.1:port or
	// http://127.0.0.1:port.
	Addr string
	// Token is the bearer token Addr requires (a TCP listener); empty for a
	// unix socket.
	Token string
}

// Options configure Serve.
type Options struct {
	// Assets is the built frontend, index.html at its root; nil serves
	// only the protocol (and 404 elsewhere).
	Assets fs.FS
	// Upstream is the core.
	Upstream Upstream
	// Listen is the loopback host:port to listen on ("" = 127.0.0.1:0).
	Listen string
	// Token is the page's token ("" = a fresh random one).
	Token string
	// Logf, when set, reports proxy errors (method and path only, never a
	// query: it may carry the token).
	Logf func(format string, args ...any)
}

// Server is a running host. It stops when Serve's context ends or on Close.
type Server struct {
	o      Options
	token  string
	cookie string // per-port cookie name: two hosts on 127.0.0.1 share cookies
	url    string
	core   http.Handler
	assets http.Handler
	index  []byte // index.html with the endpoint script, nil if absent

	mu  sync.Mutex
	key string // one-time launch key; "" once redeemed

	hs     *http.Server
	cancel context.CancelFunc // ends every request, hijacked ones too
	done   chan struct{}
	err    error
}

// ErrNotLoopback: Options.Listen must name a loopback address.
var ErrNotLoopback = errors.New("guihost: só escuta em loopback")

// Serve listens on o.Listen and serves until ctx ends or Close.
func Serve(ctx context.Context, o Options) (*Server, error) {
	if o.Listen == "" {
		o.Listen = "127.0.0.1:0"
	}
	host, _, err := net.SplitHostPort(o.Listen)
	if err != nil {
		return nil, fmt.Errorf("guihost: %w", err)
	}
	if !loopbackHost(host) {
		return nil, fmt.Errorf("%w: %s", ErrNotLoopback, host)
	}
	core, err := coreHandler(o.Upstream, o.Logf)
	if err != nil {
		return nil, err
	}
	s := &Server{o: o, token: o.Token, core: core, key: rand.Text(), done: make(chan struct{})}
	if s.token == "" {
		s.token = rand.Text()
	}
	if o.Assets != nil {
		s.assets = http.FileServerFS(o.Assets)
		if b, err := fs.ReadFile(o.Assets, "index.html"); err == nil {
			s.index = injectEndpoint(b, s.token)
		}
	}
	l, err := net.Listen("tcp", o.Listen)
	if err != nil {
		return nil, fmt.Errorf("guihost: %w", err)
	}
	port := l.Addr().(*net.TCPAddr).Port
	s.url = fmt.Sprintf("http://%s", net.JoinHostPort(l.Addr().(*net.TCPAddr).IP.String(), fmt.Sprint(port)))
	s.cookie = fmt.Sprintf("nk_session_%d", port)
	base, cancel := context.WithCancel(context.Background())
	s.cancel = cancel
	s.hs = &http.Server{Handler: s.Handler(), ReadHeaderTimeout: 10 * time.Second,
		BaseContext: func(net.Listener) context.Context { return base }}
	go func() {
		err := s.hs.Serve(l)
		if !errors.Is(err, http.ErrServerClosed) {
			s.err = err
		}
		close(s.done)
	}()
	go func() {
		select {
		case <-ctx.Done():
			_ = s.Close()
		case <-s.done:
		}
	}()
	return s, nil
}

// URL is the host's origin (http://127.0.0.1:PORT), with no secret in it.
func (s *Server) URL() string { return s.url }

// LaunchURL is URL with the one-time key: the first page load trades it
// for the session cookie (later loads need the cookie). Hand it to one
// window or browser; do not log it.
func (s *Server) LaunchURL() string {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.url + "/?" + LaunchParam + "=" + s.key
}

// Token is the page's bearer token (for clients other than the page).
func (s *Server) Token() string { return s.token }

// Done is closed once the server has stopped.
func (s *Server) Done() <-chan struct{} { return s.done }

// Err is why the server stopped on its own (nil after Close).
func (s *Server) Err() error {
	<-s.done
	return s.err
}

// Close stops the server, ending open streams and sockets.
func (s *Server) Close() error {
	s.cancel()
	err := s.hs.Close()
	<-s.done
	return err
}

// Handler is the host's routing: loopback Host check, the launch key, the
// token, then /v1/** to the core and the rest to the frontend.
func (s *Server) Handler() http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if !loopbackHost(hostname(r.Host)) {
			writeErr(w, http.StatusForbidden, proto.CodeUnauthorized, "host não é loopback")
			return
		}
		if r.URL.Query().Has(LaunchParam) {
			s.launch(w, r)
			return
		}
		if !s.authorized(r) {
			w.Header().Set("WWW-Authenticate", "Bearer")
			if isAPI(r.URL.Path) {
				writeErr(w, http.StatusUnauthorized, proto.CodeUnauthorized, "token ausente ou inválido")
				return
			}
			w.Header().Set("Content-Type", "text/plain; charset=utf-8")
			w.WriteHeader(http.StatusUnauthorized)
			fmt.Fprintln(w, "nekuroddo: abra a janela pelo nkg (o link de abertura vale uma vez)")
			return
		}
		if isAPI(r.URL.Path) {
			s.core.ServeHTTP(w, r)
			return
		}
		s.serveAsset(w, r)
	})
}

func isAPI(p string) bool { return p == "/v1" || strings.HasPrefix(p, "/v1/") }

// launch trades the one-time key for the session cookie and redirects to
// the same URL without the key. A page that already has the cookie (a
// reload of the launch URL) is just redirected.
func (s *Server) launch(w http.ResponseWriter, r *http.Request) {
	if !s.authorized(r) {
		got := r.URL.Query().Get(LaunchParam)
		s.mu.Lock()
		ok := s.key != "" && subtle.ConstantTimeCompare([]byte(got), []byte(s.key)) == 1
		if ok {
			s.key = "" // one time only
		}
		s.mu.Unlock()
		if !ok {
			writeErr(w, http.StatusForbidden, proto.CodeUnauthorized, "link de abertura inválido ou já usado")
			return
		}
		http.SetCookie(w, &http.Cookie{Name: s.cookie, Value: s.token, Path: "/",
			HttpOnly: true, SameSite: http.SameSiteStrictMode})
	}
	u := *r.URL
	q := u.Query()
	q.Del(LaunchParam)
	u.RawQuery = q.Encode()
	u.Scheme, u.Host = "", ""
	w.Header().Set("Cache-Control", "no-store")
	http.Redirect(w, r, u.String(), http.StatusSeeOther)
}

// authorized: the session cookie, a bearer token or ?token=.
func (s *Server) authorized(r *http.Request) bool {
	want := []byte(s.token)
	eq := func(got string) bool { return got != "" && subtle.ConstantTimeCompare([]byte(got), want) == 1 }
	if c, err := r.Cookie(s.cookie); err == nil && eq(c.Value) {
		return true
	}
	if h, ok := strings.CutPrefix(r.Header.Get("Authorization"), "Bearer "); ok && eq(h) {
		return true
	}
	return eq(r.URL.Query().Get("token"))
}

// serveAsset serves a file of the frontend; "/", index.html and any path
// that is not a file (client-side routes) get the injected index.
func (s *Server) serveAsset(w http.ResponseWriter, r *http.Request) {
	if s.assets == nil {
		http.NotFound(w, r)
		return
	}
	p := strings.TrimPrefix(path.Clean("/"+r.URL.Path), "/")
	if p != "" && p != "index.html" {
		if fi, err := fs.Stat(s.o.Assets, p); err == nil && !fi.IsDir() {
			s.assets.ServeHTTP(w, r)
			return
		}
		if path.Ext(p) != "" { // a missing file, not a route
			http.NotFound(w, r)
			return
		}
	}
	if s.index == nil {
		http.Error(w, "nekuroddo: frontend sem index.html (rode make gui-build)", http.StatusNotFound)
		return
	}
	h := w.Header()
	h.Set("Content-Type", "text/html; charset=utf-8")
	h.Set("Cache-Control", "no-store")
	h.Set("Referrer-Policy", "no-referrer")
	if r.Method == http.MethodHead {
		return
	}
	_, _ = w.Write(s.index)
}

// injectEndpoint adds, first thing in <head>, the script the frontend's
// getEndpoint() looks for: the page's own origin and the token.
func injectEndpoint(index []byte, token string) []byte {
	tok, _ := json.Marshal(token)
	script := []byte(`<script>window.nkEndpoint=()=>({url:location.origin,token:` + string(tok) + `});</script>`)
	at := 0
	if i := bytes.Index(bytes.ToLower(index), []byte("<head>")); i >= 0 {
		at = i + len("<head>")
	}
	out := make([]byte, 0, len(index)+len(script))
	out = append(out, index[:at]...)
	out = append(out, script...)
	return append(out, index[at:]...)
}

// coreHandler is /v1/** for u: the in-process handler or a reverse proxy.
// Either way the page's credentials are taken off the request.
func coreHandler(u Upstream, logf func(string, ...any)) (http.Handler, error) {
	if u.Handler != nil {
		next := u.Handler
		return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			r2 := r.Clone(r.Context())
			scrub(r2.Header, r2.URL, "")
			r2.RequestURI = r2.URL.RequestURI()
			next.ServeHTTP(w, r2)
		}), nil
	}
	target, tr, err := dialTarget(u.Addr)
	if err != nil {
		return nil, err
	}
	rp := &httputil.ReverseProxy{
		Rewrite: func(pr *httputil.ProxyRequest) {
			pr.SetURL(target)
			scrub(pr.Out.Header, pr.Out.URL, u.Token)
		},
		Transport:     tr,
		FlushInterval: -1, // SSE: every write goes out at once
		ErrorHandler: func(w http.ResponseWriter, r *http.Request, err error) {
			if logf != nil {
				logf("guihost: %s %s: %v", r.Method, r.URL.Path, err)
			}
			writeErr(w, http.StatusBadGateway, proto.CodeUnavailable, "core indisponível")
		},
	}
	return rp, nil
}

// scrub drops the page's credentials (cookie, bearer, ?token=) and sets
// the upstream's token, if any.
func scrub(h http.Header, u *url.URL, token string) {
	h.Del("Cookie")
	h.Del("Authorization")
	if q := u.Query(); q.Has("token") {
		q.Del("token")
		u.RawQuery = q.Encode()
	}
	if token != "" {
		h.Set("Authorization", "Bearer "+token)
	}
}

// dialTarget turns an upstream address into the proxy's target URL and
// transport.
func dialTarget(addr string) (*url.URL, *http.Transport, error) {
	tr := http.DefaultTransport.(*http.Transport).Clone()
	tr.Proxy = nil // a loopback core never goes through $HTTP_PROXY
	scheme, rest, _ := strings.Cut(addr, "://")
	switch scheme {
	case "unix":
		if rest == "" {
			return nil, nil, errors.New("guihost: unix:// sem caminho")
		}
		tr.DialContext = func(ctx context.Context, _, _ string) (net.Conn, error) {
			var d net.Dialer
			return d.DialContext(ctx, "unix", rest)
		}
		return &url.URL{Scheme: "http", Host: "nk"}, tr, nil
	case "tcp", "http":
		rest = strings.TrimSuffix(rest, "/")
		if rest == "" {
			return nil, nil, fmt.Errorf("guihost: %s:// sem endereço", scheme)
		}
		return &url.URL{Scheme: "http", Host: rest}, tr, nil
	}
	return nil, nil, fmt.Errorf("guihost: upstream %q: use unix:///caminho, tcp://127.0.0.1:porta ou um handler", addr)
}

// hostname is the host of a Host header without its port.
func hostname(hostport string) string {
	if h, _, err := net.SplitHostPort(hostport); err == nil {
		return h
	}
	return strings.Trim(hostport, "[]")
}

func loopbackHost(h string) bool {
	if h == "localhost" {
		return true
	}
	ip := net.ParseIP(h)
	return ip != nil && ip.IsLoopback()
}

func writeErr(w http.ResponseWriter, status int, code, msg string) {
	b, _ := json.Marshal(proto.Error{Error: msg, Code: code})
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(status)
	_, _ = w.Write(append(b, '\n'))
}
