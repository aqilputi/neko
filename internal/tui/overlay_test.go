package tui

import (
	"testing"

	"github.com/aqilputi/nekuroddo/internal/core"
)

// The TUI resolves Lua's overrides first and then the core's keymap
// (keyResolver); remote clients resolve the served table GET /v1/keymap
// gives them (core.Keymap.Overlay). Both answer the same in every mode
// except two cases the served table cannot say and the TUI never meets
// (it resolves no world mode): an override in a parent mode does not take
// a key a sub-mode binds itself (a TUI rebind of j leaves NAV's j alone),
// and a key deleted in a sub-mode falls back to its parent's binding.
func TestKeyResolverMatchesOverlay(t *testing.T) {
	over := core.Keymap{
		{Command: "nav.top", Keys: []string{"j"}, Mode: core.ModeNormal},                                    // a rebind over a default
		{Command: luaKeyPrefix + "normal:z", Keys: []string{"z"}, Mode: core.ModeNormal},                    // a new key, a function
		{Keys: []string{"X"}, Mode: core.ModeNormal},                                                        // a deleted default
		{Command: "session.archive", Keys: []string{"ctrl+y"}, Mode: core.ModeAny},                          // a new "*" key
		{Command: luaKeyPrefix + "normal.todos:enter", Keys: []string{"enter"}, Mode: core.ModeNormalTodos}, // a sub-mode default
		{Command: "world.right", Keys: []string{"n"}, Mode: core.ModeNormalWorld},
		{Command: "world.turn_left", Keys: []string{"j"}, Mode: core.ModeWorldWalk},
		{Command: luaKeyPrefix + "world:world+a", Keys: []string{"world+a"}, Mode: core.ModeWorld},
		{Keys: []string{"h"}, Mode: core.ModeNormalWorld},
		{Command: luaKeyPrefix + "world.library:e", Keys: []string{"e"}, Mode: core.ModeWorldLibrary},
	}
	m, _ := luaModel(t, over)
	r := m.keymap()
	served := r.base.Overlay(r.over)
	keys := map[string]bool{"j": true, "z": true, "X": true, "ctrl+y": true, "enter": true, "n": true, "h": true,
		"world+a": true, "e": true, "l": true, "q": true, "gg": true, "g": true, "?": true, "esc": true}
	for _, b := range r.base {
		for _, k := range b.Keys {
			keys[k] = true
		}
	}
	modes := append([]string{core.ModeNormal, core.ModeNormalTodos, core.ModeInsert, core.ModeAny}, core.WorldModes...)
	effective := func(name string, ok bool) string {
		if !ok {
			return ""
		}
		return name
	}
	type slot struct{ mode, key string }
	differ := map[slot][2]string{ // TUI, served
		{core.ModeNormalWorld, "j"}:  {"nav.top", "world.down"},
		{core.ModeNormalWorld, "h"}:  {"", "nav.collapse"},
		{core.ModeWorldLibrary, "j"}: {"nav.top", "world.shelf_down"},
		{core.ModeWorldLibrary, "h"}: {"", "world.book_prev"},
		{core.ModeNormalWorld, "z"}:  {luaKeyPrefix + "normal:z", "world.hud"},
		{core.ModeWorldLibrary, "z"}: {luaKeyPrefix + "normal:z", "world.hud"},
	}
	n := 0
	for _, mode := range modes {
		for key := range keys {
			tui := effective(r.Resolve(mode, key))
			srv := effective(served.Resolve(mode, key))
			if d, ok := differ[slot{mode, key}]; ok {
				if !core.IsWorldMode(mode) || tui != d[0] || srv != d[1] {
					t.Errorf("%s %q: TUI %q, served %q, want %q", mode, key, tui, srv, d)
				}
				continue
			}
			if tui != srv {
				t.Errorf("%s %q: TUI %q, served %q", mode, key, tui, srv)
			}
			if r.StartsSequence(mode, key) != served.StartsSequence(mode, key) {
				t.Errorf("%s %q: StartsSequence differs", mode, key)
			}
			n++
		}
	}
	if n < 100 {
		t.Fatalf("only %d cases", n)
	}
	if c := served.Collisions(); len(c) != 0 {
		t.Fatalf("served keymap collisions: %v", c)
	}
}
