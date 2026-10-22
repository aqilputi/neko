package rpc

import (
	"errors"
	"fmt"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"

	"github.com/aqilputi/nekuroddo/internal/app"
	"github.com/aqilputi/nekuroddo/internal/app/apptest"
	"github.com/aqilputi/nekuroddo/internal/config"
	"github.com/aqilputi/nekuroddo/internal/core"
	"github.com/aqilputi/nekuroddo/internal/lua"
	"github.com/aqilputi/nekuroddo/internal/rpc/proto"
)

// fakeLuaKeys is a runtime with key overrides: it answers Keymap from a
// table and RunBinding from a map, recording the calls.
type fakeLuaKeys struct {
	fakeLua
	km   core.Keymap
	ran  []string
	ctxs []app.Context
}

func (f *fakeLuaKeys) Keymap() core.Keymap { return f.km }

func (f *fakeLuaKeys) RunBinding(ctx app.Context, name string) (app.Result, error) {
	f.mu.Lock()
	defer f.mu.Unlock()
	f.ran = append(f.ran, name)
	f.ctxs = append(f.ctxs, ctx)
	switch name {
	case "lua.key:normal.world:n":
		return app.Result{Toast: "olá"}, nil
	case "lua.key:normal:boom":
		return app.Result{Toast: "boom", Err: true}, errors.New("init.lua:3: boom")
	}
	return app.Result{}, fmt.Errorf("%w: %s", app.ErrUnknownCommand, name)
}

func TestKeymapOverlaysLuaOverrides(t *testing.T) {
	rt := &fakeLuaKeys{km: core.Keymap{
		{Command: "lua.key:normal.world:n", Keys: []string{"n"}, Mode: core.ModeNormalWorld},
		{Command: "nav.top", Keys: []string{"j"}, Mode: core.ModeNormal},
		{Keys: []string{"X"}, Mode: core.ModeNormal},
	}}
	e := newEnv(t, func(o *Options, _ *fakesT) { o.Lua = rt })
	got, err := e.c.Keymap(ctxT(t))
	if err != nil {
		t.Fatal(err)
	}
	want := e.a.Keymap().Overlay(rt.km)
	if len(got) != len(want) {
		t.Fatalf("%d bindings, want %d", len(got), len(want))
	}
	for _, c := range []struct{ mode, key, want string }{
		{core.ModeNormalWorld, "n", "lua.key:normal.world:n"},
		{core.ModeNormal, "j", "nav.top"},
		{core.ModeNormal, "X", ""},
	} {
		if cmd, _ := got.Resolve(c.mode, c.key); cmd != c.want {
			t.Errorf("%s %q = %q, want %q", c.mode, c.key, cmd, c.want)
		}
	}
	if b, ok := got.Find("lua.key:normal.world:n"); !ok || b.Help.Desc != "lua" {
		t.Fatalf("lua key = %+v", b)
	}
	// a runtime without overrides (only Eval) serves the registry's keymap
	e2 := newEnv(t, func(o *Options, _ *fakesT) { o.Lua = &fakeLua{} })
	plain, err := e2.c.Keymap(ctxT(t))
	if err != nil || len(plain) != len(e2.a.Keymap()) {
		t.Fatalf("plain keymap = %d bindings, %v", len(plain), err)
	}
}

func TestRunLuaKey(t *testing.T) {
	rt := &fakeLuaKeys{}
	e := newEnv(t, func(o *Options, _ *fakesT) { o.Lua = rt })
	res, err := e.c.Run(ctxT(t), "lua.key:normal.world:n", nil, proto.Context{SessionID: idleSession.SessionID})
	if err != nil || res.Toast != "olá" || res.Err {
		t.Fatalf("run = %+v, %v", res, err)
	}
	rt.mu.Lock()
	if len(rt.ran) != 1 || rt.ctxs[0].SessionID != idleSession.SessionID {
		t.Fatalf("ran %v with %+v", rt.ran, rt.ctxs)
	}
	rt.mu.Unlock()
	// a key bound to no function (or to nothing): unknown_command
	if _, err := e.c.Run(ctxT(t), "lua.key:normal:q", nil, proto.Context{}); !IsCode(err, proto.CodeUnknownCommand) {
		t.Fatalf("unbound key = %v", err)
	}
	// the function failed: failed, with its message
	if _, err := e.c.Run(ctxT(t), "lua.key:normal:boom", nil, proto.Context{}); !IsCode(err, proto.CodeFailed) || !strings.Contains(err.Error(), "boom") {
		t.Fatalf("failing key = %v", err)
	}
	// the names are not commands: GET /v1/commands never lists them
	cmds, err := e.c.Commands(ctxT(t), proto.Context{})
	if err != nil {
		t.Fatal(err)
	}
	for _, c := range cmds {
		if strings.HasPrefix(c.Name, core.LuaKeyPrefix) {
			t.Fatalf("listed %s", c.Name)
		}
	}
	// a bad body is refused before anything runs
	resp, err := e.rawUnix().Post("http://nk/v1/commands/lua.key:normal.world:n", "application/json", strings.NewReader("{"))
	if err != nil {
		t.Fatal(err)
	}
	resp.Body.Close()
	if resp.StatusCode != http.StatusBadRequest {
		t.Fatalf("bad json = %d", resp.StatusCode)
	}
}

// Without a Lua runtime, or with one that only evaluates, a lua.key name
// is an unknown command.
func TestRunLuaKeyWithoutRuntime(t *testing.T) {
	for name, opt := range map[string]envOpt{
		"none":      func(*Options, *fakesT) {},
		"eval only": func(o *Options, _ *fakesT) { o.Lua = &fakeLua{} },
	} {
		e := newEnv(t, opt)
		_, err := e.c.Run(ctxT(t), "lua.key:normal.world:n", nil, proto.Context{})
		if !IsCode(err, proto.CodeUnknownCommand) {
			t.Errorf("%s: %v", name, err)
		}
	}
}

// The whole path with the real runtime: nk.keymap.set with a function
// publishes keymap.changed, GET /v1/keymap carries the row and POST
// /v1/commands/lua.key:… runs it (its toast lands on the bus).
func TestLuaKeyEndToEnd(t *testing.T) {
	e := newEnv(t)
	rt, err := lua.New(e.a, lua.Options{ConfigDir: t.TempDir(), Caps: []string{lua.CapToast},
		CallbackTimeout: time.Second})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(rt.Close)
	srv := New(e.a, Options{Version: "test", Heartbeat: time.Hour, Lua: rt})
	t.Cleanup(func() { _ = srv.Close() })
	sock := shortDir(t) + "/s"
	l, err := ListenUnix(sock)
	if err != nil {
		t.Fatal(err)
	}
	go func() { _ = srv.Serve(l) }()
	c, err := Dial("unix://"+sock, "")
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(c.Close)

	ev := e.a.Subscribe(t.Context())
	if _, err := rt.Eval(`nk.keymap.set("normal.world", "n", function() nk.ui.toast("olá") end)`); err != nil {
		t.Fatal(err)
	}
	waitEvent(t, ev, lua.EvKeymapChanged, nil)
	km, err := c.Keymap(ctxT(t))
	if err != nil {
		t.Fatal(err)
	}
	b, ok := km.Find("lua.key:normal.world:n")
	if !ok || b.Mode != core.ModeNormalWorld || len(b.Keys) != 1 || b.Keys[0] != "n" || b.Help.Desc != "lua" {
		t.Fatalf("served row = %+v %v", b, ok)
	}
	res, err := c.Run(ctxT(t), "lua.key:normal.world:n", nil, proto.Context{})
	if err != nil || res.Err {
		t.Fatalf("run = %+v, %v", res, err)
	}
	waitEvent(t, ev, app.EvToast, func(e core.Event) bool { return strings.Contains(fmt.Sprint(e.Data), "olá") })
}

// A core with no command at all serves an empty table, not null.
func TestKeymapEmpty(t *testing.T) {
	a := app.New(config.Default(), apptest.New().Ports())
	t.Cleanup(a.Close)
	s := New(a, Options{})
	t.Cleanup(func() { _ = s.Close() })
	w := httptest.NewRecorder()
	s.keymap(w, httptest.NewRequest(http.MethodGet, "/v1/keymap", nil))
	if got := strings.TrimSpace(w.Body.String()); w.Code != http.StatusOK || got != "[]" {
		t.Fatalf("keymap = %d %s", w.Code, got)
	}
}

// waitEvent reads ev until an event named name (and matching ok, if
// given) arrives, for up to 5 s.
func waitEvent(t *testing.T, ev <-chan core.Event, name string, ok func(core.Event) bool) {
	t.Helper()
	timeout := time.After(5 * time.Second)
	for {
		select {
		case got := <-ev:
			if got.Name == name && (ok == nil || ok(got)) {
				return
			}
		case <-timeout:
			t.Fatalf("no %s event", name)
		}
	}
}
