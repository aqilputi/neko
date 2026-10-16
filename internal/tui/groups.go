package tui

import (
	"fmt"
	"image/color"
	"strings"

	tea "charm.land/bubbletea/v2"
	"charm.land/lipgloss/v2"

	"github.com/aqilputi/nekuroddo/internal/app"
	"github.com/aqilputi/nekuroddo/internal/core"
	"github.com/aqilputi/nekuroddo/internal/store"
	"github.com/aqilputi/nekuroddo/internal/theme"
)

// Groups relate sessions doing similar work: space picks sessions (as in
// yazi), `:group <tag>` groups them, and each member learns about its peers
// through `nk hook-context`. When one warns the others (SendMessage), the
// sender and the receivers show ✉ and the cat meows. The use cases and the
// state (groups, ✉ cues, message log) live in app (app/groups.go); this
// file keeps the picks, parses the palette and draws.

// mailKeep is how long a ✉ stays (app.MailKeep; it blinks for app.MailCueFor).
const mailKeep = app.MailKeep

// groupColor is a stable color per tag, from the theme's context palette
// (offset so it rarely matches the section color next to it).
func groupColor(t theme.Theme, tag string) color.Color {
	return t.ContextColor(core.GroupColorIndex(tag))
}

// togglePick is space: picks or unpicks the session under the cursor and
// moves down, like yazi. On a context row it still folds the section.
func (m *Model) togglePick() {
	if m.tab == tabTodos {
		return
	}
	if r := m.cur(); m.tab != tabInbox && r != nil && r.Kind == core.RowCtx {
		m.toggle()
		return
	}
	sel := m.selected()
	if sel == nil {
		m.move(1)
		return
	}
	if m.picked[sel.SessionID] {
		delete(m.picked, sel.SessionID)
	} else {
		m.picked[sel.SessionID] = true
	}
	m.move(1)
}

// targets are the picked sessions, or the one under the cursor.
func (m *Model) targets() []*store.Session {
	var out []*store.Session
	if len(m.picked) > 0 {
		for i := range m.snap.Sessions {
			if m.picked[m.snap.Sessions[i].SessionID] {
				out = append(out, &m.snap.Sessions[i])
			}
		}
		return out
	}
	if sel := m.selected(); sel != nil {
		out = append(out, sel)
	}
	return out
}

// groupCtx is the client's selection as app sees it: the picked sessions
// (in tree order) and the one under the cursor.
func (m *Model) groupCtx() app.Context {
	var ctx app.Context
	if len(m.picked) > 0 {
		for _, s := range m.targets() {
			ctx.Picked = append(ctx.Picked, s.SessionID)
		}
	}
	if sel := m.selected(); sel != nil {
		ctx.SessionID = sel.SessionID
	}
	return ctx
}

// resultCmd turns a use case's Result into the toast it asks for.
func resultCmd(res app.Result, err error) tea.Cmd {
	switch {
	case res.Toast != "":
		return toastCmd(res.Toast, res.Err || err != nil)
	case err != nil:
		return toastCmd(err.Error(), true)
	}
	return nil
}

// groupCommand parses what was typed in the palette; nil when it is not a
// group command, so the palette falls back to fuzzy search.
func (m *Model) groupCommand(q string) *paletteItem {
	f := strings.Fields(q)
	if len(f) == 0 {
		return nil
	}
	n := len(m.targets())
	what := fmt.Sprintf("%d sessões", n)
	if len(m.picked) == 0 {
		what = "a sessão do cursor"
	}
	switch f[0] {
	case "group", "g":
		if len(f) < 2 {
			return &paletteItem{title: "group <tag>", hint: "agrupa " + what}
		}
		tag := core.CleanTag(strings.Join(f[1:], " "))
		return &paletteItem{title: "▶ group #" + tag, hint: what, run: func(m *Model) tea.Cmd { return m.groupTargets(tag) }}
	case "ungroup", "ug":
		tag := ""
		if len(f) > 1 {
			tag = core.CleanTag(f[1])
		}
		title := "▶ ungroup " + what
		if tag != "" {
			title = "▶ ungroup #" + tag + " (" + what + ")"
		}
		return &paletteItem{title: title, hint: "tira do grupo", run: func(m *Model) tea.Cmd { return m.ungroupTargets(tag) }}
	case "notify", "n":
		tag, text := "", strings.TrimSpace(strings.TrimPrefix(q, f[0]))
		if len(f) > 1 && strings.HasPrefix(f[1], "#") {
			tag = core.CleanTag(f[1])
			text = strings.TrimSpace(strings.TrimPrefix(text, f[1]))
		}
		if tag == "" {
			if sel := m.selected(); sel != nil {
				if tags := m.app.TagsOf(sel.SessionID); len(tags) > 0 {
					tag = tags[0]
				}
			}
		}
		if tag == "" || text == "" {
			return &paletteItem{title: "notify [#tag] <mensagem>", hint: "avisa o grupo"}
		}
		return &paletteItem{title: "▶ notify #" + tag + ": " + text, hint: fmt.Sprintf("%d sessões", len(m.app.Groups().Groups[tag])),
			run: func(m *Model) tea.Cmd { return m.notifyGroup(tag, text) }}
	}
	return nil
}

func (m *Model) groupTargets(tag string) tea.Cmd {
	res, err := m.app.GroupTargets(m.groupCtx(), tag)
	if err == nil {
		m.picked = map[string]bool{}
	}
	return resultCmd(res, err)
}

func (m *Model) ungroupTargets(tag string) tea.Cmd {
	res, err := m.app.UngroupTargets(m.groupCtx(), tag)
	if err == nil {
		m.picked = map[string]bool{}
	}
	return resultCmd(res, err)
}

// notifyGroup queues your note for every member (app.NotifyGroup) and the
// cat tells you so.
func (m *Model) notifyGroup(tag, text string) tea.Cmd {
	res, err := m.app.NotifyGroup(tag, text)
	if err == nil {
		m.cat.Meow(core.Truncate("✉ você → #"+tag, 50))
	}
	return resultCmd(res, err)
}

func (m *Model) session(id string) *store.Session {
	for i := range m.snap.Sessions {
		if m.snap.Sessions[i].SessionID == id {
			return &m.snap.Sessions[i]
		}
	}
	return nil
}

// sessionNamed finds the sender of a message by the name Claude gave it.
func (m *Model) sessionNamed(name string) *store.Session {
	return app.SessionNamed(m.snap.Sessions, name)
}

// mailMark is the ✉ drawn next to a session that just sent or got a
// message: blinking at first, then dim (app.MailCue.State, by app's clock).
func (m *Model) mailMark(t theme.Theme, id string) (string, color.Color) {
	c, ok := m.app.MailCueOf(id)
	if !ok {
		return "", nil
	}
	mark := "✉←"
	if c.Sent {
		mark = "✉→"
	}
	switch c.State(m.app.Clock().Now()) {
	case app.CueBlink:
		if m.frame%6 < 3 {
			return mark, t.Accent
		}
		return mark, t.Accent2
	case app.CueDim:
		return mark, t.Muted
	}
	return "", nil
}

// mailBlinking keeps the fast animation clock on while a ✉ blinks.
func (m *Model) mailBlinking() bool { return m.app.MailBlinking() }

// peersOfCursor are the sessions sharing a group with the one under the
// cursor: the tree highlights them.
func (m *Model) peersOfCursor() map[string]bool {
	sel := m.selected()
	if sel == nil {
		return nil
	}
	out := map[string]bool{}
	for _, id := range m.app.Peers(sel.SessionID) {
		out[id] = true
	}
	return out
}

// groupsDetail is the "grupos" part of a session's details: peers and the
// last messages it was part of.
func (m *Model) groupsDetail(t theme.Theme, s *store.Session, w int) []string {
	grp := m.app.Groups()
	var lines []string
	b := lipgloss.NewStyle().Background(t.Bg)
	for _, tag := range grp.Of(s.SessionID) {
		gc := groupColor(t, tag)
		var peers []string
		for _, mb := range grp.Groups[tag] {
			if mb.SessionID != s.SessionID {
				l := mb.Label
				if l == "" {
					l = mb.Name
				}
				peers = append(peers, l)
			}
		}
		lines = append(lines, b.Foreground(gc).Bold(true).Render("▍#"+tag)+
			b.Foreground(t.Subtle).Render(core.Truncate(" com "+strings.Join(peers, " · "), max(w-len(tag)-3, 4))))
	}
	var mine []app.PeerMsg
	for _, p := range m.app.Messages() {
		if p.To == s.SessionID || p.From == s.SessionID {
			mine = append(mine, p)
		}
	}
	if len(mine) > 5 {
		mine = mine[len(mine)-5:]
	}
	if len(mine) > 0 {
		lines = append(lines, "", b.Foreground(t.Accent).Bold(true).Render("✉ mensagens"))
		for _, p := range mine {
			dir := "← " + p.FromName
			if p.From == s.SessionID {
				dir = "→ " + orDash(m.labelOf(p.To))
			}
			lines = append(lines, b.Foreground(t.Muted).Render(p.At.Format("15:04")+" ")+
				b.Foreground(t.Accent2).Render(core.Truncate(dir, 22)+" ")+
				b.Foreground(t.Text).Render(core.Truncate(p.Text, max(w-30, 8))))
		}
	}
	return lines
}

func (m *Model) labelOf(id string) string {
	if s := m.session(id); s != nil {
		return s.Label()
	}
	return ""
}

// groupsSlice is the groups slice's own state: this client's picks (the
// groups, ✉ cues and messages are the core's).
type groupsSlice struct {
	picked map[string]bool // sessions picked with space
}

func newGroupsSlice() groupsSlice { return groupsSlice{picked: map[string]bool{}} }
