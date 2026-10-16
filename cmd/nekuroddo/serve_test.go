package main

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"io"
	"os"
	"path/filepath"
	"slices"
	"strings"
	"sync"
	"testing"
	"time"

	tea "charm.land/bubbletea/v2"

	"github.com/aqilputi/nekuroddo/internal/app"
	"github.com/aqilputi/nekuroddo/internal/config"
	"github.com/aqilputi/nekuroddo/internal/rpc"
	"github.com/aqilputi/nekuroddo/internal/rpc/proto"
)

// syncBuffer is a bytes.Buffer safe to write from the server goroutine
// while the test reads it.
type syncBuffer struct {
	mu sync.Mutex
	b  bytes.Buffer
}

func (s *syncBuffer) Write(p []byte) (int, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.b.Write(p)
}

func (s *syncBuffer) String() string {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.b.String()
}

// fakeClaudeBin is a claude that lists no agents and records the argv of
// anything else (a dispatch) in the returned log.
func fakeClaudeBin(t *testing.T) string {
	t.Helper()
	dir := t.TempDir()
	log := filepath.Join(dir, "args")
	bin := filepath.Join(dir, "claude")
	// -p is the scheduler's /usage: answered, never logged as a dispatch
	script := "#!/bin/sh\ncase \"$1\" in\n  agents) echo '[]' ;;\n  -p) echo 'Current session: 5% used' ;;\n  *) echo \"$@\" > " + log + "; echo 'started abcd1234' ;;\nesac\n"
	if err := os.WriteFile(bin, []byte(script), 0o755); err != nil {
		t.Fatal(err)
	}
	t.Setenv("NEKO_CLAUDE", bin)
	return log
}

// runtimeDir is a short XDG_RUNTIME_DIR (a unix socket path is limited to
// 108 bytes, more than t.TempDir() may leave).
func runtimeDir(t *testing.T) string {
	t.Helper()
	dir, err := os.MkdirTemp("/tmp", "nk")
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { os.RemoveAll(dir) })
	t.Setenv("XDG_RUNTIME_DIR", dir)
	return dir
}

// startServe runs `nk serve args…` until the test ends and waits for the
// address it prints.
func startServe(t *testing.T, args ...string) string {
	t.Helper()
	ctx, cancel := context.WithCancel(context.Background())
	var out, errb syncBuffer
	done := make(chan int, 1)
	go func() {
		done <- runContext(ctx, append([]string{"serve"}, args...), strings.NewReader(""), &out, &errb)
	}()
	t.Cleanup(func() {
		cancel()
		select {
		case code := <-done:
			if code != 0 {
				t.Errorf("serve exited %d: %s", code, errb.String())
			}
		case <-time.After(5 * time.Second):
			t.Error("serve did not stop on cancel")
		}
	})
	deadline := time.Now().Add(5 * time.Second)
	for time.Now().Before(deadline) {
		if s := out.String(); strings.Contains(s, "servindo em ") {
			addr := strings.TrimSpace(s[strings.Index(s, "servindo em ")+len("servindo em "):])
			return strings.Fields(addr)[0]
		}
		select {
		case code := <-done:
			t.Fatalf("serve exited early with %d: %s", code, errb.String())
		case <-time.After(10 * time.Millisecond):
		}
	}
	t.Fatalf("serve never printed its address: out=%q err=%q", out.String(), errb.String())
	return ""
}

func hello(t *testing.T, addr, token string) {
	t.Helper()
	c, err := rpc.Dial(addr, token)
	if err != nil {
		t.Fatal(err)
	}
	defer c.Close()
	ctx, cancel := context.WithTimeout(context.Background(), 2*time.Second)
	defer cancel()
	h, err := c.Hello(ctx)
	if err != nil || h.App != "nekuroddo" {
		t.Fatalf("hello: %+v %v", h, err)
	}
}

func nk(args []string, stdin string) (code int, out, errs string) {
	var o, e bytes.Buffer
	code = run(args, strings.NewReader(stdin), &o, &e)
	return code, o.String(), e.String()
}

func TestServeUnixAnswersHello(t *testing.T) {
	isolate(t)
	fakeClaudeBin(t)
	dir := runtimeDir(t)
	sock := filepath.Join(dir, "s.sock")
	addr := startServe(t, "--listen", "unix://"+sock)
	if addr != "unix://"+sock {
		t.Fatalf("printed address %q", addr)
	}
	hello(t, addr, "")
	if fi, err := os.Stat(sock); err != nil || fi.Mode().Perm() != 0o600 {
		t.Fatalf("socket must be 0600: %v %v", fi, err)
	}
}

func TestServeDefaultSocket(t *testing.T) {
	isolate(t)
	fakeClaudeBin(t)
	runtimeDir(t)
	addr := startServe(t)
	if addr != "unix://"+rpc.DefaultSocketPath() {
		t.Fatalf("default address %q, want the default socket", addr)
	}
	hello(t, addr, "")
}

func TestServeTCPWritesToken(t *testing.T) {
	isolate(t)
	fakeClaudeBin(t)
	runtimeDir(t)
	addr := startServe(t, "--listen", "tcp://127.0.0.1:0")
	if !strings.HasPrefix(addr, "tcp://127.0.0.1:") || strings.HasSuffix(addr, ":0") {
		t.Fatalf("printed address %q: want the real port", addr)
	}
	fi, err := os.Stat(rpc.DefaultTokenPath())
	if err != nil || fi.Mode().Perm() != 0o600 {
		t.Fatalf("token file must be 0600: %v %v", fi, err)
	}
	token, err := rpc.ReadTokenFile(rpc.DefaultTokenPath())
	if err != nil || token == "" {
		t.Fatalf("token: %q %v", token, err)
	}
	hello(t, addr, token)
	// nk run finds the token by itself
	if code, out, errs := nk([]string{"run", "--addr", addr}, ""); code != 0 || !strings.Contains(out, "session.new") {
		t.Fatalf("run over tcp: %d %q %q", code, out, errs)
	}
}

// The host always runs the Lua runtime, so remote clients are told Lua
// commands may show up (hello's capabilities).
func TestServeAnnouncesLua(t *testing.T) {
	isolate(t)
	fakeClaudeBin(t)
	addr := startServe(t, "--listen", "unix://"+filepath.Join(runtimeDir(t), "s.sock"))
	c, err := rpc.Dial(addr, "")
	if err != nil {
		t.Fatal(err)
	}
	defer c.Close()
	ctx, cancel := context.WithTimeout(context.Background(), 2*time.Second)
	defer cancel()
	h, err := c.Hello(ctx)
	if err != nil {
		t.Fatal(err)
	}
	if !slices.Contains(h.Capabilities, "lua") || !slices.Contains(h.Capabilities, "panes") {
		t.Fatalf("capabilities = %v, want panes and lua", h.Capabilities)
	}
}

func TestServeBadFlags(t *testing.T) {
	isolate(t)
	runtimeDir(t)
	for _, args := range [][]string{{"serve", "--nope"}, {"serve", "extra"}} {
		if code, _, errs := nk(args, ""); code != 2 || errs == "" {
			t.Fatalf("%v: want usage error 2, got %d %q", args, code, errs)
		}
	}
	if code, _, errs := nk([]string{"serve", "--listen", "ftp://x"}, ""); code != 1 || !strings.Contains(errs, "ftp") {
		t.Fatalf("bad address: want 1 and the reason, got %d %q", code, errs)
	}
}

func TestServeTokenFileError(t *testing.T) {
	isolate(t)
	fakeClaudeBin(t)
	dir := runtimeDir(t)
	// the token's place is taken by a directory: writing it must fail
	if err := os.MkdirAll(filepath.Join(rpc.DefaultTokenPath(), "x"), 0o700); err != nil {
		t.Fatal(err)
	}
	_ = dir
	if code, _, errs := nk([]string{"serve", "--listen", "tcp://127.0.0.1:0"}, ""); code != 1 || errs == "" {
		t.Fatalf("token write failure: want 1, got %d %q", code, errs)
	}
}

func TestRunListsAndExecutes(t *testing.T) {
	isolate(t)
	log := fakeClaudeBin(t)
	runtimeDir(t)
	startServe(t)

	code, out, errs := nk([]string{"run"}, "")
	if code != 0 || !strings.Contains(out, "session.new") || !strings.Contains(out, "sessions.refresh") {
		t.Fatalf("nk run lists the commands: %d %q %q", code, out, errs)
	}

	code, out, errs = nk([]string{"run", "sessions.refresh"}, "")
	if code != 0 {
		t.Fatalf("sessions.refresh: %d %q %q", code, out, errs)
	}

	// bypass asks first: N cancels and nothing runs
	dir := t.TempDir()
	args := []string{"run", "session.new", "dir=" + dir, "prompt=oi", "perm=bypassPermissions", "name=x"}
	code, _, errs = nk(args, "n\n")
	if code != 1 || !strings.Contains(errs, "BYPASS") || !strings.Contains(errs, "cancelado") {
		t.Fatalf("declined: want 1, the question and cancelado, got %d %q", code, errs)
	}
	if _, err := os.Stat(log); err == nil {
		t.Fatal("a declined dispatch must not run claude")
	}
	// y confirms: claude runs and the toast comes back
	code, out, errs = nk(args, "y\n")
	if code != 0 || !strings.Contains(out, "abcd1234") {
		t.Fatalf("confirmed: %d %q %q", code, out, errs)
	}
	if b, _ := os.ReadFile(log); !strings.Contains(string(b), "--bg") || !strings.Contains(string(b), "bypassPermissions") {
		t.Fatalf("claude argv: %q", b)
	}
	// --yes skips the question
	os.Remove(log)
	code, out, _ = nk(append(args, "--yes"), "")
	if code != 0 || !strings.Contains(out, "abcd1234") {
		t.Fatalf("--yes: %d %q", code, out)
	}
	// the data comes out as JSON
	if !strings.Contains(out, `"id": "abcd1234"`) && !strings.Contains(out, `"ID": "abcd1234"`) {
		t.Fatalf("data as JSON: %q", out)
	}
}

func TestRunErrors(t *testing.T) {
	isolate(t)
	fakeClaudeBin(t)
	runtimeDir(t)

	// no server
	if code, _, errs := nk([]string{"run", "sessions.refresh"}, ""); code != 1 || !strings.Contains(errs, "nk serve") {
		t.Fatalf("no server: want 1 and a hint, got %d %q", code, errs)
	}
	if code, _, errs := nk([]string{"run"}, ""); code != 1 || errs == "" {
		t.Fatalf("no server (list): want 1, got %d %q", code, errs)
	}

	startServe(t)
	cases := []struct {
		args []string
		code int
		want string
	}{
		{[]string{"run", "nope.nope"}, 1, "nope.nope"},
		{[]string{"run", "session.new"}, 1, "dir"},                  // missing argument
		{[]string{"run", "pane.close"}, 1, "cliente"},               // a client command
		{[]string{"run", "session.stop", "--session", "zz"}, 1, ""}, // no such session
		{[]string{"run", "sessions.refresh", "oops"}, 2, "oops"},    // not k=v
		{[]string{"run", "--nope"}, 2, "nope"},
		{[]string{"run", "--addr", "ftp://x", "sessions.refresh"}, 1, "ftp"},
	}
	for _, c := range cases {
		code, _, errs := nk(c.args, "")
		if code != c.code || !strings.Contains(errs, c.want) {
			t.Errorf("%v: want %d with %q, got %d %q", c.args, c.code, c.want, code, errs)
		}
	}
}

func TestRunConfirmEOFDeclines(t *testing.T) {
	isolate(t)
	log := fakeClaudeBin(t)
	runtimeDir(t)
	startServe(t)
	code, _, errs := nk([]string{"run", "session.new", "dir=" + t.TempDir(), "prompt=oi", "perm=bypassPermissions"}, "")
	if code != 1 || !strings.Contains(errs, "cancelado") {
		t.Fatalf("EOF on the question declines: %d %q", code, errs)
	}
	if _, err := os.Stat(log); err == nil {
		t.Fatal("nothing should run")
	}
}

func TestRunTCPWithoutToken(t *testing.T) {
	isolate(t)
	runtimeDir(t)
	// no token file next to the socket: say so
	if code, _, errs := nk([]string{"run", "--addr", "tcp://127.0.0.1:1"}, ""); code != 1 || !strings.Contains(errs, "token") {
		t.Fatalf("want 1 and a token error, got %d %q", code, errs)
	}
}

func TestPrintResult(t *testing.T) {
	var out, errb bytes.Buffer
	if code := printResult(&out, &errb, rpcResult(`{"toast":"falhou","err":true}`)); code != 1 || !strings.Contains(errb.String(), "falhou") {
		t.Fatalf("error toast: %d %q", code, errb.String())
	}
	out.Reset()
	if code := printResult(&out, &errb, rpcResult(`{"exec":{"argv":["claude","attach","x"],"dir":"/tmp"}}`)); code != 0 || !strings.Contains(out.String(), "claude attach x") {
		t.Fatalf("exec: %d %q", code, out.String())
	}
	out.Reset()
	if code := printResult(&out, &errb, rpcResult(`{"data":null}`)); code != 0 || out.Len() != 0 {
		t.Fatalf("null data prints nothing: %d %q", code, out.String())
	}
	out.Reset()
	if code := printResult(&out, &errb, proto.Result{Data: json.RawMessage("{bad")}); code != 0 || strings.TrimSpace(out.String()) != "{bad" {
		t.Fatalf("data that is not JSON goes out as is: %d %q", code, out.String())
	}
	out.Reset()
	if code := printResult(&out, &errb, rpcResult(`{"data":"x"}`)); code != 0 || strings.TrimSpace(out.String()) != `"x"` {
		t.Fatalf("data: %d %q", code, out.String())
	}
}

// The hooks run on every Claude Code event: they must stay a few stats and a
// file append, never building the core (wire, store, Lua).
func TestHooksDoNotBuildTheApp(t *testing.T) {
	isolate(t)
	before := hostsBuilt.Load()
	start := time.Now()
	for _, sub := range []string{"hook", "hook-context"} {
		code, out, errs := nk([]string{sub}, `{"hook_event_name":"Stop","session_id":"s1"}`)
		if code != 0 || errs != "" || (sub == "hook" && out != "") {
			t.Fatalf("%s: %d %q %q", sub, code, out, errs)
		}
	}
	if hostsBuilt.Load() != before {
		t.Fatal("a hook built the app")
	}
	if d := time.Since(start); d > time.Second {
		t.Fatalf("hooks took %v", d)
	}
}

func TestListenRunsTUIAndServer(t *testing.T) {
	isolate(t)
	fakeClaudeBin(t)
	dir := runtimeDir(t)
	sock := "unix://" + filepath.Join(dir, "l.sock")
	var ran bool
	defer swapTUI(func(m tea.Model) (tea.Model, error) {
		ran = true
		hello(t, sock, "") // the server answers while the TUI runs
		return m, nil
	})()
	before := hostsBuilt.Load()
	code, _, errs := nk([]string{"--listen", sock}, "")
	if code != 0 || !ran {
		t.Fatalf("nk --listen: %d ran=%v %q", code, ran, errs)
	}
	if hostsBuilt.Load() != before+1 {
		t.Fatal("the TUI and the server must share one app")
	}
	if _, err := os.Stat(filepath.Join(dir, "l.sock")); !os.IsNotExist(err) {
		t.Fatal("the socket should be gone after the TUI quits")
	}
}

func TestTUIWithoutListen(t *testing.T) {
	isolate(t)
	fakeClaudeBin(t)
	runtimeDir(t)
	var got tea.Model
	defer swapTUI(func(m tea.Model) (tea.Model, error) { got = m; return m, nil })()
	if code, _, errs := nk(nil, ""); code != 0 || got == nil {
		t.Fatalf("plain nk: %d %q", code, errs)
	}
	if _, err := os.Stat(rpc.DefaultSocketPath()); err == nil {
		t.Fatal("plain nk must not open a socket")
	}
	// a failing TUI exits 1
	defer swapTUI(func(m tea.Model) (tea.Model, error) { return m, errors.New("sem tty") })()
	if code, _, errs := nk(nil, ""); code != 1 || !strings.Contains(errs, "sem tty") {
		t.Fatalf("TUI error: %d %q", code, errs)
	}
}

func TestTUIFlags(t *testing.T) {
	isolate(t)
	runtimeDir(t)
	defer swapTUI(func(m tea.Model) (tea.Model, error) { t.Fatal("must not start"); return m, nil })()
	if code, _, errs := nk([]string{"--nope"}, ""); code != 2 || errs == "" {
		t.Fatalf("bad flag: %d %q", code, errs)
	}
	if code, _, errs := nk([]string{"--listen", "unix:///tmp/x", "extra"}, ""); code != 2 || !strings.Contains(errs, "extra") {
		t.Fatalf("stray argument: %d %q", code, errs)
	}
	if code, _, errs := nk([]string{"--listen", "ftp://x"}, ""); code != 1 || !strings.Contains(errs, "ftp") {
		t.Fatalf("bad address: %d %q", code, errs)
	}
}

func TestHelp(t *testing.T) {
	isolate(t)
	for _, a := range []string{"help", "-h", "--help"} {
		code, out, _ := nk([]string{a}, "")
		if code != 0 {
			t.Fatalf("%s: %d", a, code)
		}
		for _, want := range []string{"nk serve", "nk run", "nk hook", "nk hook-context", "--listen"} {
			if !strings.Contains(out, want) {
				t.Fatalf("%s: usage lacks %q:\n%s", a, want, out)
			}
		}
	}
}

func TestConfigErrorIsAWarning(t *testing.T) {
	isolate(t)
	fakeClaudeBin(t)
	runtimeDir(t)
	if err := os.MkdirAll(filepath.Dir(config.Path()), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(config.Path(), []byte("refresh = ["), 0o644); err != nil {
		t.Fatal(err)
	}
	defer swapTUI(func(m tea.Model) (tea.Model, error) { return m, nil })()
	if code, _, errs := nk(nil, ""); code != 0 || !strings.Contains(errs, "usando padrões") {
		t.Fatalf("broken config: %d %q", code, errs)
	}
}

func TestLuaLoadErrorIsAWarning(t *testing.T) {
	isolate(t)
	fakeClaudeBin(t)
	runtimeDir(t)
	if err := os.MkdirAll(filepath.Dir(config.Path()), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(filepath.Dir(config.Path()), "init.lua"), []byte("this is not lua"), 0o644); err != nil {
		t.Fatal(err)
	}
	var h *host
	defer swapTUI(func(m tea.Model) (tea.Model, error) { return m, nil })()
	if code, _, errs := nk(nil, ""); code != 0 || !strings.Contains(errs, "init.lua") {
		t.Fatalf("broken init.lua: %d %q", code, errs)
	}
	// lua.reload is registered and reports the same error
	h = newHost(config.Default(), io.Discard)
	defer h.Close()
	res, err := h.app.Run(app.Context{}, "lua.reload", nil)
	if err == nil || !res.Err {
		t.Fatalf("lua.reload with a broken file: %+v %v", res, err)
	}
	if err := os.WriteFile(filepath.Join(filepath.Dir(config.Path()), "init.lua"), []byte("x = 1"), 0o644); err != nil {
		t.Fatal(err)
	}
	if res, err := h.app.Run(app.Context{}, "lua.reload", nil); err != nil || res.Toast == "" {
		t.Fatalf("lua.reload: %+v %v", res, err)
	}
}

func waitFor(t *testing.T, ok func() bool) {
	t.Helper()
	deadline := time.Now().Add(3 * time.Second)
	for !ok() {
		if time.Now().After(deadline) {
			t.Fatal("timed out")
		}
		time.Sleep(2 * time.Millisecond)
	}
}

func swapTUI(f func(tea.Model) (tea.Model, error)) func() {
	old := runTUI
	runTUI = f
	return func() { runTUI = old }
}

func rpcResult(s string) proto.Result {
	var r proto.Result
	if err := json.Unmarshal([]byte(s), &r); err != nil {
		panic(err)
	}
	return r
}
