package ui

import (
	"path/filepath"
	"testing"

	tea "charm.land/bubbletea/v2"

	"github.com/aqilputi/nekuroddo/internal/config"
	"github.com/aqilputi/nekuroddo/internal/core"
	"github.com/aqilputi/nekuroddo/internal/store"
)

func TestArchiveTabAndContextArchive(t *testing.T) {
	t.Setenv("XDG_STATE_HOME", filepath.Join(t.TempDir(), "state"))
	t.Setenv("NEKO_CLAUDE", "/bin/false") // refresh must not touch the real claude
	m := newTestModel(config.Default())
	m.w, m.h = 120, 40
	setSnap(&m, store.Snapshot{Sessions: []store.Session{
		{SessionID: "a", Context: "ctx1"},
		{SessionID: "b", Context: "ctx1"},
		{SessionID: "c", Context: "ctx2", Archived: true, Stale: true},
	}})
	m.rebuild()
	if m.view.ArchivedN != 1 || len(sessOrder(&m)) != 2 {
		t.Fatalf("sessions tab should hide the archived one: %v", sessOrder(&m))
	}
	// X on the ctx1 row asks, then archives both
	m.view.Cursor = 0
	if m.rows[0].Kind != core.RowCtx {
		t.Fatal("first row should be a context")
	}
	m.keyNormal(keyPress("X"))
	if m.top() == nil {
		t.Fatal("archiving a whole context must ask")
	}
	m.key(tea.KeyPressMsg{Code: 'y', Text: "y"})
	// the store now has a and b archived; simulate the refresh result
	setSnap(&m, store.Snapshot{Sessions: []store.Session{
		{SessionID: "a", Context: "ctx1", Archived: true, Stale: true},
		{SessionID: "b", Context: "ctx1", Archived: true, Stale: true},
		{SessionID: "c", Context: "ctx2", Archived: true, Stale: true},
	}})
	m.keyNormal(keyPress("4"))
	m.rebuild()
	if got := sessOrder(&m); len(got) != 3 {
		t.Fatalf("archive tab should list all 3: %v", got)
	}
	// X on a session in the archive tab restores it
	for i, r := range m.rows {
		if r.Kind == core.RowSess && r.Sess.SessionID == "c" {
			m.view.Cursor = i
		}
	}
	m.keyNormal(keyPress("X"))
	if got := sessOrder(&m); len(got) != 2 {
		t.Fatalf("restored session should leave the archive tab: %v", got)
	}
}
