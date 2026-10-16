package app

// Slice "jira/todo" (WP-2.5): Jira issues through the JiraClient port (a
// headless claude), and the bridge between them and the todo list.

import (
	"errors"
	"fmt"
	"strings"
	"time"

	"github.com/aqilputi/nekuroddo/internal/core"
	"github.com/aqilputi/nekuroddo/internal/jira"
	"github.com/aqilputi/nekuroddo/internal/todo"
)

// EvJiraChanged is published after every Jira fetch.
const EvJiraChanged = "jira.changed"

// JiraChanged is the payload of jira.changed; clients read the issues with
// App.JiraCache.
type JiraChanged struct {
	FetchedAt time.Time `json:"fetched_at"`
	Issues    int       `json:"issues"`
	CostUSD   float64   `json:"cost_usd"`
	Err       string    `json:"err,omitempty"`
}

// ErrJiraOff is any Jira command while [jira] is not configured.
var ErrJiraOff = errors.New("jira desligado: configure [jira] no config.toml")

// staleIssue is how long an issue may sit untouched before the todos tab
// hides it (unless showing everything).
const staleIssue = 90 * 24 * time.Hour

// IssueStale reports whether is was last updated too long before now.
func IssueStale(is jira.Issue, now time.Time) bool { return now.Sub(is.UpdatedAt()) > staleIssue }

// JiraShouldFetch is the fetch guard: Jira on, no fetch running, and the
// cache older than cache_minutes (or force). A fetch costs ~$0.1.
func JiraShouldFetch(cfg jira.Config, c jira.Cache, loading, force bool, now time.Time) bool {
	if !cfg.Enabled || loading {
		return false
	}
	return force || now.Sub(c.FetchedAt) >= time.Duration(cfg.CacheMinutes)*time.Minute
}

// JiraFetchText is the toast after a fetch.
func JiraFetchText(c jira.Cache, err error) string {
	if err != nil {
		return "jira: " + err.Error()
	}
	return fmt.Sprintf("jira: %d issues (custou $%.2f)", len(c.Issues), c.CostUSD)
}

// JiraCache is the last Jira list; the first call reads the one saved on
// disk, so opening the todos tab costs nothing.
func (a *App) JiraCache() jira.Cache {
	s := a.tj()
	s.mu.Lock()
	defer s.mu.Unlock()
	a.readCacheLocked(s)
	return s.cache
}

func (a *App) readCacheLocked(s *todoJiraState) {
	if !s.cacheRead {
		s.cacheRead = true
		s.cache = jira.LoadCache()
	}
}

// JiraLoading reports whether a fetch is running.
func (a *App) JiraLoading() bool {
	s := a.tj()
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.loading
}

// JiraErr is the last fetch's error.
func (a *App) JiraErr() error {
	s := a.tj()
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.jiraErr
}

// FetchJira refreshes the Jira list when JiraShouldFetch says so (ran
// reports whether it did) and publishes jira.changed. A failure keeps the
// last list.
func (a *App) FetchJira(force bool) (c jira.Cache, ran bool, err error) {
	run := a.BeginJiraFetch(force)
	if run == nil {
		return a.JiraCache(), false, nil
	}
	c, err = run()
	return c, true, err
}

// BeginJiraFetch is FetchJira in two halves, for a client with a UI thread:
// the guard runs now and, when it says fetch, JiraLoading turns true at
// once; run (nil when there is nothing to fetch) then does the fetch off
// that thread, keeps the result and publishes jira.changed.
func (a *App) BeginJiraFetch(force bool) (run func() (jira.Cache, error)) {
	cfg := a.cfg.Jira
	s := a.tj()
	s.mu.Lock()
	defer s.mu.Unlock()
	a.readCacheLocked(s)
	if !JiraShouldFetch(cfg, s.cache, s.loading, force, a.clock.Now()) {
		return nil
	}
	s.loading = true
	return func() (jira.Cache, error) { return a.endJiraFetch(cfg) }
}

func (a *App) endJiraFetch(cfg jira.Config) (c jira.Cache, err error) {
	s := a.tj()
	got, err := a.ports.Jira.Fetch(cfg)
	s.mu.Lock()
	s.loading, s.jiraErr = false, err
	if err == nil {
		s.cache = got
	}
	c = s.cache
	s.mu.Unlock()
	ev := JiraChanged{FetchedAt: c.FetchedAt, Issues: len(c.Issues), CostUSD: c.CostUSD}
	if err != nil {
		ev.Err = err.Error()
	}
	a.Publish(EvJiraChanged, ev)
	return c, err
}

func issueByKey(c jira.Cache, key string) *jira.Issue {
	for i := range c.Issues {
		if c.Issues[i].Key == key {
			return &c.Issues[i]
		}
	}
	return nil
}

// JiraDescription is the issue body built from a task.
func JiraDescription(t todo.Task) string {
	var b strings.Builder
	if t.Desc != "" {
		b.WriteString(t.Desc + "\n\n")
	}
	for _, r := range t.Refs {
		fmt.Fprintf(&b, "- %s: %s %s\n", r.Kind, r.Value, r.URL)
	}
	for _, l := range t.Links {
		fmt.Fprintf(&b, "- %s\n", l)
	}
	for _, n := range t.Notes {
		fmt.Fprintf(&b, "\n> %s — %s\n", n.At[:min(10, len(n.At))], n.Text)
	}
	b.WriteString("\n_criada pelo nekuroddo a partir do todo #" + fmt.Sprint(t.ID) + "_")
	return b.String()
}

// JiraDefaultTarget is the "PROJ/Type" a new issue goes to by default.
func JiraDefaultTarget(cfg jira.Config) string { return cfg.DefaultProject + "/" + cfg.DefaultType }

// JiraTarget parses "proj/Type" (the type defaults to cfg's).
func JiraTarget(cfg jira.Config, val string) (proj, typ string) {
	proj, typ, _ = strings.Cut(val, "/")
	proj = strings.ToUpper(strings.TrimSpace(proj))
	typ = strings.TrimSpace(typ)
	if typ == "" {
		typ = cfg.DefaultType
	}
	return proj, typ
}

// TaskHasIssue is the error for sending a task that already has an issue.
func TaskHasIssue(t todo.Task) error {
	if k := t.JiraKey(); k != "" {
		return fmt.Errorf("#%d já tem issue: %s", t.ID, k)
	}
	return nil
}

// CreateIssueQuestion is the confirmation before writing to Jira.
func CreateIssueQuestion(proj, typ, title string) string {
	return fmt.Sprintf("criar no Jira %s/%s, atribuída a você:\n\"%s\"\n(escreve no Jira)", proj, typ, core.Truncate(title, 70))
}

// JiraCreatedText is the toast after an issue was created and linked.
func JiraCreatedText(key string, task int, cost float64) string {
	return fmt.Sprintf("%s criada e ligada ao #%d ($%.2f)", key, task, cost)
}

// JiraCreateFailure explains a failed create: a denied write shows the
// approve hint (when configured); anything else points to the run's log.
func JiraCreateFailure(err error, approveHint string) string {
	var denied jira.DeniedError
	if errors.As(err, &denied) && approveHint != "" {
		return "jira: " + denied.Error() + "  — se for o seu hook, rode:  " + approveHint + "  e aperte J de novo"
	}
	return "jira: " + err.Error() + "  (log: ~/.cache/nekuroddo/jira-last.jsonl)"
}

// CreateIssue writes t to Jira as proj/typ, assigned to the user. It only
// creates: LinkIssue ties it to the task.
func (a *App) CreateIssue(t todo.Task, proj, typ string) (key string, cost float64, err error) {
	n := jira.NewIssue{Project: proj, Type: typ, Summary: t.Title, Description: JiraDescription(t), Labels: []string{"nekuroddo"}}
	return a.ports.Jira.Create(a.cfg.Jira, n)
}

// LinkIssue records key on task id and reloads the list.
func (a *App) LinkIssue(id int, key string) ([]todo.Group, error) {
	_ = a.ports.Todo.LinkJira(id, key)
	return a.LoadTodos()
}

// AskCreateIssue asks before sending t to Jira at target ("PROJ/Type", empty
// = the default). Yes creates it, links it to the task and refreshes the
// Jira list.
func (a *App) AskCreateIssue(t todo.Task, target string) Result {
	if err := TaskHasIssue(t); err != nil {
		return Fail(err)
	}
	if strings.TrimSpace(target) == "" {
		target = JiraDefaultTarget(a.cfg.Jira)
	}
	proj, typ := JiraTarget(a.cfg.Jira, target)
	return a.Ask(CreateIssueQuestion(proj, typ, t.Title), func() (Result, error) {
		key, cost, err := a.CreateIssue(t, proj, typ)
		if err != nil {
			return Result{Toast: JiraCreateFailure(err, a.cfg.Jira.ApproveHint), Err: true}, err
		}
		g, lerr := a.LinkIssue(t.ID, key)
		_, _, _ = a.FetchJira(true)
		return Result{Toast: JiraCreatedText(key, t.ID, cost), Data: listOf(g, lerr)}, nil
	})
}

// RegisterTodoJira adds the slice's commands to the registry: todo.start,
// todo.done, jira.toggle, jira.fetch and jira.open_url.
func (a *App) RegisterTodoJira() error {
	onTodo := WithAvailable(func(c Context) bool { return c.TodoID != 0 })
	onIssue := WithAvailable(func(c Context) bool { return c.IssueKey != "" })
	onRow := WithAvailable(func(c Context) bool { return c.TodoID != 0 || c.IssueKey != "" })
	cmds := []struct {
		c   core.Command
		run Runner
		opt []Option
	}{
		{core.Command{Name: "todo.start", Title: "começa a tarefa", Keys: []string{"s"}, Mode: core.ModeNormal},
			a.runTodoStart, []Option{onTodo}},
		{core.Command{Name: "todo.done", Title: "marca a tarefa como done", Keys: []string{"D"}, Mode: core.ModeNormal, Confirm: true},
			a.runTodoDone, []Option{onTodo}},
		{core.Command{Name: "jira.toggle", Title: "todo↔jira: issue vira tarefa, tarefa vira issue", Keys: []string{"J"},
			Mode: core.ModeNormal, Confirm: true,
			Args: []core.Arg{{Name: "target", Type: core.ArgString, Prompt: "projeto/tipo"}}},
			a.runJiraToggle, []Option{onRow}},
		{core.Command{Name: "jira.fetch", Title: "busca as issues no Jira (via claude, ~$0.1)", Keys: []string{"F"}, Mode: core.ModeNormal},
			a.runJiraFetch, nil},
		{core.Command{Name: "jira.open_url", Title: "abre a issue no browser", Keys: []string{"o"}, Mode: core.ModeNormal},
			a.runJiraOpenURL, []Option{onIssue}},
	}
	for _, c := range cmds {
		if err := a.reg.Register(c.c, c.run, c.opt...); err != nil {
			return err
		}
	}
	return nil
}

func (a *App) todoOf(ctx Context) (todo.Task, error) {
	if t := FindTodo(a.Todos().Groups, ctx.TodoID); t != nil {
		return *t, nil
	}
	return todo.Task{}, fmt.Errorf("todo #%d não encontrado", ctx.TodoID)
}

func (a *App) issueOf(ctx Context) (jira.Issue, error) {
	if is := issueByKey(a.JiraCache(), ctx.IssueKey); is != nil {
		return *is, nil
	}
	return jira.Issue{}, fmt.Errorf("issue %s não está no cache do jira", ctx.IssueKey)
}

func (a *App) runTodoStart(ctx Context, _ Args) (Result, error) {
	t, err := a.todoOf(ctx)
	if err != nil {
		return Result{}, err
	}
	return Result{Data: listOf(a.StartTodo(t.ID))}, nil
}

func (a *App) runTodoDone(ctx Context, _ Args) (Result, error) {
	t, err := a.todoOf(ctx)
	if err != nil {
		return Result{}, err
	}
	return a.AskDoneTodo(t), nil
}

func (a *App) runJiraToggle(ctx Context, args Args) (Result, error) {
	if !a.cfg.Jira.Enabled {
		return Result{}, ErrJiraOff
	}
	if ctx.IssueKey != "" {
		is, err := a.issueOf(ctx)
		if err != nil {
			return Result{}, err
		}
		return a.IssueToTodo(a.Todos().Groups, is.Key, is.Summary), nil
	}
	t, err := a.todoOf(ctx)
	if err != nil {
		return Result{}, err
	}
	return a.AskCreateIssue(t, args.String("target")), nil
}

func (a *App) runJiraFetch(Context, Args) (Result, error) {
	if !a.cfg.Jira.Enabled {
		return Result{}, ErrJiraOff
	}
	c, ran, err := a.FetchJira(true)
	switch {
	case err != nil:
		return Result{Toast: JiraFetchText(c, err), Err: true}, nil
	case !ran:
		return Info("jira: já buscando…"), nil
	}
	return Info(JiraFetchText(c, nil)), nil
}

func (a *App) runJiraOpenURL(ctx Context, _ Args) (Result, error) {
	is, err := a.issueOf(ctx)
	if err != nil {
		return Result{}, err
	}
	return Result{}, a.OpenURL(is.URL)
}

// OpenURL opens u in the browser.
func (a *App) OpenURL(u string) error {
	if u == "" {
		return errors.New("sem url")
	}
	return a.ports.Open.OpenURL(u)
}
