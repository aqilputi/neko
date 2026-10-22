package core

import (
	"reflect"
	"testing"
)

// overlayBase is a small registry keymap: a command with two keys, a
// sub-mode, a world mode and a "*" binding.
func overlayBase() Keymap {
	return Keymap{
		{Command: "nav.left", Keys: []string{"h", "left"}, Mode: ModeNormal, Help: Help{Key: "h/left", Desc: "esquerda"}},
		{Command: "todo.dispatch", Keys: []string{"enter"}, Mode: ModeNormalTodos, Help: Help{Key: "enter", Desc: "despacha"}},
		{Command: "world.right", Keys: []string{"l", "right"}, Mode: ModeNormalWorld, Help: Help{Key: "l/right", Desc: "próxima estação"}},
		{Command: "switcher.open", Keys: []string{"ctrl+k"}, Mode: ModeAny, Help: Help{Key: "ctrl+k", Desc: "trocar"}},
	}
}

func TestOverlayReplacesTheSameModeAndKey(t *testing.T) {
	got := overlayBase().Overlay(Keymap{
		{Command: "nav.top", Keys: []string{"h"}, Mode: ModeNormal, Help: Help{Key: "h", Desc: "topo"}},
	})
	want := Keymap{
		{Command: "nav.left", Keys: []string{"left"}, Mode: ModeNormal, Help: Help{Key: "left", Desc: "esquerda"}},
		{Command: "nav.top", Keys: []string{"h"}, Mode: ModeNormal, Help: Help{Key: "h", Desc: "topo"}},
		overlayBase()[1], overlayBase()[2], overlayBase()[3],
	}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("Overlay =\n%+v\nwant\n%+v", got, want)
	}
	// both keys taken: the override sits where the base binding was
	got = overlayBase().Overlay(Keymap{
		{Command: "a", Keys: []string{"h"}, Mode: ModeNormal},
		{Command: "b", Keys: []string{"left"}, Mode: ModeNormal},
	})
	if len(got) != 5 || got[0].Command != "a" || got[1].Command != "b" || got[2].Command != "todo.dispatch" {
		t.Fatalf("Overlay in place = %+v", got)
	}
	// a key in another mode is not the same key
	got = overlayBase().Overlay(Keymap{{Command: "x", Keys: []string{"h"}, Mode: ModeNormalWorld}})
	if !reflect.DeepEqual(got[:4], overlayBase()) || len(got) != 5 || got[4].Command != "x" {
		t.Fatalf("other mode = %+v", got)
	}
}

func TestOverlayEmptyCommandRemovesTheKey(t *testing.T) {
	got := overlayBase().Overlay(Keymap{
		{Keys: []string{"l"}, Mode: ModeNormalWorld},
		{Keys: []string{"ctrl+k"}, Mode: ModeAny},
		{Keys: []string{"zz"}, Mode: ModeNormal}, // nothing there: nothing happens
	})
	want := Keymap{
		overlayBase()[0], overlayBase()[1],
		{Command: "world.right", Keys: []string{"right"}, Mode: ModeNormalWorld, Help: Help{Key: "right", Desc: "próxima estação"}},
	}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("Overlay delete =\n%+v\nwant\n%+v", got, want)
	}
	if c, ok := got.Resolve(ModeNormalWorld, "l"); ok {
		t.Fatalf("l still resolves to %s", c)
	}
}

func TestOverlayLuaKeyPassesThrough(t *testing.T) {
	got := overlayBase().Overlay(Keymap{
		{Command: "lua.key:normal.world:n", Keys: []string{"n"}, Mode: ModeNormalWorld},
		{Command: "lua.key:normal:h", Keys: []string{"h"}, Mode: ModeNormal, Help: Help{Desc: "outra coisa"}},
	})
	if c, _ := got.Resolve(ModeNormalWorld, "n"); c != "lua.key:normal.world:n" {
		t.Fatalf("n = %q", c)
	}
	for _, name := range []string{"lua.key:normal.world:n", "lua.key:normal:h"} {
		b, ok := got.Find(name)
		if !ok || b.Help.Desc != "lua" || b.Help.Key != b.Keys[0] {
			t.Errorf("%s = %+v", name, b)
		}
	}
}

func TestOverlayKeepsOrderAndCopies(t *testing.T) {
	base := overlayBase()
	over := Keymap{
		{Command: "new.one", Keys: []string{"1"}},                                                          // no mode: normal
		{Command: "world.right", Keys: []string{"n"}, Mode: ModeNormalWorld},                               // same command, new key: help from the base
		{Command: "todo.other", Keys: []string{"enter"}, Mode: ModeNormalTodos, Help: Help{Desc: "outro"}}, // in place
	}
	got := base.Overlay(over)
	names := make([]string, len(got))
	for i, b := range got {
		names[i] = b.Command
	}
	want := []string{"nav.left", "todo.other", "world.right", "switcher.open", "new.one", "world.right"}
	if !reflect.DeepEqual(names, want) {
		t.Fatalf("order = %v, want %v", names, want)
	}
	if got[4].Mode != ModeNormal || got[4].Help.Key != "1" {
		t.Fatalf("new.one = %+v", got[4])
	}
	if got[5].Help != (Help{Key: "n", Desc: "próxima estação"}) {
		t.Fatalf("help of a rebound command = %+v", got[5].Help)
	}
	// neither input changes, and the output shares no keys with them
	if !reflect.DeepEqual(base, overlayBase()) || over[0].Mode != "" {
		t.Fatal("Overlay changed its inputs")
	}
	got[0].Keys[0] = "X"
	got[1].Keys[0] = "X"
	if base[0].Keys[0] != "h" || over[2].Keys[0] != "enter" {
		t.Fatal("Overlay shares key slices")
	}
	// nothing over: the same table
	if g := base.Overlay(nil); !reflect.DeepEqual(g, base) {
		t.Fatalf("Overlay(nil) = %+v", g)
	}
	if g := Keymap(nil).Overlay(nil); g == nil || len(g) != 0 {
		t.Fatalf("nil.Overlay(nil) = %#v", g)
	}
}

// Two overrides of one key (Lua rebuilds keep one; any other caller): the
// last wins and the table has no collision.
func TestOverlayLastOverrideWins(t *testing.T) {
	got := overlayBase().Overlay(Keymap{
		{Command: "a", Keys: []string{"x", "y"}, Mode: ModeNormal},
		{Command: "b", Keys: []string{"x"}, Mode: ModeNormal},
		{Command: "c", Keys: []string{"y"}, Mode: ModeNormal},
	})
	if c := got.Collisions(); len(c) != 0 {
		t.Fatalf("collisions: %v", c)
	}
	for key, want := range map[string]string{"x": "b", "y": "c"} {
		if c, _ := got.Resolve(ModeNormal, key); c != want {
			t.Errorf("%s = %q, want %s", key, c, want)
		}
	}
	if _, ok := got.Find("a"); ok {
		t.Fatal("a has no key left and stays")
	}
}

// Overrides never create a collision over the world's modes.
func TestOverlayNoCollisionsInWorldModes(t *testing.T) {
	base := Keymap{
		{Command: "world.left", Keys: []string{"h", "left"}, Mode: ModeNormalWorld},
		{Command: "world.forward", Keys: []string{"w"}, Mode: ModeWorldWalk},
		{Command: "world.book_prev", Keys: []string{"h", "left"}, Mode: ModeWorldLibrary},
		{Command: "world.quit", Keys: []string{"world+shift+e"}, Mode: ModeWorld},
	}
	got := base.Overlay(Keymap{
		{Command: "world.right", Keys: []string{"h"}, Mode: ModeNormalWorld},
		{Command: "lua.key:world.walk:w", Keys: []string{"w"}, Mode: ModeWorldWalk},
		{Command: "lua.key:world:world+shift+e", Keys: []string{"world+shift+e"}, Mode: ModeWorld},
		{Command: "world.book_next", Keys: []string{"h"}, Mode: ModeWorldLibrary},
		{Keys: []string{"left"}, Mode: ModeWorldLibrary},
	})
	if c := got.Collisions(); len(c) != 0 {
		t.Fatalf("collisions: %v", c)
	}
	for _, c := range []struct{ mode, key, want string }{
		{ModeNormalWorld, "h", "world.right"},
		{ModeNormalWorld, "left", "world.left"},
		{ModeWorldWalk, "w", "lua.key:world.walk:w"},
		{ModeWorld, "world+shift+e", "lua.key:world:world+shift+e"},
		{ModeWorldLibrary, "h", "world.book_next"},
		{ModeWorldLibrary, "left", "world.left"}, // deleted here: falls back to normal.world
	} {
		if got, _ := got.Resolve(c.mode, c.key); got != c.want {
			t.Errorf("%s %q = %q, want %s", c.mode, c.key, got, c.want)
		}
	}
}

func TestWorldLibraryMode(t *testing.T) {
	if ModeWorldLibrary != "world.library" || !IsWorldMode(ModeWorldLibrary) {
		t.Fatalf("ModeWorldLibrary = %q", ModeWorldLibrary)
	}
	if p := ParentMode(ModeWorldLibrary); p != ModeNormalWorld {
		t.Fatalf("ParentMode(world.library) = %q", p)
	}
	var chain []string
	for m := ModeWorldLibrary; m != ""; m = ParentMode(m) {
		chain = append(chain, m)
	}
	if want := []string{"world.library", "normal.world", "normal", "*"}; !reflect.DeepEqual(chain, want) {
		t.Fatalf("chain = %v", chain)
	}
}
