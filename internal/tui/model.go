// Package tui is nekuroddo's Bubble Tea program: a vim-driven tree of Claude Code
// sessions grouped by context, an inbox of agents waiting on you, the todo
// list, and an embedded pane running the real `claude attach`.
package tui

import (
	"fmt"
	"strings"
	"time"

	tea "charm.land/bubbletea/v2"

	"github.com/aqilputi/nekuroddo/internal/app"
	"github.com/aqilputi/nekuroddo/internal/cat"
	"github.com/aqilputi/nekuroddo/internal/claude"
	"github.com/aqilputi/nekuroddo/internal/config"
	"github.com/aqilputi/nekuroddo/internal/core"
	"github.com/aqilputi/nekuroddo/internal/jira"
	"github.com/aqilputi/nekuroddo/internal/store"
	"github.com/aqilputi/nekuroddo/internal/theme"
	"github.com/aqilputi/nekuroddo/internal/todo"
)

type tab int

const (
	tabSessions tab = iota
	tabInbox
	tabTodos
	tabArchive
	numTabs
)

type mode int

const (
	modeNormal mode = iota
	modeInsert      // keys go to the embedded claude
	modeInput       // a prompt line at the bottom
)

// What the bottom input line is collecting.
type inputPurpose int

const (
	inReply inputPurpose = iota
	inJiraProject
	inJiraAgentDir
	inNewDir
	inNewPrompt
	inDispatchDir
	inFilter
	inRename    // draftDir = sessionId
	inRenameCtx // draftDir = the context as shown
	inMove      // draftDir = sessionId
)

// todoRow is one line of the todos tab: a task or a Jira issue.
type todoRow struct {
	group string
	task  *todo.Task
	issue *jira.Issue
}

// Model is the TUI: shared view state here, and one embedded struct per slice
// (wave 2 of docs/plano-arquitetura.md), each declared in the slice's file.
type Model struct {
	cfg config.Config
	app *app.App // the core: every use case and all domain state live there
	lua Lua      // the Lua runtime, or nil
	th  int

	w, h int
	tab  tab
	mode mode

	// snap is the core's snapshot as the bus (snapshot.changed) or the
	// last refresh the TUI asked for handed it over; after a session command
	// (archive, rename…) pullSnap reads it again to show the change at once.
	snap       store.Snapshot
	refreshing bool // a refresh the TUI asked for (R, back from fullscreen…) runs

	// The tree and the inbox: client-only view state (filter, fold, frozen
	// order, cursor, sort, context colors) and the rows core.BuildTree made.
	view    core.ViewState
	rows    []core.Row
	lastTab tab // rows depend on the tab (archive vs the rest)

	dialogs []dialog // drawn over the UI; the top one gets the keys

	pendingG  bool
	pendingD  bool
	toast     string
	toastErr  bool
	toastSeq  int
	lastCtrlC time.Time

	cat cat.Model

	themeSet  bool // theme came from config; otherwise follow the terminal
	resizeSeq int

	// one animation clock (spinners, blinking, the cat)
	frame   int
	animOn  bool
	animGen int

	// Each slice keeps its state in a struct declared in its own file, so a
	// slice's work never touches this one (fields are promoted: m.picked…).
	groupsSlice   // groups.go
	sessionsSlice // sessions.go
	inputSlice    // input.go
	paneSlice     // pane.go
	todoJiraSlice // todo_jira.go
	notifySlice   // notify.go
}

// Lua is what the TUI uses of the Lua runtime (*lua.Runtime): ":lua <code>"
// in the palette, and the key overrides laid over the core's keymap.
type Lua interface {
	Eval(code string) (string, error)
	Keymap() core.Keymap
	RunBinding(ctx app.Context, command string) (app.Result, error)
}

// New builds the TUI over a core the caller composed (cmd/nekuroddo: the
// real adapters, the builtin commands, Lua); cfg is the presentation's
// config (theme, sort, refresh), normally a.Config().
func New(a *app.App, cfg config.Config) Model {
	return Model{
		cfg: cfg, app: a, th: theme.ByName(cfg.Theme),
		view: core.NewViewState(cfg.Sort), cat: cat.New(), themeSet: cfg.Theme != "",
		groupsSlice:   newGroupsSlice(),
		sessionsSlice: newSessionsSlice(),
		inputSlice:    newInputSlice(cfg),
		paneSlice:     newPaneSlice(),
		todoJiraSlice: newTodoJiraSlice(a),
		notifySlice:   newNotifySlice(),
	}
}

// WithLua gives the TUI the Lua runtime (nil: no Lua).
func (m Model) WithLua(rt Lua) Model {
	m.lua = rt
	return m
}

func (m Model) t() theme.Theme { return theme.All[m.th] }

// ---- messages ----

// snapMsg is a finished refresh the TUI asked for: the core's new snapshot
// (its notices come on the bus, like the scheduler's).
type snapMsg struct{ snap store.Snapshot }

// refreshMsg asks for a refresh now (R, a command that changed sessions).
type refreshMsg struct{}

// todosMsg says the core reloaded the todo list (app.Todos has it); err is
// that load's error.
type todosMsg struct{ err error }
type toastMsg struct {
	text string
	err  bool
	d    time.Duration
}
type dispatchedMsg struct {
	id   string
	task *todo.Task
	err  error
}
type replyMsg struct{ pane string }

// usageMsg is the /usage report `u` fetched (the scheduler fetches the
// periodic ones itself).
type usageMsg struct {
	rep claude.UsageReport
	err error
}
type jiraMsg struct {
	cache jira.Cache
	err   error
}
type jiraCreatedMsg struct {
	task int
	key  string
	cost float64
	err  error
}
type paneOpenMsg struct {
	sess store.Session
	id   string
	err  error
	text string // reply to send once attached
}

// Init loads the todos and asks the terminal's background. The refreshes,
// the hook log and /usage are the core's scheduler's (app.RunSchedule,
// started with WithScheduler once the TUI follows the bus); the TUI only
// draws what it publishes.
func (m Model) Init() tea.Cmd {
	return tea.Batch(m.loadTodos(), tea.RequestBackgroundColor)
}

// WithScheduler gives the TUI what starts the core's scheduler (cmd: a
// goroutine running app.RunSchedule). It runs once, right after the TUI
// subscribed to the bus (its first window size), so no event of the first
// refresh is missed. Without one nothing refreshes by itself (tests).
func (m Model) WithScheduler(start func()) Model {
	m.startSchedule = start
	return m
}

// refresh asks the core for a new snapshot now (app.RefreshNotices, off the
// UI goroutine): R, back from fullscreen, a command that changed sessions.
// One at a time: a refresh asked meanwhile is dropped (the scheduler's next
// one comes soon anyway). Its notices go out on the bus.
func (m *Model) refresh() tea.Cmd {
	if m.refreshing {
		return nil
	}
	m.refreshing = true
	a := m.app
	return func() tea.Msg { return snapMsg{snap: a.Refresh()} }
}

func toastCmd(text string, err bool) tea.Cmd {
	return func() tea.Msg { return toastMsg{text: text, err: err} }
}

// stickyToast stays for long enough to copy a command from it.
func stickyToast(text string) tea.Cmd {
	return func() tea.Msg { return toastMsg{text: text, err: true, d: 30 * time.Second} }
}

// ---- data → rows ----

// rebuild lays the snapshot out as rows (core.BuildTree), keeping the cursor
// on the row it was on.
func (m *Model) rebuild() {
	m.view = m.view.WithCursor(m.rows, m.view.Cursor)
	m.view.Tab = core.Tab(m.tab)
	m.rows, m.view = core.BuildTree(m.snap.Sessions, m.view)
}

func (m *Model) cur() *core.Row {
	if m.view.Cursor >= 0 && m.view.Cursor < len(m.rows) {
		return &m.rows[m.view.Cursor]
	}
	return nil
}

// selected is the session under the cursor in the current tab.
func (m *Model) selected() *store.Session {
	switch m.tab {
	case tabInbox:
		if m.view.InboxCursor < len(m.view.Inbox) {
			return m.view.Inbox[m.view.InboxCursor]
		}
	case tabSessions, tabArchive:
		if r := m.cur(); r != nil && r.Sess != nil {
			return r.Sess
		}
	}
	return nil
}

// ExitBanner reminds what keeps running after nekuroddo quits.
func (m Model) ExitBanner() string {
	var b strings.Builder
	for _, s := range m.snap.Sessions {
		if s.ShortID != "" && (s.State == "busy" || s.Attention == store.AttnBlocked) {
			fmt.Fprintf(&b, "  %-9s claude attach %s   %s\n", s.State, s.ShortID, core.Truncate(s.Label(), 50))
		}
	}
	if b.Len() == 0 {
		return "ᓚᘏᗢ até mais\n"
	}
	return "ᓚᘏᗢ seguem em background:\n" + b.String()
}

// windowPct estimates how much of the current 5-hour window a session used
// (core.WindowPct over the /usage session limit; 0 when unknown).
func (m *Model) windowPct(s *store.Session) float64 {
	pct := 0
	if l, ok := m.usage().Find("session"); ok {
		pct = l.Pct
	}
	costs := make([]float64, len(m.snap.Sessions))
	for i := range m.snap.Sessions {
		costs[i] = m.snap.Sessions[i].WindowCost
	}
	return core.WindowPct(pct, s.WindowCost, costs)
}
