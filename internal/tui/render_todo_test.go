package tui

import (
	"errors"
	"strings"
	"testing"
	"time"

	"github.com/aqilputi/nekuroddo/internal/app"
	"github.com/aqilputi/nekuroddo/internal/app/apptest"
	"github.com/aqilputi/nekuroddo/internal/config"
	"github.com/aqilputi/nekuroddo/internal/jira"
	"github.com/aqilputi/nekuroddo/internal/todo"
)

const rhJiraTime = "2006-01-02T15:04:05.000-0700"

// rhTodoModel: two initiatives of tasks and two Jira issues (one old).
func rhTodoModel(t *testing.T) Model {
	t.Helper()
	m := rhTodoBase(t)
	bhSetTodos(t, &m, []todo.Group{
		{Group: "trabalho", Tasks: []todo.Task{
			{ID: 7, Title: "subir o rc", Status: "doing", Initiative: "trabalho", Priority: rhInt(1),
				When: rhStr("2026-10-20"), Contexts: []string{"@neko"},
				Refs:  []todo.Ref{{Kind: "jira", Value: "VIS-1", URL: "https://jira/VIS-1"}},
				Links: []string{"https://example.com/doc"}, Desc: "descrição da tarefa",
				Notes: []todo.Note{{At: "2026-10-15T10:00:00", Text: "comecei"}}},
			{ID: 8, Title: "esperar o silmar", Status: "waiting", WaitingOn: rhStr("silmar"), Due: rhStr("2026-10-30")},
		}},
		{Group: "casa", Tasks: []todo.Task{
			{ID: 9, Title: "lavar a louça", Status: "todo"},
			{ID: 10, Title: "pagar conta", Status: "done"},
			{ID: 11, Title: "status estranho", Status: "???"},
		}},
	})
	now := time.Now()
	bhSetJira(t, &m, jira.Cache{FetchedAt: now.Add(-10 * time.Minute), Issues: []jira.Issue{
		{Key: "VIS-1", Summary: "rc quebrado", Status: "In Progress", StatusCat: "indeterminate", Type: "Bug",
			Priority: "High", Updated: now.Add(-time.Hour).Format(rhJiraTime), URL: "https://jira/VIS-1", Desc: "o **rc** não sobe"},
		{Key: "VIS-2", Summary: "issue nova", Status: "To Do", StatusCat: "new", Updated: now.Add(-2 * time.Hour).Format(rhJiraTime)},
		{Key: "VIS-3", Summary: "fechada", Status: "Done", StatusCat: "done", Updated: now.Format(rhJiraTime)},
		{Key: "VIS-4", Summary: "estranha", StatusCat: "weird", Updated: now.Format(rhJiraTime)},
		{Key: "VIS-0", Summary: "velha demais", StatusCat: "new", Updated: now.Add(-200 * 24 * time.Hour).Format(rhJiraTime)},
	}})
	m.rebuildTodos()
	return m
}

// rhTodoBase is rhModel with Jira on and the todo skill installed, over the
// fake todo and Jira ports of bhModel (empty list, nothing fetched).
func rhTodoBase(t *testing.T) Model {
	t.Helper()
	rhEnv(t)
	cfg := config.Default()
	cfg.Jira.Enabled = true
	m := newTestModel(cfg, withFakeTodoJira)
	m.w, m.h = 140, 40
	m.todoAvail = true
	return m
}

func rhTodoCur(m *Model, match func(todoRow) bool) {
	for i, r := range m.todos {
		if match(r) {
			m.todoCur = i
			return
		}
	}
}

func TestRhTodoViewStates(t *testing.T) {
	m := rhTodoBase(t)
	m.todoAvail = false
	rhHas(t, m.todoView(m.t(), 50, 10), "skill todo não encontrada")
	m.todoAvail = true
	td, _ := bhFakes(&m)
	td.Errs["List"] = errors.New("todo list: python explodiu")
	_, _ = m.app.LoadTodos()
	rhHas(t, m.todoView(m.t(), 50, 10), "python explodiu")
	td.Errs["List"] = nil
	_, _ = m.app.LoadTodos()
	rhHas(t, m.todoView(m.t(), 50, 10), "lista vazia")
}

func TestRhTodoViewTasksAndIssues(t *testing.T) {
	m := rhTodoModel(t)
	out := m.todoView(m.t(), 80, 40)
	rhFits(t, out, 80)
	rhHas(t, out, "▾ trabalho", "▾ casa", "▾ ⚑ jira · minhas issues (há 10m)")

	l := rhLineWith(out, "subir o rc")
	for _, w := range []string{"7", "◐", "!!!", "⚑ VIS-1", "◷ 10-20", "@neko"} {
		if !strings.Contains(l, w) {
			t.Fatalf("task line misses %q: %q", w, l)
		}
	}
	if l := rhLineWith(out, "esperar o silmar"); !strings.Contains(l, "⧗") || !strings.Contains(l, "⧗ silmar") {
		t.Fatalf("waiting task: %q", l)
	}
	for title, icon := range map[string]string{"lavar a louça": "☐", "pagar conta": "☑", "status estranho": "   11 "} {
		if l := rhLineWith(out, title); !strings.Contains(l, icon) {
			t.Fatalf("%s should show %q: %q", title, icon, l)
		}
	}
	// issues: key, state icon, status; VIS-1 links back to task #7
	if l := rhLineWith(out, "rc quebrado"); !strings.Contains(l, "VIS-1") || !strings.Contains(l, "◐") || !strings.Contains(l, "↔#7") {
		t.Fatalf("issue line: %q", l)
	}
	if l := rhLineWith(out, "fechada"); !strings.Contains(l, "☑") {
		t.Fatalf("done issue: %q", l)
	}
	if l := rhLineWith(out, "estranha"); !strings.Contains(l, "☐") {
		t.Fatalf("unknown category falls back to ☐: %q", l)
	}
	rhHasNot(t, out, "velha demais") // untouched for months: hidden unless H

	m.view.ShowAll = true
	m.app.BeginJiraFetch(true) // a fetch starts (never run here)
	m.rebuildTodos()
	out = m.todoView(m.t(), 80, 40)
	rhHas(t, out, "velha demais", "buscando…")
}

func TestRhTodoViewScrollsToCursor(t *testing.T) {
	m := rhTodoModel(t)
	rhTodoCur(&m, func(r todoRow) bool { return r.issue != nil && r.issue.Key == "VIS-4" })
	lines := rhLines(m.todoView(m.t(), 80, 4))
	if len(lines) != 4 {
		t.Fatalf("want 4 lines, got %d", len(lines))
	}
	if !strings.Contains(strings.Join(lines, "\n"), "estranha") {
		t.Fatalf("cursor row must be visible:\n%s", strings.Join(lines, "\n"))
	}
}

func TestRhTodoDetailTask(t *testing.T) {
	m := rhTodoModel(t)
	rhTodoCur(&m, func(r todoRow) bool { return r.task != nil && r.task.ID == 7 })
	out := m.todoDetail(m.t(), 70, 40)
	rhFits(t, out, 70)
	rhHas(t, out, "#7 subir o rc", "doing", "iniciativa", "trabalho", "contextos", "@neko", "quando", "2026-10-20",
		"descrição da tarefa", "jira", "VIS-1  https://jira/VIS-1", "link", "https://example.com/doc",
		"2026-10-15T10:00", "comecei",
		"o **rc** não sobe", // the linked issue's description
		"enter dispara um agent · s start · D done")
	rhHasNot(t, out, "J cria no Jira") // already linked

	rhTodoCur(&m, func(r todoRow) bool { return r.task != nil && r.task.ID == 8 })
	out = m.todoDetail(m.t(), 70, 40)
	rhHas(t, out, "prazo", "2026-10-30", "J cria no Jira")
}

func TestRhTodoDetailIssue(t *testing.T) {
	m := rhTodoModel(t)
	rhTodoCur(&m, func(r todoRow) bool { return r.issue != nil && r.issue.Key == "VIS-1" })
	out := m.todoDetail(m.t(), 70, 40)
	rhFits(t, out, 70)
	rhHas(t, out, "VIS-1", "Bug", "rc quebrado", "In Progress", "High", "https://jira/VIS-1", "#7 subir o rc",
		"o **rc** não sobe", "J vira todo")

	rhTodoCur(&m, func(r todoRow) bool { return r.issue != nil && r.issue.Key == "VIS-2" })
	rhHasNot(t, m.todoDetail(m.t(), 70, 40), "#7") // no task tracks it
}

func TestRhTodoDetailNothingSelected(t *testing.T) {
	m := rhModel(t)
	if out := m.todoDetail(m.t(), 70, 40); out != "" {
		t.Fatalf("no row: empty detail, got %q", out)
	}
}

func TestRhLongDescriptionKeepsHintVisible(t *testing.T) {
	m := rhTodoModel(t)
	m.app.JiraCache().Issues[0].Desc = strings.Repeat("linha de descrição comprida. ", 300) // the cache shares its issues
	rhTodoCur(&m, func(r todoRow) bool { return r.issue != nil && r.issue.Key == "VIS-1" })
	const h = 20
	out := m.todoDetail(m.t(), 50, h)
	lines := rhLines(out)
	if len(lines) > h {
		t.Fatalf("detail must fit %d lines, got %d", h, len(lines))
	}
	if !strings.Contains(lines[len(lines)-1], "J vira todo") {
		t.Fatalf("hint must stay on the last line: %q", lines[len(lines)-1])
	}
	rhHas(t, out, "…")
}

func TestRhClipLines(t *testing.T) {
	cases := []struct {
		in   string
		n    int
		want string
	}{
		{"a\nb\nc", 5, "a\nb\nc"},
		{"a\nb\nc", 3, "a\nb\nc"},
		{"a\nb\nc", 2, "a\n…"},
		{"a\nb\nc", 1, "…"},
		{"a\nb\nc", 0, ""},
		{"a", -3, ""},
	}
	for _, c := range cases {
		if got := clipLines(c.in, c.n); got != c.want {
			t.Errorf("clipLines(%q, %d) = %q, want %q", c.in, c.n, got, c.want)
		}
	}
}

func TestRhWithDescNoRoom(t *testing.T) {
	m := rhModel(t)
	lines := []string{"a", "b", "c"}
	out := withDesc(m.t(), lines, "descrição", "dica", 30, 5) // no room: description dropped
	rhHasNot(t, out, "descrição")
	rhHas(t, out, "dica")
	out = withDesc(m.t(), []string{"a"}, "", "dica", 30, 10)
	if got := rhLines(out); len(got) != 3 {
		t.Fatalf("no description: fields, blank, hint; got %q", got)
	}
}

// TestRhTodoSliceThroughTheCore: the tab reads and writes through app (its
// ports are fakes here), never the todo/jira/xdg-open adapters.
func TestRhTodoSliceThroughTheCore(t *testing.T) {
	rhEnv(t)
	f := apptest.New()
	f.Todo.Avail = true
	f.Todo.Groups = []todo.Group{{Group: "g", Tasks: []todo.Task{{ID: 3, Title: "x"}}}}
	cfg := config.Default()
	cfg.Jira.Enabled = true
	a := app.New(cfg, f.Ports())
	defer a.Close()
	s := newTodoJiraSlice(a)
	if !s.todoAvail || len(a.JiraCache().Issues) != 0 {
		t.Fatalf("starts from the core: %+v", s)
	}
	m := New(a, a.Config())
	m.todoJiraSlice = s
	m.update(m.loadTodos()())
	if len(m.todos) != 1 || m.todos[0].task.ID != 3 {
		t.Fatalf("loaded through app: %+v", m.todos)
	}
	m.update(m.startTodo()())
	if c := f.Todo.CallsTo("Start"); len(c) != 1 || c[0].Args[0] != 3 {
		t.Fatalf("started through app: %v", c)
	}
	if m.openIssueURL("https://x/1")() != nil || len(f.Open.URLs()) != 1 {
		t.Fatal("opened through the Opener port")
	}
	f.Open.Err = errors.New("sem browser")
	if msg, ok := m.openIssueURL("https://x/2")().(toastMsg); !ok || !msg.err {
		t.Fatal("a failure is a toast")
	}
}

func TestRhTodosTabComposed(t *testing.T) {
	m := rhTodoModel(t)
	m.tab = tabTodos
	rhTodoCur(&m, func(r todoRow) bool { return r.task != nil && r.task.ID == 9 })
	out := m.View().Content
	rhHas(t, out, " todos ", " tarefa ", "#9 lavar a louça", "3 Todos (")
}
