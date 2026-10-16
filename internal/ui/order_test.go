package ui

import (
	"testing"
	"time"

	"github.com/aqilputi/nekuroddo/internal/config"
	"github.com/aqilputi/nekuroddo/internal/core"
	"github.com/aqilputi/nekuroddo/internal/store"
)

func sessOrder(m *Model) []string {
	var out []string
	for _, r := range m.rows {
		if r.Kind == core.RowSess {
			out = append(out, r.Sess.SessionID)
		}
	}
	return out
}

// Activity must not reshuffle the tree; newcomers go on top; R re-sorts.
func TestOrderIsFrozen(t *testing.T) {
	now := time.Now()
	m := New(config.Default())
	m.snap = store.Snapshot{Sessions: []store.Session{
		{SessionID: "a", Context: "c", LastAt: now.Add(-1 * time.Minute)},
		{SessionID: "b", Context: "c", LastAt: now.Add(-2 * time.Minute)},
		{SessionID: "c", Context: "c", LastAt: now.Add(-3 * time.Minute),
			Subs: []store.Sub{{ID: "s1", Done: true}, {ID: "s2"}}},
	}}
	m.rebuild()
	if got := sessOrder(&m); got[0] != "a" || got[2] != "c" {
		t.Fatalf("initial order %v", got)
	}
	// c becomes the most active: order must not change
	m.snap.Sessions[2].LastAt = now
	m.rebuild()
	if got := sessOrder(&m); got[0] != "a" || got[1] != "b" || got[2] != "c" {
		t.Fatalf("order jumped: %v", got)
	}
	// finished subagent hidden, running one shown
	subs := 0
	for _, r := range m.rows {
		if r.Kind == core.RowSub {
			subs++
			if r.Sub.Done {
				t.Fatal("finished subagent shown")
			}
		}
	}
	if subs != 1 {
		t.Fatalf("want 1 running subagent row, got %d", subs)
	}
	// a new session lands on top without moving the others
	m.snap.Sessions = append(m.snap.Sessions, store.Session{SessionID: "d", Context: "c", LastAt: now.Add(-time.Hour)})
	m.rebuild()
	if got := sessOrder(&m); got[0] != "d" || got[1] != "a" || got[3] != "c" {
		t.Fatalf("newcomer placement %v", got)
	}
	// R: re-sort by activity
	m.keyNormal(keyPress("R"))
	if got := sessOrder(&m); got[0] != "c" {
		t.Fatalf("after R want c first, got %v", got)
	}
}
