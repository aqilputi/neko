package main

import (
	"context"
	"io"
	"os"
	"path/filepath"
	"testing"
	"time"

	"github.com/aqilputi/nekuroddo/internal/app"
	"github.com/aqilputi/nekuroddo/internal/config"
	"github.com/aqilputi/nekuroddo/internal/lua"
	"github.com/aqilputi/nekuroddo/internal/rpc"
)

// writeInit puts init.lua in the (isolated) config dir.
func writeInit(t *testing.T, src string) string {
	t.Helper()
	p := filepath.Join(filepath.Dir(config.Path()), "init.lua")
	if err := os.MkdirAll(filepath.Dir(p), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(p, []byte(src), 0o644); err != nil {
		t.Fatal(err)
	}
	return p
}

func evalHost(t *testing.T, h *host, code string) string {
	t.Helper()
	out, err := h.lua.Eval(code)
	if err != nil {
		t.Fatalf("Eval(%q): %v", code, err)
	}
	return out
}

// The TUI's host gives Lua every UI capability the TUI draws; nk serve's
// only those a remote client gets through the bus (toast, confirm, notify,
// scene: the world, nkw, runs ui.scene).
func TestHostCapabilities(t *testing.T) {
	isolate(t)
	tuiHost := newHost(config.Default(), io.Discard, tuiCaps...)
	defer tuiHost.Close()
	for cap, want := range map[string]string{"toast": "true", "confirm": "true", "cat": "true", "palette": "true", "notify": "true", "scene": "false"} {
		if got := evalHost(t, tuiHost, `nk.ui.has("`+cap+`")`); got != want {
			t.Errorf("tui host has(%s) = %s", cap, got)
		}
	}
	serveHost := newHost(config.Default(), io.Discard) // no caps given: nk serve's
	defer serveHost.Close()
	for cap, want := range map[string]string{"toast": "true", "confirm": "true", "notify": "true", "scene": "true", "cat": "false", "palette": "false"} {
		if got := evalHost(t, serveHost, `nk.ui.has("`+cap+`")`); got != want {
			t.Errorf("serve host has(%s) = %s", cap, got)
		}
	}
}

// The host registers the runtime's own commands.
func TestHostRegistersLuaCommands(t *testing.T) {
	isolate(t)
	h := newHost(config.Default(), io.Discard)
	defer h.Close()
	for _, name := range []string{lua.CmdReload, lua.CmdMessages, lua.CmdEnable} {
		if _, ok := h.app.Registry().Lookup(name); !ok {
			t.Errorf("%s not registered", name)
		}
	}
	res, err := h.app.Run(app.Context{}, lua.CmdMessages, nil)
	if err != nil || res.Toast == "" {
		t.Fatalf("lua.messages: %+v %v", res, err)
	}
}

// [lua] enabled = false: no file loads.
func TestHostLuaDisabled(t *testing.T) {
	isolate(t)
	writeInit(t, `nk.command("from-init", function() end)`)
	cfg := config.Default()
	cfg.Lua.Enabled = false
	h := newHost(cfg, io.Discard)
	defer h.Close()
	if _, ok := h.app.Registry().Lookup("from-init"); ok {
		t.Fatal("a disabled runtime loads nothing")
	}
	cfg.Lua.Enabled = true
	h2 := newHost(cfg, io.Discard)
	defer h2.Close()
	if _, ok := h2.app.Registry().Lookup("from-init"); !ok {
		t.Fatal("enabled: init.lua loads")
	}
}

// The [lua] budgets reach the runtime's options.
func TestLuaOptions(t *testing.T) {
	o := luaOptions(config.Lua{Enabled: true, CommandTimeoutMS: 900, CallbackTimeoutMS: 20}, []string{"cat"})
	if o.CommandTimeout != 900*time.Millisecond || o.CallbackTimeout != 20*time.Millisecond || o.Disabled || len(o.Caps) != 1 {
		t.Fatalf("%+v", o)
	}
	if o := luaOptions(config.Lua{}, nil); !o.Disabled || o.CommandTimeout != lua.DefaultCommandTimeout || o.CallbackTimeout != lua.DefaultCallbackTimeout {
		t.Fatalf("zero [lua]: %+v", o)
	}
}

// The host's RPC server evaluates Lua (POST /v1/lua/eval) in its runtime.
func TestListenRPCServesLuaEval(t *testing.T) {
	isolate(t)
	dir := runtimeDir(t)
	h := newHost(config.Default(), io.Discard)
	defer h.Close()
	srv, addr, err := listenRPC(h, "unix://"+filepath.Join(dir, "nk.sock"))
	if err != nil {
		t.Fatal(err)
	}
	defer srv.Close()
	c, err := rpc.Dial(addr, "")
	if err != nil {
		t.Fatal(err)
	}
	defer c.Close()
	got, err := c.LuaEval(context.Background(), "1 + 1")
	if err != nil || got.Result != "2" || got.Error != "" {
		t.Fatalf("lua eval: %+v %v", got, err)
	}
}

// [lua] auto_reload = true: a changed init.lua is picked up on its own.
func TestHostAutoReload(t *testing.T) {
	isolate(t)
	defer func(d time.Duration) { autoReloadEvery = d }(autoReloadEvery)
	autoReloadEvery = 5 * time.Millisecond
	p := writeInit(t, `nk.command("a", function() end)`)
	cfg := config.Default()
	cfg.Lua.AutoReload = true
	h := newHost(cfg, io.Discard)
	defer h.Close()
	writeInit(t, `nk.command("b", function() end)`)
	at := time.Now().Add(time.Hour)
	if err := os.Chtimes(p, at, at); err != nil {
		t.Fatal(err)
	}
	waitFor(t, func() bool { _, ok := h.app.Registry().Lookup("b"); return ok })
}
