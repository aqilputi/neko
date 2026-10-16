package lua

import (
	"bytes"
	"context"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"reflect"
	"sort"
	"strings"
	"time"

	glua "github.com/yuin/gopher-lua"

	"github.com/aqilputi/nekuroddo/internal/app"
	"github.com/aqilputi/nekuroddo/internal/core"
)

// Files: init.lua, then plugins/*.lua in alphabetical order (plan §5.1).
const (
	initFile   = "init.lua"
	pluginsDir = "plugins"
)

// luaCmd is a command a file registered.
type luaCmd struct {
	cmd core.Command
	fn  *glua.LFunction
}

// keyBinding is a nk.keymap.set/del: a command name, a function, or
// neither (the key is unbound).
type keyBinding struct {
	mode, key string
	command   string
	fn        *glua.LFunction
}

// handler is a nk.on.
type handler struct {
	event string
	fn    *glua.LFunction
}

// fileState is what one file registered. While the file loads it is staged
// (nothing reaches the registry); a load that ends without error commits it
// in place of the file's previous state.
type fileState struct {
	name     string
	cmds     []*luaCmd
	keys     []*keyBinding
	handlers []handler
}

func (fs *fileState) command(name string) (*luaCmd, int) {
	for i, c := range fs.cmds {
		if c.cmd.Name == name {
			return c, i
		}
	}
	return nil, -1
}

// setKey adds or replaces the binding for kb's mode and key.
func (fs *fileState) setKey(kb *keyBinding) {
	for i, k := range fs.keys {
		if k.mode == kb.mode && k.key == kb.key {
			fs.keys[i] = kb
			return
		}
	}
	fs.keys = append(fs.keys, kb)
}

// off drops fs's handlers of event (only fn's when fn is set).
func (fs *fileState) off(event string, fn *glua.LFunction) {
	kept := fs.handlers[:0]
	for _, h := range fs.handlers {
		if h.event != event || (fn != nil && h.fn != fn) {
			kept = append(kept, h)
		}
	}
	fs.handlers = kept
}

// Load reads init.lua and plugins/*.lua (none: nothing changes). It is
// Reload under another name: the first load is a reload from nothing.
func (r *Runtime) Load() error { return r.Reload() }

// Reload re-reads every file. Each file is staged and committed on its own:
// a file that fails keeps its last good commands, keys and handlers (or has
// none, on the first load), and the error names it with its line. A file
// gone from disk is dropped; a disabled one is skipped until Enable.
func (r *Runtime) Reload() error {
	var err error
	if derr := r.do(func() { err = r.reload() }); derr != nil {
		return derr
	}
	return err
}

func (r *Runtime) reload() error {
	names, err := r.discover()
	if err != nil {
		return err
	}
	var errs []error
	seen := map[string]bool{evalFile: true}
	for _, name := range names {
		seen[name] = true
		if r.isDisabled(name) {
			continue
		}
		if err := r.loadFile(name); err != nil {
			errs = append(errs, err)
			r.report(name, err, false)
		}
	}
	for _, name := range append([]string(nil), r.order...) {
		if !seen[name] {
			r.dropFile(name)
		}
	}
	// handlers and keys follow the files' order, the REPL's last
	rank := map[string]int{evalFile: len(names)}
	for i, n := range names {
		rank[n] = i
	}
	sort.SliceStable(r.order, func(i, j int) bool { return rank[r.order[i]] < rank[r.order[j]] })
	r.rebuild()
	return errors.Join(errs...)
}

// discover lists the files to load, as names relative to the config dir.
func (r *Runtime) discover() ([]string, error) {
	var names []string
	if st, err := os.Stat(filepath.Join(r.opts.ConfigDir, initFile)); err == nil && st.Mode().IsRegular() {
		names = append(names, initFile)
	}
	plugins, err := filepath.Glob(filepath.Join(r.opts.ConfigDir, pluginsDir, "*.lua"))
	if err != nil {
		return nil, err
	}
	sort.Strings(plugins)
	for _, p := range plugins {
		if st, err := os.Stat(p); err == nil && st.Mode().IsRegular() {
			names = append(names, pluginsDir+"/"+filepath.Base(p))
		}
	}
	return names, nil
}

func (r *Runtime) isDisabled(name string) bool {
	r.mu.Lock()
	defer r.mu.Unlock()
	return r.disabled[name]
}

// loadFile runs one file staged and commits it if it ran clean. VM goroutine.
func (r *Runtime) loadFile(name string) error {
	src, err := os.ReadFile(filepath.Join(r.opts.ConfigDir, filepath.FromSlash(name)))
	if err != nil {
		return fmt.Errorf("lua: %s: %w", name, err)
	}
	fn, err := r.L.Load(bytes.NewReader(src), name)
	if err != nil {
		return luaError(err)
	}
	staged := &fileState{name: name}
	r.loading = staged
	_, err = r.invoke(name, app.Context{}, r.opts.CommandTimeout, fn)
	r.loading = nil
	if errors.Is(err, ErrBudget) {
		return fmt.Errorf("lua: %s: %w", name, err)
	}
	if err != nil {
		return err
	}
	return r.commit(staged)
}

// commit makes staged the live state of its file: the old commands leave
// the registry and the new ones enter it.
func (r *Runtime) commit(staged *fileState) error {
	reg := r.app.Registry()
	old, existed := r.files[staged.name]
	if existed {
		for _, c := range old.cmds {
			reg.Unregister(c.cmd.Name)
		}
	}
	var errs []error
	kept := staged.cmds[:0]
	for _, c := range staged.cmds {
		if err := reg.Register(c.cmd, r.runner(c.cmd.Name)); err != nil {
			errs = append(errs, fmt.Errorf("lua: %s: %w", staged.name, err))
			continue
		}
		kept = append(kept, c)
	}
	staged.cmds = kept
	r.files[staged.name] = staged
	if !existed {
		r.order = append(r.order, staged.name)
	}
	r.rebuild()
	return errors.Join(errs...)
}

// dropFile removes file's registrations. VM goroutine.
func (r *Runtime) dropFile(name string) {
	fs, ok := r.files[name]
	if !ok {
		return
	}
	for _, c := range fs.cmds {
		r.app.Registry().Unregister(c.cmd.Name)
	}
	delete(r.files, name)
	for i, n := range r.order {
		if n == name {
			r.order = append(r.order[:i], r.order[i+1:]...)
			break
		}
	}
	r.rebuild()
}

// target is the file state nk.* registrations go to: the staged file while
// one loads, else the live state of the file whose code is running (created
// for the REPL). staged reports which.
func (r *Runtime) target() (fs *fileState, staged bool) {
	if r.loading != nil {
		return r.loading, true
	}
	name := evalFile
	if r.cur != nil {
		name = r.cur.file
	}
	fs, ok := r.files[name]
	if !ok {
		fs = &fileState{name: name}
		r.files[name] = fs
		r.order = append(r.order, name)
	}
	return fs, false
}

// rebuild recomputes what other goroutines read: the key overrides, the
// event names followed and the file list. A changed keymap is published.
func (r *Runtime) rebuild() {
	type slot struct{ mode, key string }
	var km core.Keymap
	at := map[slot]int{}
	wanted := map[string]bool{}
	var files []string
	for _, name := range r.order {
		fs := r.files[name]
		if name != evalFile {
			files = append(files, name)
		}
		for _, h := range fs.handlers {
			wanted[h.event] = true
		}
		for _, kb := range fs.keys {
			b := r.binding(kb)
			if i, ok := at[slot{kb.mode, kb.key}]; ok {
				km[i] = b // last wins, in the first one's place
				continue
			}
			at[slot{kb.mode, kb.key}] = len(km)
			km = append(km, b)
		}
	}
	r.mu.Lock()
	changed := !reflect.DeepEqual(km, r.keymap)
	r.keymap = km
	r.wanted = wanted
	r.fileList = files
	r.mu.Unlock()
	if changed {
		r.app.Publish(EvKeymapChanged, nil)
	}
}

// binding is how kb shows in the served keymap.
func (r *Runtime) binding(kb *keyBinding) core.Binding {
	b := core.Binding{Command: kb.command, Keys: []string{kb.key}, Mode: kb.mode}
	switch {
	case kb.fn != nil:
		b.Command = keyCommand(kb.mode, kb.key)
		b.Help = core.Help{Key: kb.key, Desc: "lua"}
	case kb.command != "":
		desc := kb.command
		if c, ok := r.app.Registry().Lookup(kb.command); ok && c.Title != "" {
			desc = c.Title
		}
		b.Help = core.Help{Key: kb.key, Desc: desc}
	}
	return b
}

// keyCommand names a key bound to a Lua function (RunBinding runs it).
func keyCommand(mode, key string) string { return keyCommandPrefix + mode + ":" + key }

// Keymap is the Lua key overrides, in file order (later files win a key in
// the earlier one's place). A binding with an empty Command unbinds the key
// (nk.keymap.del); one whose Command starts with "lua.key:" is a Lua
// function, run with RunBinding. Clients lay it over the keymap derived from
// the registry; keymap.changed is published whenever it changes.
func (r *Runtime) Keymap() core.Keymap {
	r.mu.Lock()
	defer r.mu.Unlock()
	return append(core.Keymap(nil), r.keymap...)
}

// Files lists the files loaded and live, in load order.
func (r *Runtime) Files() []string {
	r.mu.Lock()
	defer r.mu.Unlock()
	return append([]string(nil), r.fileList...)
}

// RunBinding runs what a key resolved to: a "lua.key:<mode>:<key>" function
// under the callback budget, or any other command through app.Run.
func (r *Runtime) RunBinding(ctx app.Context, command string) (app.Result, error) {
	rest, ok := strings.CutPrefix(command, keyCommandPrefix)
	if !ok {
		return r.app.Run(ctx, command, nil)
	}
	mode, key, _ := strings.Cut(rest, ":")
	var (
		res app.Result
		err error
	)
	if derr := r.do(func() { res, err = r.runKey(ctx, mode, key) }); derr != nil {
		return app.Result{}, derr
	}
	return res, err
}

func (r *Runtime) runKey(ctx app.Context, mode, key string) (app.Result, error) {
	var (
		file string
		fn   *glua.LFunction
	)
	for _, name := range r.order {
		for _, kb := range r.files[name].keys {
			if kb.mode == mode && kb.key == key {
				file, fn = name, kb.fn
			}
		}
	}
	if fn == nil {
		return app.Result{}, fmt.Errorf("%w: %s", app.ErrUnknownCommand, keyCommand(mode, key))
	}
	return r.finish(file, true, func() (glua.LValue, error) {
		return r.invoke(file, ctx, r.opts.CallbackTimeout, fn, ctxToLua(r.L, ctx))
	})
}

// finish runs a top-level entry and turns its outcome into a Result:
// errors are reported (and counted against file), success resets the count.
func (r *Runtime) finish(file string, top bool, run func() (glua.LValue, error)) (app.Result, error) {
	ret, err := run()
	if err != nil {
		if top {
			r.report(file, err, true)
		}
		return app.Result{Toast: err.Error(), Err: true}, err
	}
	if top {
		r.succeeded(file)
	}
	return toResult(ret), nil
}

// ---- events ----

// pump forwards the bus events some handler follows to the VM goroutine.
// A subscription cut for lagging is renewed.
func (r *Runtime) pump(ctx context.Context, events <-chan core.Event) {
	defer r.bg.Done()
	for {
		for ev := range events {
			if r.wants(ev.Name) {
				r.post(func() { r.deliver(ev) })
			}
		}
		select {
		case <-ctx.Done():
			return
		case <-time.After(20 * time.Millisecond): // the broker may be closed: do not spin
		}
		events = r.app.Subscribe(ctx)
	}
}

func (r *Runtime) wants(name string) bool {
	r.mu.Lock()
	defer r.mu.Unlock()
	return r.wanted[name]
}

// deliver calls ev's handlers, file by file, each under the callback budget.
func (r *Runtime) deliver(ev core.Event) {
	type call struct {
		file string
		fn   *glua.LFunction
	}
	var calls []call
	for _, name := range r.order {
		for _, h := range r.files[name].handlers {
			if h.event == ev.Name {
				calls = append(calls, call{name, h.fn})
			}
		}
	}
	data := jsonData(ev.Data)
	for _, c := range calls {
		if _, live := r.files[c.file]; !live { // disabled by an earlier failure
			continue
		}
		_, err := r.invoke(c.file, app.Context{}, r.opts.CallbackTimeout, c.fn, toLua(r.L, data), glua.LString(ev.Name))
		switch {
		case err != nil && ev.Name == EvLuaError:
			r.reportQuiet(c.file, err) // no lua.error about a lua.error handler
		case err != nil:
			r.report(c.file, err, true)
		default:
			r.succeeded(c.file)
		}
	}
}
