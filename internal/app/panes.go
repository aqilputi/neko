package app

// Panes: which pane shows which session, replies waiting to be typed into a
// pane, and fullscreen. The pane itself (PTY + emulator) belongs to the
// client that draws it — the TUI needs term's Screen half, which app must
// not import — so the use cases here hand the client an AttachPlan to start
// and the client reports back (Started, Exited, Close).

import (
	"fmt"
	"sync"
	"time"

	"github.com/aqilputi/nekuroddo/internal/claude"
	"github.com/aqilputi/nekuroddo/internal/core"
	"github.com/aqilputi/nekuroddo/internal/store"
)

// Pane events (plan §2.5).
const (
	EvPaneOpened = "pane.opened"
	EvPaneClosed = "pane.closed"
	EvPaneExited = "pane.exited"
)

// How long after showing a pane its pending reply is typed: a pane already
// running takes it at once; a fresh `claude attach` needs time to draw.
const (
	ReplyDelayReused = 50 * time.Millisecond
	ReplyDelayNew    = 1500 * time.Millisecond
)

// PaneEvent is the payload of pane.opened, pane.closed and pane.exited.
type PaneEvent struct {
	ID        string `json:"id"`
	SessionID string `json:"session_id,omitempty"`
	Err       string `json:"err,omitempty"`
}

// AttachPlan tells a client how to show a session in a pane.
type AttachPlan struct {
	SessionID string        `json:"session_id"`
	ID        string        `json:"id,omitempty"` // pane id ("" until a history session is resumed)
	Title     string        `json:"title,omitempty"`
	Dir       string        `json:"dir"`
	Argv      []string      `json:"argv,omitempty"`   // what runs in the pane (claude attach <id>)
	Reuse     bool          `json:"reuse,omitempty"`  // the pane is open: just show it
	Resume    bool          `json:"resume,omitempty"` // history: run PaneManager.Resume, then Attach
	Focus     bool          `json:"focus,omitempty"`  // keys go to the pane
	Reply     string        `json:"reply,omitempty"`  // text to type once the pane is ready
	ReplyIn   time.Duration `json:"reply_in,omitempty"`
}

// PendingReply is a reply waiting to be typed into a pane.
type PendingReply struct {
	Text string    `json:"text"`
	Due  time.Time `json:"due"`
}

// PaneManager is the core's table of panes: session → pane id, panes the
// clients started, and replies to type. Safe for concurrent use.
type PaneManager struct {
	a *App

	mu      sync.Mutex
	of      map[string]string // session id → pane id (history sessions get a new short id)
	open    map[string]string // pane id → session id, for panes a client started
	replies map[string]PendingReply
}

func newPaneManager(a *App) *PaneManager {
	return &PaneManager{a: a, of: map[string]string{}, open: map[string]string{}, replies: map[string]PendingReply{}}
}

// Panes is the core's pane table (one per App).
func (a *App) Panes() *PaneManager { return a.panes }

// Open decides how to show s: reuse its pane if alive, attach it if it runs
// in the background, or resume it first if it is history. alive is the
// client's word on whether a pane id still runs; nil means "started here and
// not closed". Reusing sets the reply pending at once; for a new pane it is
// set by Started.
func (pm *PaneManager) Open(s store.Session, focus bool, reply string, alive func(id string) bool) (AttachPlan, error) {
	if s.Kind == "interactive" {
		return AttachPlan{}, fmt.Errorf("sessão interativa aberta em outro terminal (pid %d) — use o terminal dela", s.PID)
	}
	pm.mu.Lock()
	defer pm.mu.Unlock()
	if alive == nil {
		alive = func(id string) bool { _, ok := pm.open[id]; return ok }
	}
	id := pm.of[s.SessionID]
	if id == "" {
		id = s.ShortID
	}
	if id != "" && alive(id) {
		pm.of[s.SessionID] = id
		plan := AttachPlan{SessionID: s.SessionID, ID: id, Title: s.Label(), Dir: s.Cwd, Reuse: true, Focus: focus}
		if reply != "" {
			plan.Reply, plan.ReplyIn = reply, ReplyDelayReused
			pm.replies[id] = PendingReply{Text: reply, Due: pm.a.clock.Now().Add(ReplyDelayReused)}
		}
		return plan, nil
	}
	if s.ShortID != "" {
		return pm.Attach(s, s.ShortID, focus, reply), nil
	}
	return AttachPlan{SessionID: s.SessionID, Title: s.Label(), Dir: s.Cwd, Resume: true, Focus: focus, Reply: reply}, nil
}

// Attach is the plan for a new pane running `claude attach id` for s.
func (pm *PaneManager) Attach(s store.Session, id string, focus bool, reply string) AttachPlan {
	plan := AttachPlan{SessionID: s.SessionID, ID: id, Title: s.Label(), Dir: s.Cwd,
		Argv: pm.a.ports.Claude.AttachArgv(id), Focus: focus}
	if reply != "" {
		plan.Reply, plan.ReplyIn = reply, ReplyDelayNew
	}
	return plan
}

// Resume puts a history session into the background daemon and returns its
// new short id. It runs claude: call it off any UI thread.
func (pm *PaneManager) Resume(dir, sessionID string) (string, error) {
	return pm.a.ports.Claude.ResumeInBackground(dir, sessionID)
}

// Started records that a client started plan's pane: the session is bound to
// it, marked seen, its reply set pending, and pane.opened published.
func (pm *PaneManager) Started(plan AttachPlan) {
	now := pm.a.clock.Now()
	pm.mu.Lock()
	pm.of[plan.SessionID] = plan.ID
	pm.open[plan.ID] = plan.SessionID
	if plan.Reply != "" {
		pm.replies[plan.ID] = PendingReply{Text: plan.Reply, Due: now.Add(plan.ReplyIn)}
	}
	pm.mu.Unlock()
	pm.a.ports.Sessions.MarkSeen(plan.SessionID, now)
	pm.a.Publish(EvPaneOpened, PaneEvent{ID: plan.ID, SessionID: plan.SessionID})
}

// forget drops pane id; it returns its session and whether it was known.
func (pm *PaneManager) forget(id string) (string, bool) {
	pm.mu.Lock()
	defer pm.mu.Unlock()
	sid, known := pm.open[id]
	delete(pm.open, id)
	delete(pm.replies, id)
	for s, p := range pm.of {
		if p == id {
			sid, known = s, true
			delete(pm.of, s)
		}
	}
	return sid, known
}

// Close forgets pane id (the client closed it) and publishes pane.closed if
// it was known.
func (pm *PaneManager) Close(id string) {
	if sid, known := pm.forget(id); known {
		pm.a.Publish(EvPaneClosed, PaneEvent{ID: id, SessionID: sid})
	}
}

// Exited forgets pane id, whose program ended, and publishes pane.exited.
func (pm *PaneManager) Exited(id string, err error) {
	sid, _ := pm.forget(id)
	ev := PaneEvent{ID: id, SessionID: sid}
	if err != nil {
		ev.Err = err.Error()
	}
	pm.a.Publish(EvPaneExited, ev)
}

// TakeReply hands over pane id's pending reply, once.
func (pm *PaneManager) TakeReply(id string) (string, bool) {
	pm.mu.Lock()
	defer pm.mu.Unlock()
	r, ok := pm.replies[id]
	delete(pm.replies, id)
	return r.Text, ok
}

// Reply is pane id's pending reply, if any.
func (pm *PaneManager) Reply(id string) (PendingReply, bool) {
	pm.mu.Lock()
	defer pm.mu.Unlock()
	r, ok := pm.replies[id]
	return r, ok
}

// PaneOf is the pane bound to a session ("" if none).
func (pm *PaneManager) PaneOf(sessionID string) string {
	pm.mu.Lock()
	defer pm.mu.Unlock()
	return pm.of[sessionID]
}

// IsOpen reports whether a client started pane id and has not closed it.
func (pm *PaneManager) IsOpen(id string) bool {
	pm.mu.Lock()
	defer pm.mu.Unlock()
	_, ok := pm.open[id]
	return ok
}

// Bindings is a copy of the session → pane id table.
func (pm *PaneManager) Bindings() map[string]string {
	pm.mu.Lock()
	defer pm.mu.Unlock()
	out := make(map[string]string, len(pm.of))
	for k, v := range pm.of {
		out[k] = v
	}
	return out
}

// Replies is a copy of the pending replies' text by pane id.
func (pm *PaneManager) Replies() map[string]string {
	pm.mu.Lock()
	defer pm.mu.Unlock()
	out := make(map[string]string, len(pm.replies))
	for k, r := range pm.replies {
		out[k] = r.Text
	}
	return out
}

// Fullscreen asks the client to hand the whole terminal to `claude attach`
// (or `claude --resume` for history) and come back when it exits. A
// background session's embedded pane is given up (the client closes it).
func (a *App) Fullscreen(s store.Session) (Result, error) {
	var argv []string
	switch {
	case s.ShortID != "":
		argv = a.ports.Claude.AttachArgv(s.ShortID)
		a.Panes().Close(s.ShortID)
	case s.Kind == "history":
		// the port has no resume argv: the binary is attach's first word
		bin := a.ports.Claude.AttachArgv(s.SessionID)[0]
		argv = []string{bin, claude.AllowBypass, "--resume", s.SessionID}
	default:
		return Result{}, fmt.Errorf("sessão interativa aberta em outro terminal")
	}
	a.ports.Sessions.MarkSeen(s.SessionID, a.clock.Now())
	return Result{Exec: &core.ExecRequest{Argv: argv, Dir: s.Cwd}}, nil
}

// session finds a session of the current snapshot by id.
func (a *App) paneSession(id string) (store.Session, bool) {
	for _, s := range a.Snapshot().Sessions {
		if s.SessionID == id {
			return s, true
		}
	}
	return store.Session{}, false
}

// openCommand runs Open for the selected session, resuming a history one,
// and returns the plan the client starts (Data).
func (a *App) openCommand(focus bool) Runner {
	return func(ctx Context, _ Args) (Result, error) {
		s, _ := a.paneSession(ctx.SessionID)
		pm := a.Panes()
		plan, err := pm.Open(s, focus, "", nil)
		if err != nil {
			return Result{}, err
		}
		if plan.Resume {
			id, err := pm.Resume(plan.Dir, plan.SessionID)
			if err != nil {
				return Result{}, err
			}
			plan = pm.Attach(s, id, focus, "")
		}
		return Result{Data: plan}, nil
	}
}

// RegisterPanes adds session.open, session.preview, session.fullscreen and
// the client commands pane.close and pane.zoom to the registry.
func (a *App) RegisterPanes() error {
	hasSession := WithAvailable(func(ctx Context) bool { _, ok := a.paneSession(ctx.SessionID); return ok })
	cmds := []struct {
		c    core.Command
		run  Runner
		opts []Option
	}{
		{core.Command{Name: "session.open", Title: "abre e foca", Keys: []string{"enter"}, Mode: core.ModeNormal},
			a.openCommand(true), []Option{hasSession}},
		{core.Command{Name: "session.preview", Title: "abre sem focar", Keys: []string{"o"}, Mode: core.ModeNormal},
			a.openCommand(false), []Option{hasSession}},
		{core.Command{Name: "session.fullscreen", Title: "tela cheia", Keys: []string{"O"}, Mode: core.ModeNormal},
			func(ctx Context, _ Args) (Result, error) {
				s, _ := a.paneSession(ctx.SessionID)
				return a.Fullscreen(s)
			}, []Option{hasSession}},
		{core.Command{Name: "pane.close", Title: "fecha painel", Keys: []string{"w"}, Mode: core.ModeNormal, Scope: core.ScopeClient}, nil, nil},
		{core.Command{Name: "pane.zoom", Title: "zoom do painel", Keys: []string{"z"}, Mode: core.ModeNormal, Scope: core.ScopeClient}, nil, nil},
	}
	for _, c := range cmds {
		if err := a.reg.Register(c.c, c.run, c.opts...); err != nil {
			return err
		}
	}
	return nil
}
