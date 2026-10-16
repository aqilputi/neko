package ui

import (
	"path/filepath"
	"testing"

	tea "charm.land/bubbletea/v2"

	"github.com/aqilputi/nekuroddo/internal/config"
	"github.com/aqilputi/nekuroddo/internal/store"
)

func TestArchiveTabAndContextArchive(t *testing.T) {
	t.Setenv("XDG_STATE_HOME", filepath.Join(t.TempDir(), "state"))
	t.Setenv("NEKO_CLAUDE", "/bin/false") // refresh must not touch the real claude
	m := New(config.Default())
	m.w, m.h = 120, 40
	m.snap = store.Snapshot{Sessions: []store.Session{
		{SessionID: "a", Context: "ctx1"},
		{SessionID: "b", Context: "ctx1"},
		{SessionID: "c", Context: "ctx2", Archived: true, Stale: true},
	}}
	m.rebuild()
	if m.archivedN != 1 || len(sessOrder(&m)) != 2 {
		t.Fatalf("sessions tab should hide the archived one: %v", sessOrder(&m))
	}
	// X on the ctx1 row asks, then archives both
	m.cursor = 0
	if m.rows[0].kind != "ctx" {
		t.Fatal("first row should be a context")
	}
	m.keyNormal(keyPress("X"))
	if m.top() == nil {
		t.Fatal("archiving a whole context must ask")
	}
	m.key(tea.KeyPressMsg{Code: 'y', Text: "y"})
	// the store now has a and b archived; simulate the refresh result
	for i := range m.snap.Sessions {
		m.snap.Sessions[i].Archived, m.snap.Sessions[i].Stale = true, true
	}
	m.keyNormal(keyPress("4"))
	m.rebuild()
	if got := sessOrder(&m); len(got) != 3 {
		t.Fatalf("archive tab should list all 3: %v", got)
	}
	// X on a session in the archive tab restores it
	for i, r := range m.rows {
		if r.kind == "sess" && r.sess.SessionID == "c" {
			m.cursor = i
		}
	}
	m.keyNormal(keyPress("X"))
	if got := sessOrder(&m); len(got) != 2 {
		t.Fatalf("restored session should leave the archive tab: %v", got)
	}
}
