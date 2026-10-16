package ui

// Slice "jira/todo": the todos tab. The use cases live in app (todo.go,
// jira.go); this file keeps the tab's rows and cursor, asks for input and
// turns the core's answers into messages.

import (
	"fmt"

	tea "charm.land/bubbletea/v2"

	"github.com/aqilputi/nekuroddo/internal/app"
	"github.com/aqilputi/nekuroddo/internal/jira"
	"github.com/aqilputi/nekuroddo/internal/todo"
	"github.com/aqilputi/nekuroddo/internal/wire"
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

// issueForKey is the cached Jira issue with that key, if any.
func (m *Model) issueForKey(key string) *jira.Issue {
	if key == "" {
		return nil
	}
	for i := range m.jira.Issues {
		if m.jira.Issues[i].Key == key {
			return &m.jira.Issues[i]
		}
	}
	return nil
}

// rebuildTodos flattens tasks and Jira issues (as the core last handed them
// over) into the todos tab.
func (m *Model) rebuildTodos() {
	m.todos = m.todos[:0]
	for gi := range m.tasks {
		g := &m.tasks[gi]
		for i := range g.Tasks {
			m.todos = append(m.todos, todoRow{group: g.Group, task: &g.Tasks[i]})
		}
	}
	if m.cfg.Jira.Enabled {
		now := m.app.Clock().Now()
		for i := range m.jira.Issues {
			is := &m.jira.Issues[i]
			// Issues untouched for months stay out of the way unless H.
			if !m.view.ShowAll && app.IssueStale(*is, now) {
				continue
			}
			m.todos = append(m.todos, todoRow{group: "jira", issue: is})
		}
	}
	m.todoCur = min(m.todoCur, max(len(m.todos)-1, 0))
}

// todosCmd runs a todo use case in the background and hands the reloaded
// list to onTodos.
func todosCmd(run func() ([]todo.Group, error)) tea.Cmd {
	return func() tea.Msg {
		g, err := run()
		return todosMsg{g, err}
	}
}

// todoResult turns a Result whose Data is an app.TodoList into a message.
func todoResult(res app.Result) tea.Msg {
	if res.Err {
		return toastMsg{text: res.Toast, err: true}
	}
	l, _ := res.Data.(app.TodoList)
	return todosMsg{l.Groups, l.Error()}
}

func (m Model) loadTodos() tea.Cmd {
	if !m.todoAvail {
		return nil
	}
	return todosCmd(m.app.LoadTodos)
}

// fetchJira refreshes the Jira list through headless claude (~$0.1 a call),
// so it only runs on demand or when the cache is older than cache_minutes.
func (m *Model) fetchJira(force bool) tea.Cmd {
	if !app.JiraShouldFetch(m.cfg.Jira, m.jira, m.jiraLoading, force, m.app.Clock().Now()) {
		return nil
	}
	m.jiraLoading = true
	a := m.app
	return func() tea.Msg {
		c, _, err := a.FetchJira(force)
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
		a, groups, is := m.app, m.tasks, *r.issue
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

// openURL opens u in the browser through the Opener port.
// TODO(WP-2.7): update.go calls this free function, which cannot reach
// m.app; once it calls m.openIssueURL(u) this shim goes away.
func openURL(u string) tea.Cmd { return openWith(wire.Opener{}, u) }

// openIssueURL opens u through the core (app.OpenURL).
func (m *Model) openIssueURL(u string) tea.Cmd { return openWith(m.app, u) }

func openWith(o app.Opener, u string) tea.Cmd {
	return func() tea.Msg {
		if err := o.OpenURL(u); err != nil {
			return toastMsg{text: err.Error(), err: true}
		}
		return nil
	}
}

func (m *Model) onTodos(msg todosMsg) tea.Cmd {
	m.todoErr = msg.err
	if msg.err == nil {
		m.tasks = msg.groups
	}
	m.rebuildTodos()
	return nil
}

func (m *Model) onJira(msg jiraMsg) tea.Cmd {
	m.jiraLoading = false
	m.jiraErr = msg.err
	if msg.err != nil {
		return toastCmd(app.JiraFetchText(msg.cache, msg.err), true)
	}
	m.jira = msg.cache
	m.rebuildTodos()
	return toastCmd(app.JiraFetchText(m.jira, nil), false)
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

// todoJiraSlice is the todos tab: the core's task list and Jira cache as
// last received, the cursor, and the issue an agent is being asked for.
type todoJiraSlice struct {
	todos     []todoRow
	todoCur   int
	todoErr   error
	todoAvail bool
	tasks     []todo.Group

	jira        jira.Cache
	jiraLoading bool
	jiraErr     error
	draftIssue  *jira.Issue
}

// newTodoJiraSlice starts the tab from the core: whether the todo skill is
// there and the Jira list saved on disk.
// TODO(WP-2.7): model.go calls it without the app; pass it
// (newTodoJiraSlice(a)) and drop the fallback, which asks the adapters.
func newTodoJiraSlice(a ...*app.App) todoJiraSlice {
	if len(a) > 0 && a[0] != nil {
		return todoJiraSlice{todoAvail: a[0].TodoAvailable(), jira: a[0].JiraCache()}
	}
	return todoJiraSlice{todoAvail: wire.Todo{}.Available(), jira: jira.LoadCache()}
}
