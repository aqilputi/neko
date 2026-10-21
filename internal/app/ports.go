package app

import (
	"time"

	"github.com/aqilputi/nekuroddo/internal/claude"
	"github.com/aqilputi/nekuroddo/internal/events"
	"github.com/aqilputi/nekuroddo/internal/groups"
	"github.com/aqilputi/nekuroddo/internal/jira"
	"github.com/aqilputi/nekuroddo/internal/store"
	"github.com/aqilputi/nekuroddo/internal/todo"
)

// The ports are the only way app reaches the world. internal/wire binds them
// to the real packages; internal/app/apptest has in-memory fakes.

// Clock is the time source for anything a test needs to control (TTLs, cues).
type Clock interface{ Now() time.Time }

// ClaudeCLI is the Claude Code CLI (package claude).
type ClaudeCLI interface {
	ListAgents() ([]claude.Agent, error)
	Dispatch(dir, name, prompt string, extra ...string) (shortID string, err error)
	ResumeInBackground(dir, sessionID string) (string, error)
	Stop(id string) error
	Remove(id string) error
	AttachArgv(id string) []string
	FetchUsage() (claude.UsageReport, error)
}

// SessionRepository is the session store; *store.Store already satisfies it.
type SessionRepository interface {
	Refresh() store.Snapshot
	Reapply(sessions []store.Session)
	Archive(ids ...string)
	Unarchive(ids ...string)
	Rename(sessionID, name string)
	SetContext(sessionID, ctx string)
	RenameContext(shown, name string)
	MarkSeen(sessionID string, at time.Time)
	SetWindowStart(t time.Time)
	// Tail is the last n lines of a session's activity (none for a session
	// with no transcript yet). Called off the refresh goroutine.
	Tail(sessionID string, n int) ([]claude.TailLine, error)
}

// GroupStore persists groups and queues notes for sessions (package groups).
type GroupStore interface {
	Load() groups.File
	Save(groups.File) error
	Queue(sessionID string, n groups.Note) error
}

// EventLog yields hook events recorded since the last poll; *events.Tail
// already satisfies it.
type EventLog interface{ Poll() []events.Event }

// PTYSpawner starts a program in a pseudo-terminal.
type PTYSpawner interface {
	Start(id, title, dir string, cols, rows int, argv ...string) (Pane, error)
}

// Pane is the framework-free half of a terminal pane (term.Process).
type Pane interface {
	ID() string
	Write(p []byte) (int, error) // keys/paste, already encoded
	Resize(cols, rows int)
	Output() <-chan struct{}                         // coalesced "new output" signal (local emulator)
	Subscribe() (bytes <-chan []byte, cancel func()) // raw PTY bytes, for remote clients
	Exited() <-chan error
	Done() bool
	Title() string
	Close()
}

// Notifier shows a desktop notification (notify-send today; GUI toast tomorrow).
type Notifier interface {
	Notify(title, body string) error
}

// Opener opens a URL in the user's browser (xdg-open).
type Opener interface{ OpenURL(u string) error }

// JiraClient reads and creates Jira issues through a headless claude.
type JiraClient interface {
	// Cached is the last fetch kept on disk (the real adapter) or the
	// world's own issues (demo, fakes): the core never reads the disk itself.
	Cached() jira.Cache
	Fetch(cfg jira.Config) (jira.Cache, error)
	Create(cfg jira.Config, n jira.NewIssue) (key string, cost float64, err error)
}

// TodoClient is the user's todo list (the todo skill's script).
type TodoClient interface {
	Available() bool
	List() ([]todo.Group, error)
	Start(id int) error
	Done(id int) error
	LinkJira(id int, key string) error
	AddFromJira(key, summary string) error
	AddNote(id int, text string) error
}

// Ports bundles every port app needs.
type Ports struct {
	Clock    Clock
	Claude   ClaudeCLI
	Sessions SessionRepository
	Groups   GroupStore
	Events   EventLog
	Panes    PTYSpawner
	Notify   Notifier
	Open     Opener
	Jira     JiraClient
	Todo     TodoClient
}
