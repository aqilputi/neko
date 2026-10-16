package ui

// Slice "sessions": archive, stop, remove, rename and move sessions. The use
// cases live in app (app/sessions.go); this file maps keys and prompts to
// m.app calls and their Results to tea.Cmds.

import (
	"sort"

	tea "charm.land/bubbletea/v2"

	"github.com/aqilputi/nekuroddo/internal/app"
	"github.com/aqilputi/nekuroddo/internal/core"
)

// syncApp hands the TUI's snapshot to the core, which the session commands
// read (and update in place). Until the TUI follows snapshot.changed (WP-2.7)
// the TUI refreshes on its own.
func (m *Model) syncApp() { m.app.UseSnapshot(m.snap) }

// toggleArchive is X: on a session it archives or restores it; on a context
// row it archives (or, in the archive tab, restores) every session listed
// under it, asking first.
func (m *Model) toggleArchive() tea.Cmd {
	r := m.cur()
	if m.tab == tabInbox {
		if sel := m.selected(); sel != nil {
			r = &core.Row{Kind: core.RowSess, Sess: sel}
		}
	}
	if r == nil {
		return nil
	}
	m.syncApp()
	if r.Kind == core.RowCtx {
		var ids []string
		for _, x := range m.rows {
			if x.Kind == core.RowSess && x.Ctx == r.Ctx {
				ids = append(ids, x.Sess.SessionID)
			}
		}
		if len(ids) == 0 {
			return nil
		}
		ctx := app.Context{Picked: ids}
		if m.tab == tabArchive {
			ctx.Tab = app.TabArchive
		}
		res, err := m.app.ToggleArchive(ctx, r.Ctx)
		return m.sessionResult(res, err, false)
	}
	res, err := m.app.ToggleArchive(app.Context{SessionID: r.Sess.SessionID}, "")
	m.rebuild()
	return m.sessionResult(res, err, false)
}

// archiveStale archives every stale session not archived yet, asking first.
func (m *Model) archiveStale() tea.Cmd {
	m.syncApp()
	res, err := m.app.ArchiveStale()
	return m.sessionResult(res, err, false)
}

// removeEnded runs `claude rm` on background chats whose conversation was
// ended, asking first; their panes close on yes.
func (m *Model) removeEnded() tea.Cmd {
	m.syncApp()
	res, err := m.app.RemoveEnded()
	return m.sessionResult(res, err, true)
}

// removeSelected is dd: removes the selected background session, asking first.
func (m *Model) removeSelected() tea.Cmd {
	m.syncApp()
	res, err := m.app.Remove(m.selectedCtx())
	return m.sessionResult(res, err, true)
}

// stop is x: stops the selected background session, asking first (any other
// session: nothing happens).
func (m *Model) stop() {
	m.syncApp()
	if res, err := m.app.Stop(m.selectedCtx()); err == nil && res.Pending != nil {
		m.askApp(res, true)
	}
}

// selectedCtx is the selection for a session command.
func (m *Model) selectedCtx() app.Context {
	if sel := m.selected(); sel != nil {
		return app.Context{SessionID: sel.SessionID}
	}
	return app.Context{}
}

// sessionResult turns a session command's Result into what the TUI does: a
// confirmation (whose parked action runs in a tea.Cmd when async, since it
// calls claude), a toast, a refresh.
func (m *Model) sessionResult(res app.Result, err error, async bool) tea.Cmd {
	if err != nil {
		return toastCmd(err.Error(), true)
	}
	if res.Pending != nil {
		m.askApp(res, async)
		return nil
	}
	var cmds []tea.Cmd
	if res.Toast != "" {
		cmds = append(cmds, toastCmd(res.Toast, res.Err))
	}
	if eff, _ := res.Data.(app.SessionEffect); eff.Refresh {
		cmds = append(cmds, m.refresh())
	}
	return tea.Batch(cmds...)
}

// askApp shows res's confirmation (like confirmToken, but yes first closes
// the panes the Result names, and a quick local action runs right away so
// its refresh goes through m.refresh); any other key cancels the token.
func (m *Model) askApp(res app.Result, async bool) {
	tok := res.Pending.Token
	eff, _ := res.Data.(app.SessionEffect)
	m.push(&confirmDialog{q: res.Pending.Question,
		yes: func(m *Model) tea.Cmd {
			for _, id := range eff.ClosePanes {
				m.closePane(id)
			}
			if !async {
				res, err := m.app.Confirm(tok)
				return m.sessionResult(res, err, false)
			}
			a := m.app
			return func() tea.Msg { return confirmedMsg(a.Confirm(tok)) }
		},
		no: func(m *Model) { _ = m.app.Cancel(tok) }, // already gone: nothing to do
	})
}

// confirmedMsg is the one message a parked action run off the UI goroutine
// comes back as: its toast, else a refresh (tickMsg).
func confirmedMsg(res app.Result, err error) tea.Msg {
	eff, _ := res.Data.(app.SessionEffect)
	switch {
	case err != nil:
		return toastMsg{text: err.Error(), err: true}
	case res.Toast != "":
		return toastMsg{text: res.Toast, err: res.Err}
	case eff.Refresh:
		return tickMsg{}
	}
	return nil
}

// submitRename applies the session name typed at R (empty: back to claude's).
func (m *Model) submitRename(val string) tea.Cmd {
	m.syncApp()
	res, err := m.app.Rename(m.draftDir, val)
	m.rebuild()
	return m.sessionResult(res, err, false)
}

// submitMove applies the context typed at m (empty: back to the directory rule).
func (m *Model) submitMove(val string) tea.Cmd {
	m.syncApp()
	res, err := m.app.Move(m.draftDir, val)
	m.rebuild()
	return m.sessionResult(res, err, false)
}

// submitRenameCtx renames a section, keeping its color.
func (m *Model) submitRenameCtx(val string) tea.Cmd {
	if val == m.draftDir {
		return nil
	}
	if m.view.CtxColor != nil {
		if c, ok := m.view.CtxColor[m.draftDir]; ok && val != "" {
			m.view.CtxColor[val] = c // keep the section's color
		}
	}
	m.syncApp()
	res, err := m.app.RenameContext(m.draftDir, val)
	m.rebuild()
	return m.sessionResult(res, err, false)
}

// contextNames lists the contexts in the current snapshot, for the move prompt.
func (m *Model) contextNames() []string {
	seen := map[string]bool{}
	var out []string
	for _, s := range m.snap.Sessions {
		if !seen[s.Context] {
			seen[s.Context] = true
			out = append(out, s.Context)
		}
	}
	sort.Strings(out)
	return out
}

// sessionsSlice is the sessions slice's state (archive, stop, remove,
// rename and move keep none of their own: the core has it).
type sessionsSlice struct{}

func newSessionsSlice() sessionsSlice { return sessionsSlice{} }
