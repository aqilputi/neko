package app_test

import (
	"errors"
	"reflect"
	"strings"
	"testing"

	"github.com/aqilputi/nekuroddo/internal/app"
	"github.com/aqilputi/nekuroddo/internal/app/apptest"
	"github.com/aqilputi/nekuroddo/internal/config"
	"github.com/aqilputi/nekuroddo/internal/core"
	"github.com/aqilputi/nekuroddo/internal/jira"
	"github.com/aqilputi/nekuroddo/internal/store"
	"github.com/aqilputi/nekuroddo/internal/todo"
)

// clientNames are the plan §2.4 names every client handles itself: they are
// listed (palette, keymap, GUI) but never run in the core.
var clientNames = strings.Fields(`nav.up nav.down nav.top nav.bottom nav.half_up nav.half_down
	nav.page_up nav.page_down nav.collapse nav.expand nav.pick nav.next_attention nav.prev_attention
	tab.sessions tab.inbox tab.todos tab.archive tab.next tab.prev view.filter view.show_all view.sort
	view.theme view.flat palette.open switcher.open help.open usage.show app.quit
	session.insert pane.close pane.zoom`)

var cmdBg = store.Session{SessionID: "a", ShortID: "aaaa1111", Name: "alpha", Kind: "background", Cwd: "/w"}

// cmdApp is a core with every builtin registered, a background session,
// two tasks and a cached Jira issue.
func cmdApp(t *testing.T) (*app.App, *apptest.Fakes) {
	t.Helper()
	t.Setenv("XDG_CACHE_HOME", t.TempDir()) // no real jira.json
	cfg := config.Default()
	cfg.Jira.Enabled = true
	cfg.Jira.DefaultProject, cfg.Jira.DefaultType = "PROJ", "Task"
	f := apptest.New()
	f.Claude.NextID = "bbbb2222"
	f.Sessions.SetSnapshot(store.Snapshot{Sessions: []store.Session{cmdBg,
		{SessionID: "h", Name: "hist", Kind: "history", Cwd: "/old"}}})
	f.Todo.Avail = true
	f.Todo.Groups = []todo.Group{{Group: "hoje", Tasks: []todo.Task{
		{ID: 1, Title: "escrever testes", Contexts: []string{"@repo"}},
	}}}
	f.Jira.Cache = jira.Cache{FetchedAt: f.Clock.Now(), Issues: []jira.Issue{{Key: "K-1", Summary: "uma issue", URL: "https://x/K-1"}}}
	a := app.New(cfg, f.Ports())
	t.Cleanup(a.Close)
	if err := a.RegisterBuiltins(); err != nil {
		t.Fatal(err)
	}
	a.Refresh()
	if _, err := a.LoadTodos(); err != nil {
		t.Fatal(err)
	}
	if _, _, err := a.FetchJira(true); err != nil {
		t.Fatal(err)
	}
	f.Claude.Reset()
	f.Todo.Reset()
	return a, f
}

func TestBuiltinsRegisterEveryPlanName(t *testing.T) {
	a, _ := cmdApp(t)
	all := map[string]core.Command{}
	for _, c := range a.Registry().All() {
		all[c.Name] = c
	}
	for _, n := range planNames {
		if _, ok := all[n]; !ok {
			t.Errorf("%s is not registered", n)
		}
	}
	client := map[string]bool{}
	for _, n := range clientNames {
		client[n] = true
		c := all[n]
		if c.Scope != core.ScopeClient || c.Title == "" {
			t.Errorf("%s: scope %q title %q", n, c.Scope, c.Title)
		}
		if _, err := a.Registry().Run(app.Context{}, n, nil); !errors.Is(err, app.ErrClientCommand) {
			t.Errorf("%s runs in the core: %v", n, err)
		}
	}
	for n, c := range all {
		if !client[n] && c.Scope != core.ScopeCore {
			t.Errorf("%s: scope %q", n, c.Scope)
		}
	}
}

func TestBuiltinKeymap(t *testing.T) {
	a, _ := cmdApp(t)
	km := a.Keymap()
	if c := km.Collisions(); len(c) != 0 {
		t.Fatalf("keys bound twice in a mode: %v", c)
	}
	for _, c := range []struct {
		name, mode string
		keys       []string
	}{
		{"nav.top", core.ModeNormal, []string{"gg"}},
		{"session.remove", core.ModeNormal, []string{"dd"}},
		{"session.new", core.ModeNormal, []string{"c"}},
		{"help.open", core.ModeNormal, []string{"ctrl+/", "?", "ctrl+_"}},
		{"switcher.open", core.ModeAny, []string{"ctrl+k"}},
		{"todo.dispatch", core.ModeNormalTodos, []string{"enter"}},
		{"jira.open_url", core.ModeNormalTodos, []string{"o"}},
		{"todo.start", core.ModeNormalTodos, []string{"s"}},
	} {
		b, ok := km.Find(c.name)
		if !ok || b.Mode != c.mode || !reflect.DeepEqual(b.Keys, c.keys) || b.Help.Desc == "" {
			t.Errorf("%s = %+v", c.name, b)
		}
	}
	// every key a binding has resolves back to its command in its mode
	for _, b := range km {
		for _, k := range b.Keys {
			if got, _ := km.Resolve(b.Mode, k); got != b.Command {
				t.Errorf("%s %q resolves to %q, not %s", b.Mode, k, got, b.Command)
			}
		}
	}
	// the todos tab still reaches the normal keys it does not shadow
	if got, _ := km.Resolve(core.ModeNormalTodos, "c"); got != "session.new" {
		t.Fatalf("todos c = %q", got)
	}
	// palette-only commands have no binding
	if _, ok := km.Find("sessions.archive_stale"); ok {
		t.Fatal("palette-only command bound")
	}
}

// argValue is a value a test can pass for an argument of type typ.
func argValue(typ string) string {
	switch typ {
	case core.ArgDir:
		return "/tmp"
	case core.ArgInt:
		return "1"
	case core.ArgTag:
		return "t"
	}
	return "x"
}

func TestEveryCoreCommandRuns(t *testing.T) {
	a, _ := cmdApp(t)
	ctx := app.Context{SessionID: "a", TodoID: 1, IssueKey: "K-1"}
	for _, c := range a.Registry().All() {
		if c.Scope != core.ScopeCore {
			continue
		}
		args := app.Args{}
		for _, arg := range c.Args {
			args[arg.Name] = argValue(arg.Type)
		}
		_, err := a.Run(ctx, c.Name, args)
		for _, bad := range []error{app.ErrUnknownCommand, app.ErrClientCommand, app.ErrUnavailable, app.ErrMissingArg} {
			if errors.Is(err, bad) {
				t.Errorf("%s: %v", c.Name, err)
			}
		}
	}
}

func TestTodoDispatchCommand(t *testing.T) {
	a, f := cmdApp(t)
	// a task: its repo by default, and the agent starts the task
	res, err := a.Run(app.Context{TodoID: 1}, "todo.dispatch", nil)
	if err != nil {
		t.Fatal(err)
	}
	d, _ := res.Data.(app.Dispatched)
	if d.ID != "bbbb2222" || d.TaskID != 1 || f.Todo.Count("Start") != 1 {
		t.Fatalf("task dispatch = %+v", res)
	}
	call := f.Claude.Last()
	if call.Method != "Dispatch" || call.Args[1] != "todo #1 escrever testes" || call.Args[2] != todo.Prompt(f.Todo.Groups[0].Tasks[0]) {
		t.Fatalf("Dispatch = %+v", call)
	}
	// an issue, in the directory given
	if _, err := a.Run(app.Context{IssueKey: "K-1"}, "todo.dispatch", app.Args{"dir": "/r"}); err != nil {
		t.Fatal(err)
	}
	call = f.Claude.Last()
	if call.Args[0] != "/r" || call.Args[1] != "K-1 uma issue" || !strings.Contains(call.Args[2].(string), "K-1") {
		t.Fatalf("issue Dispatch = %+v", call)
	}
	// bypass asks first
	res, err = a.Run(app.Context{TodoID: 1}, "todo.dispatch", app.Args{"perm": app.PermBypass})
	if err != nil || res.Pending == nil {
		t.Fatalf("bypass = %+v %v", res, err)
	}
	// nothing selected, or gone from the list
	if _, err := a.Run(app.Context{}, "todo.dispatch", nil); !errors.Is(err, app.ErrUnavailable) {
		t.Fatalf("nothing selected: %v", err)
	}
	if _, err := a.Run(app.Context{TodoID: 9}, "todo.dispatch", nil); err == nil {
		t.Fatal("unknown task")
	}
	if _, err := a.Run(app.Context{IssueKey: "NOPE-1"}, "todo.dispatch", nil); err == nil {
		t.Fatal("unknown issue")
	}
	if _, err := a.Run(app.Context{IssueKey: "NOPE-1"}, "todo.dispatch", app.Args{"dir": "/r"}); err == nil {
		t.Fatal("unknown issue, dir given")
	}
	if _, err := a.Run(app.Context{TodoID: 9}, "todo.dispatch", app.Args{"dir": "/r"}); err == nil {
		t.Fatal("unknown task, dir given")
	}
}

func TestSessionReplyCommand(t *testing.T) {
	a, f := cmdApp(t)
	res, err := a.Run(app.Context{SessionID: "a"}, "session.reply", app.Args{"text": "sim"})
	if err != nil {
		t.Fatal(err)
	}
	plan, _ := res.Data.(app.AttachPlan)
	if plan.Reply != "sim" || plan.Focus || plan.SessionID != "a" {
		t.Fatalf("reply plan = %+v", plan)
	}
	// a history session is resumed first, then attached with the reply
	res, err = a.Run(app.Context{SessionID: "h"}, "session.reply", app.Args{"text": "2"})
	plan, _ = res.Data.(app.AttachPlan)
	if err != nil || plan.Reply != "2" || f.Claude.Count("ResumeInBackground") != 1 {
		t.Fatalf("history reply = %+v %v", plan, err)
	}
	f.Claude.Errs = map[string]error{"ResumeInBackground": errors.New("boom")}
	if _, err := a.Run(app.Context{SessionID: "h"}, "session.reply", app.Args{"text": "2"}); err == nil {
		t.Fatal("a failed resume fails")
	}
	if _, err := a.Run(app.Context{SessionID: "a"}, "session.reply", nil); !errors.Is(err, app.ErrMissingArg) {
		t.Fatalf("no text: %v", err)
	}
	if _, err := a.Run(app.Context{}, "session.reply", app.Args{"text": "x"}); !errors.Is(err, app.ErrUnavailable) {
		t.Fatalf("no session: %v", err)
	}
}

func TestSessionReplyInteractive(t *testing.T) {
	f := apptest.New()
	f.Sessions.SetSnapshot(store.Snapshot{Sessions: []store.Session{{SessionID: "i", Kind: "interactive"}}})
	a := app.New(config.Default(), f.Ports())
	t.Cleanup(a.Close)
	if err := a.RegisterBuiltins(); err != nil {
		t.Fatal(err)
	}
	a.Refresh()
	if _, err := a.Run(app.Context{SessionID: "i"}, "session.reply", app.Args{"text": "x"}); err == nil {
		t.Fatal("an interactive session cannot be answered from here")
	}
}

func TestSessionsRefreshCommand(t *testing.T) {
	a, f := cmdApp(t)
	n := f.Sessions.Count("Refresh")
	res, err := a.Run(app.Context{}, "sessions.refresh", nil)
	if err != nil || f.Sessions.Count("Refresh") != n+1 {
		t.Fatalf("refresh = %+v %v", res, err)
	}
}

func TestRegistryAllAndSetKeys(t *testing.T) {
	r := app.NewRegistry()
	hidden := app.WithAvailable(func(app.Context) bool { return false })
	if err := r.Register(core.Command{Name: "a", Keys: []string{"a"}, Mode: core.ModeNormal}, echo(""), hidden); err != nil {
		t.Fatal(err)
	}
	if got := names(r.All()); !reflect.DeepEqual(got, []string{"a"}) || len(r.List(app.Context{})) != 0 {
		t.Fatalf("All lists what List hides: %v", got)
	}
	if err := r.SetKeys("a", core.ModeNormalTodos, []string{"A", "ctrl+a"}); err != nil {
		t.Fatal(err)
	}
	if c, _ := r.Lookup("a"); !reflect.DeepEqual(c.Keys, []string{"A", "ctrl+a"}) || c.Mode != core.ModeNormalTodos {
		t.Fatalf("SetKeys = %+v", c)
	}
	if err := r.SetKeys("a", "", nil); err != nil {
		t.Fatal(err)
	}
	if c, _ := r.Lookup("a"); len(c.Keys) != 0 || c.Mode != core.ModeNormalTodos {
		t.Fatalf("unbind keeps the mode: %+v", c)
	}
	if err := r.SetKeys("nope", "", []string{"x"}); !errors.Is(err, app.ErrUnknownCommand) {
		t.Fatalf("unknown: %v", err)
	}
}

func TestKeymapFollowsTheRegistry(t *testing.T) {
	a, _ := cmdApp(t)
	if err := a.Registry().Register(core.Command{Name: "lua.hello", Title: "oi", Keys: []string{"ctrl+h"}, Source: "lua:init.lua"},
		echo("oi")); err != nil {
		t.Fatal(err)
	}
	if got, _ := a.Keymap().Resolve(core.ModeNormal, "ctrl+h"); got != "lua.hello" {
		t.Fatalf("a Lua command's key = %q", got)
	}
	if err := a.Registry().SetKeys("nav.down", "", []string{"ctrl+n"}); err != nil {
		t.Fatal(err)
	}
	if got, _ := a.Keymap().Resolve(core.ModeNormal, "ctrl+n"); got != "nav.down" {
		t.Fatalf("rebound = %q", got)
	}
}

func TestRegisterKeyCommandsClash(t *testing.T) {
	for _, name := range []string{"nav.down", "sessions.refresh"} {
		a, _ := newApp(t)
		if err := a.Registry().Register(core.Command{Name: name}, echo("")); err != nil {
			t.Fatal(err)
		}
		if err := a.RegisterKeyCommands(); !errors.Is(err, app.ErrDuplicateCommand) {
			t.Fatalf("%s taken: %v", name, err)
		}
	}
}
