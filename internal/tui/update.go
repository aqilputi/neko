package tui

import (
	"context"
	"fmt"
	"strings"
	"time"

	"charm.land/bubbles/v2/key"
	tea "charm.land/bubbletea/v2"

	"github.com/aqilputi/nekuroddo/internal/app"
	"github.com/aqilputi/nekuroddo/internal/config"
	"github.com/aqilputi/nekuroddo/internal/core"
	"github.com/aqilputi/nekuroddo/internal/events"
	"github.com/aqilputi/nekuroddo/internal/theme"
)

func (m Model) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	cmd := m.update(msg)
	if m.tab != m.lastTab { // archive tab and the others list different rows
		m.lastTab = m.tab
		m.rebuild()
	}
	m.syncActive()
	return m, tea.Batch(cmd, m.ensureAnim())
}

func (m *Model) update(msg tea.Msg) tea.Cmd {
	switch msg := msg.(type) {
	case tea.WindowSizeMsg:
		first := m.w == 0 && msg.Width > 0 // the program's first size: start following the core
		m.w, m.h = msg.Width, msg.Height
		// Debounce: a window drag would otherwise SIGWINCH claude on every step.
		m.resizeSeq++
		seq := m.resizeSeq
		tick := tea.Tick(120*time.Millisecond, func(time.Time) tea.Msg { return resizeMsg{seq} })
		if first {
			return tea.Batch(tick, m.followBus())
		}
		return tick

	case busMsg:
		return tea.Batch(m.onBus(msg.ev), listenBus(msg.ch))

	case resizeMsg:
		if msg.seq == m.resizeSeq {
			m.resizePanes()
		}
		return nil

	case animMsg:
		if msg.gen != m.animGen {
			return nil
		}
		m.animOn = false
		m.frame++
		m.cat.Advance()
		return nil

	case tea.FocusMsg:
		m.app.SetFocused(true) // notifications and the bell only while away
		return nil

	case tea.BlurMsg:
		m.app.SetFocused(false)
		return nil

	case tea.BackgroundColorMsg:
		if !m.themeSet {
			if msg.IsDark() {
				m.th = theme.ByName("catppuccin-mocha")
			} else {
				m.th = theme.ByName("rose-pine-dawn")
			}
		}
		return nil

	case snapMsg:
		m.refreshing = false
		m.snap = msg.snap
		m.rebuild()
		cmds := []tea.Cmd{m.showNotices(msg.notices), m.scheduleTick()}
		if m.refreshAgain { // an event arrived while we were refreshing
			m.refreshAgain = false
			cmds = append(cmds, m.refresh())
		}
		return tea.Batch(cmds...)

	case tickMsg:
		return m.refresh()

	case usageMsg:
		return m.onUsage(msg)

	case usageTickMsg:
		return m.fetchUsage(false)

	case eventsTickMsg:
		return m.pollEvents()

	case eventsMsg:
		return tea.Batch(m.onEvents([]events.Event(msg)), m.scheduleEvents())

	case todosMsg:
		return m.onTodos(msg)

	case jiraMsg:
		return m.onJira(msg)

	case confirmResolvedMsg:
		return m.onConfirmResolved(msg)

	case commandDoneMsg:
		return m.commandResult(msg.name, msg.res, msg.err)

	case jiraCreatedMsg:
		return m.onJiraCreated(msg)

	case toastMsg:
		m.toast, m.toastErr = msg.text, msg.err
		m.toastSeq++
		seq, d := m.toastSeq, msg.d
		if d == 0 {
			d = 5 * time.Second
			if msg.err {
				d = 10 * time.Second
			}
		}
		return tea.Tick(d, func(time.Time) tea.Msg { return clearToastMsg{seq} })

	case clearToastMsg:
		if msg.seq == m.toastSeq {
			m.toast = ""
		}
		return nil

	case paneOutputMsg:
		return m.onPaneOutput(msg)

	case paneExitMsg:
		return m.onPaneExit(msg)

	case replyMsg:
		return m.onReply(msg)

	case enterMsg:
		return m.onEnter(msg)

	case paneOpenMsg:
		return m.onPaneOpen(msg)

	case dispatchedMsg:
		return m.onDispatched(msg)

	case execDoneMsg:
		return m.refresh()

	case tea.PasteMsg:
		if d, ok := m.top().(*paletteDialog); ok {
			var cmd tea.Cmd
			d.input, cmd = d.input.Update(msg)
			d.filter(m)
			return cmd
		}
		switch m.mode {
		case modeInsert:
			if p := m.panes[m.active]; p != nil {
				p.Paste(msg.Content)
			}
		case modeInput:
			var cmd tea.Cmd
			m.input, cmd = m.input.Update(msg)
			return cmd
		}
		return nil

	case tea.MouseMsg:
		return m.mouse(msg)

	case tea.KeyPressMsg:
		return m.key(msg)
	}

	if m.mode == modeInput {
		var cmd tea.Cmd
		m.input, cmd = m.input.Update(msg)
		return cmd
	}
	return nil
}

type enterMsg struct{ pane string }
type resizeMsg struct{ seq int }
type execDoneMsg struct{ err error }
type animMsg struct{ gen int }
type clearToastMsg struct{ seq int }
type eventsTickMsg struct{}
type eventsMsg []events.Event

// ensureAnim runs the one animation clock: fast while something visibly
// animates (spinners, blinking, a meowing cat), slow for the sleeping cat.
func (m *Model) ensureAnim() tea.Cmd {
	if m.animOn || m.w == 0 {
		return nil
	}
	m.animOn = true
	m.animGen++
	gen := m.animGen
	d := time.Second
	if busy, waiting, _ := m.counts(); busy+waiting > 0 || m.cat.Awake() || m.mailBlinking() {
		d = 150 * time.Millisecond
	}
	return tea.Tick(d, func(time.Time) tea.Msg { return animMsg{gen} })
}

func (m *Model) key(k tea.KeyPressMsg) tea.Cmd {
	if d := m.top(); d != nil {
		done, cmd := d.handle(m, k)
		if done && m.top() == d {
			m.pop()
		}
		return cmd
	}
	switch m.mode {
	case modeInsert:
		if key.Matches(k, leaveInsert) {
			m.mode = modeNormal
			return nil
		}
		// The keys bound in INSERT (or in every mode) are nekuroddo's even
		// inside claude: ctrl+k (claude loses its kill-line) jumps to another
		// live chat without leaving INSERT first.
		if name, ok := m.keymap().Resolve(core.ModeInsert, k.String()); ok {
			_, cmd := m.runCommand(name, k.String(), nil)
			return cmd
		}
		if p := m.panes[m.active]; p != nil {
			// classic renderer: nekuroddo scrolls the scrollback (fullscreen claude
			// handles these keys itself)
			if !p.WantsMouse() {
				switch k.String() {
				case "shift+pgup":
					p.Scroll(p.Height() / 2)
					return nil
				case "shift+pgdown":
					p.Scroll(-p.Height() / 2)
					return nil
				}
			}
			p.SendKey(k)
		} else {
			m.mode = modeNormal
		}
		return nil

	case modeInput:
		switch k.String() {
		case "esc":
			m.mode = modeNormal
			if m.purpose == inFilter {
				m.view.Filter = ""
				m.rebuild()
			}
			return nil
		case "enter":
			return m.submitInput()
		case "tab", "shift+tab":
			if m.dispatching() { // cycle the permission mode of this dispatch
				d := 1
				if k.String() == "shift+tab" {
					d = len(permModes) - 1
				}
				m.perm = (m.perm + d) % len(permModes)
				m.refreshPrompt()
				return nil
			}
		}
		var cmd tea.Cmd
		m.input, cmd = m.input.Update(k)
		if m.purpose == inFilter {
			m.view.Filter = m.input.Value()
			m.rebuild()
		}
		return cmd
	}
	_, cmd := m.keyNormal(k)
	return cmd
}

// keyNormal handles NORMAL mode: the key resolves to a command name in the
// core's keymap (the todos tab first, then NORMAL) and the command runs
// (runCommand). A two-key sequence (gg, dd) waits for its second press.
// esc is the client's own: it drops the picks, then the filter.
func (m *Model) keyNormal(k tea.KeyPressMsg) (bool, tea.Cmd) {
	s, mode, km := k.String(), m.keyMode(), m.keymap()
	if p := m.pendingKey(); p != "" {
		m.setPendingKey("")
		if name, ok := km.Resolve(mode, p+p); ok && s == p {
			return m.runCommand(name, s, nil)
		}
	}
	if name, ok := km.Resolve(mode, s); ok {
		return m.runCommand(name, s, nil)
	}
	switch {
	case km.StartsSequence(mode, s):
		m.setPendingKey(s)
	case s == "esc":
		m.clearSelection()
	default:
		return false, nil
	}
	return true, nil
}

// pendingKey is the first press of a sequence waiting for its second (the
// TUI keeps gg and dd).
func (m *Model) pendingKey() string {
	switch {
	case m.pendingG:
		return "g"
	case m.pendingD:
		return "d"
	}
	return ""
}

func (m *Model) setPendingKey(s string) { m.pendingG, m.pendingD = s == "g", s == "d" }

// clearSelection is esc: the picks go first, then the filter.
func (m *Model) clearSelection() {
	if len(m.picked) > 0 {
		m.picked = map[string]bool{}
		return
	}
	if m.view.Filter != "" {
		m.view.Filter = ""
		m.rebuild()
	}
}

// ---- commands ----

// Keys resolve to command names in the core's keymap (app.Keymap); a name
// runs here when the TUI handles it itself (clientHandlers: the client
// scope, and core commands whose arguments or selection the TUI asks for
// first), else in the core's registry with this client's selection.

// clientHandler runs a command the TUI handles; key is what was pressed
// (the palette passes the binding's first key). handled false lets the key
// fall through, as an unbound one.
type clientHandler func(m *Model, key string) (handled bool, cmd tea.Cmd)

var clientHandlers map[string]clientHandler

func init() {
	clientHandlers = map[string]clientHandler{
		"nav.down":           moveBy(func(*Model) int { return 1 }),
		"nav.up":             moveBy(func(*Model) int { return -1 }),
		"nav.top":            moveBy(func(*Model) int { return -1 << 30 }),
		"nav.bottom":         moveBy(func(*Model) int { return 1 << 30 }),
		"nav.half_down":      moveBy(func(m *Model) int { return m.listHeight() / 2 }),
		"nav.half_up":        moveBy(func(m *Model) int { return -m.listHeight() / 2 }),
		"nav.page_down":      moveBy(func(m *Model) int { return m.listHeight() }),
		"nav.page_up":        moveBy(func(m *Model) int { return -m.listHeight() }),
		"nav.collapse":       do((*Model).collapse),
		"nav.expand":         do((*Model).expand),
		"nav.pick":           do((*Model).togglePick),
		"nav.next_attention": do(func(m *Model) { m.jumpAttention(1) }),
		"nav.prev_attention": do(func(m *Model) { m.jumpAttention(-1) }),

		"tab.sessions": toTab(tabSessions),
		"tab.inbox":    toTab(tabInbox),
		"tab.todos":    toTab(tabTodos),
		"tab.archive":  toTab(tabArchive),
		"tab.next":     func(m *Model, _ string) (bool, tea.Cmd) { return m.setTab((m.tab + 1) % numTabs) },
		"tab.prev":     func(m *Model, _ string) (bool, tea.Cmd) { return m.setTab((m.tab + numTabs - 1) % numTabs) },
		"inbox.summon": toTab(tabInbox),

		"view.filter":   run((*Model).filterKey),
		"view.show_all": run((*Model).toggleShowAll),
		"view.sort":     run((*Model).nextSort),
		"view.theme":    run((*Model).nextTheme),

		"palette.open":  run((*Model).openPalette),
		"switcher.open": run((*Model).openSwitcher),
		"help.open":     do(func(m *Model) { m.push(helpDialog{}) }),
		"usage.show":    run(func(m *Model) tea.Cmd { m.push(usageDialog{}); return m.fetchUsage(true) }),
		"app.quit":      (*Model).quitKey,

		"session.open":       run(func(m *Model) tea.Cmd { return m.openOrDispatch(true) }),
		"session.insert":     run((*Model).insertKey),
		"session.preview":    run((*Model).previewKey),
		"jira.open_url":      run((*Model).previewKey),
		"session.fullscreen": run((*Model).fullscreenKey),
		"session.reply":      run((*Model).replyKey),
		"session.new":        run((*Model).newKey),
		"session.archive":    run((*Model).toggleArchive),
		"session.rename":     notOnTodos((*Model).renameKey),
		"session.move":       notOnTodos((*Model).moveKey),
		"session.stop":       do((*Model).stopKey),
		"session.remove":     run((*Model).removeSelected),
		"sessions.refresh":   run((*Model).refreshKey),
		"pane.zoom":          do((*Model).zoomKey),
		"pane.close":         do(func(m *Model) { m.closeActive() }),

		"todo.dispatch": run((*Model).dispatchTodo),
		"todo.start":    run((*Model).startKey),
		"todo.done":     do((*Model).doneTodo),
		"jira.toggle":   run((*Model).jiraAction),
		"jira.fetch":    run((*Model).jiraFetchKey),
	}
}

// notOnTodos is a session handler the todos tab leaves alone.
func notOnTodos(f func(*Model) tea.Cmd) clientHandler {
	return func(m *Model, _ string) (bool, tea.Cmd) {
		if m.tab == tabTodos {
			return false, nil
		}
		return true, f(m)
	}
}

// quitKey asks before quitting; ctrl+c twice within a second quits at once.
func (m *Model) quitKey(key string) (bool, tea.Cmd) {
	if key == "ctrl+c" && m.lastCtrlC.After(time.Now().Add(-time.Second)) {
		for id := range m.panes {
			m.closePane(id)
		}
		return true, tea.Quit
	}
	m.lastCtrlC = time.Now()
	m.askQuit()
	return true, nil
}

// openOrDispatch is enter: on the todos tab an agent for the task, else the
// selected session in the pane.
func (m *Model) openOrDispatch(focus bool) tea.Cmd {
	if m.tab == tabTodos {
		return m.dispatchTodo()
	}
	if sel := m.selected(); sel != nil {
		return m.openPane(*sel, focus, "")
	}
	return nil
}

// insertKey is i: into the open pane, or open the selection focused.
func (m *Model) insertKey() tea.Cmd {
	if m.active != "" {
		m.mode = modeInsert
		return nil
	}
	return m.openOrDispatch(true)
}

// previewKey is o: an issue on the todos tab opens in the browser (the
// core's jira.open_url); else the selected session opens unfocused.
func (m *Model) previewKey() tea.Cmd {
	if r := m.curRow(); m.tab == tabTodos && r != nil && r.issue != nil {
		return m.runCore("jira.open_url", nil)
	}
	if sel := m.selected(); sel != nil {
		return m.openPane(*sel, false, "")
	}
	return nil
}

func (m *Model) fullscreenKey() tea.Cmd {
	if sel := m.selected(); sel != nil {
		return m.fullscreen(*sel)
	}
	return nil
}

// replyKey asks for a quick reply (session.reply's text), filled with the
// suggested one.
func (m *Model) replyKey() tea.Cmd {
	sel := m.selected()
	if sel == nil {
		return nil
	}
	if sel.Kind == "interactive" {
		return toastCmd("sessão interativa: responda no terminal dela", true)
	}
	m.draftDir = sel.SessionID
	prompt := "↩ " + core.Truncate(sel.Label(), 30)
	if n := core.OptionCount(sel.Questions); n > 0 {
		prompt += fmt.Sprintf(" (1-%d escolhe opção)", n)
	}
	return m.ask(inReply, prompt+" › ", sel.SuggestedReply)
}

// newKey asks where a new agent starts (session.new's dir), then its prompt.
func (m *Model) newKey() tea.Cmd {
	dir := "~"
	if sel := m.selected(); sel != nil {
		dir = config.Tilde(sel.Cwd)
	}
	return m.ask(inNewDir, "diretório › ", dir)
}

// renameKey asks for a session's name, or a section's on a context row.
func (m *Model) renameKey() tea.Cmd {
	if r := m.cur(); m.tab != tabInbox && r != nil && r.Kind == core.RowCtx {
		m.draftDir = r.Ctx
		return m.ask(inRenameCtx, "renomeia a seção \""+core.Truncate(r.Ctx, 30)+"\" (vazio desfaz) › ", r.Ctx)
	}
	if sel := m.selected(); sel != nil {
		m.draftDir = sel.SessionID
		return m.ask(inRename, "renomeia (vazio volta ao nome do claude) › ", sel.Label())
	}
	return nil
}

// moveKey asks for the selected session's context.
func (m *Model) moveKey() tea.Cmd {
	sel := m.selected()
	if sel == nil {
		return nil
	}
	m.draftDir = sel.SessionID
	hint := strings.Join(m.contextNames(), " · ")
	return m.ask(inMove, "contexto ("+core.Truncate(hint, 50)+"; vazio volta à regra do diretório) › ", sel.Context)
}

// stopKey is x: the core's session.stop asks first; on anything but a
// background session nothing happens.
func (m *Model) stopKey() {
	if res, err := m.app.Run(m.commandCtx(), "session.stop", nil); err == nil && res.Pending != nil {
		m.askApp(res, true)
	}
}

// refreshKey is R: a refresh that also re-sorts the frozen order.
func (m *Model) refreshKey() tea.Cmd {
	m.view = m.view.ResetOrder()
	m.rebuild()
	return tea.Batch(m.refresh(), m.loadTodos())
}

func (m *Model) zoomKey() {
	if m.active != "" {
		m.zoom = !m.zoom
		m.resizePanes()
	}
}

func (m *Model) closeActive() {
	if m.active != "" {
		m.closePane(m.active)
	}
}

// startKey is s: the core's todo.start for the task under the cursor.
func (m *Model) startKey() tea.Cmd {
	if m.curTodo() == nil {
		return nil
	}
	return m.runCore("todo.start", nil)
}

// jiraFetchKey is F: fetch the Jira issues now (the tab shows it running).
func (m *Model) jiraFetchKey() tea.Cmd {
	if !m.cfg.Jira.Enabled {
		return toastCmd("jira desligado: configure [jira] no config.toml", true)
	}
	return tea.Batch(toastCmd("buscando no Jira via claude…", false), m.fetchJira(true))
}

// do is a handler that only changes the model.
func do(f func(*Model)) clientHandler {
	return func(m *Model, _ string) (bool, tea.Cmd) { f(m); return true, nil }
}

// run is a handler that always handles the key.
func run(f func(*Model) tea.Cmd) clientHandler {
	return func(m *Model, _ string) (bool, tea.Cmd) { return true, f(m) }
}

func moveBy(d func(*Model) int) clientHandler {
	return do(func(m *Model) { m.move(d(m)) })
}

func toTab(t tab) clientHandler {
	return func(m *Model, _ string) (bool, tea.Cmd) { return m.setTab(t) }
}

// setTab switches tabs; the todos tab reloads the list (and Jira, if stale).
func (m *Model) setTab(t tab) (bool, tea.Cmd) {
	m.tab = t
	if t == tabTodos {
		return true, tea.Batch(m.loadTodos(), m.fetchJira(false))
	}
	return true, nil
}

func (m *Model) filterKey() tea.Cmd {
	m.tab = tabSessions
	return m.ask(inFilter, "/ ", m.view.Filter)
}

func (m *Model) toggleShowAll() tea.Cmd {
	m.view.ShowAll = !m.view.ShowAll
	m.rebuild()
	m.rebuildTodos()
	if m.view.ShowAll {
		return toastCmd("mostrando tudo, inclusive paradas e arquivadas", false)
	}
	return toastCmd(fmt.Sprintf("só as ativas (≤ %.0fd)", m.cfg.StaleDays), false)
}

func (m *Model) nextSort() tea.Cmd {
	m.view.SortMode = (m.view.SortMode + 1) % len(core.SortModes)
	m.view = m.view.ResetOrder()
	m.rebuild()
	return toastCmd("ordem: "+core.SortModes[m.view.SortMode], false)
}

func (m *Model) nextTheme() tea.Cmd {
	m.themeSet = true
	m.th = (m.th + 1) % len(theme.All)
	return toastCmd("tema: "+theme.All[m.th].Name, false)
}

// keyMode is the keymap mode NORMAL keys resolve in: the todos tab has its
// own keys over NORMAL's.
func (m *Model) keyMode() string {
	if m.tab == tabTodos {
		return core.ModeNormalTodos
	}
	return core.ModeNormal
}

// runCommand runs name: the TUI's handler, a key Lua bound (lua.go), or the
// core's registry.
func (m *Model) runCommand(name, key string, args app.Args) (bool, tea.Cmd) {
	if h, ok := clientHandlers[name]; ok {
		return h(m, key)
	}
	if name == "" || strings.HasPrefix(name, luaKeyPrefix) {
		return true, m.runLuaKey(name)
	}
	return true, m.runCore(name, args)
}

// tabNames are the tabs as app.Context.Tab names them.
var tabNames = [numTabs]string{"sessions", "inbox", "todos", app.TabArchive}

// commandCtx is this client's selection as the core sees it: the session
// under the cursor, the picked ones, the tab, and the task or issue under
// the cursor on the todos tab.
func (m *Model) commandCtx() app.Context {
	ctx := m.groupCtx()
	ctx.Tab = tabNames[m.tab]
	if r := m.curRow(); m.tab == tabTodos && r != nil {
		if r.task != nil {
			ctx.TodoID = r.task.ID
		}
		if r.issue != nil {
			ctx.IssueKey = r.issue.Key
		}
	}
	return ctx
}

// runCore runs a core command with this client's selection. One that may
// block (it runs claude, the todo skill, a browser, or Lua) runs off the UI
// goroutine and comes back as commandDoneMsg.
func (m *Model) runCore(name string, args app.Args) tea.Cmd {
	a, ctx := m.app, m.commandCtx()
	if c, ok := a.Registry().Lookup(name); ok && (blocking[name] || c.Source != core.SourceBuiltin) {
		return func() tea.Msg {
			res, err := a.Run(ctx, name, args)
			return doneMsg(name, res, err)
		}
	}
	res, err := a.Run(ctx, name, args)
	return m.commandResult(name, res, err)
}

// doneMsg is what a command run off the UI goroutine comes back as: the
// message itself when it needs nothing of the model (an error or a toast,
// the reloaded todos, an agent dispatched), else a commandDoneMsg.
func doneMsg(name string, res app.Result, err error) tea.Msg {
	if err != nil {
		return toastMsg{text: errText(res, err), err: true}
	}
	if res.Pending == nil && res.Exec == nil {
		switch res.Data.(type) {
		case app.TodoList:
			return todoResult(res)
		case app.Dispatched:
			return resultMsg(res, nil)
		case nil:
			if res.Toast == "" {
				return nil
			}
			return toastMsg{text: res.Toast, err: res.Err}
		}
	}
	return commandDoneMsg{name: name, res: res}
}

// blocking are the builtin commands whose runner waits on another program.
var blocking = map[string]bool{
	"todo.start": true, "todo.dispatch": true, "jira.toggle": true, "jira.fetch": true, "jira.open_url": true,
	"session.open": true, "session.preview": true, "session.reply": true, "session.fullscreen": true,
	"session.new": true, "sessions.refresh": true,
}

// quickConfirm are the commands whose confirmed action is quick and local:
// it runs at once, on the UI goroutine. Any other runs in the background.
var quickConfirm = map[string]bool{"session.archive": true, "sessions.archive_stale": true, "group.dissolve": true}

// commandDoneMsg is a core command's answer that came back off the UI
// goroutine.
type commandDoneMsg struct {
	name string
	res  app.Result
	err  error
}

// commandResult turns a core command's Result into what the TUI does: an
// error or a toast, a confirmation (a dialog answered with its token), a
// refresh, panes closed, the todos redrawn, an agent announced, a pane shown.
func (m *Model) commandResult(name string, res app.Result, err error) tea.Cmd {
	if err != nil {
		return toastCmd(errText(res, err), true)
	}
	if res.Pending != nil {
		m.askApp(res, !quickConfirm[name])
		return nil
	}
	var cmds []tea.Cmd
	if res.Toast != "" {
		cmds = append(cmds, toastCmd(res.Toast, res.Err))
	}
	switch d := res.Data.(type) {
	case app.SessionEffect:
		for _, id := range d.ClosePanes {
			m.closePane(id)
		}
		m.pullSnap() // the command changed the core's sessions: show it now
		if d.Refresh {
			cmds = append(cmds, m.refresh())
		}
	case app.TodoList:
		m.rebuildTodos()
	case app.Dispatched:
		cmds = append(cmds, func() tea.Msg { return resultMsg(res, nil) })
	case app.AttachPlan:
		cmds = append(cmds, m.showPlan(d))
	}
	if res.Exec != nil {
		cmds = append(cmds, toastCmd(name+": tela cheia só pela tecla O", true))
	}
	return tea.Batch(cmds...)
}

// ---- the core's event bus ----

// busMsg is an event from the core's bus the TUI follows, and the
// subscription to keep listening on.
type busMsg struct {
	ev core.Event
	ch <-chan core.Event
}

// followBus subscribes to the core's event bus for the program's life.
func (m *Model) followBus() tea.Cmd { return listenBus(m.app.Subscribe(context.Background())) }

// listenBus waits for the next event the TUI reacts to. Only
// confirm.resolved is followed: the TUI's own commands already drew their
// toasts and refreshes from their Results, so the other events (toast,
// dispatched, snapshot.changed…) would only repeat them. A subscription the
// bus cut (closed) ends the listening.
func listenBus(ch <-chan core.Event) tea.Cmd {
	return func() tea.Msg {
		for ev := range ch {
			if ev.Name == app.EvConfirmResolved {
				return busMsg{ev: ev, ch: ch}
			}
		}
		return nil
	}
}

// onBus reacts to a core event: a confirmation answered or expired, maybe
// by another client, closes its dialog here.
func (m *Model) onBus(ev core.Event) tea.Cmd {
	if r, ok := ev.Data.(app.ConfirmResolved); ok {
		return m.onConfirmResolved(confirmResolvedMsg{token: r.Token})
	}
	return nil
}

// errText is a failed command's message: the toast the core wrote for it,
// else the error.
func errText(res app.Result, err error) string {
	if res.Toast != "" {
		return res.Toast
	}
	return err.Error()
}

// showPlan shows a pane a core command planned: the open one, or a new one.
func (m *Model) showPlan(plan app.AttachPlan) tea.Cmd {
	if !plan.Reuse {
		return m.startPane(plan)
	}
	m.active = plan.ID
	if plan.Focus {
		m.mode = modeInsert
	}
	if plan.Reply != "" {
		return m.sendReplyLater(plan.ID, plan.ReplyIn)
	}
	return nil
}
