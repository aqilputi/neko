package ui

import (
	"testing"

	"github.com/aqilputi/neko/internal/claude"
	"github.com/aqilputi/neko/internal/config"
	"github.com/aqilputi/neko/internal/store"
)

// The window's % is split by what each session consumed in the window.
func TestWindowShare(t *testing.T) {
	m := New(config.Default())
	m.usage = claude.ParseUsage("Current session: 12% used · resets Oct 15, 11:30am (America/Sao_Paulo)")
	m.snap = store.Snapshot{Sessions: []store.Session{
		{SessionID: "a", WindowCost: 3},
		{SessionID: "b", WindowCost: 1},
		{SessionID: "c"},
	}}
	if p := m.windowPct(&m.snap.Sessions[0]); p != 9 {
		t.Fatalf("a: %v want 9", p)
	}
	if p := m.windowPct(&m.snap.Sessions[1]); p != 3 {
		t.Fatalf("b: %v want 3", p)
	}
	if p := m.windowPct(&m.snap.Sessions[2]); p != 0 || pctText(p) != "" {
		t.Fatalf("idle session should show nothing: %v", p)
	}
	if pctText(0.4) != "<1%" {
		t.Fatal("tiny shares show <1%")
	}
}
