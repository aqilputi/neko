package tui

import (
	"testing"

	"github.com/aqilputi/nekuroddo/internal/app"
	"github.com/aqilputi/nekuroddo/internal/config"
	"github.com/aqilputi/nekuroddo/internal/core"
	"github.com/aqilputi/nekuroddo/internal/store"
	"github.com/aqilputi/nekuroddo/internal/term"
)

// The right side follows the selection: the pane shows only on its own
// session; elsewhere (other sessions, todos) the details show and the pane
// keeps running hidden.
func TestRightSideFollowsSelection(t *testing.T) {
	m := newTestModel(config.Default())
	m.w, m.h = 120, 40
	setSnap(&m, store.Snapshot{Sessions: []store.Session{
		{SessionID: "a", ShortID: "aaaa1111", Context: "c", Kind: "background"},
		{SessionID: "b", ShortID: "bbbb2222", Context: "c", Kind: "background"},
	}})
	m.rebuild()
	p, err := term.Start("aaaa1111", "a", "/", 40, 10, "sleep", "5")
	if err != nil {
		t.Fatal(err)
	}
	defer p.Close()
	m.panes["aaaa1111"] = p
	m.app.Panes().Started(app.AttachPlan{SessionID: "a", ID: "aaaa1111"})

	sel := func(sid string) {
		for i, r := range m.rows {
			if r.Kind == core.RowSess && r.Sess.SessionID == sid {
				m.view.Cursor = i
			}
		}
		m.syncActive()
	}
	sel("a")
	if m.active != "aaaa1111" {
		t.Fatal("pane should show on its session")
	}
	sel("b")
	if m.active != "" {
		t.Fatal("another session should show its details")
	}
	if m.panes["aaaa1111"] == nil {
		t.Fatal("hidden pane must keep running")
	}
	sel("a")
	m.tab = tabTodos
	m.syncActive()
	if m.active != "" {
		t.Fatal("todos tab should show task details")
	}
	// INSERT pins the pane even when switching tabs
	m.tab = tabSessions
	sel("a")
	m.mode = modeInsert
	m.tab = tabTodos
	m.syncActive()
	if m.active != "aaaa1111" {
		t.Fatal("INSERT should keep the pane")
	}
}
