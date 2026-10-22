// Package app holds nekuroddo's use cases. It talks to the world only through
// the ports (ports.go) and is the one face every client uses: the command
// registry, the event bus and token confirmations. It must not import
// os/exec, net, Bubble Tea or internal/term (internal/archtest checks).
package app

import (
	"context"
	"sync"
	"time"

	"github.com/aqilputi/nekuroddo/internal/config"
	"github.com/aqilputi/nekuroddo/internal/core"
	"github.com/aqilputi/nekuroddo/internal/store"
)

// Event names (plan §2.5). Slices add their own next to their code.
const (
	EvSnapshotChanged  = "snapshot.changed"
	EvToast            = "toast"
	EvConfirmRequested = "confirm.requested"
	EvConfirmResolved  = "confirm.resolved"
	EvCommandRan       = "command.ran"
)

// SnapshotChanged is the payload of snapshot.changed; clients read the
// snapshot itself with App.Snapshot (or GET /v1/snapshot).
type SnapshotChanged struct {
	At       time.Time `json:"at"`
	Sessions int       `json:"sessions"`
	Err      string    `json:"err,omitempty"`
}

// ToastData is the payload of toast.
type ToastData struct {
	Text string `json:"text"`
	Err  bool   `json:"err,omitempty"`
}

// CommandRan is the payload of command.ran.
type CommandRan struct {
	Name string `json:"name"`
	Err  string `json:"err,omitempty"`
}

// App is the core: one per process, shared by every client.
type App struct {
	cfg   config.Config
	ports Ports
	clock Clock
	bus   *Broker
	reg   *Registry

	mu   sync.RWMutex
	snap store.Snapshot

	cmu     sync.Mutex
	pending map[string]*pendingAction

	// Each slice's state, declared next to its use cases.
	grp      groupState    // groups.go
	watching watchState    // watch.go
	panes    *PaneManager  // panes.go
	todoJira todoJiraState // todo.go, jira.go
	tails    tailState     // tail.go
}

// New builds the core over p. A nil Clock means wall time; the other ports
// must be set for the use cases that need them (apptest.Ports has fakes).
// The registry starts empty: RegisterBuiltins fills it.
func New(cfg config.Config, p Ports) *App {
	if p.Clock == nil {
		p.Clock = wallClock{}
	}
	a := &App{
		cfg: cfg, ports: p, clock: p.Clock,
		bus:     NewBroker(p.Clock),
		reg:     NewRegistry(),
		pending: map[string]*pendingAction{},
	}
	a.grp.init(p.Groups)
	a.watching.init()
	a.panes = newPaneManager(a)
	return a
}

// RegisterBuiltins registers every builtin command (plan §2.4): sessions,
// dispatch, panes, groups, todos and Jira, and the client-scope ones. The composition root (ui.New
// today, nk serve later) calls it once; a second call fails with
// ErrDuplicateCommand.
func (a *App) RegisterBuiltins() error {
	for _, register := range []func() error{
		a.RegisterSessionCommands, a.RegisterDispatch, a.RegisterPanes,
		a.RegisterGroupCommands, a.RegisterTodoJira, a.RegisterKeyCommands, a.RegisterWorldCommands,
	} {
		if err := register(); err != nil {
			return err
		}
	}
	return nil
}

func (a *App) Config() config.Config { return a.cfg }
func (a *App) Ports() Ports          { return a.ports }
func (a *App) Clock() Clock          { return a.clock }
func (a *App) Registry() *Registry   { return a.reg }

// Snapshot is a copy of the last snapshot Refresh took (zero before the
// first): session commands change the core's own sessions under a.mu, so a
// client must never hold that slice (it would race and change under it).
func (a *App) Snapshot() store.Snapshot {
	a.mu.RLock()
	defer a.mu.RUnlock()
	return copySnapshot(a.snap)
}

// copySnapshot copies the session slice; the sessions' own slices (subagents,
// questions, links) are only ever replaced by a refresh, never edited.
func copySnapshot(s store.Snapshot) store.Snapshot {
	if s.Sessions != nil {
		s.Sessions = append([]store.Session(nil), s.Sessions...)
	}
	return s
}

// Refresh is RefreshNotices for a caller that does not draw the notices
// (they are published as notice anyway).
func (a *App) Refresh() store.Snapshot {
	snap, _ := a.RefreshNotices()
	return snap
}

// RefreshNotices is the one refresh every client goes through: it re-reads
// the sessions, makes them the core's snapshot and publishes
// snapshot.changed; then it watches the state transitions (ObserveSessions:
// session.state, and a notice for the sessions that came back to the user),
// publishes session.tail for the tails that grew (observeTails) and keeps the group members' names in step (SyncGroupNames). The notices
// go back to the caller, which draws them. The caller gets its own copy of
// the snapshot (see Snapshot); after a session command (archive, rename…) it
// reads Snapshot again to show the change. Safe to call off the client's UI
// goroutine.
func (a *App) RefreshNotices() (store.Snapshot, []Notice) {
	snap := a.ports.Sessions.Refresh()
	a.mu.Lock()
	a.snap = copySnapshot(snap) // the core's own; snap stays the caller's
	a.mu.Unlock()
	d := SnapshotChanged{At: snap.At, Sessions: len(snap.Sessions)}
	if snap.Err != nil {
		d.Err = snap.Err.Error()
	}
	a.Publish(EvSnapshotChanged, d)
	notices := a.ObserveSessions(snap.Sessions)
	a.observeTails(snap.Sessions)
	if a.ports.Groups != nil {
		_, _ = a.SyncGroupNames(snap.Sessions) // best effort: a failed save is retried next refresh
	}
	return snap, notices
}

// Subscribe follows the event bus until ctx ends (see Broker.Subscribe).
func (a *App) Subscribe(ctx context.Context) <-chan core.Event { return a.bus.Subscribe(ctx) }

// Publish puts an event on the bus.
func (a *App) Publish(name string, data any) { a.bus.Publish(core.Event{Name: name, Data: data}) }

// Toast publishes a toast for every client.
func (a *App) Toast(text string, isErr bool) { a.Publish(EvToast, ToastData{Text: text, Err: isErr}) }

// Run executes a registered command and publishes command.ran.
func (a *App) Run(ctx Context, name string, args Args) (Result, error) {
	res, err := a.reg.Run(ctx, name, args)
	ran := CommandRan{Name: name}
	if err != nil {
		ran.Err = err.Error()
	}
	a.Publish(EvCommandRan, ran)
	return res, err
}

// Close ends every subscription.
func (a *App) Close() { a.bus.Close() }
