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
}

// New builds the core over p. A nil Clock means wall time; the other ports
// must be set for the use cases that need them (apptest.Ports has fakes).
func New(cfg config.Config, p Ports) *App {
	if p.Clock == nil {
		p.Clock = wallClock{}
	}
	return &App{
		cfg: cfg, ports: p, clock: p.Clock,
		bus:     NewBroker(p.Clock),
		reg:     NewRegistry(),
		pending: map[string]*pendingAction{},
	}
}

func (a *App) Config() config.Config { return a.cfg }
func (a *App) Ports() Ports          { return a.ports }
func (a *App) Clock() Clock          { return a.clock }
func (a *App) Registry() *Registry   { return a.reg }

// Snapshot is the last snapshot Refresh took (zero before the first).
func (a *App) Snapshot() store.Snapshot {
	a.mu.RLock()
	defer a.mu.RUnlock()
	return a.snap
}

// Refresh re-reads the sessions and publishes snapshot.changed.
func (a *App) Refresh() store.Snapshot {
	snap := a.ports.Sessions.Refresh()
	a.mu.Lock()
	a.snap = snap
	a.mu.Unlock()
	d := SnapshotChanged{At: snap.At, Sessions: len(snap.Sessions)}
	if snap.Err != nil {
		d.Err = snap.Err.Error()
	}
	a.Publish(EvSnapshotChanged, d)
	return snap
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
