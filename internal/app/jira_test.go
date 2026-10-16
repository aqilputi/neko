package app_test

import (
	"encoding/json"
	"errors"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"github.com/aqilputi/nekuroddo/internal/app"
	"github.com/aqilputi/nekuroddo/internal/app/apptest"
	"github.com/aqilputi/nekuroddo/internal/config"
	"github.com/aqilputi/nekuroddo/internal/jira"
	"github.com/aqilputi/nekuroddo/internal/todo"
)

func TestJiraDescription(t *testing.T) {
	d := app.JiraDescription(todo.Task{ID: 3, Desc: "contexto",
		Refs:  []todo.Ref{{Kind: "pr", Value: "#9", URL: "https://pr"}},
		Links: []string{"https://doc"},
		Notes: []todo.Note{{At: "2026-10-01T10:00:00", Text: "nota"}, {At: "x", Text: "curta"}}})
	for _, want := range []string{"contexto\n\n", "- pr: #9 https://pr", "- https://doc", "> 2026-10-01 — nota", "> x — curta", "todo #3_"} {
		if !strings.Contains(d, want) {
			t.Errorf("description lacks %q:\n%s", want, d)
		}
	}
	if d := app.JiraDescription(todo.Task{ID: 1}); d != "\n_criada pelo nekuroddo a partir do todo #1_" {
		t.Fatalf("bare task: %q", d)
	}
}

func TestJiraTargetAndTexts(t *testing.T) {
	cfg := jira.Config{DefaultProject: "PROJ", DefaultType: "Task"}
	if app.JiraDefaultTarget(cfg) != "PROJ/Task" {
		t.Fatal("default target")
	}
	if p, ty := app.JiraTarget(cfg, " vis / Bug "); p != "VIS" || ty != "Bug" {
		t.Fatalf("parsed: %q %q", p, ty)
	}
	if p, ty := app.JiraTarget(cfg, "vis"); p != "VIS" || ty != "Task" {
		t.Fatalf("default type: %q %q", p, ty)
	}
	q := app.CreateIssueQuestion("VIS", "Bug", strings.Repeat("y", 90))
	if !strings.HasPrefix(q, "criar no Jira VIS/Bug, atribuída a você:\n\"") || !strings.HasSuffix(q, "…\"\n(escreve no Jira)") {
		t.Fatalf("question: %q", q)
	}
	if err := app.TaskHasIssue(todo.Task{ID: 2, Refs: []todo.Ref{{Kind: "jira", Value: "PROJ-7"}}}); err == nil || err.Error() != "#2 já tem issue: PROJ-7" {
		t.Fatalf("linked task: %v", err)
	}
	if app.TaskHasIssue(todo.Task{ID: 1}) != nil {
		t.Fatal("plain task")
	}
	if s := app.JiraCreatedText("PROJ-9", 4, 0.3); s != "PROJ-9 criada e ligada ao #4 ($0.30)" {
		t.Fatalf("created: %q", s)
	}
	denied := jira.DeniedError{Reason: "hook"}
	if s := app.JiraCreateFailure(denied, "nk approve"); s != "jira: o claude negou a chamada: hook  — se for o seu hook, rode:  nk approve  e aperte J de novo" {
		t.Fatalf("approve hint: %q", s)
	}
	if s := app.JiraCreateFailure(denied, ""); s != "jira: o claude negou a chamada: hook  (log: ~/.cache/nekuroddo/jira-last.jsonl)" {
		t.Fatalf("no hint: the log: %q", s)
	}
	if s := app.JiraCreateFailure(errors.New("boom"), "nk approve"); !strings.Contains(s, "jira: boom  (log:") {
		t.Fatalf("other errors: the log: %q", s)
	}
	if s := app.JiraFetchText(jira.Cache{CostUSD: 0.12, Issues: make([]jira.Issue, 1)}, nil); s != "jira: 1 issues (custou $0.12)" {
		t.Fatalf("fetched: %q", s)
	}
	if s := app.JiraFetchText(jira.Cache{}, errors.New("sem rede")); s != "jira: sem rede" {
		t.Fatalf("fetch error: %q", s)
	}
}

func TestJiraShouldFetchAndStale(t *testing.T) {
	now := apptest.Epoch
	cfg := jira.Config{Enabled: true, CacheMinutes: 30}
	fresh := jira.Cache{FetchedAt: now.Add(-10 * time.Minute)}
	stale := jira.Cache{FetchedAt: now.Add(-31 * time.Minute)}
	for _, c := range []struct {
		name           string
		cfg            jira.Config
		c              jira.Cache
		loading, force bool
		want           bool
	}{
		{"off", jira.Config{}, stale, false, true, false},
		{"loading", cfg, stale, true, true, false},
		{"fresh", cfg, fresh, false, false, false},
		{"fresh forced", cfg, fresh, false, true, true},
		{"stale", cfg, stale, false, false, true},
	} {
		if got := app.JiraShouldFetch(c.cfg, c.c, c.loading, c.force, now); got != c.want {
			t.Errorf("%s: %v", c.name, got)
		}
	}
	recent := jira.Issue{Updated: now.Add(-time.Hour).Format("2006-01-02T15:04:05.000-0700")}
	old := jira.Issue{Updated: now.Add(-91 * 24 * time.Hour).Format("2006-01-02T15:04:05.000-0700")}
	if app.IssueStale(recent, now) || !app.IssueStale(old, now) {
		t.Fatal("90 days")
	}
}

func TestFetchJiraFreshnessAndGuard(t *testing.T) {
	a, f := tjApp(t)
	clk := f.Clock
	evs := tjEvents(t, a)
	f.Jira.Cache = jira.Cache{FetchedAt: clk.Now(), CostUSD: 0.1, Issues: []jira.Issue{{Key: "PROJ-8"}}}
	c, ran, err := a.FetchJira(false)
	if err != nil || !ran || len(c.Issues) != 1 {
		t.Fatalf("empty cache: fetch: %v %v", ran, err)
	}
	if e, ok := tjHas(evs(), app.EvJiraChanged); !ok || e.Data.(app.JiraChanged).Issues != 1 {
		t.Fatalf("jira.changed: %+v", e)
	}
	if got := a.JiraCache(); len(got.Issues) != 1 {
		t.Fatal("kept")
	}
	// fresh: not refetched unless forced
	if _, ran, _ := a.FetchJira(false); ran || f.Jira.Count("Fetch") != 1 {
		t.Fatal("a fresh cache is not refetched")
	}
	clk.Advance(31 * time.Minute)
	if _, ran, _ := a.FetchJira(false); !ran {
		t.Fatal("stale: refetched")
	}
	if _, ran, _ := a.FetchJira(true); !ran || f.Jira.Count("Fetch") != 3 {
		t.Fatal("forced")
	}
	if c := f.Jira.CallsTo("Fetch"); c[0].Args[0].(jira.Config).DefaultProject != "PROJ" {
		t.Fatal("fetches with the configured jira")
	}
	// an error keeps the cache and the error
	f.Jira.Errs = map[string]error{"Fetch": errors.New("sem rede")}
	if _, ran, err := a.FetchJira(true); !ran || err == nil {
		t.Fatal("error returned")
	}
	if a.JiraErr() == nil || len(a.JiraCache().Issues) != 1 || a.JiraLoading() {
		t.Fatal("error kept, cache kept, loading over")
	}
	if e, ok := tjHas(evs(), app.EvJiraChanged); !ok || e.Data.(app.JiraChanged).Err != "sem rede" {
		t.Fatalf("jira.changed with the error: %+v", e)
	}
}

// slowJira blocks Fetch until released, to see the loading guard.
type slowJira struct {
	apptest.FakeJira
	started chan struct{}
	release chan struct{}
}

func (j *slowJira) Fetch(cfg jira.Config) (jira.Cache, error) {
	j.started <- struct{}{}
	<-j.release
	return j.FakeJira.Fetch(cfg)
}

func TestFetchJiraOneAtATime(t *testing.T) {
	t.Setenv("XDG_CACHE_HOME", t.TempDir())
	cfg := config.Default()
	cfg.Jira.Enabled = true
	f := apptest.New()
	sj := &slowJira{started: make(chan struct{}), release: make(chan struct{})}
	p := f.Ports()
	p.Jira = sj
	a := app.New(cfg, p)
	defer a.Close()
	done := make(chan bool)
	go func() { _, ran, _ := a.FetchJira(true); done <- ran }()
	<-sj.started
	if !a.JiraLoading() {
		t.Fatal("loading while fetching")
	}
	if _, ran, err := a.FetchJira(true); ran || err != nil {
		t.Fatal("one fetch at a time")
	}
	close(sj.release)
	if !<-done || a.JiraLoading() {
		t.Fatal("first fetch ran and finished")
	}
}

func TestJiraCacheLoadsFromDisk(t *testing.T) {
	dir := t.TempDir()
	t.Setenv("XDG_CACHE_HOME", dir)
	c := jira.Cache{FetchedAt: apptest.Epoch, Issues: []jira.Issue{{Key: "D-1"}}}
	b, _ := json.Marshal(c)
	_ = os.MkdirAll(filepath.Join(dir, "nekuroddo"), 0o755)
	if err := os.WriteFile(filepath.Join(dir, "nekuroddo", "jira.json"), b, 0o600); err != nil {
		t.Fatal(err)
	}
	a := app.New(config.Default(), apptest.Ports())
	defer a.Close()
	if got := a.JiraCache(); len(got.Issues) != 1 || got.Issues[0].Key != "D-1" {
		t.Fatalf("starts from the cache on disk (opening costs nothing): %+v", got)
	}
}

func TestCreateIssue(t *testing.T) {
	a, f := tjApp(t)
	f.Jira.Key, f.Jira.Cost = "VIS-9", 0.25
	task := todo.Task{ID: 1, Title: "escrever testes", Desc: "d"}
	key, cost, err := a.CreateIssue(task, "VIS", "Bug")
	if err != nil || key != "VIS-9" || cost != 0.25 {
		t.Fatalf("created: %q %v %v", key, cost, err)
	}
	c := f.Jira.CallsTo("Create")
	n := c[0].Args[1].(jira.NewIssue)
	if n.Project != "VIS" || n.Type != "Bug" || n.Summary != "escrever testes" || n.Description != app.JiraDescription(task) ||
		len(n.Labels) != 1 || n.Labels[0] != "nekuroddo" {
		t.Fatalf("issue: %+v", n)
	}
}

func TestLinkIssueReloads(t *testing.T) {
	a, f := tjApp(t)
	f.Todo.Errs = map[string]error{"LinkJira": errors.New("ignored")}
	g, err := a.LinkIssue(1, "VIS-9")
	if err != nil || len(g) != 1 {
		t.Fatal("reloaded")
	}
	if c := f.Todo.CallsTo("LinkJira"); len(c) != 1 || c[0].Args[0] != 1 || c[0].Args[1] != "VIS-9" {
		t.Fatalf("LinkJira: %v", c)
	}
}

func TestAskCreateIssueThenLinksAndRefreshes(t *testing.T) {
	a, f := tjApp(t)
	f.Jira.Key, f.Jira.Cost = "VIS-9", 0.3
	f.Jira.Cache = jira.Cache{FetchedAt: f.Clock.Now(), Issues: []jira.Issue{{Key: "VIS-9"}}}
	linked := todo.Task{ID: 2, Refs: []todo.Ref{{Kind: "jira", Value: "PROJ-7"}}}
	if res := a.AskCreateIssue(linked, ""); !res.Err || res.Toast != "#2 já tem issue: PROJ-7" {
		t.Fatalf("already linked: %+v", res)
	}
	res := a.AskCreateIssue(todo.Task{ID: 1, Title: "escrever testes"}, " vis / Bug ")
	if res.Pending == nil || res.Pending.Question != app.CreateIssueQuestion("VIS", "Bug", "escrever testes") {
		t.Fatalf("asks first: %+v", res)
	}
	if f.Jira.Count("Create") != 0 {
		t.Fatal("nothing written before yes")
	}
	got, err := a.Confirm(res.Pending.Token)
	if err != nil || got.Err || got.Toast != "VIS-9 criada e ligada ao #1 ($0.30)" {
		t.Fatalf("created: %+v %v", got, err)
	}
	if c := f.Todo.CallsTo("LinkJira"); len(c) != 1 || c[0].Args[1] != "VIS-9" {
		t.Fatal("linked to the task")
	}
	if f.Jira.Count("Fetch") != 1 {
		t.Fatal("jira cache refreshed")
	}
	// empty target: the default project/type
	res = a.AskCreateIssue(todo.Task{ID: 1, Title: "t"}, "")
	if !strings.Contains(res.Pending.Question, "PROJ/Task") {
		t.Fatalf("default target: %q", res.Pending.Question)
	}
	// a denied write: the approve hint, nothing linked
	f.Jira.Errs = map[string]error{"Create": jira.DeniedError{Reason: "hook"}}
	got, err = a.Confirm(res.Pending.Token)
	if err == nil || !got.Err || got.Toast != app.JiraCreateFailure(jira.DeniedError{Reason: "hook"}, "") {
		t.Fatalf("denied: %+v", got)
	}
	if f.Todo.Count("LinkJira") != 1 {
		t.Fatal("not linked")
	}
}

func TestOpenURL(t *testing.T) {
	a, f := tjApp(t)
	if err := a.OpenURL("https://x/PROJ-8"); err != nil {
		t.Fatal(err)
	}
	if u := f.Open.URLs(); len(u) != 1 || u[0] != "https://x/PROJ-8" {
		t.Fatalf("opened: %v", u)
	}
	f.Open.Err = errors.New("sem xdg-open")
	if a.OpenURL("x") == nil {
		t.Fatal("error returned")
	}
	if a.OpenURL("") == nil {
		t.Fatal("no url")
	}
}

func TestRegisterTodoJiraCommands(t *testing.T) {
	a, f := tjApp(t)
	if err := a.RegisterTodoJira(); err != nil {
		t.Fatal(err)
	}
	if err := a.RegisterTodoJira(); err == nil {
		t.Fatal("twice: duplicate")
	}
	names := map[string]bool{}
	for _, c := range a.Registry().List(app.Context{TodoID: 1, IssueKey: "PROJ-8"}) {
		names[c.Name] = true
	}
	for _, n := range []string{"todo.start", "todo.done", "jira.toggle", "jira.fetch", "jira.open_url"} {
		if !names[n] {
			t.Errorf("%s not registered", n)
		}
	}
	if got := a.Registry().List(app.Context{}); len(got) != 1 || got[0].Name != "jira.fetch" {
		t.Fatalf("with nothing selected only jira.fetch: %v", got)
	}
	f.Jira.Cache = jira.Cache{FetchedAt: f.Clock.Now(), CostUSD: 0.1,
		Issues: []jira.Issue{{Key: "PROJ-8", Summary: "solta", URL: "https://x/PROJ-8"}}}
	if _, err := a.LoadTodos(); err != nil {
		t.Fatal(err)
	}
	run := func(name string, ctx app.Context, args app.Args) (app.Result, error) {
		t.Helper()
		return a.Run(ctx, name, args)
	}

	// jira.fetch
	res, err := run("jira.fetch", app.Context{}, nil)
	if err != nil || res.Toast != "jira: 1 issues (custou $0.10)" {
		t.Fatalf("fetch: %+v %v", res, err)
	}
	f.Jira.Errs = map[string]error{"Fetch": errors.New("sem rede")}
	if res, _ := run("jira.fetch", app.Context{}, nil); !res.Err || res.Toast != "jira: sem rede" {
		t.Fatalf("fetch error: %+v", res)
	}
	f.Jira.Errs = nil

	// todo.start / todo.done
	if res, err := run("todo.start", app.Context{TodoID: 1}, nil); err != nil || res.Data.(app.TodoList).Groups == nil {
		t.Fatalf("start: %+v %v", res, err)
	}
	if _, err := run("todo.start", app.Context{TodoID: 99}, nil); err == nil {
		t.Fatal("unknown task")
	}
	res, err = run("todo.done", app.Context{TodoID: 1}, nil)
	if err != nil || res.Pending == nil {
		t.Fatalf("done asks: %+v %v", res, err)
	}
	if _, err := run("todo.done", app.Context{TodoID: 99}, nil); err == nil {
		t.Fatal("unknown task")
	}

	// jira.toggle
	res, err = run("jira.toggle", app.Context{IssueKey: "PROJ-8"}, nil)
	if err != nil || res.Err || f.Todo.Count("AddFromJira") != 1 {
		t.Fatalf("issue → todo: %+v %v", res, err)
	}
	if _, err := run("jira.toggle", app.Context{IssueKey: "X-1"}, nil); err == nil {
		t.Fatal("unknown issue")
	}
	res, _ = run("jira.toggle", app.Context{TodoID: 1}, app.Args{"target": "vis/Bug"})
	if res.Pending == nil || !strings.Contains(res.Pending.Question, "VIS/Bug") {
		t.Fatalf("task → jira asks: %+v", res)
	}
	if res, _ := run("jira.toggle", app.Context{TodoID: 2}, nil); !res.Err {
		t.Fatal("linked task: error")
	}
	if _, err := run("jira.toggle", app.Context{TodoID: 99}, nil); err == nil {
		t.Fatal("unknown task")
	}

	// jira.open_url
	if _, err := run("jira.open_url", app.Context{IssueKey: "PROJ-8"}, nil); err != nil {
		t.Fatal(err)
	}
	if u := f.Open.URLs(); len(u) != 1 || u[0] != "https://x/PROJ-8" {
		t.Fatalf("opened: %v", u)
	}
	if _, err := run("jira.open_url", app.Context{IssueKey: "X-1"}, nil); err == nil {
		t.Fatal("unknown issue")
	}
}

func TestJiraCommandsWhenOff(t *testing.T) {
	t.Setenv("XDG_CACHE_HOME", t.TempDir())
	a := app.New(config.Default(), apptest.Ports())
	defer a.Close()
	if err := a.RegisterTodoJira(); err != nil {
		t.Fatal(err)
	}
	for _, c := range []struct {
		name string
		ctx  app.Context
	}{{"jira.fetch", app.Context{}}, {"jira.toggle", app.Context{TodoID: 1}}} {
		if _, err := a.Run(c.ctx, c.name, nil); !errors.Is(err, app.ErrJiraOff) {
			t.Errorf("%s with jira off: %v", c.name, err)
		}
	}
}
