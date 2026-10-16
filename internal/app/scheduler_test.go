package app_test

import (
	"context"
	"errors"
	"runtime"
	"sync"
	"testing"
	"time"

	"github.com/aqilputi/nekuroddo/internal/app"
	"github.com/aqilputi/nekuroddo/internal/app/apptest"
	"github.com/aqilputi/nekuroddo/internal/claude"
	"github.com/aqilputi/nekuroddo/internal/config"
	"github.com/aqilputi/nekuroddo/internal/events"
	"github.com/aqilputi/nekuroddo/internal/store"
)

// stepClock is apptest's FakeClock that also makes timers (app.TimerClock):
// they fire only when Advance passes them, so a test drives the schedule
// step by step.
type stepClock struct {
	*apptest.FakeClock
	mu     sync.Mutex
	timers map[*stepTimer]struct{}
}

type stepTimer struct {
	clk *stepClock
	at  time.Time
	d   time.Duration
	c   chan time.Time
}

func newStepClock() *stepClock {
	return &stepClock{FakeClock: apptest.NewFakeClock(apptest.Epoch), timers: map[*stepTimer]struct{}{}}
}

func (c *stepClock) NewTimer(d time.Duration) app.Timer {
	t := &stepTimer{clk: c, at: c.Now().Add(d), d: d, c: make(chan time.Time, 1)}
	c.mu.Lock()
	c.timers[t] = struct{}{}
	c.mu.Unlock()
	return t
}

func (t *stepTimer) C() <-chan time.Time { return t.c }

func (t *stepTimer) Stop() {
	t.clk.mu.Lock()
	delete(t.clk.timers, t)
	t.clk.mu.Unlock()
}

// Advance moves the clock and fires the timers it passed.
func (c *stepClock) Advance(d time.Duration) {
	c.FakeClock.Advance(d)
	now := c.Now()
	c.mu.Lock()
	defer c.mu.Unlock()
	for t := range c.timers {
		if !t.at.After(now) {
			delete(c.timers, t)
			t.c <- now
		}
	}
}

// pending is the durations of the timers armed and not fired nor stopped.
func (c *stepClock) pending() map[time.Duration]int {
	c.mu.Lock()
	defer c.mu.Unlock()
	out := map[time.Duration]int{}
	for t := range c.timers {
		out[t.d]++
	}
	return out
}

// countEvents is the EventLog port counting its polls.
type countEvents struct {
	apptest.FakeEvents
	mu    sync.Mutex
	polls int
}

func (e *countEvents) Poll() []events.Event {
	evs := e.FakeEvents.Poll()
	e.mu.Lock()
	e.polls++
	e.mu.Unlock()
	return evs
}

func (e *countEvents) count() int {
	e.mu.Lock()
	defer e.mu.Unlock()
	return e.polls
}

// eventually waits (5 s at most) for cond.
func eventually(t *testing.T, what string, cond func() bool) {
	t.Helper()
	deadline := time.Now().Add(5 * time.Second)
	for !cond() {
		if time.Now().After(deadline) {
			t.Fatalf("timed out waiting: %s", what)
		}
		time.Sleep(time.Millisecond)
	}
}

type schedRig struct {
	a    *app.App
	f    *apptest.Fakes
	clk  *stepClock
	evs  *countEvents
	stop context.CancelFunc
	done chan struct{}
}

// startSchedule runs a.RunSchedule(s) over fakes and a step clock; the
// test's cleanup checks it stops on cancel.
func startSchedule(t *testing.T, s app.Schedule, setup ...func(*apptest.Fakes)) *schedRig {
	t.Helper()
	f := apptest.New()
	for _, fn := range setup {
		fn(f)
	}
	clk := newStepClock()
	evs := &countEvents{}
	p := f.Ports()
	p.Clock, p.Events = clk, evs
	a := app.New(config.Default(), p)
	t.Cleanup(a.Close)
	ctx, stop := context.WithCancel(context.Background())
	r := &schedRig{a: a, f: f, clk: clk, evs: evs, stop: stop, done: make(chan struct{})}
	go func() {
		a.RunSchedule(ctx, s)
		close(r.done)
	}()
	t.Cleanup(r.halt)
	return r
}

func (r *schedRig) halt() {
	r.stop()
	select {
	case <-r.done:
	case <-time.After(5 * time.Second):
		panic("RunSchedule did not stop")
	}
}

func (r *schedRig) refreshes() int { return r.f.Sessions.Count("Refresh") }
func (r *schedRig) usages() int    { return r.f.Claude.Count("FetchUsage") }

// idle waits until every loop sleeps on its timer again.
func (r *schedRig) idle(t *testing.T, want map[time.Duration]int) {
	t.Helper()
	eventually(t, "timers armed", func() bool {
		got := r.clk.pending()
		if len(got) != len(want) {
			return false
		}
		for d, n := range want {
			if got[d] != n {
				return false
			}
		}
		return true
	})
}

func TestScheduleNormalize(t *testing.T) {
	cases := []struct {
		in, want app.Schedule
	}{
		{app.Schedule{}, app.Schedule{Refresh: 2 * time.Second, Events: 400 * time.Millisecond}},
		{app.Schedule{Refresh: 499 * time.Millisecond, Events: -1, Usage: -1}, app.Schedule{Refresh: 2 * time.Second, Events: 400 * time.Millisecond}},
		{app.Schedule{Refresh: 500 * time.Millisecond, Events: time.Second, Usage: time.Minute}, app.Schedule{Refresh: 500 * time.Millisecond, Events: time.Second, Usage: time.Minute}},
	}
	for _, c := range cases {
		if got := c.in.Normalize(); got != c.want {
			t.Errorf("%+v.Normalize() = %+v, want %+v", c.in, got, c.want)
		}
	}
}

func TestScheduleFromConfig(t *testing.T) {
	cfg := config.Default()
	cfg.Refresh, cfg.UsageRefresh = 3, 300
	if got := app.ScheduleFrom(cfg); got != (app.Schedule{Refresh: 3 * time.Second, Events: 400 * time.Millisecond, Usage: 300 * time.Second}) {
		t.Fatalf("ScheduleFrom = %+v", got)
	}
	cfg.Refresh, cfg.UsageRefresh = 0.1, 0
	if got := app.ScheduleFrom(cfg); got.Refresh != 2*time.Second || got.Usage != 0 {
		t.Fatalf("under half a second means the 2 s default; usage 0 is off: %+v", got)
	}
}

func TestRunScheduleStartsAtOnceAndTicks(t *testing.T) {
	r := startSchedule(t, app.Schedule{Refresh: 2 * time.Second, Events: 400 * time.Millisecond, Usage: 10 * time.Second},
		func(f *apptest.Fakes) { f.Claude.Usage = claude.UsageReport{Raw: "5h"} })
	all := map[time.Duration]int{2 * time.Second: 1, 400 * time.Millisecond: 1, 10 * time.Second: 1}
	r.idle(t, all)
	if r.refreshes() != 1 || r.usages() != 1 || r.evs.count() != 1 {
		t.Fatalf("at start: a refresh, a /usage and the hook log skipped to its end: %d %d %d", r.refreshes(), r.usages(), r.evs.count())
	}
	for i := 1; i <= 4; i++ { // 1.6 s: four polls, no refresh yet
		r.clk.Advance(400 * time.Millisecond)
		eventually(t, "poll", func() bool { return r.evs.count() == 1+i })
		r.idle(t, all)
	}
	if r.refreshes() != 1 {
		t.Fatalf("refreshed early: %d", r.refreshes())
	}
	r.clk.Advance(400 * time.Millisecond) // 2 s
	eventually(t, "refresh", func() bool { return r.refreshes() == 2 })
	r.idle(t, all)
	r.clk.Advance(8 * time.Second) // 10 s: /usage again
	eventually(t, "usage", func() bool { return r.usages() == 2 })
	r.idle(t, all)
	if rep, err := r.a.Usage(); err != nil || rep.Raw != "5h" {
		t.Fatal("the report went through OnUsage")
	}
}

func TestRunScheduleHookEventRefreshesAndResetsTheTimer(t *testing.T) {
	r := startSchedule(t, app.Schedule{Refresh: 2 * time.Second, Events: 400 * time.Millisecond})
	all := map[time.Duration]int{2 * time.Second: 1, 400 * time.Millisecond: 1}
	r.idle(t, all)
	if r.usages() != 0 {
		t.Fatal("usage 0: never fetched")
	}
	sub := r.a.Subscribe(t.Context())
	r.clk.Advance(800 * time.Millisecond) // two steps at once: one poll (the timer fires once)
	eventually(t, "poll", func() bool { return r.evs.count() == 2 })
	r.idle(t, all)
	r.evs.Push(events.Event{Name: "PostToolUse", SessionID: "a"})
	r.clk.Advance(400 * time.Millisecond) // t=1.2 s: the event asks for a refresh now
	eventually(t, "refresh on event", func() bool { return r.refreshes() == 2 })
	r.idle(t, all)
	if e, ok := r.a.LastEvent("a"); !ok || e.Name != "PostToolUse" {
		t.Fatal("the events went through OnHookEvents")
	}
	if len(watchNamed(watchDrain(sub), app.EvSnapshotChanged)) != 1 {
		t.Fatal("the refresh published snapshot.changed")
	}
	// the refresh timer restarted at 1.2 s: nothing at 2 s, the next at 3.2 s
	for i := range 4 {
		r.clk.Advance(400 * time.Millisecond)
		eventually(t, "poll", func() bool { return r.evs.count() == 4+i })
		r.idle(t, all)
	}
	if r.refreshes() != 2 {
		t.Fatalf("the timer was not reset: %d refreshes", r.refreshes())
	}
	r.clk.Advance(400 * time.Millisecond)
	eventually(t, "refresh", func() bool { return r.refreshes() == 3 })
}

// blockingSessions holds Refresh until released, to show refreshes asked
// meanwhile coalesce into one.
type blockingSessions struct {
	*apptest.FakeSessions
	entered chan struct{}
	release chan struct{}
}

func (s *blockingSessions) Refresh() store.Snapshot {
	s.entered <- struct{}{}
	<-s.release
	return s.FakeSessions.Refresh()
}

func TestRunScheduleCoalescesRefreshRequests(t *testing.T) {
	f := apptest.New()
	clk := newStepClock()
	evs := &countEvents{}
	bs := &blockingSessions{FakeSessions: f.Sessions, entered: make(chan struct{}, 8), release: make(chan struct{})}
	p := f.Ports()
	p.Clock, p.Events, p.Sessions = clk, evs, bs
	a := app.New(config.Default(), p)
	defer a.Close()
	ctx, stop := context.WithCancel(context.Background())
	done := make(chan struct{})
	go func() { a.RunSchedule(ctx, app.Schedule{Refresh: time.Minute, Events: time.Second}); close(done) }()
	<-bs.entered // the first refresh is running
	for i := range 3 {
		evs.Push(events.Event{Name: "PostToolUse", SessionID: "a"})
		eventually(t, "poller armed", func() bool { return clk.pending()[time.Second] == 1 })
		clk.Advance(time.Second)
		eventually(t, "poll", func() bool { return evs.count() == 2+i })
	}
	bs.release <- struct{}{} // the first ends; the three asks make one more
	<-bs.entered
	bs.release <- struct{}{}
	eventually(t, "idle", func() bool { return clk.pending()[time.Minute] == 1 })
	if n := f.Sessions.Count("Refresh"); n != 2 {
		t.Fatalf("refreshes = %d, want 2 (coalesced)", n)
	}
	stop()
	<-done
}

func TestRunSchedulePeerMeowGoesOnTheBus(t *testing.T) {
	r := startSchedule(t, app.Schedule{Refresh: time.Minute, Events: time.Second})
	r.f.Sessions.SetSnapshot(store.Snapshot{Sessions: []store.Session{{SessionID: "a", Name: "alpha"}, {SessionID: "b", Name: "bravo"}}})
	r.idle(t, map[time.Duration]int{time.Minute: 1, time.Second: 1})
	r.a.Refresh()
	sub := r.a.Subscribe(t.Context())
	r.evs.Push(
		events.Event{Name: "Notification", Type: "permission_prompt", SessionID: "a", Message: "pode?"},
		events.Event{Name: "PostToolUse", SessionID: "a"},
		events.Event{Name: "UserPromptSubmit", Type: "peer", SessionID: "b", From: "alpha", Message: "oi"},
	)
	r.clk.Advance(time.Second)
	var got []app.Notice
	eventually(t, "notices", func() bool {
		for _, e := range watchNamed(watchDrain(sub), app.EvNotice) {
			got = append(got, e.Data.(app.Notice))
		}
		return len(got) >= 2
	})
	if got[0].Meow != "miau! pode?" || got[1] != (app.Notice{Meow: "✉ alpha → bravo"}) {
		t.Fatalf("notices in order, the peer meow too: %+v", got)
	}
}

func TestRunScheduleUsageError(t *testing.T) {
	r := startSchedule(t, app.Schedule{Usage: time.Minute},
		func(f *apptest.Fakes) { f.Claude.Errs = map[string]error{"FetchUsage": errors.New("boom")} })
	r.idle(t, map[time.Duration]int{2 * time.Second: 1, 400 * time.Millisecond: 1, time.Minute: 1})
	if _, err := r.a.Usage(); err == nil {
		t.Fatal("the fetch error went through OnUsage")
	}
	r.clk.Advance(time.Minute)
	eventually(t, "fetched again", func() bool { return r.usages() == 2 })
}

func TestRunScheduleStopsOnCancel(t *testing.T) {
	before := runtime.NumGoroutine()
	for range 5 {
		r := startSchedule(t, app.Schedule{Usage: time.Minute})
		r.idle(t, map[time.Duration]int{2 * time.Second: 1, 400 * time.Millisecond: 1, time.Minute: 1})
		r.halt()
		if n := len(r.clk.pending()); n != 0 {
			t.Fatalf("timers left armed: %d", n)
		}
		r.a.Close()
	}
	eventually(t, "no goroutine left", func() bool { return runtime.NumGoroutine() <= before })
}

func TestRunScheduleCanceledBeforeStart(t *testing.T) {
	f := apptest.New()
	a := app.New(config.Default(), f.Ports()) // a plain Clock: real timers
	defer a.Close()
	ctx, stop := context.WithCancel(context.Background())
	stop()
	a.RunSchedule(ctx, app.Schedule{Usage: time.Hour})
	if f.Sessions.Count("Refresh") != 0 || f.Claude.Count("FetchUsage") != 0 {
		t.Fatal("a context already done runs nothing")
	}
}

func TestRunScheduleRealTimers(t *testing.T) {
	f := apptest.New()
	p := f.Ports()
	p.Clock = nil // wall time and real timers
	a := app.New(config.Default(), p)
	defer a.Close()
	f.Claude.Usage = claude.UsageReport{Raw: "x"}
	ctx, stop := context.WithCancel(context.Background())
	done := make(chan struct{})
	go func() {
		a.RunSchedule(ctx, app.Schedule{Refresh: 500 * time.Millisecond, Events: 10 * time.Millisecond, Usage: 20 * time.Millisecond})
		close(done)
	}()
	eventually(t, "ticks on real timers", func() bool {
		return f.Sessions.Count("Refresh") >= 2 && f.Claude.Count("FetchUsage") >= 2
	})
	stop()
	<-done
}
