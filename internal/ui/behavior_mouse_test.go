package ui

import (
	"testing"

	tea "charm.land/bubbletea/v2"

	"github.com/aqilputi/nekuroddo/internal/core"
	"github.com/aqilputi/nekuroddo/internal/todo"
)

func bhClick(x, y int) tea.MouseClickMsg {
	return tea.MouseClickMsg{X: x, Y: y, Button: tea.MouseLeft}
}

// listY is the screen row of visible list line i (header, tabs, border).
func bhListY(m *Model, i int) int { return m.rects().left.Min.Y + 1 + i }

func TestBehaviorClickTabs(t *testing.T) {
	m := bhModel(t, bhSessions()...)
	y := m.rects().tabs.Min.Y
	for _, c := range []struct {
		x    int
		want tab
	}{
		{0, tabSessions}, {10, tabSessions}, {12, tabInbox}, {25, tabInbox},
		{27, tabTodos}, {40, tabTodos}, {42, tabArchive}, {57, tabArchive},
	} {
		m.tab = tabSessions
		if c.want == tabSessions {
			m.tab = tabArchive
		}
		m.update(bhClick(c.x, y))
		if m.tab != c.want {
			t.Errorf("click at x=%d: tab %d want %d", c.x, m.tab, c.want)
		}
	}
	m.tab = tabInbox
	m.update(bhClick(130, y))
	if m.tab != tabInbox {
		t.Fatal("a click right of the tabs changes nothing")
	}
}

func TestBehaviorClickListSelectsThenOpens(t *testing.T) {
	m := bhModel(t, bhSessions()...)
	m.view.Cursor = 0
	// row 2 is session b (interactive): first click selects
	m.update(bhClick(5, bhListY(&m, 2)))
	if r := m.cur(); r.Sess == nil || r.Sess.SessionID != "b" {
		t.Fatalf("click selects: %v", core.RowKey(*r))
	}
	// second click opens it (interactive: a toast)
	cmd := m.update(bhClick(5, bhListY(&m, 2)))
	if _, ok := bhHasToast(cmd, "pid 42"); !ok {
		t.Fatal("clicking the selected row opens it")
	}
	// below the last row, on the border, or with another button: nothing
	for _, msg := range []tea.MouseClickMsg{
		bhClick(5, bhListY(&m, 20)),
		bhClick(5, m.rects().left.Min.Y),
		{X: 5, Y: bhListY(&m, 0), Button: tea.MouseRight},
	} {
		before := m.view.Cursor
		if m.update(msg) != nil || m.view.Cursor != before {
			t.Errorf("click %+v should do nothing", msg)
		}
	}
}

func TestBehaviorWheelScrollsList(t *testing.T) {
	m := bhModel(t, bhSessions()...)
	m.view.Cursor = 0
	y := bhListY(&m, 0)
	m.update(tea.MouseWheelMsg{X: 5, Y: y, Button: tea.MouseWheelDown})
	if m.view.Cursor != 3 {
		t.Fatalf("wheel down moves 3: %d", m.view.Cursor)
	}
	m.update(tea.MouseWheelMsg{X: 5, Y: y, Button: tea.MouseWheelUp})
	if m.view.Cursor != 0 {
		t.Fatalf("wheel up moves back: %d", m.view.Cursor)
	}
	// over the right side (no pane) the wheel does nothing to the list
	m.update(tea.MouseWheelMsg{X: 100, Y: y, Button: tea.MouseWheelDown})
	if m.view.Cursor != 0 {
		t.Fatal("wheel outside the list")
	}
}

func TestBehaviorClickInbox(t *testing.T) {
	m := bhModel(t, bhSessions()...)
	m.tab = tabInbox
	m.update(bhClick(5, bhListY(&m, 4))) // second card (4 lines each)
	if m.view.InboxCursor != 1 {
		t.Fatalf("click selects the second card: %d", m.view.InboxCursor)
	}
	cmd := m.update(bhClick(5, bhListY(&m, 5)))
	if cmd == nil {
		t.Fatal("clicking the selected card opens it (c resumes)")
	}
	if m.update(bhClick(5, bhListY(&m, 12))) != nil || m.view.InboxCursor != 1 {
		t.Fatal("past the last card: nothing")
	}
}

func TestBehaviorClickTodos(t *testing.T) {
	m := bhModel(t, bhSessions()...)
	bhSetTodos(t, &m, []todo.Group{
		{Group: "g1", Tasks: []todo.Task{{ID: 1, Title: "um"}, {ID: 2, Title: "dois"}}},
		{Group: "g2", Tasks: []todo.Task{{ID: 3, Title: "três"}}},
	})
	m.rebuildTodos()
	m.tab = tabTodos
	// lines: [g1] 1 2 [g2] 3
	if m.update(bhClick(5, bhListY(&m, 0))) != nil || m.todoCur != 0 {
		t.Fatal("a group header is not a task")
	}
	m.update(bhClick(5, bhListY(&m, 4)))
	if m.todoCur != 2 {
		t.Fatalf("click selects task 3: %d", m.todoCur)
	}
	m.update(bhClick(5, bhListY(&m, 4)))
	if m.mode != modeInput || m.purpose != inDispatchDir {
		t.Fatal("clicking the selected task asks where to dispatch it")
	}
	m.mode = modeNormal
	if m.update(bhClick(5, bhListY(&m, 9))) != nil {
		t.Fatal("past the last line: nothing")
	}
}

func TestBehaviorClickDismissesDialog(t *testing.T) {
	m := bhModel(t, bhSessions()...)
	m.push(helpDialog{})
	m.update(tea.MouseWheelMsg{X: 5, Y: 5, Button: tea.MouseWheelDown})
	if m.top() == nil {
		t.Fatal("the wheel leaves the dialog")
	}
	m.update(bhClick(5, 5))
	if m.top() != nil {
		t.Fatal("a click dismisses it")
	}
}

func TestBehaviorMouseOverPane(t *testing.T) {
	m := bhModel(t, bhSessions()...)
	p := bhPane(t, &m, "p1", "a")
	m.active = "p1"
	pr := m.rects().pane
	x, y := pr.Min.X+2, pr.Min.Y+2
	m.update(tea.MouseWheelMsg{X: x, Y: y, Button: tea.MouseWheelUp})
	m.update(tea.MouseWheelMsg{X: x, Y: y, Button: tea.MouseWheelDown})
	if p.Scrolled() != 0 {
		t.Fatalf("up 3 then down 3: %d", p.Scrolled())
	}
	m.update(tea.MouseReleaseMsg{X: x, Y: y})
	m.update(tea.MouseMotionMsg{X: x, Y: y})
	if m.mode != modeNormal {
		t.Fatal("release/motion do not focus")
	}
	m.update(bhClick(x, y))
	if m.mode != modeInsert {
		t.Fatal("clicking the pane focuses it")
	}
	// clicking the list leaves INSERT
	m.update(bhClick(5, bhListY(&m, 1)))
	if m.mode != modeNormal {
		t.Fatal("clicking elsewhere leaves INSERT")
	}
}
