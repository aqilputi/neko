package app_test

import (
	"errors"
	"fmt"
	"strings"
	"testing"

	"github.com/aqilputi/nekuroddo/internal/app"
	"github.com/aqilputi/nekuroddo/internal/app/apptest"
	"github.com/aqilputi/nekuroddo/internal/claude"
	"github.com/aqilputi/nekuroddo/internal/config"
	"github.com/aqilputi/nekuroddo/internal/core"
	"github.com/aqilputi/nekuroddo/internal/events"
	"github.com/aqilputi/nekuroddo/internal/store"
)

// watchDrain returns every event already published (Publish is synchronous).
func watchDrain(ch <-chan core.Event) []core.Event {
	var out []core.Event
	for {
		select {
		case e := <-ch:
			out = append(out, e)
		default:
			return out
		}
	}
}

func watchNamed(evs []core.Event, name string) []core.Event {
	var out []core.Event
	for _, e := range evs {
		if e.Name == name {
			out = append(out, e)
		}
	}
	return out
}

func watchSessions() []store.Session {
	return []store.Session{
		{SessionID: "a", Name: "alpha", Kind: "background", State: "busy"},
		{SessionID: "b", Name: "bravo", Kind: "interactive", State: "busy"},
		{SessionID: "c", Name: "charlie", Kind: "background", State: "busy"},
	}
}

func TestObserveSessionsFirstSnapshotOnlyRecords(t *testing.T) {
	a, f := newApp(t)
	ev := a.Subscribe(t.Context())
	ss := watchSessions()
	ss[0].State = "blocked"
	if n := a.ObserveSessions(ss); n != nil {
		t.Fatalf("first snapshot: %+v", n)
	}
	if got := watchDrain(ev); len(got) != 0 {
		t.Fatalf("first snapshot publishes nothing: %+v", got)
	}
	a.WaitNotifications()
	if len(f.Notify.Notes()) != 0 {
		t.Fatal("no desktop notification")
	}
}

func TestObserveSessionsTransitions(t *testing.T) {
	cases := []struct {
		name   string
		change func(ss []store.Session)
		meow   string // "" = no notice
		states int    // session.state events
	}{
		{"unchanged", func([]store.Session) {}, "", 0},
		{"busy → blocked", func(ss []store.Session) { ss[0].State = "blocked" }, "miau! alpha voltou", 1},
		{"busy → done", func(ss []store.Session) { ss[2].State = "done" }, "miau! charlie voltou", 1},
		{"busy → idle interactive", func(ss []store.Session) { ss[1].State = "idle" }, "miau! bravo voltou", 1},
		{"busy → idle background", func(ss []store.Session) { ss[0].State = "idle" }, "", 1},
		{"two return", func(ss []store.Session) { ss[0].State = "done"; ss[1].State = "blocked" }, "miau! 2 agents voltaram", 2},
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			a, f := newApp(t)
			a.SetFocused(false)
			a.ObserveSessions(watchSessions())
			ev := a.Subscribe(t.Context())
			ss := watchSessions()
			c.change(ss)
			got := a.ObserveSessions(ss)
			published := watchDrain(ev)
			if n := len(watchNamed(published, app.EvSessionState)); n != c.states {
				t.Fatalf("session.state events: %d want %d", n, c.states)
			}
			a.WaitNotifications()
			if c.meow == "" {
				if got != nil || len(f.Notify.Notes()) != 0 || len(watchNamed(published, app.EvNotice)) != 0 {
					t.Fatalf("no notice expected: %+v", got)
				}
				return
			}
			if len(got) != 1 || got[0].Meow != c.meow || !got[0].Bell || got[0].Toast != "" {
				t.Fatalf("notice = %+v", got)
			}
			if ns := watchNamed(published, app.EvNotice); len(ns) != 1 || ns[0].Data != got[0] {
				t.Fatalf("notice event: %+v", ns)
			}
			if notes := f.Notify.Notes(); len(notes) != 1 || notes[0] != (apptest.Note{Title: "nekuroddo", Body: c.meow}) {
				t.Fatalf("desktop: %+v", notes)
			}
		})
	}
}

func TestObserveSessionsStatePayload(t *testing.T) {
	a, _ := newApp(t)
	a.ObserveSessions(watchSessions())
	ev := a.Subscribe(t.Context())
	ss := watchSessions()
	ss[0].State = "blocked"
	a.ObserveSessions(ss)
	e := next(t, ev, app.EvSessionState)
	if e.Data != (app.SessionState{ID: "a", Prev: "busy", State: "blocked"}) {
		t.Fatalf("session.state = %+v", e.Data)
	}
	// a session seen for the first time later on only records its state
	ss = append(ss, store.Session{SessionID: "z", State: "blocked"})
	if n := a.ObserveSessions(ss); n != nil {
		t.Fatalf("new session: %+v", n)
	}
}

func TestObserveSessionsTruncatesLabel(t *testing.T) {
	a, _ := newApp(t)
	ss := watchSessions()
	ss[0].Name = strings.Repeat("x", 60)
	a.ObserveSessions(ss)
	ss[0].State = "blocked"
	n := a.ObserveSessions(ss)
	if len(n) != 1 || n[0].Meow != "miau! "+core.Truncate(ss[0].Name, 40)+" voltou" {
		t.Fatalf("label truncated to 40: %+v", n)
	}
}

func TestFocusedDoesNotNotify(t *testing.T) {
	a, f := newApp(t)
	if !a.Focused() {
		t.Fatal("focused by default")
	}
	a.ObserveSessions(watchSessions())
	ss := watchSessions()
	ss[0].State = "blocked"
	n := a.ObserveSessions(ss)
	a.WaitNotifications()
	if len(n) != 1 || n[0].Bell {
		t.Fatalf("focused: meow, no bell: %+v", n)
	}
	if len(f.Notify.Notes()) != 0 {
		t.Fatal("focused: no desktop notification")
	}
	a.SetFocused(false)
	if a.Focused() {
		t.Fatal("SetFocused")
	}
}

func TestNotifierErrorIsIgnored(t *testing.T) {
	a, f := newApp(t)
	f.Notify.Err = errors.New("no notify-send")
	a.SetFocused(false)
	n, _ := a.OnHookEvents([]events.Event{{Name: "Notification", SessionID: "a", Message: "oi"}})
	a.WaitNotifications()
	if len(n) != 1 || !n[0].Bell {
		t.Fatalf("still rings: %+v", n)
	}
}

func TestOnHookEventsEmpty(t *testing.T) {
	a, _ := newApp(t)
	n, refresh := a.OnHookEvents(nil)
	if n != nil || refresh || a.HooksSeen() {
		t.Fatal("no events: nothing")
	}
}

func TestOnHookEventsNeedsYou(t *testing.T) {
	a, f := newApp(t)
	ev := a.Subscribe(t.Context())
	n, refresh := a.OnHookEvents([]events.Event{
		{Name: "Notification", Type: "permission_prompt", SessionID: "a", Message: "pode rodar?"},
		{Name: "PostToolUse", SessionID: "b"},
	})
	if !refresh || !a.HooksSeen() {
		t.Fatal("events ask for a refresh")
	}
	if len(n) != 1 || n[0].Meow != "miau! pode rodar?" || n[0].Bell {
		t.Fatalf("notice = %+v", n)
	}
	if e, ok := a.LastEvent("a"); !ok || e.Message != "pode rodar?" {
		t.Fatal("last event per session")
	}
	if e, ok := a.LastEvent("b"); !ok || e.Name != "PostToolUse" {
		t.Fatal("every event is recorded")
	}
	if _, ok := a.LastEvent("zz"); ok {
		t.Fatal("unknown session")
	}
	got := watchDrain(ev)
	ns := watchNamed(got, app.EvSessionNotification)
	if len(ns) != 1 || ns[0].Data != (app.SessionNotification{ID: "a", Type: "permission_prompt", Message: "pode rodar?"}) {
		t.Fatalf("session.notification = %+v", ns)
	}
	if len(watchNamed(got, app.EvNotice)) != 1 {
		t.Fatal("notice published")
	}
	a.WaitNotifications()
	if len(f.Notify.Notes()) != 0 {
		t.Fatal("focused: no desktop notification")
	}

	// away, no message: generic line; the desktop gets the raw message
	a.SetFocused(false)
	n, _ = a.OnHookEvents([]events.Event{{Name: "Notification", SessionID: "b"}})
	a.WaitNotifications()
	if len(n) != 1 || n[0].Meow != "miau! uma sessão precisa de você" || !n[0].Bell {
		t.Fatalf("notice = %+v", n)
	}
	if notes := f.Notify.Notes(); len(notes) != 1 || notes[0] != (apptest.Note{Title: "nekuroddo", Body: ""}) {
		t.Fatalf("desktop = %+v", notes)
	}
}

func TestOnHookEventsPeer(t *testing.T) {
	a, f := newApp(t)
	f.Sessions.SetSnapshot(store.Snapshot{Sessions: []store.Session{
		{SessionID: "a", Name: "alpha"}, {SessionID: "b", Name: "bravo"}}})
	a.Refresh()
	a.SetFocused(false) // a peer message never rings, even away
	ev := a.Subscribe(t.Context())
	n, refresh := a.OnHookEvents([]events.Event{
		{Name: "UserPromptSubmit", Type: "peer", SessionID: "b", From: "alpha", Message: "oi"},
	})
	if len(n) != 1 || n[0] != (app.Notice{Meow: "✉ alpha → bravo"}) || !refresh {
		t.Fatalf("a peer message meows who told whom and refreshes: %+v", n)
	}
	if e, ok := a.LastEvent("b"); !ok || e.From != "alpha" {
		t.Fatal("recorded")
	}
	// OnPeer (app/groups.go) ran: ✉ on both ends, peer.message for every client
	if c, ok := a.MailCueOf("a"); !ok || !c.Sent {
		t.Fatal("sender cue")
	}
	evs := watchDrain(ev)
	pm := watchNamed(evs, app.EvPeerMessage)
	if len(pm) != 1 || pm[0].Data != (app.PeerMessage{To: "b", From: "alpha", FromID: "a", Text: "oi"}) {
		t.Fatalf("peer.message: %+v", evs)
	}
	if len(watchNamed(evs, app.EvNotice)) != 0 || len(f.Notify.Notes()) != 0 {
		t.Fatalf("no notice event, no desktop notification: %+v", evs)
	}
}

func TestNoNotifierPort(t *testing.T) {
	p := apptest.Ports()
	p.Notify = nil
	a := app.New(config.Default(), p)
	defer a.Close()
	a.SetFocused(false)
	n, _ := a.OnHookEvents([]events.Event{{Name: "Notification", SessionID: "a"}})
	a.WaitNotifications()
	if len(n) != 1 || !n[0].Bell {
		t.Fatalf("without a notifier the bell still rings: %+v", n)
	}
}

func usageAt(pct int) claude.UsageReport {
	return claude.ParseUsage(fmt.Sprintf("Current session: %d%% used · resets Oct 15, 11:30am (America/Sao_Paulo)\nCurrent week (all models): 10%% used", pct))
}

func TestOnUsageThresholds(t *testing.T) {
	a, f := newApp(t)
	steps := []struct {
		pct    int
		toast  string
		warned int
	}{
		{50, "", 0},
		{82, "janela de 5h em 82%", 80},
		{85, "", 80},
		{96, "janela de 5h em 96%", 95},
		{97, "", 95},
		{10, "", 0},
		{81, "janela de 5h em 81%", 80},
	}
	for _, s := range steps {
		ev := a.Subscribe(t.Context())
		n := a.OnUsage(usageAt(s.pct), nil)
		if s.toast == "" && n != nil {
			t.Errorf("%d%%: unexpected %+v", s.pct, n)
		}
		if s.toast != "" {
			if len(n) != 1 || !strings.Contains(n[0].Toast, s.toast) || !n[0].ToastErr || !strings.HasPrefix(n[0].Meow, fmt.Sprintf("miau! %d%% da janela de 5h (reseta ", s.pct)) || n[0].Bell {
				t.Errorf("%d%%: notice %+v", s.pct, n)
			}
		}
		if a.UsageWarned() != s.warned {
			t.Errorf("%d%%: warned %d want %d", s.pct, a.UsageWarned(), s.warned)
		}
		if len(watchNamed(watchDrain(ev), app.EvUsageUpdated)) != 1 {
			t.Errorf("%d%%: usage.updated", s.pct)
		}
	}
	if f.Sessions.Count("SetWindowStart") != len(steps) {
		t.Fatal("every report sets the window start")
	}
	rep, err := a.Usage()
	if err != nil || rep.Raw == "" {
		t.Fatal("the report is kept")
	}
}

func TestFetchUsageGoesThroughThePort(t *testing.T) {
	a, f := newApp(t)
	f.Claude.Usage = usageAt(12)
	rep, err := a.FetchUsage()
	if err != nil || rep.Raw != f.Claude.Usage.Raw || f.Claude.Count("FetchUsage") != 1 {
		t.Fatalf("FetchUsage = %+v, %v", rep, err)
	}
	if r, _ := a.Usage(); r.Raw != "" {
		t.Fatal("fetching does not store: OnUsage does")
	}
}

func TestOnUsageError(t *testing.T) {
	a, f := newApp(t)
	a.OnUsage(usageAt(40), nil)
	ev := a.Subscribe(t.Context())
	if n := a.OnUsage(claude.UsageReport{}, errors.New("boom")); n != nil {
		t.Fatalf("error: %+v", n)
	}
	rep, err := a.Usage()
	if err == nil || rep.Raw == "" {
		t.Fatal("an error keeps the last report")
	}
	if e := next(t, ev, app.EvUsageUpdated); e.Data != (app.UsageUpdated{Err: "boom"}) {
		t.Fatalf("usage.updated = %+v", e.Data)
	}
	if f.Sessions.Count("SetWindowStart") != 1 {
		t.Fatal("an error does not move the window")
	}
	// a report without a session limit warns about nothing
	if n := a.OnUsage(claude.ParseUsage("nada"), nil); n != nil {
		t.Fatal("no limits")
	}
	if _, err := a.Usage(); err != nil {
		t.Fatal("a good report clears the error")
	}
}

func TestPollHookEvents(t *testing.T) {
	a, f := newApp(t)
	f.Events.Push(events.Event{SessionID: "a", Name: "Stop"})
	if got := a.PollHookEvents(); len(got) != 1 || got[0].SessionID != "a" {
		t.Fatalf("poll: %+v", got)
	}
	if got := a.PollHookEvents(); len(got) != 0 {
		t.Fatalf("only new events: %+v", got)
	}
	p := apptest.Ports()
	p.Events = nil
	b := app.New(config.Default(), p)
	defer b.Close()
	if b.PollHookEvents() != nil {
		t.Fatal("no log: nothing")
	}
}
