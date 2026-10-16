package tui

import (
	"strconv"
	"strings"
	"testing"
	"time"

	"charm.land/bubbles/v2/help"
	"charm.land/bubbles/v2/key"

	"github.com/aqilputi/nekuroddo/internal/app"
	"github.com/aqilputi/nekuroddo/internal/core"
)

// nk.ui.cat.meow reaches the cat, whatever shape the payload arrived in
// (in process: map[string]string; decoded from JSON: map[string]any).
func TestLuaUIEventMeow(t *testing.T) {
	m := bhModel(t, bhSessions()...)
	if cmd := m.onLuaUIEvent(core.Event{Name: luaEvMeow, Data: map[string]string{"text": "olá"}}); cmd != nil {
		t.Fatal("a meow needs no command")
	}
	if !m.cat.Awake() {
		t.Fatal("the cat meows")
	}
	m = bhModel(t, bhSessions()...)
	m.onLuaUIEvent(core.Event{Name: luaEvMeow, Data: map[string]any{"text": strings.Repeat("x", 80)}})
	if !m.cat.Awake() {
		t.Fatal("decoded payload works too")
	}
	m = bhModel(t, bhSessions()...)
	m.onLuaUIEvent(core.Event{Name: luaEvMeow, Data: 42})
	if m.cat.Awake() {
		t.Fatal("no text: nothing to say")
	}
}

// nk.ui.palette_open opens the palette with the prefill typed.
func TestLuaUIEventPaletteOpen(t *testing.T) {
	m := bhModel(t, bhSessions()...)
	m.onLuaUIEvent(core.Event{Name: luaEvPaletteOpen, Data: map[string]any{"prefill": "group "}})
	d, ok := m.top().(*paletteDialog)
	if !ok || d.input.Value() != "group " {
		t.Fatalf("palette open with the prefill: %T", m.top())
	}
	// another request while one is open replaces nothing: it stacks no second palette
	m.onLuaUIEvent(core.Event{Name: luaEvPaletteOpen, Data: map[string]string{"prefill": "x"}})
	if len(m.dialogs) != 1 || d.input.Value() != "x" {
		t.Fatalf("the open palette takes the new prefill: %d dialogs, %q", len(m.dialogs), d.input.Value())
	}
}

func TestLuaUIEventOther(t *testing.T) {
	m := bhModel(t, bhSessions()...)
	if m.onLuaUIEvent(core.Event{Name: "toast"}) != nil || m.top() != nil || m.cat.Awake() {
		t.Fatal("other events are not Lua UI events")
	}
	if !isLuaUIEvent(luaEvMeow) || !isLuaUIEvent(luaEvPaletteOpen) || isLuaUIEvent("toast") {
		t.Fatal("isLuaUIEvent")
	}
}

// The help lists Lua's keys in a column of their own.
func TestLuaHelpColumn(t *testing.T) {
	m, _ := luaModel(t, core.Keymap{
		{Command: luaKeyPrefix + "normal:K", Keys: []string{"K"}, Mode: core.ModeNormal, Help: core.Help{Key: "K", Desc: "lua"}},
		{Command: "", Keys: []string{"j"}, Mode: core.ModeNormal},
		{Command: "tab.inbox", Keys: []string{"ctrl+y"}, Mode: core.ModeInsert, Help: core.Help{Key: "ctrl+y", Desc: "inbox"}},
		{Command: "x.y", Keys: []string{"QQ"}, Mode: core.ModeNormal},
	})
	got := helpPairs(m.luaBindings())
	want := []string{"K=função lua", "j=desligada", "ctrl+y=inbox [insert]", "Q=x.y"}
	if strings.Join(got, "|") != strings.Join(want, "|") {
		t.Fatalf("lua bindings:\n got %q\nwant %q", got, want)
	}
	// it shows in the dialog, below the core's columns
	m.push(helpDialog{})
	out := rhScreen(t, m)
	rhHas(t, out, "teclas do init.lua", "função lua", "desligada", "inbox [insert]", "x.y")
	if strings.Index(out, "teclas do init.lua") > strings.Index(out, "mouse: clique") {
		t.Fatal("the Lua section comes before the mouse line")
	}
}

func TestLuaHelpWithoutKeys(t *testing.T) {
	m := bhModel(t, bhSessions()...)
	h := help.New()
	if m.luaHelp(h, m.t()) != "" {
		t.Fatal("no Lua: the help is the core's")
	}
	m.lua = &fakeLua{}
	if m.luaHelp(h, m.t()) != "" {
		t.Fatal("Lua without keys: no section")
	}
}

func helpPairs(bs []key.Binding) []string {
	var out []string
	for _, b := range bs {
		out = append(out, b.Help().Key+"="+b.Help().Desc)
	}
	return out
}

// lua.messages answered with the ring opens the "últimos erros lua" dialog.
func TestLuaMessagesDialog(t *testing.T) {
	m := bhModel(t, bhSessions()...)
	at := time.Date(2026, 10, 16, 14, 5, 9, 0, time.Local)
	type msg struct {
		At    time.Time `json:"at"`
		File  string    `json:"file,omitempty"`
		Level string    `json:"level"`
		Text  string    `json:"text"`
	}
	res := app.Result{Toast: "lua: 2 mensagens", Data: []msg{
		{At: at, File: "init.lua", Level: "error", Text: "init.lua:3: boom"},
		{At: at, Level: "info", Text: "oi"},
	}}
	if cmd := m.onLuaMessages(res); cmd != nil {
		t.Fatal("the dialog is the answer: no toast")
	}
	d, ok := m.top().(luaMessagesDialog)
	if !ok || len(d.msgs) != 2 {
		t.Fatalf("dialog: %T %+v", m.top(), d)
	}
	out := rhScreen(t, m)
	rhHas(t, out, "últimos erros lua", "14:05:09", "error", "init.lua:3: boom", "info", "oi")
	if done, _ := d.handle(&m, bhRune('x')); !done {
		t.Fatal("any key closes it")
	}

	// nothing in the ring: just the toast
	m.pop()
	if tm, ok := bhHasToast(m.onLuaMessages(app.Result{Toast: "lua: nenhuma mensagem"}), "nenhuma mensagem"); !ok || tm.err {
		t.Fatal("empty: toast")
	}
	if m.top() != nil {
		t.Fatal("no dialog for an empty ring")
	}
	// data that is not a list of messages: the toast too
	if _, ok := bhHasToast(m.onLuaMessages(app.Result{Toast: "t", Data: func() {}}), "t"); !ok {
		t.Fatal("undecodable: toast")
	}
	if _, ok := bhHasToast(m.onLuaMessages(app.Result{Toast: "t", Data: "x"}), "t"); !ok {
		t.Fatal("not a list: toast")
	}
}

// The dialog trims to what fits: the latest messages, newest last.
func TestLuaMessagesDialogKeepsTheLatest(t *testing.T) {
	m := bhModel(t, bhSessions()...)
	var msgs []luaMessage
	for i := range 100 {
		msgs = append(msgs, luaMessage{Level: "info", Text: "m" + string(rune('A'+i%26)) + "-" + strconv.Itoa(i)})
	}
	d := luaMessagesDialog{msgs: msgs}
	out := d.view(&m, m.t(), 80)
	if !strings.Contains(out, "mV-99") || strings.Contains(out, "mA-0") {
		t.Fatalf("latest kept:\n%s", out)
	}
	if d.title() == "" || d.width(&m) <= 0 {
		t.Fatal("title/width")
	}
}
