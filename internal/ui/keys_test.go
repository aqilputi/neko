package ui

import (
	"reflect"
	"testing"

	"charm.land/bubbles/v2/key"

	"github.com/aqilputi/nekuroddo/internal/config"
	"github.com/aqilputi/nekuroddo/internal/core"
)

// The palette runs actions by synthesizing the binding's first key; each
// synthesized key must match its own binding.
func TestKeyPressMatchesBindings(t *testing.T) {
	m := New(config.Default())
	for _, group := range m.fullHelp() {
		for _, b := range group {
			k := b.Keys()[0]
			if !key.Matches(keyPress(k), b) {
				t.Errorf("keyPress(%q).String()=%q does not match %v", k, keyPress(k).String(), b.Keys())
			}
		}
	}
}

// keyCommands is the command behind each keyMap field.
var keyCommands = map[string]string{
	"Up": "nav.up", "Down": "nav.down", "Top": "nav.top", "Bottom": "nav.bottom",
	"HalfDown": "nav.half_down", "HalfUp": "nav.half_up", "PageDown": "nav.page_down", "PageUp": "nav.page_up",
	"Collapse": "nav.collapse", "Expand": "nav.expand", "Pick": "nav.pick",
	"NextAttn": "nav.next_attention", "PrevAttn": "nav.prev_attention",
	"TabSessions": "tab.sessions", "TabInbox": "tab.inbox", "TabTodos": "tab.todos", "TabArchive": "tab.archive",
	"NextTab": "tab.next", "PrevTab": "tab.prev",
	"Filter": "view.filter", "ShowAll": "view.show_all", "Sort": "view.sort", "Archive": "session.archive",
	"Rename": "session.rename", "Move": "session.move",
	"Open": "session.open", "Preview": "session.preview", "Full": "session.fullscreen", "Insert": "session.insert",
	"Reply": "session.reply", "New": "session.new", "Stop": "session.stop", "Remove": "session.remove",
	"Close": "pane.close", "Zoom": "pane.zoom",
	"Dispatch": "todo.dispatch", "Start": "todo.start", "Done": "todo.done", "Jira": "jira.toggle",
	"JiraFetch": "jira.fetch", "OpenURL": "jira.open_url",
	"Palette": "palette.open", "Switch": "switcher.open", "Usage": "usage.show", "Theme": "view.theme",
	"Refresh": "sessions.refresh", "Help": "help.open", "Quit": "app.quit",
}

// Every key keys.go binds resolves, in the core's keymap, to the command
// behind its binding: on the todos tab for the todo/Jira keys, in NORMAL
// for the rest. A two-key sequence binds its first key and shows both.
func TestEveryKeyResolvesToACommand(t *testing.T) {
	m := New(config.Default())
	km := m.app.Keymap()
	v := reflect.ValueOf(m.keys())
	for i := range v.NumField() {
		field := v.Type().Field(i).Name
		name, ok := keyCommands[field]
		if !ok {
			t.Errorf("keyMap.%s has no command", field)
			continue
		}
		b := v.Field(i).Interface().(key.Binding)
		mode := core.ModeNormal
		if bd, _ := km.Find(name); bd.Mode == core.ModeNormalTodos {
			mode = core.ModeNormalTodos
		}
		for _, k := range b.Keys() {
			if h := b.Help().Key; core.IsSequence(h) {
				k = h
			}
			if got, _ := km.Resolve(mode, k); got != name {
				t.Errorf("keyMap.%s: %q resolves to %q, want %s", field, k, got, name)
			}
		}
	}
}
