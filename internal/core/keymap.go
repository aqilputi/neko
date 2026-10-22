package core

import (
	"fmt"
	"strings"
	"unicode/utf8"
)

// ModeNormalTodos is NORMAL mode on the todos tab. A mode "a.b" is a
// sub-mode of "a": its bindings shadow the parent's, and every key it does
// not bind falls back to the parent's (then to "*").
const ModeNormalTodos = "normal.todos"

// The world's modes (the nkw compositor, plan-mundo §3.12). Only the nkw
// resolves them, in its own order (world first; normal.world falls back to
// normal and "*"; world.focus has no fallback); every other client ignores
// them. A key with "world+" is a global chord whatever mode binds it.
const (
	ModeWorld        = "world"
	ModeNormalWorld  = "normal.world"
	ModeWorldWalk    = "world.walk"
	ModeWorldFocus   = "world.focus"
	ModeWorldTalk    = "world.talk"
	ModeWorldHint    = "world.hint"
	ModeWorldPalette = "world.palette"
	ModeWorldLine    = "world.line"
	// ModeWorldLibrary is browsing a shelf of the library (plan-mundo §7):
	// it falls back to NAV (normal.world), not to "world".
	ModeWorldLibrary = "world.library"
)

// WorldModes lists the world's modes.
var WorldModes = []string{ModeWorld, ModeNormalWorld, ModeWorldWalk, ModeWorldFocus, ModeWorldTalk,
	ModeWorldHint, ModeWorldPalette, ModeWorldLine, ModeWorldLibrary}

// IsWorldMode reports whether mode is one of the world's ("world",
// "world.*", "normal.world"): bindings the TUI and the GUI never show.
func IsWorldMode(mode string) bool {
	return mode == ModeWorld || mode == ModeNormalWorld || strings.HasPrefix(mode, ModeWorld+".")
}

// BuildKeymap makes the keymap every client resolves keys with (plan §2.7)
// from the commands, in their order: one Binding per command with keys. A
// command with no mode binds in NORMAL; its help text is its title.
func BuildKeymap(cmds []Command) Keymap {
	var km Keymap
	for _, c := range cmds {
		if len(c.Keys) == 0 {
			continue
		}
		mode := c.Mode
		if mode == "" {
			mode = ModeNormal
		}
		keys := append([]string(nil), c.Keys...)
		km = append(km, Binding{Command: c.Name, Keys: keys, Mode: mode, Help: Help{Key: helpKey(keys), Desc: c.Title}})
	}
	return km
}

func helpKey(keys []string) string { return strings.Join(keys, "/") }

// ParentMode is the mode a mode falls back to: "normal.todos" → "normal" →
// "*" → "". The library's mode is the one exception: "world.library" →
// "normal.world" (the shelf keys shadow NAV's, the rest is NAV).
func ParentMode(mode string) string {
	switch {
	case mode == ModeAny || mode == "":
		return ""
	case mode == ModeWorldLibrary:
		return ModeNormalWorld
	case strings.Contains(mode, "."):
		return mode[:strings.LastIndex(mode, ".")]
	}
	return ModeAny
}

// Resolve is the command key runs in mode: the mode's own bindings first,
// then its parents' up to "*". A sequence ("gg") resolves as typed.
func (k Keymap) Resolve(mode, key string) (string, bool) {
	for m := mode; m != ""; m = ParentMode(m) {
		for _, b := range k {
			if b.Mode == m && hasKey(b.Keys, key) {
				return b.Command, true
			}
		}
	}
	return "", false
}

// StartsSequence reports whether key is the first press of a sequence bound
// in mode ("g" of "gg").
func (k Keymap) StartsSequence(mode, key string) bool {
	if !IsSequence(key + key) {
		return false
	}
	_, ok := k.Resolve(mode, key+key)
	return ok
}

// IsSequence reports whether a key is a sequence: two presses of the same
// printable key ("gg", "dd").
func IsSequence(key string) bool {
	r, n := utf8.DecodeRuneInString(key)
	return n > 0 && len(key) == 2*n && strings.Count(key, string(r)) == 2
}

func hasKey(keys []string, key string) bool {
	for _, k := range keys {
		if k == key {
			return true
		}
	}
	return false
}

// Find is the binding of command.
func (k Keymap) Find(command string) (Binding, bool) {
	for _, b := range k {
		if b.Command == command {
			return b, true
		}
	}
	return Binding{}, false
}

// With overlays over on k (Lua's nk.keymap.set): a binding for a command k
// has replaces its keys (and its mode, when given; the help text stays),
// one with no keys unbinds it, and the rest are appended. k is not changed.
func (k Keymap) With(over Keymap) Keymap {
	out := append(Keymap(nil), k...)
	for _, o := range over {
		i := -1
		for j, b := range out {
			if b.Command == o.Command {
				i = j
				break
			}
		}
		switch {
		case len(o.Keys) == 0:
			if i >= 0 {
				out = append(out[:i], out[i+1:]...)
			}
			continue
		case i < 0:
			if o.Mode == "" {
				o.Mode = ModeNormal
			}
			o.Help.Key = helpKey(o.Keys)
			out = append(out, o)
			continue
		}
		b := out[i]
		b.Keys = append([]string(nil), o.Keys...)
		if o.Mode != "" {
			b.Mode = o.Mode
		}
		b.Help.Key = helpKey(b.Keys)
		if o.Help.Desc != "" {
			b.Help.Desc = o.Help.Desc
		}
		out[i] = b
	}
	return out
}

// LuaKeyPrefix names a key bound to a Lua function: "lua.key:<mode>:<key>"
// (internal/lua's Runtime.RunBinding runs it; POST /v1/commands/<name>).
const LuaKeyPrefix = "lua.key:"

// Overlay lays over on k (the Lua runtime's nk.keymap.set/del on the
// registry keymap, GET /v1/keymap): a binding of over replaces the same
// (mode, key) of k, in that binding's place; one with an empty Command
// removes that key; the rest are appended in over's order. Two overrides
// of one key: the last wins. A "lua.key:" binding passes through with the
// help text "lua"; another one with no help text takes its command's. An
// override with no mode is NORMAL's. Neither input is changed and the
// result shares no key slice with them.
func (k Keymap) Overlay(over Keymap) Keymap {
	type slot struct{ mode, key string }
	// normalise over: one owner per slot, the last one
	norm := make(Keymap, 0, len(over))
	for _, o := range over {
		if o.Mode == "" {
			o.Mode = ModeNormal
		}
		o.Keys = append([]string(nil), o.Keys...)
		norm = append(norm, o)
	}
	owner := map[slot]int{}
	for i, o := range norm {
		for _, key := range o.Keys {
			owner[slot{o.Mode, key}] = i
		}
	}
	for i := range norm {
		var keys []string
		for _, key := range norm[i].Keys {
			if owner[slot{norm[i].Mode, key}] == i {
				keys = append(keys, key)
			}
		}
		norm[i].Keys = keys
		norm[i].Help.Key = helpKey(keys)
		switch {
		case strings.HasPrefix(norm[i].Command, LuaKeyPrefix):
			norm[i].Help.Desc = "lua"
		case norm[i].Help.Desc == "":
			if b, ok := k.Find(norm[i].Command); ok {
				norm[i].Help.Desc = b.Help.Desc
			}
		}
	}
	placed := make([]bool, len(norm))
	place := func(out Keymap, i int) Keymap {
		if placed[i] {
			return out
		}
		placed[i] = true
		if norm[i].Command == "" || len(norm[i].Keys) == 0 {
			return out
		}
		return append(out, norm[i])
	}
	out := make(Keymap, 0, len(k)+len(norm))
	for _, b := range k {
		var keep []string
		var hit []int
		for _, key := range b.Keys {
			if i, ok := owner[slot{b.Mode, key}]; ok {
				hit = append(hit, i)
				continue
			}
			keep = append(keep, key)
		}
		if len(keep) > 0 {
			b.Keys = keep
			if len(hit) > 0 {
				b.Help.Key = helpKey(keep)
			}
			out = append(out, b)
		}
		for _, i := range hit {
			out = place(out, i)
		}
	}
	for i := range norm {
		out = place(out, i)
	}
	return out
}

// Collisions lists every key bound to more than one command in the same
// mode, as `mode "key": cmd, cmd`. A "*" binding counts in every mode; a
// sub-mode shadowing its parent is not a collision.
func (k Keymap) Collisions() []string {
	type slot struct{ mode, key string }
	var order []slot
	by := map[slot][]int{}
	for i, b := range k {
		for _, key := range b.Keys {
			s := slot{b.Mode, key}
			if _, seen := by[s]; !seen {
				order = append(order, s)
			}
			by[s] = append(by[s], i)
		}
	}
	var out []string
	for _, s := range order {
		idx := by[s]
		if s.mode != ModeAny {
			idx = merge(idx, by[slot{ModeAny, s.key}])
		}
		if len(idx) < 2 {
			continue
		}
		names := make([]string, len(idx))
		for j, i := range idx {
			names[j] = k[i].Command
		}
		out = append(out, fmt.Sprintf("%s %q: %s", s.mode, s.key, strings.Join(names, ", ")))
	}
	return out
}

// merge joins two ascending index lists, keeping the order.
func merge(a, b []int) []int {
	out := make([]int, 0, len(a)+len(b))
	for len(a) > 0 && len(b) > 0 {
		if a[0] < b[0] {
			out, a = append(out, a[0]), a[1:]
		} else {
			out, b = append(out, b[0]), b[1:]
		}
	}
	return append(append(out, a...), b...)
}
