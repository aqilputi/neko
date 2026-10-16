// Package apptest has in-memory fakes of every app port. They record their
// calls for assertions and are safe for concurrent use. Configure a fake by
// setting its exported fields before handing it to app; use the setter
// methods (SetSnapshot, Push, …) to change it afterwards.
package apptest

import (
	"errors"
	"sync"
	"time"

	"github.com/aqilputi/nekuroddo/internal/app"
	"github.com/aqilputi/nekuroddo/internal/claude"
	"github.com/aqilputi/nekuroddo/internal/events"
	"github.com/aqilputi/nekuroddo/internal/groups"
	"github.com/aqilputi/nekuroddo/internal/jira"
	"github.com/aqilputi/nekuroddo/internal/store"
	"github.com/aqilputi/nekuroddo/internal/todo"
)

// Epoch is where a FakeClock starts unless told otherwise.
var Epoch = time.Date(2026, 1, 1, 12, 0, 0, 0, time.UTC)

// Fakes is one fake per port.
type Fakes struct {
	Clock    *FakeClock
	Claude   *FakeClaude
	Sessions *FakeSessions
	Groups   *FakeGroups
	Events   *FakeEvents
	Panes    *FakePanes
	Notify   *FakeNotifier
	Open     *FakeOpener
	Jira     *FakeJira
	Todo     *FakeTodo
}

// New returns a fresh set of fakes.
func New() *Fakes {
	return &Fakes{
		Clock:    NewFakeClock(Epoch),
		Claude:   &FakeClaude{},
		Sessions: &FakeSessions{},
		Groups:   NewFakeGroups(),
		Events:   &FakeEvents{},
		Panes:    &FakePanes{},
		Notify:   &FakeNotifier{},
		Open:     &FakeOpener{},
		Jira:     &FakeJira{},
		Todo:     &FakeTodo{},
	}
}

// Ports exposes the fakes as app ports.
func (f *Fakes) Ports() app.Ports {
	return app.Ports{
		Clock: f.Clock, Claude: f.Claude, Sessions: f.Sessions, Groups: f.Groups, Events: f.Events,
		Panes: f.Panes, Notify: f.Notify, Open: f.Open, Jira: f.Jira, Todo: f.Todo,
	}
}

// Ports is New().Ports(), for callers that never inspect the fakes.
func Ports() app.Ports { return New().Ports() }

// ---- call recording ----

// Call is one recorded method call.
type Call struct {
	Method string
	Args   []any
}

// Recorder records calls; embedded by every fake.
type Recorder struct {
	mu    sync.Mutex
	calls []Call
}

func (r *Recorder) record(method string, args ...any) {
	r.mu.Lock()
	r.calls = append(r.calls, Call{Method: method, Args: args})
	r.mu.Unlock()
}

// Calls returns a copy of every call so far, in order.
func (r *Recorder) Calls() []Call {
	r.mu.Lock()
	defer r.mu.Unlock()
	return append([]Call(nil), r.calls...)
}

// CallsTo returns the calls to one method.
func (r *Recorder) CallsTo(method string) []Call {
	var out []Call
	for _, c := range r.Calls() {
		if c.Method == method {
			out = append(out, c)
		}
	}
	return out
}

// Count is how many times method was called.
func (r *Recorder) Count(method string) int { return len(r.CallsTo(method)) }

// Last is the most recent call (zero Call if none).
func (r *Recorder) Last() Call {
	cs := r.Calls()
	if len(cs) == 0 {
		return Call{}
	}
	return cs[len(cs)-1]
}

// Reset forgets the recorded calls.
func (r *Recorder) Reset() {
	r.mu.Lock()
	r.calls = nil
	r.mu.Unlock()
}

// errs is the per-method error table shared by the fakes.
type errs map[string]error

func (e errs) of(method string) error { return e[method] }

// ---- Clock ----

// FakeClock only moves when told to.
type FakeClock struct {
	mu sync.Mutex
	t  time.Time
}

// NewFakeClock starts at start (Epoch if zero).
func NewFakeClock(start time.Time) *FakeClock {
	if start.IsZero() {
		start = Epoch
	}
	return &FakeClock{t: start}
}

func (c *FakeClock) Now() time.Time {
	c.mu.Lock()
	defer c.mu.Unlock()
	return c.t
}

// Advance moves the clock forward by d.
func (c *FakeClock) Advance(d time.Duration) {
	c.mu.Lock()
	c.t = c.t.Add(d)
	c.mu.Unlock()
}

// Set moves the clock to t.
func (c *FakeClock) Set(t time.Time) {
	c.mu.Lock()
	c.t = t
	c.mu.Unlock()
}

// ---- Claude ----

// FakeClaude answers with its fields; Errs makes a method fail by name.
type FakeClaude struct {
	Recorder
	Agents []claude.Agent
	NextID string // returned by Dispatch and ResumeInBackground
	Usage  claude.UsageReport
	Errs   map[string]error
}

func (c *FakeClaude) ListAgents() ([]claude.Agent, error) {
	c.record("ListAgents")
	if err := errs(c.Errs).of("ListAgents"); err != nil {
		return nil, err
	}
	return c.Agents, nil
}

func (c *FakeClaude) Dispatch(dir, name, prompt string, extra ...string) (string, error) {
	c.record("Dispatch", dir, name, prompt, extra)
	if err := errs(c.Errs).of("Dispatch"); err != nil {
		return "", err
	}
	return c.NextID, nil
}

func (c *FakeClaude) ResumeInBackground(dir, sessionID string) (string, error) {
	c.record("ResumeInBackground", dir, sessionID)
	if err := errs(c.Errs).of("ResumeInBackground"); err != nil {
		return "", err
	}
	return c.NextID, nil
}

func (c *FakeClaude) Stop(id string) error {
	c.record("Stop", id)
	return errs(c.Errs).of("Stop")
}

func (c *FakeClaude) Remove(id string) error {
	c.record("Remove", id)
	return errs(c.Errs).of("Remove")
}

func (c *FakeClaude) AttachArgv(id string) []string {
	c.record("AttachArgv", id)
	return []string{"claude", "attach", id}
}

func (c *FakeClaude) FetchUsage() (claude.UsageReport, error) {
	c.record("FetchUsage")
	if err := errs(c.Errs).of("FetchUsage"); err != nil {
		return claude.UsageReport{}, err
	}
	return c.Usage, nil
}

// ---- Sessions ----

// FakeSessions returns Snap from Refresh and records every mutation.
type FakeSessions struct {
	Recorder
	mu   sync.Mutex
	Snap store.Snapshot
}

// SetSnapshot changes what the next Refresh returns.
func (s *FakeSessions) SetSnapshot(snap store.Snapshot) {
	s.mu.Lock()
	s.Snap = snap
	s.mu.Unlock()
}

func (s *FakeSessions) Refresh() store.Snapshot {
	s.record("Refresh")
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.Snap
}

func (s *FakeSessions) Reapply(sessions []store.Session) { s.record("Reapply", sessions) }
func (s *FakeSessions) Archive(ids ...string)            { s.record("Archive", ids) }
func (s *FakeSessions) Unarchive(ids ...string)          { s.record("Unarchive", ids) }
func (s *FakeSessions) Rename(sessionID, name string)    { s.record("Rename", sessionID, name) }
func (s *FakeSessions) SetContext(sessionID, ctx string) { s.record("SetContext", sessionID, ctx) }
func (s *FakeSessions) RenameContext(shown, name string) { s.record("RenameContext", shown, name) }
func (s *FakeSessions) MarkSeen(sessionID string, at time.Time) {
	s.record("MarkSeen", sessionID, at)
}
func (s *FakeSessions) SetWindowStart(t time.Time) { s.record("SetWindowStart", t) }

// ---- Groups ----

// FakeGroups keeps groups.json and the mail queue in memory.
type FakeGroups struct {
	Recorder
	mu    sync.Mutex
	file  groups.File
	queue map[string][]groups.Note
	Errs  map[string]error
}

// NewFakeGroups starts with no groups.
func NewFakeGroups() *FakeGroups { return &FakeGroups{} }

func copyFile(f groups.File) groups.File {
	c := groups.File{Groups: map[string][]groups.Member{}}
	for tag, ms := range f.Groups {
		c.Groups[tag] = append([]groups.Member(nil), ms...)
	}
	return c
}

func (g *FakeGroups) Load() groups.File {
	g.record("Load")
	g.mu.Lock()
	defer g.mu.Unlock()
	return copyFile(g.file)
}

func (g *FakeGroups) Save(f groups.File) error {
	g.record("Save", f)
	if err := errs(g.Errs).of("Save"); err != nil {
		return err
	}
	g.mu.Lock()
	g.file = copyFile(f)
	g.mu.Unlock()
	return nil
}

func (g *FakeGroups) Queue(sessionID string, n groups.Note) error {
	g.record("Queue", sessionID, n)
	if err := errs(g.Errs).of("Queue"); err != nil {
		return err
	}
	g.mu.Lock()
	if g.queue == nil {
		g.queue = map[string][]groups.Note{}
	}
	g.queue[sessionID] = append(g.queue[sessionID], n)
	g.mu.Unlock()
	return nil
}

// Queued returns the notes queued for a session.
func (g *FakeGroups) Queued(sessionID string) []groups.Note {
	g.mu.Lock()
	defer g.mu.Unlock()
	return append([]groups.Note(nil), g.queue[sessionID]...)
}

// ---- Events ----

// FakeEvents hands out pushed events on the next Poll.
type FakeEvents struct {
	mu      sync.Mutex
	pending []events.Event
}

// Push queues events for the next Poll.
func (e *FakeEvents) Push(evs ...events.Event) {
	e.mu.Lock()
	e.pending = append(e.pending, evs...)
	e.mu.Unlock()
}

func (e *FakeEvents) Poll() []events.Event {
	e.mu.Lock()
	defer e.mu.Unlock()
	out := e.pending
	e.pending = nil
	return out
}

// ---- Panes ----

// FakePanes starts FakePanes and remembers them by id.
type FakePanes struct {
	Recorder
	mu    sync.Mutex
	panes map[string]*FakePane
	Err   error
}

func (ps *FakePanes) Start(id, title, dir string, cols, rows int, argv ...string) (app.Pane, error) {
	ps.record("Start", id, title, dir, cols, rows, argv)
	if ps.Err != nil {
		return nil, ps.Err
	}
	p := NewFakePane(id, title)
	p.Dir, p.Argv, p.cols, p.rows = dir, argv, cols, rows
	ps.mu.Lock()
	if ps.panes == nil {
		ps.panes = map[string]*FakePane{}
	}
	ps.panes[id] = p
	ps.mu.Unlock()
	return p, nil
}

// Pane returns the last pane started with id (nil if none).
func (ps *FakePanes) Pane(id string) *FakePane {
	ps.mu.Lock()
	defer ps.mu.Unlock()
	return ps.panes[id]
}

// ErrClosed is what a closed FakePane's Write returns.
var ErrClosed = errors.New("apptest: pane closed")

// FakePane is a pane with no process: Emit plays program output, Exit ends it.
type FakePane struct {
	Dir  string
	Argv []string

	id         string
	mu         sync.Mutex
	title      string
	cols, rows int
	written    []byte
	subs       map[int]chan []byte
	nextSub    int
	out        chan struct{}
	exited     chan error
	done       bool
	closed     bool
}

// NewFakePane makes a running pane.
func NewFakePane(id, title string) *FakePane {
	return &FakePane{id: id, title: title, subs: map[int]chan []byte{},
		out: make(chan struct{}, 1), exited: make(chan error, 1)}
}

func (p *FakePane) ID() string { return p.id }

func (p *FakePane) Write(b []byte) (int, error) {
	p.mu.Lock()
	defer p.mu.Unlock()
	if p.closed {
		return 0, ErrClosed
	}
	p.written = append(p.written, b...)
	return len(b), nil
}

// Written is everything written to the pane so far.
func (p *FakePane) Written() string {
	p.mu.Lock()
	defer p.mu.Unlock()
	return string(p.written)
}

func (p *FakePane) Resize(cols, rows int) {
	p.mu.Lock()
	p.cols, p.rows = cols, rows
	p.mu.Unlock()
}

// Size is the last size set by Start or Resize.
func (p *FakePane) Size() (cols, rows int) {
	p.mu.Lock()
	defer p.mu.Unlock()
	return p.cols, p.rows
}

func (p *FakePane) Output() <-chan struct{} { return p.out }

func (p *FakePane) Subscribe() (<-chan []byte, func()) {
	p.mu.Lock()
	defer p.mu.Unlock()
	id := p.nextSub
	p.nextSub++
	ch := make(chan []byte, 64)
	if p.done {
		close(ch)
		return ch, func() {}
	}
	p.subs[id] = ch
	return ch, func() {
		p.mu.Lock()
		defer p.mu.Unlock()
		if c, ok := p.subs[id]; ok {
			delete(p.subs, id)
			close(c)
		}
	}
}

// Emit plays program output: subscribers get the bytes (dropped if their
// buffer is full) and Output is signalled (coalesced).
func (p *FakePane) Emit(b []byte) {
	p.mu.Lock()
	for _, ch := range p.subs {
		select {
		case ch <- append([]byte(nil), b...):
		default:
		}
	}
	p.mu.Unlock()
	select {
	case p.out <- struct{}{}:
	default:
	}
}

// Exit ends the program with err, like term.Process: the first receive on
// Exited gets err, later ones nil, and every subscriber is closed. Calling it
// again does nothing.
func (p *FakePane) Exit(err error) {
	p.mu.Lock()
	defer p.mu.Unlock()
	if p.done {
		return
	}
	p.done = true
	for id, ch := range p.subs {
		delete(p.subs, id)
		close(ch)
	}
	p.exited <- err
	close(p.exited)
}

func (p *FakePane) Exited() <-chan error { return p.exited }

func (p *FakePane) Done() bool {
	p.mu.Lock()
	defer p.mu.Unlock()
	return p.done
}

func (p *FakePane) Title() string {
	p.mu.Lock()
	defer p.mu.Unlock()
	return p.title
}

// SetTitle plays the program setting its window title.
func (p *FakePane) SetTitle(t string) {
	p.mu.Lock()
	p.title = t
	p.mu.Unlock()
}

func (p *FakePane) Close() {
	p.mu.Lock()
	p.closed = true
	p.mu.Unlock()
}

// Closed reports whether Close was called.
func (p *FakePane) Closed() bool {
	p.mu.Lock()
	defer p.mu.Unlock()
	return p.closed
}

// ---- Notifier / Opener ----

// Note is one desktop notification.
type Note struct{ Title, Body string }

// FakeNotifier records notifications; Err makes Notify fail (and not record).
type FakeNotifier struct {
	mu    sync.Mutex
	notes []Note
	Err   error
}

func (n *FakeNotifier) Notify(title, body string) error {
	if n.Err != nil {
		return n.Err
	}
	n.mu.Lock()
	n.notes = append(n.notes, Note{title, body})
	n.mu.Unlock()
	return nil
}

// Notes returns the notifications shown so far.
func (n *FakeNotifier) Notes() []Note {
	n.mu.Lock()
	defer n.mu.Unlock()
	return append([]Note(nil), n.notes...)
}

// FakeOpener records URLs; Err makes OpenURL fail (and not record).
type FakeOpener struct {
	mu   sync.Mutex
	urls []string
	Err  error
}

func (o *FakeOpener) OpenURL(u string) error {
	if o.Err != nil {
		return o.Err
	}
	o.mu.Lock()
	o.urls = append(o.urls, u)
	o.mu.Unlock()
	return nil
}

// URLs returns the URLs opened so far.
func (o *FakeOpener) URLs() []string {
	o.mu.Lock()
	defer o.mu.Unlock()
	return append([]string(nil), o.urls...)
}

// ---- Jira ----

// FakeJira answers Cached with Disk, Fetch with Cache and Create with Key/Cost.
type FakeJira struct {
	Recorder
	Disk  jira.Cache // what "the disk" holds before any fetch
	Cache jira.Cache
	Key   string
	Cost  float64
	Errs  map[string]error
}

func (j *FakeJira) Cached() jira.Cache {
	j.record("Cached")
	return j.Disk
}

func (j *FakeJira) Fetch(cfg jira.Config) (jira.Cache, error) {
	j.record("Fetch", cfg)
	if err := errs(j.Errs).of("Fetch"); err != nil {
		return jira.Cache{}, err
	}
	return j.Cache, nil
}

func (j *FakeJira) Create(cfg jira.Config, n jira.NewIssue) (string, float64, error) {
	j.record("Create", cfg, n)
	if err := errs(j.Errs).of("Create"); err != nil {
		return "", j.Cost, err
	}
	return j.Key, j.Cost, nil
}

// ---- Todo ----

// FakeTodo answers List with Groups; Avail is what Available says.
type FakeTodo struct {
	Recorder
	Avail  bool
	Groups []todo.Group
	Errs   map[string]error
}

func (t *FakeTodo) Available() bool { t.record("Available"); return t.Avail }

func (t *FakeTodo) List() ([]todo.Group, error) {
	t.record("List")
	if err := errs(t.Errs).of("List"); err != nil {
		return nil, err
	}
	return t.Groups, nil
}

func (t *FakeTodo) Start(id int) error { t.record("Start", id); return errs(t.Errs).of("Start") }
func (t *FakeTodo) Done(id int) error  { t.record("Done", id); return errs(t.Errs).of("Done") }
func (t *FakeTodo) LinkJira(id int, key string) error {
	t.record("LinkJira", id, key)
	return errs(t.Errs).of("LinkJira")
}
func (t *FakeTodo) AddFromJira(key, summary string) error {
	t.record("AddFromJira", key, summary)
	return errs(t.Errs).of("AddFromJira")
}
func (t *FakeTodo) AddNote(id int, text string) error {
	t.record("AddNote", id, text)
	return errs(t.Errs).of("AddNote")
}

// Compile-time: every fake satisfies its port.
var (
	_ app.Clock             = (*FakeClock)(nil)
	_ app.ClaudeCLI         = (*FakeClaude)(nil)
	_ app.SessionRepository = (*FakeSessions)(nil)
	_ app.GroupStore        = (*FakeGroups)(nil)
	_ app.EventLog          = (*FakeEvents)(nil)
	_ app.PTYSpawner        = (*FakePanes)(nil)
	_ app.Pane              = (*FakePane)(nil)
	_ app.Notifier          = (*FakeNotifier)(nil)
	_ app.Opener            = (*FakeOpener)(nil)
	_ app.JiraClient        = (*FakeJira)(nil)
	_ app.TodoClient        = (*FakeTodo)(nil)
)
