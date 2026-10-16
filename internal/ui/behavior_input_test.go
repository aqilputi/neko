package ui

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	tea "charm.land/bubbletea/v2"

	"github.com/aqilputi/nekuroddo/internal/claude"
	"github.com/aqilputi/nekuroddo/internal/config"
	"github.com/aqilputi/nekuroddo/internal/core"
	"github.com/aqilputi/nekuroddo/internal/jira"
	"github.com/aqilputi/nekuroddo/internal/store"
	"github.com/aqilputi/nekuroddo/internal/todo"
)

func bhQuestions() []claude.Question {
	return []claude.Question{
		{Question: "q1", Options: []claude.Option{{Label: "sim"}, {Label: "não"}}},
		{Question: "q2", Options: []claude.Option{{Label: "talvez"}}},
	}
}

func TestBehaviorOptionCountAndResolve(t *testing.T) {
	s := &store.Session{Questions: bhQuestions()}
	if n := core.OptionCount(s.Questions); n != 3 {
		t.Fatalf("options across questions: %d", n)
	}
	if n := core.OptionCount(store.Session{}.Questions); n != 0 {
		t.Fatalf("no questions: %d", n)
	}
	for _, c := range []struct{ in, want string }{
		{"1", "sim"},
		{"2", "não"},
		{"3", "talvez"}, // numbering continues into the next question
		{"4", "4"},      // out of range: sent as typed
		{"0", "0"},
		{"02", "02"}, // not a bare number
		{"2 por favor", "2 por favor"},
		{"ok", "ok"},
	} {
		if got := core.ResolveOption(s.Questions, c.in); got != c.want {
			t.Errorf("resolveOption(%q)=%q want %q", c.in, got, c.want)
		}
	}
}

func TestBehaviorTruncate(t *testing.T) {
	for _, c := range []struct {
		s    string
		n    int
		want string
	}{
		{"abc", 5, "abc"}, {"abc", 3, "abc"}, {"abcdef", 4, "abc…"}, {"abcdef", 1, "…"},
		{"abcdef", 0, "…"}, {"ãõéçü", 3, "ãõ…"},
	} {
		if got := core.Truncate(c.s, c.n); got != c.want {
			t.Errorf("core.Truncate(%q,%d)=%q want %q", c.s, c.n, got, c.want)
		}
	}
}

func TestBehaviorReplyInteractiveRefused(t *testing.T) {
	m := bhModel(t, bhSessions()...)
	bhSelect(t, &m, "b")
	_, cmd := m.keyNormal(bhRune('r'))
	if m.mode != modeNormal {
		t.Fatal("no input line for an interactive session")
	}
	if tm, ok := bhHasToast(cmd, "interativa"); !ok || !tm.err {
		t.Fatal("r on an interactive session explains why not")
	}
}

func TestBehaviorReplyPromptAndOpenPane(t *testing.T) {
	ss := bhSessions()
	ss[0].Questions = bhQuestions()
	ss[0].SuggestedReply = "pode seguir"
	m := bhModel(t, ss...)
	bhSelect(t, &m, "a")
	bhPane(t, &m, "aaaa1111", "a")
	m.keyNormal(bhRune('r'))
	if m.mode != modeInput || m.purpose != inReply || m.draftDir != "a" {
		t.Fatal("r opens the reply line")
	}
	if !strings.Contains(m.input.Prompt, "(1-3 escolhe opção)") || m.input.Value() != "pode seguir" {
		t.Fatalf("prompt %q value %q", m.input.Prompt, m.input.Value())
	}
	m.input.SetValue("3")
	cmd := bhEnter(&m)
	if m.app.Panes().Replies()["aaaa1111"] != "talvez" {
		t.Fatalf("the bare number turns into the option label: %q", m.app.Panes().Replies()["aaaa1111"])
	}
	if m.active != "aaaa1111" || m.mode != modeNormal {
		t.Fatal("reply shows the pane without focusing it")
	}
	// the delayed reply arrives: pasted, then enter
	var gotReply bool
	for _, msg := range bhRun(cmd) {
		if r, ok := msg.(replyMsg); ok && r.pane == "aaaa1111" {
			gotReply = true
		}
	}
	if !gotReply {
		t.Fatal("the reply is scheduled for the pane")
	}
	enter := m.update(replyMsg{pane: "aaaa1111"})
	if _, ok := m.app.Panes().Replies()["aaaa1111"]; ok {
		t.Fatal("the pending reply is consumed")
	}
	msgs := bhRun(enter)
	if len(msgs) != 1 {
		t.Fatalf("paste is followed by enter: %v", msgs)
	}
	if _, ok := bhHasToast(m.update(msgs[0]), "resposta enviada"); !ok {
		t.Fatal("enter confirms the reply")
	}
	// a second replyMsg without pending text does nothing
	if m.update(replyMsg{pane: "aaaa1111"}) != nil || m.update(replyMsg{pane: "nope"}) != nil {
		t.Fatal("nothing pending: no-op")
	}
}

func TestBehaviorReplyEmptyOrUnknownSession(t *testing.T) {
	m := bhModel(t, bhSessions()...)
	m.ask(inReply, "› ", "")
	m.draftDir = "a"
	if cmd := bhEnter(&m); cmd != nil {
		t.Fatal("empty reply sends nothing")
	}
	m.ask(inReply, "› ", "oi")
	m.draftDir = "gone"
	if cmd := bhEnter(&m); cmd != nil {
		t.Fatal("reply to a vanished session sends nothing")
	}
}

func TestBehaviorReplyToHistoryResumes(t *testing.T) {
	m := bhModel(t, bhSessions()...)
	m.ask(inReply, "› ", "continua")
	m.draftDir = "c"
	cmd := bhEnter(&m)
	msgs := bhRun(cmd)
	if len(msgs) != 1 {
		t.Fatalf("resume runs once: %v", msgs)
	}
	po, ok := msgs[0].(paneOpenMsg)
	if !ok || po.err == nil || po.text != "continua" || po.sess.SessionID != "c" {
		t.Fatalf("with a failing claude the resume reports the error: %+v", msgs[0])
	}
	if tm, ok := bhHasToast(m.update(po), "claude --bg --resume"); !ok || !tm.err {
		t.Fatal("the resume error is shown")
	}
}

func TestBehaviorPaneOpenMsgStartsPane(t *testing.T) {
	m := bhModel(t, bhSessions()...)
	t.Cleanup(func() {
		for id := range m.panes {
			m.closePane(id)
		}
	})
	cmd := m.update(paneOpenMsg{sess: m.snap.Sessions[2], id: "cccc3333", text: "olá"})
	if cmd == nil {
		t.Fatal("starting a pane listens to it")
	}
	if m.panes["cccc3333"] == nil || m.app.Panes().PaneOf("c") != "cccc3333" || m.active != "cccc3333" {
		t.Fatal("the resumed session gets a pane")
	}
	if m.mode != modeInsert || m.app.Panes().Replies()["cccc3333"] != "olá" {
		t.Fatal("focused, with the reply pending")
	}
	// the attach exits (claude is /bin/false): the pane closes
	m.update(paneExitMsg{ID: "cccc3333"})
	if m.panes["cccc3333"] != nil || m.mode != modeNormal || m.active != "" {
		t.Fatal("ExitMsg closes the pane and leaves INSERT")
	}
}

func TestBehaviorOpenKeys(t *testing.T) {
	m := bhModel(t, bhSessions()...)
	t.Cleanup(func() {
		for id := range m.panes {
			m.closePane(id)
		}
	})
	// enter on an interactive session: it lives in another terminal
	bhSelect(t, &m, "b")
	_, cmd := m.keyNormal(keyPress("enter"))
	if tm, ok := bhHasToast(cmd, "pid 42"); !ok || !tm.err {
		t.Fatal("enter on an interactive session points to its terminal")
	}
	// o on a background session attaches without focus
	bhSelect(t, &m, "a")
	m.keyNormal(bhRune('o'))
	if m.panes["aaaa1111"] == nil || m.mode != modeNormal || m.active != "aaaa1111" {
		t.Fatal("o attaches in the pane without focusing")
	}
	// enter on a session whose pane is open just focuses it
	p := m.panes["aaaa1111"]
	bhPane(t, &m, "aaaa1111", "a") // replace with a live one
	p.Close()
	if cmd := m.openPane(m.snap.Sessions[0], true, ""); cmd != nil || m.mode != modeInsert {
		t.Fatal("an open pane is reused and focused")
	}
	m.mode = modeNormal
	// i without an active pane opens like enter
	m.active = ""
	bhSelect(t, &m, "b")
	m.syncActive()
	_, cmd = m.keyNormal(bhRune('i'))
	if _, ok := bhHasToast(cmd, "interativa"); !ok {
		t.Fatal("i without a pane behaves like enter")
	}
	// enter on a context row does nothing
	m.view.Cursor = 0
	if _, cmd := m.keyNormal(keyPress("enter")); cmd != nil {
		t.Fatal("enter on a context row does nothing")
	}
}

func TestBehaviorFullscreen(t *testing.T) {
	m := bhModel(t, bhSessions()...)
	bhSelect(t, &m, "a")
	bhPane(t, &m, "aaaa1111", "a")
	_, cmd := m.keyNormal(bhRune('O'))
	if cmd == nil || m.panes["aaaa1111"] != nil {
		t.Fatal("O hands the terminal to claude attach and closes the embedded pane")
	}
	bhSelect(t, &m, "c")
	if _, cmd := m.keyNormal(bhRune('O')); cmd == nil {
		t.Fatal("O on a history session resumes it fullscreen")
	}
	bhSelect(t, &m, "b")
	_, cmd = m.keyNormal(bhRune('O'))
	if _, ok := bhHasToast(cmd, "outro terminal"); !ok {
		t.Fatal("O on an interactive session refuses")
	}
	if _, ok := bhHasToast(m.fullscreen(store.Session{Kind: "interactive"}), "outro terminal"); !ok {
		t.Fatal("fullscreen needs an id")
	}
}

func TestBehaviorNewSessionFlow(t *testing.T) {
	m := bhModel(t, bhSessions()...)
	m.view.Cursor = 0
	m.keyNormal(bhRune('c'))
	if m.purpose != inNewDir || m.input.Value() != "~" {
		t.Fatalf("c without a session starts at ~: %q", m.input.Value())
	}
	if !strings.Contains(m.input.Prompt, "padrão") {
		t.Fatalf("dispatch lines show the permission badge: %q", m.input.Prompt)
	}
	m.key(tea.KeyPressMsg{Code: tea.KeyEscape})
	bhSelect(t, &m, "a")
	m.keyNormal(bhRune('c'))
	if m.input.Value() != "/tmp" {
		t.Fatalf("c starts at the session's dir: %q", m.input.Value())
	}
	m.input.SetValue("")
	if cmd := bhEnter(&m); cmd != nil || m.mode != modeNormal {
		t.Fatal("an empty dir cancels")
	}
	m.keyNormal(bhRune('c'))
	m.input.SetValue("~/proj")
	bhEnter(&m)
	if m.purpose != inNewPrompt || m.mode != modeInput || m.draftDir != config.Expand("~/proj") {
		t.Fatalf("then asks for the prompt in %q", m.draftDir)
	}
	if cmd := bhEnter(&m); cmd != nil {
		t.Fatal("an empty prompt cancels")
	}
}

// tab cycles permission modes only on dispatch lines; elsewhere it is text.
func TestBehaviorPermCycleOnlyWhenDispatching(t *testing.T) {
	m := bhModel(t, bhSessions()...)
	m.ask(inNewDir, "dir › ", "")
	for i := 1; i <= len(permModes); i++ {
		m.key(tea.KeyPressMsg{Code: tea.KeyTab})
		if m.perm != i%len(permModes) {
			t.Fatalf("tab #%d: perm %d", i, m.perm)
		}
	}
	m.key(tea.KeyPressMsg{Code: tea.KeyTab})
	m.key(tea.KeyPressMsg{Code: tea.KeyTab})
	if !strings.Contains(m.input.Prompt, "[auto]") {
		t.Fatalf("badge names the mode: %q", m.input.Prompt)
	}
	m.key(tea.KeyPressMsg{Code: tea.KeyEscape})
	m.perm = 0
	m.ask(inRename, "nome › ", "")
	m.key(tea.KeyPressMsg{Code: tea.KeyTab})
	if m.perm != 0 || strings.Contains(m.input.Prompt, "padrão") {
		t.Fatal("tab on a non-dispatch line leaves the mode alone")
	}
}

func TestBehaviorPasteIntoInput(t *testing.T) {
	m := bhModel(t, bhSessions()...)
	m.ask(inNewPrompt, "› ", "")
	m.update(tea.PasteMsg{Content: "colado"})
	if m.input.Value() != "colado" {
		t.Fatalf("paste goes to the input line: %q", m.input.Value())
	}
	// in NORMAL a paste is ignored
	m.mode = modeNormal
	if m.update(tea.PasteMsg{Content: "x"}) != nil {
		t.Fatal("paste in NORMAL does nothing")
	}
	// in INSERT it goes to the pane
	bhPane(t, &m, "p1", "")
	m.active, m.mode = "p1", modeInsert
	m.update(tea.PasteMsg{Content: "x"})
	// other messages in input mode reach the text input
	m.ask(inNewPrompt, "› ", "")
	_ = m.update(struct{}{})
	if m.mode != modeInput {
		t.Fatal("unknown messages keep the input line")
	}
	m.mode = modeNormal
	if m.update(struct{}{}) != nil {
		t.Fatal("unknown messages in NORMAL do nothing")
	}
}

func TestBehaviorRenameContextSubmit(t *testing.T) {
	m := bhModel(t, bhSessions()...)
	m.view.Cursor = 0 // ctx:c1
	m.keyNormal(bhRune('n'))
	if m.purpose != inRenameCtx || m.input.Value() != "c1" || m.draftDir != "c1" {
		t.Fatal("n on a context renames the section")
	}
	if cmd := bhEnter(&m); cmd != nil {
		t.Fatal("unchanged name: nothing to do")
	}
	color := m.view.CtxColor["c1"]
	m.keyNormal(bhRune('n'))
	m.input.SetValue("novo")
	cmd := bhEnter(&m)
	if _, ok := bhHasToast(cmd, "seção renomeada"); !ok {
		t.Fatal("rename announces itself")
	}
	if m.view.CtxColor["novo"] != color {
		t.Fatal("the renamed section keeps its color")
	}
	// in the inbox n renames the session, not a section
	m.tab = tabInbox
	m.view.InboxCursor = 0
	m.keyNormal(bhRune('n'))
	if m.purpose != inRename || m.draftDir != m.view.Inbox[0].SessionID {
		t.Fatal("n in the inbox renames the session")
	}
}

func TestBehaviorMoveSubmit(t *testing.T) {
	m := bhModel(t, bhSessions()...)
	bhSelect(t, &m, "a")
	m.keyNormal(bhRune('m'))
	m.input.SetValue("outro")
	if _, ok := bhHasToast(bhEnter(&m), "movida para outro"); !ok {
		t.Fatal("move announces the new context")
	}
	bhSelect(t, &m, "b")
	m.keyNormal(bhRune('m'))
	m.input.SetValue("")
	if _, ok := bhHasToast(bhEnter(&m), "regra do diretório"); !ok {
		t.Fatal("empty context goes back to the rule")
	}
	bhSelect(t, &m, "b")
	m.keyNormal(bhRune('n'))
	m.input.SetValue("novo nome")
	if _, ok := bhHasToast(bhEnter(&m), "renomeada"); !ok {
		t.Fatal("rename announces itself")
	}
}

// ---- todos tab ----

func bhTodoModel(t *testing.T, jiraOn bool) Model {
	t.Helper()
	cfg := config.Default()
	cfg.UsageRefresh = 0
	cfg.Jira.Enabled = jiraOn
	cfg.Jira.DefaultProject, cfg.Jira.DefaultType = "PROJ", "Task"
	cfg.Jira.CacheMinutes = 60
	m := bhModelCfg(t, cfg, bhSessions()...)
	bhSetTodos(t, &m, []todo.Group{{Group: "hoje", Tasks: []todo.Task{
		{ID: 1, Title: "escrever testes", Contexts: []string{"@repo"}},
		{ID: 2, Title: "já no jira", Refs: []todo.Ref{{Kind: "jira", Value: "PROJ-7"}}},
	}}})
	recent := time.Now().Add(-time.Hour).Format("2006-01-02T15:04:05.000-0700")
	old := time.Now().Add(-200 * 24 * time.Hour).Format("2006-01-02T15:04:05.000-0700")
	if jiraOn {
		bhSetJira(t, &m, jira.Cache{Issues: []jira.Issue{
			{Key: "PROJ-7", Summary: "ligada", Updated: recent, URL: "https://x/PROJ-7"},
			{Key: "PROJ-8", Summary: "solta", Updated: recent, URL: "https://x/PROJ-8"},
			{Key: "PROJ-1", Summary: "velha", Updated: old},
		}})
	}
	m.tab = tabTodos
	m.rebuildTodos()
	return m
}

func TestBehaviorRebuildTodos(t *testing.T) {
	m := bhTodoModel(t, true)
	if len(m.todos) != 4 {
		t.Fatalf("2 tasks + 2 recent issues: %d", len(m.todos))
	}
	m.view.ShowAll = true
	m.rebuildTodos()
	if len(m.todos) != 5 {
		t.Fatalf("H also shows old issues: %d", len(m.todos))
	}
	off := bhTodoModel(t, false)
	if len(off.todos) != 2 {
		t.Fatalf("jira off: only tasks: %d", len(off.todos))
	}
	off.todoCur = 9
	off.rebuildTodos()
	if off.todoCur != 1 {
		t.Fatalf("cursor clamps: %d", off.todoCur)
	}
	if got := m.todoLineIdx(); len(got) != 7 || got[0] != -1 || got[3] != -1 {
		t.Fatalf("group headers interleave: %v", got)
	}
}

func TestBehaviorTodoLookups(t *testing.T) {
	m := bhTodoModel(t, true)
	if tk := m.todoForKey("PROJ-7"); tk == nil || tk.ID != 2 {
		t.Fatal("the task linked to PROJ-7")
	}
	if m.todoForKey("PROJ-8") != nil {
		t.Fatal("PROJ-8 is not tracked")
	}
	if is := m.issueForKey("PROJ-8"); is == nil || is.Summary != "solta" {
		t.Fatal("issue by key")
	}
	if m.issueForKey("") != nil || m.issueForKey("X-1") != nil {
		t.Fatal("unknown key: nil")
	}
	m.todoCur = 99
	if m.curRow() != nil || m.curTodo() != nil {
		t.Fatal("out of range: no row")
	}
}

func TestBehaviorJiraActionDisabled(t *testing.T) {
	m := bhTodoModel(t, false)
	_, cmd := m.keyNormal(bhRune('J'))
	if tm, ok := bhHasToast(cmd, "jira desligado"); !ok || !tm.err {
		t.Fatal("J with jira off explains")
	}
	_, cmd = m.keyNormal(bhRune('F'))
	if _, ok := bhHasToast(cmd, "jira desligado"); !ok {
		t.Fatal("F with jira off explains")
	}
}

func TestBehaviorJiraActionRows(t *testing.T) {
	m := bhTodoModel(t, true)
	// task already linked
	m.todoCur = 1
	if tm, ok := bhHasToast(m.jiraAction(), "#2 já tem issue: PROJ-7"); !ok || !tm.err {
		t.Fatal("a linked task says so")
	}
	// issue already tracked by a task
	m.todoCur = 2
	if _, ok := bhHasToast(m.jiraAction(), "PROJ-7 já está no todo #2"); !ok {
		t.Fatal("a tracked issue says so")
	}
	// untracked issue: added through the todo skill (missing here: error toast)
	m.todoCur = 3
	if tm, ok := bhHasToast(m.jiraAction(), ""); !ok || !tm.err {
		t.Fatal("adding without the todo skill fails with a toast")
	}
	// plain task: asks for project/type
	m.todoCur = 0
	m.keyNormal(bhRune('J'))
	if m.purpose != inJiraProject || m.input.Value() != "PROJ/Task" {
		t.Fatalf("J on a task asks for project/type: %q", m.input.Value())
	}
	m.input.SetValue(" vis / Bug ")
	if cmd := bhEnter(&m); cmd != nil {
		t.Fatal("creating asks first")
	}
	d, ok := m.top().(*confirmDialog)
	if !ok || !strings.Contains(d.q, "VIS/Bug") || !strings.Contains(d.q, "escrever testes") {
		t.Fatalf("confirmation names project/type and title: %+v", m.top())
	}
	m.pop()
	m.keyNormal(bhRune('J'))
	m.input.SetValue("vis")
	bhEnter(&m)
	if d := m.top().(*confirmDialog); !strings.Contains(d.q, "VIS/Task") {
		t.Fatalf("no type: the default one: %q", d.q)
	}
	m.pop()
	m.keyNormal(bhRune('J'))
	m.input.SetValue("")
	if bhEnter(&m) != nil || m.top() != nil {
		t.Fatal("empty project cancels")
	}
	// no rows
	empty := bhTodoModel(t, true)
	empty.todos = nil
	if empty.jiraAction() != nil {
		t.Fatal("no row: nothing")
	}
}

func TestBehaviorJiraFetchKey(t *testing.T) {
	m := bhTodoModel(t, true)
	_, cmd := m.keyNormal(bhRune('F'))
	if cmd == nil || !m.app.JiraLoading() {
		t.Fatal("F fetches")
	}
	if m.fetchJira(true) != nil {
		t.Fatal("one fetch at a time")
	}
	// the fetch ends with a fresh cache
	_, jf := bhFakes(&m)
	jf.Cache.FetchedAt, jf.Errs["Fetch"] = time.Now(), nil
	bhRun(cmd)
	if m.fetchJira(false) != nil {
		t.Fatal("a fresh cache is not refetched")
	}
}

func TestBehaviorJiraDescription(t *testing.T) {
	d := jiraDescription(todo.Task{ID: 3, Desc: "contexto",
		Refs:  []todo.Ref{{Kind: "pr", Value: "#9", URL: "https://pr"}},
		Links: []string{"https://doc"},
		Notes: []todo.Note{{At: "2026-10-01T10:00:00", Text: "nota"}, {At: "x", Text: "curta"}}})
	for _, want := range []string{"contexto\n\n", "- pr: #9 https://pr", "- https://doc", "> 2026-10-01 — nota", "> x — curta", "todo #3_"} {
		if !strings.Contains(d, want) {
			t.Errorf("description lacks %q:\n%s", want, d)
		}
	}
}

func TestBehaviorDispatchTodo(t *testing.T) {
	m := bhTodoModel(t, true)
	log := fakeClaude(t) // after bhEnv, which points claude at /bin/false
	repo := filepath.Join(t.TempDir(), "repo")
	if err := os.MkdirAll(repo, 0o755); err != nil {
		t.Fatal(err)
	}
	m.cfg.TodoRepos = map[string]string{"@repo": repo}
	m.todoCur = 0
	m.keyNormal(keyPress("enter"))
	if m.purpose != inDispatchDir || m.input.Value() != repo || m.draftTask == nil || m.draftTask.ID != 1 {
		t.Fatalf("enter on a task asks where: %q", m.input.Value())
	}
	cmd := bhEnter(&m)
	msgs := bhRun(cmd)
	if len(msgs) != 1 {
		t.Fatalf("dispatch: %v", msgs)
	}
	dm := msgs[0].(dispatchedMsg)
	if dm.err != nil || dm.id != "abcd1234" || dm.task == nil || dm.task.ID != 1 {
		t.Fatalf("dispatched: %+v", dm)
	}
	if b, _ := os.ReadFile(log); !strings.Contains(string(b), "--name todo #1 escrever testes") {
		t.Fatalf("args: %s", b)
	}
	// the dispatch result starts the task (todo skill missing here)
	got := m.update(dm)
	if _, ok := bhHasToast(got, "agent abcd1234 disparado"); !ok {
		t.Fatal("dispatch announced")
	}
	// a task without a repo starts at home
	m.todoCur = 1
	m.keyNormal(keyPress("enter"))
	if m.input.Value() != "~" {
		t.Fatalf("no repo: home: %q", m.input.Value())
	}
	m.input.SetValue("")
	if bhEnter(&m) != nil {
		t.Fatal("empty dir cancels")
	}
	// an issue goes to the jira agent dir
	m.todoCur = 3
	m.keyNormal(keyPress("enter"))
	if m.purpose != inJiraAgentDir || m.input.Value() != "~/visio/dev" || m.draftIssue.Key != "PROJ-8" {
		t.Fatalf("issue dispatch dir: %q", m.input.Value())
	}
	m.input.SetValue(t.TempDir())
	msgs = bhRun(bhEnter(&m))
	if len(msgs) != 1 || msgs[0].(dispatchedMsg).err != nil {
		t.Fatalf("issue dispatch: %v", msgs)
	}
	if b, _ := os.ReadFile(log); !strings.Contains(string(b), "--name PROJ-8 solta") {
		t.Fatalf("args: %s", b)
	}
	m.cfg.Jira.AgentDir = "/srv"
	m.keyNormal(keyPress("enter"))
	if m.input.Value() != "/srv" {
		t.Fatalf("configured agent dir: %q", m.input.Value())
	}
	m.input.SetValue("")
	if bhEnter(&m) != nil {
		t.Fatal("empty dir cancels")
	}
	// no row: nothing
	m.todos = nil
	if m.dispatchTodo() != nil {
		t.Fatal("no row: nothing to dispatch")
	}
}

func TestBehaviorTodoStartDoneAndBrowser(t *testing.T) {
	m := bhTodoModel(t, true)
	m.todoCur = 0
	_, cmd := m.keyNormal(bhRune('s'))
	msgs := bhRun(cmd)
	if len(msgs) != 1 {
		t.Fatalf("s starts the task: %v", msgs)
	}
	if tm, ok := msgs[0].(todosMsg); !ok || tm.err == nil {
		t.Fatalf("without the todo skill the reload fails: %+v", msgs[0])
	}
	m.update(msgs[0])
	if m.app.Todos().Error() == nil {
		t.Fatal("the error is kept for the view")
	}
	m.keyNormal(bhRune('D'))
	d, ok := m.top().(*confirmDialog)
	if !ok || !strings.Contains(d.q, "#1") {
		t.Fatal("D asks before marking done")
	}
	cmd = m.key(bhRune('y'))
	if msgs := bhRun(cmd); len(msgs) != 1 {
		t.Fatalf("done reloads: %v", msgs)
	}
	// o on an issue opens the browser (xdg-open missing here: error toast)
	m.todoCur = 3
	_, cmd = m.keyNormal(bhRune('o'))
	if tm, ok := bhHasToast(cmd, ""); !ok || !tm.err {
		t.Fatal("o on an issue tries the browser")
	}
	// on a jira row s/D do nothing
	if _, cmd := m.keyNormal(bhRune('s')); cmd != nil || m.top() != nil {
		t.Fatal("s on an issue does nothing")
	}
	m.keyNormal(bhRune('D'))
	if m.top() != nil {
		t.Fatal("D on an issue does nothing")
	}
	// todosMsg success replaces the tasks
	td, _ := bhFakes(&m)
	td.Groups, td.Errs["List"] = []todo.Group{{Group: "g", Tasks: []todo.Task{{ID: 9}}}}, nil
	m.update(todosCmd(m.app.LoadTodos)())
	if m.app.Todos().Error() != nil || len(m.app.Todos().Groups) != 1 || m.todos[0].task.ID != 9 {
		t.Fatal("todosMsg loads the tasks")
	}
}

func TestBehaviorTodosTabTriggersLoads(t *testing.T) {
	m := bhTodoModel(t, true)
	m.tab = tabSessions
	_, cmd := m.keyNormal(bhRune('3'))
	if !m.app.JiraLoading() {
		t.Fatal("opening todos fetches a stale jira cache")
	}
	bhRun(cmd) // the fetch ends (claude missing: the cache stays stale)
	m.tab = tabInbox
	m.keyNormal(tea.KeyPressMsg{Code: tea.KeyTab})
	if m.tab != tabTodos || !m.app.JiraLoading() {
		t.Fatal("tab into todos fetches too")
	}
	// n and m do nothing in todos
	m.keyNormal(bhRune('n'))
	m.keyNormal(bhRune('m'))
	if m.mode != modeNormal {
		t.Fatal("rename/move are for sessions")
	}
}
