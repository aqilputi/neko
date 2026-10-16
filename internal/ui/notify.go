package ui

// Slice "notify": state transitions, hook events, desktop notifications
// and the /usage poll (WP-2.6 moves the logic to app/watch.go).

import (
	"fmt"
	"os/exec"
	"time"

	tea "charm.land/bubbletea/v2"

	"github.com/aqilputi/nekuroddo/internal/claude"
	"github.com/aqilputi/nekuroddo/internal/core"
	"github.com/aqilputi/nekuroddo/internal/events"
)

// notice transitions: a session that just became blocked or done makes the
// cat meow and rings the terminal bell.
func (m *Model) notice() tea.Cmd {
	var waiting []string
	for _, s := range m.snap.Sessions {
		prev, had := m.prevState[s.SessionID]
		m.prevState[s.SessionID] = s.State
		if !had || prev == s.State {
			continue
		}
		if s.State == "blocked" || s.State == "done" || (s.Kind == "interactive" && s.State == "idle" && prev == "busy") {
			waiting = append(waiting, s.Label())
		}
	}
	if len(waiting) == 0 {
		return nil
	}
	msg := "miau! " + core.Truncate(waiting[0], 40) + " voltou"
	if len(waiting) > 1 {
		msg = fmt.Sprintf("miau! %d agents voltaram", len(waiting))
	}
	m.cat.Meow(core.Truncate(msg, 50))
	if m.focused {
		return nil // you're looking at nekuroddo: the cat is enough
	}
	return tea.Batch(tea.Raw("\a"), notifyDesktop("nekuroddo", msg))
}

func notifyDesktop(title, body string) tea.Cmd {
	return func() tea.Msg {
		if p, err := exec.LookPath("notify-send"); err == nil {
			_ = exec.Command(p, "-a", "nekuroddo", title, body).Run()
		}
		return nil
	}
}

func (m *Model) pollEvents() tea.Cmd {
	tail := m.evTail
	return func() tea.Msg { return eventsMsg(tail.Poll()) }
}

func (m *Model) scheduleEvents() tea.Cmd {
	return tea.Tick(400*time.Millisecond, func(time.Time) tea.Msg { return eventsTickMsg{} })
}

// onEvents reacts to hook events: refresh now, and meow when one needs you.
func (m *Model) onEvents(evs []events.Event) tea.Cmd {
	if len(evs) == 0 {
		return nil
	}
	m.hooksSeen = true
	var cmds []tea.Cmd
	for _, e := range evs {
		m.events[e.SessionID] = e
		if e.Type == "peer" {
			m.onPeer(e)
			continue
		}
		if e.NeedsYou() {
			msg := "miau! " + e.Message
			if e.Message == "" {
				msg = "miau! uma sessão precisa de você"
			}
			m.cat.Meow(core.Truncate(msg, 50))
			if !m.focused {
				cmds = append(cmds, tea.Raw("\a"), notifyDesktop("nekuroddo", e.Message))
			}
		}
	}
	if m.refreshing {
		m.refreshAgain = true
	} else {
		cmds = append(cmds, m.refresh())
	}
	return tea.Batch(cmds...)
}

// fetchUsage runs `claude -p /usage` (local, ~2s, no tokens spent).
func (m *Model) fetchUsage(manual bool) tea.Cmd {
	if m.cfg.UsageRefresh <= 0 && !manual {
		return nil
	}
	return func() tea.Msg {
		rep, err := claude.FetchUsage()
		return usageMsg{rep, err, manual}
	}
}

func (m *Model) onUsage(msg usageMsg) tea.Cmd {
	var next tea.Cmd
	if !msg.manual && m.cfg.UsageRefresh > 0 {
		next = tea.Tick(time.Duration(m.cfg.UsageRefresh*float64(time.Second)), func(time.Time) tea.Msg { return usageTickMsg{} })
	}
	if msg.err != nil {
		m.usageErr = msg.err
		return next
	}
	m.usage, m.usageErr = msg.rep, nil
	m.store.SetWindowStart(m.usage.WindowStart())
	// the cat warns once per threshold of the 5-hour window
	if l, ok := m.usage.Find("session"); ok {
		for _, th := range []int{95, 80} {
			if l.Pct >= th && m.usageWarned < th {
				m.usageWarned = th
				m.cat.Meow(fmt.Sprintf("miau! %d%% da janela de 5h (reseta %s)", l.Pct, l.ShortReset()))
				return tea.Batch(next, toastCmd(fmt.Sprintf("janela de 5h em %d%% — reseta %s", l.Pct, l.ShortReset()), true))
			}
		}
		if l.Pct < 80 {
			m.usageWarned = 0 // new window
		}
	}
	return next
}
