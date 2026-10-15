package ui

import (
	"errors"
	"fmt"
	"os/exec"
	"sort"
	"strings"
	"time"

	"charm.land/bubbles/v2/key"
	tea "charm.land/bubbletea/v2"

	"github.com/aqilputi/neko/internal/claude"
	"github.com/aqilputi/neko/internal/config"
	"github.com/aqilputi/neko/internal/events"
	"github.com/aqilputi/neko/internal/jira"
	"github.com/aqilputi/neko/internal/store"
	"github.com/aqilputi/neko/internal/term"
	"github.com/aqilputi/neko/internal/theme"
	"github.com/aqilputi/neko/internal/todo"
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
		var next tea.Cmd
		if !msg.manual && m.cfg.UsageRefresh > 0 {
			next = tea.Tick(time.Duration(m.cfg.UsageRefresh*float64(time.Second)), func(time.Time) tea.Msg { return usageTickMsg{} })
		}
		if msg.err != nil {
			m.usageErr = msg.err
			return next
		}
		m.usage, m.usageErr = msg.rep, nil
		m.store.SetWindowStart(m.usage.WindowStart())
		// the cat warns once per threshold of the 5-hour window
		if l, ok := m.usage.Find("session"); ok {
			for _, th := range []int{95, 80} {
				if l.Pct >= th && m.usageWarned < th {
					m.usageWarned = th
					m.cat.Meow(fmt.Sprintf("miau! %d%% da janela de 5h (reseta %s)", l.Pct, l.ShortReset()))
					return tea.Batch(next, toastCmd(fmt.Sprintf("janela de 5h em %d%% — reseta %s", l.Pct, l.ShortReset()), true))
				}
			}
			if l.Pct < 80 {
				m.usageWarned = 0 // new window
			}
		}
		return next

	case usageTickMsg:
		return m.fetchUsage(false)

	case eventsTickMsg:
		tail := m.evTail
		return func() tea.Msg { return eventsMsg(tail.Poll()) }

	case eventsMsg:
		return tea.Batch(m.onEvents([]events.Event(msg)), m.scheduleEvents())

	case todosMsg:
		m.todoErr = msg.err
		if msg.err == nil {
			m.tasks = msg.groups
		}
		m.rebuildTodos()
		return nil

	case jiraMsg:
		m.jiraLoading = false
		m.jiraErr = msg.err
		if msg.err != nil {
			return toastCmd("jira: "+msg.err.Error(), true)
		}
		m.jira = msg.cache
		m.rebuildTodos()
		return toastCmd(fmt.Sprintf("jira: %d issues (custou $%.2f)", len(m.jira.Issues), m.jira.CostUSD), false)

	case jiraCreatedMsg:
		if errors.Is(msg.err, jira.ErrNeedsApproval) {
			if m.cfg.Jira.ApproveHint == "" {
				return stickyToast("um hook bloqueou a escrita no Jira — libere a escrita e aperte J de novo")
			}
			return stickyToast("o guard bloqueou a escrita no Jira — rode no Claude Code:  " + m.cfg.Jira.ApproveHint + "  e aperte J de novo")
		}
		if msg.err != nil {
			return stickyToast("jira: " + msg.err.Error())
		}
		id, key := msg.task, msg.key
		return tea.Batch(
			toastCmd(fmt.Sprintf("%s criada e ligada ao #%d ($%.2f)", key, id, msg.cost), false),
			func() tea.Msg {
				_ = todo.LinkJira(id, key)
				g, err := todo.List()
				return todosMsg{g, err}
			},
			m.fetchJira(true))

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

	case term.OutputMsg:
		if p := m.panes[msg.ID]; p != nil {
			cmds := []tea.Cmd{p.Listen()}
			// claude copied something (OSC 52): hand it to the real terminal
			for _, c := range p.TakeClipboard() {
				cmds = append(cmds, tea.SetClipboard(c), toastCmd(fmt.Sprintf("copiado (%d caracteres)", len([]rune(c))), false))
			}
			return tea.Batch(cmds...)
		}
		return nil

	case term.ExitMsg:
		// `claude attach` returned (← in claude, or the session ended).
		m.closePane(msg.ID)
		return m.refresh()

	case replyMsg:
		if p := m.panes[msg.pane]; p != nil {
			if text := m.pending[msg.pane]; text != "" {
				delete(m.pending, msg.pane)
				p.Paste(text)
				return tea.Tick(150*time.Millisecond, func(time.Time) tea.Msg { return enterMsg{msg.pane} })
			}
		}
		return nil

	case enterMsg:
		if p := m.panes[msg.pane]; p != nil {
			p.Type("\r")
		}
		return toastCmd("resposta enviada ✓", false)

	case paneOpenMsg:
		if msg.err != nil {
			return toastCmd(msg.err.Error(), true)
		}
		s := msg.sess
		s.ShortID = msg.id
		return tea.Batch(m.startPane(s, msg.id, true, msg.text), m.refresh())

	case dispatchedMsg:
		if msg.err != nil {
			return toastCmd(msg.err.Error(), true)
		}
		cmds := []tea.Cmd{m.refresh(), toastCmd("agent "+msg.id+" disparado", false)}
		if msg.task != nil {
			t := *msg.task
			id := msg.id
			cmds = append(cmds, func() tea.Msg {
				_ = todo.Start(t.ID)
				_ = todo.AddNote(t.ID, "neko: agent "+id+" disparado (claude attach "+id+")")
				g, err := todo.List()
				return todosMsg{g, err}
			})
		}
		return tea.Batch(cmds...)

	case execDoneMsg:
		return m.refresh()

	case tea.PasteMsg:
		if d, ok := m.top().(*paletteDialog); ok {
			var cmd tea.Cmd
			d.input, cmd = d.input.Update(msg)
			d.filter()
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
	if busy, waiting, _ := m.counts(); busy+waiting > 0 || m.cat.Awake() {
		d = 150 * time.Millisecond
	}
	return tea.Tick(d, func(time.Time) tea.Msg { return animMsg{gen} })
}

func (m *Model) scheduleEvents() tea.Cmd {
	return tea.Tick(400*time.Millisecond, func(time.Time) tea.Msg { return eventsTickMsg{} })
}

// onEvents reacts to hook events: refresh now, and meow when one needs you.
func (m *Model) onEvents(evs []events.Event) tea.Cmd {
	if len(evs) == 0 {
		return nil
	}
	m.hooksSeen = true
	var cmds []tea.Cmd
	for _, e := range evs {
		m.events[e.SessionID] = e
		if e.NeedsYou() {
			msg := "miau! " + e.Message
			if e.Message == "" {
				msg = "miau! uma sessão precisa de você"
			}
			m.cat.Meow(truncate(msg, 50))
			if !m.focused {
				cmds = append(cmds, tea.Raw("\a"), notifyDesktop("neko", e.Message))
			}
		}
	}
	if m.refreshing {
		m.refreshAgain = true
	} else {
		cmds = append(cmds, m.refresh())
	}
	return tea.Batch(cmds...)
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
		// ctrl+k is neko's even inside claude (claude loses its kill-line):
		// jump to another live chat without leaving INSERT first.
		if key.Matches(k, keys.Switch) {
			return m.openSwitcher()
		}
		if p := m.panes[m.active]; p != nil {
			// classic renderer: neko scrolls the scrollback (fullscreen claude
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
				m.filter = ""
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
			m.filter = m.input.Value()
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
		m.showAll = !m.showAll
		m.rebuild()
		m.rebuildTodos()
		if m.showAll {
			return true, toastCmd("mostrando tudo, inclusive paradas e arquivadas", false)
		}
		return true, toastCmd(fmt.Sprintf("só as ativas (≤ %.0fd)", m.cfg.StaleDays), false)
	case key.Matches(k, km.Sort):
		m.sortMode = (m.sortMode + 1) % len(sortModes)
		m.resetOrder()
		m.rebuild()
		return true, toastCmd("ordem: "+sortModes[m.sortMode], false)
	case key.Matches(k, km.Archive):
		return true, m.toggleArchive()
	case key.Matches(k, km.TabArchive):
		m.tab = tabArchive
	case !todos && key.Matches(k, km.Rename):
		if r := m.cur(); m.tab != tabInbox && r != nil && r.kind == "ctx" {
			m.draftDir = r.ctx
			return true, m.ask(inRenameCtx, "renomeia a seção \""+truncate(r.ctx, 30)+"\" (vazio desfaz) › ", r.ctx)
		}
		if sel := m.selected(); sel != nil {
			m.draftDir = sel.SessionID
			return true, m.ask(inRename, "renomeia (vazio volta ao nome do claude) › ", sel.Label())
		}
	case !todos && key.Matches(k, km.Move):
		if sel := m.selected(); sel != nil {
			m.draftDir = sel.SessionID
			hint := strings.Join(m.contextNames(), " · ")
			return true, m.ask(inMove, "contexto ("+truncate(hint, 50)+"; vazio volta à regra do diretório) › ", sel.Context)
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
	case key.Matches(k, km.Toggle):
		m.toggle()
	case key.Matches(k, km.NextAttn):
		m.jumpAttention(1)
	case key.Matches(k, km.PrevAttn):
		m.jumpAttention(-1)
	case key.Matches(k, km.Filter):
		m.tab = tabSessions
		return true, m.ask(inFilter, "/ ", m.filter)
	case key.Matches(k, km.Theme):
		m.themeSet = true
		m.th = (m.th + 1) % len(theme.All)
		return true, toastCmd("tema: "+theme.All[m.th].Name, false)
	case key.Matches(k, km.Refresh):
		m.resetOrder() // R also re-sorts the frozen order
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
			prompt := "↩ " + truncate(sel.Label(), 30)
			if n := optionCount(sel); n > 0 {
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
		if sel := m.selected(); sel != nil && sel.ShortID != "" {
			id, label := sel.ShortID, sel.Label()
			m.confirmThen("parar \""+truncate(label, 50)+"\"? (a conversa fica salva)", func(*Model) tea.Cmd {
				return func() tea.Msg {
					if err := claude.Stop(id); err != nil {
						return toastMsg{text: err.Error(), err: true}
					}
					return snapMsgCmd()
				}
			})
		}
	case key.Matches(k, km.Remove):
		m.pendingD = true
	case todos && key.Matches(k, km.Start):
		if t := m.curTodo(); t != nil {
			id := t.ID
			return true, func() tea.Msg {
				_ = todo.Start(id)
				g, err := todo.List()
				return todosMsg{g, err}
			}
		}
	case todos && key.Matches(k, km.Done):
		if t := m.curTodo(); t != nil {
			id, title := t.ID, t.Title
			m.confirmThen(fmt.Sprintf("marcar #%d \"%s\" como done?", id, truncate(title, 50)), func(*Model) tea.Cmd {
				return func() tea.Msg { _ = todo.Done(id); g, err := todo.List(); return todosMsg{g, err} }
			})
		}
	case k.String() == "esc":
		if m.filter != "" {
			m.filter = ""
			m.rebuild()
		}
	default:
		return false, nil
	}
	return true, nil
}

// snapMsgCmd is returned after a mutating CLI call to trigger a refresh.
func snapMsgCmd() tea.Msg { return tickMsg{} }

func (m *Model) removeSelected() tea.Cmd {
	sel := m.selected()
	if sel == nil || sel.ShortID == "" {
		return toastCmd("só sessões em background podem ser removidas", true)
	}
	id, label := sel.ShortID, sel.Label()
	m.confirmThen("remover \""+truncate(label, 50)+"\" (e o worktree, se seguro)?", func(m *Model) tea.Cmd {
		m.closePane(id)
		return func() tea.Msg {
			if err := claude.Remove(id); err != nil {
				return toastMsg{text: err.Error(), err: true}
			}
			return snapMsgCmd()
		}
	})
	return nil
}

// fullscreen hands the whole terminal to `claude attach` (or resume), and
// comes back when it exits — ← inside claude returns here.
func (m *Model) fullscreen(s store.Session) tea.Cmd {
	var c *exec.Cmd
	switch {
	case s.ShortID != "":
		c = exec.Command(claude.Bin(), "attach", s.ShortID)
	case s.Kind == "history":
		c = exec.Command(claude.Bin(), claude.AllowBypass, "--resume", s.SessionID)
	default:
		return toastCmd("sessão interativa aberta em outro terminal", true)
	}
	c.Dir = s.Cwd
	if s.ShortID != "" {
		m.closePane(s.ShortID)
	}
	m.store.MarkSeen(s.SessionID, time.Now())
	return tea.ExecProcess(c, func(err error) tea.Msg { return execDoneMsg{err} })
}

func (m *Model) submitInput() tea.Cmd {
	val := strings.TrimSpace(m.input.Value())
	m.input.Blur()
	m.mode = modeNormal
	switch m.purpose {
	case inFilter:
		m.filter = val
		m.rebuild()
	case inReply:
		if val == "" {
			return nil
		}
		for i := range m.snap.Sessions {
			if s := m.snap.Sessions[i]; s.SessionID == m.draftDir {
				return m.openPane(s, false, resolveOption(&s, val))
			}
		}
	case inNewDir:
		if val == "" {
			return nil
		}
		m.draftDir = config.Expand(val)
		m.draftTask = nil
		return m.ask(inNewPrompt, "prompt › ", "")
	case inJiraProject:
		t := m.curTodo()
		if t == nil || val == "" {
			return nil
		}
		proj, typ, _ := strings.Cut(val, "/")
		proj = strings.ToUpper(strings.TrimSpace(proj))
		typ = strings.TrimSpace(typ)
		if typ == "" {
			typ = m.cfg.Jira.DefaultType
		}
		task := *t
		n := jira.NewIssue{Project: proj, Type: typ, Summary: task.Title, Description: jiraDescription(task), Labels: []string{"neko"}}
		cfg := m.cfg.Jira
		m.confirmThen(fmt.Sprintf("criar no Jira %s/%s, atribuída a você:\n\"%s\"\n(escreve no Jira)", proj, typ, truncate(task.Title, 70)), func(*Model) tea.Cmd {
			return tea.Batch(toastCmd("criando no Jira via claude…", false), func() tea.Msg {
				key, cost, err := jira.Create(cfg, n)
				return jiraCreatedMsg{task: task.ID, key: key, cost: cost, err: err}
			})
		})
		return nil
	case inJiraAgentDir:
		if val == "" || m.draftIssue == nil {
			return nil
		}
		is := *m.draftIssue
		dir := config.Expand(val)
		return m.dispatch(dir, is.Key+" "+truncate(is.Summary, 40), jira.AgentPrompt(is), nil)
	case inDispatchDir:
		if val == "" || m.draftTask == nil {
			return nil
		}
		task := *m.draftTask
		dir := config.Expand(val)
		return m.dispatch(dir, fmt.Sprintf("todo #%d %s", task.ID, truncate(task.Title, 40)), todo.Prompt(task), &task)
	case inNewPrompt:
		if val == "" {
			return nil
		}
		return m.dispatch(m.draftDir, "", val, nil)
	case inRename:
		m.store.Rename(m.draftDir, val)
		return m.afterOverride("renomeada")
	case inMove:
		m.store.SetContext(m.draftDir, val)
		if val == "" {
			return m.afterOverride("contexto volta à regra do diretório")
		}
		return m.afterOverride("movida para " + val)
	case inRenameCtx:
		if val == m.draftDir {
			return nil
		}
		m.store.RenameContext(m.draftDir, val)
		if m.ctxColor != nil {
			if c, ok := m.ctxColor[m.draftDir]; ok && val != "" {
				m.ctxColor[val] = c // keep the section's color
			}
		}
		return m.afterOverride("seção renomeada")
	}
	return nil
}

// afterOverride shows a name/context change right away, without waiting
// for the next refresh.
func (m *Model) afterOverride(msg string) tea.Cmd {
	m.store.Reapply(m.snap.Sessions)
	m.rebuild()
	return tea.Batch(toastCmd(msg, false), m.refresh())
}

// contextNames lists the contexts in the current snapshot, for the move prompt.
func (m *Model) contextNames() []string {
	seen := map[string]bool{}
	var out []string
	for _, s := range m.snap.Sessions {
		if !seen[s.Context] {
			seen[s.Context] = true
			out = append(out, s.Context)
		}
	}
	sort.Strings(out)
	return out
}

func (m *Model) curRow() *todoRow {
	if m.todoCur < len(m.todos) {
		return &m.todos[m.todoCur]
	}
	return nil
}

func (m *Model) curTodo() *todo.Task {
	if r := m.curRow(); r != nil {
		return r.task
	}
	return nil
}

// jiraAction is J in the todos tab: a task goes to Jira (only when you ask),
// an issue comes into the todo list.
func (m *Model) jiraAction() tea.Cmd {
	if !m.cfg.Jira.Enabled {
		return toastCmd("jira desligado: configure [jira] no config.toml", true)
	}
	r := m.curRow()
	if r == nil {
		return nil
	}
	if r.issue != nil {
		if t := m.todoForKey(r.issue.Key); t != nil {
			return toastCmd(fmt.Sprintf("%s já está no todo #%d", r.issue.Key, t.ID), true)
		}
		key, sum := r.issue.Key, r.issue.Summary
		return func() tea.Msg {
			if err := todo.AddFromJira(key, sum); err != nil {
				return toastMsg{text: err.Error(), err: true}
			}
			g, err := todo.List()
			return todosMsg{g, err}
		}
	}
	if k := r.task.JiraKey(); k != "" {
		return toastCmd(fmt.Sprintf("#%d já tem issue: %s", r.task.ID, k), true)
	}
	def := m.cfg.Jira.DefaultProject + "/" + m.cfg.Jira.DefaultType
	return m.ask(inJiraProject, fmt.Sprintf("#%d no Jira — projeto/tipo › ", r.task.ID), def)
}

// jiraDescription is the issue body built from a task.
func jiraDescription(t todo.Task) string {
	var b strings.Builder
	if t.Desc != "" {
		b.WriteString(t.Desc + "\n\n")
	}
	for _, r := range t.Refs {
		fmt.Fprintf(&b, "- %s: %s %s\n", r.Kind, r.Value, r.URL)
	}
	for _, l := range t.Links {
		fmt.Fprintf(&b, "- %s\n", l)
	}
	for _, n := range t.Notes {
		fmt.Fprintf(&b, "\n> %s — %s\n", n.At[:min(10, len(n.At))], n.Text)
	}
	b.WriteString("\n_criada pelo neko a partir do todo #" + fmt.Sprint(t.ID) + "_")
	return b.String()
}

func openURL(u string) tea.Cmd {
	return func() tea.Msg {
		if err := exec.Command("xdg-open", u).Start(); err != nil {
			return toastMsg{text: err.Error(), err: true}
		}
		return nil
	}
}

func (m *Model) dispatchTodo() tea.Cmd {
	if r := m.curRow(); r != nil && r.issue != nil {
		m.draftIssue = r.issue
		dir := m.cfg.Jira.AgentDir
		if dir == "" {
			dir = "~/visio/dev"
		}
		return m.ask(inJiraAgentDir, "agent p/ "+r.issue.Key+" em › ", dir)
	}
	t := m.curTodo()
	if t == nil {
		return nil
	}
	dir := ""
	for _, c := range t.Contexts {
		if d := m.cfg.RepoFor(c); d != "" {
			dir = d
			break
		}
	}
	if dir == "" {
		dir = config.Expand("~")
	}
	m.draftTask = t
	return m.ask(inDispatchDir, fmt.Sprintf("agent p/ #%d em › ", t.ID), config.Tilde(dir))
}

// ---- navigation ----

func (m *Model) listHeight() int {
	_, _, ph := m.layout()
	return max(ph-4, 1)
}

func (m *Model) move(d int) {
	switch m.tab {
	case tabSessions, tabArchive:
		m.cursor = clamp(m.cursor+d, 0, len(m.rows)-1)
	case tabInbox:
		m.inboxCur = clamp(m.inboxCur+d, 0, len(m.inbox)-1)
	case tabTodos:
		m.todoCur = clamp(m.todoCur+d, 0, len(m.todos)-1)
	}
}

func clamp(v, lo, hi int) int {
	if hi < lo {
		return lo
	}
	return max(lo, min(v, hi))
}

func (m *Model) collapse() {
	r := m.cur()
	if r == nil || (m.tab != tabSessions && m.tab != tabArchive) {
		return
	}
	switch r.kind {
	case "ctx":
		m.collapsed["ctx:"+r.ctx] = true
	case "sess":
		if m.visibleSubs(r.sess) > 0 && !m.collapsed["sess:"+r.sess.SessionID] {
			m.collapsed["sess:"+r.sess.SessionID] = true
		} else {
			m.collapsed["ctx:"+r.ctx] = true
		}
	case "sub":
		m.collapsed["sess:"+r.sess.SessionID] = true
		m.rebuild()
		for i, x := range m.rows {
			if x.kind == "sess" && x.sess.SessionID == r.sess.SessionID {
				m.cursor = i
			}
		}
		return
	}
	m.rebuild()
}

func (m *Model) expand() {
	r := m.cur()
	if r == nil || (m.tab != tabSessions && m.tab != tabArchive) {
		return
	}
	switch r.kind {
	case "ctx":
		delete(m.collapsed, "ctx:"+r.ctx)
	case "sess":
		delete(m.collapsed, "sess:"+r.sess.SessionID)
	}
	m.rebuild()
}

func (m *Model) toggle() {
	r := m.cur()
	if r == nil {
		return
	}
	key := "ctx:" + r.ctx
	if r.kind != "ctx" {
		key = "sess:" + r.sess.SessionID
	}
	m.collapsed[key] = !m.collapsed[key]
	m.rebuild()
}

func (m *Model) jumpAttention(dir int) {
	if m.tab == tabInbox {
		m.move(dir)
		return
	}
	m.tab = tabSessions
	n := len(m.rows)
	for i := 1; i <= n; i++ {
		j := ((m.cursor+dir*i)%n + n) % n
		if r := m.rows[j]; r.kind == "sess" && r.sess.Attention != store.AttnNone {
			m.cursor = j
			return
		}
	}
}

func optionCount(s *store.Session) int {
	n := 0
	for _, q := range s.Questions {
		n += len(q.Options)
	}
	return n
}

// resolveOption turns a bare option number ("2") into that option's label,
// numbering options across all pending questions in order.
func resolveOption(s *store.Session, val string) string {
	var n int
	if _, err := fmt.Sscanf(val, "%d", &n); err != nil || fmt.Sprint(n) != val {
		return val
	}
	i := 0
	for _, q := range s.Questions {
		for _, o := range q.Options {
			i++
			if i == n {
				return o.Label
			}
		}
	}
	return val
}

// visibleSubs counts the subagent rows a session shows (finished ones hide
// unless H).
func (m *Model) visibleSubs(s *store.Session) int {
	n := 0
	for _, sa := range s.Subs {
		if !sa.Done || m.showAll {
			n++
		}
	}
	return n
}
