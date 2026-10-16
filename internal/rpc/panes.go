package rpc

// Panes for remote clients. The core decides how to show a session
// (app.PaneManager: reuse, attach, resume); this server is the client that
// starts the PTY (through the PTYSpawner port), keeps it, and relays its raw
// bytes over one WebSocket per viewer: binary frames both ways, text JSON
// frames for control (resize in; title and exit out).

import (
	"context"
	"encoding/json"
	"net/http"
	"sort"
	"sync"
	"time"

	"github.com/coder/websocket"

	"github.com/aqilputi/nekuroddo/internal/app"
	"github.com/aqilputi/nekuroddo/internal/rpc/proto"
	"github.com/aqilputi/nekuroddo/internal/store"
)

// Pane defaults and limits.
const (
	defaultCols = 80
	defaultRows = 24
	maxSide     = 2000
	wsReadLimit = 1 << 20
	// enterDelay is the pause between pasting a reply and pressing enter
	// (the TUI waits the same).
	enterDelay = 150 * time.Millisecond
	// defaultExitWait is how long a viewer whose byte stream ended waits for
	// the exit status before giving up (the stream also ends for a viewer
	// that lagged too far behind).
	defaultExitWait = 2 * time.Second
)

// paneEntry is one pane the server runs.
type paneEntry struct {
	p         app.Pane
	id        string
	sessionID string

	mu         sync.Mutex
	cols, rows int
	err        error

	once sync.Once
	done chan struct{} // closed when the program exited or the pane was closed
}

// finish marks the pane over; it reports whether this call did it.
func (e *paneEntry) finish(err error) bool {
	did := false
	e.once.Do(func() {
		e.mu.Lock()
		e.err = err
		e.mu.Unlock()
		close(e.done)
		did = true
	})
	return did
}

func (e *paneEntry) exitErr() error {
	e.mu.Lock()
	defer e.mu.Unlock()
	return e.err
}

func (e *paneEntry) size() (int, int) {
	e.mu.Lock()
	defer e.mu.Unlock()
	return e.cols, e.rows
}

// paneHub is the server's table of panes.
type paneHub struct {
	s      *Server
	openMu sync.Mutex // one open at a time: two clients must not start the same session twice

	mu    sync.Mutex
	panes map[string]*paneEntry
}

func newPaneHub(s *Server) *paneHub { return &paneHub{s: s, panes: map[string]*paneEntry{}} }

func (h *paneHub) get(id string) *paneEntry {
	h.mu.Lock()
	defer h.mu.Unlock()
	return h.panes[id]
}

func (h *paneHub) alive(id string) bool { return h.get(id) != nil }

// remove drops e (if it is still the entry for its id).
func (h *paneHub) remove(e *paneEntry) {
	h.mu.Lock()
	defer h.mu.Unlock()
	if h.panes[e.id] == e {
		delete(h.panes, e.id)
	}
}

// add starts tracking p: when its program exits the pane is dropped and the
// core told (pane.exited).
func (h *paneHub) add(p app.Pane, sessionID string, cols, rows int) *paneEntry {
	e := &paneEntry{p: p, id: p.ID(), sessionID: sessionID, cols: cols, rows: rows, done: make(chan struct{})}
	h.mu.Lock()
	h.panes[e.id] = e
	h.mu.Unlock()
	go func() {
		select {
		case err := <-p.Exited():
			if e.finish(err) {
				h.remove(e)
				h.s.a.Panes().Exited(e.id, err)
			}
		case <-e.done: // closed by us
		}
	}()
	return e
}

// close ends pane id (SIGHUP to its program) and tells the core
// (pane.closed); false if there is no such pane.
func (h *paneHub) close(id string) bool {
	e := h.get(id)
	if e == nil {
		return false
	}
	h.remove(e)
	if e.finish(nil) {
		e.p.Close()
		h.s.a.Panes().Close(id)
	}
	return true
}

func (h *paneHub) closeAll() {
	h.mu.Lock()
	ids := make([]string, 0, len(h.panes))
	for id := range h.panes {
		ids = append(ids, id)
	}
	h.mu.Unlock()
	for _, id := range ids {
		h.close(id)
	}
}

func (h *paneHub) list() []proto.Pane {
	h.mu.Lock()
	es := make([]*paneEntry, 0, len(h.panes))
	for _, e := range h.panes {
		es = append(es, e)
	}
	h.mu.Unlock()
	out := make([]proto.Pane, 0, len(es))
	for _, e := range es {
		c, r := e.size()
		out = append(out, proto.Pane{ID: e.id, SessionID: e.sessionID, Title: e.p.Title(), Cols: c, Rows: r})
	}
	sort.Slice(out, func(i, j int) bool { return out[i].ID < out[j].ID })
	return out
}

// resize sets e's size (last writer wins) and publishes pane.resized.
func (h *paneHub) resize(e *paneEntry, cols, rows int) {
	e.mu.Lock()
	e.cols, e.rows = cols, rows
	e.mu.Unlock()
	e.p.Resize(cols, rows)
	h.s.a.Publish(proto.EvPaneResized, proto.PaneResized{ID: e.id, Cols: cols, Rows: rows})
}

// AttachPane makes a pane started elsewhere in this process (a host that
// embeds the core) reachable over the protocol, as if POST /v1/panes had
// started it.
func (s *Server) AttachPane(p app.Pane, sessionID string, cols, rows int) {
	s.panes.add(p, sessionID, cols, rows)
}

// typeReply types pane id's pending reply after d (the core's timing), then
// enter, like the TUI does.
func (s *Server) typeReply(id string, d time.Duration) {
	s.goBackground(func() {
		if !sleepCtx(s.ctx, s.o.replyDelay(d)) {
			return
		}
		text, ok := s.a.Panes().TakeReply(id)
		e := s.panes.get(id)
		if !ok || text == "" || e == nil {
			return
		}
		_, _ = e.p.Write([]byte("\x1b[200~" + text + "\x1b[201~"))
		if sleepCtx(s.ctx, s.o.replyDelay(enterDelay)) {
			_, _ = e.p.Write([]byte("\r"))
		}
	})
}

func sleepCtx(ctx context.Context, d time.Duration) bool {
	t := time.NewTimer(d)
	defer t.Stop()
	select {
	case <-ctx.Done():
		return false
	case <-t.C:
		return true
	}
}

func validSize(cols, rows int) bool {
	return cols > 0 && rows > 0 && cols <= maxSide && rows <= maxSide
}

// ---- handlers ----

func (s *Server) listPanes(w http.ResponseWriter, _ *http.Request) {
	writeJSON(w, http.StatusOK, s.panes.list())
}

func (s *Server) findSession(id string) (store.Session, bool) {
	for _, x := range s.a.Snapshot().Sessions {
		if x.SessionID == id {
			return x, true
		}
	}
	return store.Session{}, false
}

// openPane is POST /v1/panes: the same reuse/attach/resume decision as the
// TUI's openPane, with the PTY started here.
func (s *Server) openPane(w http.ResponseWriter, r *http.Request) {
	var req proto.PaneOpenRequest
	if !decode(w, r, &req) {
		return
	}
	if req.SessionID == "" {
		writeErr(w, http.StatusBadRequest, proto.CodeBadRequest, "falta session_id")
		return
	}
	if req.Cols <= 0 || req.Rows <= 0 {
		req.Cols, req.Rows = defaultCols, defaultRows
	}
	if !validSize(req.Cols, req.Rows) {
		writeErr(w, http.StatusBadRequest, proto.CodeBadRequest, "tamanho inválido")
		return
	}
	sess, ok := s.findSession(req.SessionID)
	if !ok {
		writeErr(w, http.StatusNotFound, proto.CodeNotFound, "sessão desconhecida: "+req.SessionID)
		return
	}
	s.panes.openMu.Lock()
	defer s.panes.openMu.Unlock()
	pm := s.a.Panes()
	plan, err := pm.Open(sess, false, req.Reply, s.panes.alive)
	if err != nil {
		writeAppErr(w, err)
		return
	}
	if plan.Reuse {
		if plan.Reply != "" {
			s.typeReply(plan.ID, plan.ReplyIn)
		}
		writeJSON(w, http.StatusOK, proto.PaneOpened{PaneID: plan.ID, Reused: true})
		return
	}
	if plan.Resume {
		id, err := pm.Resume(plan.Dir, plan.SessionID)
		if err != nil {
			writeAppErr(w, err)
			return
		}
		plan = pm.Attach(sess, id, false, req.Reply)
	}
	if s.spawner == nil {
		writeErr(w, http.StatusNotImplemented, proto.CodeNotImplemented, "este servidor não abre painéis")
		return
	}
	p, err := s.spawner.Start(plan.ID, plan.Title, plan.Dir, req.Cols, req.Rows, plan.Argv...)
	if err != nil {
		writeAppErr(w, err)
		return
	}
	s.panes.add(p, plan.SessionID, req.Cols, req.Rows)
	pm.Started(plan)
	if plan.Reply != "" {
		s.typeReply(plan.ID, plan.ReplyIn)
	}
	writeJSON(w, http.StatusOK, proto.PaneOpened{PaneID: plan.ID})
}

func (s *Server) resizePane(w http.ResponseWriter, r *http.Request) {
	var req proto.Resize
	if !decode(w, r, &req) {
		return
	}
	e := s.panes.get(r.PathValue("id"))
	if e == nil {
		writeErr(w, http.StatusNotFound, proto.CodeNotFound, "painel desconhecido")
		return
	}
	if !validSize(req.Cols, req.Rows) {
		writeErr(w, http.StatusBadRequest, proto.CodeBadRequest, "tamanho inválido")
		return
	}
	s.panes.resize(e, req.Cols, req.Rows)
	w.WriteHeader(http.StatusNoContent)
}

func (s *Server) closePane(w http.ResponseWriter, r *http.Request) {
	if !s.panes.close(r.PathValue("id")) {
		writeErr(w, http.StatusNotFound, proto.CodeNotFound, "painel desconhecido")
		return
	}
	w.WriteHeader(http.StatusNoContent)
}

// paneWS is WS /v1/panes/{id}/ws. Several viewers may share a pane; one
// going away leaves the pane running.
func (s *Server) paneWS(w http.ResponseWriter, r *http.Request) {
	e := s.panes.get(r.PathValue("id"))
	if e == nil {
		writeErr(w, http.StatusNotFound, proto.CodeNotFound, "painel desconhecido")
		return
	}
	// Origin is not checked: unix sockets are reachable only by the user,
	// and TCP requires the token (a cross-site page cannot know it).
	c, err := websocket.Accept(w, r, &websocket.AcceptOptions{InsecureSkipVerify: true})
	if err != nil {
		return
	}
	c.SetReadLimit(wsReadLimit)
	// Subscribe before anything else: every byte after the first frame the
	// viewer gets is relayed.
	bytes, cancel := e.p.Subscribe()
	defer cancel()
	ctx, stop := context.WithCancel(r.Context())
	defer stop()
	go s.paneReader(ctx, stop, c, e)

	title := e.p.Title()
	if title != "" && sendControl(ctx, c, proto.Control{Type: proto.CtlTitle, Text: title}) != nil {
		_ = c.CloseNow()
		return
	}
	for {
		select {
		case <-ctx.Done():
			_ = c.CloseNow()
			return
		case b, ok := <-bytes:
			if !ok {
				s.paneEnded(ctx, c, e, nil)
				return
			}
			if c.Write(ctx, websocket.MessageBinary, b) != nil {
				_ = c.CloseNow()
				return
			}
			if t := e.p.Title(); t != title {
				title = t
				_ = sendControl(ctx, c, proto.Control{Type: proto.CtlTitle, Text: t})
			}
		case <-e.done:
			s.paneEnded(ctx, c, e, bytes)
			return
		}
	}
}

// paneEnded sends what is left of the output and the exit frame, then
// closes the socket. bytes is nil when the stream already ended; then it
// waits a little for the exit status (a lagging viewer gets "try again").
func (s *Server) paneEnded(ctx context.Context, c *websocket.Conn, e *paneEntry, bytes <-chan []byte) {
	if bytes == nil {
		select {
		case <-e.done:
		case <-time.After(s.o.exitWait):
			_ = c.Close(websocket.StatusTryAgainLater, "atrasado demais: reconecte")
			return
		case <-ctx.Done():
			_ = c.CloseNow()
			return
		}
	}
	for drained := false; !drained && bytes != nil; {
		select {
		case b, ok := <-bytes:
			if !ok {
				drained = true
				break
			}
			_ = c.Write(ctx, websocket.MessageBinary, b)
		default:
			drained = true
		}
	}
	ctl := proto.Control{Type: proto.CtlExit}
	if err := e.exitErr(); err != nil {
		ctl.Err = err.Error()
	}
	_ = sendControl(ctx, c, ctl)
	_ = c.Close(websocket.StatusNormalClosure, "fim")
}

// paneReader relays the viewer's frames: binary to the PTY, resize controls
// to the pane. It ends the connection's context when the viewer goes away.
func (s *Server) paneReader(ctx context.Context, stop func(), c *websocket.Conn, e *paneEntry) {
	defer stop()
	for {
		typ, b, err := c.Read(ctx)
		if err != nil {
			return
		}
		if typ == websocket.MessageBinary {
			_, _ = e.p.Write(b)
			continue
		}
		var ctl proto.Control
		if json.Unmarshal(b, &ctl) == nil && ctl.Type == proto.CtlResize && validSize(ctl.Cols, ctl.Rows) {
			s.panes.resize(e, ctl.Cols, ctl.Rows)
		}
	}
}

func sendControl(ctx context.Context, c *websocket.Conn, ctl proto.Control) error {
	b, _ := json.Marshal(ctl)
	return c.Write(ctx, websocket.MessageText, b)
}
