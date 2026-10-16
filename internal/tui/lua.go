package tui

import (
	"strings"

	tea "charm.land/bubbletea/v2"

	"github.com/aqilputi/nekuroddo/internal/core"
)

// luaKeyPrefix names a key Lua bound to a function (lua.Runtime.RunBinding
// runs it).
const luaKeyPrefix = "lua.key:"

// keyResolver resolves keys in Lua's overrides (nk.keymap.set/del) first,
// then in the core's keymap. A key Lua deleted resolves to "" (it does
// nothing).
type keyResolver struct{ over, base core.Keymap }

// keymap is what the TUI resolves keys with. TODO(WP-5.3): the status bar
// and the help still show the core's keymap only, not Lua's overrides.
func (m *Model) keymap() keyResolver {
	r := keyResolver{base: m.app.Keymap()}
	if m.lua != nil {
		r.over = m.lua.Keymap()
	}
	return r
}

func (r keyResolver) Resolve(mode, key string) (string, bool) {
	if name, ok := r.over.Resolve(mode, key); ok {
		return name, true
	}
	return r.base.Resolve(mode, key)
}

func (r keyResolver) StartsSequence(mode, key string) bool {
	return r.over.StartsSequence(mode, key) || r.base.StartsSequence(mode, key)
}

// runLuaKey runs a key Lua bound to a function, off the UI goroutine (it
// has its own budget); "" is a key Lua deleted: it is swallowed.
func (m *Model) runLuaKey(name string) tea.Cmd {
	if name == "" || m.lua == nil {
		return nil
	}
	rt, ctx := m.lua, m.commandCtx()
	return func() tea.Msg {
		res, err := rt.RunBinding(ctx, name)
		return doneMsg(name, res, err)
	}
}

// luaCommand is ":lua <code>" in the palette: the code runs in the Lua
// runtime (Eval, off the UI goroutine, under its command budget) and the
// result comes back as a toast. nil when there is no runtime or q is
// something else.
func (m *Model) luaCommand(q string) *paletteItem {
	code, ok := strings.CutPrefix(strings.TrimLeft(q, " "), "lua ")
	if !ok || m.lua == nil {
		return nil
	}
	const hint = "avalia no runtime Lua"
	code = strings.TrimSpace(code)
	if code == "" {
		return &paletteItem{title: "lua <código>", hint: hint}
	}
	rt := m.lua
	return &paletteItem{title: "▶ lua " + code, hint: hint, run: func(*Model) tea.Cmd {
		return func() tea.Msg {
			out, err := rt.Eval(code)
			if err != nil {
				return toastMsg{text: err.Error(), err: true}
			}
			return toastMsg{text: "lua › " + out}
		}
	}}
}
