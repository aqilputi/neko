package core

import (
	"reflect"
	"testing"
)

func TestBuildKeymap(t *testing.T) {
	km := BuildKeymap([]Command{
		{Name: "nav.down", Title: "desce", Keys: []string{"j", "down"}, Mode: ModeNormal},
		{Name: "sessions.archive_stale", Title: "palette only"},
		{Name: "x.nomode", Title: "sem modo", Keys: []string{"ctrl+a"}},
		{Name: "nav.top", Title: "topo", Keys: []string{"gg"}, Mode: ModeNormal},
	})
	want := Keymap{
		{Command: "nav.down", Keys: []string{"j", "down"}, Mode: ModeNormal, Help: Help{Key: "j/down", Desc: "desce"}},
		{Command: "x.nomode", Keys: []string{"ctrl+a"}, Mode: ModeNormal, Help: Help{Key: "ctrl+a", Desc: "sem modo"}},
		{Command: "nav.top", Keys: []string{"gg"}, Mode: ModeNormal, Help: Help{Key: "gg", Desc: "topo"}},
	}
	if !reflect.DeepEqual(km, want) {
		t.Fatalf("BuildKeymap = %+v", km)
	}
	// the keys are copied: changing the command later does not move the binding
	cmds := []Command{{Name: "a", Keys: []string{"a"}}}
	km = BuildKeymap(cmds)
	cmds[0].Keys[0] = "b"
	if km[0].Keys[0] != "a" {
		t.Fatal("keys shared with the command")
	}
}

func TestParentMode(t *testing.T) {
	for in, want := range map[string]string{
		ModeNormalTodos: ModeNormal, ModeNormal: ModeAny, ModeInsert: ModeAny, ModeAny: "", "": "", "a.b.c": "a.b",
		ModeNormalWorld: ModeNormal, ModeWorld: ModeAny, ModeWorldWalk: ModeWorld, ModeWorldLibrary: ModeNormalWorld,
		ModeWorldPhone: "", // the phone has no fallback (S26)
	} {
		if got := ParentMode(in); got != want {
			t.Errorf("ParentMode(%q) = %q, want %q", in, got, want)
		}
	}
}

// The world's modes (nkw) are the ones no other client shows.
func TestIsWorldMode(t *testing.T) {
	for _, m := range WorldModes {
		if !IsWorldMode(m) {
			t.Errorf("%s is not a world mode", m)
		}
	}
	if len(WorldModes) != 10 {
		t.Fatalf("WorldModes = %v", WorldModes)
	}
	for _, m := range []string{ModeNormal, ModeNormalTodos, ModeInsert, ModeAny, "", "worldly", "normal.worldx", "x.world"} {
		if IsWorldMode(m) {
			t.Errorf("%q is a world mode", m)
		}
	}
	if !IsWorldMode("world.novo") {
		t.Error("a new world.* sub-mode is a world mode too")
	}
}

func TestKeymapResolve(t *testing.T) {
	km := Keymap{
		{Command: "session.open", Keys: []string{"enter"}, Mode: ModeNormal},
		{Command: "todo.dispatch", Keys: []string{"enter"}, Mode: ModeNormalTodos},
		{Command: "switcher.open", Keys: []string{"ctrl+k"}, Mode: ModeAny},
		{Command: "nav.top", Keys: []string{"gg"}, Mode: ModeNormal},
	}
	for _, c := range []struct{ mode, key, want string }{
		{ModeNormal, "enter", "session.open"},
		{ModeNormalTodos, "enter", "todo.dispatch"}, // the sub-mode shadows its parent
		{ModeNormalTodos, "ctrl+k", "switcher.open"},
		{ModeInsert, "ctrl+k", "switcher.open"}, // "*" is every mode
		{ModeNormal, "gg", "nav.top"},
		{ModeInsert, "enter", ""},
		{ModeNormal, "g", ""},
	} {
		got, ok := km.Resolve(c.mode, c.key)
		if got != c.want || ok != (c.want != "") {
			t.Errorf("Resolve(%q, %q) = %q %v, want %q", c.mode, c.key, got, ok, c.want)
		}
	}
	if !km.StartsSequence(ModeNormalTodos, "g") || km.StartsSequence(ModeNormal, "enter") || km.StartsSequence(ModeInsert, "g") {
		t.Fatal("StartsSequence")
	}
}

func TestKeymapFindAndWith(t *testing.T) {
	km := Keymap{
		{Command: "a", Keys: []string{"a"}, Mode: ModeNormal, Help: Help{Key: "a", Desc: "A"}},
		{Command: "b", Keys: []string{"b"}, Mode: ModeNormal},
	}
	if b, ok := km.Find("b"); !ok || b.Keys[0] != "b" {
		t.Fatal("Find b")
	}
	if _, ok := km.Find("nope"); ok {
		t.Fatal("Find nope")
	}
	over := km.With(Keymap{
		{Command: "a", Keys: []string{"ctrl+a"}},                   // rebinds a, keeps its mode and help text
		{Command: "c", Keys: []string{"c"}, Mode: ModeNormalTodos}, // a new binding
	})
	if b, _ := over.Find("a"); b.Keys[0] != "ctrl+a" || b.Mode != ModeNormal || b.Help != (Help{Key: "ctrl+a", Desc: "A"}) {
		t.Fatalf("rebind = %+v", b)
	}
	if len(over) != 3 || over[2].Command != "c" || over[2].Help.Key != "c" {
		t.Fatalf("With = %+v", over)
	}
	if b, _ := km.Find("a"); b.Keys[0] != "a" {
		t.Fatal("With changed the original keymap")
	}
	// a mode and a help text given replace them
	if b, _ := km.With(Keymap{{Command: "b", Keys: []string{"B"}, Mode: ModeInsert, Help: Help{Desc: "bê"}}}).Find("b"); b.Mode != ModeInsert || b.Help.Desc != "bê" {
		t.Fatalf("mode/help override = %+v", b)
	}
	if b, _ := km.With(Keymap{{Command: "new", Keys: []string{"N"}}}).Find("new"); b.Mode != ModeNormal {
		t.Fatalf("a new binding with no mode is normal: %+v", b)
	}
	// a binding with no keys unbinds the command
	if _, ok := km.With(Keymap{{Command: "a"}}).Find("a"); ok {
		t.Fatal("empty keys unbind")
	}
}

func TestKeymapCollisions(t *testing.T) {
	km := Keymap{
		{Command: "session.open", Keys: []string{"enter"}, Mode: ModeNormal},
		{Command: "todo.dispatch", Keys: []string{"enter"}, Mode: ModeNormalTodos}, // shadowing is fine
		{Command: "switcher.open", Keys: []string{"ctrl+k"}, Mode: ModeAny},
	}
	if c := km.Collisions(); len(c) != 0 {
		t.Fatalf("no collisions: %v", c)
	}
	km = append(km,
		Binding{Command: "x", Keys: []string{"enter"}, Mode: ModeNormal},
		Binding{Command: "y", Keys: []string{"ctrl+k"}, Mode: ModeInsert},
		Binding{Command: "z", Keys: []string{"q", "q"}, Mode: ModeNormal},
		Binding{Command: "w1", Keys: []string{"w"}, Mode: ModeNormal},
		Binding{Command: "w2", Keys: []string{"w"}, Mode: ModeAny},
	)
	want := []string{
		`normal "enter": session.open, x`,
		`insert "ctrl+k": switcher.open, y`,
		`normal "q": z, z`,
		`normal "w": w1, w2`,
	}
	if got := km.Collisions(); !reflect.DeepEqual(got, want) {
		t.Fatalf("Collisions = %q", got)
	}
}

func TestIsSequence(t *testing.T) {
	for k, want := range map[string]bool{"gg": true, "dd": true, "g": false, "gG": false, "up": false, "": false, "ctrl+d": false} {
		if IsSequence(k) != want {
			t.Errorf("IsSequence(%q) != %v", k, want)
		}
	}
}

// world.phone is the phone's keys (S26): a world mode with no fallback, so
// a key the phone does not bind resolves to nothing here (the nkw chains
// "world" before it by itself).
func TestWorldPhoneMode(t *testing.T) {
	if ModeWorldPhone != "world.phone" || !IsWorldMode(ModeWorldPhone) {
		t.Fatalf("ModeWorldPhone = %q", ModeWorldPhone)
	}
	km := Keymap{
		{Command: "world.phone_open", Keys: []string{"enter"}, Mode: ModeWorldPhone},
		{Command: "world.phone", Keys: []string{"world+n"}, Mode: ModeWorld},
		{Command: "app.quit", Keys: []string{"ctrl+c"}, Mode: ModeAny},
	}
	if c, _ := km.Resolve(ModeWorldPhone, "enter"); c != "world.phone_open" {
		t.Fatalf("enter = %q", c)
	}
	for _, k := range []string{"world+n", "ctrl+c"} {
		if c, ok := km.Resolve(ModeWorldPhone, k); ok {
			t.Errorf("%s falls back to %q", k, c)
		}
	}
}
