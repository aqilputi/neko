package app

// Slice "jira/todo" (WP-2.5): the user's todo list, through the TodoClient
// port. Jira lives next door, in jira.go; both share todoJiraState.

import (
	"errors"
	"fmt"
	"sync"

	"github.com/aqilputi/nekuroddo/internal/config"
	"github.com/aqilputi/nekuroddo/internal/core"
	"github.com/aqilputi/nekuroddo/internal/jira"
	"github.com/aqilputi/nekuroddo/internal/todo"
)

// EvTodosChanged is published after every reload of the todo list.
const EvTodosChanged = "todos.changed"

// TodoList is the todo list as last loaded (the payload of todos.changed and
// the Data of the todo commands). Err is the last load's error; Groups then
// stays the last good list.
type TodoList struct {
	Groups []todo.Group `json:"groups"`
	Err    string       `json:"err,omitempty"`
}

// Error is Err as an error (nil when the load worked).
func (l TodoList) Error() error {
	if l.Err == "" {
		return nil
	}
	return errors.New(l.Err)
}

// todoJiraState is what the slice keeps (App.todoJira): the last todo list
// and the Jira cache with its loading guard.
type todoJiraState struct {
	mu        sync.Mutex
	todos     TodoList
	cache     jira.Cache
	cacheRead bool // cache came from disk already
	loading   bool
	jiraErr   error
}

func (a *App) tj() *todoJiraState { return &a.todoJira }

// TodoAvailable reports whether the todo skill is installed.
func (a *App) TodoAvailable() bool { return a.ports.Todo != nil && a.ports.Todo.Available() }

// Todos is the list as last loaded.
func (a *App) Todos() TodoList {
	s := a.tj()
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.todos
}

// LoadTodos reads the list. A failure keeps the last good list and the
// error; either way todos.changed is published.
func (a *App) LoadTodos() ([]todo.Group, error) {
	g, err := a.ports.Todo.List()
	s := a.tj()
	s.mu.Lock()
	if err != nil {
		s.todos.Err = err.Error()
	} else {
		s.todos = TodoList{Groups: g}
	}
	l := s.todos
	s.mu.Unlock()
	a.Publish(EvTodosChanged, l)
	return g, err
}

// StartTodo marks task id as started and reloads the list (a failed start
// shows in the reloaded list, as before).
func (a *App) StartTodo(id int) ([]todo.Group, error) {
	_ = a.ports.Todo.Start(id)
	return a.LoadTodos()
}

// DoneTodo marks task id as done and reloads the list.
func (a *App) DoneTodo(id int) ([]todo.Group, error) {
	_ = a.ports.Todo.Done(id)
	return a.LoadTodos()
}

// DoneQuestion is the confirmation before marking a task done.
func DoneQuestion(id int, title string) string {
	return fmt.Sprintf("marcar #%d \"%s\" como done?", id, core.Truncate(title, 50))
}

// AskDoneTodo asks before marking t done; yes reloads (Data: TodoList).
func (a *App) AskDoneTodo(t todo.Task) Result {
	id := t.ID
	return a.Ask(DoneQuestion(id, t.Title), func() (Result, error) {
		g, err := a.DoneTodo(id)
		return Result{Data: listOf(g, err)}, nil
	})
}

func listOf(g []todo.Group, err error) TodoList {
	if err != nil {
		return TodoList{Groups: g, Err: err.Error()}
	}
	return TodoList{Groups: g}
}

// TodoForKey is the task in groups already tracking Jira issue key.
func TodoForKey(groups []todo.Group, key string) *todo.Task {
	if key == "" {
		return nil
	}
	for gi := range groups {
		for i := range groups[gi].Tasks {
			if t := &groups[gi].Tasks[i]; t.JiraKey() == key {
				return t
			}
		}
	}
	return nil
}

// FindTodo is task id in groups.
func FindTodo(groups []todo.Group, id int) *todo.Task {
	for gi := range groups {
		for i := range groups[gi].Tasks {
			if t := &groups[gi].Tasks[i]; t.ID == id {
				return t
			}
		}
	}
	return nil
}

// IssueToTodo brings Jira issue key into the todo list, unless a task in
// groups already tracks it. Data is the reloaded TodoList.
func (a *App) IssueToTodo(groups []todo.Group, key, summary string) Result {
	if t := TodoForKey(groups, key); t != nil {
		return Result{Toast: fmt.Sprintf("%s já está no todo #%d", key, t.ID), Err: true}
	}
	if err := a.ports.Todo.AddFromJira(key, summary); err != nil {
		return Fail(err)
	}
	return Result{Data: listOf(a.LoadTodos())}
}

// TodoDispatchDir is where an agent for t starts: the repo of its first
// @context that has one (cfg.RepoFor), else home. It is shown with ~.
func TodoDispatchDir(cfg config.Config, t todo.Task) string {
	for _, c := range t.Contexts {
		if d := cfg.RepoFor(c); d != "" {
			return config.Tilde(d)
		}
	}
	return config.Tilde(config.Expand("~"))
}

// IssueDispatchDir is where an agent for a Jira issue starts.
func IssueDispatchDir(cfg jira.Config) string {
	if cfg.AgentDir != "" {
		return cfg.AgentDir
	}
	return "~/dev"
}

// DispatchDir is the directory to suggest for an agent working on the
// selected task (ctx.TodoID) or issue (ctx.IssueKey).
func (a *App) DispatchDir(ctx Context) (string, error) {
	switch {
	case ctx.IssueKey != "":
		if issueByKey(a.JiraCache(), ctx.IssueKey) == nil {
			return "", fmt.Errorf("issue %s não está no cache do jira", ctx.IssueKey)
		}
		return IssueDispatchDir(a.cfg.Jira), nil
	case ctx.TodoID != 0:
		t := FindTodo(a.Todos().Groups, ctx.TodoID)
		if t == nil {
			return "", fmt.Errorf("todo #%d não encontrado", ctx.TodoID)
		}
		return TodoDispatchDir(a.cfg, *t), nil
	}
	return "", ErrNoTodoSelected
}

// ErrNoTodoSelected is a todo command run with no task or issue selected.
var ErrNoTodoSelected = errors.New("nenhuma tarefa ou issue selecionada")
