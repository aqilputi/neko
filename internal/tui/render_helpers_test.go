package tui

import (
	"path/filepath"
	"strings"
	"testing"
	"time"

	"charm.land/lipgloss/v2"
	"github.com/charmbracelet/x/ansi"

	"github.com/aqilputi/nekuroddo/internal/claude"
	"github.com/aqilputi/nekuroddo/internal/config"
	"github.com/aqilputi/nekuroddo/internal/core"
	"github.com/aqilputi/nekuroddo/internal/store"
)

// rhEnv isolates every file nekuroddo might read or write.
func rhEnv(t *testing.T) {
	t.Helper()
	root := t.TempDir()
	t.Setenv("HOME", root)
	t.Setenv("XDG_STATE_HOME", filepath.Join(root, "state"))
	t.Setenv("XDG_CONFIG_HOME", filepath.Join(root, "config"))
	t.Setenv("XDG_CACHE_HOME", filepath.Join(root, "cache"))
	t.Setenv("NEKO_CLAUDE", "/bin/false")
	t.Setenv("NEKO_TODO_SCRIPT", "/nonexistent")
}

// rhModel is an empty 140×40 model with no data.
func rhModel(t *testing.T) Model {
	t.Helper()
	rhEnv(t)
	m := newTestModel(config.Default())
	m.w, m.h = 140, 40

	return m
}

// rhUsage is a /usage report: 40% of the 5h window, 75% of the week.
func rhUsage() claude.UsageReport {
	return claude.UsageReport{
		Limits: []claude.Limit{
			{Name: "Current session", Pct: 40, Resets: "Oct 15, 11:30am"},
			{Name: "Current week (all models)", Pct: 75, Resets: "Oct 21, 10pm"},
		},
		Raw: "Current session: 40% used\nCurrent week (all models): 75% used\n\nWhat's contributing to your usage\nlinha um\nlinha dois",
		At:  time.Now(),
	}
}

// rhSessions: two contexts, one of each kind of state that matters.
func rhSessions() []store.Session {
	now := time.Now()
	return []store.Session{
		{SessionID: "a", ShortID: "bg1", Name: "deploy do rc", Context: "alpha", Kind: "background", State: "busy",
			Cwd: "/srv/alpha", Model: "claude-opus-4-1", Tokens: 1234, Started: now.Add(-time.Hour), LastAt: now,
			WindowCost: 3, LastUser: "faz o deploy", LastText: "rodando os testes",
			Subs: []store.Sub{
				{ID: "s1", Description: "explorar código", Model: "claude-haiku-4-5-20251001"},
				{ID: "s2", Description: "revisar", Model: "claude-sonnet-4-5", Done: true},
			}},
		{SessionID: "b", ShortID: "bg2", Name: "migração", Context: "alpha", Kind: "background", State: "blocked",
			Attention: store.AttnBlocked, LastAt: now.Add(-2 * time.Minute), WindowCost: 1,
			Needs: "qual bucket usar?", Detail: "esperando decisão",
			Questions: []claude.Question{
				{Question: "Qual região?", Options: []claude.Option{{Label: "us-east1", Description: "mais barato"}, {Label: "sa-east1"}}},
				{Question: "Apagar o antigo?", Options: []claude.Option{{Label: "sim"}}},
			},
			SuggestedReply: "us-east1, sem apagar",
			Links: []claude.Child{
				{Kind: "pr", Title: "migra bucket", Href: "https://example.com/pr/1"},
				{Kind: "artifact", Title: "relatório", Href: "https://example.com/a"},
			}},
		{SessionID: "c", Name: "antiga", Context: "beta", Kind: "history", State: "history", LastAt: now.Add(-5 * time.Hour)},
		{SessionID: "d", Name: "esquecida", Context: "beta", Kind: "background", State: "done", Stale: true, LastAt: now.Add(-10 * 24 * time.Hour)},
		{SessionID: "e", Name: "terminal", Context: "beta", Kind: "interactive", State: "idle", PID: 42,
			Attention: store.AttnIdle, LastAt: now.Add(-3 * time.Hour)},
	}
}

// rhFilled is rhModel with the sessions above, the usage report and rows built.
func rhFilled(t *testing.T) Model {
	t.Helper()
	m := rhModel(t)
	setSnap(&m, store.Snapshot{Sessions: rhSessions(), At: time.Now()})
	m.app.OnUsage(rhUsage(), nil)
	m.rebuild()
	return m
}

func rhCursor(m *Model, id string) {
	for i, r := range m.rows {
		if r.Kind == core.RowSess && r.Sess.SessionID == id {
			m.view.Cursor = i
			return
		}
	}
}

func rhCursorCtx(m *Model, ctx string) {
	for i, r := range m.rows {
		if r.Kind == core.RowCtx && r.Ctx == ctx {
			m.view.Cursor = i
			return
		}
	}
}

// rhLines strips the styling and splits into lines.
func rhLines(s string) []string { return strings.Split(ansi.Strip(s), "\n") }

// rhFits fails when any line of s is wider than w cells.
func rhFits(t *testing.T, s string, w int) {
	t.Helper()
	for i, l := range strings.Split(s, "\n") {
		if lw := lipgloss.Width(l); lw > w {
			t.Fatalf("line %d is %d cells wide, max %d: %q", i, lw, w, ansi.Strip(l))
		}
	}
}

func rhHas(t *testing.T, s string, wants ...string) {
	t.Helper()
	plain := ansi.Strip(s)
	for _, w := range wants {
		if !strings.Contains(plain, w) {
			t.Fatalf("missing %q in:\n%s", w, plain)
		}
	}
}

func rhHasNot(t *testing.T, s string, nots ...string) {
	t.Helper()
	plain := ansi.Strip(s)
	for _, w := range nots {
		if strings.Contains(plain, w) {
			t.Fatalf("unexpected %q in:\n%s", w, plain)
		}
	}
}

// rhLineWith is the first stripped line containing sub ("" if none).
func rhLineWith(s, sub string) string {
	for _, l := range rhLines(s) {
		if strings.Contains(l, sub) {
			return l
		}
	}
	return ""
}

func rhStr(s string) *string { return &s }
func rhInt(i int) *int       { return &i }

func rhBase(m Model) lipgloss.Style { return lipgloss.NewStyle().Background(m.t().Bg) }
