package app

import (
	"errors"
	"fmt"
	"sync"

	"github.com/aqilputi/nekuroddo/internal/core"
)

var (
	ErrUnknownCommand   = errors.New("comando desconhecido")
	ErrDuplicateCommand = errors.New("comando já registrado")
	ErrUnavailable      = errors.New("comando indisponível aqui")
	ErrClientCommand    = errors.New("comando do cliente: não roda no core")
	ErrMissingArg       = errors.New("falta o argumento")
)

// Runner executes a command for the client whose selection is ctx.
type Runner func(ctx Context, args Args) (Result, error)

// Option tunes a registered command.
type Option func(*entry)

// WithAvailable hides the command (from List) and refuses it (in Run) when
// available says no for the caller's selection.
func WithAvailable(available func(Context) bool) Option {
	return func(e *entry) { e.available = available }
}

type entry struct {
	cmd       core.Command
	run       Runner
	available func(Context) bool
}

func (e *entry) ok(ctx Context) bool { return e.available == nil || e.available(ctx) }

// Registry is the one table of named commands behind the keymap, the
// palette, Lua, RPC and the GUI. Every Run looks the name up again, so a
// command replaced or wrapped later (Lua) takes effect immediately.
type Registry struct {
	mu     sync.RWMutex
	order  []string
	byName map[string]*entry
}

// NewRegistry returns an empty registry.
func NewRegistry() *Registry { return &Registry{byName: map[string]*entry{}} }

// Register adds c. Source defaults to "builtin" and Scope to "core"; a core
// command needs a runner, a client one (navigation, folding…) has none.
func (r *Registry) Register(c core.Command, run Runner, opts ...Option) error {
	if c.Name == "" {
		return errors.New("comando sem nome")
	}
	if c.Source == "" {
		c.Source = core.SourceBuiltin
	}
	if c.Scope == "" {
		c.Scope = core.ScopeCore
	}
	if c.Scope != core.ScopeClient && run == nil {
		return fmt.Errorf("comando %s sem execução", c.Name)
	}
	e := &entry{cmd: c, run: run}
	for _, o := range opts {
		o(e)
	}
	r.mu.Lock()
	defer r.mu.Unlock()
	if _, dup := r.byName[c.Name]; dup {
		return fmt.Errorf("%w: %s", ErrDuplicateCommand, c.Name)
	}
	r.byName[c.Name] = e
	r.order = append(r.order, c.Name)
	return nil
}

// Unregister removes a command; it reports whether it existed.
func (r *Registry) Unregister(name string) bool {
	r.mu.Lock()
	defer r.mu.Unlock()
	if _, ok := r.byName[name]; !ok {
		return false
	}
	delete(r.byName, name)
	for i, n := range r.order {
		if n == name {
			r.order = append(r.order[:i], r.order[i+1:]...)
			break
		}
	}
	return true
}

// Wrap replaces a core command's runner with wrap(previous runner).
func (r *Registry) Wrap(name string, wrap func(next Runner) Runner) error {
	r.mu.Lock()
	defer r.mu.Unlock()
	e, ok := r.byName[name]
	if !ok {
		return fmt.Errorf("%w: %s", ErrUnknownCommand, name)
	}
	if e.run == nil {
		return fmt.Errorf("%w: %s", ErrClientCommand, name)
	}
	ne := *e
	ne.run = wrap(e.run)
	r.byName[name] = &ne
	return nil
}

// Lookup returns a command's description.
func (r *Registry) Lookup(name string) (core.Command, bool) {
	r.mu.RLock()
	defer r.mu.RUnlock()
	e, ok := r.byName[name]
	if !ok {
		return core.Command{}, false
	}
	return e.cmd, true
}

// Run executes name for ctx. No lock is held while the runner runs.
func (r *Registry) Run(ctx Context, name string, args Args) (Result, error) {
	r.mu.RLock()
	e, ok := r.byName[name]
	r.mu.RUnlock()
	switch {
	case !ok:
		return Result{}, fmt.Errorf("%w: %s", ErrUnknownCommand, name)
	case e.run == nil:
		return Result{}, fmt.Errorf("%w: %s", ErrClientCommand, name)
	case !e.ok(ctx):
		return Result{}, fmt.Errorf("%w: %s", ErrUnavailable, name)
	}
	for _, a := range e.cmd.Args {
		if a.Required && args[a.Name] == "" {
			return Result{}, fmt.Errorf("%w: %s (%s)", ErrMissingArg, a.Name, name)
		}
	}
	return e.run(ctx, args)
}

// List returns the commands available for ctx, in registration order.
func (r *Registry) List(ctx Context) []core.Command {
	r.mu.RLock()
	es := make([]*entry, 0, len(r.order))
	for _, n := range r.order {
		es = append(es, r.byName[n])
	}
	r.mu.RUnlock()
	var out []core.Command
	for _, e := range es {
		if e.ok(ctx) {
			out = append(out, e.cmd)
		}
	}
	return out
}
