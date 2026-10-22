package tui

import (
	"encoding/json"
	"strings"
	"time"

	"charm.land/bubbles/v2/help"
	"charm.land/bubbles/v2/key"
	tea "charm.land/bubbletea/v2"
	"charm.land/lipgloss/v2"

	"github.com/aqilputi/nekuroddo/internal/app"
	"github.com/aqilputi/nekuroddo/internal/core"
	"github.com/aqilputi/nekuroddo/internal/theme"
)

// luaKeyPrefix names a key Lua bound to a function (lua.Runtime.RunBinding
// runs it).
const luaKeyPrefix = "lua.key:"

// keyResolver resolves keys in Lua's overrides (nk.keymap.set/del) first,
// then in the core's keymap. A key Lua deleted resolves to "" (it does
// nothing).
type keyResolver struct{ over, base core.Keymap }

// keymap is what the TUI resolves keys with. The help lists Lua's
// overrides in a column of their own (withLuaHelp).
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

// ---- nk.ui.* events (the bus) ----

// The UI events Lua publishes when the host gave it the capability
// (lua.EvUICatMeow, lua.EvUIPaletteOpen: the TUI does not import lua).
const (
	luaEvMeow        = "ui.cat.meow"
	luaEvPaletteOpen = "ui.palette_open"
)

// isLuaUIEvent reports whether onLuaUIEvent handles an event: the bus
// listener follows these.
func isLuaUIEvent(name string) bool { return name == luaEvMeow || name == luaEvPaletteOpen }

// onLuaUIEvent draws a Lua UI event from the bus: nk.ui.cat.meow makes the
// cat say the text; nk.ui.palette_open opens the palette with the prefill
// typed (an open palette takes it instead). Other events: nil.
func (m *Model) onLuaUIEvent(e core.Event) tea.Cmd {
	switch e.Name {
	case luaEvMeow:
		if text := eventField(e.Data, "text"); text != "" {
			m.cat.Meow(core.Truncate(text, 50))
		}
	case luaEvPaletteOpen:
		q := eventField(e.Data, "prefill")
		if d, ok := m.top().(*paletteDialog); ok {
			d.input.SetValue(q)
			d.input.CursorEnd()
			d.filter(m)
			return nil
		}
		return m.openPaletteWith(q)
	}
	return nil
}

// eventField is a string field of an event payload: map[string]string in
// process, map[string]any once it went through JSON.
func eventField(data any, name string) string {
	switch d := data.(type) {
	case map[string]string:
		return d[name]
	case map[string]any:
		s, _ := d[name].(string)
		return s
	}
	return ""
}

// ---- help ----

// luaHelpCols is how many columns the help's Lua section has.
const luaHelpCols = 3

// luaHelp is the help dialog's section with Lua's keys, below the core's
// columns ("" when Lua bound none).
func (m *Model) luaHelp(h help.Model, t theme.Theme) string {
	bs := m.luaBindings()
	if len(bs) == 0 {
		return ""
	}
	per := (len(bs) + luaHelpCols - 1) / luaHelpCols
	var cols [][]key.Binding
	for len(bs) > 0 {
		n := min(per, len(bs))
		cols = append(cols, bs[:n])
		bs = bs[n:]
	}
	head := lipgloss.NewStyle().Background(t.Bg).Foreground(t.Accent).Bold(true).Render("teclas do init.lua")
	return "\n\n" + head + "\n" + h.FullHelpView(cols)
}

// luaBindings are Lua's key overrides as help entries. A deleted key shows
// as "desligada", a function as "função lua", a key outside NORMAL with its
// mode.
func (m *Model) luaBindings() []key.Binding {
	if m.lua == nil {
		return nil
	}
	var col []key.Binding
	for _, b := range m.lua.Keymap() {
		if core.IsWorldMode(b.Mode) { // the world's keys (nkw), never typed here
			continue
		}
		keys := make([]string, len(b.Keys))
		for i, k := range b.Keys {
			if core.IsSequence(k) {
				k = k[:len(k)/2]
			}
			keys[i] = k
		}
		desc := b.Help.Desc
		switch {
		case b.Command == "":
			desc = "desligada"
		case strings.HasPrefix(b.Command, luaKeyPrefix):
			desc = "função lua"
		case desc == "":
			desc = b.Command
		}
		if b.Mode != core.ModeNormal {
			desc += " [" + b.Mode + "]"
		}
		col = append(col, kb(keys, strings.Join(keys, "/"), desc))
	}
	return col
}

// ---- lua.messages ----

// luaMessage is one entry of lua.messages' Data (lua.Message, read through
// JSON: the TUI does not import lua).
type luaMessage struct {
	At    time.Time `json:"at"`
	File  string    `json:"file,omitempty"`
	Level string    `json:"level"`
	Text  string    `json:"text"`
}

// onLuaMessages shows lua.messages' answer: the "últimos erros lua" dialog
// when the ring has something, else the command's toast.
func (m *Model) onLuaMessages(res app.Result) tea.Cmd {
	var msgs []luaMessage
	if b, err := json.Marshal(res.Data); err == nil {
		_ = json.Unmarshal(b, &msgs) // not a list of messages: msgs stays empty
	}
	if len(msgs) == 0 {
		return toastCmd(res.Toast, res.Err)
	}
	m.push(luaMessagesDialog{msgs: msgs})
	return nil
}

// luaMessagesDialog lists the runtime's last errors and nk.log messages,
// newest last.
type luaMessagesDialog struct{ msgs []luaMessage }

func (luaMessagesDialog) title() string                                  { return "últimos erros lua · qualquer tecla fecha" }
func (luaMessagesDialog) width(m *Model) int                             { return min(m.w-4, 130) }
func (luaMessagesDialog) handle(*Model, tea.KeyPressMsg) (bool, tea.Cmd) { return true, nil }

func (d luaMessagesDialog) view(m *Model, t theme.Theme, w int) string {
	b := lipgloss.NewStyle().Background(t.Bg)
	msgs := d.msgs
	if n := max(m.h-10, 5); len(msgs) > n {
		msgs = msgs[len(msgs)-n:]
	}
	lines := make([]string, 0, len(msgs))
	for _, msg := range msgs {
		lvl := b.Foreground(t.Muted)
		if msg.Level == "error" {
			lvl = b.Foreground(t.Error).Bold(true)
		}
		text := msg.Text
		if msg.File != "" && !strings.Contains(text, msg.File) {
			text = msg.File + ": " + text
		}
		lines = append(lines, b.Foreground(t.Subtle).Render(" "+msg.At.Local().Format("15:04:05")+" ")+
			lvl.Width(6).Render(msg.Level)+b.Foreground(t.Text).Render(core.Truncate(text, max(w-20, 10))))
	}
	return strings.Join(lines, "\n")
}
