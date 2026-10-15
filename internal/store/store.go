// Package store merges `claude agents --json` with the transcripts into the
// session list the UI shows. A Store is owned by one goroutine at a time (the
// refresh command); the UI only ever sees the immutable Snapshot it returns.
package store

import (
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"time"

	"github.com/aqilputi/neko/internal/claude"
	"github.com/aqilputi/neko/internal/config"
)

// Attention ranks why a session needs the user. Lower is more urgent.
type Attention int

const (
	AttnNone     Attention = iota
	AttnBlocked            // waiting for an answer or a permission
	AttnReturned           // finished a turn/task you haven't looked at
	AttnIdle               // interactive, idle at the prompt
)

type Sub struct {
	ID, Description, Type, Model, LastText string
	Cost                                   float64
	LastAt                                 time.Time
	Done                                   bool
}

type Session struct {
	ShortID   string // background id for `claude attach`; "" for interactive/history
	SessionID string
	Name      string
	Title     string
	Cwd       string
	Context   string
	Kind      string // background | interactive | history
	State     string // normalized: busy, blocked, done, stopped, idle, history
	PID       int
	Started   time.Time
	LastAt    time.Time

	Model string
	Cost  float64 // incl. subagents
	// WindowCost is the usage weight since the current 5-hour window began;
	// the UI turns it into this session's share of the window's %.
	WindowCost  float64
	CostToday   float64
	Tokens      int64
	LastUser    string
	LastText    string
	LastTool    string
	WaitingTool string
	Subs        []Sub

	// From Claude Code's own state files.
	WaitingFor     string // interactive: "permission prompt", "input needed", …
	Detail         string // background: one-line status written by the daemon
	Needs          string // background: what it needs from you
	SuggestedReply string
	Questions      []claude.Question
	Links          []claude.Child // PRs and artifacts it produced
	Result         string

	Attention Attention
	// Stale: idle longer than stale_days (or archived by hand). Stale sessions
	// never ask for attention and are hidden unless "show all" is on.
	Stale      bool
	Archived   bool
	ArchivedAt time.Time
	// Ended: the chat was closed for good (it accepts no more messages);
	// Finished: a background job that is done/stopped/failed and already
	// seen. Both are hidden like stale ones.
	Ended    bool
	Finished bool
}

// Label is the best human name for the session.
func (s Session) Label() string {
	// Names Claude derives on its own say nothing: "<dir>-3b" for interactive
	// sessions, or the session's own id for one resumed in the background.
	derived := s.Name == "" || strings.HasPrefix(s.Name, filepath.Base(s.Cwd)+"-") ||
		s.Name == s.ShortID || strings.HasPrefix(s.SessionID, s.Name)
	if s.Title != "" && derived {
		return s.Title
	}
	if s.Name != "" {
		return s.Name
	}
	if s.Title != "" {
		return s.Title
	}
	return s.SessionID[:min(8, len(s.SessionID))]
}

// Attachable reports whether the session can be opened in the embedded pane
// as is (background sessions) or after resuming into the daemon (history).
func (s Session) Attachable() bool { return s.ShortID != "" }

type Snapshot struct {
	Sessions  []Session
	Err       error
	At        time.Time
	TodayCost float64
}

type Store struct {
	cfg   config.Config
	trans map[string]*claude.Transcript

	refreshMu sync.Mutex // serializes Refresh: transcripts are not safe for concurrent use

	windowStart time.Time // start of the current 5-hour window (from /usage)

	mu       sync.Mutex           // guards seen/archived/windowStart: written by the UI, read by Refresh
	seen     map[string]time.Time // sessionId → LastAt the user has looked at
	archived map[string]time.Time // sessionId → when it was archived by hand
}

func New(cfg config.Config) *Store {
	s := &Store{cfg: cfg, trans: map[string]*claude.Transcript{}, seen: map[string]time.Time{}, archived: map[string]time.Time{}}
	s.loadSeen()
	return s
}

func statePath(name string) string {
	dir := os.Getenv("XDG_STATE_HOME")
	if dir == "" {
		home, _ := os.UserHomeDir()
		dir = filepath.Join(home, ".local", "state")
	}
	return filepath.Join(dir, "neko", name)
}

func (s *Store) loadSeen() {
	if b, err := os.ReadFile(statePath("seen.json")); err == nil {
		_ = json.Unmarshal(b, &s.seen)
	}
	if b, err := os.ReadFile(statePath("archived.json")); err == nil {
		// v1 stored sessionId → true; v2 stores the archive time
		var raw map[string]json.RawMessage
		_ = json.Unmarshal(b, &raw)
		for id, v := range raw {
			var at time.Time
			if json.Unmarshal(v, &at) != nil {
				at = time.Now()
			}
			s.archived[id] = at
		}
	}
}

// SetWindowStart tells Refresh when the current 5-hour window began.
func (s *Store) SetWindowStart(t time.Time) {
	s.mu.Lock()
	s.windowStart = t
	s.mu.Unlock()
}

func (s *Store) window() time.Time {
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.windowStart.IsZero() {
		return time.Now().Add(-5 * time.Hour)
	}
	return s.windowStart
}

// MarkSeen records that the user looked at the session up to `at`.
func (s *Store) MarkSeen(sessionID string, at time.Time) {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.seen[sessionID] = at
	_ = os.MkdirAll(filepath.Dir(statePath("seen.json")), 0o755)
	if b, err := json.Marshal(s.seen); err == nil {
		_ = os.WriteFile(statePath("seen.json"), b, 0o644)
	}
}

func (s *Store) saveArchived() {
	_ = os.MkdirAll(filepath.Dir(statePath("archived.json")), 0o755)
	if b, err := json.Marshal(s.archived); err == nil {
		_ = os.WriteFile(statePath("archived.json"), b, 0o644)
	}
}

// Archive hides sessions by hand. They come back on their own if they show
// activity after this moment (see Refresh).
func (s *Store) Archive(ids ...string) {
	s.mu.Lock()
	defer s.mu.Unlock()
	now := time.Now()
	for _, id := range ids {
		s.archived[id] = now
	}
	s.saveArchived()
}

// Unarchive brings sessions back.
func (s *Store) Unarchive(ids ...string) {
	s.mu.Lock()
	defer s.mu.Unlock()
	for _, id := range ids {
		delete(s.archived, id)
	}
	s.saveArchived()
}

func (s *Store) pricer() claude.Pricer {
	return func(model string) (float64, float64, float64, float64) {
		p := s.cfg.PriceFor(model)
		return p.Input, p.Output, p.CacheWrite, p.CacheRead
	}
}

func (s *Store) transcript(path string) *claude.Transcript {
	t := s.trans[path]
	if t == nil {
		t = claude.NewTranscript(path)
		s.trans[path] = t
	}
	_ = t.Refresh(s.pricer())
	return t
}

// Refresh polls the CLI and the transcripts and returns a fresh snapshot.
func (s *Store) Refresh() Snapshot {
	s.refreshMu.Lock()
	defer s.refreshMu.Unlock()
	snap := Snapshot{At: time.Now()}
	win := s.window()
	agents, err := claude.ListAgents()
	snap.Err = err

	bySID := map[string]bool{}
	for _, a := range agents {
		bySID[a.SessionID] = true
		sess := Session{
			SessionID: a.SessionID, Name: a.Name, Cwd: a.Cwd, Kind: a.Kind, PID: a.PID,
			Started: time.UnixMilli(a.StartedAt),
		}
		if a.Kind == "background" {
			sess.ShortID = a.ID
			sess.State = normState(a.State)
			if j := claude.ReadJob(a.ID); j != nil {
				sess.Detail, sess.Needs, sess.SuggestedReply, sess.Links = j.Detail, j.Needs, j.SuggestedReply, j.Children
				if j.Block != nil {
					sess.Questions = j.Block.Questions
				}
				if j.Output != nil {
					sess.Result = j.Output.Result
				}
			}
		} else {
			sess.State = normState(a.Status)
			if l := claude.ReadLive(a.PID); l != nil {
				sess.State = normState(l.Status)
				sess.WaitingFor = l.WaitingFor
			}
		}
		if p := claude.TranscriptPath(a.Cwd, a.SessionID); p != "" {
			fill(&sess, s.transcript(p), win)
		}
		snap.Sessions = append(snap.Sessions, sess)
	}

	if days := s.cfg.ShowHistoryDays; days > 0 {
		for _, p := range claude.RecentTranscripts(time.Now().AddDate(0, 0, -days)) {
			sid := strings.TrimSuffix(filepath.Base(p), ".jsonl")
			if bySID[sid] {
				continue
			}
			t := s.transcript(p)
			if t.Requests == 0 || t.Cwd == "" {
				continue
			}
			sess := Session{SessionID: sid, Cwd: t.Cwd, Kind: "history", State: "history", Started: t.Started}
			fill(&sess, t, win)
			snap.Sessions = append(snap.Sessions, sess)
		}
	}

	for i := range snap.Sessions {
		ss := &snap.Sessions[i]
		ss.Context = s.cfg.ContextFor(ss.Cwd)
		if ss.LastAt.IsZero() { // no transcript yet: its start is its last activity
			ss.LastAt = ss.Started
		}
		s.mu.Lock()
		if at, ok := s.archived[ss.SessionID]; ok {
			if ss.LastAt.After(at.Add(time.Minute)) {
				delete(s.archived, ss.SessionID) // active again: unarchive on its own
				s.saveArchived()
			} else {
				ss.Archived, ss.ArchivedAt = true, at
			}
		}
		s.mu.Unlock()
		idle := time.Since(ss.LastAt) > time.Duration(s.cfg.StaleDays*24*float64(time.Hour))
		ss.Stale = ss.Archived || (s.cfg.StaleDays > 0 && idle && ss.State != "busy")
		ss.Attention = s.attention(*ss)
		switch ss.State {
		case "done", "stopped", "failed":
			ss.Finished = ss.Kind == "background" && ss.Attention == AttnNone
		}
		if ss.Ended || ss.Finished {
			ss.Stale = true // closed chats and seen finished jobs stay out of the way (H shows them)
		}
		ss.Attention = s.attention(*ss)
		snap.TodayCost += ss.CostToday
	}
	return snap
}

func normState(s string) string {
	switch s {
	case "busy", "shell", "running", "working", "active", "starting":
		return "busy"
	case "blocked", "waiting", "needs_input", "permission":
		return "blocked"
	case "done", "completed", "finished", "exited":
		return "done"
	case "stopped", "killed":
		return "stopped"
	case "failed", "crashed":
		return "failed"
	case "idle", "ready":
		return "idle"
	case "":
		return "idle"
	}
	return s
}

func fill(sess *Session, t *claude.Transcript, window time.Time) {
	sess.Title = t.Title
	sess.WindowCost = t.CostSince(window)
	if t.Ended {
		sess.Ended = true
		sess.State = "ended" // the daemon may still report it as working
	}
	if sess.Cwd == "" {
		sess.Cwd = t.Cwd
	}
	sess.Model, sess.Cost, sess.Tokens = t.Model, t.TotalCost(), t.Tokens
	sess.CostToday = t.CostByDay[time.Now().Format("2006-01-02")]
	sess.LastUser, sess.LastText, sess.LastTool, sess.WaitingTool = t.LastUser, t.LastText, t.LastTool, t.WaitingTool
	sess.LastAt = t.LastAt
	if sess.LastAt.IsZero() {
		sess.LastAt = sess.Started
	}
	for _, sa := range t.Subagents {
		sess.Subs = append(sess.Subs, Sub{
			ID: sa.ID, Description: sa.Description, Type: sa.Type, Model: sa.Model,
			LastText: sa.LastText, Cost: sa.Cost, LastAt: sa.LastAt, Done: sa.Done,
		})
	}
}

func (s *Store) attention(ss Session) Attention {
	if ss.Stale || ss.Ended {
		return AttnNone
	}
	s.mu.Lock()
	seen := s.seen[ss.SessionID]
	s.mu.Unlock()
	switch ss.State {
	case "blocked":
		return AttnBlocked
	case "done":
		if ss.LastAt.After(seen) {
			return AttnReturned
		}
	case "idle":
		if ss.Kind == "interactive" && ss.LastAt.After(seen) {
			return AttnIdle
		}
	}
	return AttnNone
}
