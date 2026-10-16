package tui

import (
	"reflect"
	"strings"
	"testing"
	"time"

	tea "charm.land/bubbletea/v2"

	"github.com/aqilputi/nekuroddo/internal/core"
	"github.com/aqilputi/nekuroddo/internal/store"
	"github.com/aqilputi/nekuroddo/internal/theme"
)

func TestBehaviorRebuildLayout(t *testing.T) {
	m := bhModel(t, bhSessions()...)
	want := []string{"ctx:c1", "sess:a", "sess:b", "sub:s1", "ctx:c2", "sess:c"}
	if got := bhKinds(&m); !reflect.DeepEqual(got, want) {
		t.Fatalf("rows %v want %v", got, want)
	}
	if m.view.Hidden != 1 {
		t.Fatalf("stale session should be counted as hidden: %d", m.view.Hidden)
	}
	if len(m.view.Inbox) != 2 || m.view.Inbox[0].SessionID != "a" || m.view.Inbox[1].SessionID != "c" {
		t.Fatalf("inbox: blocked first, then returned: %v", m.view.Inbox)
	}
	if m.rows[0].CtxN != 2 {
		t.Fatalf("context row counts its sessions: %d", m.rows[0].CtxN)
	}
	// the cursor follows the selected row when rows move
	bhSelect(t, &m, "c")
	m.view.Collapsed["ctx:c1"] = true
	m.rebuild()
	if r := m.cur(); r == nil || r.Sess == nil || r.Sess.SessionID != "c" {
		t.Fatalf("cursor should stay on c: %v", bhKinds(&m))
	}
}

func TestBehaviorFilterMatchesAndBypassesStale(t *testing.T) {
	m := bhModel(t, bhSessions()...)
	m.view.Filter = "delta"
	m.rebuild()
	if got := sessOrder(&m); !reflect.DeepEqual(got, []string{"d"}) {
		t.Fatalf("a filter also finds stale sessions: %v", got)
	}
	m.view.Filter = "zzzzqqq"
	m.rebuild()
	if len(m.rows) != 0 || m.view.Cursor != 0 {
		t.Fatalf("nothing matches: %v cursor %d", bhKinds(&m), m.view.Cursor)
	}
}

func TestBehaviorSortModes(t *testing.T) {
	now := time.Now()
	ss := []store.Session{
		{SessionID: "x", Name: "zulu", Context: "k2", Cost: 1, LastAt: now},
		{SessionID: "y", Name: "alpha", Context: "k1", Cost: 9, LastAt: now.Add(-time.Hour)},
		{SessionID: "z", Name: "mike", Context: "k1", Cost: 5, LastAt: now.Add(-2 * time.Hour)},
	}
	m := bhModel(t, ss...)
	if got := sessOrder(&m); !reflect.DeepEqual(got, []string{"x", "y", "z"}) {
		t.Fatalf("activity: %v", got)
	}
	steps := []struct {
		mode string
		want []string
	}{
		{"custo", []string{"y", "z", "x"}}, // k1 costs 14 > k2 1
		{"nome", []string{"y", "z", "x"}},  // k1 < k2, alpha < mike
		{"atividade", []string{"x", "y", "z"}},
	}
	for _, s := range steps {
		_, cmd := m.keyNormal(bhRune('S'))
		if core.SortModes[m.view.SortMode] != s.mode {
			t.Fatalf("S should cycle to %s, got %s", s.mode, core.SortModes[m.view.SortMode])
		}
		if _, ok := bhHasToast(cmd, "ordem: "+s.mode); !ok {
			t.Fatalf("S should announce %s", s.mode)
		}
		if got := sessOrder(&m); !reflect.DeepEqual(got, s.want) {
			t.Fatalf("%s: got %v want %v", s.mode, got, s.want)
		}
	}
}

func TestBehaviorSortIndex(t *testing.T) {
	for _, tc := range []struct {
		in   string
		want int
	}{{"atividade", 0}, {"custo", 1}, {"nome", 2}, {"", 0}, {"bogus", 0}} {
		if got := core.SortIndex(tc.in); got != tc.want {
			t.Errorf("core.SortIndex(%q)=%d want %d", tc.in, got, tc.want)
		}
	}
	for _, tc := range []struct {
		in   string
		want int
	}{{"", 0}, {"acceptEdits", 1}, {"bypassPermissions", 4}, {"nope", 0}} {
		if got := permIndex(tc.in); got != tc.want {
			t.Errorf("permIndex(%q)=%d want %d", tc.in, got, tc.want)
		}
	}
}

func TestBehaviorSortInbox(t *testing.T) {
	a := &store.Session{SessionID: "a", Attention: store.AttnIdle}
	b := &store.Session{SessionID: "b", Attention: store.AttnBlocked}
	c := &store.Session{SessionID: "c", Attention: store.AttnBlocked}
	d := &store.Session{SessionID: "d", Attention: store.AttnReturned}
	xs := []*store.Session{a, b, c, d}
	core.SortInbox(xs, map[string]int{"sess:c": -1, "sess:b": 5})
	var got []string
	for _, s := range xs {
		got = append(got, s.SessionID)
	}
	if want := []string{"c", "b", "d", "a"}; !reflect.DeepEqual(got, want) {
		t.Fatalf("got %v want %v", got, want)
	}
}

func TestBehaviorMovementKeys(t *testing.T) {
	var ss []store.Session
	now := time.Now()
	for i := 0; i < 60; i++ {
		ss = append(ss, store.Session{SessionID: string(rune('A' + i)), Context: "c", LastAt: now.Add(-time.Duration(i) * time.Minute)})
	}
	m := bhModel(t, ss...)
	last := len(m.rows) - 1
	half := m.listHeight() / 2
	if m.listHeight() < 2 {
		t.Fatalf("list height %d", m.listHeight())
	}
	cases := []struct {
		name  string
		keys  []tea.KeyPressMsg
		start int
		want  int
	}{
		{"j", []tea.KeyPressMsg{bhRune('j')}, 0, 1},
		{"down", []tea.KeyPressMsg{{Code: tea.KeyDown}}, 0, 1},
		{"k", []tea.KeyPressMsg{bhRune('k')}, 2, 1},
		{"k at top clamps", []tea.KeyPressMsg{bhRune('k')}, 0, 0},
		{"G", []tea.KeyPressMsg{bhRune('G')}, 0, last},
		{"j at bottom clamps", []tea.KeyPressMsg{bhRune('j')}, last, last},
		{"gg", []tea.KeyPressMsg{bhRune('g'), bhRune('g')}, 10, 0},
		{"g then j is just j", []tea.KeyPressMsg{bhRune('g'), bhRune('j')}, 10, 11},
		{"ctrl+d", []tea.KeyPressMsg{{Code: 'd', Mod: tea.ModCtrl}}, 0, half},
		{"ctrl+u", []tea.KeyPressMsg{{Code: 'u', Mod: tea.ModCtrl}}, half, 0},
		{"ctrl+f", []tea.KeyPressMsg{{Code: 'f', Mod: tea.ModCtrl}}, 0, m.listHeight()},
		{"ctrl+b", []tea.KeyPressMsg{{Code: 'b', Mod: tea.ModCtrl}}, last, last - m.listHeight()},
	}
	for _, c := range cases {
		m.view.Cursor = c.start
		for _, k := range c.keys {
			m.key(k)
		}
		if m.view.Cursor != c.want {
			t.Errorf("%s: cursor %d want %d", c.name, m.view.Cursor, c.want)
		}
	}
	if m.pendingG {
		t.Fatal("pendingG must be consumed")
	}
}

func TestBehaviorMoveInboxAndTodos(t *testing.T) {
	m := bhModel(t, bhSessions()...)
	m.tab = tabInbox
	m.move(5)
	if m.view.InboxCursor != 1 {
		t.Fatalf("inbox cursor clamps to the last: %d", m.view.InboxCursor)
	}
	m.move(-5)
	if m.view.InboxCursor != 0 {
		t.Fatalf("inbox cursor clamps to 0: %d", m.view.InboxCursor)
	}
	m.tab = tabTodos
	m.move(3)
	if m.todoCur != 0 {
		t.Fatalf("no todos: cursor stays 0, got %d", m.todoCur)
	}
}

func TestBehaviorClamp(t *testing.T) {
	for _, c := range []struct{ v, lo, hi, want int }{
		{5, 0, 3, 3}, {-1, 0, 3, 0}, {2, 0, 3, 2}, {7, 0, -1, 0},
	} {
		if got := core.Clamp(c.v, c.lo, c.hi); got != c.want {
			t.Errorf("core.Clamp(%d,%d,%d)=%d want %d", c.v, c.lo, c.hi, got, c.want)
		}
	}
}

func TestBehaviorTabKeys(t *testing.T) {
	m := bhModel(t, bhSessions()...)
	steps := []struct {
		k    tea.KeyPressMsg
		want tab
	}{
		{bhRune('2'), tabInbox},
		{bhRune('3'), tabTodos},
		{bhRune('4'), tabArchive},
		{bhRune('1'), tabSessions},
		{tea.KeyPressMsg{Code: tea.KeyTab}, tabInbox},
		{tea.KeyPressMsg{Code: tea.KeyTab}, tabTodos},
		{tea.KeyPressMsg{Code: tea.KeyTab}, tabArchive},
		{tea.KeyPressMsg{Code: tea.KeyTab}, tabSessions},
		{tea.KeyPressMsg{Code: tea.KeyTab, Mod: tea.ModShift}, tabArchive},
		{tea.KeyPressMsg{Code: tea.KeyTab, Mod: tea.ModShift}, tabTodos},
	}
	for i, s := range steps {
		m.key(s.k)
		if m.tab != s.want {
			t.Fatalf("step %d (%s): tab %d want %d", i, s.k.String(), m.tab, s.want)
		}
	}
}

// Update rebuilds rows when the tab changes (archive lists other rows).
func TestBehaviorUpdateRebuildsOnTabChange(t *testing.T) {
	ss := bhSessions()
	ss = append(ss, store.Session{SessionID: "z", Context: "old", Archived: true, Stale: true})
	m := bhModel(t, ss...)
	var tm tea.Model = m
	tm, _ = tm.Update(bhRune('4'))
	got := tm.(Model)
	if got.tab != tabArchive || got.lastTab != tabArchive {
		t.Fatal("4 should switch to the archive")
	}
	if ids := sessOrder(&got); !reflect.DeepEqual(ids, []string{"z"}) {
		t.Fatalf("archive rows after Update: %v", ids)
	}
}

func TestBehaviorShowAllToggle(t *testing.T) {
	m := bhModel(t, bhSessions()...)
	_, cmd := m.keyNormal(bhRune('H'))
	if !m.view.ShowAll {
		t.Fatal("H shows all")
	}
	if _, ok := bhHasToast(cmd, "mostrando tudo"); !ok {
		t.Fatal("H on should say so")
	}
	if got := sessOrder(&m); len(got) != 4 {
		t.Fatalf("stale shown: %v", got)
	}
	subs := 0
	for _, r := range m.rows {
		if r.Kind == core.RowSub {
			subs++
		}
	}
	if subs != 2 {
		t.Fatalf("finished subagents shown with H: %d", subs)
	}
	_, cmd = m.keyNormal(bhRune('H'))
	if _, ok := bhHasToast(cmd, "só as ativas"); !ok || m.view.ShowAll {
		t.Fatal("H again hides")
	}
}

func TestBehaviorCollapseExpand(t *testing.T) {
	m := bhModel(t, bhSessions()...)
	bhSelect(t, &m, "b")
	m.view.Collapsed["sess:b"] = true
	m.rebuild()
	// h on a folded session folds its context
	m.keyNormal(bhRune('h'))
	if !m.view.Collapsed["ctx:c1"] {
		t.Fatal("h on a folded session folds the context")
	}
	if got := bhKinds(&m); !reflect.DeepEqual(got, []string{"ctx:c1", "ctx:c2", "sess:c"}) {
		t.Fatalf("rows %v", got)
	}
	// l opens the context again
	m.view.Cursor = 0
	m.keyNormal(bhRune('l'))
	if m.view.Collapsed["ctx:c1"] {
		t.Fatal("l on a context opens it")
	}
	// l on the session opens its subs
	bhSelect(t, &m, "b")
	m.keyNormal(bhRune('l'))
	if m.view.Collapsed["sess:b"] {
		t.Fatal("l on a session opens its subagents")
	}
	// h on a session without visible subs folds the context
	bhSelect(t, &m, "a")
	m.keyNormal(bhRune('h'))
	if !m.view.Collapsed["ctx:c1"] {
		t.Fatal("h on a leaf session folds its context")
	}
	// h on a context row folds it
	m.view.Collapsed = map[string]bool{}
	m.rebuild()
	for i, r := range m.rows {
		if r.Kind == core.RowCtx && r.Ctx == "c2" {
			m.view.Cursor = i
		}
	}
	m.keyNormal(bhRune('h'))
	if !m.view.Collapsed["ctx:c2"] {
		t.Fatal("h on a context folds it")
	}
	// h on a session with subs folds the session only
	bhSelect(t, &m, "b")
	m.keyNormal(bhRune('h'))
	if !m.view.Collapsed["sess:b"] || m.view.Collapsed["ctx:c1"] {
		t.Fatal("h on a session with subs folds the session first")
	}
}

// h on a subagent row of the last context folds its session and moves the
// cursor to it.
func TestBehaviorCollapseSubInLastContext(t *testing.T) {
	m := bhModel(t, store.Session{SessionID: "b", Context: "c", Subs: []store.Sub{{ID: "s1"}}})
	m.view.Cursor = 2
	if m.cur().Kind != core.RowSub {
		t.Fatalf("rows %v", bhKinds(&m))
	}
	m.keyNormal(bhRune('h'))
	if !m.view.Collapsed["sess:b"] {
		t.Fatal("h on a subagent folds its session")
	}
	if r := m.cur(); r == nil || r.Kind != core.RowSess || r.Sess.SessionID != "b" {
		t.Fatalf("cursor should go to the parent: %v", bhKinds(&m))
	}
}

// BUG: collapse keeps r := m.cur() (a pointer into m.rows) across
// m.rebuild(), which rewrites m.rows in place. Folding a subagent whose row
// is followed by another context leaves r pointing at that context row
// (sess == nil) and r.Sess.SessionID panics.
func TestBehaviorCollapseSubFollowedByContext(t *testing.T) {
	m := bhModel(t, bhSessions()...)
	for i, r := range m.rows {
		if r.Kind == core.RowSub {
			m.view.Cursor = i
		}
	}
	m.keyNormal(bhRune('h'))
	if r := m.cur(); r == nil || r.Kind != core.RowSess || r.Sess.SessionID != "b" {
		t.Fatalf("cursor should go to the parent: %v", bhKinds(&m))
	}
}

func TestBehaviorCollapseIgnoredOutsideTree(t *testing.T) {
	m := bhModel(t, bhSessions()...)
	m.tab = tabInbox
	m.collapse()
	m.expand()
	if len(m.view.Collapsed) != 0 {
		t.Fatal("folding only works in the tree tabs")
	}
	empty := bhModel(t)
	empty.collapse()
	empty.expand()
	empty.toggle()
	if len(empty.view.Collapsed) != 0 {
		t.Fatal("nothing to fold without rows")
	}
}

func TestBehaviorToggleSessionRow(t *testing.T) {
	m := bhModel(t, bhSessions()...)
	bhSelect(t, &m, "b")
	m.toggle()
	if !m.view.Collapsed["sess:b"] {
		t.Fatal("toggle on a session flips its fold")
	}
	m.toggle()
	if m.view.Collapsed["sess:b"] {
		t.Fatal("toggle again unfolds")
	}
}

func TestBehaviorVisibleSubs(t *testing.T) {
	m := bhModel(t)
	s := &store.Session{Subs: []store.Sub{{ID: "1"}, {ID: "2", Done: true}, {ID: "3", Done: true}}}
	if n := m.visibleSubs(s); n != 1 {
		t.Fatalf("only running subs: %d", n)
	}
	m.view.ShowAll = true
	if n := m.visibleSubs(s); n != 3 {
		t.Fatalf("H shows all subs: %d", n)
	}
}

func TestBehaviorJumpAttention(t *testing.T) {
	m := bhModel(t, bhSessions()...)
	m.view.Cursor = 0
	m.keyNormal(bhRune('a'))
	if r := m.cur(); r.Sess == nil || r.Sess.SessionID != "a" {
		t.Fatalf("a jumps to the first waiting session: %v", core.RowKey(*r))
	}
	m.keyNormal(bhRune('a'))
	if r := m.cur(); r.Sess == nil || r.Sess.SessionID != "c" {
		t.Fatalf("a again goes to the next: %v", core.RowKey(*r))
	}
	m.keyNormal(bhRune('a'))
	if r := m.cur(); r.Sess.SessionID != "a" {
		t.Fatal("a wraps around")
	}
	m.keyNormal(bhRune('A'))
	if r := m.cur(); r.Sess.SessionID != "c" {
		t.Fatal("A goes backwards (wrapping)")
	}
	// from another tab, a goes back to the tree
	m.tab = tabTodos
	m.keyNormal(bhRune('a'))
	if m.tab != tabSessions {
		t.Fatal("a returns to the sessions tab")
	}
	// in the inbox it just moves
	m.tab = tabInbox
	m.view.InboxCursor = 0
	m.keyNormal(bhRune('a'))
	if m.view.InboxCursor != 1 {
		t.Fatalf("a in the inbox moves down: %d", m.view.InboxCursor)
	}
	// nothing waiting: the cursor stays
	q := bhModel(t, store.Session{SessionID: "q", Context: "x"})
	q.view.Cursor = 1
	q.jumpAttention(1)
	if q.view.Cursor != 1 {
		t.Fatal("no attention: cursor unchanged")
	}
}

func TestBehaviorFilterInput(t *testing.T) {
	m := bhModel(t, bhSessions()...)
	m.tab = tabInbox
	m.keyNormal(bhRune('/'))
	if m.mode != modeInput || m.purpose != inFilter || m.tab != tabSessions {
		t.Fatal("/ opens the filter in the sessions tab")
	}
	bhType(&m, "charl")
	if m.view.Filter != "charl" {
		t.Fatalf("typing filters live: %q", m.view.Filter)
	}
	if got := sessOrder(&m); !reflect.DeepEqual(got, []string{"c"}) {
		t.Fatalf("live filter rows: %v", got)
	}
	bhEnter(&m)
	if m.mode != modeNormal || m.view.Filter != "charl" {
		t.Fatal("enter keeps the filter")
	}
	// esc in NORMAL clears the filter
	m.keyNormal(tea.KeyPressMsg{Code: tea.KeyEscape})
	if m.view.Filter != "" || len(sessOrder(&m)) != 3 {
		t.Fatalf("esc clears the filter: %q %v", m.view.Filter, sessOrder(&m))
	}
	// esc inside the filter input drops it too
	m.keyNormal(bhRune('/'))
	bhType(&m, "alp")
	m.key(tea.KeyPressMsg{Code: tea.KeyEscape})
	if m.mode != modeNormal || m.view.Filter != "" {
		t.Fatal("esc in the filter line clears it")
	}
}

func TestBehaviorThemeKey(t *testing.T) {
	m := bhModel(t)
	start := m.th
	_, cmd := m.keyNormal(bhRune('T'))
	if m.th != (start+1)%len(theme.All) || !m.themeSet {
		t.Fatal("T cycles the theme and pins it")
	}
	if _, ok := bhHasToast(cmd, "tema: "+theme.All[m.th].Name); !ok {
		t.Fatal("T announces the theme")
	}
}

func TestBehaviorUnknownKeyNotHandled(t *testing.T) {
	m := bhModel(t, bhSessions()...)
	if ok, cmd := m.keyNormal(bhRune('Q')); ok || cmd != nil {
		t.Fatal("an unbound key is not handled")
	}
}

func TestBehaviorPaneKeysWithoutPane(t *testing.T) {
	m := bhModel(t, bhSessions()...)
	m.keyNormal(bhRune('z'))
	m.keyNormal(bhRune('w'))
	if m.zoom {
		t.Fatal("z without a pane does nothing")
	}
}

func TestBehaviorPaneKeysWithPane(t *testing.T) {
	m := bhModel(t, bhSessions()...)
	bhSelect(t, &m, "a")
	bhPane(t, &m, "aaaa1111", "a")
	m.syncActive()
	if m.active != "aaaa1111" {
		t.Fatal("the selected session's pane shows")
	}
	m.keyNormal(bhRune('z'))
	if !m.zoom {
		t.Fatal("z zooms")
	}
	if lw, _, _ := m.layout(); lw != 0 {
		t.Fatal("zoom hides the list")
	}
	m.keyNormal(bhRune('i'))
	if m.mode != modeInsert {
		t.Fatal("i focuses the pane")
	}
	// INSERT: keys go to the pane; ctrl+] leaves
	m.key(bhRune('x'))
	m.key(tea.KeyPressMsg{Code: tea.KeyPgUp, Mod: tea.ModShift})
	m.key(tea.KeyPressMsg{Code: tea.KeyPgDown, Mod: tea.ModShift})
	if m.mode != modeInsert {
		t.Fatal("still INSERT")
	}
	m.key(tea.KeyPressMsg{Code: ']', Mod: tea.ModCtrl})
	if m.mode != modeNormal {
		t.Fatal("ctrl+] leaves INSERT")
	}
	m.mode = modeInsert
	m.keyNormal(bhRune('w')) // via NORMAL handler: closes the pane
	if m.panes["aaaa1111"] != nil || m.active != "" || m.zoom || m.mode != modeNormal {
		t.Fatal("w closes the pane, the zoom and INSERT")
	}
	if m.app.Panes().PaneOf("a") != "" {
		t.Fatal("closing forgets the session's pane")
	}
}

func TestBehaviorInsertWithoutPaneFallsBack(t *testing.T) {
	m := bhModel(t, bhSessions()...)
	m.mode = modeInsert
	m.key(bhRune('x'))
	if m.mode != modeNormal {
		t.Fatal("INSERT without a pane goes back to NORMAL")
	}
}

func TestBehaviorPickOutsideTree(t *testing.T) {
	m := bhModel(t, bhSessions()...)
	m.tab = tabTodos
	m.togglePick()
	if len(m.picked) != 0 {
		t.Fatal("no picking in todos")
	}
	m.tab = tabInbox
	m.togglePick()
	if !m.picked["a"] || m.view.InboxCursor != 1 {
		t.Fatal("space in the inbox picks and moves down")
	}
	m.view.InboxCursor = 0
	m.togglePick()
	if m.picked["a"] {
		t.Fatal("space again unpicks")
	}
	empty := bhModel(t)
	empty.tab = tabInbox
	empty.togglePick()
	if len(empty.picked) != 0 {
		t.Fatal("nothing to pick")
	}
}

func TestBehaviorSelectedByTab(t *testing.T) {
	m := bhModel(t, bhSessions()...)
	m.tab = tabInbox
	if s := m.selected(); s == nil || s.SessionID != "a" {
		t.Fatal("inbox selects from the inbox")
	}
	m.view.InboxCursor = 9
	if m.selected() != nil {
		t.Fatal("out of range: nothing")
	}
	m.tab = tabTodos
	if m.selected() != nil {
		t.Fatal("todos select no session")
	}
	m.tab = tabSessions
	m.view.Cursor = 0
	if m.selected() != nil {
		t.Fatal("a context row is no session")
	}
	m.view.Cursor = -1
	if m.cur() != nil {
		t.Fatal("negative cursor: no row")
	}
}

func TestBehaviorRefreshKeyResetsOrder(t *testing.T) {
	m := bhModel(t, bhSessions()...)
	m.view.Order["sess:zzz"] = 99
	m.keyNormal(bhRune('R'))
	if _, ok := m.view.Order["sess:zzz"]; ok {
		t.Fatal("R rebuilds the frozen order from scratch")
	}
}

func TestBehaviorContextNames(t *testing.T) {
	m := bhModel(t, bhSessions()...)
	if got := m.contextNames(); !reflect.DeepEqual(got, []string{"c1", "c2"}) {
		t.Fatalf("contexts: %v", got)
	}
	bhSelect(t, &m, "a")
	m.keyNormal(bhRune('m'))
	if m.purpose != inMove || !strings.Contains(m.input.Prompt, "c1 · c2") || m.input.Value() != "c1" {
		t.Fatalf("m asks for the context: %q %q", m.input.Prompt, m.input.Value())
	}
}
