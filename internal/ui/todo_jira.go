package ui

// Slice "jira/todo": the todos tab, Jira issues and agents dispatched
// from them (WP-2.5 moves the logic to app/jira.go and app/todo.go).

import (
	"errors"
	"fmt"
	"os/exec"
	"strings"
	"time"

	tea "charm.land/bubbletea/v2"

	"github.com/aqilputi/nekuroddo/internal/config"
	"github.com/aqilputi/nekuroddo/internal/core"
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

// rebuildTodos flattens tasks and Jira issues into the todos tab.
func (m *Model) rebuildTodos() {
	m.todos = m.todos[:0]
	for gi := range m.tasks {
		g := &m.tasks[gi]
		for i := range g.Tasks {
			m.todos = append(m.todos, todoRow{group: g.Group, task: &g.Tasks[i]})
		}
	}
	if m.cfg.Jira.Enabled {
		for i := range m.jira.Issues {
			is := &m.jira.Issues[i]
			// Issues untouched for months stay out of the way unless H.
			if !m.view.ShowAll && time.Since(is.UpdatedAt()) > 90*24*time.Hour {
				continue
			}
			m.todos = append(m.todos, todoRow{group: "jira", issue: is})
		}
	}
	m.todoCur = min(m.todoCur, max(len(m.todos)-1, 0))
}

func (m Model) loadTodos() tea.Cmd {
	if !m.todoAvail {
		return nil
	}
	return func() tea.Msg {
		g, err := todo.List()
		return todosMsg{g, err}
	}
}

// fetchJira refreshes the Jira list through headless claude (~$0.1 a call),
// so it only runs on demand or when the cache is older than cache_minutes.
func (m *Model) fetchJira(force bool) tea.Cmd {
	cfg := m.cfg.Jira
	if !cfg.Enabled || m.jiraLoading || (!force && m.jira.Fresh(cfg)) {
		return nil
	}
	m.jiraLoading = true
	return func() tea.Msg {
		c, err := jira.Fetch(cfg)
		return jiraMsg{c, err}
	}
}

// jiraAction is J in the todos tab: a task goes to Jira (only when you ask),
// an issue comes into the todo list.
func (m *Model) jiraAction() tea.Cmd {
	if !m.cfg.Jira.Enabled {
		return toastCmd("jira desligado: configure [jira] no config.toml", true)
	}
	r := m.curRow()
	if r == nil {
		return nil
	}
	if r.issue != nil {
		if t := m.todoForKey(r.issue.Key); t != nil {
			return toastCmd(fmt.Sprintf("%s já está no todo #%d", r.issue.Key, t.ID), true)
		}
		key, sum := r.issue.Key, r.issue.Summary
		return func() tea.Msg {
			if err := todo.AddFromJira(key, sum); err != nil {
				return toastMsg{text: err.Error(), err: true}
			}
			g, err := todo.List()
			return todosMsg{g, err}
		}
	}
	if k := r.task.JiraKey(); k != "" {
		return toastCmd(fmt.Sprintf("#%d já tem issue: %s", r.task.ID, k), true)
	}
	def := m.cfg.Jira.DefaultProject + "/" + m.cfg.Jira.DefaultType
	return m.ask(inJiraProject, fmt.Sprintf("#%d no Jira — projeto/tipo › ", r.task.ID), def)
}

// submitJiraProject creates the selected task in Jira ("PROJ/Type"), asking first.
func (m *Model) submitJiraProject(val string) tea.Cmd {
	t := m.curTodo()
	if t == nil || val == "" {
		return nil
	}
	proj, typ, _ := strings.Cut(val, "/")
	proj = strings.ToUpper(strings.TrimSpace(proj))
	typ = strings.TrimSpace(typ)
	if typ == "" {
		typ = m.cfg.Jira.DefaultType
	}
	task := *t
	n := jira.NewIssue{Project: proj, Type: typ, Summary: task.Title, Description: jiraDescription(task), Labels: []string{"nekuroddo"}}
	cfg := m.cfg.Jira
	m.confirmThen(fmt.Sprintf("criar no Jira %s/%s, atribuída a você:\n\"%s\"\n(escreve no Jira)", proj, typ, core.Truncate(task.Title, 70)), func(*Model) tea.Cmd {
		return tea.Batch(toastCmd("criando no Jira via claude…", false), func() tea.Msg {
			key, cost, err := jira.Create(cfg, n)
			return jiraCreatedMsg{task: task.ID, key: key, cost: cost, err: err}
		})
	})
	return nil
}

// jiraDescription is the issue body built from a task.
func jiraDescription(t todo.Task) string {
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

func (m *Model) dispatchTodo() tea.Cmd {
	if r := m.curRow(); r != nil && r.issue != nil {
		m.draftIssue = r.issue
		dir := m.cfg.Jira.AgentDir
		if dir == "" {
			dir = "~/visio/dev"
		}
		return m.ask(inJiraAgentDir, "agent p/ "+r.issue.Key+" em › ", dir)
	}
	t := m.curTodo()
	if t == nil {
		return nil
	}
	dir := ""
	for _, c := range t.Contexts {
		if d := m.cfg.RepoFor(c); d != "" {
			dir = d
			break
		}
	}
	if dir == "" {
		dir = config.Expand("~")
	}
	m.draftTask = t
	return m.ask(inDispatchDir, fmt.Sprintf("agent p/ #%d em › ", t.ID), config.Tilde(dir))
}

// startTodo marks the selected task as started (nil when none is selected).
func (m *Model) startTodo() tea.Cmd {
	t := m.curTodo()
	if t == nil {
		return nil
	}
	id := t.ID
	return func() tea.Msg {
		_ = todo.Start(id)
		g, err := todo.List()
		return todosMsg{g, err}
	}
}

// doneTodo marks the selected task as done, asking first.
func (m *Model) doneTodo() {
	if t := m.curTodo(); t != nil {
		id, title := t.ID, t.Title
		m.confirmThen(fmt.Sprintf("marcar #%d \"%s\" como done?", id, core.Truncate(title, 50)), func(*Model) tea.Cmd {
			return func() tea.Msg { _ = todo.Done(id); g, err := todo.List(); return todosMsg{g, err} }
		})
	}
}

func openURL(u string) tea.Cmd {
	return func() tea.Msg {
		if err := exec.Command("xdg-open", u).Start(); err != nil {
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
		return toastCmd("jira: "+msg.err.Error(), true)
	}
	m.jira = msg.cache
	m.rebuildTodos()
	return toastCmd(fmt.Sprintf("jira: %d issues (custou $%.2f)", len(m.jira.Issues), m.jira.CostUSD), false)
}

func (m *Model) onJiraCreated(msg jiraCreatedMsg) tea.Cmd {
	var denied jira.DeniedError
	if errors.As(msg.err, &denied) && m.cfg.Jira.ApproveHint != "" {
		return stickyToast("jira: " + denied.Error() + "  — se for o seu hook, rode:  " + m.cfg.Jira.ApproveHint + "  e aperte J de novo")
	}
	if msg.err != nil {
		return stickyToast("jira: " + msg.err.Error() + "  (log: ~/.cache/nekuroddo/jira-last.jsonl)")
	}
	id, key := msg.task, msg.key
	return tea.Batch(
		toastCmd(fmt.Sprintf("%s criada e ligada ao #%d ($%.2f)", key, id, msg.cost), false),
		func() tea.Msg {
			_ = todo.LinkJira(id, key)
			g, err := todo.List()
			return todosMsg{g, err}
		},
		m.fetchJira(true))
}
