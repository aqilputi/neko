package tui

import (
	"errors"
	"sync"
	"testing"

	tea "charm.land/bubbletea/v2"

	"github.com/aqilputi/nekuroddo/internal/app"
	"github.com/aqilputi/nekuroddo/internal/core"
)

// fakeLua stands for *lua.Runtime: Eval echoes, the keymap is what a test
// sets, RunBinding records what ran.
type fakeLua struct {
	mu   sync.Mutex
	km   core.Keymap
	ran  []string
	ctxs []app.Context
}

func (f *fakeLua) Eval(code string) (string, error) {
	if code == "boom" {
		return "", errors.New("lua: boom")
	}
	return "=" + code, nil
}

func (f *fakeLua) Keymap() core.Keymap { return f.km }

func (f *fakeLua) RunBinding(ctx app.Context, command string) (app.Result, error) {
	f.mu.Lock()
	defer f.mu.Unlock()
	f.ran = append(f.ran, command)
	f.ctxs = append(f.ctxs, ctx)
	return app.Info("rodou " + command), nil
}

func luaModel(t *testing.T, km core.Keymap) (Model, *fakeLua) {
	t.Helper()
	f := &fakeLua{km: km}
	m := bhModel(t, bhSessions()...).WithLua(f)
	return m, f
}

// ":lua <code>" in the palette evaluates and shows the result as a toast.
func TestPaletteLuaEval(t *testing.T) {
	m, _ := luaModel(t, nil)
	m.openPaletteWith("lua 1 + 1")
	d := m.top().(*paletteDialog)
	if it := d.items[d.matches[0].Index]; it.title != "▶ lua 1 + 1" || it.run == nil {
		t.Fatalf("the typed code is the item enter runs: %+v", it)
	}
	if tm, ok := bhHasToast(m.key(tea.KeyPressMsg{Code: tea.KeyEnter}), "lua › =1 + 1"); !ok || tm.err {
		t.Fatalf("result toast: %+v %v", tm, ok)
	}
	if m.top() != nil {
		t.Fatal("the palette closes")
	}

	m.openPaletteWith("lua boom")
	if tm, ok := bhHasToast(m.key(tea.KeyPressMsg{Code: tea.KeyEnter}), "lua: boom"); !ok || !tm.err {
		t.Fatalf("error toast: %+v %v", tm, ok)
	}

	// no code yet: the item waits
	m.openPaletteWith("lua ")
	d = m.top().(*paletteDialog)
	if it := d.items[d.matches[0].Index]; it.run != nil || it.title != "lua <código>" {
		t.Fatalf("waiting item: %+v", it)
	}
	if m.key(tea.KeyPressMsg{Code: tea.KeyEnter}); m.top() != d {
		t.Fatal("enter on the waiting item keeps the palette")
	}
}

// With no Lua runtime, "lua …" is just text to search.
func TestPaletteLuaWithoutRuntime(t *testing.T) {
	m := bhModel(t, bhSessions()...)
	if m.luaCommand("lua 1") != nil {
		t.Fatal("no runtime: no :lua")
	}
	m.lua = &fakeLua{}
	if m.luaCommand("luar") != nil || m.luaCommand("group x") != nil {
		t.Fatal("only \"lua <code>\" is Lua")
	}
}

// Lua's key overrides come before the core's keymap: a key bound to a Lua
// function runs it (with this client's selection), a key bound to a
// command runs that command, a deleted key does nothing.
func TestLuaKeymapOverlay(t *testing.T) {
	m, f := luaModel(t, core.Keymap{
		{Command: luaKeyPrefix + "normal:K", Keys: []string{"K"}, Mode: core.ModeNormal},
		{Command: "", Keys: []string{"j"}, Mode: core.ModeNormal},
		{Command: "tab.inbox", Keys: []string{"Y"}, Mode: core.ModeNormal},
		{Command: luaKeyPrefix + "insert:ctrl+y", Keys: []string{"ctrl+y"}, Mode: core.ModeInsert},
		{Command: luaKeyPrefix + "normal:QQ", Keys: []string{"QQ"}, Mode: core.ModeNormal},
	})
	bhSelect(t, &m, "a")
	if tm, ok := bhHasToast(m.key(bhRune('K')), "rodou lua.key:normal:K"); !ok || tm.err {
		t.Fatalf("K runs the Lua function: %+v %v", tm, ok)
	}
	if len(f.ctxs) != 1 || f.ctxs[0].SessionID != "a" {
		t.Fatalf("with the selection: %+v", f.ctxs)
	}

	cur := m.view.Cursor
	if cmd := m.key(bhRune('j')); cmd != nil || m.view.Cursor != cur {
		t.Fatal("a deleted key does nothing")
	}

	m.key(bhRune('Y'))
	if m.tab != tabInbox {
		t.Fatal("a key rebound to a command runs it")
	}

	// Lua's sequences count as sequences (the TUI itself only keeps gg/dd
	// pending, as before)
	if !m.keymap().StartsSequence(core.ModeNormal, "Q") {
		t.Fatal("Lua's sequences count")
	}

	// INSERT keys bound by Lua are nekuroddo's even inside claude
	m.mode = modeInsert
	bhRun(m.key(tea.KeyPressMsg{Code: 'y', Mod: tea.ModCtrl}))
	if len(f.ran) != 2 || f.ran[1] != luaKeyPrefix+"insert:ctrl+y" {
		t.Fatalf("ctrl+y in INSERT: %v", f.ran)
	}
}

// Without Lua the TUI resolves keys exactly as the core's keymap does.
func TestKeymapWithoutLua(t *testing.T) {
	m := bhModel(t, bhSessions()...)
	km := m.keymap()
	if km.over != nil {
		t.Fatal("no overrides without Lua")
	}
	want, _ := m.app.Keymap().Resolve(core.ModeNormal, "j")
	if got, ok := km.Resolve(core.ModeNormal, "j"); !ok || got != want {
		t.Fatalf("j: %q want %q", got, want)
	}
	if m.runLuaKey(luaKeyPrefix+"normal:K") != nil || m.runLuaKey("") != nil {
		t.Fatal("no runtime: a Lua key does nothing")
	}
}
