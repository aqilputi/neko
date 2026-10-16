package rpc

import (
	"context"
	"net"
	"net/http"
	"os"
	"path/filepath"
	"strconv"
	"testing"
	"time"

	"github.com/aqilputi/nekuroddo/internal/app"
	"github.com/aqilputi/nekuroddo/internal/app/apptest"
	"github.com/aqilputi/nekuroddo/internal/config"
	"github.com/aqilputi/nekuroddo/internal/store"
)

// Sessions of the fake snapshot every test starts from.
var (
	bgSession = store.Session{SessionID: "11111111-aaaa", ShortID: "bg01", Name: "fila", Cwd: "/r/a",
		Kind: "background", State: "blocked", Attention: store.AttnBlocked}
	idleSession = store.Session{SessionID: "22222222-bbbb", ShortID: "bg02", Name: "docs", Cwd: "/r/b",
		Kind: "background", State: "idle"}
	histSession = store.Session{SessionID: "33333333-cccc", Name: "velha", Cwd: "/r/c",
		Kind: "history", State: "history", Attention: store.AttnReturned}
)

// env is a real server on a short unix socket over an app built on fakes.
type env struct {
	t    *testing.T
	f    *apptest.Fakes
	a    *app.App
	srv  *Server
	c    *Client
	sock string
}

type fakesT = apptest.Fakes

type envOpt func(*Options, *fakesT)

func itoa(i int) string { return strconv.Itoa(i) }

type configT = config.Config

func newEnv(t *testing.T, opts ...envOpt) *env {
	t.Helper()
	return newEnvCfg(t, nil, opts...)
}

// newEnvCfg is newEnv with the config changed by cfgFn first.
func newEnvCfg(t *testing.T, cfgFn func(*configT), opts ...envOpt) *env {
	t.Helper()
	cfg := config.Default()
	if cfgFn != nil {
		cfgFn(&cfg)
	}
	t.Setenv("XDG_CACHE_HOME", shortDir(t)) // the Jira cache is read from disk
	f := apptest.New()
	f.Sessions.SetSnapshot(store.Snapshot{At: apptest.Epoch,
		Sessions: []store.Session{bgSession, idleSession, histSession}})
	o := Options{Version: "test", Heartbeat: time.Hour}
	for _, fn := range opts {
		fn(&o, f)
	}
	a := app.New(cfg, f.Ports())
	if err := a.RegisterBuiltins(); err != nil {
		t.Fatal(err)
	}
	a.Refresh()
	srv := New(a, o)
	dir := shortDir(t)
	sock := filepath.Join(dir, "s")
	l, err := ListenUnix(sock)
	if err != nil {
		t.Fatal(err)
	}
	go func() { _ = srv.Serve(l) }()
	c, err := Dial("unix://"+sock, "")
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() {
		c.Close()
		_ = srv.Close()
		a.Close()
	})
	return &env{t: t, f: f, a: a, srv: srv, c: c, sock: sock}
}

// shortDir is a temp dir short enough for a unix socket path (108 bytes).
func shortDir(t *testing.T) string {
	t.Helper()
	dir, err := os.MkdirTemp("/tmp", "nk")
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { os.RemoveAll(dir) })
	return dir
}

// tcp serves the env's server on a loopback TCP port too; it returns the
// base URL.
func (e *env) tcp() string {
	e.t.Helper()
	l, err := Listen("tcp://127.0.0.1:0")
	if err != nil {
		e.t.Fatal(err)
	}
	go func() { _ = e.srv.Serve(l) }()
	return "http://" + l.Addr().String()
}

// rawUnix is an http.Client over the env's socket, for requests the typed
// client does not make.
func (e *env) rawUnix() *http.Client {
	return &http.Client{Transport: &http.Transport{
		DialContext: func(ctx context.Context, _, _ string) (net.Conn, error) {
			var d net.Dialer
			return d.DialContext(ctx, "unix", e.sock)
		},
	}}
}

func ctxT(t *testing.T) context.Context {
	ctx, cancel := context.WithTimeout(t.Context(), 10*time.Second)
	t.Cleanup(cancel)
	return ctx
}

// eventually polls cond for up to 5s.
func eventually(t *testing.T, what string, cond func() bool) {
	t.Helper()
	deadline := time.Now().Add(5 * time.Second)
	for !cond() {
		if time.Now().After(deadline) {
			t.Fatalf("timed out waiting for %s", what)
		}
		time.Sleep(5 * time.Millisecond)
	}
}
