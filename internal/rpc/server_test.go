package rpc

import (
	"encoding/json"
	"errors"
	"io"
	"net"
	"net/http"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/aqilputi/nekuroddo/internal/rpc/proto"
)

func TestHello(t *testing.T) {
	e := newEnv(t)
	h, err := e.c.Hello(ctxT(t))
	if err != nil {
		t.Fatal(err)
	}
	if h.App != "nekuroddo" || h.Protocol != 1 || h.Version != "test" || h.PID != os.Getpid() {
		t.Fatalf("hello = %+v", h)
	}
	if len(h.Capabilities) != 1 || h.Capabilities[0] != proto.CapPanes {
		t.Fatalf("capabilities = %v", h.Capabilities)
	}
}

func TestHelloExtraCapabilities(t *testing.T) {
	e := newEnv(t, func(o *Options, _ *fakesT) { o.Capabilities = []string{proto.CapDemo} })
	h, err := e.c.Hello(ctxT(t))
	if err != nil {
		t.Fatal(err)
	}
	if strings.Join(h.Capabilities, ",") != "panes,demo" {
		t.Fatalf("capabilities = %v", h.Capabilities)
	}
}

func TestUnixSocketIs0600(t *testing.T) {
	e := newEnv(t)
	fi, err := os.Stat(e.sock)
	if err != nil {
		t.Fatal(err)
	}
	if fi.Mode().Perm() != 0o600 || fi.Mode()&os.ModeSocket == 0 {
		t.Fatalf("mode = %v", fi.Mode())
	}
}

func TestListenUnixReplacesStaleSocket(t *testing.T) {
	dir := shortDir(t)
	sock := filepath.Join(dir, "s")
	// a socket file nobody listens on
	l, err := net.Listen("unix", sock)
	if err != nil {
		t.Fatal(err)
	}
	l.(*net.UnixListener).SetUnlinkOnClose(false)
	l.Close()
	if _, err := os.Stat(sock); err != nil {
		t.Fatalf("stale socket missing: %v", err)
	}
	l2, err := ListenUnix(sock)
	if err != nil {
		t.Fatalf("stale socket not replaced: %v", err)
	}
	l2.Close()
}

func TestListenUnixRefusesLiveSocket(t *testing.T) {
	e := newEnv(t)
	if _, err := ListenUnix(e.sock); !errors.Is(err, ErrInUse) {
		t.Fatalf("err = %v, want ErrInUse", err)
	}
}

func TestListenUnixRefusesRegularFile(t *testing.T) {
	dir := shortDir(t)
	p := filepath.Join(dir, "f")
	if err := os.WriteFile(p, []byte("x"), 0o600); err != nil {
		t.Fatal(err)
	}
	if _, err := ListenUnix(p); err == nil {
		t.Fatal("listened over a regular file")
	}
}

func TestListenSpecs(t *testing.T) {
	dir := shortDir(t)
	l, err := Listen("unix://" + filepath.Join(dir, "x"))
	if err != nil {
		t.Fatal(err)
	}
	l.Close()
	for _, bad := range []string{"tcp://0.0.0.0:0", "tcp://example.com:80", "ftp://x", "nope", "tcp://[::1"} {
		if l, err := Listen(bad); err == nil {
			l.Close()
			t.Errorf("Listen(%q) worked", bad)
		}
	}
	l, err = Listen("tcp://localhost:0")
	if err != nil {
		t.Fatal(err)
	}
	l.Close()
}

func TestTCPNeedsToken(t *testing.T) {
	e := newEnv(t, func(o *Options, _ *fakesT) { o.Token = "s3cr3t" })
	base := e.tcp()

	resp, err := http.Get(base + "/v1/hello")
	if err != nil {
		t.Fatal(err)
	}
	var body proto.Error
	_ = json.NewDecoder(resp.Body).Decode(&body)
	resp.Body.Close()
	if resp.StatusCode != http.StatusUnauthorized || body.Code != proto.CodeUnauthorized {
		t.Fatalf("no token: %d %+v", resp.StatusCode, body)
	}

	req, _ := http.NewRequest(http.MethodGet, base+"/v1/hello", nil)
	req.Header.Set("Authorization", "Bearer wrong")
	resp, err = http.DefaultClient.Do(req)
	if err != nil {
		t.Fatal(err)
	}
	resp.Body.Close()
	if resp.StatusCode != http.StatusUnauthorized {
		t.Fatalf("wrong token: %d", resp.StatusCode)
	}

	resp, err = http.Get(base + "/v1/hello?token=s3cr3t")
	if err != nil {
		t.Fatal(err)
	}
	resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		t.Fatalf("query token: %d", resp.StatusCode)
	}

	c, err := Dial(base, "s3cr3t")
	if err != nil {
		t.Fatal(err)
	}
	defer c.Close()
	if _, err := c.Hello(ctxT(t)); err != nil {
		t.Fatalf("bearer token: %v", err)
	}
	bad, _ := Dial(strings.Replace(base, "http://", "tcp://", 1), "nope")
	defer bad.Close()
	_, err = bad.Hello(ctxT(t))
	var api *APIError
	if !errors.As(err, &api) || api.Status != http.StatusUnauthorized || !IsCode(err, proto.CodeUnauthorized) {
		t.Fatalf("bad client err = %v", err)
	}
}

func TestTCPWithoutTokenRefused(t *testing.T) {
	e := newEnv(t)
	l, err := Listen("tcp://127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	defer l.Close()
	if err := e.srv.Serve(l); !errors.Is(err, ErrNoToken) {
		t.Fatalf("Serve = %v, want ErrNoToken", err)
	}
}

func TestServeAfterClose(t *testing.T) {
	e := newEnv(t)
	_ = e.srv.Close()
	_ = e.srv.Close() // twice is fine
	dir := shortDir(t)
	l, err := ListenUnix(filepath.Join(dir, "s"))
	if err != nil {
		t.Fatal(err)
	}
	if err := e.srv.Serve(l); !errors.Is(err, http.ErrServerClosed) {
		t.Fatalf("Serve after Close = %v", err)
	}
}

func TestPhaseTwoEndpointsAre501(t *testing.T) {
	e := newEnv(t)
	hc := e.rawUnix()
	for _, r := range []struct{ method, path string }{
		{http.MethodPost, "/v1/view/tree"}, {http.MethodGet, "/v1/theme"}, {http.MethodPost, "/v1/lua/eval"},
	} {
		req, _ := http.NewRequest(r.method, "http://nk"+r.path, strings.NewReader("{}"))
		resp, err := hc.Do(req)
		if err != nil {
			t.Fatal(err)
		}
		var body proto.Error
		_ = json.NewDecoder(resp.Body).Decode(&body)
		resp.Body.Close()
		if resp.StatusCode != http.StatusNotImplemented || body.Code != proto.CodeNotImplemented {
			t.Errorf("%s %s = %d %+v", r.method, r.path, resp.StatusCode, body)
		}
	}
}

func TestUnknownPathAndMethod(t *testing.T) {
	e := newEnv(t)
	hc := e.rawUnix()
	resp, err := hc.Get("http://nk/v2/hello")
	if err != nil {
		t.Fatal(err)
	}
	resp.Body.Close()
	if resp.StatusCode != http.StatusNotFound {
		t.Fatalf("unknown path = %d", resp.StatusCode)
	}
	resp, err = hc.Post("http://nk/v1/hello", "application/json", nil)
	if err != nil {
		t.Fatal(err)
	}
	resp.Body.Close()
	if resp.StatusCode != http.StatusMethodNotAllowed {
		t.Fatalf("wrong method = %d", resp.StatusCode)
	}
}

func TestBadJSONIs400(t *testing.T) {
	e := newEnv(t)
	hc := e.rawUnix()
	for _, p := range []string{"/v1/commands/session.rename", "/v1/confirm/x", "/v1/panes", "/v1/panes/bg01/resize"} {
		resp, err := hc.Post("http://nk"+p, "application/json", strings.NewReader("{nope"))
		if err != nil {
			t.Fatal(err)
		}
		b, _ := io.ReadAll(resp.Body)
		resp.Body.Close()
		if resp.StatusCode != http.StatusBadRequest {
			t.Errorf("%s: %d %s", p, resp.StatusCode, b)
		}
	}
}

func TestTokenFile(t *testing.T) {
	dir := shortDir(t)
	tok := NewToken()
	if len(tok) < 26 || tok == NewToken() {
		t.Fatalf("token %q", tok)
	}
	p := filepath.Join(dir, "sub", "tok")
	if err := WriteTokenFile(p, tok); err != nil {
		t.Fatal(err)
	}
	fi, err := os.Stat(p)
	if err != nil || fi.Mode().Perm() != 0o600 {
		t.Fatalf("token file: %v %v", fi.Mode(), err)
	}
	got, err := ReadTokenFile(p)
	if err != nil || got != tok {
		t.Fatalf("read back %q, %v", got, err)
	}
	if _, err := ReadTokenFile(filepath.Join(dir, "none")); err == nil {
		t.Fatal("read a missing token file")
	}
}

func TestDefaultPaths(t *testing.T) {
	t.Setenv("XDG_RUNTIME_DIR", "/run/user/test")
	uid := os.Getuid()
	if got := DefaultSocketPath(); got != filepath.Join("/run/user/test", "nekuroddo-"+itoa(uid)+".sock") {
		t.Fatalf("socket = %s", got)
	}
	if got := DefaultTokenPath(); got != filepath.Join("/run/user/test", "nekuroddo-"+itoa(uid)+".token") {
		t.Fatalf("token = %s", got)
	}
	t.Setenv("XDG_RUNTIME_DIR", "")
	if got := DefaultSocketPath(); !strings.HasPrefix(got, os.TempDir()) {
		t.Fatalf("fallback socket = %s", got)
	}
}

func TestDialSpecs(t *testing.T) {
	for _, bad := range []string{"", "ftp://x", "unix://"} {
		if _, err := Dial(bad, ""); err == nil {
			t.Errorf("Dial(%q) worked", bad)
		}
	}
	c, err := Dial("http://127.0.0.1:1/", "")
	if err != nil {
		t.Fatal(err)
	}
	c.Close()
	if _, err := c.Hello(ctxT(t)); err == nil {
		t.Fatal("hello to a closed port worked")
	}
}
