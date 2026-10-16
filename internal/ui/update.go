package ui

import (
	"fmt"
	"strings"
	"time"

	"charm.land/bubbles/v2/key"
	tea "charm.land/bubbletea/v2"

	"github.com/aqilputi/nekuroddo/internal/config"
	"github.com/aqilputi/nekuroddo/internal/core"
	"github.com/aqilputi/nekuroddo/internal/events"
	"github.com/aqilputi/nekuroddo/internal/store"
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
		m.w, m.h = msg.Width, msg.Height
		// Debounce: a window drag would otherwise SIGWINCH claude on every step.
		m.resizeSeq++
		seq := m.resizeSeq
		return tea.Tick(120*time.Millisecond, func(time.Time) tea.Msg { return resizeMsg{seq} })

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
		m.focused = true
		return nil

	case tea.BlurMsg:
		m.focused = false
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
		m.snap = store.Snapshot(msg)
		m.syncGroupNames()
		m.rebuild()
		cmds := []tea.Cmd{m.notice(), m.scheduleTick()}
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
		// ctrl+k is nekuroddo's even inside claude (claude loses its kill-line):
		// jump to another live chat without leaving INSERT first.
		if key.Matches(k, keys.Switch) {
			return m.openSwitcher()
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

// keyNormal handles NORMAL mode. The palette runs actions through it too.
func (m *Model) keyNormal(k tea.KeyPressMsg) (bool, tea.Cmd) {
	km := keys
	// two-key sequences: gg, dd
	if m.pendingG {
		m.pendingG = false
		if k.String() == "g" {
			m.move(-1 << 30)
			return true, nil
		}
	}
	if m.pendingD {
		m.pendingD = false
		if k.String() == "d" {
			return true, m.removeSelected()
		}
	}
	todos := m.tab == tabTodos

	switch {
	case key.Matches(k, km.Quit):
		if k.String() == "ctrl+c" && m.lastCtrlC.After(time.Now().Add(-time.Second)) {
			for id := range m.panes {
				m.closePane(id)
			}
			return true, tea.Quit
		}
		m.lastCtrlC = time.Now()
		m.askQuit()
	case key.Matches(k, km.Help):
		m.push(helpDialog{})
	case key.Matches(k, km.Usage):
		m.push(usageDialog{})
		return true, m.fetchUsage(true)
	case key.Matches(k, km.Palette):
		return true, m.openPalette()
	case key.Matches(k, km.Switch):
		return true, m.openSwitcher()
	case key.Matches(k, km.TabSessions):
		m.tab = tabSessions
	case key.Matches(k, km.TabInbox):
		m.tab = tabInbox
	case key.Matches(k, km.TabTodos):
		m.tab = tabTodos
		return true, tea.Batch(m.loadTodos(), m.fetchJira(false))
	case key.Matches(k, km.NextTab):
		m.tab = (m.tab + 1) % numTabs
		if m.tab == tabTodos {
			return true, tea.Batch(m.loadTodos(), m.fetchJira(false))
		}
	case key.Matches(k, km.PrevTab):
		m.tab = (m.tab + numTabs - 1) % numTabs
	case key.Matches(k, km.ShowAll):
		m.view.ShowAll = !m.view.ShowAll
		m.rebuild()
		m.rebuildTodos()
		if m.view.ShowAll {
			return true, toastCmd("mostrando tudo, inclusive paradas e arquivadas", false)
		}
		return true, toastCmd(fmt.Sprintf("só as ativas (≤ %.0fd)", m.cfg.StaleDays), false)
	case key.Matches(k, km.Sort):
		m.view.SortMode = (m.view.SortMode + 1) % len(core.SortModes)
		m.view = m.view.ResetOrder()
		m.rebuild()
		return true, toastCmd("ordem: "+core.SortModes[m.view.SortMode], false)
	case key.Matches(k, km.Archive):
		return true, m.toggleArchive()
	case key.Matches(k, km.TabArchive):
		m.tab = tabArchive
	case !todos && key.Matches(k, km.Rename):
		if r := m.cur(); m.tab != tabInbox && r != nil && r.Kind == core.RowCtx {
			m.draftDir = r.Ctx
			return true, m.ask(inRenameCtx, "renomeia a seção \""+core.Truncate(r.Ctx, 30)+"\" (vazio desfaz) › ", r.Ctx)
		}
		if sel := m.selected(); sel != nil {
			m.draftDir = sel.SessionID
			return true, m.ask(inRename, "renomeia (vazio volta ao nome do claude) › ", sel.Label())
		}
	case !todos && key.Matches(k, km.Move):
		if sel := m.selected(); sel != nil {
			m.draftDir = sel.SessionID
			hint := strings.Join(m.contextNames(), " · ")
			return true, m.ask(inMove, "contexto ("+core.Truncate(hint, 50)+"; vazio volta à regra do diretório) › ", sel.Context)
		}
	case todos && key.Matches(k, km.Jira):
		return true, m.jiraAction()
	case todos && key.Matches(k, km.JiraFetch):
		if !m.cfg.Jira.Enabled {
			return true, toastCmd("jira desligado: configure [jira] no config.toml", true)
		}
		return true, tea.Batch(toastCmd("buscando no Jira via claude…", false), m.fetchJira(true))
	case key.Matches(k, km.Down):
		m.move(1)
	case key.Matches(k, km.Up):
		m.move(-1)
	case key.Matches(k, km.HalfDown):
		m.move(m.listHeight() / 2)
	case key.Matches(k, km.HalfUp):
		m.move(-m.listHeight() / 2)
	case key.Matches(k, km.PageDown):
		m.move(m.listHeight())
	case key.Matches(k, km.PageUp):
		m.move(-m.listHeight())
	case key.Matches(k, km.Top):
		m.pendingG = true
	case key.Matches(k, km.Bottom):
		m.move(1 << 30)
	case key.Matches(k, km.Collapse):
		m.collapse()
	case key.Matches(k, km.Expand):
		m.expand()
	case key.Matches(k, km.Pick):
		m.togglePick()
	case key.Matches(k, km.NextAttn):
		m.jumpAttention(1)
	case key.Matches(k, km.PrevAttn):
		m.jumpAttention(-1)
	case key.Matches(k, km.Filter):
		m.tab = tabSessions
		return true, m.ask(inFilter, "/ ", m.view.Filter)
	case key.Matches(k, km.Theme):
		m.themeSet = true
		m.th = (m.th + 1) % len(theme.All)
		return true, toastCmd("tema: "+theme.All[m.th].Name, false)
	case key.Matches(k, km.Refresh):
		m.view = m.view.ResetOrder() // R also re-sorts the frozen order
		m.rebuild()
		return true, tea.Batch(m.refresh(), m.loadTodos())
	case key.Matches(k, km.Zoom):
		if m.active != "" {
			m.zoom = !m.zoom
			m.resizePanes()
		}
	case key.Matches(k, km.Close):
		if m.active != "" {
			m.closePane(m.active)
		}
	case key.Matches(k, km.Insert) && m.active != "":
		m.mode = modeInsert
	case key.Matches(k, km.Open, km.Insert):
		if todos {
			return true, m.dispatchTodo()
		}
		if sel := m.selected(); sel != nil {
			return true, m.openPane(*sel, true, "")
		}
	case key.Matches(k, km.Preview):
		if r := m.curRow(); todos && r != nil && r.issue != nil {
			return true, openURL(r.issue.URL)
		}
		if sel := m.selected(); sel != nil {
			return true, m.openPane(*sel, false, "")
		}
	case key.Matches(k, km.Full):
		if sel := m.selected(); sel != nil {
			return true, m.fullscreen(*sel)
		}
	case key.Matches(k, km.Reply):
		if sel := m.selected(); sel != nil {
			if sel.Kind == "interactive" {
				return true, toastCmd("sessão interativa: responda no terminal dela", true)
			}
			m.draftDir = sel.SessionID
			prompt := "↩ " + core.Truncate(sel.Label(), 30)
			if n := core.OptionCount(sel.Questions); n > 0 {
				prompt += fmt.Sprintf(" (1-%d escolhe opção)", n)
			}
			return true, m.ask(inReply, prompt+" › ", sel.SuggestedReply)
		}
	case key.Matches(k, km.New):
		dir := "~"
		if sel := m.selected(); sel != nil {
			dir = config.Tilde(sel.Cwd)
		}
		return true, m.ask(inNewDir, "diretório › ", dir)
	case key.Matches(k, km.Stop):
		m.stop()
	case key.Matches(k, km.Remove):
		m.pendingD = true
	case todos && key.Matches(k, km.Start):
		if cmd := m.startTodo(); cmd != nil {
			return true, cmd
		}
	case todos && key.Matches(k, km.Done):
		m.doneTodo()
	case k.String() == "esc":
		if len(m.picked) > 0 { // esc first drops the picks, then the filter
			m.picked = map[string]bool{}
			return true, nil
		}
		if m.view.Filter != "" {
			m.view.Filter = ""
			m.rebuild()
		}
	default:
		return false, nil
	}
	return true, nil
}
