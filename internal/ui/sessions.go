package ui

// Slice "sessions": archive, stop, remove, rename and move sessions
// (WP-2.2 moves the logic to app/sessions.go).

import (
	"fmt"
	"sort"
	"strings"

	tea "charm.land/bubbletea/v2"

	"github.com/aqilputi/nekuroddo/internal/claude"
	"github.com/aqilputi/nekuroddo/internal/core"
)

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
		restore := m.tab == tabArchive
		verb := "arquivar"
		if restore {
			verb = "restaurar"
		}
		m.confirmThen(fmt.Sprintf("%s as %d sessões de \"%s\"?", verb, len(ids), r.Ctx), func(m *Model) tea.Cmd {
			if restore {
				m.store.Unarchive(ids...)
			} else {
				m.store.Archive(ids...)
			}
			return tea.Batch(toastCmd(fmt.Sprintf("%d sessões: %s ✓", len(ids), verb), false), m.refresh())
		})
		return nil
	}
	s := r.Sess
	if s.Archived {
		m.store.Unarchive(s.SessionID)
		s.Archived, s.Stale = false, false
		m.rebuild()
		return tea.Batch(toastCmd("restaurada", false), m.refresh())
	}
	m.store.Archive(s.SessionID)
	s.Archived, s.Stale = true, true
	m.rebuild()
	msg := "arquivada (aba 4 Arquivo) — volta sozinha se tiver atividade nova"
	if s.State == "busy" {
		msg = "arquivada, mas está rodando: volta assim que tiver atividade nova"
	}
	return tea.Batch(toastCmd(msg, false), m.refresh())
}

// archiveStale archives every stale session not archived yet.
func (m *Model) archiveStale() tea.Cmd {
	var ids []string
	for _, s := range m.snap.Sessions {
		if s.Stale && !s.Archived && s.State != "busy" {
			ids = append(ids, s.SessionID)
		}
	}
	if len(ids) == 0 {
		return toastCmd("nenhuma sessão parada para arquivar", false)
	}
	m.confirmThen(fmt.Sprintf("arquivar as %d sessões paradas há mais de %.0fd?", len(ids), m.cfg.StaleDays), func(m *Model) tea.Cmd {
		m.store.Archive(ids...)
		return tea.Batch(toastCmd(fmt.Sprintf("%d sessões arquivadas ✓", len(ids)), false), m.refresh())
	})
	return nil
}

// removeEnded runs `claude rm` on background chats whose conversation was
// ended: they take no more messages, but their process lingers.
func (m *Model) removeEnded() tea.Cmd {
	var ids []string
	for _, s := range m.snap.Sessions {
		if s.Ended && s.ShortID != "" {
			ids = append(ids, s.ShortID)
		}
	}
	if len(ids) == 0 {
		return toastCmd("nenhum chat encerrado com processo sobrando", false)
	}
	m.confirmThen(fmt.Sprintf("remover %d chats encerrados (claude rm)? a conversa já não aceita mensagens", len(ids)), func(m *Model) tea.Cmd {
		for _, id := range ids {
			m.closePane(id)
		}
		return func() tea.Msg {
			var failed []string
			for _, id := range ids {
				if err := claude.Remove(id); err != nil {
					failed = append(failed, id)
				}
			}
			if len(failed) > 0 {
				return toastMsg{text: "falhou: " + strings.Join(failed, " "), err: true}
			}
			return tickMsg{}
		}
	})
	return nil
}

func (m *Model) removeSelected() tea.Cmd {
	sel := m.selected()
	if sel == nil || sel.ShortID == "" {
		return toastCmd("só sessões em background podem ser removidas", true)
	}
	id, label := sel.ShortID, sel.Label()
	m.confirmThen("remover \""+core.Truncate(label, 50)+"\" (e o worktree, se seguro)?", func(m *Model) tea.Cmd {
		m.closePane(id)
		return func() tea.Msg {
			if err := claude.Remove(id); err != nil {
				return toastMsg{text: err.Error(), err: true}
			}
			return snapMsgCmd()
		}
	})
	return nil
}

// stop is s: stops the selected background session, asking first.
func (m *Model) stop() {
	if sel := m.selected(); sel != nil && sel.ShortID != "" {
		id, label := sel.ShortID, sel.Label()
		m.confirmThen("parar \""+core.Truncate(label, 50)+"\"? (a conversa fica salva)", func(*Model) tea.Cmd {
			return func() tea.Msg {
				if err := claude.Stop(id); err != nil {
					return toastMsg{text: err.Error(), err: true}
				}
				return snapMsgCmd()
			}
		})
	}
}

// snapMsgCmd is returned after a mutating CLI call to trigger a refresh.
func snapMsgCmd() tea.Msg { return tickMsg{} }

// submitRename applies the session name typed at R (empty: back to claude's).
func (m *Model) submitRename(val string) tea.Cmd {
	m.store.Rename(m.draftDir, val)
	return m.afterOverride("renomeada")
}

// submitMove applies the context typed at m (empty: back to the directory rule).
func (m *Model) submitMove(val string) tea.Cmd {
	m.store.SetContext(m.draftDir, val)
	if val == "" {
		return m.afterOverride("contexto volta à regra do diretório")
	}
	return m.afterOverride("movida para " + val)
}

// submitRenameCtx renames a section, keeping its color.
func (m *Model) submitRenameCtx(val string) tea.Cmd {
	if val == m.draftDir {
		return nil
	}
	m.store.RenameContext(m.draftDir, val)
	if m.view.CtxColor != nil {
		if c, ok := m.view.CtxColor[m.draftDir]; ok && val != "" {
			m.view.CtxColor[val] = c // keep the section's color
		}
	}
	return m.afterOverride("seção renomeada")
}

// afterOverride shows a name/context change right away, without waiting
// for the next refresh.
func (m *Model) afterOverride(msg string) tea.Cmd {
	m.store.Reapply(m.snap.Sessions)
	m.rebuild()
	return tea.Batch(toastCmd(msg, false), m.refresh())
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
// rename and move keep none of their own yet).
type sessionsSlice struct{}

func newSessionsSlice() sessionsSlice { return sessionsSlice{} }
