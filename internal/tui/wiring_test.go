package tui

import (
	"testing"

	tea "charm.land/bubbletea/v2"

	"github.com/aqilputi/nekuroddo/internal/app"
	"github.com/aqilputi/nekuroddo/internal/app/apptest"
	"github.com/aqilputi/nekuroddo/internal/config"
	"github.com/aqilputi/nekuroddo/internal/theme"
	"github.com/aqilputi/nekuroddo/internal/todo"
)

// New takes the core the caller composed (it builds none: no wire here) and
// the presentation's config.
func TestNewUsesTheGivenCore(t *testing.T) {
	a := app.New(config.Default(), apptest.Ports())
	cfg := config.Default()
	cfg.Theme = theme.All[1].Name
	m := New(a, cfg)
	if m.app != a || m.th != 1 || !m.themeSet || m.lua != nil {
		t.Fatalf("New(a, cfg): app %v theme %d set %v lua %v", m.app == a, m.th, m.themeSet, m.lua)
	}
	if len(a.Registry().All()) != 0 {
		t.Fatal("New must not register commands: the composition root does")
	}
}

// A confirmation answered elsewhere (confirm.resolved) closes its dialog;
// other dialogs stay.
func TestConfirmResolvedClosesItsDialog(t *testing.T) {
	m := bhModel(t, bhSessions()...)
	m.confirmThen("local?", func(*Model) tea.Cmd { return nil })
	res := m.app.Ask("parked?", func() (app.Result, error) { return app.Result{}, nil })
	m.confirmToken(res.Pending.Question, res.Pending.Token)
	if m.update(confirmResolvedMsg{token: "other"}) != nil || len(m.dialogs) != 2 {
		t.Fatal("another token: nothing closes")
	}
	m.update(confirmResolvedMsg{token: res.Pending.Token})
	if len(m.dialogs) != 1 || m.top().(*confirmDialog).q != "local?" {
		t.Fatalf("only the parked question's dialog closes: %d left", len(m.dialogs))
	}
	m.update(confirmResolvedMsg{})
	if len(m.dialogs) != 1 {
		t.Fatal("a local dialog has no token to match")
	}
	// the session commands' dialogs carry their token too
	bhSelect(t, &m, "a")
	m.stop()
	d, ok := m.top().(*confirmDialog)
	if !ok || d.token == "" {
		t.Fatal("x parks a token")
	}
	m.update(confirmResolvedMsg{token: d.token})
	if m.top() == d {
		t.Fatal("answered elsewhere: closed here")
	}
}

// J on an issue brings it into the list through the core, which reloads it.
func TestIssueToTodoReloadsThroughTheCore(t *testing.T) {
	m := bhTodoModel(t, true)
	td, _ := bhFakes(&m)
	td.Errs["AddFromJira"], td.Errs["List"] = nil, nil
	td.Groups = []todo.Group{{Group: "hoje", Tasks: []todo.Task{{ID: 3, Refs: []todo.Ref{{Kind: "jira", Value: "PROJ-8"}}}}}}
	m.todoCur = 3 // PROJ-8, not tracked yet
	msgs := bhRun(m.jiraAction())
	if len(msgs) != 1 {
		t.Fatalf("one message: %v", msgs)
	}
	if tm, ok := msgs[0].(todosMsg); !ok || tm.err != nil {
		t.Fatalf("the reloaded list: %+v", msgs[0])
	}
	m.update(msgs[0])
	if m.todoForKey("PROJ-8") == nil {
		t.Fatal("the issue is a task now")
	}
}

// After an agent for a task starts, the list is reloaded through the core.
func TestDispatchedReloadsTodosThroughTheCore(t *testing.T) {
	m := bhModel(t, bhSessions()...)
	td, _ := bhFakes(&m)
	td.Groups, td.Errs["List"] = []todo.Group{{Group: "g", Tasks: []todo.Task{{ID: 4, Status: "doing"}}}}, nil
	var reloaded bool
	for _, msg := range bhRun(m.update(dispatchedMsg{id: "abcd1234", task: &todo.Task{ID: 4}})) {
		if tm, ok := msg.(todosMsg); ok && tm.err == nil {
			reloaded = true
			m.update(msg)
		}
	}
	if !reloaded || len(m.app.Todos().Groups) != 1 {
		t.Fatalf("todos reloaded: %v %+v", reloaded, m.app.Todos())
	}
}
