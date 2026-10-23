package tui

import (
	"strings"
	"testing"

	"github.com/aqilputi/nekuroddo/internal/core"
)

// worldOnlyTitles are the titles of the world's commands (nkw) that no
// other command has, so seeing one means a world command leaked.
func worldOnlyTitles(t *testing.T, m Model) []string {
	t.Helper()
	other := map[string]bool{}
	for _, short := range barLabels { // the bar's own words ("abre", "fecha") are the TUI's
		other[short] = true
	}
	world := map[string]bool{}
	for _, c := range m.app.Registry().All() {
		if core.IsWorldMode(c.Mode) {
			world[c.Title] = true
		} else {
			other[c.Title] = true
		}
	}
	if len(world) == 0 {
		t.Fatal("the core registers no world command")
	}
	var out []string
	for title := range world {
		if !other[title] {
			out = append(out, title)
		}
	}
	return out
}

// The TUI never shows the world's modes or commands: not in the palette,
// the help, the status bar, nor Lua's key column.
func TestTUIHidesTheWorld(t *testing.T) {
	m, _ := luaModel(t, core.Keymap{
		{Command: "world.right", Keys: []string{"n"}, Mode: core.ModeNormalWorld, Help: core.Help{Key: "n", Desc: "próxima estação"}},
		{Command: luaKeyPrefix + "world:world+a", Keys: []string{"world+a"}, Mode: core.ModeWorld, Help: core.Help{Key: "world+a", Desc: "lua"}},
		{Command: "tab.inbox", Keys: []string{"ctrl+y"}, Mode: core.ModeInsert, Help: core.Help{Key: "ctrl+y", Desc: "inbox"}},
	})
	titles := worldOnlyTitles(t, m)
	shown := map[string]bool{}
	for _, it := range m.paletteItems() {
		shown[it.title] = true
	}
	for _, col := range m.fullHelp() {
		for _, b := range col {
			shown[b.Help().Desc] = true
		}
	}
	for _, tab := range []tab{tabSessions, tabInbox, tabTodos, tabArchive} {
		m.tab = tab
		for _, b := range m.shortHelp() {
			shown[b.Help().Desc] = true
		}
	}
	for _, title := range titles {
		if shown[title] {
			t.Errorf("world command %q shows in the TUI", title)
		}
	}
	if got := helpPairs(m.luaBindings()); strings.Join(got, "|") != "ctrl+y=inbox [insert]" {
		t.Fatalf("lua bindings = %q", got)
	}
	m.tab = tabSessions
	m.push(helpDialog{})
	out := rhScreen(t, m)
	rhHasNot(t, out, "normal.world", "world.", "world+", "XF86", "estação")
	rhHas(t, out, "inbox [insert]")
}
