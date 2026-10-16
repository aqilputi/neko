package ui

import (
	"context"
	"errors"
	"strings"
	"testing"
	"time"

	tea "charm.land/bubbletea/v2"

	"github.com/aqilputi/nekuroddo/internal/app"
	"github.com/aqilputi/nekuroddo/internal/config"
	"github.com/aqilputi/nekuroddo/internal/core"
	"github.com/aqilputi/nekuroddo/internal/groups"
	"github.com/aqilputi/nekuroddo/internal/todo"
)

// luaCmd registers a command as a Lua file would: its runner records the
// selection and arguments it got.
func luaCmd(t *testing.T, m *Model, c core.Command, res app.Result, err error) *[]app.Args {
	t.Helper()
	var got []app.Args
	c.Source = "lua:init.lua"
	if err2 := m.app.Registry().Register(c, func(_ app.Context, args app.Args) (app.Result, error) {
		got = append(got, args)
		return res, err
	}); err2 != nil {
		t.Fatal(err2)
	}
	return &got
}

func paletteItem2(m *Model, title string) *paletteItem {
	for _, it := range m.paletteItems() {
		if it.title == title {
			return &it
		}
	}
	return nil
}

// A key bound to a Lua command runs it in the core (off the UI goroutine).
func TestKeyRunsARegisteredCommand(t *testing.T) {
	m := bhModel(t, bhSessions()...)
	got := luaCmd(t, &m, core.Command{Name: "lua.hello", Title: "diz oi", Keys: []string{"ctrl+h"}}, app.Info("oi!"), nil)
	handled, cmd := m.keyNormal(keyPress("ctrl+h"))
	if !handled {
		t.Fatal("a bound key is handled")
	}
	if _, ok := bhHasToast(cmd, "oi!"); !ok || len(*got) != 1 {
		t.Fatalf("the command ran and its toast shows: %v", *got)
	}
	// and it shows in the palette, with its key
	it := paletteItem2(&m, "diz oi")
	if it == nil || it.hint != "ctrl+h" {
		t.Fatalf("Lua commands show in the palette: %+v", it)
	}
	if _, ok := bhHasToast(it.run(&m), "oi!"); !ok {
		t.Fatal("the palette runs it by name")
	}
}

// A command with arguments waits for them typed after its name.
func TestPaletteTypedArguments(t *testing.T) {
	m := bhModel(t, bhSessions()...)
	got := luaCmd(t, &m, core.Command{Name: "deploy", Title: "faz deploy", Args: []core.Arg{
		{Name: "env", Required: true}, {Name: "note"}, {Name: "msg"},
	}}, app.Info("foi"), nil)
	it := paletteItem2(&m, "faz deploy")
	if it == nil {
		t.Fatal("listed")
	}
	it.run(&m)
	d, ok := m.top().(*paletteDialog)
	if !ok || d.input.Value() != "deploy " {
		t.Fatal("running it asks for the arguments in the palette")
	}
	w := m.typedCommand("deploy")
	if w == nil || w.run != nil || w.title != "deploy <env> [note] [msg]" {
		t.Fatalf("missing the required one: %+v", w)
	}
	r := m.typedCommand("deploy prod  urgente  bem rápido ")
	if r == nil || r.run == nil || r.title != "▶ deploy prod urgente bem rápido" {
		t.Fatalf("typed: %+v", r)
	}
	if _, ok := bhHasToast(r.run(&m), "foi"); !ok || (*got)[0]["env"] != "prod" || (*got)[0]["msg"] != "bem rápido" {
		t.Fatalf("args: %v", *got)
	}
	// optional ones may be left out
	if r := m.typedCommand("deploy prod"); r == nil || r.run == nil || r.title != "▶ deploy prod" {
		t.Fatalf("only the required: %+v", r)
	}
	// not a command, a command without arguments, a client one
	for _, q := range []string{"  ", "nada", "jira.fetch x", "nav.down 3"} {
		if m.argCommand(q) != nil {
			t.Errorf("%q is not a typed command", q)
		}
	}
}

func TestPaletteTypedGroupCommands(t *testing.T) {
	m := bhModel(t, bhSessions()...)
	bhSaveGroup(t, &m, "t1", groups.Member{SessionID: "a"})
	bhSelect(t, &m, "a")
	if name, args := m.groupArgs("n oi pessoal"); name != "group.notify" || args["tag"] != "t1" || args["text"] != "oi pessoal" {
		t.Fatalf("notify defaults to the cursor's group: %s %v", name, args)
	}
	if name, args := m.groupArgs("notify #outro oi"); name != "group.notify" || args["tag"] != "outro" || args["text"] != "oi" {
		t.Fatalf("notify #tag: %s %v", name, args)
	}
	if name, args := m.groupArgs("ug T1"); name != "group.remove" || args["tag"] != "t1" {
		t.Fatalf("ungroup tag: %s %v", name, args)
	}
	it := m.typedCommand("notify oi")
	if _, ok := bhHasToast(it.run(&m), "#t1"); !ok || !m.cat.Awake() {
		t.Fatal("notify runs group.notify and the cat says so")
	}
}

func TestCommandResult(t *testing.T) {
	m := bhModel(t, bhSessions()...)
	bhPane(t, &m, "p1", "a")
	m.active = "p1"
	// panes closed and a refresh
	m.refreshing = false
	cmd := m.commandResult("x", app.Result{Data: app.SessionEffect{ClosePanes: []string{"p1"}, Refresh: true}}, nil)
	if cmd == nil || m.panes["p1"] != nil || m.active != "" {
		t.Fatal("the panes close and a refresh starts")
	}
	m.refreshing = true
	// the todos redrawn
	bhSetTodos(t, &m, []todo.Group{{Group: "g", Tasks: []todo.Task{{ID: 4}}}})
	m.commandResult("x", app.Result{Data: app.TodoList{}}, nil)
	if len(m.todos) != 1 {
		t.Fatal("a todo list redraws the tab")
	}
	// an agent announced
	if msgs := bhRun(m.commandResult("x", app.Result{Data: app.Dispatched{ID: "abcd"}}, nil)); len(msgs) != 1 || msgs[0].(dispatchedMsg).id != "abcd" {
		t.Fatalf("dispatched: %v", msgs)
	}
	// a fullscreen request only the O key carries out
	if _, ok := bhHasToast(m.commandResult("lua.x", app.Result{Exec: &core.ExecRequest{Argv: []string{"x"}}}, nil), "tela cheia"); !ok {
		t.Fatal("Exec")
	}
	// an error says what the core wrote, else the error
	if tm, ok := bhHasToast(m.commandResult("x", app.Result{Toast: "explicado", Err: true}, errors.New("cru")), "explicado"); !ok || !tm.err {
		t.Fatal("the core's toast wins")
	}
	if _, ok := bhHasToast(m.commandResult("x", app.Result{}, errors.New("cru")), "cru"); !ok {
		t.Fatal("else the error")
	}
	// a confirmation asks with its token
	res := m.app.Ask("seguro?", func() (app.Result, error) { return app.Info("feito"), nil })
	if m.commandResult("lua.x", res, nil) != nil {
		t.Fatal("asking returns nothing yet")
	}
	if d, ok := m.top().(*confirmDialog); !ok || d.token != res.Pending.Token {
		t.Fatal("the dialog keeps the token")
	}
	// an unknown command
	if _, ok := bhHasToast(m.runCore("nope", nil), "comando desconhecido"); !ok {
		t.Fatal("unknown command")
	}
}

func TestShowPlan(t *testing.T) {
	m := bhModel(t, bhSessions()...)
	bhPane(t, &m, "p1", "a")
	if cmd := m.showPlan(app.AttachPlan{ID: "p1", Reuse: true, Focus: true, Reply: "oi", ReplyIn: time.Millisecond}); cmd == nil ||
		m.active != "p1" || m.mode != modeInsert {
		t.Fatal("reuse: shown, focused, the reply typed later")
	}
	m.mode = modeNormal
	if cmd := m.commandResult("session.reply", app.Result{Data: app.AttachPlan{ID: "p1", Reuse: true}}, nil); cmd != nil || m.mode != modeNormal {
		t.Fatal("reuse without focus or reply")
	}
	if cmd := m.showPlan(app.AttachPlan{ID: "p2", Dir: "/", Argv: []string{"/bin/sleep", "5"}}); cmd == nil || m.panes["p2"] == nil {
		t.Fatal("a new pane starts")
	}
	t.Cleanup(func() { m.closePane("p2") })
}

func TestDoneMsg(t *testing.T) {
	for _, c := range []struct {
		res  app.Result
		err  error
		want string
	}{
		{app.Result{}, errors.New("x"), "toast"},
		{app.Info("oi"), nil, "toast"},
		{app.Result{}, nil, "nil"},
		{app.Result{Data: app.TodoList{}}, nil, "todos"},
		{app.Result{Data: app.Dispatched{ID: "i"}}, nil, "dispatched"},
		{app.Result{Data: app.SessionEffect{Refresh: true}}, nil, "done"},
		{app.Result{Pending: &core.PendingConfirmation{Token: "t"}}, nil, "done"},
		{app.Result{Exec: &core.ExecRequest{}}, nil, "done"},
	} {
		var got string
		switch doneMsg("n", c.res, c.err).(type) {
		case toastMsg:
			got = "toast"
		case nil:
			got = "nil"
		case todosMsg:
			got = "todos"
		case dispatchedMsg:
			got = "dispatched"
		case commandDoneMsg:
			got = "done"
		}
		if got != c.want {
			t.Errorf("%+v %v: %s, want %s", c.res, c.err, got, c.want)
		}
	}
	// commandDoneMsg goes through commandResult on the UI goroutine
	m := bhModel(t, bhSessions()...)
	if _, ok := bhHasToast(m.update(commandDoneMsg{name: "x", err: errors.New("falhou")}), "falhou"); !ok {
		t.Fatal("commandDoneMsg")
	}
}

// A confirmation answered elsewhere (another client, or expired) closes its
// dialog here: the TUI follows confirm.resolved on the core's bus.
func TestBusClosesAnsweredConfirmations(t *testing.T) {
	m := bhModel(t, bhSessions()...)
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	listen := listenBus(m.app.Subscribe(ctx))
	res := m.app.Ask("seguro?", func() (app.Result, error) { return app.Result{}, nil })
	m.commandResult("lua.x", res, nil)
	m.app.Publish("toast", app.ToastData{Text: "ignorado"}) // other events are skipped
	if err := m.app.Cancel(res.Pending.Token); err != nil {
		t.Fatal(err)
	}
	msg, ok := listen().(busMsg)
	if !ok || msg.ev.Name != app.EvConfirmResolved {
		t.Fatalf("listen = %+v", msg)
	}
	if m.update(msg) == nil || m.top() != nil {
		t.Fatal("the dialog closes and the TUI keeps listening")
	}
	if m.onBus(core.Event{Name: "x"}) != nil {
		t.Fatal("other events do nothing")
	}
	cancel()
	if listenBus(m.app.Subscribe(ctx))() != nil {
		t.Fatal("a closed subscription ends the listening")
	}
}

// The first window size starts following the bus.
func TestFirstSizeFollowsTheBus(t *testing.T) {
	m := New(config.Default())
	cmd := m.update(tea.WindowSizeMsg{Width: 80, Height: 20})
	if _, ok := cmd().(tea.BatchMsg); !ok {
		t.Fatal("first size: resize tick and the bus")
	}
}

func TestCommandCtx(t *testing.T) {
	m := bhTodoModel(t, true)
	m.todoCur = 0
	if c := m.commandCtx(); c.TodoID != 1 || c.Tab != "todos" {
		t.Fatalf("task: %+v", c)
	}
	m.todoCur = 3
	if c := m.commandCtx(); c.IssueKey != "PROJ-8" || c.TodoID != 0 {
		t.Fatalf("issue: %+v", c)
	}
	// i with no pane open dispatches, as enter does
	m.todoCur = 0
	m.keyNormal(keyPress("i"))
	if m.mode != modeInput || m.purpose != inDispatchDir {
		t.Fatal("i on a task asks where its agent starts")
	}
	m.mode = modeNormal
	m.tab = tabArchive
	if c := m.commandCtx(); c.Tab != app.TabArchive || c.IssueKey != "" {
		t.Fatalf("archive: %+v", c)
	}
}

// Keys with nothing to act on do nothing.
func TestKeysWithoutSelection(t *testing.T) {
	m := bhModel(t)
	for _, k := range []string{"enter", "o", "O", "r", "n", "m", "i"} {
		if handled, cmd := m.keyNormal(keyPress(k)); !handled || cmd != nil || m.top() != nil || m.mode != modeNormal {
			t.Errorf("%s with nothing selected: %v", k, cmd)
		}
	}
	// a two-key sequence interrupted runs the second key
	m = bhModel(t, bhSessions()...)
	m.view.Cursor = 0
	m.keyNormal(keyPress("g"))
	m.keyNormal(keyPress("j"))
	if m.pendingKey() != "" || m.view.Cursor != 1 {
		t.Fatalf("g then j: cursor %d", m.view.Cursor)
	}
	// a sequence the TUI cannot wait on (it keeps only gg and dd) never runs
	got := luaCmd(t, &m, core.Command{Name: "lua.yy", Title: "yy", Keys: []string{"yy"}}, app.Result{}, nil)
	m.keyNormal(keyPress("y"))
	if handled, _ := m.keyNormal(keyPress("y")); !handled || len(*got) != 0 {
		t.Fatal("yy is swallowed, not run")
	}
	if handled, _ := m.keyNormal(keyPress("Z")); handled {
		t.Fatal("unbound")
	}
}

// inbox.summon (palette only) goes to the inbox.
func TestInboxSummon(t *testing.T) {
	m := bhModel(t, bhSessions()...)
	it := paletteItem2(&m, "inbox: vai para quem te espera")
	if it == nil || it.run(&m) != nil || m.tab != tabInbox {
		t.Fatal("inbox.summon")
	}
}

// A key bound in INSERT reaches nekuroddo even inside the pane.
func TestInsertModeCommands(t *testing.T) {
	m := bhModel(t, bhSessions()...)
	bhPane(t, &m, "p1", "a")
	m.active, m.mode = "p1", modeInsert
	got := luaCmd(t, &m, core.Command{Name: "lua.ins", Title: "ins", Keys: []string{"ctrl+o"}, Mode: core.ModeInsert}, app.Info("ok"), nil)
	if _, ok := bhHasToast(m.key(keyPress("ctrl+o")), "ok"); !ok || len(*got) != 1 {
		t.Fatal("an INSERT binding runs")
	}
}

// An unbound command shows nowhere in the help.
func TestUnboundCommandLeavesTheHelp(t *testing.T) {
	m := New(config.Default())
	if err := m.app.Registry().SetKeys("view.theme", "", nil); err != nil {
		t.Fatal(err)
	}
	if b := m.keys().Theme; b.Enabled() {
		t.Fatal("unbound: disabled")
	}
	for _, col := range m.fullHelp() {
		for _, b := range col {
			if b.Help().Desc == "tema" && b.Enabled() {
				t.Fatal("tema still in the help")
			}
		}
	}
	if handled, _ := m.keyNormal(keyPress("T")); handled {
		t.Fatal("T no longer bound")
	}
	if !strings.Contains(m.keys().Help.Help().Key, "ctrl+/") {
		t.Fatal("the rest stays")
	}
}
