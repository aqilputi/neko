package main

import (
	"context"
	"os"
	"path/filepath"
	"slices"
	"strings"
	"testing"
	"time"

	tea "charm.land/bubbletea/v2"

	"github.com/aqilputi/nekuroddo/internal/app"
	"github.com/aqilputi/nekuroddo/internal/app/apptest"
	"github.com/aqilputi/nekuroddo/internal/config"
	"github.com/aqilputi/nekuroddo/internal/rpc"
)

// dialT is an rpc client for the test, closed with it.
func dialT(t *testing.T, addr string) *rpc.Client {
	t.Helper()
	c, err := rpc.Dial(addr, "")
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(c.Close)
	return c
}

func ctxT(t *testing.T) context.Context {
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	t.Cleanup(cancel)
	return ctx
}

// nk serve keeps the core up to date with no TUI: a refresh on every
// period, and the hook log polled (an event that needs the user goes out
// as session.notification).
func TestServeRefreshesWithoutTUI(t *testing.T) {
	isolate(t)
	fakeClaudeBin(t)
	if err := os.MkdirAll(filepath.Dir(config.Path()), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(config.Path(), []byte("refresh_seconds = 0.5\nusage_refresh_seconds = 0\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	// an older event: the log exists when the core skips to its end
	if code, _, _ := nk([]string{"hook"}, `{"hook_event_name":"Stop","session_id":"s0","cwd":"/tmp"}`); code != 0 {
		t.Fatal("hook")
	}
	addr := startServe(t, "--listen", "unix://"+filepath.Join(runtimeDir(t), "s.sock"))
	c := dialT(t, addr)
	ctx := ctxT(t)
	waitFor(t, func() bool { // the first refresh ran: the hook log was skipped to its end before it
		s, err := c.Snapshot(ctx)
		return err == nil && !s.At.IsZero()
	})
	evs, err := c.Events(ctx, "")
	if err != nil {
		t.Fatal(err)
	}
	defer evs.Close()
	// a hook fires meanwhile (nk hook writes the log the core polls)
	if code, _, _ := nk([]string{"hook"}, `{"hook_event_name":"Notification","session_id":"s1","cwd":"/tmp","notification_type":"permission_prompt","message":"pode?"}`); code != 0 {
		t.Fatal("hook")
	}
	snaps, notified := 0, false
	for snaps < 2 || !notified {
		select {
		case e, ok := <-evs.C:
			if !ok {
				t.Fatalf("stream ended: %v", evs.Err())
			}
			switch e.Name {
			case app.EvSnapshotChanged:
				snaps++
			case app.EvSessionNotification:
				notified = true
			}
		case <-ctx.Done():
			t.Fatalf("snapshots %d, notified %v", snaps, notified)
		}
	}
}

func TestScheduleOnce(t *testing.T) {
	f := apptest.New()
	a := app.New(config.Default(), f.Ports())
	defer a.Close()
	start, stop := scheduleOnce(context.Background(), a, app.Schedule{Refresh: time.Hour, Events: time.Hour})
	start()
	start() // once
	waitFor(t, func() bool { return f.Sessions.Count("Refresh") >= 1 })
	stop()
	if n := f.Sessions.Count("Refresh"); n != 1 {
		t.Fatalf("one scheduler: %d refreshes", n)
	}
	// never started: stop returns at once
	_, stop2 := scheduleOnce(context.Background(), a, app.Schedule{})
	stop2()
}

// nk --listen hands the TUI's panes to the server: a client sees the pane
// (and its session) the TUI started, and can attach to it.
func TestPaneHookAttachesToTheServer(t *testing.T) {
	isolate(t)
	fakeClaudeBin(t)
	h := newHost(config.Default(), os.Stderr)
	defer h.Close()
	srv, addr, err := listenRPC(h, "unix://"+filepath.Join(runtimeDir(t), "p.sock"))
	if err != nil {
		t.Fatal(err)
	}
	defer srv.Close()
	p := apptest.NewFakePane("tui1", "claude")
	paneHook(srv)(p, "sess-1", 100, 30)
	ps, err := dialT(t, addr).Panes(ctxT(t))
	if err != nil || len(ps) != 1 || ps[0].ID != "tui1" || ps[0].SessionID != "sess-1" || ps[0].Cols != 100 {
		t.Fatalf("panes = %+v %v", ps, err)
	}
}

// nk --listen starts the core's scheduler from the TUI (once it follows the
// bus): before the TUI's first size nothing refreshed, after it the
// snapshot the server serves is fresh.
func TestListenStartsTheSchedulerFromTheTUI(t *testing.T) {
	isolate(t)
	fakeClaudeBin(t)
	sock := "unix://" + filepath.Join(runtimeDir(t), "l.sock")
	defer swapTUI(func(m tea.Model) (tea.Model, error) {
		c := dialT(t, sock)
		if s, err := c.Snapshot(ctxT(t)); err != nil || !s.At.IsZero() {
			t.Errorf("before the TUI's first size: %+v %v", s.At, err)
		}
		m, _ = m.Update(tea.WindowSizeMsg{Width: 100, Height: 30})
		waitFor(t, func() bool {
			s, err := c.Snapshot(ctxT(t))
			return err == nil && !s.At.IsZero()
		})
		return m, nil
	})()
	if code, _, errs := nk([]string{"--listen", sock}, ""); code != 0 {
		t.Fatalf("nk --listen: %d %q", code, errs)
	}
}

// nk serve --demo serves the scripted demo world: hello answers and the
// snapshot lists the demo sessions.
func TestServeDemo(t *testing.T) {
	isolate(t)
	t.Setenv("NEKO_CLAUDE", "/bin/false") // the demo never runs claude
	addr := startServe(t, "--demo", "--listen", "unix://"+filepath.Join(runtimeDir(t), "d.sock"))
	hello(t, addr, "")
	c := dialT(t, addr)
	// a client can tell it is looking at the fake world (docs/protocolo.md)
	if h, err := c.Hello(ctxT(t)); err != nil || !slices.Contains(h.Capabilities, "demo") {
		t.Fatalf("demo hello capabilities = %v, %v; want \"demo\"", h.Capabilities, err)
	}
	waitFor(t, func() bool {
		s, err := c.Snapshot(ctxT(t))
		return err == nil && len(s.Sessions) > 0
	})
	s, _ := c.Snapshot(ctxT(t))
	for _, x := range s.Sessions {
		if strings.TrimSpace(x.SessionID) == "" {
			t.Fatalf("a demo session without an id: %+v", x)
		}
	}
	// the world (nkw) reads its keys and palette from here too
	km, err := c.Keymap(ctxT(t))
	if err != nil {
		t.Fatal(err)
	}
	if got, _ := km.Resolve("normal.world", "world+w"); got != "world.walk" {
		t.Fatalf("demo keymap: normal.world world+w = %q", got)
	}
	if got, _ := km.Resolve("world.walk", "f"); got != "world.hint" {
		t.Fatalf("demo keymap: world.walk f = %q", got)
	}
}
