package tui

// Slice "notify": state transitions, hook events, desktop notifications and
// the /usage report. app decides what to tell the user (app/watch.go), and
// its scheduler (app/scheduler.go) refreshes, polls the hook log and fetches
// /usage; this slice follows the bus and draws the notices: the cat meows,
// the terminal bell rings, toasts show.

import (
	tea "charm.land/bubbletea/v2"

	"github.com/aqilputi/nekuroddo/internal/app"
	"github.com/aqilputi/nekuroddo/internal/claude"
	"github.com/aqilputi/nekuroddo/internal/core"
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

// fetchUsage runs `claude -p /usage` through app now (u: local, ~2s, no
// tokens spent); the periodic fetch is the scheduler's.
func (m *Model) fetchUsage() tea.Cmd {
	a := m.app
	return func() tea.Msg {
		rep, err := a.FetchUsage()
		return usageMsg{rep, err}
	}
}

// onUsage hands the report u fetched to app; its warnings come back on the
// bus as notices, like the scheduler's.
func (m *Model) onUsage(msg usageMsg) tea.Cmd {
	m.app.OnUsage(msg.rep, msg.err)
	return nil
}

// usage is the core's last /usage report (zero until the first one).
func (m *Model) usage() claude.UsageReport {
	rep, _ := m.app.Usage()
	return rep
}

// notifySlice is what the slice keeps of its own: what starts the core's
// scheduler, until it ran (the core has the rest).
type notifySlice struct {
	startSchedule func()
}

func newNotifySlice() notifySlice { return notifySlice{} }
