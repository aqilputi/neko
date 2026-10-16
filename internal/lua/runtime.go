// Package lua is nekuroddo's scripting shell (plan §1.4, §5): one gopher-lua
// VM in the core, driven only through internal/app. Lua registers commands
// in the same registry the TUI, RPC and GUI use and follows the same event
// bus, so one init.lua serves every client.
//
// The LState belongs to a single goroutine (the VM loop): every entry from Go
// (a command run, an event, a key, Eval, a reload) is marshalled onto it and
// runs under a time budget enforced with LState.SetContext.
package lua

import (
	"context"
	"errors"
	"fmt"
	"sort"
	"strings"
	"sync"
	"time"

	glua "github.com/yuin/gopher-lua"

	"github.com/aqilputi/nekuroddo/internal/app"
	"github.com/aqilputi/nekuroddo/internal/core"
	"github.com/aqilputi/nekuroddo/internal/paths"
)

// Budgets (plan §5.3).
const (
	// DefaultCommandTimeout bounds a command run from a palette/key/RPC, a
	// file load and Eval (config: lua.command_timeout_ms).
	DefaultCommandTimeout = 5 * time.Second
	// DefaultCallbackTimeout bounds event handlers and keymap functions.
	DefaultCallbackTimeout = 50 * time.Millisecond
	// MaxFailures consecutive callback failures disable a file.
	MaxFailures = 3
	// MaxMessages is how many log/error messages the ring keeps.
	MaxMessages = 100
)

// Events this package publishes on the app bus.
const (
	EvLuaError       = "lua.error"
	EvKeymapChanged  = "keymap.changed"
	EvUIPaletteOpen  = "ui.palette_open"
	EvUICatMeow      = "ui.cat.meow"
	sourcePrefix     = "lua:"
	evalFile         = "<eval>"
	keyCommandPrefix = "lua.key:"
)

// UI capabilities a host declares in Options.Caps (nk.ui.has).
const (
	CapToast   = "toast"
	CapConfirm = "confirm"
	CapCat     = "cat"
	CapPalette = "palette"
	CapScene   = "scene"
	CapNotify  = "notify"
)

var (
	// ErrBudget is a Lua call cut off by its time budget.
	ErrBudget = errors.New("lua: orçamento de tempo estourado")
	// ErrClosed is a call on a closed runtime.
	ErrClosed = errors.New("lua: runtime fechado")
)

// Options configure a Runtime; zero values take the defaults.
type Options struct {
	// ConfigDir holds init.lua and plugins/ (default paths.ConfigDir()).
	ConfigDir string
	// CommandTimeout bounds commands, loads and Eval (default 5 s).
	CommandTimeout time.Duration
	// CallbackTimeout bounds event handlers and keymap functions (default 50 ms).
	CallbackTimeout time.Duration
	// Caps are the UI capabilities the host provides (nk.ui.has): "toast",
	// "confirm", "cat", "palette", "scene", "notify".
	Caps []string
	// Disabled loads no file ([lua] enabled = false): Load does nothing,
	// Reload and Enable fail with ErrDisabled, AutoReload never starts.
	Disabled bool
}

// Message is one entry of the runtime's log ring (errors and nk.log).
type Message struct {
	At    time.Time `json:"at"`
	File  string    `json:"file,omitempty"`
	Level string    `json:"level"`
	Text  string    `json:"text"`
}

// ErrorEvent is the payload of lua.error.
type ErrorEvent struct {
	File  string `json:"file,omitempty"`
	Error string `json:"error"`
}

// Runtime is the Lua VM bound to one App.
type Runtime struct {
	app  *app.App
	opts Options
	caps map[string]bool

	calls     chan func()
	quit      chan struct{}
	stopped   chan struct{}
	closeOnce sync.Once
	cancel    context.CancelFunc // ends the event subscription
	bg        sync.WaitGroup     // event pump and fire-and-forget work

	// Owned by the VM goroutine.
	L       *glua.LState
	files   map[string]*fileState // live, by file name
	order   []string              // live file names, load order
	loading *fileState            // staging while a file loads
	cur     *invocation           // what is running now (nil when idle)

	mu       sync.Mutex // guards the fields below (read from any goroutine)
	msgs     []Message
	fails    map[string]int
	disabled map[string]bool
	keymap   core.Keymap
	wanted   map[string]bool // event names some handler follows
	fileList []string        // live files, load order (Files)
	closed   bool            // Close began: no new background work
	watching bool            // AutoReload started
}

// invocation is the Lua entry running now: its file (for attribution and
// for what nk.command/nk.on register under) and the caller's selection.
type invocation struct {
	file string
	ctx  app.Context
}

// New starts a runtime over a. It does not read any file: call Load.
func New(a *app.App, opts Options) (*Runtime, error) {
	if a == nil {
		return nil, errors.New("lua: app nil")
	}
	if opts.ConfigDir == "" {
		opts.ConfigDir = paths.ConfigDir()
	}
	if opts.CommandTimeout <= 0 {
		opts.CommandTimeout = DefaultCommandTimeout
	}
	if opts.CallbackTimeout <= 0 {
		opts.CallbackTimeout = DefaultCallbackTimeout
	}
	r := &Runtime{
		app: a, opts: opts, caps: map[string]bool{},
		calls: make(chan func(), 64), quit: make(chan struct{}), stopped: make(chan struct{}),
		files: map[string]*fileState{}, fails: map[string]int{},
		disabled: map[string]bool{}, wanted: map[string]bool{},
	}
	for _, c := range opts.Caps {
		r.caps[c] = true
	}
	r.L = newState()
	r.installAPI()
	ctx, cancel := context.WithCancel(context.Background())
	r.cancel = cancel
	events := a.Subscribe(ctx)
	go r.loop()
	r.bg.Add(1)
	go r.pump(ctx, events)
	return r, nil
}

// newState is an LState with the safe subset of the standard library: no
// io, no package/require, and os reduced to the clock and the environment
// (os.exit would take the TUI down; os.execute would escape the budget).
func newState() *glua.LState {
	L := glua.NewState(glua.Options{SkipOpenLibs: true})
	for _, lib := range []struct {
		name string
		fn   glua.LGFunction
	}{
		{glua.BaseLibName, glua.OpenBase}, {glua.TabLibName, glua.OpenTable},
		{glua.StringLibName, glua.OpenString}, {glua.MathLibName, glua.OpenMath},
		{glua.CoroutineLibName, glua.OpenCoroutine}, {glua.OsLibName, glua.OpenOs},
	} {
		L.Push(L.NewFunction(lib.fn))
		L.Push(glua.LString(lib.name))
		L.Call(1, 0)
	}
	if os, ok := L.GetGlobal("os").(*glua.LTable); ok {
		for _, unsafe := range []string{"exit", "execute", "remove", "rename", "setenv", "setlocale", "tmpname"} {
			os.RawSetString(unsafe, glua.LNil)
		}
	}
	for _, unsafe := range []string{"dofile", "loadfile"} {
		L.SetGlobal(unsafe, glua.LNil)
	}
	return L
}

// loop is the VM goroutine: the only one that touches r.L.
func (r *Runtime) loop() {
	defer close(r.stopped)
	for {
		select {
		case f := <-r.calls:
			f()
		case <-r.quit:
			return
		}
	}
}

// do runs f on the VM goroutine and waits for it.
func (r *Runtime) do(f func()) error {
	done := make(chan struct{})
	select {
	case r.calls <- func() { defer close(done); f() }:
	case <-r.quit:
		return ErrClosed
	}
	select {
	case <-done:
		return nil
	case <-r.stopped:
		// the loop may have quit with f still queued
		select {
		case <-done:
			return nil
		default:
			return ErrClosed
		}
	}
}

// post queues f on the VM goroutine without waiting.
func (r *Runtime) post(f func()) {
	select {
	case r.calls <- f:
	case <-r.quit:
	}
}

// Close stops the VM: Lua commands leave the registry, handlers stop and
// the LState is closed. Safe to call twice.
func (r *Runtime) Close() {
	r.closeOnce.Do(func() {
		r.mu.Lock()
		r.closed = true
		r.mu.Unlock()
		_ = r.do(func() {
			for _, name := range append([]string(nil), r.order...) {
				r.dropFile(name)
			}
		})
		r.cancel()
		close(r.quit)
		<-r.stopped
		r.bg.Wait()
		r.L.Close()
	})
}

// call runs fn with args under budget and returns its first result. A call
// made while another is running (nk.run of a Lua command, a nested
// callback) shares the outer call's budget.
func (r *Runtime) call(budget time.Duration, fn *glua.LFunction, args ...glua.LValue) (ret glua.LValue, err error) {
	L := r.L
	top := L.GetTop()
	var ctx context.Context
	if L.Context() == nil {
		var cancel context.CancelFunc
		ctx, cancel = context.WithTimeout(context.Background(), budget)
		defer cancel()
		L.SetContext(ctx)
		defer L.RemoveContext()
	} else {
		ctx = L.Context()
	}
	defer func() {
		if p := recover(); p != nil { // never let the VM take the process down
			L.SetTop(top)
			ret, err = glua.LNil, fmt.Errorf("lua: pânico: %v", p)
		}
	}()
	if err := L.CallByParam(glua.P{Fn: fn, NRet: 1, Protect: true}, args...); err != nil {
		L.SetTop(top)
		if ctx.Err() != nil {
			return glua.LNil, fmt.Errorf("%w (%v)", ErrBudget, budget)
		}
		return glua.LNil, luaError(err)
	}
	ret = L.Get(-1)
	L.SetTop(top)
	return ret, nil
}

// luaError is a Lua error as Go text: "file:line: message", no traceback.
func luaError(err error) error {
	var ae *glua.ApiError
	if errors.As(err, &ae) && ae.Object != nil {
		return errors.New("lua: " + ae.Object.String())
	}
	return fmt.Errorf("lua: %w", err)
}

// Eval runs code (":lua <code>" in a palette) under the command budget and
// returns its result as text. An expression is tried first, so "1 + 1"
// gives "2" without a return.
func (r *Runtime) Eval(code string) (string, error) {
	var (
		out string
		err error
	)
	if derr := r.do(func() {
		fn, lerr := r.L.Load(strings.NewReader("return "+code), evalFile)
		if lerr != nil {
			fn, lerr = r.L.Load(strings.NewReader(code), evalFile)
		}
		if lerr != nil {
			err = luaError(lerr)
			r.report(evalFile, err, false)
			return
		}
		var ret glua.LValue
		ret, err = r.invoke(evalFile, app.Context{}, r.opts.CommandTimeout, fn)
		if err != nil {
			r.report(evalFile, err, false)
			return
		}
		out = display(ret)
	}); derr != nil {
		return "", derr
	}
	return out, err
}

// invoke is call with r.cur set for the duration (attribution, nk.run's
// default selection).
func (r *Runtime) invoke(file string, ctx app.Context, budget time.Duration, fn *glua.LFunction, args ...glua.LValue) (glua.LValue, error) {
	prev := r.cur
	r.cur = &invocation{file: file, ctx: ctx}
	defer func() { r.cur = prev }()
	return r.call(budget, fn, args...)
}

// display renders a result for Eval.
func display(v glua.LValue) string {
	if v == nil || v == glua.LNil {
		return "nil"
	}
	if t, ok := v.(*glua.LTable); ok {
		return fmt.Sprint(fromLua(t))
	}
	return v.String()
}

// ---- errors, the message ring and per-file failures ----

// log appends to the ring (oldest dropped past MaxMessages).
func (r *Runtime) log(file, level, text string) {
	r.mu.Lock()
	r.msgs = append(r.msgs, Message{At: r.app.Clock().Now(), File: file, Level: level, Text: text})
	if n := len(r.msgs) - MaxMessages; n > 0 {
		r.msgs = append([]Message(nil), r.msgs[n:]...)
	}
	r.mu.Unlock()
}

// Messages is a copy of the ring, oldest first ("últimos erros lua").
func (r *Runtime) Messages() []Message {
	r.mu.Lock()
	defer r.mu.Unlock()
	return append([]Message(nil), r.msgs...)
}

// report logs err, shows it as a toast and publishes lua.error. A callback
// failure (counted) also counts towards disabling the file. VM goroutine.
func (r *Runtime) report(file string, err error, counted bool) {
	r.reportErr(file, err, counted, true)
}

// reportQuiet is report without lua.error (for a lua.error handler's own
// failure, which would otherwise feed itself).
func (r *Runtime) reportQuiet(file string, err error) { r.reportErr(file, err, true, false) }

func (r *Runtime) reportErr(file string, err error, counted, publish bool) {
	text := err.Error()
	r.log(file, "error", text)
	r.app.Toast(text, true)
	if publish {
		r.app.Publish(EvLuaError, ErrorEvent{File: file, Error: text})
	}
	if !counted || file == evalFile {
		return
	}
	r.mu.Lock()
	r.fails[file]++
	n := r.fails[file]
	r.mu.Unlock()
	if n >= MaxFailures {
		r.disable(file)
	}
}

// succeeded resets file's consecutive failures.
func (r *Runtime) succeeded(file string) {
	r.mu.Lock()
	delete(r.fails, file)
	r.mu.Unlock()
}

// disable drops file's commands, keys and handlers until Enable. VM goroutine.
func (r *Runtime) disable(file string) {
	r.dropFile(file)
	r.mu.Lock()
	r.disabled[file] = true
	delete(r.fails, file)
	r.mu.Unlock()
	msg := fmt.Sprintf("lua: %s desativado após %d falhas seguidas (lua.enable %s religa)", file, MaxFailures, file)
	r.log(file, "error", msg)
	r.app.Toast(msg, true)
}

// Disabled lists the files disabled after MaxFailures, sorted.
func (r *Runtime) Disabled() []string {
	r.mu.Lock()
	defer r.mu.Unlock()
	out := make([]string, 0, len(r.disabled))
	for f := range r.disabled {
		out = append(out, f)
	}
	sort.Strings(out)
	return out
}

// Enable turns a disabled file back on and reloads.
func (r *Runtime) Enable(file string) error {
	r.mu.Lock()
	_, was := r.disabled[file]
	delete(r.disabled, file)
	r.mu.Unlock()
	if !was {
		return fmt.Errorf("lua: %s não está desativado", file)
	}
	return r.Reload()
}

// HasCap reports whether the host provides UI capability c.
func (r *Runtime) HasCap(c string) bool { return r.caps[c] }

// source is the registry Source of file's commands.
func source(file string) string { return sourcePrefix + file }

// fileOfSource is the file a "lua:<file>" Source names.
func fileOfSource(s string) (string, bool) { return strings.CutPrefix(s, sourcePrefix) }
