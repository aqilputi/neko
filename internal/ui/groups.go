package ui

import (
	"fmt"
	"image/color"
	"strings"
	"time"

	tea "charm.land/bubbletea/v2"
	"charm.land/lipgloss/v2"

	"github.com/aqilputi/nekuroddo/internal/app"
	"github.com/aqilputi/nekuroddo/internal/core"
	"github.com/aqilputi/nekuroddo/internal/events"
	"github.com/aqilputi/nekuroddo/internal/groups"
	"github.com/aqilputi/nekuroddo/internal/store"
	"github.com/aqilputi/nekuroddo/internal/theme"
)

// Groups relate sessions doing similar work: space picks sessions (as in
// yazi), `:group <tag>` groups them, and each member learns about its peers
// through `nk hook-context`. When one warns the others (SendMessage), the
// sender and the receivers show ✉ and the cat meows. The use cases live in
// app (app/groups.go); this file keeps the picks, parses the palette and
// draws.

// mailKeep is how long a ✉ stays (app.MailKeep; it blinks for app.MailCueFor).
const mailKeep = app.MailKeep

// mailCue mirrors app.MailCue for the renderers.
type mailCue struct {
	at   time.Time
	sent bool // the session sent it (→); otherwise received (←)
}

// peerMsg mirrors app.PeerMsg (one message seen between sessions).
type peerMsg struct {
	at       time.Time
	from, to string // session ids ("" when the sender is not known)
	fromName string
	text     string
}

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

// pullGroups refreshes the slice's read-only mirrors of app's groups, ✉
// cues and messages (the renderers read them; app owns the state).
func (m *Model) pullGroups() {
	m.grp = m.app.Groups()
	m.mail = map[string]mailCue{}
	for id, c := range m.app.Mail() {
		m.mail[id] = mailCue{at: c.At, sent: c.Sent}
	}
	m.msgs = nil
	for _, p := range m.app.Messages() {
		m.msgs = append(m.msgs, peerMsg{at: p.At, from: p.From, to: p.To, fromName: p.FromName, text: p.Text})
	}
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
				if tags := m.grp.Of(sel.SessionID); len(tags) > 0 {
					tag = tags[0]
				}
			}
		}
		if tag == "" || text == "" {
			return &paletteItem{title: "notify [#tag] <mensagem>", hint: "avisa o grupo"}
		}
		return &paletteItem{title: "▶ notify #" + tag + ": " + text, hint: fmt.Sprintf("%d sessões", len(m.grp.Groups[tag])),
			run: func(m *Model) tea.Cmd { return m.notifyGroup(tag, text) }}
	}
	return nil
}

func (m *Model) groupTargets(tag string) tea.Cmd {
	res, err := m.app.GroupTargets(m.groupCtx(), tag, m.snap.Sessions...)
	m.pullGroups()
	if err == nil {
		m.picked = map[string]bool{}
	}
	return resultCmd(res, err)
}

func (m *Model) ungroupTargets(tag string) tea.Cmd {
	res, err := m.app.UngroupTargets(m.groupCtx(), tag)
	m.pullGroups()
	if err == nil {
		m.picked = map[string]bool{}
	}
	return resultCmd(res, err)
}

// notifyGroup queues your note for every member (app.NotifyGroup) and the
// cat tells you so.
func (m *Model) notifyGroup(tag, text string) tea.Cmd {
	res, err := m.app.NotifyGroup(tag, text, m.snap.Sessions...)
	m.pullGroups()
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

// onPeer reacts to a message another session sent: app records the ✉ on
// both ends and the message; the cat says who told whom.
func (m *Model) onPeer(e events.Event) {
	seen := m.app.OnPeer(e, m.snap.Sessions...)
	m.pullGroups()
	m.cat.Meow(core.Truncate("✉ "+core.Truncate(seen.FromLabel, 16)+" → "+core.Truncate(seen.ToLabel, 16), 50))
}

// addMsg appends to the message mirror (app.Messages is the real log).
func (m *Model) addMsg(p peerMsg) {
	m.msgs = append(m.msgs, p)
	if len(m.msgs) > 100 {
		m.msgs = m.msgs[len(m.msgs)-100:]
	}
}

// mailMark is the ✉ drawn next to a session that just sent or got a
// message: blinking at first, then dim (app.MailCue.State, by app's clock).
func (m *Model) mailMark(t theme.Theme, id string) (string, color.Color) {
	c, ok := m.mail[id]
	if !ok {
		return "", nil
	}
	mark := "✉←"
	if c.sent {
		mark = "✉→"
	}
	switch (app.MailCue{At: c.at, Sent: c.sent}).State(m.app.Clock().Now()) {
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
func (m *Model) mailBlinking() bool {
	now := m.app.Clock().Now()
	for _, c := range m.mail {
		if (app.MailCue{At: c.at, Sent: c.sent}).State(now) == app.CueBlink {
			return true
		}
	}
	return false
}

// syncGroupNames refreshes the names kept in groups.json when Claude
// renames a session (they are the SendMessage addresses), and reloads the
// file in case another nekuroddo changed it (app.SyncGroupNames).
func (m *Model) syncGroupNames() {
	_, _ = m.app.SyncGroupNames(m.snap.Sessions)
	m.pullGroups()
}

// peersOfCursor are the sessions sharing a group with the one under the
// cursor: the tree highlights them.
func (m *Model) peersOfCursor() map[string]bool {
	sel := m.selected()
	if sel == nil {
		return nil
	}
	out := map[string]bool{}
	for _, id := range app.PeersIn(m.grp, sel.SessionID) {
		out[id] = true
	}
	return out
}

// groupsDetail is the "grupos" part of a session's details: peers and the
// last messages it was part of.
func (m *Model) groupsDetail(t theme.Theme, s *store.Session, w int) []string {
	tags := m.grp.Of(s.SessionID)
	var lines []string
	b := lipgloss.NewStyle().Background(t.Bg)
	for _, tag := range tags {
		gc := groupColor(t, tag)
		var peers []string
		for _, mb := range m.grp.Groups[tag] {
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
	var mine []peerMsg
	for _, p := range m.msgs {
		if p.to == s.SessionID || p.from == s.SessionID {
			mine = append(mine, p)
		}
	}
	if len(mine) > 5 {
		mine = mine[len(mine)-5:]
	}
	if len(mine) > 0 {
		lines = append(lines, "", b.Foreground(t.Accent).Bold(true).Render("✉ mensagens"))
		for _, p := range mine {
			dir := "← " + p.fromName
			if p.from == s.SessionID {
				dir = "→ " + orDash(m.labelOf(p.to))
			}
			lines = append(lines, b.Foreground(t.Muted).Render(p.at.Format("15:04")+" ")+
				b.Foreground(t.Accent2).Render(core.Truncate(dir, 22)+" ")+
				b.Foreground(t.Text).Render(core.Truncate(p.text, max(w-30, 8))))
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

// groupsSlice is the groups slice's state. picked is this client's own
// selection; grp, mail and msgs are read-only mirrors of app's state
// (app.Groups/Mail/Messages), refreshed by pullGroups after each use case.
// They stay only because code and tests outside this slice still read the
// fields (WP-2.7 drops them).
type groupsSlice struct {
	picked map[string]bool // sessions picked with space
	grp    groups.File
	mail   map[string]mailCue // ✉ next to sessions that sent/got a message
	msgs   []peerMsg
}

func newGroupsSlice() groupsSlice {
	return groupsSlice{picked: map[string]bool{}, grp: groups.File{Groups: map[string][]groups.Member{}}, mail: map[string]mailCue{}}
}
