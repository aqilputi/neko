// Package ui is nekuroddo's Bubble Tea program: a vim-driven tree of Claude Code
// sessions grouped by context, an inbox of agents waiting on you, the todo
// list, and an embedded pane running the real `claude attach`.
package ui

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
	"github.com/aqilputi/nekuroddo/internal/wire"
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
	th  int

	w, h int
	tab  tab
	mode mode

	// snap is the core's snapshot as the last refresh (app.RefreshNotices)
	// handed it over; its sessions are shared with the core, so the session
	// commands' in-place changes (archive, rename…) show at once.
	snap       store.Snapshot
	refreshing bool

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

// New builds the TUI over the real world: the core over wire.Default's
// adapters, with every builtin command registered. This is the only place
// internal/ui reaches internal/wire (internal/archtest checks).
func New(cfg config.Config) Model {
	a := app.New(cfg, wire.Default(cfg))
	if err := a.RegisterBuiltins(); err != nil {
		// the builtins are fixed at compile time: a clash is a bug the
		// app tests catch, never a runtime condition
		panic(err)
	}
	return NewWithApp(a)
}

// NewWithApp builds the TUI over an existing core (its config, its state).
func NewWithApp(a *app.App) Model {
	cfg := a.Config()
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

func (m Model) t() theme.Theme { return theme.All[m.th] }

// ---- messages ----

// snapMsg is a finished refresh: the core's new snapshot and the notices
// its transitions made (app.RefreshNotices).
type snapMsg struct {
	snap    store.Snapshot
	notices []app.Notice
}
type tickMsg struct{}

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
type usageMsg struct {
	rep    claude.UsageReport
	err    error
	manual bool // from `u`: does not schedule the next poll
}
type usageTickMsg struct{}
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

func (m Model) Init() tea.Cmd {
	m.app.PollHookEvents() // start at the end: only new events matter
	return tea.Batch(m.refresh(), m.loadTodos(), tea.RequestBackgroundColor, func() tea.Msg { return eventsTickMsg{} }, m.fetchUsage(false))
}

// refresh asks the core for a new snapshot (app.RefreshNotices, off the UI
// goroutine). One at a time: a refresh asked meanwhile is coalesced by the
// caller (refreshAgain).
func (m *Model) refresh() tea.Cmd {
	if m.refreshing {
		return nil
	}
	m.refreshing = true
	a := m.app
	return func() tea.Msg {
		snap, notices := a.RefreshNotices()
		return snapMsg{snap: snap, notices: notices}
	}
}

func (m Model) scheduleTick() tea.Cmd {
	d := time.Duration(m.cfg.Refresh * float64(time.Second))
	if d < 500*time.Millisecond {
		d = 2 * time.Second
	}
	return tea.Tick(d, func(time.Time) tea.Msg { return tickMsg{} })
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
