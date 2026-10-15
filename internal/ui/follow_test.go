package ui

import (
	"testing"

	"github.com/aqilputi/neko/internal/config"
	"github.com/aqilputi/neko/internal/store"
	"github.com/aqilputi/neko/internal/term"
)

// The right side follows the selection: the pane shows only on its own
// session; elsewhere (other sessions, todos) the details show and the pane
// keeps running hidden.
func TestRightSideFollowsSelection(t *testing.T) {
	m := New(config.Default())
	m.w, m.h = 120, 40
	m.snap = store.Snapshot{Sessions: []store.Session{
		{SessionID: "a", ShortID: "aaaa1111", Context: "c", Kind: "background"},
		{SessionID: "b", ShortID: "bbbb2222", Context: "c", Kind: "background"},
	}}
	m.rebuild()
	p, err := term.Start("aaaa1111", "a", "/", 40, 10, "sleep", "5")
	if err != nil {
		t.Fatal(err)
	}
	defer p.Close()
	m.panes["aaaa1111"], m.paneOf["a"] = p, "aaaa1111"

	sel := func(sid string) {
		for i, r := range m.rows {
			if r.kind == "sess" && r.sess.SessionID == sid {
				m.cursor = i
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
