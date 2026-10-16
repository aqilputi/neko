package demo

import (
	"errors"
	"fmt"
	"sort"
	"time"

	"github.com/aqilputi/nekuroddo/internal/claude"
	"github.com/aqilputi/nekuroddo/internal/events"
	"github.com/aqilputi/nekuroddo/internal/jira"
	"github.com/aqilputi/nekuroddo/internal/todo"
)

// ErrUnknown is what the demo's claude and todo answer for an id they do
// not have.
var ErrUnknown = errors.New("demo: id desconhecido")

// ---- ClaudeCLI ----

type claudePort struct{ s *Scenario }

// agentState is the `claude agents` state of a store state.
var agentState = map[string]string{"busy": "running", "blocked": "blocked", "done": "done", "stopped": "stopped",
	"idle": "idle"}

func (p claudePort) ListAgents() ([]claude.Agent, error) {
	p.s.mu.Lock()
	defer p.s.mu.Unlock()
	var out []claude.Agent
	for _, x := range p.s.sessionsLocked() {
		if x.ShortID == "" {
			continue
		}
		out = append(out, claude.Agent{ID: x.ShortID, PID: 0, Cwd: x.Cwd, Kind: "background",
			StartedAt: x.Started.UnixMilli(), SessionID: x.SessionID, Name: x.Name, State: agentState[x.State]})
	}
	return out, nil
}

func (p claudePort) Dispatch(dir, name, prompt string, _ ...string) (string, error) {
	p.s.mu.Lock()
	defer p.s.mu.Unlock()
	p.s.nextID++
	short := fmt.Sprintf("dmnew%03d", p.s.nextID)
	p.s.extras = append(p.s.extras, extra{short: short, name: name, prompt: prompt, dir: dir, at: p.s.clock.Now()})
	return short, nil
}

func (p claudePort) ResumeInBackground(_, sessionID string) (string, error) {
	p.s.mu.Lock()
	defer p.s.mu.Unlock()
	if r, ok := p.s.resumed[sessionID]; ok {
		return r.short, nil
	}
	if p.s.lookupLocked(sessionID).SessionID == "" {
		return "", fmt.Errorf("%w: %s", ErrUnknown, sessionID)
	}
	p.s.nextID++
	short := fmt.Sprintf("dmres%03d", p.s.nextID)
	p.s.resumed[sessionID] = extra{short: short, at: p.s.clock.Now()}
	return short, nil
}

func (p claudePort) Stop(id string) error {
	p.s.mu.Lock()
	defer p.s.mu.Unlock()
	x, ok := p.s.byShortLocked(id)
	if !ok {
		return fmt.Errorf("%w: %s", ErrUnknown, id)
	}
	p.s.stopped[x.SessionID] = true
	return nil
}

func (p claudePort) Remove(id string) error {
	p.s.mu.Lock()
	defer p.s.mu.Unlock()
	x, ok := p.s.byShortLocked(id)
	if !ok {
		return fmt.Errorf("%w: %s", ErrUnknown, id)
	}
	p.s.removed[x.SessionID] = true
	return nil
}

// AttachArgv is only data: the demo's Panes never runs it.
func (claudePort) AttachArgv(id string) []string { return []string{"claude", "attach", id} }

func (p claudePort) FetchUsage() (claude.UsageReport, error) {
	pct := UsagePct(Step(p.s.Elapsed()))
	lim := []claude.Limit{
		{Name: "Current session", Pct: pct, Resets: "5:00pm"},
		{Name: "Current week (all models)", Pct: 34, Resets: "Oct 20, 9:00am"},
		{Name: "Current week (Opus)", Pct: 12, Resets: "Oct 20, 9:00am"},
	}
	raw := ""
	for _, l := range lim {
		raw += fmt.Sprintf("%s: %d%% used · resets %s\n", l.Name, l.Pct, l.Resets)
	}
	return claude.UsageReport{Limits: lim, Raw: raw, At: p.s.clock.Now()}, nil
}

// ---- EventLog ----

type eventsPort struct{ s *Scenario }

// Poll hands out what the script did since the last poll: a permission
// notification each time the gateway blocks, and the ✉ messages of
// #deploy-rc (as Claude Code's hook would record them). A long jump of the
// clock yields only the latest maxBurst events.
func (p eventsPort) Poll() []events.Event {
	p.s.mu.Lock()
	defer p.s.mu.Unlock()
	from, to := p.s.lastPoll, p.s.Elapsed()
	p.s.lastPoll = to
	var out []events.Event
	for _, a := range AlertsBetween(from, to) {
		out = append(out, events.Event{At: p.s.t0.Add(a.At), Name: "Notification", SessionID: a.SessionID,
			Type: "permission_prompt", Message: a.Message})
	}
	for _, m := range MailsBetween(from, to) {
		out = append(out, events.Event{At: p.s.t0.Add(m.At), Name: "UserPromptSubmit", SessionID: m.To,
			Type: "peer", From: m.FromName, Message: m.Text})
	}
	sort.SliceStable(out, func(i, j int) bool { return out[i].At.Before(out[j].At) })
	return keepLast(out, maxBurst)
}

// ---- JiraClient ----

type jiraPort struct{ s *Scenario }

func issueURL(key string) string { return "https://example.invalid/browse/" + key }

func seedIssues() []jira.Issue {
	mk := func(key, summary, status, cat, typ, prio string) jira.Issue {
		return jira.Issue{Key: key, Summary: summary, Status: status, StatusCat: cat, Type: typ, Priority: prio,
			Updated: "2026-10-15T10:00:00.000-0300", URL: issueURL(key)}
	}
	return []jira.Issue{
		mk("DEMO-101", "rate limit por tenant no gateway", "Em andamento", "indeterminate", "Story", "High"),
		mk("DEMO-102", "migrar a coluna user_ref", "Bloqueado", "indeterminate", "Task", "Highest"),
		mk("DEMO-103", "alerta de memória do worker", "A fazer", "new", "Bug", "Medium"),
		mk("DEMO-104", "relatório de uso do Q3", "Em revisão", "indeterminate", "Task", "Low"),
	}
}

// Cached is the demo's own issues: never the user's cache on disk.
func (p jiraPort) Cached() jira.Cache {
	c, _ := p.Fetch(jira.Config{})
	return c
}

func (p jiraPort) Fetch(jira.Config) (jira.Cache, error) {
	p.s.mu.Lock()
	defer p.s.mu.Unlock()
	return jira.Cache{FetchedAt: p.s.clock.Now(), Issues: append([]jira.Issue(nil), p.s.issues...)}, nil
}

func (p jiraPort) Create(_ jira.Config, n jira.NewIssue) (string, float64, error) {
	p.s.mu.Lock()
	defer p.s.mu.Unlock()
	key := fmt.Sprintf("DEMO-%d", 200+len(p.s.issues)-len(seedIssues())+1)
	p.s.issues = append(p.s.issues, jira.Issue{Key: key, Summary: n.Summary, Status: "A fazer", StatusCat: "new",
		Type: "Task", Priority: "Medium", URL: issueURL(key),
		Updated: p.s.clock.Now().Format("2006-01-02T15:04:05.000-0700")})
	return key, 0.01, nil
}

// ---- TodoClient ----

type todoPort struct{ s *Scenario }

func seedTodos() []todo.Group {
	p := func(n int) *int { return &n }
	who := "Silmar"
	return []todo.Group{
		{Group: "hoje", Tasks: []todo.Task{
			{ID: 1, Title: "revisar o PR #418 do relatório", Status: "todo", Initiative: "relatórios",
				Contexts: []string{"@visio"}, Priority: p(2),
				Refs: []todo.Ref{{Kind: "jira", Value: "DEMO-104", URL: issueURL("DEMO-104")}}},
			{ID: 2, Title: "aprovar a migração 0042", Status: "doing", Initiative: "gateway",
				Contexts: []string{"@visio"}, Priority: p(1),
				Refs: []todo.Ref{{Kind: "jira", Value: "DEMO-102", URL: issueURL("DEMO-102")}}},
			{ID: 3, Title: "responder sobre o rc", Status: "waiting", WaitingOn: &who, Contexts: []string{"@visio"}},
		}},
		{Group: "amanhã", Tasks: []todo.Task{
			{ID: 4, Title: "atualizar os dotfiles do notebook", Status: "todo", Contexts: []string{"@casa"}},
			{ID: 5, Title: "escrever um post sobre o indexador", Status: "todo", Initiative: "oss"},
		}},
	}
}

func copyTodos(gs []todo.Group) []todo.Group {
	out := make([]todo.Group, 0, len(gs))
	for _, g := range gs {
		c := todo.Group{Group: g.Group, Tasks: make([]todo.Task, 0, len(g.Tasks))}
		for _, t := range g.Tasks {
			t.Refs = append([]todo.Ref(nil), t.Refs...)
			t.Notes = append([]todo.Note(nil), t.Notes...)
			t.Contexts = append([]string(nil), t.Contexts...)
			c.Tasks = append(c.Tasks, t)
		}
		out = append(out, c)
	}
	return out
}

func (todoPort) Available() bool { return true }

func (p todoPort) List() ([]todo.Group, error) {
	p.s.mu.Lock()
	defer p.s.mu.Unlock()
	return copyTodos(p.s.todos), nil
}

// edit runs f on task id (under the lock); ErrUnknown if there is none.
func (p todoPort) edit(id int, f func(g, i int)) error {
	p.s.mu.Lock()
	defer p.s.mu.Unlock()
	for g := range p.s.todos {
		for i := range p.s.todos[g].Tasks {
			if p.s.todos[g].Tasks[i].ID == id {
				f(g, i)
				return nil
			}
		}
	}
	return fmt.Errorf("%w: tarefa %d", ErrUnknown, id)
}

func (p todoPort) Start(id int) error {
	return p.edit(id, func(g, i int) { p.s.todos[g].Tasks[i].Status = "doing" })
}

// Done drops the task from the list (the todo skill lists only open ones).
func (p todoPort) Done(id int) error {
	return p.edit(id, func(g, i int) {
		ts := p.s.todos[g].Tasks
		p.s.todos[g].Tasks = append(ts[:i:i], ts[i+1:]...)
	})
}

func (p todoPort) LinkJira(id int, key string) error {
	return p.edit(id, func(g, i int) {
		t := &p.s.todos[g].Tasks[i]
		t.Refs = append(t.Refs, todo.Ref{Kind: "jira", Value: key, URL: issueURL(key)})
	})
}

func (p todoPort) AddNote(id int, text string) error {
	return p.edit(id, func(g, i int) {
		t := &p.s.todos[g].Tasks[i]
		t.Notes = append(t.Notes, todo.Note{At: p.s.clock.Now().Format(time.RFC3339), Text: text})
	})
}

func (p todoPort) AddFromJira(key, summary string) error {
	p.s.mu.Lock()
	defer p.s.mu.Unlock()
	p.s.todos[0].Tasks = append(p.s.todos[0].Tasks, todo.Task{ID: p.s.nextTD, Title: summary, Status: "todo",
		Refs: []todo.Ref{{Kind: "jira", Value: key, URL: issueURL(key)}}})
	p.s.nextTD++
	return nil
}
