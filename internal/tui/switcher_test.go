package tui

import (
	"strings"
	"testing"
	"time"

	tea "charm.land/bubbletea/v2"

	"github.com/aqilputi/nekuroddo/internal/config"
	"github.com/aqilputi/nekuroddo/internal/store"
)

func TestSwitcherListsLiveChatsOnly(t *testing.T) {
	now := time.Now()
	m := newTestModel(config.Default())
	m.w, m.h = 120, 40
	setSnap(&m, store.Snapshot{Sessions: []store.Session{
		{SessionID: "busy", Name: "migração zone presence", Context: "acme", State: "busy", Kind: "interactive", LastAt: now},
		{SessionID: "wait", Name: "docker cleanup", Context: "acme", State: "blocked", Attention: store.AttnBlocked, Kind: "interactive", LastAt: now.Add(-time.Hour)},
		{SessionID: "ended", Name: "scope-guard hook testing", Context: "acme", State: "ended", Ended: true, Stale: true},
		{SessionID: "old", Name: "kanban", Context: "home", State: "history", Kind: "history"},
	}})
	m.rebuild()
	m.keyNormal(tea.KeyPressMsg{Code: 'k', Mod: tea.ModCtrl})
	d, ok := m.top().(*paletteDialog)
	if !ok {
		t.Fatal("ctrl+k should open the switcher")
	}
	var titles []string
	for _, it := range d.items {
		titles = append(titles, it.title)
	}
	if len(titles) != 2 || !strings.Contains(titles[0], "docker cleanup") {
		t.Fatalf("want waiting first then busy, no ended/history: %v", titles)
	}
	// fuzzy: "migz" finds "migração zone presence"
	for _, r := range "migz" {
		m.key(tea.KeyPressMsg{Code: r, Text: string(r)})
	}
	if len(d.matches) != 1 || !strings.Contains(d.items[d.matches[0].Index].title, "migração") {
		t.Fatalf("fuzzy: %+v", d.matches)
	}
	m.key(tea.KeyPressMsg{Code: tea.KeyEnter})
	if m.top() != nil {
		t.Fatal("enter should close the switcher")
	}
	if r := m.cur(); r == nil || r.Sess == nil || r.Sess.SessionID != "busy" {
		t.Fatal("enter should select the chosen chat")
	}
}

// Ended chats never show in the tree (H reveals them) nor in the inbox.
func TestEndedHidden(t *testing.T) {
	m := newTestModel(config.Default())
	setSnap(&m, store.Snapshot{Sessions: []store.Session{
		{SessionID: "ended", Context: "c", State: "ended", Ended: true, Stale: true},
		{SessionID: "live", Context: "c", State: "busy"},
	}})
	m.rebuild()
	if got := sessOrder(&m); len(got) != 1 || got[0] != "live" || len(m.view.Inbox) != 0 {
		t.Fatalf("rows %v inbox %d", got, len(m.view.Inbox))
	}
	m.keyNormal(keyPress("H"))
	if got := sessOrder(&m); len(got) != 2 {
		t.Fatalf("H should reveal it: %v", got)
	}
}

// In INSERT, ctrl+k opens the switcher instead of reaching claude; esc
// closes it and leaves you in INSERT.
func TestCtrlKInsideClaude(t *testing.T) {
	m := newTestModel(config.Default())
	m.w, m.h = 120, 40
	m.mode = modeInsert
	m.key(tea.KeyPressMsg{Code: 'k', Mod: tea.ModCtrl})
	if _, ok := m.top().(*paletteDialog); !ok {
		t.Fatal("ctrl+k in INSERT should open the switcher")
	}
	m.key(tea.KeyPressMsg{Code: tea.KeyEscape})
	if m.top() != nil || m.mode != modeInsert {
		t.Fatal("esc should close it and stay in INSERT")
	}
}
