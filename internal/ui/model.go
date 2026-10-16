// Package ui is neko's Bubble Tea program: a vim-driven tree of Claude Code
// sessions grouped by context, an inbox of agents waiting on you, the todo
// list, and an embedded pane running the real `claude attach`.
package ui

import (
	"fmt"
	"os/exec"
	"sort"
	"strings"
	"time"

	"charm.land/bubbles/v2/textinput"
	tea "charm.land/bubbletea/v2"
	"charm.land/lipgloss/v2"
	"github.com/sahilm/fuzzy"

	"github.com/aqilputi/neko/internal/cat"
	"github.com/aqilputi/neko/internal/claude"
	"github.com/aqilputi/neko/internal/config"
	"github.com/aqilputi/neko/internal/events"
	"github.com/aqilputi/neko/internal/groups"
	"github.com/aqilputi/neko/internal/jira"
	"github.com/aqilputi/neko/internal/store"
	"github.com/aqilputi/neko/internal/term"
	"github.com/aqilputi/neko/internal/theme"
	"github.com/aqilputi/neko/internal/todo"
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

// row is one line of the flattened tree.
type row struct {
	kind    string // "ctx" | "sess" | "sub"
	ctx     string
	sess    *store.Session
	sub     *store.Sub
	depth   int
	ctxCost float64
	ctxN    int
}

// todoRow is one line of the todos tab: a task or a Jira issue.
type todoRow struct {
	group string
	task  *todo.Task
	issue *jira.Issue
}

var sortModes = []string{"atividade", "custo", "nome"}

// permModes are the --permission-mode values tab cycles through when
// dispatching an agent ("" = whatever Claude Code's settings say).
var permModes = []string{"", "acceptEdits", "auto", "plan", "bypassPermissions"}

func permIndex(mode string) int {
	for i, p := range permModes {
		if p == mode {
			return i
		}
	}
	return 0
}

type Model struct {
	cfg   config.Config
	store *store.Store
	th    int

	w, h int
	tab  tab
	mode mode

	snap       store.Snapshot
	refreshing bool
	prevState  map[string]string
	ctxColor   map[string]int

	rows      []row
	cursor    int
	offset    int
	collapsed map[string]bool
	filter    string
	showAll   bool // also show stale/archived sessions and finished subagents
	// order freezes the tree: each context/session gets a rank the first
	// time it shows up and keeps it, so rows don't jump as activity changes.
	// S (new criterion) and R reset it; newcomers go on top.
	order     map[string]int
	newRank   int
	hidden    int // how many sessions the stale filter hid
	archivedN int // sessions archived by hand
	lastTab   tab // rows depend on the tab (archive vs the rest)
	sortMode  int

	inbox     []*store.Session
	inboxCur  int
	todos     []todoRow
	todoCur   int
	todoErr   error
	todoAvail bool
	tasks     []todo.Group

	usage       claude.UsageReport // `claude -p /usage`: 5h session and weekly limits
	usageErr    error
	usageWarned int // highest session % threshold already announced

	jira        jira.Cache
	jiraLoading bool
	jiraErr     error
	draftIssue  *jira.Issue

	panes   map[string]*term.Pane
	paneOf  map[string]string // sessionId → pane id (history sessions get a new short id)
	active  string            // pane id shown on the right (follows the selection, see syncActive)
	zoom    bool
	pending map[string]string // pane id → reply to type once it is ready

	input      textinput.Model
	purpose    inputPurpose
	basePrompt string // the input prompt without the permission badge
	perm       int    // index into permModes for the next dispatch
	draftDir   string
	draftTask  *todo.Task

	dialogs []dialog // drawn over the UI; the top one gets the keys

	pendingG  bool
	pendingD  bool
	toast     string
	toastErr  bool
	toastSeq  int
	lastCtrlC time.Time

	cat cat.Model

	focused   bool // terminal focus (notifications only when away)
	themeSet  bool // theme came from config; otherwise follow the terminal
	resizeSeq int

	// one animation clock (spinners, blinking, the cat)
	frame   int
	animOn  bool
	animGen int

	// Claude Code hook events (neko hook → events.jsonl)
	evTail       *events.Tail
	events       map[string]events.Event // last event per session
	hooksSeen    bool
	refreshAgain bool

	// groups of related sessions (see groups.go)
	picked map[string]bool // sessions picked with space
	grp    groups.File
	mail   map[string]mailCue // ✉ next to sessions that sent/got a message
	msgs   []peerMsg
}

func New(cfg config.Config) Model {
	ti := textinput.New()
	ti.Prompt = "› "
	ti.CharLimit = 4000
	return Model{
		cfg: cfg, store: store.New(cfg), th: theme.ByName(cfg.Theme),
		prevState: map[string]string{}, ctxColor: map[string]int{},
		collapsed: map[string]bool{}, panes: map[string]*term.Pane{}, paneOf: map[string]string{}, pending: map[string]string{},
		input: ti, cat: cat.New(), todoAvail: todo.Available(),
		sortMode: sortIndex(cfg.Sort), jira: jira.LoadCache(), perm: permIndex(cfg.PermissionMode),
		focused: true, themeSet: cfg.Theme != "",
		evTail: &events.Tail{}, events: map[string]events.Event{}, order: map[string]int{},
		picked: map[string]bool{}, grp: groups.Load(), mail: map[string]mailCue{},
	}
}

func sortIndex(name string) int {
	for i, n := range sortModes {
		if n == name {
			return i
		}
	}
	return 0
}

func (m Model) t() theme.Theme { return theme.All[m.th] }

// ---- messages ----

type snapMsg store.Snapshot
type tickMsg struct{}
type todosMsg struct {
	groups []todo.Group
	err    error
}
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
	m.evTail.Poll() // start at the end: only new events matter
	return tea.Batch(m.refresh(), m.loadTodos(), tea.RequestBackgroundColor, func() tea.Msg { return eventsTickMsg{} }, m.fetchUsage(false))
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

// fetchUsage runs `claude -p /usage` (local, ~2s, no tokens spent).
func (m *Model) fetchUsage(manual bool) tea.Cmd {
	if m.cfg.UsageRefresh <= 0 && !manual {
		return nil
	}
	return func() tea.Msg {
		rep, err := claude.FetchUsage()
		return usageMsg{rep, err, manual}
	}
}

func (m *Model) refresh() tea.Cmd {
	if m.refreshing {
		return nil
	}
	m.refreshing = true
	st := m.store
	return func() tea.Msg { return snapMsg(st.Refresh()) }
}

func (m Model) scheduleTick() tea.Cmd {
	d := time.Duration(m.cfg.Refresh * float64(time.Second))
	if d < 500*time.Millisecond {
		d = 2 * time.Second
	}
	return tea.Tick(d, func(time.Time) tea.Msg { return tickMsg{} })
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

func toastCmd(text string, err bool) tea.Cmd {
	return func() tea.Msg { return toastMsg{text: text, err: err} }
}

// stickyToast stays for long enough to copy a command from it.
func stickyToast(text string) tea.Cmd {
	return func() tea.Msg { return toastMsg{text: text, err: true, d: 30 * time.Second} }
}

// ---- data → rows ----

func (m *Model) rebuild() {
	var sel string
	if r := m.cur(); r != nil {
		sel = rowKey(*r)
	}
	m.rows = m.rows[:0]
	m.inbox = m.inbox[:0]
	m.hidden, m.archivedN = 0, 0
	f := strings.ToLower(m.filter)

	type group struct {
		ctx      string
		sessions []*store.Session
		last     time.Time
		cost     float64
	}
	var groups []*group
	byCtx := map[string]*group{}
	for i := range m.snap.Sessions {
		s := &m.snap.Sessions[i]
		if f != "" && len(fuzzy.Find(f, []string{strings.ToLower(s.Label() + " " + s.Context + " " + s.Name + " " + s.Cwd)})) == 0 {
			continue
		}
		if s.Archived {
			m.archivedN++
		}
		if m.tab == tabArchive {
			if !s.Archived {
				continue // the archive tab lists only what you archived
			}
		} else if s.Stale && !m.showAll && f == "" {
			m.hidden++
			continue
		}
		if s.Attention != store.AttnNone {
			m.inbox = append(m.inbox, s)
		}
		g := byCtx[s.Context]
		if g == nil {
			g = &group{ctx: s.Context}
			byCtx[s.Context] = g
			groups = append(groups, g)
			if _, ok := m.ctxColor[s.Context]; !ok {
				m.ctxColor[s.Context] = len(m.ctxColor)
			}
		}
		g.sessions = append(g.sessions, s)
		g.cost += s.Cost
		if s.LastAt.After(g.last) {
			g.last = s.LastAt
		}
	}
	less := func(a, b *store.Session) bool {
		switch sortModes[m.sortMode] {
		case "custo":
			return a.Cost > b.Cost
		case "nome":
			return strings.ToLower(a.Label()) < strings.ToLower(b.Label())
		}
		return a.LastAt.After(b.LastAt)
	}
	groupLess := func(a, b *group) bool {
		switch sortModes[m.sortMode] {
		case "custo":
			return a.cost > b.cost
		case "nome":
			return a.ctx < b.ctx
		}
		return a.last.After(b.last)
	}

	// Rank whatever is new, in criterion order: on the first build everything
	// is new and ranks 0..n; later arrivals get negative ranks (on top).
	fresh := len(m.order) == 0
	var newGroups []*group
	for _, g := range groups {
		if _, ok := m.order["ctx:"+g.ctx]; !ok {
			newGroups = append(newGroups, g)
		}
	}
	sort.SliceStable(newGroups, func(i, j int) bool { return groupLess(newGroups[i], newGroups[j]) })
	m.assignRanks(fresh, len(newGroups), func(i int) string { return "ctx:" + newGroups[i].ctx })
	for _, g := range groups {
		var newSess []*store.Session
		for _, s := range g.sessions {
			if _, ok := m.order["sess:"+s.SessionID]; !ok {
				newSess = append(newSess, s)
			}
		}
		sort.SliceStable(newSess, func(i, j int) bool { return less(newSess[i], newSess[j]) })
		m.assignRanks(fresh, len(newSess), func(i int) string { return "sess:" + newSess[i].SessionID })
	}

	sort.SliceStable(groups, func(i, j int) bool { return m.order["ctx:"+groups[i].ctx] < m.order["ctx:"+groups[j].ctx] })
	for _, g := range groups {
		sort.SliceStable(g.sessions, func(i, j int) bool {
			return m.order["sess:"+g.sessions[i].SessionID] < m.order["sess:"+g.sessions[j].SessionID]
		})
		m.rows = append(m.rows, row{kind: "ctx", ctx: g.ctx, ctxCost: g.cost, ctxN: len(g.sessions)})
		if m.collapsed["ctx:"+g.ctx] {
			continue
		}
		for _, s := range g.sessions {
			m.rows = append(m.rows, row{kind: "sess", ctx: s.Context, sess: s, depth: 1})
			if !m.collapsed["sess:"+s.SessionID] {
				for j := range s.Subs {
					// finished subagents are noise; H shows them
					if s.Subs[j].Done && !m.showAll {
						continue
					}
					m.rows = append(m.rows, row{kind: "sub", ctx: s.Context, sess: s, sub: &s.Subs[j], depth: 2})
				}
			}
		}
	}
	sortInbox(m.inbox, m.order)
	m.cursor = min(m.cursor, max(len(m.rows)-1, 0))
	if sel != "" {
		for i, r := range m.rows {
			if rowKey(r) == sel {
				m.cursor = i
				break
			}
		}
	}
	m.inboxCur = min(m.inboxCur, max(len(m.inbox)-1, 0))
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
			if !m.showAll && time.Since(is.UpdatedAt()) > 90*24*time.Hour {
				continue
			}
			m.todos = append(m.todos, todoRow{group: "jira", issue: is})
		}
	}
	m.todoCur = min(m.todoCur, max(len(m.todos)-1, 0))
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

// sortInbox: most urgent first, then the tree's frozen order (stable).
func sortInbox(xs []*store.Session, order map[string]int) {
	sort.SliceStable(xs, func(i, j int) bool {
		a, b := xs[i], xs[j]
		if a.Attention != b.Attention {
			return a.Attention < b.Attention
		}
		return order["sess:"+a.SessionID] < order["sess:"+b.SessionID]
	})
}

// assignRanks gives n new keys a rank: 0..n-1 on a fresh order, otherwise
// below every existing rank so newcomers show on top, in criterion order.
func (m *Model) assignRanks(fresh bool, n int, key func(int) string) {
	if fresh {
		base := len(m.order)
		for i := 0; i < n; i++ {
			m.order[key(i)] = base + i
		}
		return
	}
	for i := n - 1; i >= 0; i-- {
		m.newRank--
		m.order[key(i)] = m.newRank
	}
}

// resetOrder re-sorts by the current criterion on the next build.
func (m *Model) resetOrder() {
	m.order = map[string]int{}
	m.newRank = 0
}

func rowKey(r row) string {
	switch r.kind {
	case "ctx":
		return "ctx:" + r.ctx
	case "sess":
		return "sess:" + r.sess.SessionID
	default:
		return "sub:" + r.sub.ID
	}
}

func (m *Model) cur() *row {
	if m.cursor >= 0 && m.cursor < len(m.rows) {
		return &m.rows[m.cursor]
	}
	return nil
}

// selected is the session under the cursor in the current tab.
func (m *Model) selected() *store.Session {
	switch m.tab {
	case tabInbox:
		if m.inboxCur < len(m.inbox) {
			return m.inbox[m.inboxCur]
		}
	case tabSessions, tabArchive:
		if r := m.cur(); r != nil && r.sess != nil {
			return r.sess
		}
	}
	return nil
}

// notice transitions: a session that just became blocked or done makes the
// cat meow and rings the terminal bell.
func (m *Model) notice() tea.Cmd {
	var waiting []string
	for _, s := range m.snap.Sessions {
		prev, had := m.prevState[s.SessionID]
		m.prevState[s.SessionID] = s.State
		if !had || prev == s.State {
			continue
		}
		if s.State == "blocked" || s.State == "done" || (s.Kind == "interactive" && s.State == "idle" && prev == "busy") {
			waiting = append(waiting, s.Label())
		}
	}
	if len(waiting) == 0 {
		return nil
	}
	msg := "miau! " + truncate(waiting[0], 40) + " voltou"
	if len(waiting) > 1 {
		msg = fmt.Sprintf("miau! %d agents voltaram", len(waiting))
	}
	m.cat.Meow(truncate(msg, 50))
	if m.focused {
		return nil // you're looking at neko: the cat is enough
	}
	return tea.Batch(tea.Raw("\a"), notifyDesktop("neko", msg))
}

func notifyDesktop(title, body string) tea.Cmd {
	return func() tea.Msg {
		if p, err := exec.LookPath("notify-send"); err == nil {
			_ = exec.Command(p, "-a", "neko", title, body).Run()
		}
		return nil
	}
}

// ---- panes ----

func (m *Model) paneSize() (int, int) {
	r := m.rects().pane
	return r.Dx(), r.Dy()
}

// openPane attaches the session in the embedded pane, resuming it into the
// daemon first when it is not a background session.
func (m *Model) openPane(s store.Session, focus bool, reply string) tea.Cmd {
	if s.Kind == "interactive" {
		return toastCmd(fmt.Sprintf("sessão interativa aberta em outro terminal (pid %d) — use o terminal dela", s.PID), true)
	}
	// already open (history sessions keep the pane under their new short id)
	if id := m.paneOf[s.SessionID]; id != "" {
		if p := m.panes[id]; p != nil && !p.Done() {
			m.active = id
			if focus {
				m.mode = modeInsert
			}
			if reply != "" {
				m.pending[id] = reply
				return m.sendReplyLater(id, 50*time.Millisecond)
			}
			return nil
		}
	}
	if s.ShortID != "" {
		return m.startPane(s, s.ShortID, focus, reply)
	}
	sess := s
	return func() tea.Msg {
		id, err := claude.ResumeInBackground(sess.Cwd, sess.SessionID)
		return paneOpenMsg{sess: sess, id: id, err: err, text: reply}
	}
}

func (m *Model) startPane(s store.Session, id string, focus bool, reply string) tea.Cmd {
	w, h := m.paneSize()
	p, err := term.Start(id, s.Label(), s.Cwd, w, h, claude.AttachArgv(id)...)
	if err != nil {
		return toastCmd("attach: "+err.Error(), true)
	}
	m.panes[id] = p
	m.paneOf[s.SessionID] = id
	m.active = id
	if focus {
		m.mode = modeInsert
	}
	m.store.MarkSeen(s.SessionID, time.Now())
	cmds := []tea.Cmd{p.Listen()}
	if reply != "" {
		m.pending[id] = reply
		cmds = append(cmds, m.sendReplyLater(id, 1500*time.Millisecond))
	}
	return tea.Batch(cmds...)
}

func (m *Model) sendReplyLater(id string, d time.Duration) tea.Cmd {
	return tea.Tick(d, func(time.Time) tea.Msg { return replyMsg{id} })
}

// syncActive makes the right side follow the selection: the selected
// session's pane if it has one open, otherwise its details. Panes that are
// not shown keep running hidden. In INSERT or zoom the pane stays put.
func (m *Model) syncActive() {
	if m.mode == modeInsert || (m.zoom && m.active != "") {
		return
	}
	m.active = ""
	if sel := m.selected(); sel != nil {
		id := m.paneOf[sel.SessionID]
		if id == "" {
			id = sel.ShortID
		}
		if p := m.panes[id]; p != nil && !p.Done() {
			m.active = id
		}
	}
}

func (m *Model) closePane(id string) {
	if p := m.panes[id]; p != nil {
		p.Close()
		delete(m.panes, id)
	}
	for sid, pid := range m.paneOf {
		if pid == id {
			delete(m.paneOf, sid)
		}
	}
	if m.active == id {
		m.active = ""
		m.zoom = false
		if m.mode == modeInsert {
			m.mode = modeNormal
		}
	}
}

func (m *Model) resizePanes() {
	w, h := m.paneSize()
	for _, p := range m.panes {
		p.Resize(w, h)
	}
}

// ---- input line ----

func (m *Model) ask(p inputPurpose, prompt, value string) tea.Cmd {
	m.mode = modeInput
	m.purpose = p
	m.basePrompt = prompt
	m.refreshPrompt()
	m.input.SetValue(value)
	m.input.CursorEnd()
	return m.input.Focus()
}

// dispatching reports whether the input line ends in launching an agent,
// so it shows (and tab cycles) the permission mode.
func (m *Model) dispatching() bool {
	switch m.purpose {
	case inNewDir, inNewPrompt, inDispatchDir, inJiraAgentDir:
		return true
	}
	return false
}

func (m *Model) refreshPrompt() {
	prompt := m.basePrompt
	if m.dispatching() {
		t := m.t()
		badge := lipgloss.NewStyle().Foreground(t.Muted).Render("[padrão ⇥] ")
		switch mode := permModes[m.perm]; mode {
		case "":
		case "bypassPermissions":
			badge = lipgloss.NewStyle().Foreground(t.Bg).Background(t.Error).Bold(true).Render(" ⚠ BYPASS ") + " "
		default:
			badge = lipgloss.NewStyle().Foreground(t.Accent2).Bold(true).Render("[" + mode + "] ")
		}
		prompt = badge + prompt
	}
	m.input.Prompt = prompt
	m.input.SetWidth(max(m.w-lipglossWidth(prompt)-4, 10))
}

// dispatch launches a background agent with the chosen permission mode.
// Bypass asks first unless it is the configured default (permission_mode).
// The mode then goes back to the default, so a bypass picked with tab never
// sticks around unnoticed.
func (m *Model) dispatch(dir, name, prompt string, task *todo.Task) tea.Cmd {
	mode := permModes[m.perm]
	m.perm = permIndex(m.cfg.PermissionMode)
	var extra []string
	if mode != "" {
		extra = []string{"--permission-mode", mode}
	}
	run := func() tea.Msg {
		id, err := claude.Dispatch(dir, name, prompt, extra...)
		return dispatchedMsg{id: id, task: task, err: err}
	}
	if mode == "bypassPermissions" && m.cfg.PermissionMode != "bypassPermissions" {
		m.confirmThen(fmt.Sprintf("disparar em BYPASS (sem nenhuma checagem de permissão) em %s?\nos hooks, como o guarded-access, continuam valendo", config.Tilde(dir)),
			func(*Model) tea.Cmd { return run })
		return nil
	}
	return run
}

func truncate(s string, n int) string {
	r := []rune(s)
	if len(r) <= n {
		return s
	}
	if n <= 1 {
		return "…"
	}
	return string(r[:n-1]) + "…"
}

// ExitBanner reminds what keeps running after neko quits.
func (m Model) ExitBanner() string {
	var b strings.Builder
	for _, s := range m.snap.Sessions {
		if s.ShortID != "" && (s.State == "busy" || s.Attention == store.AttnBlocked) {
			fmt.Fprintf(&b, "  %-9s claude attach %s   %s\n", s.State, s.ShortID, truncate(s.Label(), 50))
		}
	}
	if b.Len() == 0 {
		return "ᓚᘏᗢ até mais\n"
	}
	return "ᓚᘏᗢ seguem em background:\n" + b.String()
}

// toggleArchive is X: on a session it archives or restores it; on a context
// row it archives (or, in the archive tab, restores) every session listed
// under it, asking first.
func (m *Model) toggleArchive() tea.Cmd {
	r := m.cur()
	if m.tab == tabInbox {
		if sel := m.selected(); sel != nil {
			r = &row{kind: "sess", sess: sel}
		}
	}
	if r == nil {
		return nil
	}
	if r.kind == "ctx" {
		var ids []string
		for _, x := range m.rows {
			if x.kind == "sess" && x.ctx == r.ctx {
				ids = append(ids, x.sess.SessionID)
			}
		}
		if len(ids) == 0 {
			return nil
		}
		restore := m.tab == tabArchive
		verb := "arquivar"
		if restore {
			verb = "restaurar"
		}
		m.confirmThen(fmt.Sprintf("%s as %d sessões de \"%s\"?", verb, len(ids), r.ctx), func(m *Model) tea.Cmd {
			if restore {
				m.store.Unarchive(ids...)
			} else {
				m.store.Archive(ids...)
			}
			return tea.Batch(toastCmd(fmt.Sprintf("%d sessões: %s ✓", len(ids), verb), false), m.refresh())
		})
		return nil
	}
	s := r.sess
	if s.Archived {
		m.store.Unarchive(s.SessionID)
		s.Archived, s.Stale = false, false
		m.rebuild()
		return tea.Batch(toastCmd("restaurada", false), m.refresh())
	}
	m.store.Archive(s.SessionID)
	s.Archived, s.Stale = true, true
	m.rebuild()
	msg := "arquivada (aba 4 Arquivo) — volta sozinha se tiver atividade nova"
	if s.State == "busy" {
		msg = "arquivada, mas está rodando: volta assim que tiver atividade nova"
	}
	return tea.Batch(toastCmd(msg, false), m.refresh())
}

// archiveStale archives every stale session not archived yet.
func (m *Model) archiveStale() tea.Cmd {
	var ids []string
	for _, s := range m.snap.Sessions {
		if s.Stale && !s.Archived && s.State != "busy" {
			ids = append(ids, s.SessionID)
		}
	}
	if len(ids) == 0 {
		return toastCmd("nenhuma sessão parada para arquivar", false)
	}
	m.confirmThen(fmt.Sprintf("arquivar as %d sessões paradas há mais de %.0fd?", len(ids), m.cfg.StaleDays), func(m *Model) tea.Cmd {
		m.store.Archive(ids...)
		return tea.Batch(toastCmd(fmt.Sprintf("%d sessões arquivadas ✓", len(ids)), false), m.refresh())
	})
	return nil
}

// removeEnded runs `claude rm` on background chats whose conversation was
// ended: they take no more messages, but their process lingers.
func (m *Model) removeEnded() tea.Cmd {
	var ids []string
	for _, s := range m.snap.Sessions {
		if s.Ended && s.ShortID != "" {
			ids = append(ids, s.ShortID)
		}
	}
	if len(ids) == 0 {
		return toastCmd("nenhum chat encerrado com processo sobrando", false)
	}
	m.confirmThen(fmt.Sprintf("remover %d chats encerrados (claude rm)? a conversa já não aceita mensagens", len(ids)), func(m *Model) tea.Cmd {
		for _, id := range ids {
			m.closePane(id)
		}
		return func() tea.Msg {
			var failed []string
			for _, id := range ids {
				if err := claude.Remove(id); err != nil {
					failed = append(failed, id)
				}
			}
			if len(failed) > 0 {
				return toastMsg{text: "falhou: " + strings.Join(failed, " "), err: true}
			}
			return tickMsg{}
		}
	})
	return nil
}

// windowPct estimates how much of the current 5-hour window a session used:
// the window's % from /usage, split across sessions by what each consumed
// since the window began (subagents included). 0 when unknown or idle.
func (m *Model) windowPct(s *store.Session) float64 {
	l, ok := m.usage.Find("session")
	if !ok || s.WindowCost <= 0 {
		return 0
	}
	var total float64
	for i := range m.snap.Sessions {
		total += m.snap.Sessions[i].WindowCost
	}
	if total <= 0 {
		return 0
	}
	return float64(l.Pct) * s.WindowCost / total
}

// pctText formats a share of the window: "", "<1%", "3%", "12%".
func pctText(p float64) string {
	switch {
	case p <= 0:
		return ""
	case p < 1:
		return "<1%"
	}
	return fmt.Sprintf("%.0f%%", p)
}
