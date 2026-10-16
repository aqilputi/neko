package lua

// The nk.* API (plan §5.2). Every function here runs on the VM goroutine,
// inside a budgeted call, and reaches the world only through internal/app.

import (
	"encoding/json"
	"fmt"
	"math"
	"slices"
	"sort"
	"time"

	glua "github.com/yuin/gopher-lua"

	"github.com/aqilputi/nekuroddo/internal/app"
	"github.com/aqilputi/nekuroddo/internal/config"
	"github.com/aqilputi/nekuroddo/internal/core"
	"github.com/aqilputi/nekuroddo/internal/paths"
	"github.com/aqilputi/nekuroddo/internal/store"
)

// APIVersion is nk.version.
const APIVersion = 1

// permModes are the --permission-mode values nk.dispatch accepts ("" =
// Claude Code's own settings). Bypass still asks, unless it is the
// configured default (app.Dispatch).
var permModes = []string{"", "acceptEdits", "auto", "plan", app.PermBypass}

var argTypes = []string{core.ArgString, core.ArgSession, core.ArgDir, core.ArgTag, core.ArgInt}

// installAPI defines the global nk table.
func (r *Runtime) installAPI() {
	L := r.L
	nk := L.NewTable()
	L.SetFuncs(nk, map[string]glua.LGFunction{
		"command":    r.nkCommand,
		"run":        r.nkRun,
		"on":         r.nkOn,
		"off":        r.nkOff,
		"sessions":   r.nkSessions,
		"session":    r.nkSession,
		"inbox":      r.nkInbox,
		"dispatch":   r.nkDispatch,
		"stop":       r.nkStop,
		"remove":     r.nkRemove,
		"notify":     r.nkGroupNotify,
		"config_dir": func(L *glua.LState) int { L.Push(glua.LString(r.opts.ConfigDir)); return 1 },
		"state_dir":  func(L *glua.LState) int { L.Push(glua.LString(paths.StateDir())); return 1 },
		"log":        r.nkLog,
	})
	nk.RawSetString("version", glua.LNumber(APIVersion))
	nk.RawSetString("keymap", L.SetFuncs(L.NewTable(), map[string]glua.LGFunction{
		"set": r.nkKeymapSet,
		"del": r.nkKeymapDel,
	}))
	nk.RawSetString("group", L.SetFuncs(L.NewTable(), map[string]glua.LGFunction{
		"add":      r.nkGroupAdd,
		"remove":   r.nkGroupRemove,
		"notify":   r.nkGroupNotify,
		"dissolve": r.nkGroupDissolve,
		"tags":     r.nkGroupTags,
		"of":       r.nkGroupOf,
	}))
	ui := L.SetFuncs(L.NewTable(), map[string]glua.LGFunction{
		"has":          func(L *glua.LState) int { L.Push(glua.LBool(r.caps[L.CheckString(1)])); return 1 },
		"toast":        r.nkToast,
		"confirm":      r.nkConfirm,
		"palette_open": r.nkPaletteOpen,
		"notify":       r.nkUINotify,
	})
	ui.RawSetString("cat", L.SetFuncs(L.NewTable(), map[string]glua.LGFunction{"meow": r.nkMeow}))
	nk.RawSetString("ui", ui)
	L.SetGlobal("nk", nk)
}

// ---- commands ----

// nk.command(name, fn, { doc=, hint=, args=, keys=, mode=, confirm= })
func (r *Runtime) nkCommand(L *glua.LState) int {
	name := L.CheckString(1)
	fn := L.CheckFunction(2)
	opts := L.OptTable(3, L.NewTable())
	if name == "" {
		L.ArgError(1, "nome vazio")
	}
	c := core.Command{
		Name: name, Title: optString(L, opts, "doc", name), Hint: optString(L, opts, "hint", ""),
		Mode: optString(L, opts, "mode", core.ModeNormal), Scope: core.ScopeCore,
		Confirm: glua.LVAsBool(opts.RawGetString("confirm")),
	}
	c.Keys = optStrings(L, opts, "keys")
	c.Args = commandArgs(L, opts.RawGetString("args"))
	fs, staged := r.target()
	c.Source = source(fs.name)
	if prev, ok := r.app.Registry().Lookup(name); ok && prev.Source != c.Source {
		L.RaiseError("comando %s já registrado (%s)", name, prev.Source)
	}
	lc := &luaCmd{cmd: c, fn: fn}
	if _, i := fs.command(name); i >= 0 {
		fs.cmds[i] = lc
	} else {
		fs.cmds = append(fs.cmds, lc)
	}
	if staged {
		return 0
	}
	reg := r.app.Registry()
	reg.Unregister(name)
	if err := reg.Register(c, r.runner(name)); err != nil {
		L.RaiseError("%s", err.Error())
	}
	r.rebuild()
	return 0
}

func commandArgs(L *glua.LState, v glua.LValue) []core.Arg {
	if v == glua.LNil {
		return nil
	}
	t, ok := v.(*glua.LTable)
	if !ok {
		L.RaiseError("args: esperava uma lista")
	}
	var out []core.Arg
	for i := 1; i <= t.Len(); i++ {
		switch a := t.RawGetInt(i).(type) {
		case glua.LString:
			out = append(out, core.Arg{Name: string(a), Type: core.ArgString, Prompt: string(a) + " › "})
		case *glua.LTable:
			arg := core.Arg{
				Name: optString(L, a, "name", ""), Type: optString(L, a, "type", core.ArgString),
				Required: glua.LVAsBool(a.RawGetString("required")),
			}
			if arg.Name == "" {
				L.RaiseError("args[%d]: falta name", i)
			}
			if !slices.Contains(argTypes, arg.Type) {
				L.RaiseError("args[%d]: tipo %q desconhecido", i, arg.Type)
			}
			arg.Prompt = optString(L, a, "prompt", arg.Name+" › ")
			out = append(out, arg)
		default:
			L.RaiseError("args[%d]: esperava nome ou tabela", i)
		}
	}
	return out
}

// runner is the registry's entry into a Lua command: it hops onto the VM.
func (r *Runtime) runner(name string) app.Runner {
	return func(ctx app.Context, args app.Args) (app.Result, error) {
		var (
			res app.Result
			err error
		)
		if derr := r.do(func() { res, err = r.runCommand(ctx, name, args) }); derr != nil {
			return app.Result{}, derr
		}
		return res, err
	}
}

// findCommand is a Lua command by name: staged (the file loading now) first.
func (r *Runtime) findCommand(name string) (string, *luaCmd) {
	if r.loading != nil {
		if c, _ := r.loading.command(name); c != nil {
			return r.loading.name, c
		}
	}
	for _, f := range r.order {
		if c, _ := r.files[f].command(name); c != nil {
			return f, c
		}
	}
	return "", nil
}

// runCommand runs a Lua command as fn(args, ctx). VM goroutine. A run from
// inside another Lua call (nk.run) shares its budget and leaves error
// reporting to the outer call.
func (r *Runtime) runCommand(ctx app.Context, name string, args app.Args) (app.Result, error) {
	file, lc := r.findCommand(name)
	if lc == nil {
		return app.Result{}, fmt.Errorf("%w: %s", app.ErrUnknownCommand, name)
	}
	return r.finish(file, r.cur == nil, func() (glua.LValue, error) {
		return r.invoke(file, ctx, r.opts.CommandTimeout, lc.fn, argsToLua(r.L, args), ctxToLua(r.L, ctx))
	})
}

// nk.run(name, args, ctx) -> result table. A Lua command runs inline;
// anything else goes through app.Run. ctx defaults to the caller's.
func (r *Runtime) nkRun(L *glua.LState) int {
	name := L.CheckString(1)
	args := argsFromLua(L, L.OptTable(2, L.NewTable()))
	ctx := app.Context{}
	if r.cur != nil {
		ctx = r.cur.ctx
	}
	if t, ok := L.Get(3).(*glua.LTable); ok {
		ctx = ctxFromLua(t)
	}
	var (
		res app.Result
		err error
	)
	if _, lc := r.findCommand(name); lc != nil {
		for _, a := range lc.cmd.Args {
			if a.Required && args[a.Name] == "" {
				L.RaiseError("%v: %s (%s)", app.ErrMissingArg, a.Name, name)
			}
		}
		res, err = r.runCommand(ctx, name, args)
		ran := app.CommandRan{Name: name}
		if err != nil {
			ran.Err = err.Error()
		}
		r.app.Publish(app.EvCommandRan, ran)
	} else {
		if c, ok := r.app.Registry().Lookup(name); ok {
			if _, isLua := fileOfSource(c.Source); isLua { // would come back to this goroutine
				L.RaiseError("comando lua %s indisponível", name)
			}
		}
		res, err = r.app.Run(ctx, name, args)
	}
	if err != nil {
		L.RaiseError("%s", err.Error())
	}
	L.Push(resultToLua(L, res))
	return 1
}

// ---- keymap ----

// nk.keymap.set(mode, key, cmd): cmd is a command name or a function.
func (r *Runtime) nkKeymapSet(L *glua.LState) int {
	mode, key := L.CheckString(1), L.CheckString(2)
	if mode == "" || key == "" {
		L.RaiseError("keymap.set: modo e tecla não podem ser vazios")
	}
	kb := &keyBinding{mode: mode, key: key}
	switch v := L.Get(3).(type) {
	case glua.LString:
		kb.command = string(v)
	case *glua.LFunction:
		kb.fn = v
	default:
		L.ArgError(3, "esperava nome de comando ou função")
	}
	r.setKey(kb)
	return 0
}

// nk.keymap.del(mode, key): the key does nothing (default binding included).
func (r *Runtime) nkKeymapDel(L *glua.LState) int {
	mode, key := L.CheckString(1), L.CheckString(2)
	if mode == "" || key == "" {
		L.RaiseError("keymap.del: modo e tecla não podem ser vazios")
	}
	r.setKey(&keyBinding{mode: mode, key: key})
	return 0
}

func (r *Runtime) setKey(kb *keyBinding) {
	fs, staged := r.target()
	fs.setKey(kb)
	if !staged {
		r.rebuild()
	}
}

// ---- events ----

// nk.on(event, fn(data, name))
func (r *Runtime) nkOn(L *glua.LState) int {
	event := L.CheckString(1)
	fn := L.CheckFunction(2)
	if event == "" {
		L.ArgError(1, "evento vazio")
	}
	fs, staged := r.target()
	fs.handlers = append(fs.handlers, handler{event: event, fn: fn})
	if !staged {
		r.rebuild()
	}
	return 0
}

// nk.off(event[, fn]): without fn, every handler of event goes.
func (r *Runtime) nkOff(L *glua.LState) int {
	event := L.CheckString(1)
	var fn *glua.LFunction
	if f, ok := L.Get(2).(*glua.LFunction); ok {
		fn = f
	}
	if r.loading != nil {
		r.loading.off(event, fn)
	}
	for _, name := range r.order {
		r.files[name].off(event, fn)
	}
	r.rebuild()
	return 0
}

// ---- sessions ----

func attentionName(a store.Attention) string {
	switch a {
	case store.AttnBlocked:
		return "blocked"
	case store.AttnReturned:
		return "returned"
	case store.AttnIdle:
		return "idle"
	}
	return ""
}

func unix(t time.Time) glua.LNumber {
	if t.IsZero() {
		return 0
	}
	return glua.LNumber(t.Unix())
}

// sessionToLua is a copy of s as data.
func (r *Runtime) sessionToLua(L *glua.LState, s store.Session) *glua.LTable {
	t := L.NewTable()
	for k, v := range map[string]glua.LValue{
		"id": glua.LString(s.SessionID), "short_id": glua.LString(s.ShortID), "name": glua.LString(s.Name),
		"label": glua.LString(s.Label()), "title": glua.LString(s.Title), "cwd": glua.LString(s.Cwd),
		"context": glua.LString(s.Context), "kind": glua.LString(s.Kind), "state": glua.LString(s.State),
		"model": glua.LString(s.Model), "cost": glua.LNumber(s.Cost),
		"attention": glua.LString(attentionName(s.Attention)), "stale": glua.LBool(s.Stale),
		"archived": glua.LBool(s.Archived), "ended": glua.LBool(s.Ended),
		"started": unix(s.Started), "last_at": unix(s.LastAt),
		"waiting_for": glua.LString(s.WaitingFor), "needs": glua.LString(s.Needs),
	} {
		t.RawSetString(k, v)
	}
	t.RawSetString("tags", stringsToLua(L, r.app.TagsOf(s.SessionID)))
	return t
}

func (r *Runtime) nkSessions(L *glua.LState) int {
	t := L.NewTable()
	for _, s := range r.app.Snapshot().Sessions {
		t.Append(r.sessionToLua(L, s))
	}
	L.Push(t)
	return 1
}

// findSession is a session by session id or short (background) id.
func findSession(ss []store.Session, id string) (store.Session, bool) {
	for _, s := range ss {
		if id != "" && (s.SessionID == id || s.ShortID == id) {
			return s, true
		}
	}
	return store.Session{}, false
}

func (r *Runtime) nkSession(L *glua.LState) int {
	s, ok := findSession(r.app.Snapshot().Sessions, L.CheckString(1))
	if !ok {
		L.Push(glua.LNil)
		return 1
	}
	L.Push(r.sessionToLua(L, s))
	return 1
}

// nk.inbox() -> session ids waiting on you, most urgent first.
func (r *Runtime) nkInbox(L *glua.LState) int {
	ss := r.app.Snapshot().Sessions
	var xs []*store.Session
	for i := range ss {
		if ss[i].Attention != store.AttnNone {
			xs = append(xs, &ss[i])
		}
	}
	core.SortInbox(xs, nil)
	t := L.NewTable()
	for _, s := range xs {
		t.Append(glua.LString(s.SessionID))
	}
	L.Push(t)
	return 1
}

// resolveIDs maps short ids to session ids. An id the snapshot does not know
// (an agent dispatched a moment ago) triggers one refresh.
func (r *Runtime) resolveIDs(ids []string) []string {
	out := make([]string, len(ids))
	ss := r.app.Snapshot().Sessions
	refreshed := false
	for i, id := range ids {
		s, ok := findSession(ss, id)
		if !ok && !refreshed {
			ss, refreshed = r.app.Refresh().Sessions, true
			s, ok = findSession(ss, id)
		}
		out[i] = id
		if ok {
			out[i] = s.SessionID
		}
	}
	return out
}

func (r *Runtime) resolveID(id string) string { return r.resolveIDs([]string{id})[0] }

// ---- dispatch, stop, remove ----

// nk.dispatch{ dir=, prompt=, name=, perm= } -> short id, or a pending
// confirmation table { token=, question=, expires_at= } (bypass).
func (r *Runtime) nkDispatch(L *glua.LState) int {
	t := L.CheckTable(1)
	req := app.DispatchReq{
		Dir: config.Expand(optString(L, t, "dir", "")), Prompt: optString(L, t, "prompt", ""),
		Name: optString(L, t, "name", ""), PermMode: optString(L, t, "perm", ""),
	}
	switch {
	case req.Dir == "":
		L.RaiseError("dispatch: falta dir")
	case req.Prompt == "":
		L.RaiseError("dispatch: falta prompt")
	case !slices.Contains(permModes, req.PermMode):
		L.RaiseError("dispatch: perm %q inválido (use %v)", req.PermMode, permModes[1:])
	case len(optStrings(L, t, "extra")) > 0:
		L.RaiseError("dispatch: extra ainda não é suportado")
	}
	res, err := r.app.Dispatch(req)
	raiseIfFailed(L, res, err)
	if res.Pending != nil {
		L.Push(pendingToLua(L, res.Pending))
		return 1
	}
	d, _ := res.Data.(app.Dispatched)
	L.Push(glua.LString(d.ID))
	return 1
}

// nk.stop(id) / nk.remove(id) -> pending confirmation table.
func (r *Runtime) nkStop(L *glua.LState) int {
	return r.sessionAction(L, r.app.Stop)
}

func (r *Runtime) nkRemove(L *glua.LState) int {
	return r.sessionAction(L, r.app.Remove)
}

func (r *Runtime) sessionAction(L *glua.LState, act func(app.Context) (app.Result, error)) int {
	res, err := act(app.Context{SessionID: r.resolveID(L.CheckString(1))})
	raiseIfFailed(L, res, err)
	if res.Pending != nil {
		L.Push(pendingToLua(L, res.Pending))
		return 1
	}
	L.Push(resultToLua(L, res))
	return 1
}

// ---- groups ----

func idsArg(L *glua.LState, n int) []string {
	switch v := L.Get(n).(type) {
	case glua.LString:
		return []string{string(v)}
	case *glua.LTable:
		return tableStrings(L, v)
	}
	L.ArgError(n, "esperava id ou lista de ids")
	return nil
}

func (r *Runtime) nkGroupAdd(L *glua.LState) int {
	tag, ids := L.CheckString(1), idsArg(L, 2)
	res, err := r.app.GroupTargets(app.Context{Picked: r.resolveIDs(ids)}, tag)
	return pushToast(L, res, err)
}

func (r *Runtime) nkGroupRemove(L *glua.LState) int {
	tag, ids := L.CheckString(1), idsArg(L, 2)
	res, err := r.app.UngroupTargets(app.Context{Picked: r.resolveIDs(ids)}, tag)
	return pushToast(L, res, err)
}

// nk.notify(tag, text) / nk.group.notify(tag, text)
func (r *Runtime) nkGroupNotify(L *glua.LState) int {
	res, err := r.app.NotifyGroup(L.CheckString(1), L.CheckString(2))
	return pushToast(L, res, err)
}

// nk.group.dissolve(tag): no confirmation, the script is the user's.
func (r *Runtime) nkGroupDissolve(L *glua.LState) int {
	res, err := r.app.DissolveGroup(L.CheckString(1))
	return pushToast(L, res, err)
}

func (r *Runtime) nkGroupTags(L *glua.LState) int {
	gs := r.app.Groups().Groups
	tags := make([]string, 0, len(gs))
	for tag := range gs {
		tags = append(tags, tag)
	}
	sort.Strings(tags)
	L.Push(stringsToLua(L, tags))
	return 1
}

func (r *Runtime) nkGroupOf(L *glua.LState) int {
	L.Push(stringsToLua(L, r.app.TagsOf(r.resolveID(L.CheckString(1)))))
	return 1
}

// ---- ui ----

func (r *Runtime) nkToast(L *glua.LState) int {
	r.app.Toast(L.CheckString(1), L.OptBool(2, false))
	L.Push(glua.LTrue)
	return 1
}

// nk.ui.confirm(question, fn) -> token. Any client may answer; yes runs fn
// on the VM under the command budget.
func (r *Runtime) nkConfirm(L *glua.LState) int {
	q, fn := L.CheckString(1), L.CheckFunction(2)
	file := evalFile
	if r.cur != nil {
		file = r.cur.file
	}
	res := r.app.Ask(q, func() (app.Result, error) {
		var (
			res app.Result
			err error
		)
		if derr := r.do(func() {
			res, err = r.finish(file, true, func() (glua.LValue, error) {
				return r.invoke(file, app.Context{}, r.opts.CommandTimeout, fn)
			})
		}); derr != nil {
			return app.Result{}, derr
		}
		return res, err
	})
	L.Push(glua.LString(res.Pending.Token))
	return 1
}

// publishIf puts a UI event on the bus when the host has cap; it returns
// whether it did.
func (r *Runtime) publishIf(L *glua.LState, c, event string, data any) int {
	if !r.caps[c] {
		L.Push(glua.LFalse)
		return 1
	}
	r.app.Publish(event, data)
	L.Push(glua.LTrue)
	return 1
}

func (r *Runtime) nkPaletteOpen(L *glua.LState) int {
	return r.publishIf(L, "palette", EvUIPaletteOpen, map[string]string{"prefill": L.OptString(1, "")})
}

func (r *Runtime) nkMeow(L *glua.LState) int {
	return r.publishIf(L, "cat", EvUICatMeow, map[string]string{"text": L.OptString(1, "")})
}

// nk.ui.notify(body[, title]): a desktop notification, off the VM (it is a
// process). Needs the "notify" capability.
func (r *Runtime) nkUINotify(L *glua.LState) int {
	body, title := L.CheckString(1), L.OptString(2, "nekuroddo")
	n := r.app.Ports().Notify
	if !r.caps["notify"] || n == nil {
		L.Push(glua.LFalse)
		return 1
	}
	r.bg.Add(1)
	go func() {
		defer r.bg.Done()
		_ = n.Notify(title, body)
	}()
	L.Push(glua.LTrue)
	return 1
}

// nk.log([level,] msg)
func (r *Runtime) nkLog(L *glua.LState) int {
	level, msg := "info", L.CheckString(1)
	if L.GetTop() >= 2 {
		level, msg = msg, L.CheckString(2)
	}
	file := evalFile
	if r.cur != nil {
		file = r.cur.file
	}
	r.log(file, level, msg)
	return 0
}

// ---- results and errors ----

// raiseIfFailed turns a failed app call into a Lua error (the Result's toast
// says it best).
func raiseIfFailed(L *glua.LState, res app.Result, err error) {
	switch {
	case res.Err && res.Toast != "":
		L.RaiseError("%s", res.Toast)
	case err != nil:
		L.RaiseError("%s", err.Error())
	}
}

func pushToast(L *glua.LState, res app.Result, err error) int {
	raiseIfFailed(L, res, err)
	L.Push(glua.LString(res.Toast))
	return 1
}

func pendingToLua(L *glua.LState, p *core.PendingConfirmation) *glua.LTable {
	t := L.NewTable()
	t.RawSetString("token", glua.LString(p.Token))
	t.RawSetString("question", glua.LString(p.Question))
	t.RawSetString("expires_at", unix(p.ExpiresAt))
	return t
}

// resultToLua is an app.Result as { toast=, err=, pending=, exec=, data= }.
func resultToLua(L *glua.LState, res app.Result) *glua.LTable {
	t := L.NewTable()
	t.RawSetString("toast", glua.LString(res.Toast))
	t.RawSetString("err", glua.LBool(res.Err))
	if res.Pending != nil {
		t.RawSetString("pending", pendingToLua(L, res.Pending))
	}
	if res.Exec != nil {
		e := L.NewTable()
		e.RawSetString("argv", stringsToLua(L, res.Exec.Argv))
		e.RawSetString("dir", glua.LString(res.Exec.Dir))
		t.RawSetString("exec", e)
	}
	if res.Data != nil {
		t.RawSetString("data", toLua(L, jsonData(res.Data)))
	}
	return t
}

// toResult reads what a Lua command returned: nothing, a toast text, or a
// table { toast=, err=, data=, pending= } (a pending table itself, as
// nk.dispatch returns it, also works).
func toResult(v glua.LValue) app.Result {
	switch v := v.(type) {
	case *glua.LTable:
		if p := pendingFromLua(v); p != nil {
			return app.Result{Pending: p}
		}
		res := app.Result{Err: glua.LVAsBool(v.RawGetString("err"))}
		if s, ok := v.RawGetString("toast").(glua.LString); ok {
			res.Toast = string(s)
		}
		if p, ok := v.RawGetString("pending").(*glua.LTable); ok {
			res.Pending = pendingFromLua(p)
		}
		if d := v.RawGetString("data"); d != glua.LNil {
			res.Data = fromLua(d)
		}
		return res
	case glua.LString:
		return app.Result{Toast: string(v)}
	case nil:
		return app.Result{}
	}
	if v == glua.LNil {
		return app.Result{}
	}
	return app.Result{Toast: v.String()}
}

func pendingFromLua(t *glua.LTable) *core.PendingConfirmation {
	tok, ok := t.RawGetString("token").(glua.LString)
	if !ok || tok == "" {
		return nil
	}
	p := &core.PendingConfirmation{Token: string(tok)}
	if q, ok := t.RawGetString("question").(glua.LString); ok {
		p.Question = string(q)
	}
	if e, ok := t.RawGetString("expires_at").(glua.LNumber); ok && e > 0 {
		p.ExpiresAt = time.Unix(int64(e), 0)
	}
	return p
}

// ---- conversions ----

func optString(L *glua.LState, t *glua.LTable, key, def string) string {
	switch v := t.RawGetString(key).(type) {
	case glua.LString:
		return string(v)
	case glua.LNumber:
		return v.String()
	case *glua.LNilType:
		return def
	}
	L.RaiseError("%s: esperava texto", key)
	return def
}

// optStrings reads t[key] as a list of strings (nil when absent).
func optStrings(L *glua.LState, t *glua.LTable, key string) []string {
	switch v := t.RawGetString(key).(type) {
	case *glua.LNilType:
		return nil
	case *glua.LTable:
		return tableStrings(L, v)
	}
	L.RaiseError("%s: esperava uma lista", key)
	return nil
}

func tableStrings(L *glua.LState, t *glua.LTable) []string {
	var out []string
	for i := 1; i <= t.Len(); i++ {
		s, ok := t.RawGetInt(i).(glua.LString)
		if !ok {
			L.RaiseError("item %d: esperava texto", i)
		}
		out = append(out, string(s))
	}
	return out
}

func stringsToLua(L *glua.LState, ss []string) *glua.LTable {
	t := L.CreateTable(len(ss), 0)
	for _, s := range ss {
		t.Append(glua.LString(s))
	}
	return t
}

func argsToLua(L *glua.LState, args app.Args) *glua.LTable {
	t := L.NewTable()
	for k, v := range args {
		t.RawSetString(k, glua.LString(v))
	}
	return t
}

// argsFromLua reads named arguments; scalars become text.
func argsFromLua(L *glua.LState, t *glua.LTable) app.Args {
	args := app.Args{}
	t.ForEach(func(k, v glua.LValue) {
		switch v.(type) {
		case glua.LString, glua.LNumber, glua.LBool:
			args[k.String()] = v.String()
		default:
			L.RaiseError("argumento %s: esperava texto", k.String())
		}
	})
	return args
}

func ctxToLua(L *glua.LState, c app.Context) *glua.LTable {
	t := L.NewTable()
	t.RawSetString("session_id", glua.LString(c.SessionID))
	t.RawSetString("picked", stringsToLua(L, c.Picked))
	t.RawSetString("tab", glua.LString(c.Tab))
	t.RawSetString("todo_id", glua.LNumber(c.TodoID))
	t.RawSetString("issue_key", glua.LString(c.IssueKey))
	return t
}

func ctxFromLua(t *glua.LTable) app.Context {
	c := app.Context{}
	if s, ok := t.RawGetString("session_id").(glua.LString); ok {
		c.SessionID = string(s)
	}
	if p, ok := t.RawGetString("picked").(*glua.LTable); ok {
		for i := 1; i <= p.Len(); i++ {
			c.Picked = append(c.Picked, p.RawGetInt(i).String())
		}
	}
	if s, ok := t.RawGetString("tab").(glua.LString); ok {
		c.Tab = string(s)
	}
	if n, ok := t.RawGetString("todo_id").(glua.LNumber); ok {
		c.TodoID = int(n)
	}
	if s, ok := t.RawGetString("issue_key").(glua.LString); ok {
		c.IssueKey = string(s)
	}
	return c
}

// jsonData turns any payload into plain data (maps, slices, scalars) the
// way JSON clients see it; what JSON cannot carry becomes nil.
func jsonData(v any) any {
	switch v.(type) {
	case nil, string, bool, float64, int:
		return v
	}
	b, err := json.Marshal(v)
	if err != nil {
		return nil
	}
	var out any
	if err := json.Unmarshal(b, &out); err != nil {
		return nil
	}
	return out
}

// toLua converts plain data (as jsonData returns it) to Lua.
func toLua(L *glua.LState, v any) glua.LValue {
	switch v := v.(type) {
	case nil:
		return glua.LNil
	case bool:
		return glua.LBool(v)
	case float64:
		return glua.LNumber(v)
	case int:
		return glua.LNumber(v)
	case string:
		return glua.LString(v)
	case []any:
		t := L.CreateTable(len(v), 0)
		for _, x := range v {
			t.Append(toLua(L, x))
		}
		return t
	case map[string]any:
		t := L.CreateTable(0, len(v))
		for k, x := range v {
			t.RawSetString(k, toLua(L, x))
		}
		return t
	}
	return toLua(L, jsonData(v))
}

// maxDepth bounds fromLua on self-referencing tables.
const maxDepth = 32

// fromLua converts a Lua value to plain Go data: a table with only 1..n keys
// is a list, any other a map; functions and the like become nil.
func fromLua(v glua.LValue) any { return fromLuaDepth(v, 0) }

func fromLuaDepth(v glua.LValue, depth int) any {
	switch v := v.(type) {
	case glua.LBool:
		return bool(v)
	case glua.LNumber:
		if f := float64(v); f == math.Trunc(f) && math.Abs(f) < 1<<53 {
			return int64(f)
		}
		return float64(v)
	case glua.LString:
		return string(v)
	case *glua.LTable:
		if depth >= maxDepth {
			return nil
		}
		n := v.Len()
		count := 0
		v.ForEach(func(glua.LValue, glua.LValue) { count++ })
		if n > 0 && count == n {
			list := make([]any, 0, n)
			for i := 1; i <= n; i++ {
				list = append(list, fromLuaDepth(v.RawGetInt(i), depth+1))
			}
			return list
		}
		m := map[string]any{}
		v.ForEach(func(k, x glua.LValue) { m[k.String()] = fromLuaDepth(x, depth+1) })
		return m
	}
	return nil
}
