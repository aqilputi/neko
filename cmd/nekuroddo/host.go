package main

import (
	"flag"
	"fmt"
	"io"
	"sync/atomic"

	"github.com/aqilputi/nekuroddo/internal/app"
	"github.com/aqilputi/nekuroddo/internal/config"
	"github.com/aqilputi/nekuroddo/internal/lua"
	"github.com/aqilputi/nekuroddo/internal/rpc"
	"github.com/aqilputi/nekuroddo/internal/rpc/proto"
	"github.com/aqilputi/nekuroddo/internal/wire"
)

// hostsBuilt counts newHost calls: the hooks must never build one.
var hostsBuilt atomic.Int64

// host is the composition root of one process: the core over the real
// adapters, its builtin commands, and the Lua runtime with the user's files.
type host struct {
	app *app.App
	lua *lua.Runtime
	// caps are announced in hello besides "panes" and "lua" ("demo" for
	// the scripted world)
	caps []string
}

// The UI capabilities each host gives Lua (nk.ui.has). The TUI draws all of
// them but the GUI's scene; nk serve has no screen of its own, so only what
// reaches a remote client through the bus (toast, confirm.requested,
// ui.scene for the world, nkw) and the desktop notification it sends itself.
var (
	tuiCaps   = []string{lua.CapToast, lua.CapConfirm, lua.CapCat, lua.CapPalette, lua.CapNotify}
	serveCaps = []string{lua.CapToast, lua.CapConfirm, lua.CapNotify, lua.CapScene}
)

// autoReloadEvery is how often [lua] auto_reload looks at the files (tests
// shorten it).
var autoReloadEvery = lua.AutoReloadEvery

// newHost builds the core (wire.Default + RegisterBuiltins) and loads Lua
// with the client's UI capabilities (none given: nk serve's). A Lua error
// is a warning on stderr, never fatal: the last good files (or none) stay.
func newHost(cfg config.Config, stderr io.Writer, caps ...string) *host {
	hostsBuilt.Add(1)
	a := app.New(cfg, wire.Default(cfg))
	if err := a.RegisterBuiltins(); err != nil {
		// the builtins are fixed at compile time: a clash is a bug the app
		// tests catch, never a runtime condition
		panic(err)
	}
	if len(caps) == 0 {
		caps = serveCaps
	}
	rt, err := lua.New(a, luaOptions(cfg.Lua, caps))
	if err == nil {
		err = rt.RegisterCommands()
	}
	if err != nil {
		panic(err) // only a nil app or a clash of fixed names: bugs, not conditions
	}
	if err := rt.Load(); err != nil {
		fmt.Fprintln(stderr, "nekuroddo:", err)
	}
	if cfg.Lua.AutoReload {
		rt.AutoReload(autoReloadEvery)
	}
	return &host{app: a, lua: rt}
}

// luaOptions are the runtime's options from the [lua] table.
func luaOptions(c config.Lua, caps []string) lua.Options {
	return lua.Options{
		CommandTimeout: c.CommandTimeout(), CallbackTimeout: c.CallbackTimeout(),
		Caps: caps, Disabled: !c.Enabled,
	}
}

// Close stops Lua and ends the core's subscriptions.
func (h *host) Close() {
	h.lua.Close()
	h.app.Close()
}

// listenRPC serves h's core over RPC at spec (unix:///… or
// tcp://127.0.0.1:…), with POST /v1/lua/eval run by h's Lua runtime. A TCP
// listener gets a fresh token, written 0600 where nk run looks for it. It
// returns the server and the address actually listened on.
func listenRPC(h *host, spec string) (*rpc.Server, string, error) {
	l, err := rpc.Listen(spec)
	if err != nil {
		return nil, "", err
	}
	// the host always runs the Lua runtime: clients may see Lua commands
	o := rpc.Options{Version: version, Capabilities: append([]string{proto.CapLua}, h.caps...), Lua: h.lua}
	addr := spec
	if l.Addr().Network() != "unix" {
		o.Token = rpc.NewToken()
		if err := rpc.WriteTokenFile(rpc.DefaultTokenPath(), o.Token); err != nil {
			_ = l.Close()
			return nil, "", fmt.Errorf("token: %w", err)
		}
		addr = "tcp://" + l.Addr().String()
	}
	s := rpc.New(h.app, o)
	go func() { _ = s.Serve(l) }() // ends with http.ErrServerClosed on Close
	return s, addr, nil
}

func defaultSocket() string { return rpc.DefaultSocketPath() }
func defaultToken() string  { return rpc.DefaultTokenPath() }

// newFlags is a flag set that reports its errors on stderr and never exits.
func newFlags(name string, stderr io.Writer) *flag.FlagSet {
	fs := flag.NewFlagSet(name, flag.ContinueOnError)
	fs.SetOutput(stderr)
	return fs
}
