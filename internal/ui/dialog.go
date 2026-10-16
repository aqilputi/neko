package ui

import (
	"fmt"
	"image/color"
	"sort"
	"strings"

	"charm.land/bubbles/v2/help"
	"charm.land/bubbles/v2/textinput"
	tea "charm.land/bubbletea/v2"
	"charm.land/lipgloss/v2"
	"github.com/sahilm/fuzzy"

	"github.com/aqilputi/nekuroddo/internal/app"
	"github.com/aqilputi/nekuroddo/internal/config"
	"github.com/aqilputi/nekuroddo/internal/core"
	"github.com/aqilputi/nekuroddo/internal/store"
	"github.com/aqilputi/nekuroddo/internal/theme"
)

// dialog is a box drawn over the UI. While one is open every key goes to
// the top of the stack.
type dialog interface {
	title() string
	// handle reacts to a key; done closes the dialog.
	handle(m *Model, k tea.KeyPressMsg) (done bool, cmd tea.Cmd)
	view(m *Model, t theme.Theme, w int) string
	width(m *Model) int
}

func (m *Model) push(d dialog) { m.dialogs = append(m.dialogs, d) }

func (m *Model) top() dialog {
	if len(m.dialogs) == 0 {
		return nil
	}
	return m.dialogs[len(m.dialogs)-1]
}

func (m *Model) pop() { m.dialogs = m.dialogs[:len(m.dialogs)-1] }

// dialogBox renders a dialog with its title in the top border.
func dialogBox(t theme.Theme, title, body string, w int) string {
	return panel(t, w, lipgloss.Height(body)+2, true, title, body)
}

// ---- confirm ----

type confirmDialog struct {
	q     string
	yes   func(m *Model) tea.Cmd
	no    func(m *Model) // optional: any other key
	token string         // a confirmation parked in the core (app.Ask); "" for a local one
}

func (d *confirmDialog) title() string { return "confirma?" }

// confirmFooterW is the answer line's width (" y/enter sim    qualquer outra
// tecla: não"); the box is never narrower than it.
var confirmFooterW = lipgloss.Width(" y/enter sim    qualquer outra tecla: não")

func (d *confirmDialog) width(m *Model) int {
	return min(max(lipgloss.Width(d.q), confirmFooterW)+6, m.w-4)
}

func (d *confirmDialog) handle(m *Model, k tea.KeyPressMsg) (bool, tea.Cmd) {
	switch k.String() {
	case "y", "Y", "enter":
		return true, d.yes(m)
	}
	if d.no != nil {
		d.no(m)
	}
	return true, nil
}

func (d *confirmDialog) view(m *Model, t theme.Theme, w int) string {
	b := lipgloss.NewStyle().Background(t.Bg)
	return b.Foreground(t.Text).Width(w-4).Padding(0, 1).Render(d.q) + "\n\n" +
		b.Foreground(t.Done).Bold(true).Padding(0, 1).Render("y/enter sim") + b.Foreground(t.Muted).Render("   qualquer outra tecla: não")
}

// confirmThen asks before doing something with consequences. yes runs on
// the live model, so it may change it.
func (m *Model) confirmThen(q string, yes func(m *Model) tea.Cmd) {
	m.push(&confirmDialog{q: q, yes: yes})
}

// confirmToken shows the same dialog for a confirmation parked in the core
// (app.Ask): yes answers app.Confirm(token) in the background and turns its
// Result into messages; any other key answers app.Cancel(token).
func (m *Model) confirmToken(q, token string) {
	a := m.app
	m.push(&confirmDialog{q: q, token: token,
		yes: func(*Model) tea.Cmd {
			return func() tea.Msg { return resultMsg(a.Confirm(token)) }
		},
		no: func(*Model) { _ = a.Cancel(token) }, // already gone (expired/answered elsewhere): nothing to do
	})
}

// confirmResolvedMsg says a confirmation parked in the core was answered or
// expired (the confirm.resolved event), maybe by another client.
type confirmResolvedMsg struct{ token string }

// onConfirmResolved closes the dialogs still asking for token: the question
// is no longer open. The TUI follows the core's bus (followBus) and turns
// confirm.resolved into this message.
func (m *Model) onConfirmResolved(msg confirmResolvedMsg) tea.Cmd {
	kept := m.dialogs[:0]
	for _, d := range m.dialogs {
		if c, ok := d.(*confirmDialog); ok && msg.token != "" && c.token == msg.token {
			continue
		}
		kept = append(kept, d)
	}
	m.dialogs = kept
	return nil
}

// ---- help ----

type helpDialog struct{}

func (helpDialog) title() string                                  { return "ajuda · qualquer tecla fecha" }
func (helpDialog) width(m *Model) int                             { return min(m.w-4, 150) }
func (helpDialog) handle(*Model, tea.KeyPressMsg) (bool, tea.Cmd) { return true, nil }

func (helpDialog) view(m *Model, t theme.Theme, w int) string {
	h := help.New()
	h.SetWidth(w - 4)
	h.Styles = helpStyles(t)
	h.FullSeparator = "    "
	extra := lipgloss.NewStyle().Background(t.Bg).Foreground(t.Muted).Italic(true).Render(
		"mouse: clique seleciona, clique de novo abre, roda rola · no INSERT o mouse vai pro claude (shift+arrastar seleciona texto)")
	return lipgloss.NewStyle().Background(t.Bg).Padding(0, 1).Render(h.FullHelpView(m.fullHelp()) + "\n\n" + extra)
}

func helpStyles(t theme.Theme) help.Styles {
	b := lipgloss.NewStyle().Background(t.Bg)
	return help.Styles{
		Ellipsis: b.Foreground(t.Muted), ShortKey: b.Foreground(t.Accent).Bold(true), ShortDesc: b.Foreground(t.Subtle),
		ShortSeparator: b.Foreground(t.Overlay), FullKey: b.Foreground(t.Accent).Bold(true), FullDesc: b.Foreground(t.Text),
		FullSeparator: b,
	}
}

// ---- usage ----

// usageDialog shows `claude -p /usage` as Claude Code prints it, with a bar
// next to each limit.
type usageDialog struct{}

func (usageDialog) title() string                                  { return "uso · /usage · qualquer tecla fecha" }
func (usageDialog) width(m *Model) int                             { return min(m.w-4, 110) }
func (usageDialog) handle(*Model, tea.KeyPressMsg) (bool, tea.Cmd) { return true, nil }

func (usageDialog) view(m *Model, t theme.Theme, w int) string {
	b := lipgloss.NewStyle().Background(t.Bg)
	rep, err := m.app.Usage()
	if rep.Raw == "" {
		msg := "consultando claude -p /usage…"
		if err != nil {
			msg = "não consegui ler o /usage: " + err.Error()
		}
		return b.Foreground(t.Muted).Padding(0, 1).Render(msg)
	}
	var lines []string
	for _, l := range rep.Limits {
		lines = append(lines, b.Foreground(t.Text).Bold(true).Width(30).Render(" "+l.Name)+
			costBar(t, float64(l.Pct), 100, 24, b)+
			b.Foreground(usageColor(t, l.Pct)).Bold(true).Render(fmt.Sprintf(" %3d%%", l.Pct))+
			b.Foreground(t.Muted).Render("  reseta "+l.Resets))
	}
	lines = append(lines, "")
	rest := rep.Raw
	if i := strings.Index(rest, "What's contributing"); i >= 0 {
		rest = rest[i:]
	}
	maxLines := max(m.h-len(lines)-10, 5)
	for i, l := range strings.Split(rest, "\n") {
		if i >= maxLines {
			lines = append(lines, b.Foreground(t.Muted).Render(" …"))
			break
		}
		lines = append(lines, b.Foreground(t.Subtle).Width(w-4).Render(" "+l))
	}
	lines = append(lines, "", b.Foreground(t.Muted).Italic(true).Render(fmt.Sprintf(" atualizado há %s · a cada %.0fs", ago(rep.At), m.cfg.UsageRefresh)))
	return strings.Join(lines, "\n")
}

// usageColor goes green → yellow → red as a limit fills up.
func usageColor(t theme.Theme, pct int) color.Color {
	switch {
	case pct >= 90:
		return t.Error
	case pct >= 70:
		return t.Accent
	}
	return t.Done
}

// ---- quit ----

func (m *Model) askQuit() {
	running := 0
	for _, s := range m.snap.Sessions {
		if s.State == "busy" || s.State == "blocked" {
			running++
		}
	}
	q := "sair do nekuroddo?"
	if running > 0 {
		q = fmt.Sprintf("sair do nekuroddo? %d sessões seguem rodando em background (claude attach volta nelas)", running)
	}
	m.confirmThen(q, func(m *Model) tea.Cmd {
		for id := range m.panes {
			m.closePane(id)
		}
		return tea.Quit
	})
}

// ---- command palette ----

type paletteItem struct {
	title, hint string
	run         func(m *Model) tea.Cmd
}

type paletteDialog struct {
	name    string
	input   textinput.Model
	items   []paletteItem
	base    []paletteItem // items without the typed command
	matches []fuzzy.Match
	cur     int
	// commands: the palette also runs group commands typed with arguments
	commands bool
}

type paletteSource []paletteItem

func (s paletteSource) Len() int            { return len(s) }
func (s paletteSource) String(i int) string { return s[i].title + " " + s[i].hint }

func (m *Model) openPalette() tea.Cmd {
	ti := textinput.New()
	ti.Prompt = ": "
	ti.Placeholder = "ação, sessão, tarefa, tema · group <tag> · ungroup · notify #tag msg"
	d := &paletteDialog{name: "comandos", input: ti, items: m.paletteItems(), commands: true}
	d.filter(m)
	m.push(d)
	return d.input.Focus()
}

// paletteItems: every command the core lists for this selection
// (Registry.List: builtins and Lua alike, run by name), the group commands
// that take an argument, every visible session and task to jump to, and
// every theme.
func (m *Model) paletteItems() []paletteItem {
	var items []paletteItem
	cmds := m.app.Registry().List(m.commandCtx())
	sort.SliceStable(cmds, func(i, j int) bool { return helpRank(cmds[i].Name) < helpRank(cmds[j].Name) })
	for _, c := range cmds {
		if it, ok := m.commandItem(c); ok {
			items = append(items, it)
		}
	}
	// group commands take an argument: these reopen the palette with the
	// command typed, waiting for it
	what := "a sessão do cursor"
	if len(m.picked) > 0 {
		what = fmt.Sprintf("as %d selecionadas", len(m.picked))
	}
	items = append(items,
		paletteItem{title: "group <tag> — agrupa " + what, hint: "space seleciona", run: func(m *Model) tea.Cmd { return m.openPaletteWith("group ") }},
		paletteItem{title: "ungroup [tag] — tira " + what + " do grupo", hint: "grupos", run: func(m *Model) tea.Cmd { return m.openPaletteWith("ungroup ") }},
		paletteItem{title: "notify #tag <mensagem> — avisa o grupo", hint: "grupos", run: func(m *Model) tea.Cmd { return m.openPaletteWith("notify #") }})
	grp := m.app.Groups()
	for _, tag := range grp.Tags() {
		tg := tag
		items = append(items, paletteItem{title: fmt.Sprintf("desfazer grupo #%s (%d sessões)", tg, len(grp.Groups[tg])), hint: "grupos",
			run: func(m *Model) tea.Cmd { return m.runCore("group.dissolve", app.Args{"tag": tg}) }})
	}
	for i := range m.snap.Sessions {
		s := m.snap.Sessions[i]
		if s.Stale && !m.view.ShowAll {
			continue
		}
		sid := s.SessionID
		items = append(items, paletteItem{title: "ir: " + s.Label(), hint: s.Context + " · " + config.Tilde(s.Cwd), run: func(m *Model) tea.Cmd {
			m.tab = tabSessions
			for j, r := range m.rows {
				if r.Kind == core.RowSess && r.Sess.SessionID == sid {
					m.view.Cursor = j
				}
			}
			return nil
		}})
	}
	for i, r := range m.todos {
		idx := i
		var title, hint string
		if r.task != nil {
			title, hint = fmt.Sprintf("todo #%d %s", r.task.ID, r.task.Title), r.group
		} else {
			title, hint = r.issue.Key+" "+r.issue.Summary, "jira · "+r.issue.Status
		}
		items = append(items, paletteItem{title: title, hint: hint, run: func(m *Model) tea.Cmd {
			m.tab = tabTodos
			m.todoCur = idx
			return nil
		}})
	}
	for i, th := range theme.All {
		idx := i
		items = append(items, paletteItem{title: "tema: " + th.Name, run: func(m *Model) tea.Cmd {
			m.th, m.themeSet = idx, true
			return nil
		}})
	}
	return items
}

// paletteHints are the palette hints of commands with no key.
var paletteHints = map[string]func(m *Model) string{
	"sessions.remove_ended":  func(*Model) string { return "processos que sobraram" },
	"sessions.archive_stale": func(m *Model) string { return fmt.Sprintf("> %.0fd sem atividade", m.cfg.StaleDays) },
}

// commandItem is c as a palette item: its help text (or title), its key as
// the hint, run by name. Left out: the palette itself, the two-key
// sequences (gg, dd: their keys are quicker), and the group commands (the
// palette offers them typed, with their argument).
func (m *Model) commandItem(c core.Command) (paletteItem, bool) {
	if c.Name == "palette.open" || strings.HasPrefix(c.Name, "group.") {
		return paletteItem{}, false
	}
	for _, k := range c.Keys {
		if core.IsSequence(k) {
			return paletteItem{}, false
		}
	}
	it := paletteItem{title: c.Title, hint: c.Hint}
	if d, ok := helpDesc[c.Name]; ok {
		it.title = d
	}
	key := ""
	switch {
	case len(c.Keys) > 0:
		key = c.Keys[0]
		it.hint = key
	case paletteHints[c.Name] != nil:
		it.hint = paletteHints[c.Name](m)
	}
	name := c.Name
	if _, own := clientHandlers[name]; !own && hasRequired(c) {
		// arguments first: the palette waits for them typed after the name
		it.run = func(m *Model) tea.Cmd { return m.openPaletteWith(name + " ") }
		return it, true
	}
	it.run = func(m *Model) tea.Cmd {
		_, cmd := m.runCommand(name, key, nil)
		return cmd
	}
	return it, true
}

func hasRequired(c core.Command) bool {
	for _, a := range c.Args {
		if a.Required {
			return true
		}
	}
	return false
}

// typedCommand is a command typed with its arguments: the group commands
// (group <tag>, ungroup [tag], notify [#tag] msg, as groupCommand shows
// them) or any command with arguments as "<name> <arg>…". nil falls back to
// fuzzy search.
func (m *Model) typedCommand(q string) *paletteItem {
	if it := m.groupCommand(q); it != nil {
		if it.run != nil {
			name, args := m.groupArgs(q)
			it.run = func(m *Model) tea.Cmd { return m.runTyped(name, args) }
		}
		return it
	}
	return m.argCommand(q)
}

// groupArgs maps a typed group command to its core command and arguments.
func (m *Model) groupArgs(q string) (string, app.Args) {
	f := strings.Fields(q)
	switch f[0] {
	case "group", "g":
		return "group.add", app.Args{"tag": core.CleanTag(strings.Join(f[1:], " "))}
	case "ungroup", "ug":
		args := app.Args{}
		if len(f) > 1 {
			args["tag"] = core.CleanTag(f[1])
		}
		return "group.remove", args
	}
	tag, text := "", strings.TrimSpace(strings.TrimPrefix(q, f[0]))
	if len(f) > 1 && strings.HasPrefix(f[1], "#") {
		tag = core.CleanTag(f[1])
		text = strings.TrimSpace(strings.TrimPrefix(text, f[1]))
	}
	if sel := m.selected(); tag == "" && sel != nil {
		if tags := m.app.TagsOf(sel.SessionID); len(tags) > 0 {
			tag = tags[0]
		}
	}
	return "group.notify", app.Args{"tag": tag, "text": text}
}

// runTyped runs a command typed in the palette. Grouping drops the picks it
// used; a note to a group makes the cat say so.
func (m *Model) runTyped(name string, args app.Args) tea.Cmd {
	res, err := m.app.Run(m.commandCtx(), name, args)
	if err == nil {
		switch name {
		case "group.add", "group.remove":
			m.picked = map[string]bool{}
		case "group.notify":
			m.cat.Meow(core.Truncate("✉ você → #"+args["tag"], 50))
		}
	}
	return m.commandResult(name, res, err)
}

// argCommand is "<name> <arg>…" for a command with arguments: the words
// fill its arguments in order, the last one taking the rest. Until the
// required ones are there it only shows what it waits for.
func (m *Model) argCommand(q string) *paletteItem {
	f := strings.Fields(q)
	if len(f) == 0 {
		return nil
	}
	c, ok := m.app.Registry().Lookup(f[0])
	if !ok || len(c.Args) == 0 || c.Scope != core.ScopeCore {
		return nil
	}
	args, rest := app.Args{}, strings.TrimSpace(strings.TrimPrefix(q, f[0]))
	var shown []string
	for i, a := range c.Args {
		v := rest
		if i < len(c.Args)-1 {
			v, rest, _ = strings.Cut(rest, " ")
			rest = strings.TrimSpace(rest)
		}
		if v == "" {
			if a.Required {
				return &paletteItem{title: c.Name + " " + argsUsage(c), hint: c.Title}
			}
			continue
		}
		args[a.Name] = v
		shown = append(shown, v)
	}
	name := c.Name
	return &paletteItem{title: "▶ " + name + " " + strings.Join(shown, " "), hint: c.Title,
		run: func(m *Model) tea.Cmd { return m.runTyped(name, args) }}
}

// argsUsage is "<required> [optional]…".
func argsUsage(c core.Command) string {
	var u []string
	for _, a := range c.Args {
		if a.Required {
			u = append(u, "<"+a.Name+">")
		} else {
			u = append(u, "["+a.Name+"]")
		}
	}
	return strings.Join(u, " ")
}

// filter matches what was typed. A command with arguments (group <tag>,
// ungroup, notify #tag msg) shows on top as the item enter runs.
func (d *paletteDialog) filter(m *Model) {
	q := d.input.Value()
	if d.base == nil {
		d.base = d.items
	}
	d.items = d.base
	d.matches = d.matches[:0]
	if d.commands {
		if it := m.typedCommand(q); it != nil {
			d.items = append(d.base[:len(d.base):len(d.base)], *it)
			d.matches = append(d.matches, fuzzy.Match{Index: len(d.base)})
		}
	}
	if q == "" {
		for i := range d.base {
			d.matches = append(d.matches, fuzzy.Match{Index: i})
		}
	} else {
		d.matches = append(d.matches, fuzzy.FindFrom(q, paletteSource(d.base))...)
	}
	d.cur = core.Clamp(d.cur, 0, len(d.matches)-1)
}

// openPaletteWith opens the palette with a command already typed.
func (m *Model) openPaletteWith(q string) tea.Cmd {
	cmd := m.openPalette()
	if d, ok := m.top().(*paletteDialog); ok {
		d.input.SetValue(q)
		d.input.CursorEnd()
		d.filter(m)
	}
	return cmd
}

func (d *paletteDialog) title() string      { return d.name }
func (d *paletteDialog) width(m *Model) int { return min(m.w-4, 90) }

func (d *paletteDialog) handle(m *Model, k tea.KeyPressMsg) (bool, tea.Cmd) {
	switch k.String() {
	case "esc", "ctrl+c":
		return true, nil
	case "enter":
		if d.cur < len(d.matches) {
			it := d.items[d.matches[d.cur].Index]
			if it.run == nil { // a command still missing its argument
				return false, nil
			}
			m.pop() // close before running: the action may open a dialog
			return false, it.run(m)
		}
		return true, nil
	case "down", "ctrl+n", "ctrl+j":
		d.cur = core.Clamp(d.cur+1, 0, len(d.matches)-1)
		return false, nil
	case "up", "ctrl+p", "ctrl+k":
		d.cur = core.Clamp(d.cur-1, 0, len(d.matches)-1)
		return false, nil
	}
	var cmd tea.Cmd
	d.input, cmd = d.input.Update(k)
	d.filter(m)
	return false, cmd
}

func (d *paletteDialog) view(m *Model, t theme.Theme, w int) string {
	b := lipgloss.NewStyle().Background(t.Bg)
	d.input.SetWidth(w - 8)
	lines := []string{b.Padding(0, 1).Render(d.input.View()), ""}
	rows := min(12, max(m.h-12, 3))
	start, end := scroll(d.cur, len(d.matches), rows)
	for i := start; i < end; i++ {
		mt := d.matches[i]
		it := d.items[mt.Index]
		bg := t.Bg
		if i == d.cur {
			bg = t.Overlay
		}
		st := lipgloss.NewStyle().Background(bg)
		hit := map[int]bool{}
		for _, j := range mt.MatchedIndexes {
			hit[j] = true
		}
		var title strings.Builder
		for j, r := range []rune(core.Truncate(it.title, w-24)) {
			c := st.Foreground(t.Text)
			if hit[j] {
				c = st.Foreground(t.Accent).Bold(true)
			}
			title.WriteString(c.Render(string(r)))
		}
		hint := st.Foreground(t.Muted).Render(core.Truncate(it.hint, 20))
		gap := max(w-4-lipgloss.Width(title.String())-lipgloss.Width(hint)-2, 1)
		lines = append(lines, st.Render(" ")+title.String()+st.Render(strings.Repeat(" ", gap))+hint+st.Render(" "))
	}
	if len(d.matches) == 0 {
		lines = append(lines, b.Foreground(t.Muted).Padding(0, 1).Render("nada encontrado"))
	}
	lines = append(lines, "", b.Foreground(t.Muted).Italic(true).Padding(0, 1).Render("↑/↓ ctrl+n/p · enter executa · esc fecha"))
	return strings.Join(lines, "\n")
}

// ---- ctrl+k: jump to a live chat ----

// openSwitcher lists only the chats that are running or waiting on you,
// most urgent first; enter opens the chosen one focused.
func (m *Model) openSwitcher() tea.Cmd {
	var live []store.Session
	for _, s := range m.snap.Sessions {
		if s.Ended || s.Archived {
			continue
		}
		if s.State == "busy" || s.State == "blocked" || s.Attention != store.AttnNone {
			live = append(live, s)
		}
	}
	sort.SliceStable(live, func(i, j int) bool {
		a, b := live[i], live[j]
		if (a.Attention == store.AttnNone) != (b.Attention == store.AttnNone) {
			return a.Attention != store.AttnNone // waiting on you first
		}
		return a.LastAt.After(b.LastAt)
	})
	var items []paletteItem
	for _, s := range live {
		sess := s
		icon, _ := stateIcon(m.t(), &sess, 0)
		items = append(items, paletteItem{
			title: icon + " " + sess.Label(),
			hint:  stateLabel(&sess) + " · " + sess.Context,
			run: func(m *Model) tea.Cmd {
				m.tab = tabSessions
				m.rebuild()
				for j, r := range m.rows {
					if r.Kind == core.RowSess && r.Sess.SessionID == sess.SessionID {
						m.view.Cursor = j
					}
				}
				if sess.Kind == "interactive" {
					return toastCmd(fmt.Sprintf("sessão interativa aberta em outro terminal (pid %d)", sess.PID), false)
				}
				return m.openPane(sess, true, "")
			},
		})
	}
	ti := textinput.New()
	ti.Prompt = "⌕ "
	ti.Placeholder = fmt.Sprintf("%d chats rodando ou te esperando…", len(items))
	d := &paletteDialog{name: "ir para chat · ctrl+k", input: ti, items: items}
	d.filter(m)
	m.push(d)
	return d.input.Focus()
}
