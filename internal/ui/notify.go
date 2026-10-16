package ui

// Slice "notify": state transitions, hook events, desktop notifications and
// the /usage poll. app (app/watch.go) decides what to tell the user and
// sends the desktop notification; this slice polls and schedules (WP-5.1
// moves that to app later) and draws the notices: the cat meows, the
// terminal bell rings, toasts show.

import (
	"time"

	tea "charm.land/bubbletea/v2"

	"github.com/aqilputi/nekuroddo/internal/app"
	"github.com/aqilputi/nekuroddo/internal/claude"
	"github.com/aqilputi/nekuroddo/internal/core"
	"github.com/aqilputi/nekuroddo/internal/events"
)

// notice hands the new snapshot to app: a session that just became blocked
// or done makes the cat meow (and, away from the terminal, rings the bell).
func (m *Model) notice() tea.Cmd {
	m.app.SetFocused(m.focused)
	return m.showNotices(m.app.ObserveSessions(m.snap.Sessions))
}

// showNotices draws app notices: the cat's line, the bell, a toast.
func (m *Model) showNotices(ns []app.Notice) tea.Cmd {
	var cmds []tea.Cmd
	for _, n := range ns {
		cmds = m.showNotice(n, cmds)
	}
	return tea.Batch(cmds...)
}

func (m *Model) showNotice(n app.Notice, cmds []tea.Cmd) []tea.Cmd {
	m.cat.Meow(core.Truncate(n.Meow, 50))
	if n.Bell {
		cmds = append(cmds, tea.Raw("\a")) // the desktop notification went out from app
	}
	if n.Toast != "" {
		cmds = append(cmds, toastCmd(n.Toast, n.ToastErr))
	}
	return cmds
}

func (m *Model) pollEvents() tea.Cmd {
	tail := m.evTail
	return func() tea.Msg { return eventsMsg(tail.Poll()) }
}

func (m *Model) scheduleEvents() tea.Cmd {
	return tea.Tick(400*time.Millisecond, func(time.Time) tea.Msg { return eventsTickMsg{} })
}

// onEvents hands hook events to app: refresh now, and meow when one needs you.
func (m *Model) onEvents(evs []events.Event) tea.Cmd {
	m.app.SetFocused(m.focused)
	notices, refresh := m.app.OnHookEvents(evs)
	if !refresh {
		return nil
	}
	m.hooksSeen = true
	var cmds []tea.Cmd
	// app returns one notice per event that needs you, in order; peers are
	// drawn by the groups slice. Interleaving keeps the cat's last line.
	for _, e := range evs {
		m.events[e.SessionID] = e // view mirror of app.LastEvent
		switch {
		case e.Type == "peer":
			m.onPeer(e)
		case e.NeedsYou() && len(notices) > 0:
			cmds = m.showNotice(notices[0], cmds)
			notices = notices[1:]
		}
	}
	if m.refreshing {
		m.refreshAgain = true
	} else {
		cmds = append(cmds, m.refresh())
	}
	return tea.Batch(cmds...)
}

// fetchUsage runs `claude -p /usage` through app (local, ~2s, no tokens spent).
func (m *Model) fetchUsage(manual bool) tea.Cmd {
	if m.cfg.UsageRefresh <= 0 && !manual {
		return nil
	}
	a := m.app
	return func() tea.Msg {
		rep, err := a.FetchUsage()
		return usageMsg{rep, err, manual}
	}
}

func (m *Model) onUsage(msg usageMsg) tea.Cmd {
	var next tea.Cmd
	if !msg.manual && m.cfg.UsageRefresh > 0 {
		next = tea.Tick(time.Duration(m.cfg.UsageRefresh*float64(time.Second)), func(time.Time) tea.Msg { return usageTickMsg{} })
	}
	notices := m.app.OnUsage(msg.rep, msg.err)
	m.usage, m.usageErr = m.app.Usage()
	return tea.Batch(next, m.showNotices(notices))
}

// notifySlice is terminal focus and the view's mirrors of app's watch state
// (WP-2.7 drops the mirrors and reads app directly).
type notifySlice struct {
	focused bool // terminal focus (notifications only when away); handed to app.SetFocused

	usage    claude.UsageReport // mirror of app.Usage: 5h session and weekly limits
	usageErr error

	// Claude Code hook events (nk hook → events.jsonl)
	evTail       app.EventLog            // ports.Events
	events       map[string]events.Event // mirror of app.LastEvent
	hooksSeen    bool                    // mirror of app.HooksSeen
	refreshAgain bool
}

func newNotifySlice(ports app.Ports) notifySlice {
	return notifySlice{focused: true, evTail: ports.Events, events: map[string]events.Event{}}
}
