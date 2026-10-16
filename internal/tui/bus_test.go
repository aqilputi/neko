package tui

import (
	"context"
	"testing"

	tea "charm.land/bubbletea/v2"

	"github.com/aqilputi/nekuroddo/internal/app"
	"github.com/aqilputi/nekuroddo/internal/config"
	"github.com/aqilputi/nekuroddo/internal/store"
)

// The scheduler starts once, at the first size, after the TUI subscribed:
// what its first refresh publishes reaches the TUI.
func TestFirstSizeStartsTheSchedulerAfterSubscribing(t *testing.T) {
	m := newTestModel(config.Default())
	started := 0
	a := m.app
	m = m.WithScheduler(func() {
		started++
		a.Publish(app.EvMailCue, app.MailCueEvent{ID: "x"}) // as the first refresh would
	})
	var got []busMsg
	for _, msg := range bhRun(m.update(tea.WindowSizeMsg{Width: 80, Height: 20})) {
		if b, ok := msg.(busMsg); ok {
			got = append(got, b)
		}
	}
	if started != 1 || len(got) != 1 || got[0].ev.Name != app.EvMailCue {
		t.Fatalf("started %d, heard %+v", started, got)
	}
	m.update(tea.WindowSizeMsg{Width: 90, Height: 20})
	if m.update(busRetryMsg{}) == nil || started != 1 {
		t.Fatal("the scheduler starts once")
	}
}

// A subscription the bus cut (the TUI fell behind) is followed again after
// a pause, catching up with the core's snapshot first.
func TestBusCutFollowsAgain(t *testing.T) {
	m := bhModel(t)
	if msgs := bhRun(m.update(busCutMsg{})); len(msgs) != 1 {
		t.Fatalf("a pause, then retry: %v", msgs)
	} else if _, ok := msgs[0].(busRetryMsg); !ok {
		t.Fatalf("retry: %T", msgs[0])
	}
	ts := m.app.Ports().Sessions.(*testStore)
	ts.mu.Lock()
	ts.snap = &store.Snapshot{Sessions: bhSessions()}
	ts.mu.Unlock()
	m.app.Refresh() // missed while cut
	if m.update(busRetryMsg{}) == nil || len(m.rows) == 0 {
		t.Fatal("the retry catches up and follows the bus again")
	}
}

// A busMsg is handled and the listening goes on.
func TestBusMsgKeepsListening(t *testing.T) {
	m := bhModel(t)
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	ch := m.app.Subscribe(ctx)
	m.app.Publish(app.EvUsageUpdated, app.UsageUpdated{})
	msg, ok := listenBus(ch)().(busMsg)
	if !ok {
		t.Fatal("usage.updated is followed")
	}
	if m.update(msg) == nil {
		t.Fatal("keeps listening")
	}
}

// Every pane the TUI starts goes to the pane hook (nk --listen: the RPC
// server), with its session and size.
func TestPaneHookGetsTheProcess(t *testing.T) {
	m := bhModel(t)
	var (
		got        app.Pane
		sid        string
		cols, rows int
	)
	m = m.WithPaneHook(func(p app.Pane, sessionID string, c, r int) { got, sid, cols, rows = p, sessionID, c, r })
	if m.showPlan(app.AttachPlan{SessionID: "s9", ID: "p9", Dir: "/", Argv: []string{"/bin/sleep", "5"}}) == nil {
		t.Fatal("no pane")
	}
	defer m.closePane("p9")
	w, h := m.paneSize()
	if got == nil || got.ID() != "p9" || sid != "s9" || cols != w || rows != h {
		t.Fatalf("hook: %v %q %dx%d", got, sid, cols, rows)
	}
	if got.Done() {
		t.Fatal("the hook gets the running process")
	}
	// no hook: nothing to call
	n := bhModel(t)
	if n.showPlan(app.AttachPlan{SessionID: "s8", ID: "p8", Dir: "/", Argv: []string{"/bin/sleep", "5"}}) == nil {
		t.Fatal("no pane")
	}
	n.closePane("p8")
}

// The Lua UI events (nk.ui.cat.meow, nk.ui.palette_open) come on the bus
// like the core's and are drawn by lua.go.
func TestBusFollowsLuaUIEvents(t *testing.T) {
	m := bhModel(t)
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	ch := m.app.Subscribe(ctx)
	m.app.Publish("toast", app.ToastData{Text: "ignorado"})
	m.app.Publish(luaEvMeow, map[string]string{"text": "olá"})
	msg, ok := listenBus(ch)().(busMsg)
	if !ok || msg.ev.Name != luaEvMeow {
		t.Fatalf("listen = %+v", msg)
	}
	m.update(msg)
	if !m.cat.Awake() {
		t.Fatal("the cat says the Lua text")
	}
}

// lua.messages' answer opens the "últimos erros lua" dialog (lua.go)
// instead of the usual toast.
func TestLuaMessagesCommandOpensTheDialog(t *testing.T) {
	m := bhModel(t)
	res := app.Result{Toast: "lua: 1 mensagem", Data: []map[string]any{{"level": "error", "text": "boom"}}}
	if m.commandResult("lua.messages", res, nil) != nil {
		t.Fatal("the dialog is the answer: no toast")
	}
	if m.top() == nil {
		t.Fatal("dialog")
	}
}
