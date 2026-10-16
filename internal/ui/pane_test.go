package ui

import (
	"strings"
	"testing"
	"time"

	tea "charm.land/bubbletea/v2"
	"github.com/charmbracelet/x/ansi"

	"github.com/aqilputi/nekuroddo/internal/config"
	"github.com/aqilputi/nekuroddo/internal/term"
)

// The pane's cells must land inside the right panel's border.
func TestPaneDrawnInsideBorder(t *testing.T) {
	m := New(config.Default())
	var mm tea.Model = m
	mm, _ = mm.Update(tea.WindowSizeMsg{Width: 120, Height: 30})
	um := mm.(Model)
	r := um.rects().pane
	p, err := term.Start("t", "teste", "/", r.Dx(), r.Dy(), "bash", "--norc", "-c", `printf '\e[32mMIAU-DO-PAINEL\e[0m'; sleep 2`)
	if err != nil {
		t.Fatal(err)
	}
	defer p.Close()
	um.panes["t"], um.active = p, "t"
	deadline := time.Now().Add(3 * time.Second)
	for !strings.Contains(p.Render(), "MIAU") && time.Now().Before(deadline) {
		time.Sleep(20 * time.Millisecond)
	}
	lines := strings.Split(ansi.Strip(um.View().Content), "\n")
	row := lines[r.Min.Y]
	col := strings.Index(row, "MIAU-DO-PAINEL")
	if col < 0 {
		t.Fatalf("pane text not on row %d: %q", r.Min.Y, row)
	}
	if got := ansi.StringWidth(row[:col]); got != r.Min.X {
		t.Fatalf("pane text at column %d, want %d", got, r.Min.X)
	}
	if !strings.HasPrefix(strings.TrimLeft(lines[r.Min.Y-1][len(lines[r.Min.Y-1])-4:], " "), "╮") && !strings.Contains(lines[r.Min.Y-1], "╭") {
		t.Fatalf("no border above the pane: %q", lines[r.Min.Y-1])
	}
}
