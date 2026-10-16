package ui

import (
	"fmt"
	"hash/fnv"
	"image/color"
	"strings"
	"time"

	tea "charm.land/bubbletea/v2"
	"charm.land/lipgloss/v2"

	"github.com/aqilputi/neko/internal/events"
	"github.com/aqilputi/neko/internal/groups"
	"github.com/aqilputi/neko/internal/store"
	"github.com/aqilputi/neko/internal/theme"
)

// Groups relate sessions doing similar work: space picks sessions (as in
// yazi), `:group <tag>` groups them, and each member learns about its peers
// through `neko hook-context`. When one warns the others (SendMessage), the
// sender and the receivers show ✉ and the cat meows.

// mailCueFor is how long the ✉ blinks; it then stays dim until mailKeep.
const (
	mailCueFor = 90 * time.Second
	mailKeep   = 15 * time.Minute
)

type mailCue struct {
	at   time.Time
	sent bool // the session sent it (→); otherwise received (←)
}

// peerMsg is one message seen between sessions, newest last.
type peerMsg struct {
	at       time.Time
	from, to string // session ids ("" when the sender is not known)
	fromName string
	text     string
}

// groupColor is a stable color per tag, from the theme's context palette
// (offset so it rarely matches the section color next to it).
func groupColor(t theme.Theme, tag string) color.Color {
	h := fnv.New32a()
	h.Write([]byte(tag))
	return t.ContextColor(int(h.Sum32()%97) + 3)
}

// togglePick is space: picks or unpicks the session under the cursor and
// moves down, like yazi. On a context row it still folds the section.
func (m *Model) togglePick() {
	if m.tab == tabTodos {
		return
	}
	if r := m.cur(); m.tab != tabInbox && r != nil && r.kind == "ctx" {
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

func member(s *store.Session) groups.Member {
	mb := groups.Member{SessionID: s.SessionID, Name: s.Name, Label: s.Label(), Cwd: s.Cwd}
	if mb.Name == "" {
		mb.Name = s.Label()
	}
	return mb
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
		tag := groups.CleanTag(strings.Join(f[1:], " "))
		return &paletteItem{title: "▶ group #" + tag, hint: what, run: func(m *Model) tea.Cmd { return m.groupTargets(tag) }}
	case "ungroup", "ug":
		tag := ""
		if len(f) > 1 {
			tag = groups.CleanTag(f[1])
		}
		title := "▶ ungroup " + what
		if tag != "" {
			title = "▶ ungroup #" + tag + " (" + what + ")"
		}
		return &paletteItem{title: title, hint: "tira do grupo", run: func(m *Model) tea.Cmd { return m.ungroupTargets(tag) }}
	case "notify", "n":
		tag, text := "", strings.TrimSpace(strings.TrimPrefix(q, f[0]))
		if len(f) > 1 && strings.HasPrefix(f[1], "#") {
			tag = groups.CleanTag(f[1])
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
	ts := m.targets()
	if tag == "" || len(ts) == 0 {
		return toastCmd("group: escolha sessões com space (ou ponha o cursor numa)", true)
	}
	var ms []groups.Member
	for _, s := range ts {
		ms = append(ms, member(s))
	}
	m.grp = groups.Load()
	m.grp.Add(tag, ms...)
	if err := m.grp.Save(); err != nil {
		return toastCmd("group: "+err.Error(), true)
	}
	m.picked = map[string]bool{}
	return toastCmd(fmt.Sprintf("#%s: %d sessões — cada uma fica sabendo dos pares na próxima ferramenta", tag, len(m.grp.Groups[tag])), false)
}

func (m *Model) ungroupTargets(tag string) tea.Cmd {
	var ids []string
	for _, s := range m.targets() {
		ids = append(ids, s.SessionID)
	}
	m.grp = groups.Load()
	n := m.grp.Remove(tag, ids...)
	if err := m.grp.Save(); err != nil {
		return toastCmd("ungroup: "+err.Error(), true)
	}
	m.picked = map[string]bool{}
	if n == 0 {
		return toastCmd("ungroup: nada a tirar", true)
	}
	return toastCmd(fmt.Sprintf("%d saíram do grupo", n), false)
}

// notifyGroup queues your note for every member; each gets it at its next
// tool call or prompt. Sessions not running only see it when they resume.
func (m *Model) notifyGroup(tag, text string) tea.Cmd {
	ms := m.grp.Groups[tag]
	if len(ms) == 0 {
		return toastCmd("notify: #"+tag+" não existe", true)
	}
	now := time.Now()
	idle := 0
	for _, mb := range ms {
		if err := groups.Queue(mb.SessionID, groups.Note{At: now, Tag: tag, Text: text}); err != nil {
			return toastCmd("notify: "+err.Error(), true)
		}
		m.mail[mb.SessionID] = mailCue{at: now}
		m.addMsg(peerMsg{at: now, to: mb.SessionID, fromName: "você", text: "[#" + tag + "] " + text})
		if s := m.session(mb.SessionID); s == nil || s.State != "busy" {
			idle++
		}
	}
	m.cat.Meow(truncate("✉ você → #"+tag, 50))
	msg := fmt.Sprintf("✉ #%s: aviso na fila de %d sessões", tag, len(ms))
	if idle > 0 {
		msg += fmt.Sprintf(" (%d paradas recebem quando voltarem a rodar)", idle)
	}
	return toastCmd(msg, false)
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
	if name == "" {
		return nil
	}
	for i := range m.snap.Sessions {
		s := &m.snap.Sessions[i]
		if s.Name == name || s.Label() == name {
			return s
		}
	}
	// names are cut with "…" in some listings
	if p, ok := strings.CutSuffix(name, "…"); ok && p != "" {
		for i := range m.snap.Sessions {
			if s := &m.snap.Sessions[i]; strings.HasPrefix(s.Name, p) {
				return s
			}
		}
	}
	return nil
}

// onPeer reacts to a message another session sent: ✉ on both ends, the
// message in the log, and the cat says who told whom.
func (m *Model) onPeer(e events.Event) {
	now := e.At
	if now.IsZero() {
		now = time.Now()
	}
	m.mail[e.SessionID] = mailCue{at: now}
	from := m.sessionNamed(e.From)
	pm := peerMsg{at: now, to: e.SessionID, fromName: e.From, text: e.Message}
	fromLabel := e.From
	if from != nil {
		pm.from = from.SessionID
		fromLabel = from.Label()
		m.mail[from.SessionID] = mailCue{at: now, sent: true}
	}
	m.addMsg(pm)
	toLabel := "?"
	if s := m.session(e.SessionID); s != nil {
		toLabel = s.Label()
	}
	m.cat.Meow(truncate("✉ "+truncate(fromLabel, 16)+" → "+truncate(toLabel, 16), 50))
}

func (m *Model) addMsg(p peerMsg) {
	m.msgs = append(m.msgs, p)
	if len(m.msgs) > 100 {
		m.msgs = m.msgs[len(m.msgs)-100:]
	}
}

// mailMark is the ✉ drawn next to a session that just sent or got a
// message: blinking at first, then dim.
func (m *Model) mailMark(t theme.Theme, id string) (string, color.Color) {
	c, ok := m.mail[id]
	if !ok {
		return "", nil
	}
	age := time.Since(c.at)
	if age > mailKeep {
		return "", nil
	}
	mark := "✉←"
	if c.sent {
		mark = "✉→"
	}
	if age < mailCueFor {
		if m.frame%6 < 3 {
			return mark, t.Accent
		}
		return mark, t.Accent2
	}
	return mark, t.Muted
}

// mailBlinking keeps the fast animation clock on while a ✉ blinks.
func (m *Model) mailBlinking() bool {
	for _, c := range m.mail {
		if time.Since(c.at) < mailCueFor {
			return true
		}
	}
	return false
}

// syncGroupNames refreshes the names kept in groups.json when Claude
// renames a session (they are the SendMessage addresses), and reloads the
// file in case another neko changed it.
func (m *Model) syncGroupNames() {
	m.grp = groups.Load()
	changed := false
	for tag, ms := range m.grp.Groups {
		for i, mb := range ms {
			if s := m.session(mb.SessionID); s != nil {
				if nm := member(s); nm.Name != mb.Name || nm.Label != mb.Label || nm.Cwd != mb.Cwd {
					ms[i], changed = nm, true
				}
			}
		}
		m.grp.Groups[tag] = ms
	}
	if changed {
		_ = m.grp.Save()
	}
}

// peersOfCursor are the sessions sharing a group with the one under the
// cursor: the tree highlights them.
func (m *Model) peersOfCursor() map[string]bool {
	sel := m.selected()
	if sel == nil {
		return nil
	}
	out := map[string]bool{}
	for _, tag := range m.grp.Of(sel.SessionID) {
		for _, mb := range m.grp.Groups[tag] {
			out[mb.SessionID] = true
		}
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
			b.Foreground(t.Subtle).Render(truncate(" com "+strings.Join(peers, " · "), max(w-len(tag)-3, 4))))
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
				b.Foreground(t.Accent2).Render(truncate(dir, 22)+" ")+
				b.Foreground(t.Text).Render(truncate(p.text, max(w-30, 8))))
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
