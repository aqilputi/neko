package rpc

import (
	"encoding/json"
	"errors"
	"net/http"
	"slices"
	"testing"

	"github.com/aqilputi/nekuroddo/internal/app"
	"github.com/aqilputi/nekuroddo/internal/core"
	"github.com/aqilputi/nekuroddo/internal/events"
	"github.com/aqilputi/nekuroddo/internal/jira"
	"github.com/aqilputi/nekuroddo/internal/rpc/proto"
	"github.com/aqilputi/nekuroddo/internal/todo"
)

func names(cs []proto.CommandInfo) []string {
	var out []string
	for _, c := range cs {
		out = append(out, c.Name)
	}
	return out
}

func find(cs []proto.CommandInfo, name string) (proto.CommandInfo, bool) {
	for _, c := range cs {
		if c.Name == name {
			return c, true
		}
	}
	return proto.CommandInfo{}, false
}

func TestCommandsListTheRegistry(t *testing.T) {
	e := newEnv(t)
	cs, err := e.c.Commands(ctxT(t), proto.Context{})
	if err != nil {
		t.Fatal(err)
	}
	// every builtin is listed, even those not available without a selection
	for _, want := range []string{"session.archive", "session.stop", "session.open", "session.new",
		"pane.close", "group.add", "todo.start", "jira.fetch"} {
		if _, ok := find(cs, want); !ok {
			t.Errorf("%s missing from %v", want, names(cs))
		}
	}
	stop, _ := find(cs, "session.stop")
	if stop.Available || !stop.Confirm || stop.Scope != core.ScopeCore || stop.Source != core.SourceBuiltin {
		t.Fatalf("session.stop without a session = %+v", stop)
	}
	if c, _ := find(cs, "pane.close"); c.Scope != core.ScopeClient || !c.Available {
		t.Fatalf("pane.close = %+v", c)
	}
	// in registration order: session.archive is the very first builtin
	if cs[0].Name != "session.archive" {
		t.Fatalf("first = %s", cs[0].Name)
	}

	cs, err = e.c.Commands(ctxT(t), proto.Context{SessionID: bgSession.SessionID, TodoID: 3, Picked: []string{"x"}, Tab: "inbox", IssueKey: "K-1"})
	if err != nil {
		t.Fatal(err)
	}
	for _, n := range []string{"session.stop", "session.open", "todo.start", "jira.open_url"} {
		if c, _ := find(cs, n); !c.Available {
			t.Errorf("%s not available with a selection", n)
		}
	}
}

func TestCommandsListLateRegistrations(t *testing.T) {
	e := newEnv(t)
	err := e.a.Registry().Register(core.Command{Name: "lua.hi", Title: "oi", Source: "lua:init.lua"},
		func(app.Context, app.Args) (app.Result, error) { return app.Info("oi"), nil },
		app.WithAvailable(func(c app.Context) bool { return c.Tab == "todos" }))
	if err != nil {
		t.Fatal(err)
	}
	cs, err := e.c.Commands(ctxT(t), proto.Context{Tab: "todos"})
	if err != nil {
		t.Fatal(err)
	}
	c, ok := find(cs, "lua.hi")
	if !ok || !c.Available || c.Source != "lua:init.lua" {
		t.Fatalf("lua.hi = %+v %v", c, ok)
	}
	// listed elsewhere too, as unavailable
	cs, _ = e.c.Commands(ctxT(t), proto.Context{})
	if c, ok := find(cs, "lua.hi"); !ok || c.Available {
		t.Fatalf("lua.hi off the todos tab = %+v %v", c, ok)
	}
}

func TestRunCommand(t *testing.T) {
	e := newEnv(t)
	ev := e.a.Subscribe(t.Context())
	res, err := e.c.Run(ctxT(t), "session.rename", map[string]string{"name": "novo"},
		proto.Context{SessionID: idleSession.SessionID})
	if err != nil {
		t.Fatal(err)
	}
	if res.Err || res.Pending != nil {
		t.Fatalf("result = %+v", res)
	}
	if c := e.f.Sessions.CallsTo("Rename"); len(c) != 1 || c[0].Args[1] != "novo" {
		t.Fatalf("Rename calls = %v", c)
	}
	// command.ran goes on the bus
	for got := range ev {
		if got.Name == app.EvCommandRan {
			if got.Data.(app.CommandRan).Name != "session.rename" {
				t.Fatalf("command.ran = %+v", got.Data)
			}
			break
		}
	}
}

func TestRunReturnsDataAndExec(t *testing.T) {
	e := newEnv(t)
	res, err := e.c.Run(ctxT(t), "session.fullscreen", nil, proto.Context{SessionID: bgSession.SessionID})
	if err != nil {
		t.Fatal(err)
	}
	if res.Exec == nil || !slices.Equal(res.Exec.Argv, []string{"claude", "attach", "bg01"}) || res.Exec.Dir != "/r/a" {
		t.Fatalf("exec = %+v", res.Exec)
	}
	res, err = e.c.Run(ctxT(t), "session.preview", nil, proto.Context{SessionID: bgSession.SessionID})
	if err != nil {
		t.Fatal(err)
	}
	var plan app.AttachPlan
	if err := json.Unmarshal(res.Data, &plan); err != nil || plan.ID != "bg01" {
		t.Fatalf("data = %s (%v)", res.Data, err)
	}
}

func TestRunPendingThenConfirm(t *testing.T) {
	e := newEnv(t)
	res, err := e.c.Run(ctxT(t), "session.stop", nil, proto.Context{SessionID: bgSession.SessionID})
	if err != nil {
		t.Fatal(err)
	}
	if res.Pending == nil || res.Pending.Token == "" || res.Pending.Question == "" {
		t.Fatalf("no pending: %+v", res)
	}
	if e.f.Claude.Count("Stop") != 0 {
		t.Fatal("stopped before confirming")
	}
	// the snapshot shows the pending confirmation to late clients
	snap, err := e.c.Snapshot(ctxT(t))
	if err != nil {
		t.Fatal(err)
	}
	if len(snap.Pending) != 1 || snap.Pending[0].Token != res.Pending.Token {
		t.Fatalf("snapshot pending = %+v", snap.Pending)
	}
	refreshes := e.f.Sessions.Count("Refresh")
	done, err := e.c.Confirm(ctxT(t), res.Pending.Token, true)
	if err != nil {
		t.Fatal(err)
	}
	if done.Err {
		t.Fatalf("confirm = %+v", done)
	}
	if c := e.f.Claude.CallsTo("Stop"); len(c) != 1 || c[0].Args[0] != "bg01" {
		t.Fatalf("Stop calls = %v", c)
	}
	// SessionEffect.Refresh: the server refreshes for the remote client
	eventually(t, "refresh after stop", func() bool { return e.f.Sessions.Count("Refresh") > refreshes })
	// the token is single use
	if _, err := e.c.Confirm(ctxT(t), res.Pending.Token, true); !IsCode(err, proto.CodeUnknownToken) {
		t.Fatalf("second confirm err = %v", err)
	}
}

func TestConfirmNo(t *testing.T) {
	e := newEnv(t)
	res, _ := e.c.Run(ctxT(t), "session.stop", nil, proto.Context{SessionID: bgSession.SessionID})
	out, err := e.c.Confirm(ctxT(t), res.Pending.Token, false)
	if err != nil || out.Toast != "" || out.Err {
		t.Fatalf("no = %+v %v", out, err)
	}
	if e.f.Claude.Count("Stop") != 0 {
		t.Fatal("stopped on no")
	}
	if _, err := e.c.Confirm(ctxT(t), res.Pending.Token, false); !IsCode(err, proto.CodeUnknownToken) {
		t.Fatalf("second no err = %v", err)
	}
}

func TestConfirmExpired(t *testing.T) {
	e := newEnv(t)
	res, _ := e.c.Run(ctxT(t), "session.stop", nil, proto.Context{SessionID: bgSession.SessionID})
	e.f.Clock.Advance(app.ConfirmTTL + 1)
	_, err := e.c.Confirm(ctxT(t), res.Pending.Token, true)
	var api *APIError
	if !errors.As(err, &api) || api.Status != http.StatusGone || api.Code != proto.CodeExpired {
		t.Fatalf("expired err = %v", err)
	}
	if api.Error() == "" {
		t.Fatal("empty error text")
	}
}

func TestRemoveClosesServerPanes(t *testing.T) {
	e := newEnv(t)
	op, err := e.c.OpenPane(ctxT(t), proto.PaneOpenRequest{SessionID: bgSession.SessionID})
	if err != nil {
		t.Fatal(err)
	}
	res, err := e.c.Run(ctxT(t), "session.remove", nil, proto.Context{SessionID: bgSession.SessionID})
	if err != nil || res.Pending == nil {
		t.Fatalf("remove = %+v %v", res, err)
	}
	if e.f.Panes.Pane(op.PaneID).Closed() {
		t.Fatal("pane closed before yes")
	}
	if _, err := e.c.Confirm(ctxT(t), res.Pending.Token, true); err != nil {
		t.Fatal(err)
	}
	if !e.f.Panes.Pane(op.PaneID).Closed() {
		t.Fatal("pane not closed after yes")
	}
	ps, _ := e.c.Panes(ctxT(t))
	if len(ps) != 0 {
		t.Fatalf("panes = %+v", ps)
	}
}

func TestRunErrors(t *testing.T) {
	e := newEnv(t)
	cases := []struct {
		name   string
		ctx    proto.Context
		args   map[string]string
		status int
		code   string
	}{
		{"nope.nada", proto.Context{}, nil, http.StatusNotFound, proto.CodeUnknownCommand},
		{"pane.zoom", proto.Context{}, nil, http.StatusBadRequest, proto.CodeClientCommand},
		{"session.stop", proto.Context{}, nil, http.StatusConflict, proto.CodeUnavailable},
		{"group.notify", proto.Context{}, nil, http.StatusBadRequest, proto.CodeMissingArg},
		{"group.notify", proto.Context{}, map[string]string{"tag": "nenhum", "text": "oi"}, http.StatusUnprocessableEntity, proto.CodeFailed},
	}
	for _, c := range cases {
		_, err := e.c.Run(ctxT(t), c.name, c.args, c.ctx)
		var api *APIError
		if !errors.As(err, &api) || api.Status != c.status || api.Code != c.code {
			t.Errorf("%s: err = %v (%+v)", c.name, err, api)
		}
	}
}

func TestKeymap(t *testing.T) {
	e := newEnv(t)
	km, err := e.c.Keymap(ctxT(t))
	if err != nil {
		t.Fatal(err)
	}
	by := map[string]core.Binding{}
	for _, b := range km {
		by[b.Command] = b
	}
	// a session-gated command is in the keymap even with no selection
	stop, ok := by["session.stop"]
	if !ok || !slices.Equal(stop.Keys, []string{"x"}) || stop.Mode != core.ModeNormal ||
		stop.Help.Key != "x" || stop.Help.Desc == "" {
		t.Fatalf("session.stop binding = %+v", stop)
	}
	if b := by["pane.close"]; !slices.Equal(b.Keys, []string{"w"}) {
		t.Fatalf("pane.close = %+v", b)
	}
	// palette-only commands (no keys) are not bindings
	if _, ok := by["sessions.archive_stale"]; ok {
		t.Fatal("palette-only command in keymap")
	}
	// a binding with no mode is normal mode
	_ = e.a.Registry().Register(core.Command{Name: "x.multi", Title: "multi", Keys: []string{"ctrl+a", "ctrl+b"}},
		func(app.Context, app.Args) (app.Result, error) { return app.Result{}, nil })
	km, _ = e.c.Keymap(ctxT(t))
	var multi core.Binding
	for _, b := range km {
		if b.Command == "x.multi" {
			multi = b
		}
	}
	if multi.Mode != core.ModeNormal || multi.Help.Key != "ctrl+a/ctrl+b" || multi.Help.Desc != "multi" {
		t.Fatalf("x.multi = %+v", multi)
	}
	// a rebinding (Lua nk.keymap.set) is served at once
	if err := e.a.Registry().SetKeys("x.multi", "", []string{"ctrl+z"}); err != nil {
		t.Fatal(err)
	}
	km, _ = e.c.Keymap(ctxT(t))
	for _, b := range km {
		if b.Command == "x.multi" && !slices.Equal(b.Keys, []string{"ctrl+z"}) {
			t.Fatalf("rebound x.multi = %+v", b)
		}
	}
}

func TestSnapshot(t *testing.T) {
	e := newEnv(t)
	e.f.Claude.Usage.Limits = nil
	if _, err := e.a.GroupTargets(app.Context{Picked: []string{bgSession.SessionID, idleSession.SessionID}}, "fila"); err != nil {
		t.Fatal(err)
	}
	e.f.Todo.Avail = true
	e.f.Todo.Groups = []todo.Group{{Group: "g", Tasks: []todo.Task{{ID: 1, Title: "t"}}}}
	if _, err := e.a.LoadTodos(); err != nil {
		t.Fatal(err)
	}
	snap, err := e.c.Snapshot(ctxT(t))
	if err != nil {
		t.Fatal(err)
	}
	if len(snap.Sessions) != 3 || snap.Sessions[0].SessionID != bgSession.SessionID ||
		snap.Sessions[0].Label != "fila" || snap.Sessions[0].ShortID != "bg01" || snap.Sessions[0].Kind != "background" {
		t.Fatalf("sessions = %+v", snap.Sessions)
	}
	// inbox: blocked first, then returned; not the idle background one
	if !slices.Equal(snap.Inbox, []string{bgSession.SessionID, histSession.SessionID}) {
		t.Fatalf("inbox = %v", snap.Inbox)
	}
	if ms := snap.Groups["fila"]; len(ms) != 2 {
		t.Fatalf("groups = %+v", snap.Groups)
	}
	if len(snap.Todos.Groups) != 1 || snap.Todos.Groups[0].Tasks[0].Title != "t" {
		t.Fatalf("todos = %+v", snap.Todos)
	}
	if !snap.At.Equal(e.a.Snapshot().At) || snap.Usage.Limits == nil || snap.Mail == nil || snap.Pending == nil {
		t.Fatalf("snapshot = %+v", snap)
	}
}

func TestSnapshotErrAndMail(t *testing.T) {
	e := newEnv(t)
	e.f.Jira.Cache = jira.Cache{Issues: []jira.Issue{{Key: "K-1"}}}
	e.a.OnPeer(peerEvent(bgSession.SessionID, "oi"))
	snap, err := e.c.Snapshot(ctxT(t))
	if err != nil {
		t.Fatal(err)
	}
	if _, ok := snap.Mail[bgSession.SessionID]; !ok {
		t.Fatalf("mail = %+v", snap.Mail)
	}
}

func peerEvent(to, text string) events.Event {
	return events.Event{Name: "UserPromptSubmit", Type: "peer", SessionID: to, From: "docs", Message: text}
}
