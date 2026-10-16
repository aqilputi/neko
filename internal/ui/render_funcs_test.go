package ui

import (
	"strings"
	"testing"
	"time"

	"charm.land/lipgloss/v2"
	"github.com/charmbracelet/x/ansi"

	"github.com/aqilputi/nekuroddo/internal/store"
)

func TestRhAgo(t *testing.T) {
	now := time.Now()
	cases := map[string]time.Time{
		"":      {},
		"agora": now.Add(-10 * time.Second),
		"5m":    now.Add(-5*time.Minute - time.Second),
		"3h":    now.Add(-3*time.Hour - time.Second),
		"2d":    now.Add(-49 * time.Hour),
	}
	for want, at := range cases {
		if got := ago(at); got != want {
			t.Errorf("ago(%v) = %q, want %q", time.Since(at), got, want)
		}
	}
}

func TestRhStateIconAndLabel(t *testing.T) {
	m := rhModel(t)
	th := m.t()
	cases := []struct {
		s           store.Session
		icon, label string
	}{
		{store.Session{Archived: true, State: "busy"}, "⌫", "arquivada"},
		{store.Session{Ended: true}, "⊘", "encerrada"},
		{store.Session{Stale: true, State: "done"}, "z", "parada (done)"},
		{store.Session{Stale: true, State: "busy"}, "◐", "rodando"}, // a stale one that woke up is busy
		{store.Session{State: "busy"}, "◐", "rodando"},
		{store.Session{Attention: store.AttnBlocked}, "◆", "te esperando"},
		{store.Session{Attention: store.AttnBlocked, WaitingFor: "input needed"}, "◆", "te esperando · input needed"},
		{store.Session{Attention: store.AttnReturned, State: "done"}, "✓", "voltou"},
		{store.Session{Attention: store.AttnIdle, State: "idle"}, "❯", "no prompt"},
		{store.Session{State: "done"}, "✓", "concluída"},
		{store.Session{State: "stopped"}, "■", "parada"},
		{store.Session{State: "failed"}, "✗", "falhou"},
		{store.Session{Kind: "history", State: "history"}, "·", "histórico"},
		{store.Session{State: "idle"}, "○", "idle"},
	}
	for _, c := range cases {
		s := c.s
		if icon, col := stateIcon(th, &s, 0); icon != c.icon || col == nil {
			t.Errorf("stateIcon(%+v) = %q, want %q", s, icon, c.icon)
		}
		if got := stateLabel(&s); got != c.label {
			t.Errorf("stateLabel(%+v) = %q, want %q", s, got, c.label)
		}
	}
	// animation: the spinner turns, the blocked diamond blinks
	busy := store.Session{State: "busy"}
	seen := map[string]bool{}
	for f := 0; f < 4; f++ {
		i, _ := stateIcon(th, &busy, f)
		seen[i] = true
	}
	if len(seen) != 4 {
		t.Fatalf("spinner should have 4 frames, got %v", seen)
	}
	blocked := store.Session{Attention: store.AttnBlocked}
	if i, _ := stateIcon(th, &blocked, 5); i != "◇" {
		t.Fatalf("blocked blinks to ◇, got %q", i)
	}
}

func TestRhShortModel(t *testing.T) {
	cases := map[string]string{
		"claude-haiku-4-5-20251001": "haiku-4-5",
		"claude-opus-4-1":           "opus-4-1",
		"claude-sonnet-4-5":         "sonnet-4-5",
		"opus-20250101x":            "opus-20250101x", // not a date
		"":                          "?",
		"claude-":                   "?",
	}
	for in, want := range cases {
		if got := shortModel(in); got != want {
			t.Errorf("shortModel(%q) = %q, want %q", in, got, want)
		}
	}
}

func TestRhTruncateOrDashPctText(t *testing.T) {
	for _, c := range []struct {
		in   string
		n    int
		want string
	}{
		{"abc", 3, "abc"}, {"abcd", 3, "ab…"}, {"ação!", 4, "açã…"}, {"abc", 1, "…"}, {"abc", 0, "…"},
	} {
		if got := truncate(c.in, c.n); got != c.want {
			t.Errorf("truncate(%q,%d) = %q, want %q", c.in, c.n, got, c.want)
		}
	}
	if orDash("") != "—" || orDash("x") != "x" {
		t.Fatal("orDash")
	}
	for p, want := range map[float64]string{0: "", -1: "", 0.4: "<1%", 1: "1%", 12.6: "13%"} {
		if got := pctText(p); got != want {
			t.Errorf("pctText(%v) = %q, want %q", p, got, want)
		}
	}
}

func TestRhBars(t *testing.T) {
	m := rhModel(t)
	th := m.t()
	b := lipgloss.NewStyle()
	if costBar(th, 1, 1, 0, b) != "" || pctBar(th, 50, 0, b) != "" {
		t.Fatal("zero width: empty bar")
	}
	cases := []struct {
		bar        string
		full, rest int
	}{
		{costBar(th, 0, 10, 10, b), 0, 10},
		{costBar(th, 5, 10, 10, b), 5, 5},
		{costBar(th, 50, 10, 10, b), 10, 0},  // over the max: full, not longer
		{costBar(th, 0.01, 10, 10, b), 1, 9}, // anything spent shows
		{costBar(th, 3, 0, 10, b), 1, 9},     // no max: minimal mark
	}
	for i, c := range cases {
		plain := ansi.Strip(c.bar)
		if lipgloss.Width(c.bar) != 10 || strings.Count(plain, "█") != c.full || strings.Count(plain, "░") != c.rest {
			t.Errorf("costBar case %d: %q", i, plain)
		}
	}
	for _, p := range []float64{-5, 0, 1, 33, 50, 100, 250} {
		bar := pctBar(th, p, 8, b)
		if lipgloss.Width(bar) != 8 {
			t.Errorf("pctBar(%v) width %d", p, lipgloss.Width(bar))
		}
	}
	if got := ansi.Strip(pctBar(th, 50, 4, b)); got != "██░░" {
		t.Errorf("50%% of 4 cells: %q", got)
	}
	if got := ansi.Strip(pctBar(th, 1, 4, b)); !strings.HasPrefix(got, "▏") {
		t.Errorf("1%% still shows a sliver: %q", got)
	}
	if got := ansi.Strip(pctBar(th, 100, 4, b)); got != "████" {
		t.Errorf("full: %q", got)
	}
}

func TestRhGradientKeepsText(t *testing.T) {
	m := rhModel(t)
	g := gradient(lipgloss.NewStyle(), "ᓚᘏᗢ neko", m.t().Accent, m.t().Accent2)
	if ansi.Strip(g) != "ᓚᘏᗢ neko" {
		t.Fatalf("gradient changed the text: %q", ansi.Strip(g))
	}
	if ansi.Strip(gradient(lipgloss.NewStyle(), "x", m.t().Accent, m.t().Accent2)) != "x" {
		t.Fatal("one rune")
	}
}

func TestRhScroll(t *testing.T) {
	for _, c := range []struct{ cur, n, h, s, e int }{
		{0, 3, 10, 0, 3},
		{0, 20, 5, 0, 5},
		{10, 20, 5, 8, 13},
		{19, 20, 5, 15, 20},
	} {
		if s, e := scroll(c.cur, c.n, c.h); s != c.s || e != c.e {
			t.Errorf("scroll(%d,%d,%d) = %d,%d want %d,%d", c.cur, c.n, c.h, s, e, c.s, c.e)
		}
	}
}

func TestRhPanelTitle(t *testing.T) {
	m := rhModel(t)
	out := panel(m.t(), 30, 5, true, "título", "corpo")
	lines := rhLines(out)
	if len(lines) != 5 || !strings.HasPrefix(lines[0], "╭─ título ") || !strings.HasSuffix(lines[0], "╮") {
		t.Fatalf("panel: %q", lines)
	}
	rhFits(t, out, 30)
	// a title too long for the border is left out rather than overflowing
	out = panel(m.t(), 10, 3, false, strings.Repeat("t", 30), "")
	rhFits(t, out, 10)
}
