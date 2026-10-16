// Package demo is a scripted fake world behind the app ports, for
// `nk serve --demo` and the GUI's Playwright tests: three contexts, seven
// sessions (busy, blocked on a question, returned, idle, stale, one with two
// subagents), the group #deploy-rc, a ✉ every 20 s, state changes every 15 s,
// usage near 60%, todos and Jira issues, and panes that run a harmless shell
// script drawing a Claude-like screen. It never runs claude.
//
// The script (timeline.go) is a pure function of the time since the start,
// read from the Clock, so a fake clock makes it deterministic. On top of it,
// Scenario keeps what the user did (renames, archive, dispatches, stops…) so
// the core's commands have visible effects.
package demo

import (
	"fmt"
	"strings"
	"sync"
	"time"

	"github.com/aqilputi/nekuroddo/internal/app"
	"github.com/aqilputi/nekuroddo/internal/core"
	"github.com/aqilputi/nekuroddo/internal/groups"
	"github.com/aqilputi/nekuroddo/internal/jira"
	"github.com/aqilputi/nekuroddo/internal/store"
	"github.com/aqilputi/nekuroddo/internal/todo"
)

// DispatchBusy is how long a dispatched or resumed session stays busy.
const DispatchBusy = 30 * time.Second

// Bounds that keep a long-running demo from growing without end.
const (
	maxBurst  = 16  // hook events handed out by one Poll (the latest)
	maxKept   = 100 // notifications and opened URLs remembered
	maxQueued = 50  // group notes queued per session
)

// wallClock is the default Clock.
type wallClock struct{}

func (wallClock) Now() time.Time { return time.Now() }

// Note is a desktop notification the demo swallowed.
type Note struct{ Title, Body string }

// extra is a session the user dispatched.
type extra struct {
	short, name, prompt, dir string
	at                       time.Time
}

// Scenario is the fake world. Build one with New, hand Ports to app.New.
// It is safe for concurrent use.
type Scenario struct {
	clock app.Clock
	t0    time.Time

	mu       sync.Mutex
	names    map[string]string    // session id → custom name
	ctx      map[string]string    // session id → context set by hand
	ctxNames map[string]string    // context → shown name
	archived map[string]time.Time // session id → when
	seen     map[string]time.Time // session id → last looked at
	stopped  map[string]bool      // session id
	removed  map[string]bool      // session id
	resumed  map[string]extra     // session id → its new short id and when
	extras   []extra
	nextID   int
	lastPoll time.Duration

	groups groups.File
	queue  map[string][]groups.Note
	notes  []Note
	urls   []string
	issues []jira.Issue
	todos  []todo.Group
	nextTD int
}

// New starts a scenario whose timeline begins at clock.Now() (the wall
// clock when nil).
func New(clock app.Clock) *Scenario {
	if clock == nil {
		clock = wallClock{}
	}
	s := &Scenario{clock: clock, t0: clock.Now(),
		names: map[string]string{}, ctx: map[string]string{}, ctxNames: map[string]string{},
		archived: map[string]time.Time{}, seen: map[string]time.Time{}, stopped: map[string]bool{},
		removed: map[string]bool{}, resumed: map[string]extra{}, queue: map[string][]groups.Note{},
		issues: seedIssues(), todos: seedTodos(), nextTD: 6}
	s.groups = groups.File{Groups: map[string][]groups.Member{}}
	for _, id := range GroupMembers {
		x := s.lookupLocked(id)
		s.groups.Groups[GroupTag] = append(s.groups.Groups[GroupTag],
			groups.Member{SessionID: id, Name: x.Name, Label: x.Label(), Cwd: x.Cwd})
	}
	return s
}

// Ports is New(clock).Ports().
func Ports(clock app.Clock) app.Ports { return New(clock).Ports() }

// Ports exposes the scenario as app ports (every one of them set).
func (s *Scenario) Ports() app.Ports {
	return app.Ports{Clock: s.clock, Claude: claudePort{s}, Sessions: sessionsPort{s}, Groups: groupsPort{s},
		Events: eventsPort{s}, Panes: Panes{s: s}, Notify: notifyPort{s}, Open: openPort{s},
		Jira: jiraPort{s}, Todo: todoPort{s}}
}

// Start is when the timeline began.
func (s *Scenario) Start() time.Time { return s.t0 }

// Elapsed is how far into the timeline the clock is (never negative).
func (s *Scenario) Elapsed() time.Duration { return max(s.clock.Now().Sub(s.t0), 0) }

// Notes are the desktop notifications swallowed so far (the latest maxKept).
func (s *Scenario) Notes() []Note {
	s.mu.Lock()
	defer s.mu.Unlock()
	return append([]Note(nil), s.notes...)
}

// URLs are the URLs the core asked to open (never opened; the latest maxKept).
func (s *Scenario) URLs() []string {
	s.mu.Lock()
	defer s.mu.Unlock()
	return append([]string(nil), s.urls...)
}

// Queued are the group notes queued for a session.
func (s *Scenario) Queued(id string) []groups.Note {
	s.mu.Lock()
	defer s.mu.Unlock()
	return append([]groups.Note(nil), s.queue[id]...)
}

func keepLast[T any](xs []T, n int) []T {
	if len(xs) > n {
		return append([]T(nil), xs[len(xs)-n:]...)
	}
	return xs
}

// ---- the world now ----

// sessionsLocked is the script at the clock's time plus what the user did.
func (s *Scenario) sessionsLocked() []store.Session {
	e := s.Elapsed()
	now := s.t0.Add(e)
	var out []store.Session
	for _, x := range Sessions(s.t0, e) {
		if !s.removed[x.SessionID] {
			out = append(out, x)
		}
	}
	for i, x := range s.extras {
		id := extraID(i)
		if s.removed[id] {
			continue
		}
		out = append(out, s.extraSession(id, x, now))
	}
	for i := range out {
		s.applyLocked(&out[i], now)
	}
	return out
}

func extraID(i int) string { return fmt.Sprintf("d3e0b%03d-0000-4000-8000-%012d", i+1, i+1) }

// extraSession is a dispatched session: busy for DispatchBusy, then done.
func (s *Scenario) extraSession(id string, x extra, now time.Time) store.Session {
	ctx := "demo"
	if rest, ok := strings.CutPrefix(x.dir, "/demo/"); ok {
		ctx, _, _ = strings.Cut(rest, "/")
	}
	name := x.name
	if name == "" {
		name = core.Truncate(x.prompt, 40)
	}
	out := store.Session{SessionID: id, ShortID: x.short, Name: name, Cwd: x.dir, Context: ctx, Kind: "background",
		Started: x.at, FirstUser: x.prompt, Model: "claude-sonnet-5", State: "busy", LastAt: now,
		Detail: "trabalhando em: " + core.Truncate(x.prompt, 60)}
	if now.Sub(x.at) >= DispatchBusy {
		out.State, out.Attention, out.LastAt = "done", store.AttnReturned, x.at.Add(DispatchBusy)
		out.Result = "(demo) pronto — nada foi feito de verdade."
	}
	return out
}

// applyLocked lays the user's actions over one scripted session.
func (s *Scenario) applyLocked(x *store.Session, now time.Time) {
	if r, ok := s.resumed[x.SessionID]; ok {
		x.ShortID, x.Kind, x.Stale = r.short, "background", false
		x.State, x.LastAt = "busy", now
		if now.Sub(r.at) >= DispatchBusy {
			x.State, x.LastAt = "idle", r.at.Add(DispatchBusy)
		}
	}
	if s.stopped[x.SessionID] {
		x.State, x.Attention, x.Questions, x.Needs, x.WaitingTool = "stopped", store.AttnNone, nil, "", ""
		subs := append([]store.Sub(nil), x.Subs...)
		for i := range subs {
			subs[i].Done = true
		}
		x.Subs = subs
	}
	if at, ok := s.seen[x.SessionID]; ok && x.Attention == store.AttnReturned && !x.LastAt.After(at) {
		x.Attention, x.Finished = store.AttnNone, x.Kind == "background"
	}
	s.applyNamesLocked(x)
}

// applyNamesLocked is the part Reapply redoes in place: names, contexts and
// the archive.
func (s *Scenario) applyNamesLocked(x *store.Session) {
	x.CustomName = s.names[x.SessionID]
	if c, ok := s.ctx[x.SessionID]; ok {
		x.Context, x.ContextSet = c, true
	} else if base, ok := s.baseContext(x.SessionID); ok {
		x.Context, x.ContextSet = base, false
	}
	if n, ok := s.ctxNames[x.Context]; ok && !x.ContextSet {
		x.Context = n
	}
	if at, ok := s.archived[x.SessionID]; ok {
		x.Archived, x.ArchivedAt, x.Stale = true, at, true
	} else if x.Archived {
		x.Archived, x.ArchivedAt = false, time.Time{}
	}
}

// baseContext is the scripted context of a session, before any override.
func (s *Scenario) baseContext(id string) (string, bool) {
	for _, x := range Sessions(s.t0, 0) {
		if x.SessionID == id {
			return x.Context, true
		}
	}
	return "", false
}

func (s *Scenario) lookupLocked(id string) store.Session {
	for _, x := range s.sessionsLocked() {
		if x.SessionID == id {
			return x
		}
	}
	return store.Session{}
}

// byShortLocked finds a listed session by its background id.
func (s *Scenario) byShortLocked(short string) (store.Session, bool) {
	for _, x := range s.sessionsLocked() {
		if x.ShortID != "" && x.ShortID == short {
			return x, true
		}
	}
	return store.Session{}, false
}

// ---- SessionRepository ----

type sessionsPort struct{ s *Scenario }

func (p sessionsPort) Refresh() store.Snapshot {
	p.s.mu.Lock()
	defer p.s.mu.Unlock()
	ss := p.s.sessionsLocked()
	snap := store.Snapshot{Sessions: ss, At: p.s.clock.Now()}
	for _, x := range ss {
		snap.TodayCost += x.CostToday
	}
	return snap
}

func (p sessionsPort) Reapply(sessions []store.Session) {
	p.s.mu.Lock()
	defer p.s.mu.Unlock()
	for i := range sessions {
		p.s.applyNamesLocked(&sessions[i])
	}
}

func (p sessionsPort) Archive(ids ...string) {
	p.s.mu.Lock()
	defer p.s.mu.Unlock()
	for _, id := range ids {
		p.s.archived[id] = p.s.clock.Now()
	}
}

func (p sessionsPort) Unarchive(ids ...string) {
	p.s.mu.Lock()
	defer p.s.mu.Unlock()
	for _, id := range ids {
		delete(p.s.archived, id)
	}
}

func setOrDelete(m map[string]string, k, v string) {
	if v == "" {
		delete(m, k)
		return
	}
	m[k] = v
}

func (p sessionsPort) Rename(id, name string) {
	p.s.mu.Lock()
	defer p.s.mu.Unlock()
	setOrDelete(p.s.names, id, name)
}

func (p sessionsPort) SetContext(id, ctx string) {
	p.s.mu.Lock()
	defer p.s.mu.Unlock()
	setOrDelete(p.s.ctx, id, ctx)
}

func (p sessionsPort) RenameContext(shown, name string) {
	p.s.mu.Lock()
	defer p.s.mu.Unlock()
	if name == shown {
		name = ""
	}
	setOrDelete(p.s.ctxNames, shown, name)
}

func (p sessionsPort) MarkSeen(id string, at time.Time) {
	p.s.mu.Lock()
	defer p.s.mu.Unlock()
	p.s.seen[id] = at
}

func (sessionsPort) SetWindowStart(time.Time) {}

// ---- GroupStore ----

type groupsPort struct{ s *Scenario }

func copyGroups(f groups.File) groups.File {
	c := groups.File{Groups: map[string][]groups.Member{}}
	for tag, ms := range f.Groups {
		c.Groups[tag] = append([]groups.Member(nil), ms...)
	}
	return c
}

func (p groupsPort) Load() groups.File {
	p.s.mu.Lock()
	defer p.s.mu.Unlock()
	return copyGroups(p.s.groups)
}

func (p groupsPort) Save(f groups.File) error {
	p.s.mu.Lock()
	defer p.s.mu.Unlock()
	p.s.groups = copyGroups(f)
	return nil
}

func (p groupsPort) Queue(id string, n groups.Note) error {
	p.s.mu.Lock()
	defer p.s.mu.Unlock()
	p.s.queue[id] = keepLast(append(p.s.queue[id], n), maxQueued)
	return nil
}

// ---- Notifier, Opener ----

type notifyPort struct{ s *Scenario }

func (p notifyPort) Notify(title, body string) error {
	p.s.mu.Lock()
	defer p.s.mu.Unlock()
	p.s.notes = keepLast(append(p.s.notes, Note{title, body}), maxKept)
	return nil
}

type openPort struct{ s *Scenario }

func (p openPort) OpenURL(u string) error {
	p.s.mu.Lock()
	defer p.s.mu.Unlock()
	p.s.urls = keepLast(append(p.s.urls, u), maxKept)
	return nil
}
