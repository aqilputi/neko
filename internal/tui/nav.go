package tui

// Navigation in the tree, inbox and todos: cursor moves, folding and
// jumping to whoever needs you. Client-only state (core.ViewState).

import (
	"github.com/aqilputi/nekuroddo/internal/core"
	"github.com/aqilputi/nekuroddo/internal/store"
)

func (m *Model) listHeight() int {
	_, _, ph := m.layout()
	return max(ph-4, 1)
}

func (m *Model) move(d int) {
	switch m.tab {
	case tabSessions, tabArchive:
		m.view.Cursor = core.Clamp(m.view.Cursor+d, 0, len(m.rows)-1)
	case tabInbox:
		m.view.InboxCursor = core.Clamp(m.view.InboxCursor+d, 0, len(m.view.Inbox)-1)
	case tabTodos:
		m.todoCur = core.Clamp(m.todoCur+d, 0, len(m.todos)-1)
	}
}

func (m *Model) collapse() {
	r := m.cur()
	if r == nil || (m.tab != tabSessions && m.tab != tabArchive) {
		return
	}
	switch r.Kind {
	case core.RowCtx:
		m.view.Collapsed["ctx:"+r.Ctx] = true
	case core.RowSess:
		if m.visibleSubs(r.Sess) > 0 && !m.view.Collapsed["sess:"+r.Sess.SessionID] {
			m.view.Collapsed["sess:"+r.Sess.SessionID] = true
		} else {
			m.view.Collapsed["ctx:"+r.Ctx] = true
		}
	case core.RowSub:
		// r points into m.rows, which rebuild rewrites: keep the id first
		id := r.Sess.SessionID
		m.view.Collapsed["sess:"+id] = true
		m.rebuild()
		for i, x := range m.rows {
			if x.Kind == core.RowSess && x.Sess.SessionID == id {
				m.view.Cursor = i
			}
		}
		return
	}
	m.rebuild()
}

func (m *Model) expand() {
	r := m.cur()
	if r == nil || (m.tab != tabSessions && m.tab != tabArchive) {
		return
	}
	switch r.Kind {
	case core.RowCtx:
		delete(m.view.Collapsed, "ctx:"+r.Ctx)
	case core.RowSess:
		delete(m.view.Collapsed, "sess:"+r.Sess.SessionID)
	}
	m.rebuild()
}

func (m *Model) toggle() {
	r := m.cur()
	if r == nil {
		return
	}
	key := "ctx:" + r.Ctx
	if r.Kind != core.RowCtx {
		key = "sess:" + r.Sess.SessionID
	}
	m.view.Collapsed[key] = !m.view.Collapsed[key]
	m.rebuild()
}

func (m *Model) jumpAttention(dir int) {
	if m.tab == tabInbox {
		m.move(dir)
		return
	}
	m.tab = tabSessions
	n := len(m.rows)
	for i := 1; i <= n; i++ {
		j := ((m.view.Cursor+dir*i)%n + n) % n
		if r := m.rows[j]; r.Kind == core.RowSess && r.Sess.Attention != store.AttnNone {
			m.view.Cursor = j
			return
		}
	}
}

// visibleSubs counts the subagent rows a session shows (finished ones hide
// unless H).
func (m *Model) visibleSubs(s *store.Session) int {
	n := 0
	for _, sa := range s.Subs {
		if !sa.Done || m.view.ShowAll {
			n++
		}
	}
	return n
}
