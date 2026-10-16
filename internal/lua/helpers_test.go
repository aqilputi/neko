package lua

import (
	"context"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/aqilputi/nekuroddo/internal/app"
	"github.com/aqilputi/nekuroddo/internal/core"
	"github.com/aqilputi/nekuroddo/internal/store"
)

// recorder keeps every bus event published after it started.
type recorder struct {
	mu  sync.Mutex
	evs []core.Event
}

func (e *env) record() *recorder {
	ctx, cancel := context.WithCancel(context.Background())
	e.t.Cleanup(cancel)
	ch := e.app.Subscribe(ctx)
	rec := &recorder{}
	go func() {
		for ev := range ch {
			rec.mu.Lock()
			rec.evs = append(rec.evs, ev)
			rec.mu.Unlock()
		}
	}()
	return rec
}

func (r *recorder) named(name string) []core.Event {
	r.mu.Lock()
	defer r.mu.Unlock()
	var out []core.Event
	for _, ev := range r.evs {
		if ev.Name == name {
			out = append(out, ev)
		}
	}
	return out
}

// toasts are the texts of the toast events so far.
func (r *recorder) toasts() []string {
	var out []string
	for _, ev := range r.named(app.EvToast) {
		out = append(out, ev.Data.(app.ToastData).Text)
	}
	return out
}

func (r *recorder) hasToast(sub string) bool {
	for _, t := range r.toasts() {
		if strings.Contains(t, sub) {
			return true
		}
	}
	return false
}

// eventually polls cond for up to 3 s.
func eventually(t *testing.T, what string, cond func() bool) {
	t.Helper()
	deadline := time.Now().Add(3 * time.Second)
	for !cond() {
		if time.Now().After(deadline) {
			t.Fatalf("timed out waiting for %s", what)
		}
		time.Sleep(2 * time.Millisecond)
	}
}

// eval runs code and fails the test on error.
func (e *env) eval(code string) string {
	e.t.Helper()
	out, err := e.rt.Eval(code)
	if err != nil {
		e.t.Fatalf("Eval(%q): %v", code, err)
	}
	return out
}

// load runs Load and fails the test on error.
func (e *env) load() {
	e.t.Helper()
	if err := e.rt.Load(); err != nil {
		e.t.Fatalf("Load: %v", err)
	}
}

// sessions makes snap the core's snapshot (through the fake store).
func (e *env) sessions(ss ...store.Session) {
	e.f.Sessions.SetSnapshot(store.Snapshot{Sessions: ss, At: e.f.Clock.Now()})
	e.app.Refresh()
}

func commandNamed(cs []core.Command, name string) (core.Command, bool) {
	for _, c := range cs {
		if c.Name == name {
			return c, true
		}
	}
	return core.Command{}, false
}
