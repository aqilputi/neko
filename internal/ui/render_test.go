package ui

import (
	"fmt"
	"os"
	"strings"
	"testing"

	tea "charm.land/bubbletea/v2"
	"charm.land/lipgloss/v2"
	"github.com/charmbracelet/x/ansi"

	"github.com/aqilputi/nekuroddo/internal/config"
)

// TestRenderSnapshot renders one frame with real data (NEKO_RENDER=1) to
// check the layout fits the terminal exactly.
func TestRenderSnapshot(t *testing.T) {
	if os.Getenv("NEKO_RENDER") == "" {
		t.Skip("set NEKO_RENDER=1")
	}
	cfg, _ := config.Load()
	m := New(cfg)
	var mm tea.Model = m
	mm, _ = mm.Update(tea.WindowSizeMsg{Width: 140, Height: 40})
	mm, _ = mm.Update(m.fetchUsage(true)()) // window start before the refresh
	snap, notices := m.app.RefreshNotices()
	mm, _ = mm.Update(snapMsg{snap: snap, notices: notices})
	if c := m.loadTodos(); c != nil {
		mm, _ = mm.Update(c())
	}
	for _, k := range strings.Split(os.Getenv("NEKO_KEYS"), ",") {
		if k == "" {
			continue
		}
		mm, _ = mm.Update(tea.KeyPressMsg{Code: rune(k[0]), Text: k})
	}
	out := mm.View().Content
	lines := strings.Split(out, "\n")
	bad := 0
	for i, l := range lines {
		if w := lipgloss.Width(l); w != 140 {
			bad++
			if bad < 5 {
				fmt.Printf("line %d width %d\n", i, w)
			}
		}
	}
	fmt.Printf("lines=%d badwidth=%d\n", len(lines), bad)
	os.WriteFile(os.Getenv("NEKO_OUT"), []byte(out), 0o644)
}

// TestRenderUsage renders the header and the u dialog with the real
// `claude -p /usage` (NEKO_RENDER=1).
func TestRenderUsage(t *testing.T) {
	if os.Getenv("NEKO_RENDER") == "" {
		t.Skip("set NEKO_RENDER=1")
	}
	cfg, _ := config.Load()
	m := New(cfg)
	var mm tea.Model = m
	mm, _ = mm.Update(tea.WindowSizeMsg{Width: 140, Height: 40})
	mm, _ = mm.Update(m.fetchUsage(true)())
	mm, _ = mm.Update(tea.KeyPressMsg{Code: 'u', Text: "u"})
	out := ansi.Strip(mm.View().Content)
	lines := strings.Split(out, "\n")
	fmt.Println(lines[0])
	for _, l := range lines[3:24] {
		fmt.Println(l)
	}
}
