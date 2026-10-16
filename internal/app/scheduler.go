package app

// The scheduler: what keeps the core up to date with no client asking. It
// re-reads the sessions, polls the hook log and fetches /usage on its own
// goroutines; every result goes out on the bus (snapshot.changed, notice,
// usage.updated…), and clients only follow it.

import (
	"context"
	"sync"
	"time"

	"github.com/aqilputi/nekuroddo/internal/config"
	"github.com/aqilputi/nekuroddo/internal/events"
)

// Schedule is how often RunSchedule does each job.
type Schedule struct {
	Refresh time.Duration // re-read the sessions (under 500 ms: the 2 s default)
	Events  time.Duration // poll the hook log (0: 400 ms)
	Usage   time.Duration // fetch /usage (0: never)
}

// Defaults of a Schedule.
const (
	DefaultRefresh = 2 * time.Second
	DefaultEvents  = 400 * time.Millisecond
	minRefresh     = 500 * time.Millisecond
)

// ScheduleFrom is the schedule the config asks for (refresh and
// usage_refresh, in seconds).
func ScheduleFrom(cfg config.Config) Schedule {
	return Schedule{
		Refresh: time.Duration(cfg.Refresh * float64(time.Second)),
		Usage:   time.Duration(cfg.UsageRefresh * float64(time.Second)),
	}.Normalize()
}

// Normalize applies the defaults: a refresh under half a second means 2 s,
// no events period means 400 ms, and a negative usage period means off.
func (s Schedule) Normalize() Schedule {
	if s.Refresh < minRefresh {
		s.Refresh = DefaultRefresh
	}
	if s.Events <= 0 {
		s.Events = DefaultEvents
	}
	if s.Usage < 0 {
		s.Usage = 0
	}
	return s
}

// Timer is a one-shot timer RunSchedule waits on.
type Timer interface {
	C() <-chan time.Time
	Stop()
}

// TimerClock is a Clock that also makes timers: RunSchedule waits on them,
// so a fake one drives the schedule in tests. Over a plain Clock it uses
// real timers.
type TimerClock interface {
	Clock
	NewTimer(d time.Duration) Timer
}

type realTimer struct{ t *time.Timer }

func (r realTimer) C() <-chan time.Time { return r.t.C }
func (r realTimer) Stop()               { r.t.Stop() }

func (a *App) newTimer(d time.Duration) Timer {
	if tc, ok := a.clock.(TimerClock); ok {
		return tc.NewTimer(d)
	}
	return realTimer{time.NewTimer(d)}
}

// RunSchedule drives the core until ctx ends, then returns once every job
// stopped. At once: the hook log skips to its end (only new events matter),
// a refresh (RefreshNotices) and a /usage fetch (FetchUsage, OnUsage). Then
// a refresh every s.Refresh, the hook log every s.Events (OnHookEvents; an
// event asking for a refresh gets one now, which restarts the refresh
// period, and the asks made while a refresh runs make one more after it)
// and /usage every s.Usage (0: never). Each job has its own goroutine, so a
// slow /usage never holds a refresh back. Every result goes out on the bus;
// the meows of the messages between sessions, which OnHookEvents only
// returns, are published as notice here, so every client's cat says them.
func (a *App) RunSchedule(ctx context.Context, s Schedule) {
	if ctx.Err() != nil {
		return
	}
	s = s.Normalize()
	a.PollHookEvents() // start at the end: only new events matter
	kick := make(chan struct{}, 1)
	var wg sync.WaitGroup
	wg.Go(func() { a.refreshLoop(ctx, s.Refresh, kick) })
	wg.Go(func() { a.eventsLoop(ctx, s.Events, kick) })
	if s.Usage > 0 {
		wg.Go(func() { a.usageLoop(ctx, s.Usage) })
	}
	wg.Wait()
}

// sleep waits d, or a kick; false when ctx ended first.
func (a *App) sleep(ctx context.Context, d time.Duration, kick <-chan struct{}) bool {
	t := a.newTimer(d)
	defer t.Stop()
	select {
	case <-ctx.Done():
		return false
	case <-t.C():
	case <-kick:
	}
	return ctx.Err() == nil
}

func (a *App) refreshLoop(ctx context.Context, every time.Duration, kick <-chan struct{}) {
	for {
		a.RefreshNotices()
		if !a.sleep(ctx, every, kick) {
			return
		}
	}
}

func (a *App) eventsLoop(ctx context.Context, every time.Duration, kick chan<- struct{}) {
	for a.sleep(ctx, every, nil) {
		evs := a.PollHookEvents()
		notices, refresh := a.OnHookEvents(evs)
		a.publishPeerNotices(evs, notices)
		if refresh {
			select {
			case kick <- struct{}{}:
			default: // one already asked: coalesced
			}
		}
	}
}

// publishPeerNotices publishes the notices OnHookEvents made for the
// messages between sessions (its other notices went out already). They
// come one per peer event and one per event that needs the user, in the
// order of evs.
func (a *App) publishPeerNotices(evs []events.Event, notices []Notice) {
	i := 0
	for _, e := range evs {
		switch {
		case e.Type == "peer" && i < len(notices):
			a.Publish(EvNotice, notices[i])
			i++
		case e.NeedsYou():
			i++
		}
	}
}

func (a *App) usageLoop(ctx context.Context, every time.Duration) {
	for {
		a.OnUsage(a.FetchUsage())
		if !a.sleep(ctx, every, nil) {
			return
		}
	}
}
