package main

import (
	"bytes"
	"context"
	"io"
	"net/http"
	"net/http/cookiejar"
	"os"
	"path/filepath"
	"regexp"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/aqilputi/nekuroddo/internal/config"
	"github.com/aqilputi/nekuroddo/internal/rpc"
	"github.com/aqilputi/nekuroddo/internal/rpc/proto"
)

// isolate keeps the tests off the user's config, state and runtime dirs.
func isolate(t *testing.T) {
	t.Helper()
	d := t.TempDir()
	for _, k := range []string{"HOME", "XDG_CONFIG_HOME", "XDG_STATE_HOME", "XDG_CACHE_HOME", "XDG_RUNTIME_DIR", "XDG_DATA_HOME"} {
		t.Setenv(k, filepath.Join(d, strings.ToLower(k)))
	}
	// never the user's real browser: app-mode tests set their own fake
	t.Setenv("NKG_BROWSER", filepath.Join(d, "no-browser"))
}

func distDir(t *testing.T) string {
	t.Helper()
	d := t.TempDir()
	if err := os.WriteFile(filepath.Join(d, "index.html"), []byte("<html><head></head><body>nk</body></html>"), 0o600); err != nil {
		t.Fatal(err)
	}
	return d
}

// syncBuf is an io.Writer the test can read while run writes.
type syncBuf struct {
	mu sync.Mutex
	b  bytes.Buffer
}

func (s *syncBuf) Write(p []byte) (int, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.b.Write(p)
}

func (s *syncBuf) String() string {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.b.String()
}

var launchRE = regexp.MustCompile(`abra \(o link vale uma vez\): (\S+)`)

// startNkg runs nkg with args until the test ends and returns the launch
// URL it prints.
func startNkg(t *testing.T, args ...string) string {
	t.Helper()
	ctx, cancel := context.WithCancel(context.Background())
	var out, errb syncBuf
	code := make(chan int, 1)
	go func() { code <- run(ctx, args, &out, &errb) }()
	t.Cleanup(func() {
		cancel()
		if c := <-code; c != 0 {
			t.Errorf("run = %d; stderr:\n%s", c, errb.String())
		}
	})
	deadline := time.Now().Add(10 * time.Second)
	for time.Now().Before(deadline) {
		if m := launchRE.FindStringSubmatch(out.String()); m != nil {
			return m[1]
		}
		select {
		case c := <-code:
			code <- c
			t.Fatalf("run ended with %d; stderr:\n%s", c, errb.String())
		case <-time.After(20 * time.Millisecond):
		}
	}
	t.Fatalf("no launch URL; stdout:\n%s\nstderr:\n%s", out.String(), errb.String())
	return ""
}

// page opens launch like a browser and GETs path with the cookie.
func page(t *testing.T, launch, path string) (int, string) {
	t.Helper()
	jar, _ := cookiejar.New(nil)
	c := &http.Client{Jar: jar, Timeout: 5 * time.Second}
	resp, err := c.Get(launch)
	if err != nil {
		t.Fatal(err)
	}
	resp.Body.Close()
	base := strings.SplitN(launch, "/?", 2)[0]
	resp, err = c.Get(base + path)
	if err != nil {
		t.Fatal(err)
	}
	defer resp.Body.Close()
	b, _ := io.ReadAll(resp.Body)
	return resp.StatusCode, string(b)
}

func TestBrowserDemo(t *testing.T) {
	isolate(t)
	launch := startNkg(t, "--browser", "--no-open", "--demo", "--dev", "--assets", distDir(t))
	if code, body := page(t, launch, "/v1/hello"); code != 200 || !strings.Contains(body, `"demo"`) {
		t.Fatalf("hello = %d %s", code, body)
	}
	if code, body := page(t, launch, "/"); code != http.StatusUnauthorized { // the key is spent: no cookie
		t.Fatalf("reused launch = %d %s", code, body)
	}
}

func TestBrowserIndexInjected(t *testing.T) {
	isolate(t)
	launch := startNkg(t, "--browser", "--no-open", "--demo", "--assets", distDir(t))
	code, body := page(t, launch, "/")
	if code != 200 || !strings.Contains(body, "window.nkEndpoint") {
		t.Fatalf("index = %d %s", code, body)
	}
}

func TestBrowserOpensDefault(t *testing.T) {
	isolate(t)
	opened := make(chan string, 1)
	old := openBrowser
	openBrowser = func(u string) error { opened <- u; return nil }
	t.Cleanup(func() { openBrowser = old })
	ctx, cancel := context.WithCancel(context.Background())
	code := make(chan int, 1)
	go func() {
		code <- run(ctx, []string{"--browser", "--demo", "--assets", distDir(t)}, io.Discard, io.Discard)
	}()
	select {
	case u := <-opened:
		if !strings.Contains(u, "/?nk=") {
			t.Errorf("opened %q", u)
		}
	case <-time.After(10 * time.Second):
		t.Fatal("browser not opened")
	}
	cancel()
	if c := <-code; c != 0 {
		t.Fatalf("run = %d", c)
	}
}

// --connect: a client of a running server over its unix socket.
func TestConnectUnix(t *testing.T) {
	isolate(t)
	h := newDemoHost(config.Default())
	t.Cleanup(h.Close)
	sock := filepath.Join(t.TempDir(), "nk.sock")
	l, err := rpc.Listen("unix://" + sock)
	if err != nil {
		t.Fatal(err)
	}
	srv := rpc.New(h.app, rpcOptions(h, ""))
	go func() { _ = srv.Serve(l) }()
	t.Cleanup(func() { srv.Close() })

	launch := startNkg(t, "--browser", "--no-open", "--connect", "unix://"+sock, "--assets", distDir(t))
	if code, body := page(t, launch, "/v1/hello"); code != 200 || !strings.Contains(body, `"demo"`) {
		t.Fatalf("hello via unix = %d %s", code, body)
	}
}

// --connect tcp:// reads the token file and sends it upstream.
func TestConnectTCP(t *testing.T) {
	isolate(t)
	h := newDemoHost(config.Default())
	t.Cleanup(h.Close)
	l, err := rpc.Listen("tcp://127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	srv := rpc.New(h.app, rpcOptions(h, "up-token"))
	go func() { _ = srv.Serve(l) }()
	t.Cleanup(func() { srv.Close() })
	tokFile := filepath.Join(t.TempDir(), "tok")
	if err := rpc.WriteTokenFile(tokFile, "up-token"); err != nil {
		t.Fatal(err)
	}
	launch := startNkg(t, "--browser", "--no-open", "--connect", "tcp://"+l.Addr().String(), "--token-file", tokFile, "--assets", distDir(t))
	if code, body := page(t, launch, "/v1/hello"); code != 200 {
		t.Fatalf("hello via tcp = %d %s", code, body)
	}
}

// --listen also serves the embedded core, on the same rpc server.
func TestListenAlso(t *testing.T) {
	isolate(t)
	sock := filepath.Join(t.TempDir(), "also.sock")
	startNkg(t, "--browser", "--no-open", "--demo", "--listen", "unix://"+sock, "--assets", distDir(t))
	c, err := rpc.Dial("unix://"+sock, "")
	if err != nil {
		t.Fatal(err)
	}
	defer c.Close()
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	if _, err := c.Hello(ctx); err != nil {
		t.Fatalf("hello on --listen: %v", err)
	}
}

func TestRunErrors(t *testing.T) {
	isolate(t)
	dist := distDir(t)
	missing := filepath.Join(t.TempDir(), "nope")
	for _, tc := range []struct {
		name string
		args []string
		code int
		want string
	}{
		{"app and browser", []string{"--app", "--browser"}, 2, "exclusivos"},
		{"no app browser", []string{"--app", "--demo", "--assets", dist}, 1, "abrir a janela"},
		{"help", []string{"-h"}, 2, "-connect"},
		{"bad flag", []string{"--nope"}, 2, "nope"},
		{"extra arg", []string{"--browser", "x"}, 2, "inesperado"},
		{"connect+demo", []string{"--browser", "--connect", "unix:///x", "--demo"}, 2, "não combina"},
		{"no-open alone", []string{"--no-open"}, 2, "--browser"},
		{"no assets", []string{"--browser", "--dev", "--assets", missing}, 1, "frontend não encontrado"},
		{"connect down", []string{"--browser", "--connect", "unix://" + missing, "--assets", dist}, 1, "não responde"},
		{"connect tcp no token", []string{"--browser", "--connect", "tcp://127.0.0.1:1", "--token-file", missing, "--assets", dist}, 1, "token"},
		{"connect bad scheme", []string{"--browser", "--connect", "ftp://x", "--assets", dist}, 1, "token"},
		{"bad addr", []string{"--browser", "--demo", "--addr", "8.8.8.8:0", "--assets", dist}, 1, "loopback"},
		{"bad listen", []string{"--browser", "--demo", "--listen", "tcp://8.8.8.8:0", "--assets", dist}, 1, "loopback"},
	} {
		t.Run(tc.name, func(t *testing.T) {
			var errb bytes.Buffer
			if c := run(context.Background(), tc.args, io.Discard, &errb); c != tc.code || !strings.Contains(errb.String(), tc.want) {
				t.Fatalf("run(%v) = %d, stderr %q; want %d with %q", tc.args, c, errb.String(), tc.code, tc.want)
			}
		})
	}
}

func TestUseSafeGfx(t *testing.T) {
	for _, tc := range []struct{ forced, envSet, failed, want bool }{
		{false, false, false, false},
		{true, false, false, true},
		{false, false, true, true},
		{true, true, true, false}, // the user's value wins
		{false, true, true, false},
	} {
		if got := useSafeGfx(tc.forced, tc.envSet, tc.failed); got != tc.want {
			t.Errorf("useSafeGfx(%v,%v,%v) = %v", tc.forced, tc.envSet, tc.failed, got)
		}
	}
}

func TestMarkRunning(t *testing.T) {
	p := filepath.Join(t.TempDir(), "state", "nkg-window.running")
	done := markRunning(p)
	if !fileExists(p) {
		t.Fatal("marker not written")
	}
	done()
	if fileExists(p) {
		t.Fatal("marker left after a clean close")
	}
}

func TestPickAssets(t *testing.T) {
	emb := os.DirFS(t.TempDir())
	if got, err := pickAssets(false, "unused", emb); err != nil || got != emb {
		t.Fatalf("embedded not chosen: %v", err)
	}
	dist := distDir(t)
	if _, err := pickAssets(true, dist, emb); err != nil {
		t.Fatalf("--dev: %v", err)
	}
	if _, err := pickAssets(true, filepath.Join(dist, "nope"), emb); err == nil {
		t.Fatal("missing dir accepted")
	}
}

var tokenRE = regexp.MustCompile(`token:"([^"]+)"`)

// The whole stack in one process: the page's token from the injected
// index, then the protocol client through guihost — snapshot, a pane
// opened on a demo session and its WebSocket.
func TestInProcessPaneThroughHost(t *testing.T) {
	isolate(t)
	launch := startNkg(t, "--browser", "--no-open", "--demo", "--assets", distDir(t))
	_, index := page(t, launch, "/")
	m := tokenRE.FindStringSubmatch(index)
	if m == nil {
		t.Fatalf("no token in index: %s", index)
	}
	base := strings.SplitN(launch, "/?", 2)[0]
	c, err := rpc.Dial(base, m[1])
	if err != nil {
		t.Fatal(err)
	}
	defer c.Close()
	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()
	snap, err := c.Snapshot(ctx)
	if err != nil || len(snap.Sessions) == 0 {
		t.Fatalf("snapshot: %v (%d sessions)", err, len(snap.Sessions))
	}
	opened, err := c.OpenPane(ctx, proto.PaneOpenRequest{SessionID: snap.Sessions[0].SessionID, Cols: 80, Rows: 24})
	if err != nil {
		t.Fatalf("open pane: %v", err)
	}
	pc, err := c.AttachPane(ctx, opened.PaneID)
	if err != nil {
		t.Fatalf("attach pane: %v", err)
	}
	defer pc.Close()
	if _, err := pc.Read(ctx); err != nil {
		t.Fatalf("first frame: %v", err)
	}
}
