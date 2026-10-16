package tui

// Slice "jira/todo": the todos tab. The use cases and the state (the task
// list, the Jira cache and its fetch) live in app (todo.go, jira.go); this
// file keeps the tab's rows and cursor, asks for input and turns the core's
// answers into messages.

import (
	"fmt"

	tea "charm.land/bubbletea/v2"

	"github.com/aqilputi/nekuroddo/internal/app"
	"github.com/aqilputi/nekuroddo/internal/jira"
	"github.com/aqilputi/nekuroddo/internal/todo"
)

func (m *Model) curRow() *todoRow {
	if m.todoCur < len(m.todos) {
		return &m.todos[m.todoCur]
	}
	return nil
}

func (m *Model) curTodo() *todo.Task {
	if r := m.curRow(); r != nil {
		return r.task
	}
	return nil
}

// todoForKey finds the task already tracking a Jira issue.
func (m *Model) todoForKey(key string) *todo.Task {
	for _, r := range m.todos {
		if r.task != nil && r.task.JiraKey() == key {
			return r.task
		}
	}
	return nil
}

// issueForKey is the core's cached Jira issue with that key, if any.
func (m *Model) issueForKey(key string) *jira.Issue {
	if key == "" {
		return nil
	}
	issues := m.app.JiraCache().Issues
	for i := range issues {
		if issues[i].Key == key {
			return &issues[i]
		}
	}
	return nil
}

// rebuildTodos flattens the core's tasks and Jira issues into the todos tab.
func (m *Model) rebuildTodos() {
	m.todos = m.todos[:0]
	tasks := m.app.Todos().Groups
	for gi := range tasks {
		g := &tasks[gi]
		for i := range g.Tasks {
			m.todos = append(m.todos, todoRow{group: g.Group, task: &g.Tasks[i]})
		}
	}
	if m.cfg.Jira.Enabled {
		now := m.app.Clock().Now()
		issues := m.app.JiraCache().Issues
		for i := range issues {
			is := &issues[i]
			// Issues untouched for months stay out of the way unless H.
			if !m.view.ShowAll && app.IssueStale(*is, now) {
				continue
			}
			m.todos = append(m.todos, todoRow{group: "jira", issue: is})
		}
	}
	m.todoCur = min(m.todoCur, max(len(m.todos)-1, 0))
}

// todosCmd runs a todo use case in the background; the core keeps the
// reloaded list and onTodos redraws it.
func todosCmd(run func() ([]todo.Group, error)) tea.Cmd {
	return func() tea.Msg {
		_, err := run()
		return todosMsg{err}
	}
}

// todoResult turns a Result whose Data is an app.TodoList into a message.
func todoResult(res app.Result) tea.Msg {
	if res.Err {
		return toastMsg{text: res.Toast, err: true}
	}
	l, _ := res.Data.(app.TodoList)
	return todosMsg{l.Error()}
}

func (m Model) loadTodos() tea.Cmd {
	if !m.todoAvail {
		return nil
	}
	return todosCmd(m.app.LoadTodos)
}

// fetchJira refreshes the Jira list through headless claude (~$0.1 a call),
// so it only runs on demand or when the cache is older than cache_minutes.
// The core marks the fetch as running at once (the tab says "buscando…").
func (m *Model) fetchJira(force bool) tea.Cmd {
	run := m.app.BeginJiraFetch(force)
	if run == nil {
		return nil
	}
	return func() tea.Msg {
		c, err := run()
		return jiraMsg{c, err}
	}
}

// jiraAction is J in the todos tab: a task goes to Jira (only when you ask),
// an issue comes into the todo list.
func (m *Model) jiraAction() tea.Cmd {
	if !m.cfg.Jira.Enabled {
		return toastCmd(app.ErrJiraOff.Error(), true)
	}
	r := m.curRow()
	if r == nil {
		return nil
	}
	if r.issue != nil {
		a, groups, is := m.app, m.app.Todos().Groups, *r.issue
		return func() tea.Msg { return todoResult(a.IssueToTodo(groups, is.Key, is.Summary)) }
	}
	if err := app.TaskHasIssue(*r.task); err != nil {
		return toastCmd(err.Error(), true)
	}
	return m.ask(inJiraProject, fmt.Sprintf("#%d no Jira — projeto/tipo › ", r.task.ID), app.JiraDefaultTarget(m.cfg.Jira))
}

// submitJiraProject creates the selected task in Jira ("PROJ/Type"), asking first.
func (m *Model) submitJiraProject(val string) tea.Cmd {
	t := m.curTodo()
	if t == nil || val == "" {
		return nil
	}
	proj, typ := app.JiraTarget(m.cfg.Jira, val)
	task, a := *t, m.app
	m.confirmThen(app.CreateIssueQuestion(proj, typ, task.Title), func(*Model) tea.Cmd {
		return tea.Batch(toastCmd("criando no Jira via claude…", false), func() tea.Msg {
			key, cost, err := a.CreateIssue(task, proj, typ)
			return jiraCreatedMsg{task: task.ID, key: key, cost: cost, err: err}
		})
	})
	return nil
}

// jiraDescription is the issue body built from a task.
func jiraDescription(t todo.Task) string { return app.JiraDescription(t) }

// dispatchTodo asks where an agent for the selected task or issue starts;
// submitInput then dispatches it (app.Dispatch).
func (m *Model) dispatchTodo() tea.Cmd {
	if r := m.curRow(); r != nil && r.issue != nil {
		m.draftIssue = r.issue
		return m.ask(inJiraAgentDir, "agent p/ "+r.issue.Key+" em › ", app.IssueDispatchDir(m.cfg.Jira))
	}
	t := m.curTodo()
	if t == nil {
		return nil
	}
	m.draftTask = t
	return m.ask(inDispatchDir, fmt.Sprintf("agent p/ #%d em › ", t.ID), app.TodoDispatchDir(m.cfg, *t))
}

// startTodo marks the selected task as started (nil when none is selected).
func (m *Model) startTodo() tea.Cmd {
	t := m.curTodo()
	if t == nil {
		return nil
	}
	id, a := t.ID, m.app
	return todosCmd(func() ([]todo.Group, error) { return a.StartTodo(id) })
}

// doneTodo marks the selected task as done, asking first.
func (m *Model) doneTodo() {
	if t := m.curTodo(); t != nil {
		id, a := t.ID, m.app
		m.confirmThen(app.DoneQuestion(id, t.Title), func(*Model) tea.Cmd {
			return todosCmd(func() ([]todo.Group, error) { return a.DoneTodo(id) })
		})
	}
}

// openIssueURL opens u in the browser through the core (app.OpenURL, the
// Opener port).
func (m *Model) openIssueURL(u string) tea.Cmd {
	a := m.app
	return func() tea.Msg {
		if err := a.OpenURL(u); err != nil {
			return toastMsg{text: err.Error(), err: true}
		}
		return nil
	}
}

// onTodos redraws the tab from the core's list (a failed load keeps the
// last good one; the view shows the error).
func (m *Model) onTodos(todosMsg) tea.Cmd {
	m.rebuildTodos()
	return nil
}

func (m *Model) onJira(msg jiraMsg) tea.Cmd {
	if msg.err != nil {
		return toastCmd(app.JiraFetchText(msg.cache, msg.err), true)
	}
	m.rebuildTodos()
	return toastCmd(app.JiraFetchText(msg.cache, nil), false)
}

func (m *Model) onJiraCreated(msg jiraCreatedMsg) tea.Cmd {
	if msg.err != nil {
		return stickyToast(app.JiraCreateFailure(msg.err, m.cfg.Jira.ApproveHint))
	}
	id, key, a := msg.task, msg.key, m.app
	return tea.Batch(
		toastCmd(app.JiraCreatedText(key, id, msg.cost), false),
		todosCmd(func() ([]todo.Group, error) { return a.LinkIssue(id, key) }),
		m.fetchJira(true))
}

// todoJiraSlice is the todos tab's own state: its rows (built from the
// core's list and cache), the cursor, whether the todo skill is installed
// (asked once, at start) and the issue an agent is being asked for.
type todoJiraSlice struct {
	todos      []todoRow
	todoCur    int
	todoAvail  bool
	draftIssue *jira.Issue
}

// newTodoJiraSlice starts the tab from the core.
func newTodoJiraSlice(a *app.App) todoJiraSlice {
	return todoJiraSlice{todoAvail: a.TodoAvailable()}
}
