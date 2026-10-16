package main

import (
	"context"
	"fmt"
	"io"
	"os"
	"os/signal"
	"syscall"
	"time"

	"github.com/aqilputi/nekuroddo/internal/app"
	"github.com/aqilputi/nekuroddo/internal/config"
)

// eventsEvery is how often the hook log is polled (the TUI's pace).
const eventsEvery = 400 * time.Millisecond

// serve is nk serve: the core with no TUI, served over RPC until ctx ends
// or SIGINT/SIGTERM.
func serve(ctx context.Context, args []string, stdout, stderr io.Writer) int {
	fs := newFlags("nk serve", stderr)
	listen := fs.String("listen", "unix://"+defaultSocket(), "endereço: unix:///caminho ou tcp://127.0.0.1:porta")
	if err := fs.Parse(args); err != nil {
		return 2
	}
	if fs.NArg() > 0 {
		fmt.Fprintf(stderr, "nekuroddo serve: argumento inesperado %q\n", fs.Arg(0))
		return 2
	}
	cfg := loadConfig(stderr)
	h := newHost(cfg, stderr)
	defer h.Close()
	srv, addr, err := listenRPC(h.app, *listen)
	if err != nil {
		fmt.Fprintln(stderr, "nekuroddo serve:", err)
		return 1
	}
	defer srv.Close()
	fmt.Fprintf(stdout, "nekuroddo: servindo em %s\n", addr)
	ctx, stop := signal.NotifyContext(ctx, os.Interrupt, syscall.SIGTERM)
	defer stop()
	schedule(ctx, h.app, refreshEvery(cfg), eventsEvery)
	return 0
}

// refreshEvery is the refresh period from the config (the TUI's rule: under
// half a second means the 2 s default).
func refreshEvery(cfg config.Config) time.Duration {
	d := time.Duration(cfg.Refresh * float64(time.Second))
	if d < 500*time.Millisecond {
		d = 2 * time.Second
	}
	return d
}

// schedule drives the core when no TUI does, until ctx ends: a refresh at
// once and every refresh period (app.RefreshNotices publishes what changed),
// and the hook log polled every poll period (app.OnHookEvents), with a
// refresh at once when an event asks for one. The notices go out on the bus
// as notice events; nobody here draws them. WP-5.1 moves this into app.
func schedule(ctx context.Context, a *app.App, refresh, poll time.Duration) {
	a.PollHookEvents() // start at the end: only new events matter
	a.Refresh()
	rt, pt := time.NewTicker(refresh), time.NewTicker(poll)
	defer rt.Stop()
	defer pt.Stop()
	for {
		select {
		case <-ctx.Done():
			return
		case <-rt.C:
			a.Refresh()
		case <-pt.C:
			if _, again := a.OnHookEvents(a.PollHookEvents()); again {
				a.Refresh()
				rt.Reset(refresh)
			}
		}
	}
}
