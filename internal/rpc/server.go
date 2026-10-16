// Package rpc serves the core (internal/app) over the protocol v1 of plan §3:
// HTTP+JSON for commands and queries, SSE for the event bus and one
// WebSocket per pane for raw PTY bytes. It listens on a unix socket (0600:
// file permissions are the auth) and, for webviews that cannot reach one, on
// a loopback TCP port that requires a bearer token. The server is optional:
// a plain `nk` never starts it.
//
// rpc is a shell like the TUI: it translates requests into app calls and
// never reaches claude/store/groups/jira/todo for their effects.
package rpc

import (
	"context"
	"crypto/rand"
	"crypto/subtle"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net"
	"net/http"
	"net/url"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"sync"
	"time"

	"github.com/aqilputi/nekuroddo/internal/app"
	"github.com/aqilputi/nekuroddo/internal/core"
	"github.com/aqilputi/nekuroddo/internal/groups"
	"github.com/aqilputi/nekuroddo/internal/rpc/proto"
	"github.com/aqilputi/nekuroddo/internal/store"
)

var (
	// ErrNoToken: a TCP listener needs Options.Token.
	ErrNoToken = errors.New("rpc: escutar em TCP exige um token")
	// ErrInUse: another server answers on the socket.
	ErrInUse = errors.New("rpc: já há um servidor nesse socket")
)

// Defaults for Options.
const (
	DefaultHeartbeat = 15 * time.Second
	DefaultBacklog   = 1024
	maxBody          = 1 << 20
)

// Options tune a Server.
type Options struct {
	Version      string        // hello.version
	Token        string        // bearer token every TCP listener requires
	Capabilities []string      // announced in hello besides "panes"
	Heartbeat    time.Duration // SSE keep-alive comment period (DefaultHeartbeat)
	Backlog      int           // events kept for Last-Event-ID (DefaultBacklog)
	// Panes starts the PTYs of POST /v1/panes; nil uses the app's port.
	Panes app.PTYSpawner

	replyDelay func(time.Duration) time.Duration // tests shorten the reply timers
	exitWait   time.Duration                     // defaultExitWait
}

// Server serves one App. Build it with New, then Serve as many listeners as
// needed; Close ends them all.
type Server struct {
	a       *app.App
	o       Options
	mux     *http.ServeMux
	events  *eventHub
	panes   *paneHub
	spawner app.PTYSpawner

	ctx    context.Context
	cancel context.CancelFunc
	wg     sync.WaitGroup // background work: refreshes, reply timers, the event pump

	mu      sync.Mutex
	servers []*http.Server
	closed  bool
}

// New builds a server over a and starts following its event bus (events
// published from now on are kept for SSE clients).
func New(a *app.App, o Options) *Server {
	if o.Heartbeat <= 0 {
		o.Heartbeat = DefaultHeartbeat
	}
	if o.Backlog <= 0 {
		o.Backlog = DefaultBacklog
	}
	if o.exitWait <= 0 {
		o.exitWait = defaultExitWait
	}
	if o.replyDelay == nil {
		o.replyDelay = func(d time.Duration) time.Duration { return d }
	}
	ctx, cancel := context.WithCancel(context.Background())
	s := &Server{a: a, o: o, ctx: ctx, cancel: cancel, events: newEventHub(o.Backlog), spawner: o.Panes}
	if s.spawner == nil {
		s.spawner = a.Ports().Panes
	}
	s.panes = newPaneHub(s)
	s.startPump()
	s.routes()
	return s
}

func (s *Server) routes() {
	m := http.NewServeMux()
	m.HandleFunc("GET /v1/hello", s.hello)
	m.HandleFunc("GET /v1/snapshot", s.snapshot)
	m.HandleFunc("GET /v1/events", s.serveEvents)
	m.HandleFunc("GET /v1/commands", s.commands)
	m.HandleFunc("POST /v1/commands/{name}", s.run)
	m.HandleFunc("POST /v1/confirm/{token}", s.confirm)
	m.HandleFunc("GET /v1/keymap", s.keymap)
	m.HandleFunc("GET /v1/panes", s.listPanes)
	m.HandleFunc("POST /v1/panes", s.openPane)
	m.HandleFunc("GET /v1/panes/{id}/ws", s.paneWS)
	m.HandleFunc("POST /v1/panes/{id}/resize", s.resizePane)
	m.HandleFunc("DELETE /v1/panes/{id}", s.closePane)
	// Phase 2 (WP-5.2, WP-5.3): declared so clients get a clear answer.
	for _, p := range []string{"POST /v1/view/tree", "GET /v1/theme", "POST /v1/lua/eval"} {
		m.HandleFunc(p, notImplemented)
	}
	s.mux = m
}

// Handler is the protocol with no authentication, for a host that serves
// it itself (in-process tests, a proxy that already checked the caller).
func (s *Server) Handler() http.Handler { return s.mux }

// Serve answers on l until Close. A unix socket needs no token; any other
// listener requires Options.Token (ErrNoToken otherwise).
func (s *Server) Serve(l net.Listener) error {
	h := http.Handler(s.mux)
	if l.Addr().Network() != "unix" {
		if s.o.Token == "" {
			return ErrNoToken
		}
		h = s.auth(h)
	}
	hs := &http.Server{Handler: h, ReadHeaderTimeout: 10 * time.Second,
		BaseContext: func(net.Listener) context.Context { return s.ctx }}
	s.mu.Lock()
	if s.closed {
		s.mu.Unlock()
		_ = l.Close()
		return http.ErrServerClosed
	}
	s.servers = append(s.servers, hs)
	s.mu.Unlock()
	return hs.Serve(l)
}

// Close stops every listener, ends the event streams and pane sockets, and
// closes the panes this server started. It does not close the App.
func (s *Server) Close() error {
	s.mu.Lock()
	if s.closed {
		s.mu.Unlock()
		return nil
	}
	s.closed = true
	servers := s.servers
	s.mu.Unlock()
	s.cancel()
	var errs []error
	for _, hs := range servers {
		errs = append(errs, hs.Close())
	}
	s.panes.closeAll()
	s.wg.Wait()
	return errors.Join(errs...)
}

// go runs f in the background until Close waits for it.
func (s *Server) goBackground(f func()) {
	s.wg.Add(1)
	go func() {
		defer s.wg.Done()
		f()
	}()
}

// auth demands the bearer token (header, or ?token= for WebSocket and
// EventSource, which cannot set headers).
func (s *Server) auth(next http.Handler) http.Handler {
	want := []byte(s.o.Token)
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		got := r.URL.Query().Get("token")
		if h, ok := strings.CutPrefix(r.Header.Get("Authorization"), "Bearer "); ok {
			got = h
		}
		if subtle.ConstantTimeCompare([]byte(got), want) != 1 {
			w.Header().Set("WWW-Authenticate", "Bearer")
			writeErr(w, http.StatusUnauthorized, proto.CodeUnauthorized, "token ausente ou inválido")
			return
		}
		next.ServeHTTP(w, r)
	})
}

// ---- JSON helpers ----

func writeJSON(w http.ResponseWriter, status int, v any) {
	b, err := json.Marshal(v)
	if err != nil {
		writeErr(w, http.StatusInternalServerError, proto.CodeFailed, err.Error())
		return
	}
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(status)
	_, _ = w.Write(append(b, '\n'))
}

func writeErr(w http.ResponseWriter, status int, code, msg string) {
	b, _ := json.Marshal(proto.Error{Error: msg, Code: code})
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(status)
	_, _ = w.Write(append(b, '\n'))
}

// decode reads an optional JSON body into v (an empty body leaves v as is).
func decode(w http.ResponseWriter, r *http.Request, v any) bool {
	err := json.NewDecoder(io.LimitReader(r.Body, maxBody)).Decode(v)
	if err != nil && !errors.Is(err, io.EOF) {
		writeErr(w, http.StatusBadRequest, proto.CodeBadRequest, "json: "+err.Error())
		return false
	}
	return true
}

func notImplemented(w http.ResponseWriter, r *http.Request) {
	writeErr(w, http.StatusNotImplemented, proto.CodeNotImplemented, r.URL.Path+": fase 2 (ainda não existe)")
}

// appErr maps an app error to a status and code.
func appErr(err error) (int, string) {
	switch {
	case errors.Is(err, app.ErrUnknownCommand):
		return http.StatusNotFound, proto.CodeUnknownCommand
	case errors.Is(err, app.ErrClientCommand):
		return http.StatusBadRequest, proto.CodeClientCommand
	case errors.Is(err, app.ErrUnavailable):
		return http.StatusConflict, proto.CodeUnavailable
	case errors.Is(err, app.ErrMissingArg):
		return http.StatusBadRequest, proto.CodeMissingArg
	case errors.Is(err, app.ErrUnknownToken):
		return http.StatusNotFound, proto.CodeUnknownToken
	case errors.Is(err, app.ErrExpired):
		return http.StatusGone, proto.CodeExpired
	}
	return http.StatusUnprocessableEntity, proto.CodeFailed
}

func writeAppErr(w http.ResponseWriter, err error) {
	status, code := appErr(err)
	writeErr(w, status, code, err.Error())
}

// ---- hello, snapshot ----

func (s *Server) hello(w http.ResponseWriter, _ *http.Request) {
	caps := append([]string{proto.CapPanes}, s.o.Capabilities...)
	writeJSON(w, http.StatusOK, proto.Hello{App: proto.AppName, Version: s.o.Version, Protocol: proto.Version,
		Capabilities: caps, PID: os.Getpid()})
}

// inboxOf lists the sessions waiting on the user, most urgent first.
func inboxOf(ss []store.Session) []string {
	var xs []*store.Session
	for i := range ss {
		x := &ss[i]
		if x.Attention != store.AttnNone && !x.Archived && !x.Stale && !x.Ended && !x.Finished {
			xs = append(xs, x)
		}
	}
	core.SortInbox(xs, nil)
	out := make([]string, 0, len(xs))
	for _, x := range xs {
		out = append(out, x.SessionID)
	}
	return out
}

func (s *Server) snapshot(w http.ResponseWriter, _ *http.Request) {
	snap := s.a.Snapshot()
	out := proto.Snapshot{Sessions: make([]proto.Session, 0, len(snap.Sessions)), Inbox: inboxOf(snap.Sessions),
		Groups: s.a.Groups().Groups, Mail: map[string]proto.MailCue{}, Usage: proto.FromUsage(s.a.Usage()),
		Jira: s.a.JiraCache(), Pending: s.a.Pending(), At: snap.At, Today: snap.TodayCost}
	for _, x := range snap.Sessions {
		out.Sessions = append(out.Sessions, proto.FromSession(x))
	}
	if out.Groups == nil {
		out.Groups = map[string][]groups.Member{}
	}
	for id, c := range s.a.Mail() {
		out.Mail[id] = proto.MailCue{At: c.At, Sent: c.Sent}
	}
	if out.Pending == nil {
		out.Pending = []core.PendingConfirmation{}
	}
	todos := s.a.Todos()
	out.Todos = proto.Todos{Groups: todos.Groups, Err: todos.Err}
	if snap.Err != nil {
		out.Err = snap.Err.Error()
	}
	writeJSON(w, http.StatusOK, out)
}

// ---- commands ----

func toApp(c proto.Context) app.Context {
	return app.Context{SessionID: c.SessionID, Picked: c.Picked, Tab: c.Tab, TodoID: c.TodoID, IssueKey: c.IssueKey}
}

// contextFromQuery reads a selection from ?session_id=&picked=&tab=&todo_id=&issue_key=.
func contextFromQuery(q url.Values) (proto.Context, error) {
	c := proto.Context{SessionID: q.Get("session_id"), Tab: q.Get("tab"), IssueKey: q.Get("issue_key")}
	for _, p := range q["picked"] {
		for _, id := range strings.Split(p, ",") {
			if id != "" {
				c.Picked = append(c.Picked, id)
			}
		}
	}
	if t := q.Get("todo_id"); t != "" {
		n, err := strconv.Atoi(t)
		if err != nil {
			return c, fmt.Errorf("todo_id: %w", err)
		}
		c.TodoID = n
	}
	return c, nil
}

// commands is GET /v1/commands: every registered command, in registration
// order, with Available computed for the caller's selection.
func (s *Server) commands(w http.ResponseWriter, r *http.Request) {
	pc, err := contextFromQuery(r.URL.Query())
	if err != nil {
		writeErr(w, http.StatusBadRequest, proto.CodeBadRequest, err.Error())
		return
	}
	ctx := toApp(pc)
	avail := map[string]bool{}
	for _, c := range s.a.Registry().List(ctx) {
		avail[c.Name] = true
	}
	all := s.a.Registry().All()
	out := make([]proto.CommandInfo, 0, len(all))
	for _, c := range all {
		out = append(out, proto.CommandInfo{Command: c, Available: avail[c.Name]})
	}
	writeJSON(w, http.StatusOK, out)
}

func (s *Server) run(w http.ResponseWriter, r *http.Request) {
	var req proto.RunRequest
	if !decode(w, r, &req) {
		return
	}
	res, err := s.a.Run(toApp(req.Context), r.PathValue("name"), app.Args(req.Args))
	if err != nil {
		writeAppErr(w, err)
		return
	}
	s.effects(res)
	writeJSON(w, http.StatusOK, res)
}

func (s *Server) confirm(w http.ResponseWriter, r *http.Request) {
	var req proto.ConfirmRequest
	if !decode(w, r, &req) {
		return
	}
	token := r.PathValue("token")
	if !req.Yes {
		if err := s.a.Cancel(token); err != nil {
			writeAppErr(w, err)
			return
		}
		writeJSON(w, http.StatusOK, app.Result{})
		return
	}
	res, err := s.a.Confirm(token)
	if err != nil {
		writeAppErr(w, err)
		return
	}
	s.effects(res)
	writeJSON(w, http.StatusOK, res)
}

// effects does, for remote clients, what a final Result asks the client to
// do: close the panes this server runs and refresh. (On a Result that still
// asks, the panes close only after yes; the final Result repeats them.)
func (s *Server) effects(res app.Result) {
	if res.Pending != nil {
		return
	}
	var eff app.SessionEffect
	switch d := res.Data.(type) {
	case app.SessionEffect:
		eff = d
	case *app.SessionEffect:
		if d == nil {
			return
		}
		eff = *d
	default:
		return
	}
	for _, id := range eff.ClosePanes {
		s.panes.close(id)
	}
	if eff.Refresh {
		s.goBackground(func() { s.a.Refresh() })
	}
}

// keymap is GET /v1/keymap: the same table the TUI resolves keys with
// (core.BuildKeymap over the registry, so Lua rebindings show up).
func (s *Server) keymap(w http.ResponseWriter, _ *http.Request) {
	km := s.a.Keymap()
	if km == nil {
		km = core.Keymap{}
	}
	writeJSON(w, http.StatusOK, km)
}

// ---- listeners and tokens ----

// ListenUnix listens on a unix socket at path with mode 0600. A stale socket
// (nobody answers) is replaced; a live one is ErrInUse; any other file is an
// error.
func ListenUnix(path string) (net.Listener, error) {
	if fi, err := os.Lstat(path); err == nil {
		if fi.Mode()&os.ModeSocket == 0 {
			return nil, fmt.Errorf("rpc: %s existe e não é um socket", path)
		}
		if c, err := net.DialTimeout("unix", path, 500*time.Millisecond); err == nil {
			_ = c.Close()
			return nil, fmt.Errorf("%w: %s", ErrInUse, path)
		}
		os.Remove(path)
	}
	if err := os.MkdirAll(filepath.Dir(path), 0o700); err != nil {
		return nil, err
	}
	l, err := net.Listen("unix", path)
	if err != nil {
		return nil, err
	}
	if err := os.Chmod(path, 0o600); err != nil {
		_ = l.Close()
		return nil, err
	}
	return l, nil
}

// Listen opens "unix:///path" or "tcp://127.0.0.1:port" (TCP only on a
// loopback address).
func Listen(spec string) (net.Listener, error) {
	scheme, rest, ok := strings.Cut(spec, "://")
	if !ok {
		return nil, fmt.Errorf("rpc: endereço %q: use unix:///caminho ou tcp://127.0.0.1:porta", spec)
	}
	switch scheme {
	case "unix":
		return ListenUnix(rest)
	case "tcp":
		host, _, err := net.SplitHostPort(rest)
		if err != nil {
			return nil, fmt.Errorf("rpc: %w", err)
		}
		if ip := net.ParseIP(host); host != "localhost" && (ip == nil || !ip.IsLoopback()) {
			return nil, fmt.Errorf("rpc: %s não é loopback", host)
		}
		return net.Listen("tcp", rest)
	}
	return nil, fmt.Errorf("rpc: esquema %q desconhecido", scheme)
}

// NewToken is a fresh random bearer token.
func NewToken() string { return rand.Text() }

// WriteTokenFile writes token to path with mode 0600 (atomically).
func WriteTokenFile(path, token string) error {
	if err := os.MkdirAll(filepath.Dir(path), 0o700); err != nil {
		return err
	}
	tmp, err := os.CreateTemp(filepath.Dir(path), ".token-*")
	if err != nil {
		return err
	}
	defer os.Remove(tmp.Name())
	if _, err := tmp.WriteString(token + "\n"); err != nil {
		tmp.Close()
		return err
	}
	if err := tmp.Close(); err != nil {
		return err
	}
	return os.Rename(tmp.Name(), path)
}

// ReadTokenFile reads a token written by WriteTokenFile.
func ReadTokenFile(path string) (string, error) {
	b, err := os.ReadFile(path)
	if err != nil {
		return "", err
	}
	return strings.TrimSpace(string(b)), nil
}

func runtimeFile(ext string) string {
	dir := os.Getenv("XDG_RUNTIME_DIR")
	if dir == "" {
		dir = os.TempDir()
	}
	return filepath.Join(dir, fmt.Sprintf("nekuroddo-%d.%s", os.Getuid(), ext))
}

// DefaultSocketPath is $XDG_RUNTIME_DIR/nekuroddo-<uid>.sock.
func DefaultSocketPath() string { return runtimeFile("sock") }

// DefaultTokenPath is $XDG_RUNTIME_DIR/nekuroddo-<uid>.token.
func DefaultTokenPath() string { return runtimeFile("token") }
