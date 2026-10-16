package app

// Slice "watch": session state transitions, hook events, desktop
// notifications and the /usage report. The client polls and schedules
// (WP-5.1 moves that here later); app decides what is worth telling the user
// and publishes it, and the client only draws it (the cat, the bell, toasts).

import (
	"fmt"
	"sync"
	"time"

	"github.com/aqilputi/nekuroddo/internal/claude"
	"github.com/aqilputi/nekuroddo/internal/core"
	"github.com/aqilputi/nekuroddo/internal/events"
	"github.com/aqilputi/nekuroddo/internal/store"
)

// Event names of the watch slice (plan §2.5).
const (
	EvSessionState        = "session.state"
	EvSessionNotification = "session.notification"
	EvNotice              = "notice"
	EvUsageUpdated        = "usage.updated"
)

// SessionState is the payload of session.state: a session changed state
// between two snapshots.
type SessionState struct {
	ID    string `json:"id"`
	Prev  string `json:"prev"`
	State string `json:"state"`
}

// SessionNotification is the payload of session.notification: a hook says a
// session is waiting on the user.
type SessionNotification struct {
	ID      string `json:"id"`
	Type    string `json:"type,omitempty"`
	Message string `json:"message,omitempty"`
}

// UsageUpdated is the payload of usage.updated; clients read the report
// with App.Usage.
type UsageUpdated struct {
	At  time.Time `json:"at,omitzero"`
	Err string    `json:"err,omitempty"`
}

// Notice is something the user should hear about: the cat says Meow, the
// terminal rings when Bell (the user is away; a desktop notification went
// out too), and a client shows Toast when set. It is returned to the caller
// and published as notice for every other client.
type Notice struct {
	Meow     string `json:"meow"`
	Bell     bool   `json:"bell,omitempty"`
	Toast    string `json:"toast,omitempty"`
	ToastErr bool   `json:"toast_err,omitempty"`
}

// watchState is the slice's state. App has no field for it (app.go belongs
// to wave 1), so it hangs off a side table keyed by *App; WP-2.7 may move it
// into App.
type watchState struct {
	mu        sync.Mutex
	unfocused bool // zero value: focused
	prev      map[string]string
	last      map[string]events.Event
	hooksSeen bool

	usage       claude.UsageReport
	usageErr    error
	usageWarned int // highest session % threshold already announced

	notifying sync.WaitGroup
}

var watches sync.Map // *App → *watchState

func (a *App) watch() *watchState {
	if w, ok := watches.Load(a); ok {
		return w.(*watchState)
	}
	w, _ := watches.LoadOrStore(a, &watchState{prev: map[string]string{}, last: map[string]events.Event{}})
	return w.(*watchState)
}

// SetFocused tells app whether the user is looking at a client (terminal
// focus): desktop notifications and the bell are only for when they are not.
func (a *App) SetFocused(focused bool) {
	w := a.watch()
	w.mu.Lock()
	w.unfocused = !focused
	w.mu.Unlock()
}

// Focused reports the last SetFocused (true until told otherwise).
func (a *App) Focused() bool {
	w := a.watch()
	w.mu.Lock()
	defer w.mu.Unlock()
	return !w.unfocused
}

// ObserveSessions compares sessions with the states seen last time: every
// change publishes session.state, and the sessions that came back to the
// user (blocked, done, or an interactive one idle after busy) make one
// aggregated notice. A session seen for the first time only records its
// state.
func (a *App) ObserveSessions(sessions []store.Session) []Notice {
	w := a.watch()
	var changes []SessionState
	var back []string
	w.mu.Lock()
	for _, s := range sessions {
		prev, had := w.prev[s.SessionID]
		w.prev[s.SessionID] = s.State
		if !had || prev == s.State {
			continue
		}
		changes = append(changes, SessionState{ID: s.SessionID, Prev: prev, State: s.State})
		if s.State == "blocked" || s.State == "done" || (s.Kind == "interactive" && s.State == "idle" && prev == "busy") {
			back = append(back, s.Label())
		}
	}
	w.mu.Unlock()
	for _, c := range changes {
		a.Publish(EvSessionState, c)
	}
	if len(back) == 0 {
		return nil
	}
	msg := "miau! " + core.Truncate(back[0], 40) + " voltou"
	if len(back) > 1 {
		msg = fmt.Sprintf("miau! %d agents voltaram", len(back))
	}
	return []Notice{a.notice(msg, msg)}
}

// notice builds and publishes a notice; away from the client it also sends
// a desktop notification with body.
func (a *App) notice(meow, body string) Notice {
	n := Notice{Meow: meow, Bell: !a.Focused()}
	if n.Bell {
		a.notifyDesktop(body)
	}
	a.Publish(EvNotice, n)
	return n
}

// notifyDesktop runs the Notifier off the caller's goroutine (notify-send is
// a process); a failure (no notify-send) is not worth telling anyone.
func (a *App) notifyDesktop(body string) {
	if a.ports.Notify == nil {
		return
	}
	w := a.watch()
	w.notifying.Add(1)
	go func() {
		defer w.notifying.Done()
		_ = a.ports.Notify.Notify("nekuroddo", body)
	}()
}

// WaitNotifications waits for the desktop notifications in flight.
func (a *App) WaitNotifications() { a.watch().notifying.Wait() }

// OnHookEvents takes the events the hook log recorded since the last poll:
// it keeps the last one per session (LastEvent), publishes
// session.notification and a notice for each one that needs the user.
// Messages between sessions (Type "peer") are only recorded here: the
// client hands each to OnPeer (app/groups.go), which resolves the sender and
// publishes peer.message. refresh says the sessions
// changed and should be re-read now; the client coalesces it with a refresh
// already in flight.
func (a *App) OnHookEvents(evs []events.Event) (notices []Notice, refresh bool) {
	if len(evs) == 0 {
		return nil, false
	}
	w := a.watch()
	w.mu.Lock()
	w.hooksSeen = true
	for _, e := range evs {
		w.last[e.SessionID] = e
	}
	w.mu.Unlock()
	for _, e := range evs {
		if e.Type == "peer" || !e.NeedsYou() {
			continue
		}
		a.Publish(EvSessionNotification, SessionNotification{ID: e.SessionID, Type: e.Type, Message: e.Message})
		msg := "miau! " + e.Message
		if e.Message == "" {
			msg = "miau! uma sessão precisa de você"
		}
		notices = append(notices, a.notice(msg, e.Message))
	}
	return notices, true
}

// LastEvent is the last hook event seen for a session.
func (a *App) LastEvent(sessionID string) (events.Event, bool) {
	w := a.watch()
	w.mu.Lock()
	defer w.mu.Unlock()
	e, ok := w.last[sessionID]
	return e, ok
}

// HooksSeen reports whether any hook event ever arrived (the hook is set up).
func (a *App) HooksSeen() bool {
	w := a.watch()
	w.mu.Lock()
	defer w.mu.Unlock()
	return w.hooksSeen
}

// FetchUsage runs `claude -p /usage` (local, ~2s, no tokens spent). It only
// fetches: hand the result to OnUsage. Call it off the UI goroutine.
func (a *App) FetchUsage() (claude.UsageReport, error) { return a.ports.Claude.FetchUsage() }

// OnUsage takes a /usage report (or the error fetching it): it keeps it
// (Usage), moves the session store's 5-hour window and warns once per
// threshold (80%, 95%) of the window, starting over when it drops under 80%.
// It publishes usage.updated.
func (a *App) OnUsage(rep claude.UsageReport, err error) []Notice {
	w := a.watch()
	if err != nil {
		w.mu.Lock()
		w.usageErr = err
		w.mu.Unlock()
		a.Publish(EvUsageUpdated, UsageUpdated{Err: err.Error()})
		return nil
	}
	a.ports.Sessions.SetWindowStart(rep.WindowStart())
	var warn *claude.Limit
	w.mu.Lock()
	w.usage, w.usageErr = rep, nil
	if l, ok := rep.Find("session"); ok {
		for _, th := range []int{95, 80} {
			if l.Pct >= th && w.usageWarned < th {
				w.usageWarned = th
				warn = &l
				break
			}
		}
		if warn == nil && l.Pct < 80 {
			w.usageWarned = 0 // new window
		}
	}
	w.mu.Unlock()
	a.Publish(EvUsageUpdated, UsageUpdated{At: rep.At})
	if warn == nil {
		return nil
	}
	n := Notice{
		Meow:     fmt.Sprintf("miau! %d%% da janela de 5h (reseta %s)", warn.Pct, warn.ShortReset()),
		Toast:    fmt.Sprintf("janela de 5h em %d%% — reseta %s", warn.Pct, warn.ShortReset()),
		ToastErr: true,
	}
	a.Publish(EvNotice, n)
	return []Notice{n}
}

// Usage is the last /usage report and the error of the last fetch.
func (a *App) Usage() (claude.UsageReport, error) {
	w := a.watch()
	w.mu.Lock()
	defer w.mu.Unlock()
	return w.usage, w.usageErr
}

// UsageWarned is the highest threshold of the 5-hour window already
// announced (0, 80 or 95).
func (a *App) UsageWarned() int {
	w := a.watch()
	w.mu.Lock()
	defer w.mu.Unlock()
	return w.usageWarned
}
