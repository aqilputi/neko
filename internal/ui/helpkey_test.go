package ui

import (
	"strings"
	"testing"

	tea "charm.land/bubbletea/v2"
	"github.com/charmbracelet/x/ansi"

	"github.com/aqilputi/nekuroddo/internal/config"
)

// ctrl+/ opens the full key list however the terminal encodes it, and the
// status line always shows it, even on a narrow terminal.
func TestCtrlSlashHelp(t *testing.T) {
	for _, k := range []tea.KeyPressMsg{
		{Code: '/', Mod: tea.ModCtrl}, // kitty keyboard protocol
		{Code: '_', Mod: tea.ModCtrl}, // legacy 0x1f
		{Code: '?', Text: "?"},
	} {
		m := New(config.Default())
		m.w, m.h = 120, 40
		m.keyNormal(k)
		if _, ok := m.top().(helpDialog); !ok {
			t.Errorf("%q did not open the help", k.String())
		}
	}
	for _, w := range []int{140, 100, 80} {
		m := New(config.Default())
		m.w, m.h = w, 30
		line := ansi.Strip(m.status(m.t()))
		if !strings.Contains(line, "ctrl+/ todos os comandos") {
			t.Errorf("width %d: hint cut: %q", w, line)
		}
		if w >= 100 && !strings.Contains(line, "X arquiva") {
			t.Errorf("width %d: archive hint missing: %q", w, line)
		}
		t.Logf("%3d: %s", w, line)
	}
}

// The tab bar never wraps, however much it has to say.
func TestTabsOneLine(t *testing.T) {
	m := New(config.Default())
	m.w, m.h = 90, 30
	m.view.Hidden, m.view.ArchivedN, m.view.Filter = 123, 45, "um filtro bem comprido para forçar"
	if h := strings.Count(m.tabs(m.t()), "\n"); h != 0 {
		t.Fatalf("tab bar wrapped into %d lines", h+1)
	}
}
