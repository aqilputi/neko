package main

import (
	"flag"
	"fmt"
	"io"
	"net"
	"time"

	"github.com/aqilputi/nekuroddo/internal/app"
	"github.com/aqilputi/nekuroddo/internal/config"
	"github.com/aqilputi/nekuroddo/internal/demo"
	"github.com/aqilputi/nekuroddo/internal/lua"
	"github.com/aqilputi/nekuroddo/internal/rpc"
	"github.com/aqilputi/nekuroddo/internal/rpc/proto"
	"github.com/aqilputi/nekuroddo/internal/wire"
)

// host is the core of an nkg that embeds it (plan §4.9): the same
// composition as nk's (cmd/nekuroddo/host.go), with the GUI's UI
// capabilities for Lua.
type host struct {
	app  *app.App
	lua  *lua.Runtime
	caps []string // announced in hello besides "panes" and "lua"
}

// guiCaps: the GUI draws every UI capability, the scene included.
var guiCaps = []string{lua.CapToast, lua.CapConfirm, lua.CapCat, lua.CapPalette, lua.CapScene, lua.CapNotify}

// newHost is the core over the real adapters with the user's Lua files. A
// Lua error is a warning on stderr; the last good files (or none) stay.
func newHost(cfg config.Config, stderr io.Writer) *host {
	a := app.New(cfg, wire.Default(cfg))
	rt := withLua(a, cfg)
	if err := rt.Load(); err != nil {
		fmt.Fprintln(stderr, "nkg:", err)
	}
	if cfg.Lua.AutoReload {
		rt.AutoReload(lua.AutoReloadEvery)
	}
	return &host{app: a, lua: rt}
}

// newDemoHost is the core over the demo world on wall time, with no user
// Lua files (the demo looks the same everywhere).
func newDemoHost(cfg config.Config) *host {
	a := app.New(cfg, demo.Ports(wallClock{}))
	return &host{app: a, lua: withLua(a, cfg), caps: []string{proto.CapDemo}}
}

// withLua registers the builtins and a Lua runtime on a.
func withLua(a *app.App, cfg config.Config) *lua.Runtime {
	if err := a.RegisterBuiltins(); err != nil {
		panic(err) // fixed at compile time: a bug the app tests catch
	}
	c := cfg.Lua
	rt, err := lua.New(a, lua.Options{CommandTimeout: c.CommandTimeout(), CallbackTimeout: c.CallbackTimeout(),
		Caps: guiCaps, Disabled: !c.Enabled})
	if err == nil {
		err = rt.RegisterCommands()
	}
	if err != nil {
		panic(err) // only a nil app or a clash of fixed names: bugs, not conditions
	}
	return rt
}

// Close stops Lua and ends the core's subscriptions.
func (h *host) Close() {
	h.lua.Close()
	h.app.Close()
}

func rpcOptions(h *host, token string) rpc.Options {
	return rpc.Options{Version: version, Token: token,
		Capabilities: append([]string{proto.CapLua}, h.caps...), Lua: h.lua}
}

// listenRPC opens spec for nkg --listen, like nk --listen: a TCP listener
// gets a fresh token, written 0600 where nk run looks for it. It returns
// the listener, that token ("" for a unix socket) and the address.
func listenRPC(spec string) (net.Listener, string, string, error) {
	l, err := rpc.Listen(spec)
	if err != nil {
		return nil, "", "", err
	}
	if l.Addr().Network() == "unix" {
		return l, "", spec, nil
	}
	token := rpc.NewToken()
	if err := rpc.WriteTokenFile(rpc.DefaultTokenPath(), token); err != nil {
		_ = l.Close()
		return nil, "", "", fmt.Errorf("token: %w", err)
	}
	return l, token, "tcp://" + l.Addr().String(), nil
}

// errHelp is flag.ErrHelp (-h): usage already printed.
var errHelp = flag.ErrHelp

func newFlagSet(stderr io.Writer) *flag.FlagSet {
	fs := flag.NewFlagSet("nkg", flag.ContinueOnError)
	fs.SetOutput(stderr)
	return fs
}

// wallClock is the demo world's clock: real time.
type wallClock struct{}

func (wallClock) Now() time.Time { return time.Now() }
