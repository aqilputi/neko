package tui

import (
	"errors"
	"fmt"
	"image/color"
	"strings"
	"testing"
	"time"

	tea "charm.land/bubbletea/v2"

	"github.com/aqilputi/nekuroddo/internal/app"
	"github.com/aqilputi/nekuroddo/internal/cat"
	"github.com/aqilputi/nekuroddo/internal/claude"
	"github.com/aqilputi/nekuroddo/internal/core"
	"github.com/aqilputi/nekuroddo/internal/events"
	"github.com/aqilputi/nekuroddo/internal/jira"
	"github.com/aqilputi/nekuroddo/internal/store"
	"github.com/aqilputi/nekuroddo/internal/theme"
	"github.com/aqilputi/nekuroddo/internal/todo"
)

func bhUsage(pct int) claude.UsageReport {
	return claude.ParseUsage(fmt.Sprintf("Current session: %d%% used · resets Oct 15, 11:30am (America/Sao_Paulo)\nCurrent week (all models): 10%% used", pct))
}

func TestBehaviorWindowResizeDebounced(t *testing.T) {
	m := bhModel(t, bhSessions()...)
	p := bhPane(t, &m, "p1", "a")
	first := m.update(tea.WindowSizeMsg{Width: 100, Height: 30})
	second := m.update(tea.WindowSizeMsg{Width: 120, Height: 35})
	if m.w != 120 || m.h != 35 {
		t.Fatal("the size is taken right away")
	}
	stale := bhRun(first)
	if len(stale) != 1 || stale[0].(resizeMsg).seq != 1 {
		t.Fatalf("first resize tick: %v", stale)
	}
	if m.update(stale[0]) != nil {
		t.Fatal("a superseded resize is ignored")
	}
	cur := bhRun(second)
	m.update(cur[0])
	w, h := m.paneSize()
	if p.Height() != h || w <= 0 {
		t.Fatalf("the last resize reaches the panes: pane h %d want %d", p.Height(), h)
	}
}

func TestBehaviorAnimClock(t *testing.T) {
	m := bhModel(t, bhSessions()...)
	m.animGen = 3
	if m.update(animMsg{gen: 2}) != nil || m.frame != 0 {
		t.Fatal("an old clock is ignored")
	}
	m.animOn = true
	m.update(animMsg{gen: 3})
	if m.frame != 1 || m.animOn {
		t.Fatal("the current clock advances one frame and must be re-armed")
	}
	// busy sessions: fast clock (150ms) fires within the wait
	cmd := m.ensureAnim()
	msgs := bhRun(cmd)
	if len(msgs) != 1 || msgs[0].(animMsg).gen != 4 || !m.animOn {
		t.Fatalf("fast clock: %v", msgs)
	}
	if m.ensureAnim() != nil {
		t.Fatal("one clock at a time")
	}
	// nothing moving: slow clock (1s) does not fire within the wait
	idle := bhModel(t)
	if msgs := bhRun(idle.ensureAnim()); len(msgs) != 0 {
		t.Fatalf("slow clock fired early: %v", msgs)
	}
	// not sized yet: no clock
	zero := bhModel(t)
	zero.w = 0
	if zero.ensureAnim() != nil {
		t.Fatal("no clock before the first size")
	}
}

func TestBehaviorFocusAndBackground(t *testing.T) {
	m := bhModel(t)
	m.update(tea.BlurMsg{})
	if m.app.Focused() {
		t.Fatal("blur")
	}
	m.update(tea.FocusMsg{})
	if !m.app.Focused() {
		t.Fatal("focus")
	}
	m.update(tea.BackgroundColorMsg{Color: color.Black})
	if theme.All[m.th].Name != "catppuccin-mocha" {
		t.Fatalf("dark terminal: %s", theme.All[m.th].Name)
	}
	m.update(tea.BackgroundColorMsg{Color: color.White})
	if theme.All[m.th].Name != "rose-pine-dawn" {
		t.Fatalf("light terminal: %s", theme.All[m.th].Name)
	}
	m.themeSet = true
	m.update(tea.BackgroundColorMsg{Color: color.Black})
	if theme.All[m.th].Name != "rose-pine-dawn" {
		t.Fatal("a configured theme is kept")
	}
}

func TestBehaviorSnapAndRefresh(t *testing.T) {
	m := bhModel(t)
	if m.update(refreshMsg{}) != nil {
		t.Fatal("no second refresh while one runs")
	}
	if m.update(snapMsg{snap: store.Snapshot{Sessions: bhSessions()}}) != nil || len(m.rows) == 0 || m.refreshing {
		t.Fatal("snapMsg rebuilds and ends the refresh; nothing is scheduled")
	}
	if m.update(refreshMsg{}) == nil || !m.refreshing {
		t.Fatal("a refresh request refreshes")
	}
	m.refreshing = false
	if m.update(execDoneMsg{}) == nil {
		t.Fatal("coming back from fullscreen refreshes")
	}
}

// bhFollow subscribes to m's core as the program does (followBus); the
// function it returns hands m every followed event published since, as the
// TUI's listener would, and returns what they asked for.
func bhFollow(t *testing.T, m *Model) func() tea.Cmd {
	t.Helper()
	ch := m.app.Subscribe(t.Context())
	return func() tea.Cmd {
		var cmds []tea.Cmd
		for {
			select {
			case ev := <-ch:
				if follows(ev.Name) {
					cmds = append(cmds, m.onBus(ev))
				}
			default:
				return tea.Batch(cmds...)
			}
		}
	}
}

// The scheduler's refresh reaches the TUI as snapshot.changed: it draws the
// core's snapshot.
func TestBehaviorBusSnapshot(t *testing.T) {
	m := bhModel(t)
	feed := bhFollow(t, &m)
	ts := m.app.Ports().Sessions.(*testStore)
	ts.mu.Lock()
	ts.snap = &store.Snapshot{Sessions: bhSessions()}
	ts.mu.Unlock()
	m.app.Refresh() // as app.RunSchedule does
	if len(m.rows) != 0 {
		t.Fatal("nothing drawn before the event")
	}
	if feed() != nil || len(m.snap.Sessions) != len(bhSessions()) || len(m.rows) == 0 {
		t.Fatal("snapshot.changed draws the core's snapshot")
	}
}

// todos.changed and jira.changed rebuild the todos tab.
func TestBehaviorBusTodos(t *testing.T) {
	m := bhModel(t)
	feed := bhFollow(t, &m)
	bhSetTodos(t, &m, []todo.Group{{Group: "g", Tasks: []todo.Task{{ID: 1, Title: "x"}}}})
	if len(m.todos) != 0 {
		t.Fatal("nothing drawn before the event")
	}
	feed()
	if len(m.todos) != 1 {
		t.Fatalf("todos.changed rebuilds the rows: %v", m.todos)
	}
	m.app.Publish(app.EvJiraChanged, nil)
	m.todos = nil
	feed()
	if len(m.todos) != 1 {
		t.Fatal("jira.changed rebuilds the rows")
	}
	// redraw only: /usage, the ✉ cues, peer messages
	for _, name := range []string{app.EvUsageUpdated, app.EvMailCue, app.EvPeerMessage} {
		if m.onBus(core.Event{Name: name}) != nil {
			t.Fatalf("%s: only a redraw", name)
		}
	}
}

func TestBehaviorUsageMessages(t *testing.T) {
	m := bhModel(t)
	feed := bhFollow(t, &m)
	steps := []struct {
		pct    int
		toast  string // "" = no warning
		warned int
	}{
		{50, "", 0},
		{82, "janela de 5h em 82%", 80},
		{85, "", 80}, // same threshold: once
		{96, "janela de 5h em 96%", 95},
		{97, "", 95},
		{10, "", 0}, // new window: reset
		{81, "janela de 5h em 81%", 80},
	}
	for _, s := range steps {
		if m.update(usageMsg{rep: bhUsage(s.pct)}) != nil {
			t.Fatalf("%d%%: the report goes to app, nothing else", s.pct)
		}
		toasts := bhToasts(feed()) // the warning comes back as a notice
		if s.toast == "" && len(toasts) != 0 {
			t.Errorf("%d%%: unexpected toast %v", s.pct, toasts)
		}
		if s.toast != "" && (len(toasts) != 1 || !strings.Contains(toasts[0].text, s.toast) || !toasts[0].err) {
			t.Errorf("%d%%: want toast %q, got %v", s.pct, s.toast, toasts)
		}
		if m.app.UsageWarned() != s.warned {
			t.Errorf("%d%%: warned %d want %d", s.pct, m.app.UsageWarned(), s.warned)
		}
	}
	if !m.cat.Awake() {
		t.Fatal("the cat meows at a threshold")
	}
	if rep, err := m.app.Usage(); err != nil || rep.Raw == "" {
		t.Fatal("the report is kept")
	}
	m.update(usageMsg{err: errors.New("boom")})
	if rep, err := m.app.Usage(); err == nil || rep.Raw == "" {
		t.Fatal("an error keeps the last report")
	}
	// no session limit in the report: nothing to warn about
	off := bhModel(t)
	offFeed := bhFollow(t, &off)
	off.update(usageMsg{rep: claude.ParseUsage("nada")})
	if offFeed() != nil || off.cat.Awake() {
		t.Fatal("no limits")
	}
}

func TestBehaviorNoticesFromTheBus(t *testing.T) {
	m := bhModel(t, bhSessions()...)
	feed := bhFollow(t, &m)
	// the scheduler hands the hook events to app (app.RunSchedule)
	m.app.OnHookEvents([]events.Event{{Name: "Notification", Type: "permission_prompt", SessionID: "a", Message: "pode rodar?"}})
	feed()
	if e, _ := m.app.LastEvent("a"); !m.app.HooksSeen() || e.Message != "pode rodar?" || !m.cat.Awake() {
		t.Fatal("a notification is recorded and meows")
	}
	// away from the terminal: bell + desktop notification (notify-send is not on PATH here)
	m.app.SetFocused(false)
	m.app.OnHookEvents([]events.Event{{Name: "Notification", SessionID: "b"}})
	var raw bool
	for _, msg := range bhRun(feed()) {
		if r, ok := msg.(tea.RawMsg); ok && r.Msg == "\a" {
			raw = true
		}
	}
	if !raw {
		t.Fatal("away: rings the bell")
	}
	// events that do not need you only refresh (the scheduler's)
	n := bhModel(t, bhSessions()...)
	nFeed := bhFollow(t, &n)
	n.app.OnHookEvents([]events.Event{{Name: "PostToolUse", SessionID: "a"}})
	bhRun(nFeed())
	if n.cat.Awake() {
		t.Fatal("a tool event is no reason to meow")
	}
	// a malformed notice is ignored
	if n.onBus(core.Event{Name: app.EvNotice, Data: "?"}) != nil {
		t.Fatal("not a Notice")
	}
}

func TestBehaviorNotice(t *testing.T) {
	m := bhModel(t, bhSessions()...) // the core saw these states first (setSnap)
	feed := bhFollow(t, &m)          // the refresh's notices come on the bus
	if m.update(bhRefreshMsg(t, &m)); feed() != nil || m.cat.Awake() {
		t.Fatal("the first snapshot only records states")
	}
	ss := m.app.Ports().Sessions.(*testStore).snap.Sessions // what the refresh reads
	cases := []struct {
		name   string
		change func(ss []store.Session)
		meow   bool
	}{
		{"unchanged", func([]store.Session) {}, false},
		{"busy → idle interactive", func(ss []store.Session) { ss[1].State = "idle" }, true},
		{"blocked → busy", func(ss []store.Session) { ss[0].State = "busy" }, false},
		{"busy → blocked", func(ss []store.Session) { ss[0].State = "blocked" }, true},
		{"two return", func(ss []store.Session) { ss[0].State = "done"; ss[1].State = "blocked" }, true},
	}
	for _, c := range cases {
		c.change(ss)
		m.cat = cat.New() // a fresh cat for each case
		m.update(bhRefreshMsg(t, &m))
		feed()
		if m.cat.Awake() != c.meow {
			t.Errorf("%s: meow=%v want %v", c.name, m.cat.Awake(), c.meow)
		}
	}
	// away: bell and desktop notification
	m.app.SetFocused(false)
	ss[2].State = "blocked"
	m.update(bhRefreshMsg(t, &m))
	cmd := feed()
	if cmd == nil {
		t.Fatal("away from the terminal a return rings")
	}
}

func TestBehaviorTodosAndJiraMessages(t *testing.T) {
	m := bhTodoModel(t, true)
	// each jiraMsg comes from a real fetch through the core
	_, jf := bhFakes(&m)
	fetched := func() tea.Msg {
		msgs := bhRun(m.fetchJira(true))
		if len(msgs) != 1 {
			t.Fatalf("fetch: %v", msgs)
		}
		return msgs[0]
	}
	jf.Errs["Fetch"] = errors.New("sem rede")
	if tm, ok := bhHasToast(m.update(fetched()), "jira: sem rede"); !ok || !tm.err {
		t.Fatal("jira errors are shown")
	}
	if m.app.JiraErr() == nil || m.app.JiraLoading() {
		t.Fatal("error kept, loading over")
	}
	jf.Errs["Fetch"] = nil
	jf.Cache = jira.Cache{CostUSD: 0.12, Issues: []jira.Issue{{Key: "N-1", Updated: time.Now().Format("2006-01-02T15:04:05.000-0700")}}}
	cmd := m.update(fetched())
	if _, ok := bhHasToast(cmd, "jira: 1 issues (custou $0.12)"); !ok {
		t.Fatal("jira result announced")
	}
	if m.app.JiraErr() != nil || m.app.JiraLoading() || len(m.todos) != 3 {
		t.Fatalf("todos rebuilt with the issue: %d", len(m.todos))
	}
}

func TestBehaviorJiraCreatedMessages(t *testing.T) {
	m := bhTodoModel(t, true)
	m.cfg.Jira.ApproveHint = "nk approve"
	tm, ok := bhHasToast(m.update(jiraCreatedMsg{err: jira.DeniedError{Reason: "hook"}}), "rode:  nk approve")
	if !ok || tm.d != 30*time.Second {
		t.Fatal("a denied write shows the approve hint, sticky")
	}
	m.cfg.Jira.ApproveHint = ""
	tm, ok = bhHasToast(m.update(jiraCreatedMsg{err: jira.DeniedError{Reason: "hook"}}), "jira-last.jsonl")
	if !ok || tm.d != 30*time.Second {
		t.Fatal("without a hint the error points to the log")
	}
	cmd := m.update(jiraCreatedMsg{task: 4, key: "PROJ-9", cost: 0.3})
	if !m.app.JiraLoading() { // before running cmd, which ends the fetch
		t.Fatal("creating refetches jira")
	}
	if _, ok := bhHasToast(cmd, "PROJ-9 criada e ligada ao #4 ($0.30)"); !ok {
		t.Fatal("created and linked")
	}
}

func TestBehaviorToastLifecycle(t *testing.T) {
	m := bhModel(t)
	m.update(toastMsg{text: "um"})
	first := m.toastSeq
	cmd := m.update(toastMsg{text: "dois", err: true, d: time.Millisecond})
	if m.toast != "dois" || !m.toastErr || m.toastSeq != first+1 {
		t.Fatal("the newest toast shows")
	}
	m.update(clearToastMsg{seq: first})
	if m.toast != "dois" {
		t.Fatal("an older toast's timer does not clear the newer one")
	}
	msgs := bhRun(cmd)
	if len(msgs) != 1 {
		t.Fatalf("custom duration: %v", msgs)
	}
	m.update(msgs[0])
	if m.toast != "" {
		t.Fatal("its own timer clears it")
	}
	if msg := stickyToast("x")().(toastMsg); msg.d != 30*time.Second || !msg.err {
		t.Fatal("sticky toasts last 30s")
	}
}

func TestBehaviorPaneMessages(t *testing.T) {
	m := bhModel(t, bhSessions()...)
	if m.update(paneOutputMsg{ID: "nope"}) != nil {
		t.Fatal("output from an unknown pane is ignored")
	}
	bhPane(t, &m, "p1", "a")
	if m.update(paneOutputMsg{ID: "p1"}) == nil {
		t.Fatal("output keeps listening")
	}
	if _, ok := bhHasToast(m.update(enterMsg{pane: "gone"}), "resposta enviada"); !ok {
		t.Fatal("enter toast even if the pane closed meanwhile")
	}
	if tm, ok := bhHasToast(m.update(dispatchedMsg{err: errors.New("falhou")}), "falhou"); !ok || !tm.err {
		t.Fatal("a failed dispatch is shown")
	}
	cmd := m.update(dispatchedMsg{id: "x1", task: &todo.Task{ID: 3}})
	var todos bool
	for _, msg := range bhRun(cmd) {
		if _, ok := msg.(todosMsg); ok {
			todos = true
		}
	}
	if !todos {
		t.Fatal("a dispatched task is started and the list reloaded")
	}
}

func TestBehaviorInit(t *testing.T) {
	m := bhModel(t)
	if m.Init() == nil {
		t.Fatal("Init asks the terminal's background")
	}
	for _, msg := range bhRun(m.Init()) {
		if _, ok := msg.(snapMsg); ok {
			t.Fatal("Init leaves the refreshes to the scheduler")
		}
	}
}

func TestBehaviorFetchGuards(t *testing.T) {
	m := bhModel(t)
	if m.fetchUsage() == nil {
		t.Fatal("u always fetches")
	}
	if m.fetchJira(true) != nil {
		t.Fatal("jira off: no fetch")
	}
	if m.loadTodos() != nil {
		t.Fatal("no todo skill: nothing to load")
	}
}

func TestBehaviorWindowPctAndText(t *testing.T) {
	m := bhModel(t)
	s := &store.Session{WindowCost: 2}
	if m.windowPct(s) != 0 {
		t.Fatal("no usage report: 0")
	}
	m.app.OnUsage(bhUsage(40), nil)
	if m.windowPct(s) != 0 {
		t.Fatal("nothing in the snapshot used the window: 0")
	}
	m.snap.Sessions = []store.Session{{WindowCost: 2}, {WindowCost: 6}}
	if got := m.windowPct(&m.snap.Sessions[1]); got != 30 {
		t.Fatalf("share: %v", got)
	}
	for _, c := range []struct {
		p    float64
		want string
	}{{0, ""}, {-1, ""}, {0.2, "<1%"}, {1, "1%"}, {12.4, "12%"}, {99.6, "100%"}} {
		if got := core.PctText(c.p); got != c.want {
			t.Errorf("core.PctText(%v)=%q want %q", c.p, got, c.want)
		}
	}
}
