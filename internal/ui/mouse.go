package ui

import (
	"image"

	tea "charm.land/bubbletea/v2"
	"charm.land/lipgloss/v2"
	uv "github.com/charmbracelet/ultraviolet"
)

// mouse: click selects (a second click on the same row opens it), the wheel
// scrolls the list, and in INSERT everything over the pane goes to claude.
func (m *Model) mouse(msg tea.MouseMsg) tea.Cmd {
	ev := msg.Mouse()
	pt := image.Pt(ev.X, ev.Y)
	r := m.rects()

	if d := m.top(); d != nil {
		if _, ok := msg.(tea.MouseClickMsg); ok {
			m.pop() // a click dismisses the dialog, like esc
		}
		return nil
	}

	// Over the embedded claude. When it asked for mouse reporting (Claude
	// Code's fullscreen renderer does) every event goes to it, focused or not:
	// it scrolls its own chat and selects text inside it (copies come back as
	// OSC 52). Otherwise the wheel scrolls neko's view of the scrollback.
	if p := m.panes[m.active]; p != nil && pt.In(r.pane) {
		local := uv.Mouse{X: ev.X - r.pane.Min.X, Y: ev.Y - r.pane.Min.Y, Button: uv.MouseButton(ev.Button), Mod: uv.KeyMod(ev.Mod)}
		wants := p.WantsMouse()
		switch msg.(type) {
		case tea.MouseClickMsg:
			m.mode = modeInsert // clicking the pane focuses it
			if wants {
				p.SendMouse(uv.MouseClickEvent(local))
			}
		case tea.MouseReleaseMsg:
			if wants {
				p.SendMouse(uv.MouseReleaseEvent(local))
			}
		case tea.MouseMotionMsg:
			if wants {
				p.SendMouse(uv.MouseMotionEvent(local))
			}
		case tea.MouseWheelMsg:
			switch {
			case wants:
				p.SendMouse(uv.MouseWheelEvent(local))
			case ev.Button == tea.MouseWheelUp:
				p.Scroll(3)
			case ev.Button == tea.MouseWheelDown:
				p.Scroll(-3)
			}
		}
		return nil
	}

	switch msg := msg.(type) {
	case tea.MouseWheelMsg:
		if pt.In(r.left) {
			switch msg.Button {
			case tea.MouseWheelUp:
				m.move(-3)
			case tea.MouseWheelDown:
				m.move(3)
			}
		}
	case tea.MouseClickMsg:
		if msg.Button != tea.MouseLeft {
			return nil
		}
		if pt.In(r.tabs) {
			m.clickTab(ev.X)
			return nil
		}
		if m.mode == modeInsert && !pt.In(r.pane) {
			m.mode = modeNormal // clicking elsewhere leaves INSERT
		}
		if pt.In(r.left) {
			return m.clickList(ev.Y - r.left.Min.Y - 1) // -1: border
		}
	}
	return nil
}

// clickTab maps x to the tab under it (labels are " 1 Sessões " + " " …).
func (m *Model) clickTab(x int) {
	labels := []string{"1 Sessões", "2 Inbox (00)", "3 Todos (00)", "4 Arquivo (00)"}
	pos := 0
	for i, l := range labels {
		w := lipgloss.Width(l) + 2
		if x < pos+w {
			m.tab = tab(i)
			return
		}
		pos += w + 1
	}
}

// clickList selects the row at visible line y; clicking the selected row
// again opens it (same as enter).
func (m *Model) clickList(y int) tea.Cmd {
	_, _, bh := m.layout()
	h := bh - 2
	if y < 0 || y >= h {
		return nil
	}
	idx := -1
	switch m.tab {
	case tabSessions, tabArchive:
		start, _ := scroll(m.cursor, len(m.rows), h)
		idx = start + y
		if idx >= len(m.rows) {
			return nil
		}
		if idx == m.cursor {
			_, cmd := m.keyNormal(keyPress("enter"))
			return cmd
		}
		m.cursor = idx
	case tabInbox:
		start, _ := scroll(m.inboxCur, len(m.inbox), max(h/4, 1))
		idx = start + y/4
		if idx >= len(m.inbox) {
			return nil
		}
		if idx == m.inboxCur {
			_, cmd := m.keyNormal(keyPress("enter"))
			return cmd
		}
		m.inboxCur = idx
	case tabTodos:
		lines := m.todoLineIdx()
		cur := 0
		for i, v := range lines {
			if v == m.todoCur {
				cur = i
			}
		}
		start, _ := scroll(cur, len(lines), h)
		if start+y >= len(lines) || lines[start+y] < 0 {
			return nil
		}
		idx = lines[start+y]
		if idx == m.todoCur {
			_, cmd := m.keyNormal(keyPress("enter"))
			return cmd
		}
		m.todoCur = idx
	}
	return nil
}

// todoLineIdx mirrors todoView's layout: the todo index of each line, -1
// for group headers.
func (m *Model) todoLineIdx() []int {
	var out []int
	group := ""
	for i, r := range m.todos {
		if r.group != group {
			group = r.group
			out = append(out, -1)
		}
		out = append(out, i)
	}
	return out
}
