package main

import (
	"context"
	"fmt"
	"io"
	"os"
	"os/signal"
	"sync"
	"syscall"
	"time"

	"github.com/aqilputi/nekuroddo/internal/app"
	"github.com/aqilputi/nekuroddo/internal/config"
	"github.com/aqilputi/nekuroddo/internal/demo"
	"github.com/aqilputi/nekuroddo/internal/lua"
	"github.com/aqilputi/nekuroddo/internal/rpc"
	"github.com/aqilputi/nekuroddo/internal/rpc/proto"
	"github.com/aqilputi/nekuroddo/internal/tui"
)

// serve is nk serve: the core with no TUI, served over RPC until ctx ends
// or SIGINT/SIGTERM. The core's scheduler (app.RunSchedule) keeps it up to
// date: refresh, hook log, /usage. With --demo the core runs over the
// scripted demo world (internal/demo) instead of the real claude.
func serve(ctx context.Context, args []string, stdout, stderr io.Writer) int {
	fs := newFlags("nk serve", stderr)
	listen := fs.String("listen", "unix://"+defaultSocket(), "endereço: unix:///caminho ou tcp://127.0.0.1:porta")
	demoWorld := fs.Bool("demo", false, "serve um mundo de demonstração roteirizado (sem claude de verdade)")
	if err := fs.Parse(args); err != nil {
		return 2
	}
	if fs.NArg() > 0 {
		fmt.Fprintf(stderr, "nekuroddo serve: argumento inesperado %q\n", fs.Arg(0))
		return 2
	}
	cfg := loadConfig(stderr)
	var h *host
	if *demoWorld {
		h = newDemoHost(cfg)
	} else {
		h = newHost(cfg, stderr)
	}
	defer h.Close()
	srv, addr, err := listenRPC(h, *listen)
	if err != nil {
		fmt.Fprintln(stderr, "nekuroddo serve:", err)
		return 1
	}
	defer srv.Close()
	fmt.Fprintf(stdout, "nekuroddo: servindo em %s\n", addr)
	ctx, stop := signal.NotifyContext(ctx, os.Interrupt, syscall.SIGTERM)
	defer stop()
	h.app.RunSchedule(ctx, app.ScheduleFrom(cfg))
	return 0
}

// newDemoHost is newHost over the demo world (demo.Ports on wall time): the
// builtin commands and a Lua runtime with nk serve's capabilities, but none
// of the user's Lua files (the demo must look the same everywhere).
func newDemoHost(cfg config.Config) *host {
	hostsBuilt.Add(1)
	a := app.New(cfg, demo.Ports(wallClock{}))
	if err := a.RegisterBuiltins(); err != nil {
		panic(err) // fixed at compile time: a bug the app tests catch
	}
	rt, err := lua.New(a, luaOptions(cfg.Lua, serveCaps))
	if err == nil {
		err = rt.RegisterCommands()
	}
	if err != nil {
		panic(err) // only a nil app or a clash of fixed names: bugs, not conditions
	}
	return &host{app: a, lua: rt, caps: []string{proto.CapDemo}}
}

// scheduleOnce returns start, which runs a's scheduler (app.RunSchedule) on
// its own goroutine the first time it is called (the TUI calls it once it
// follows the bus), and stop, which ends it and waits for it (fine if it
// never started).
func scheduleOnce(ctx context.Context, a *app.App, s app.Schedule) (start, stop func()) {
	ctx, cancel := context.WithCancel(ctx)
	done := make(chan struct{})
	var once sync.Once
	start = func() {
		once.Do(func() {
			go func() {
				defer close(done)
				a.RunSchedule(ctx, s)
			}()
		})
	}
	stop = func() {
		cancel()
		once.Do(func() { close(done) })
		<-done
	}
	return start, stop
}

// paneHook hands the panes the TUI starts to srv (nk --listen), so a
// client attaching the same session reuses that PTY instead of starting a
// second `claude attach`.
func paneHook(srv *rpc.Server) tui.PaneHook { return srv.AttachPane }

// wallClock is the demo world's clock: real time.
type wallClock struct{}

func (wallClock) Now() time.Time { return time.Now() }
