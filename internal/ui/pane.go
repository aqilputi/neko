package ui

// Slice "panes": the embedded `claude attach` panes, replies typed into
// them and fullscreen (WP-2.4 moves the logic to app/panes.go).

import (
	"fmt"
	"os/exec"
	"time"

	tea "charm.land/bubbletea/v2"

	"github.com/aqilputi/nekuroddo/internal/claude"
	"github.com/aqilputi/nekuroddo/internal/store"
	"github.com/aqilputi/nekuroddo/internal/term"
)

func (m *Model) paneSize() (int, int) {
	r := m.rects().pane
	return r.Dx(), r.Dy()
}

// openPane attaches the session in the embedded pane, resuming it into the
// daemon first when it is not a background session.
func (m *Model) openPane(s store.Session, focus bool, reply string) tea.Cmd {
	if s.Kind == "interactive" {
		return toastCmd(fmt.Sprintf("sessão interativa aberta em outro terminal (pid %d) — use o terminal dela", s.PID), true)
	}
	// already open (history sessions keep the pane under their new short id)
	if id := m.paneOf[s.SessionID]; id != "" {
		if p := m.panes[id]; p != nil && !p.Done() {
			m.active = id
			if focus {
				m.mode = modeInsert
			}
			if reply != "" {
				m.pending[id] = reply
				return m.sendReplyLater(id, 50*time.Millisecond)
			}
			return nil
		}
	}
	if s.ShortID != "" {
		return m.startPane(s, s.ShortID, focus, reply)
	}
	sess := s
	return func() tea.Msg {
		id, err := claude.ResumeInBackground(sess.Cwd, sess.SessionID)
		return paneOpenMsg{sess: sess, id: id, err: err, text: reply}
	}
}

func (m *Model) startPane(s store.Session, id string, focus bool, reply string) tea.Cmd {
	w, h := m.paneSize()
	p, err := term.Start(id, s.Label(), s.Cwd, w, h, claude.AttachArgv(id)...)
	if err != nil {
		return toastCmd("attach: "+err.Error(), true)
	}
	m.panes[id] = p
	m.paneOf[s.SessionID] = id
	m.active = id
	if focus {
		m.mode = modeInsert
	}
	m.store.MarkSeen(s.SessionID, time.Now())
	cmds := []tea.Cmd{listenPane(p)}
	if reply != "" {
		m.pending[id] = reply
		cmds = append(cmds, m.sendReplyLater(id, 1500*time.Millisecond))
	}
	return tea.Batch(cmds...)
}

func (m *Model) sendReplyLater(id string, d time.Duration) tea.Cmd {
	return tea.Tick(d, func(time.Time) tea.Msg { return replyMsg{id} })
}

// syncActive makes the right side follow the selection: the selected
// session's pane if it has one open, otherwise its details. Panes that are
// not shown keep running hidden. In INSERT or zoom the pane stays put.
func (m *Model) syncActive() {
	if m.mode == modeInsert || (m.zoom && m.active != "") {
		return
	}
	m.active = ""
	if sel := m.selected(); sel != nil {
		id := m.paneOf[sel.SessionID]
		if id == "" {
			id = sel.ShortID
		}
		if p := m.panes[id]; p != nil && !p.Done() {
			m.active = id
		}
	}
}

func (m *Model) closePane(id string) {
	if p := m.panes[id]; p != nil {
		p.Close()
		delete(m.panes, id)
	}
	for sid, pid := range m.paneOf {
		if pid == id {
			delete(m.paneOf, sid)
		}
	}
	if m.active == id {
		m.active = ""
		m.zoom = false
		if m.mode == modeInsert {
			m.mode = modeNormal
		}
	}
}

func (m *Model) resizePanes() {
	w, h := m.paneSize()
	for _, p := range m.panes {
		p.Resize(w, h)
	}
}

// fullscreen hands the whole terminal to `claude attach` (or resume), and
// comes back when it exits — ← inside claude returns here.
func (m *Model) fullscreen(s store.Session) tea.Cmd {
	var c *exec.Cmd
	switch {
	case s.ShortID != "":
		c = exec.Command(claude.Bin(), "attach", s.ShortID)
	case s.Kind == "history":
		c = exec.Command(claude.Bin(), claude.AllowBypass, "--resume", s.SessionID)
	default:
		return toastCmd("sessão interativa aberta em outro terminal", true)
	}
	c.Dir = s.Cwd
	if s.ShortID != "" {
		m.closePane(s.ShortID)
	}
	m.store.MarkSeen(s.SessionID, time.Now())
	return tea.ExecProcess(c, func(err error) tea.Msg { return execDoneMsg{err} })
}

func (m *Model) onPaneOpen(msg paneOpenMsg) tea.Cmd {
	if msg.err != nil {
		return toastCmd(msg.err.Error(), true)
	}
	s := msg.sess
	s.ShortID = msg.id
	return tea.Batch(m.startPane(s, msg.id, true, msg.text), m.refresh())
}

func (m *Model) onPaneOutput(msg paneOutputMsg) tea.Cmd {
	if p := m.panes[msg.ID]; p != nil {
		cmds := []tea.Cmd{listenPane(p)}
		// claude copied something (OSC 52): hand it to the real terminal
		for _, c := range p.TakeClipboard() {
			cmds = append(cmds, tea.SetClipboard(c), toastCmd(fmt.Sprintf("copiado (%d caracteres)", len([]rune(c))), false))
		}
		return tea.Batch(cmds...)
	}
	return nil
}

func (m *Model) onPaneExit(msg paneExitMsg) tea.Cmd {
	// `claude attach` returned (← in claude, or the session ended).
	m.closePane(msg.ID)
	return m.refresh()
}

func (m *Model) onReply(msg replyMsg) tea.Cmd {
	if p := m.panes[msg.pane]; p != nil {
		if text := m.pending[msg.pane]; text != "" {
			delete(m.pending, msg.pane)
			p.Paste(text)
			return tea.Tick(150*time.Millisecond, func(time.Time) tea.Msg { return enterMsg(msg) })
		}
	}
	return nil
}

func (m *Model) onEnter(msg enterMsg) tea.Cmd {
	if p := m.panes[msg.pane]; p != nil {
		p.Type("\r")
	}
	return toastCmd("resposta enviada ✓", false)
}

// paneSlice is the embedded claude panes.
type paneSlice struct {
	panes   map[string]*term.Pane
	paneOf  map[string]string // sessionId → pane id (history sessions get a new short id)
	active  string            // pane id shown on the right (follows the selection, see syncActive)
	zoom    bool
	pending map[string]string // pane id → reply to type once it is ready
}

func newPaneSlice() paneSlice {
	return paneSlice{panes: map[string]*term.Pane{}, paneOf: map[string]string{}, pending: map[string]string{}}
}
