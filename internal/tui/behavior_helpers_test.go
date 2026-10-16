package tui

import (
	"errors"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"testing"
	"time"

	tea "charm.land/bubbletea/v2"

	"github.com/aqilputi/nekuroddo/internal/app"
	"github.com/aqilputi/nekuroddo/internal/app/apptest"
	"github.com/aqilputi/nekuroddo/internal/config"
	"github.com/aqilputi/nekuroddo/internal/core"
	"github.com/aqilputi/nekuroddo/internal/events"
	"github.com/aqilputi/nekuroddo/internal/groups"
	"github.com/aqilputi/nekuroddo/internal/jira"
	"github.com/aqilputi/nekuroddo/internal/store"
	"github.com/aqilputi/nekuroddo/internal/term"
	"github.com/aqilputi/nekuroddo/internal/todo"
	"github.com/aqilputi/nekuroddo/internal/wire"
)

// bhEnv isolates a test from the real machine: state, config, cache and HOME
// live in temp dirs, claude is /bin/false, the todo skill does not exist and
// PATH has nothing on it (no notify-send, xdg-open or python3 runs for real).
func bhEnv(t *testing.T) {
	t.Helper()
	root := t.TempDir()
	for _, kv := range [][2]string{
		{"HOME", "home"}, {"XDG_STATE_HOME", "state"}, {"XDG_CONFIG_HOME", "config"},
		{"XDG_CACHE_HOME", "cache"}, {"PATH", "bin"},
	} {
		d := filepath.Join(root, kv[1])
		if err := os.MkdirAll(d, 0o755); err != nil {
			t.Fatal(err)
		}
		t.Setenv(kv[0], d)
	}
	t.Setenv("NEKO_CLAUDE", "/bin/false")
	t.Setenv("NEKO_TODO_SCRIPT", filepath.Join(root, "nope", "todo.py"))
	t.Setenv("CLAUDE_CONFIG_DIR", filepath.Join(root, "claude"))
}

// testStore is the real session store, except that Refresh returns the
// snapshot a test chose (setSnap): a real app.Refresh then hands it to the
// core and the TUI alike, as in the program.
type testStore struct {
	*store.Store
	mu   sync.Mutex
	snap *store.Snapshot
}

func (s *testStore) Refresh() store.Snapshot {
	s.mu.Lock()
	snap := s.snap
	s.mu.Unlock()
	if snap == nil {
		return s.Store.Refresh()
	}
	return *snap
}

// newTestModel is New (the real adapters, every builtin command) with the
// session store wrapped in a testStore; edit swaps other ports (fakes).
func newTestModel(cfg config.Config, edit ...func(*app.Ports)) Model {
	p := wire.Default(cfg)
	p.Sessions = &testStore{Store: p.Sessions.(*store.Store)}
	for _, e := range edit {
		e(&p)
	}
	a := app.New(cfg, p)
	if err := a.RegisterBuiltins(); err != nil {
		panic(err)
	}
	return New(a, cfg)
}

// setSnap makes snap the core's snapshot through a real refresh
// (app.Refresh) and the TUI's: they share its sessions, as in the program.
func setSnap(m *Model, snap store.Snapshot) {
	ts := m.app.Ports().Sessions.(*testStore)
	ts.mu.Lock()
	ts.snap = &snap
	ts.mu.Unlock()
	m.snap = m.app.Refresh()
}

// bhRefreshMsg runs the TUI's own refresh (app.RefreshNotices) and returns
// the message it makes, not yet handled.
func bhRefreshMsg(t *testing.T, m *Model) snapMsg {
	t.Helper()
	m.refreshing = false
	cmd := m.refresh()
	if cmd == nil {
		t.Fatal("no refresh")
	}
	return cmd().(snapMsg)
}

// errNoSkill and errNoClaude are what the fakes answer where the real todo
// skill and claude are missing (bhEnv).
var (
	errNoSkill  = errors.New("todo: skill não encontrada")
	errNoClaude = errors.New("claude: exit status 1")
)

// bhFakes are the todo and Jira ports of a bhModel: fakes answering as if
// the todo skill and claude were missing until a test says otherwise.
func bhFakes(m *Model) (*apptest.FakeTodo, *apptest.FakeJira) {
	p := m.app.Ports()
	return p.Todo.(*apptest.FakeTodo), p.Jira.(*apptest.FakeJira)
}

func withFakeTodoJira(p *app.Ports) {
	td := &apptest.FakeTodo{Errs: map[string]error{}}
	for _, op := range []string{"List", "Start", "Done", "LinkJira", "AddFromJira", "AddNote"} {
		td.Errs[op] = errNoSkill
	}
	p.Todo = td
	p.Jira = &apptest.FakeJira{Errs: map[string]error{"Fetch": errNoClaude, "Create": errNoClaude}}
}

// bhSetTodos makes groups the core's todo list (a real app.LoadTodos), then
// the skill goes missing again.
func bhSetTodos(t *testing.T, m *Model, groups []todo.Group) {
	t.Helper()
	td, _ := bhFakes(m)
	td.Groups, td.Errs["List"] = groups, nil
	if _, err := m.app.LoadTodos(); err != nil {
		t.Fatal(err)
	}
	td.Errs["List"] = errNoSkill
}

// bhSetJira makes c the core's Jira cache (a real fetch, which needs Jira
// on), then claude goes missing again.
func bhSetJira(t *testing.T, m *Model, c jira.Cache) {
	t.Helper()
	_, jf := bhFakes(m)
	jf.Cache, jf.Errs["Fetch"] = c, nil
	run := m.app.BeginJiraFetch(true)
	if run == nil {
		t.Fatal("jira is off")
	}
	if _, err := run(); err != nil {
		t.Fatal(err)
	}
	jf.Errs["Fetch"] = errNoClaude
}

// bhSaveGroup adds members to group tag on disk (as `:group` in another
// nekuroddo would) and has the core reload the groups.
func bhSaveGroup(t *testing.T, m *Model, tag string, ms ...groups.Member) {
	t.Helper()
	g := groups.Load()
	g.Add(tag, ms...)
	if err := g.Save(); err != nil {
		t.Fatal(err)
	}
	m.app.ReloadGroups()
}

// cue puts a ✉ next to session id through the core (app.OnPeer, as a hook
// event would): received at at, or sent by id when sent.
func cue(t *testing.T, m *Model, id string, at time.Time, sent bool) {
	t.Helper()
	e := events.Event{At: at, Type: "peer", SessionID: id}
	if sent {
		s := m.session(id)
		if s == nil {
			t.Fatalf("no session %s", id)
		}
		e.SessionID, e.From = "someone-else", s.Label()
	}
	m.app.OnPeer(e)
}

// bhModelCfg builds a sized model over sessions. refreshing is left on so
// commands built from it never spawn a refresh by themselves.
func bhModelCfg(t *testing.T, cfg config.Config, sessions ...store.Session) Model {
	t.Helper()
	bhEnv(t)
	m := newTestModel(cfg, withFakeTodoJira)
	m.w, m.h = 140, 40
	setSnap(&m, store.Snapshot{Sessions: sessions})
	m.refreshing = true
	m.rebuild()
	return m
}

func bhModel(t *testing.T, sessions ...store.Session) Model {
	t.Helper()
	cfg := config.Default()
	cfg.UsageRefresh = 0
	return bhModelCfg(t, cfg, sessions...)
}

// bhSessions: two contexts; "a" waits on you, "b" is busy with subagents,
// "c" returned, "d" is stale.
func bhSessions() []store.Session {
	now := time.Now()
	return []store.Session{
		{SessionID: "a", ShortID: "aaaa1111", Name: "alpha", Context: "c1", Kind: "background", State: "blocked",
			Attention: store.AttnBlocked, LastAt: now, Cwd: "/tmp"},
		{SessionID: "b", Name: "bravo", Context: "c1", Kind: "interactive", State: "busy", PID: 42, LastAt: now.Add(-time.Minute), Cwd: "/tmp",
			Subs: []store.Sub{{ID: "s1"}, {ID: "s2", Done: true}}},
		{SessionID: "c", Name: "charlie", Context: "c2", Kind: "history", State: "done",
			Attention: store.AttnReturned, LastAt: now.Add(-2 * time.Minute), Cwd: "/tmp"},
		{SessionID: "d", Name: "delta", Context: "c2", Kind: "history", State: "idle", Stale: true,
			LastAt: now.Add(-100 * time.Hour), Cwd: "/tmp"},
	}
}

// bhRun executes cmd, flattening batches, and returns what came back within
// a short wait (ticks longer than that are left behind).
func bhRun(cmd tea.Cmd) []tea.Msg {
	if cmd == nil {
		return nil
	}
	ch := make(chan tea.Msg, 1)
	go func() { ch <- cmd() }()
	var msg tea.Msg
	select {
	case msg = <-ch:
	case <-time.After(300 * time.Millisecond):
		return nil
	}
	if b, ok := msg.(tea.BatchMsg); ok {
		var out []tea.Msg
		for _, c := range b {
			out = append(out, bhRun(c)...)
		}
		return out
	}
	if msg == nil {
		return nil
	}
	return []tea.Msg{msg}
}

func bhToasts(cmd tea.Cmd) []toastMsg {
	var out []toastMsg
	for _, m := range bhRun(cmd) {
		if t, ok := m.(toastMsg); ok {
			out = append(out, t)
		}
	}
	return out
}

// bhHasToast reports whether cmd yields a toast containing sub.
func bhHasToast(cmd tea.Cmd, sub string) (toastMsg, bool) {
	for _, t := range bhToasts(cmd) {
		if strings.Contains(t.text, sub) {
			return t, true
		}
	}
	return toastMsg{}, false
}

func bhRune(r rune) tea.KeyPressMsg { return tea.KeyPressMsg{Code: r, Text: string(r)} }

// bhType types s into whatever has the keys.
func bhType(m *Model, s string) {
	for _, r := range s {
		m.key(bhRune(r))
	}
}

func bhEnter(m *Model) tea.Cmd { return m.key(tea.KeyPressMsg{Code: tea.KeyEnter}) }

// bhSelect puts the cursor on the session row with that id.
func bhSelect(t *testing.T, m *Model, id string) {
	t.Helper()
	for i, r := range m.rows {
		if r.Kind == core.RowSess && r.Sess.SessionID == id {
			m.view.Cursor = i
			return
		}
	}
	t.Fatalf("no row for session %s in %v", id, sessOrder(m))
}

// bhPane starts a harmless long-lived pane registered under id for session sid.
func bhPane(t *testing.T, m *Model, id, sid string) *term.Pane {
	t.Helper()
	p, err := term.Start(id, id, "/", 40, 10, "/bin/sleep", "30")
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(p.Close)
	m.panes[id] = p
	if sid != "" {
		m.app.Panes().Started(app.AttachPlan{SessionID: sid, ID: id}) // the core binds the session to it
	}
	return p
}

func bhKinds(m *Model) []string {
	var out []string
	for _, r := range m.rows {
		out = append(out, core.RowKey(r))
	}
	return out
}
