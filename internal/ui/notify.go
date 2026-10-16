package ui

// Slice "notify": state transitions, hook events, desktop notifications and
// the /usage poll. app (app/watch.go) decides what to tell the user, sends
// the desktop notification and keeps the state (last hook event per session,
// the /usage report, terminal focus); this slice polls and schedules (WP-5.1
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
	a := m.app
	return func() tea.Msg { return eventsMsg(a.PollHookEvents()) }
}

func (m *Model) scheduleEvents() tea.Cmd {
	return tea.Tick(400*time.Millisecond, func(time.Time) tea.Msg { return eventsTickMsg{} })
}

// onEvents hands hook events to app: refresh now, and draw what app says
// (a meow for each event that needs you and for each message between
// sessions, in order).
func (m *Model) onEvents(evs []events.Event) tea.Cmd {
	notices, refresh := m.app.OnHookEvents(evs)
	if !refresh {
		return nil
	}
	cmds := []tea.Cmd{m.showNotices(notices)}
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
	return tea.Batch(next, m.showNotices(m.app.OnUsage(msg.rep, msg.err)))
}

// usage is the core's last /usage report (zero until the first one).
func (m *Model) usage() claude.UsageReport {
	rep, _ := m.app.Usage()
	return rep
}

// notifySlice is what the slice keeps of its own: a refresh asked for while
// another ran (the core has the rest).
type notifySlice struct {
	refreshAgain bool
}

func newNotifySlice() notifySlice { return notifySlice{} }
