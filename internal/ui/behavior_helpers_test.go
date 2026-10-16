package ui

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	tea "charm.land/bubbletea/v2"

	"github.com/aqilputi/nekuroddo/internal/config"
	"github.com/aqilputi/nekuroddo/internal/core"
	"github.com/aqilputi/nekuroddo/internal/store"
	"github.com/aqilputi/nekuroddo/internal/term"
)

// bhEnv isolates a test from the real machine: state, config, cache and HOME
// live in temp dirs, claude is /bin/false, the todo skill does not exist and
// PATH has nothing on it (no notify-send, xdg-open or python3 runs for real).
func bhEnv(t *testing.T) {
	t.Helper()
	root := t.TempDir()
	for _, kv := range [][2]string{
		{"HOME", "home"}, {"XDG_STATE_HOME", "state"}, {"XDG_CONFIG_HOME", "config"},
		{"XDG_CACHE_HOME", "cache"}, {"PATH", "bin"},
	} {
		d := filepath.Join(root, kv[1])
		if err := os.MkdirAll(d, 0o755); err != nil {
			t.Fatal(err)
		}
		t.Setenv(kv[0], d)
	}
	t.Setenv("NEKO_CLAUDE", "/bin/false")
	t.Setenv("NEKO_TODO_SCRIPT", filepath.Join(root, "nope", "todo.py"))
	t.Setenv("CLAUDE_CONFIG_DIR", filepath.Join(root, "claude"))
}

// bhModelCfg builds a sized model over sessions. refreshing is left on so
// commands built from it never spawn a real store refresh.
func bhModelCfg(t *testing.T, cfg config.Config, sessions ...store.Session) Model {
	t.Helper()
	bhEnv(t)
	m := New(cfg)
	m.w, m.h = 140, 40
	m.refreshing = true
	m.snap = store.Snapshot{Sessions: sessions}
	m.rebuild()
	return m
}

func bhModel(t *testing.T, sessions ...store.Session) Model {
	t.Helper()
	cfg := config.Default()
	cfg.UsageRefresh = 0
	return bhModelCfg(t, cfg, sessions...)
}

// bhSessions: two contexts; "a" waits on you, "b" is busy with subagents,
// "c" returned, "d" is stale.
func bhSessions() []store.Session {
	now := time.Now()
	return []store.Session{
		{SessionID: "a", ShortID: "aaaa1111", Name: "alpha", Context: "c1", Kind: "background", State: "blocked",
			Attention: store.AttnBlocked, LastAt: now, Cwd: "/tmp"},
		{SessionID: "b", Name: "bravo", Context: "c1", Kind: "interactive", State: "busy", PID: 42, LastAt: now.Add(-time.Minute), Cwd: "/tmp",
			Subs: []store.Sub{{ID: "s1"}, {ID: "s2", Done: true}}},
		{SessionID: "c", Name: "charlie", Context: "c2", Kind: "history", State: "done",
			Attention: store.AttnReturned, LastAt: now.Add(-2 * time.Minute), Cwd: "/tmp"},
		{SessionID: "d", Name: "delta", Context: "c2", Kind: "history", State: "idle", Stale: true,
			LastAt: now.Add(-100 * time.Hour), Cwd: "/tmp"},
	}
}

// bhRun executes cmd, flattening batches, and returns what came back within
// a short wait (ticks longer than that are left behind).
func bhRun(cmd tea.Cmd) []tea.Msg {
	if cmd == nil {
		return nil
	}
	ch := make(chan tea.Msg, 1)
	go func() { ch <- cmd() }()
	var msg tea.Msg
	select {
	case msg = <-ch:
	case <-time.After(300 * time.Millisecond):
		return nil
	}
	if b, ok := msg.(tea.BatchMsg); ok {
		var out []tea.Msg
		for _, c := range b {
			out = append(out, bhRun(c)...)
		}
		return out
	}
	if msg == nil {
		return nil
	}
	return []tea.Msg{msg}
}

func bhToasts(cmd tea.Cmd) []toastMsg {
	var out []toastMsg
	for _, m := range bhRun(cmd) {
		if t, ok := m.(toastMsg); ok {
			out = append(out, t)
		}
	}
	return out
}

// bhHasToast reports whether cmd yields a toast containing sub.
func bhHasToast(cmd tea.Cmd, sub string) (toastMsg, bool) {
	for _, t := range bhToasts(cmd) {
		if strings.Contains(t.text, sub) {
			return t, true
		}
	}
	return toastMsg{}, false
}

func bhRune(r rune) tea.KeyPressMsg { return tea.KeyPressMsg{Code: r, Text: string(r)} }

// bhType types s into whatever has the keys.
func bhType(m *Model, s string) {
	for _, r := range s {
		m.key(bhRune(r))
	}
}

func bhEnter(m *Model) tea.Cmd { return m.key(tea.KeyPressMsg{Code: tea.KeyEnter}) }

// bhSelect puts the cursor on the session row with that id.
func bhSelect(t *testing.T, m *Model, id string) {
	t.Helper()
	for i, r := range m.rows {
		if r.Kind == core.RowSess && r.Sess.SessionID == id {
			m.view.Cursor = i
			return
		}
	}
	t.Fatalf("no row for session %s in %v", id, sessOrder(m))
}

// bhPane starts a harmless long-lived pane registered under id for session sid.
func bhPane(t *testing.T, m *Model, id, sid string) *term.Pane {
	t.Helper()
	p, err := term.Start(id, id, "/", 40, 10, "/bin/sleep", "30")
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(p.Close)
	m.panes[id] = p
	if sid != "" {
		m.paneOf[sid] = id
	}
	return p
}

func bhKinds(m *Model) []string {
	var out []string
	for _, r := range m.rows {
		out = append(out, core.RowKey(r))
	}
	return out
}
