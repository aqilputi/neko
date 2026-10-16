package main

import (
	"flag"
	"fmt"
	"io"
	"sync/atomic"

	"github.com/aqilputi/nekuroddo/internal/app"
	"github.com/aqilputi/nekuroddo/internal/config"
	"github.com/aqilputi/nekuroddo/internal/core"
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
}

// newHost builds the core (wire.Default + RegisterBuiltins) and loads Lua.
// A Lua error is a warning on stderr, never fatal: the last good files (or
// none) stay.
func newHost(cfg config.Config, stderr io.Writer) *host {
	hostsBuilt.Add(1)
	a := app.New(cfg, wire.Default(cfg))
	if err := a.RegisterBuiltins(); err != nil {
		// the builtins are fixed at compile time: a clash is a bug the app
		// tests catch, never a runtime condition
		panic(err)
	}
	rt, err := lua.New(a, lua.Options{})
	if err == nil {
		err = registerLua(a, rt)
	}
	if err != nil {
		panic(err) // only a nil app or a clash of fixed names: bugs, not conditions
	}
	if err := rt.Load(); err != nil {
		fmt.Fprintln(stderr, "nekuroddo:", err)
	}
	return &host{app: a, lua: rt}
}

// registerLua adds the commands that drive the runtime itself.
func registerLua(a *app.App, rt *lua.Runtime) error {
	return a.Registry().Register(core.Command{
		Name: "lua.reload", Title: "recarrega o init.lua e os plugins", Hint: "lua",
	}, func(app.Context, app.Args) (app.Result, error) {
		if err := rt.Reload(); err != nil {
			return app.Fail(err), err
		}
		return app.Info(fmt.Sprintf("lua recarregado (%d arquivos)", len(rt.Files()))), nil
	})
}

// Close stops Lua and ends the core's subscriptions.
func (h *host) Close() {
	h.lua.Close()
	h.app.Close()
}

// listenRPC serves a over RPC at spec (unix:///… or tcp://127.0.0.1:…). A
// TCP listener gets a fresh token, written 0600 where nk run looks for it.
// It returns the server and the address actually listened on.
func listenRPC(a *app.App, spec string) (*rpc.Server, string, error) {
	l, err := rpc.Listen(spec)
	if err != nil {
		return nil, "", err
	}
	// the host always runs the Lua runtime: clients may see Lua commands
	o := rpc.Options{Version: version, Capabilities: []string{proto.CapLua}}
	addr := spec
	if l.Addr().Network() != "unix" {
		o.Token = rpc.NewToken()
		if err := rpc.WriteTokenFile(rpc.DefaultTokenPath(), o.Token); err != nil {
			_ = l.Close()
			return nil, "", fmt.Errorf("token: %w", err)
		}
		addr = "tcp://" + l.Addr().String()
	}
	s := rpc.New(a, o)
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
