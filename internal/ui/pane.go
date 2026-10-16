package ui

// Slice "panes": the embedded `claude attach` panes, replies typed into
// them and fullscreen. The decisions live in app (app.PaneManager: which
// pane a session maps to, reuse, resume, pending replies; app.Fullscreen);
// this side starts the term.Pane an app.AttachPlan describes — it needs the
// Screen half to draw it — reports back, and keeps the presentation state.

import (
	"fmt"
	"os/exec"
	"time"

	tea "charm.land/bubbletea/v2"

	"github.com/aqilputi/nekuroddo/internal/app"
	"github.com/aqilputi/nekuroddo/internal/core"
	"github.com/aqilputi/nekuroddo/internal/store"
	"github.com/aqilputi/nekuroddo/internal/term"
)

func (m *Model) paneSize() (int, int) {
	r := m.rects().pane
	return r.Dx(), r.Dy()
}

// paneAlive is the client's word on whether pane id still runs.
func (m *Model) paneAlive(id string) bool {
	p := m.panes[id]
	return p != nil && !p.Done()
}

// syncPaneMirror copies app's pane table into paneOf/pending (read-only
// mirrors, kept until WP-2.7 removes them).
func (m *Model) syncPaneMirror() {
	pm := m.app.Panes()
	m.paneOf, m.pending = pm.Bindings(), pm.Replies()
}

// openPane shows the session in the embedded pane as app plans it: reuse
// the open pane, attach a background session, or resume a history one into
// the daemon first (off the UI thread).
func (m *Model) openPane(s store.Session, focus bool, reply string) tea.Cmd {
	pm := m.app.Panes()
	plan, err := pm.Open(s, focus, reply, m.paneAlive)
	if err != nil {
		return toastCmd(err.Error(), true)
	}
	switch {
	case plan.Reuse:
		m.syncPaneMirror()
		m.active = plan.ID
		if focus {
			m.mode = modeInsert
		}
		if plan.Reply != "" {
			return m.sendReplyLater(plan.ID, plan.ReplyIn)
		}
		return nil
	case plan.Resume:
		return func() tea.Msg {
			id, err := pm.Resume(plan.Dir, plan.SessionID)
			return paneOpenMsg{sess: s, id: id, err: err, text: reply}
		}
	}
	return m.startPane(plan)
}

// startPane runs the plan's program in a new pane and tells app it started.
func (m *Model) startPane(plan app.AttachPlan) tea.Cmd {
	w, h := m.paneSize()
	p, err := term.Start(plan.ID, plan.Title, plan.Dir, w, h, plan.Argv...)
	if err != nil {
		return toastCmd("attach: "+err.Error(), true)
	}
	m.panes[plan.ID] = p
	m.active = plan.ID
	if plan.Focus {
		m.mode = modeInsert
	}
	m.app.Panes().Started(plan)
	m.syncPaneMirror()
	cmds := []tea.Cmd{listenPane(p)}
	if plan.Reply != "" {
		cmds = append(cmds, m.sendReplyLater(plan.ID, plan.ReplyIn))
	}
	return tea.Batch(cmds...)
}

func (m *Model) sendReplyLater(id string, d time.Duration) tea.Cmd {
	return tea.Tick(d, func(time.Time) tea.Msg { return replyMsg{id} })
}

// syncActive makes the right side follow the selection (core.ActivePane).
// Panes that are not shown keep running hidden.
func (m *Model) syncActive() {
	var sid, short string
	if sel := m.selected(); sel != nil {
		sid, short = sel.SessionID, sel.ShortID
	}
	alive := make(map[string]bool, len(m.panes))
	for id := range m.panes {
		if m.paneAlive(id) {
			alive[id] = true
		}
	}
	m.active = core.ActivePane(m.active, sid, short, m.paneOf, alive, m.mode == modeInsert, m.zoom)
}

func (m *Model) closePane(id string) {
	if p := m.panes[id]; p != nil {
		p.Close()
		delete(m.panes, id)
	}
	m.app.Panes().Close(id)
	m.syncPaneMirror()
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

// fullscreen hands the whole terminal to what app.Fullscreen asks for
// (`claude attach`, or resume), and comes back when it exits — ← inside
// claude returns here.
func (m *Model) fullscreen(s store.Session) tea.Cmd {
	res, err := m.app.Fullscreen(s)
	if err != nil {
		return toastCmd(err.Error(), true)
	}
	if s.ShortID != "" {
		m.closePane(s.ShortID)
	}
	c := exec.Command(res.Exec.Argv[0], res.Exec.Argv[1:]...)
	c.Dir = res.Exec.Dir
	return tea.ExecProcess(c, func(err error) tea.Msg { return execDoneMsg{err} })
}

func (m *Model) onPaneOpen(msg paneOpenMsg) tea.Cmd {
	if msg.err != nil {
		return toastCmd(msg.err.Error(), true)
	}
	plan := m.app.Panes().Attach(msg.sess, msg.id, true, msg.text)
	return tea.Batch(m.startPane(plan), m.refresh())
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
	m.app.Panes().Exited(msg.ID, msg.Err)
	m.closePane(msg.ID)
	return m.refresh()
}

func (m *Model) onReply(msg replyMsg) tea.Cmd {
	if p := m.panes[msg.pane]; p != nil {
		if text, ok := m.app.Panes().TakeReply(msg.pane); ok && text != "" {
			m.syncPaneMirror()
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

// paneSlice is the embedded claude panes: presentation only. paneOf and
// pending mirror app.PaneManager (syncPaneMirror) for the tests until WP-2.7.
type paneSlice struct {
	panes   map[string]*term.Pane
	active  string // pane id shown on the right (follows the selection, see syncActive)
	zoom    bool
	paneOf  map[string]string // mirror: sessionId → pane id
	pending map[string]string // mirror: pane id → reply to type once it is ready
}

func newPaneSlice() paneSlice {
	return paneSlice{panes: map[string]*term.Pane{}, paneOf: map[string]string{}, pending: map[string]string{}}
}
