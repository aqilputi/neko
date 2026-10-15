package ui

import (
	"path/filepath"
	"testing"

	tea "charm.land/bubbletea/v2"

	"github.com/aqilputi/neko/internal/config"
	"github.com/aqilputi/neko/internal/store"
)

func submitLine(m *Model, s string) {
	m.input.SetValue(s)
	m.key(tea.KeyPressMsg{Code: tea.KeyEnter})
}

func TestRenameAndMove(t *testing.T) {
	t.Setenv("XDG_STATE_HOME", filepath.Join(t.TempDir(), "state"))
	t.Setenv("NEKO_CLAUDE", "/bin/false")
	cfg := config.Default()
	m := New(cfg)
	m.w, m.h = 120, 40
	ctx := cfg.ContextFor("/tmp")
	m.snap = store.Snapshot{Sessions: []store.Session{
		{SessionID: "a", Name: "a", Cwd: "/tmp", Context: ctx},
		{SessionID: "b", Name: "b", Cwd: "/tmp", Context: ctx},
	}}
	m.rebuild()
	pick := func(id string) {
		for i, r := range m.rows {
			if r.kind == "sess" && r.sess.SessionID == id {
				m.cursor = i
				return
			}
		}
		t.Fatalf("no row for %s", id)
	}

	// n on a session renames it, prefilled with its current name
	pick("a")
	m.keyNormal(keyPress("n"))
	if m.mode != modeInput || m.input.Value() != "a" {
		t.Fatalf("rename prompt: mode=%v value=%q", m.mode, m.input.Value())
	}
	submitLine(&m, "investigação X")
	pick("a")
	if got := m.selected().Label(); got != "investigação X" {
		t.Fatalf("label after rename: %q", got)
	}

	// m moves b to another context
	pick("b")
	m.keyNormal(keyPress("m"))
	submitLine(&m, "visio")
	pick("b")
	if got := m.selected().Context; got != "visio" {
		t.Fatalf("context after move: %q", got)
	}

	// n on a section header renames the section
	for i, r := range m.rows {
		if r.kind == "ctx" && r.ctx == ctx {
			m.cursor = i
		}
	}
	m.keyNormal(keyPress("n"))
	submitLine(&m, "pesquisa")
	pick("a")
	if got := m.selected().Context; got != "pesquisa" {
		t.Fatalf("section rename: %q", got)
	}
	pick("b")
	if got := m.selected().Context; got != "visio" {
		t.Fatalf("other section must not change: %q", got)
	}
}
