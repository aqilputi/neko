package app_test

import (
	"context"
	"errors"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"github.com/aqilputi/nekuroddo/internal/app"
	"github.com/aqilputi/nekuroddo/internal/app/apptest"
	"github.com/aqilputi/nekuroddo/internal/config"
	"github.com/aqilputi/nekuroddo/internal/core"
	"github.com/aqilputi/nekuroddo/internal/jira"
	"github.com/aqilputi/nekuroddo/internal/todo"
)

// tjApp is an app over fakes with the todo skill installed, Jira on and two
// tasks (#1 plain with @repo, #2 linked to PROJ-7).
func tjApp(t *testing.T) (*app.App, *apptest.Fakes) {
	t.Helper()
	t.Setenv("XDG_CACHE_HOME", t.TempDir()) // no real jira.json
	cfg := config.Default()
	cfg.Jira.Enabled = true
	cfg.Jira.DefaultProject, cfg.Jira.DefaultType = "PROJ", "Task"
	cfg.Jira.CacheMinutes = 30
	f := apptest.New()
	f.Todo.Avail = true
	f.Todo.Groups = tjGroups()
	a := app.New(cfg, f.Ports())
	t.Cleanup(a.Close)
	return a, f
}

func tjGroups() []todo.Group {
	return []todo.Group{{Group: "hoje", Tasks: []todo.Task{
		{ID: 1, Title: "escrever testes", Contexts: []string{"@repo"}},
		{ID: 2, Title: "já no jira", Refs: []todo.Ref{{Kind: "jira", Value: "PROJ-7"}}},
	}}}
}

// tjEvents collects the names (and payloads) published from now on.
func tjEvents(t *testing.T, a *app.App) func() []core.Event {
	t.Helper()
	ctx, cancel := context.WithCancel(context.Background())
	t.Cleanup(cancel)
	ch := a.Subscribe(ctx)
	return func() []core.Event {
		var out []core.Event
		for {
			select {
			case e := <-ch:
				out = append(out, e)
			case <-time.After(20 * time.Millisecond):
				return out
			}
		}
	}
}

// tjHas finds the latest event called name.
func tjHas(evs []core.Event, name string) (core.Event, bool) {
	for i := len(evs) - 1; i >= 0; i-- {
		if evs[i].Name == name {
			return evs[i], true
		}
	}
	return core.Event{}, false
}

func TestTodoAvailable(t *testing.T) {
	a, f := tjApp(t)
	if !a.TodoAvailable() {
		t.Fatal("the skill is there")
	}
	f.Todo.Avail = false
	if a.TodoAvailable() {
		t.Fatal("no skill")
	}
	p := f.Ports()
	p.Todo = nil
	if app.New(config.Default(), p).TodoAvailable() {
		t.Fatal("no port: not available")
	}
}

func TestLoadTodosKeepsListAndError(t *testing.T) {
	a, f := tjApp(t)
	evs := tjEvents(t, a)
	g, err := a.LoadTodos()
	if err != nil || len(g) != 1 || len(g[0].Tasks) != 2 {
		t.Fatalf("loaded: %v %v", g, err)
	}
	if l := a.Todos(); l.Err != "" || len(l.Groups) != 1 {
		t.Fatalf("kept: %+v", l)
	}
	e, ok := tjHas(evs(), app.EvTodosChanged)
	if !ok || len(e.Data.(app.TodoList).Groups) != 1 {
		t.Fatalf("todos.changed: %+v", e)
	}
	// a failure keeps the last good list and remembers the error
	f.Todo.Errs = map[string]error{"List": errors.New("python explodiu")}
	if _, err := a.LoadTodos(); err == nil {
		t.Fatal("error returned")
	}
	l := a.Todos()
	if l.Err != "python explodiu" || len(l.Groups) != 1 {
		t.Fatalf("error kept with the old list: %+v", l)
	}
	if e, ok := tjHas(evs(), app.EvTodosChanged); !ok || e.Data.(app.TodoList).Err == "" {
		t.Fatal("todos.changed carries the error")
	}
	if l.Error() == nil || (app.TodoList{}).Error() != nil {
		t.Fatal("TodoList.Error")
	}
}

func TestStartTodoReloads(t *testing.T) {
	a, f := tjApp(t)
	f.Todo.Errs = map[string]error{"Start": errors.New("ignored")}
	g, err := a.StartTodo(1)
	if err != nil || len(g) != 1 {
		t.Fatalf("reload after start: %v", err)
	}
	if c := f.Todo.CallsTo("Start"); len(c) != 1 || c[0].Args[0] != 1 {
		t.Fatalf("start #1: %v", c)
	}
	if f.Todo.Count("List") != 1 {
		t.Fatal("reloaded")
	}
}

func TestDoneTodoAsksThenReloads(t *testing.T) {
	a, f := tjApp(t)
	q := app.DoneQuestion(1, strings.Repeat("x", 80))
	if !strings.HasPrefix(q, "marcar #1 \"") || !strings.HasSuffix(q, "\" como done?") || !strings.Contains(q, "…") {
		t.Fatalf("today's question, title truncated: %q", q)
	}
	res := a.AskDoneTodo(todo.Task{ID: 1, Title: "escrever testes"})
	if res.Pending == nil || res.Pending.Question != `marcar #1 "escrever testes" como done?` {
		t.Fatalf("asks: %+v", res)
	}
	if f.Todo.Count("Done") != 0 {
		t.Fatal("nothing before yes")
	}
	got, err := a.Confirm(res.Pending.Token)
	if err != nil {
		t.Fatal(err)
	}
	if c := f.Todo.CallsTo("Done"); len(c) != 1 || c[0].Args[0] != 1 {
		t.Fatalf("done #1: %v", c)
	}
	if l, ok := got.Data.(app.TodoList); !ok || len(l.Groups) != 1 {
		t.Fatalf("reloaded list in Data: %+v", got)
	}
}

func TestTodoForKeyAndFind(t *testing.T) {
	g := tjGroups()
	if tk := app.TodoForKey(g, "PROJ-7"); tk == nil || tk.ID != 2 {
		t.Fatal("the task linked to PROJ-7")
	}
	if app.TodoForKey(g, "PROJ-8") != nil || app.TodoForKey(g, "") != nil {
		t.Fatal("untracked")
	}
	if tk := app.FindTodo(g, 1); tk == nil || tk.Title != "escrever testes" {
		t.Fatal("by id")
	}
	if app.FindTodo(g, 9) != nil {
		t.Fatal("unknown id")
	}
}

func TestTodoDispatchDir(t *testing.T) {
	home := t.TempDir()
	t.Setenv("HOME", home)
	repo := filepath.Join(t.TempDir(), "repo")
	cfg := config.Default()
	cfg.TodoRepos = map[string]string{"@repo": repo}
	if d := app.TodoDispatchDir(cfg, todo.Task{Contexts: []string{"@nada", "@repo"}}); d != repo {
		t.Fatalf("first context with a repo: %q", d)
	}
	if d := app.TodoDispatchDir(cfg, todo.Task{}); d != "~" {
		t.Fatalf("no repo: home: %q", d)
	}
	cfg.TodoRepos["@casa"] = filepath.Join(home, "casa")
	if d := app.TodoDispatchDir(cfg, todo.Task{Contexts: []string{"@casa"}}); d != "~/casa" {
		t.Fatalf("under home: tilde: %q", d)
	}
	if d := app.IssueDispatchDir(jira.Config{}); d != "~/visio/dev" {
		t.Fatalf("default agent dir: %q", d)
	}
	if d := app.IssueDispatchDir(jira.Config{AgentDir: "/srv"}); d != "/srv" {
		t.Fatalf("configured agent dir: %q", d)
	}
}

func TestDispatchDirFromContext(t *testing.T) {
	a, f := tjApp(t)
	if _, err := a.DispatchDir(app.Context{TodoID: 1}); err == nil {
		t.Fatal("before loading nothing is known")
	}
	if _, err := a.LoadTodos(); err != nil {
		t.Fatal(err)
	}
	if d, err := a.DispatchDir(app.Context{TodoID: 2}); err != nil || d != "~" {
		t.Fatalf("task without repo: %q %v", d, err)
	}
	f.Jira.Cache = jira.Cache{Issues: []jira.Issue{{Key: "PROJ-8"}}}
	if _, _, err := a.FetchJira(true); err != nil {
		t.Fatal(err)
	}
	if d, err := a.DispatchDir(app.Context{IssueKey: "PROJ-8"}); err != nil || d != "~/visio/dev" {
		t.Fatalf("issue: %q %v", d, err)
	}
	if _, err := a.DispatchDir(app.Context{IssueKey: "X-1"}); err == nil {
		t.Fatal("unknown issue")
	}
	if _, err := a.DispatchDir(app.Context{}); err == nil {
		t.Fatal("nothing selected")
	}
}

func TestIssueToTodo(t *testing.T) {
	a, f := tjApp(t)
	g := tjGroups()
	res := a.IssueToTodo(g, "PROJ-7", "ligada")
	if !res.Err || res.Toast != "PROJ-7 já está no todo #2" {
		t.Fatalf("already linked: %+v", res)
	}
	if f.Todo.Count("AddFromJira") != 0 {
		t.Fatal("nothing added")
	}
	res = a.IssueToTodo(g, "PROJ-8", "solta")
	if res.Err {
		t.Fatalf("added: %+v", res)
	}
	if c := f.Todo.CallsTo("AddFromJira"); len(c) != 1 || c[0].Args[0] != "PROJ-8" || c[0].Args[1] != "solta" {
		t.Fatalf("AddFromJira: %v", c)
	}
	if l, ok := res.Data.(app.TodoList); !ok || len(l.Groups) != 1 {
		t.Fatalf("reloaded: %+v", res.Data)
	}
	f.Todo.Errs = map[string]error{"AddFromJira": errors.New("sem skill")}
	if res := a.IssueToTodo(g, "PROJ-9", "x"); !res.Err || res.Toast != "sem skill" {
		t.Fatalf("failure: %+v", res)
	}
}
