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

	"github.com/aqilputi/neko/internal/config"
	"github.com/aqilputi/neko/internal/groups"
	"github.com/aqilputi/neko/internal/store"
	"github.com/aqilputi/neko/internal/theme"
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
	q   string
	yes func(m *Model) tea.Cmd
}

func (d *confirmDialog) title() string { return "confirma?" }

func (d *confirmDialog) width(m *Model) int { return min(max(lipgloss.Width(d.q)+6, 40), m.w-4) }

func (d *confirmDialog) handle(m *Model, k tea.KeyPressMsg) (bool, tea.Cmd) {
	switch k.String() {
	case "y", "Y", "enter":
		return true, d.yes(m)
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
	return lipgloss.NewStyle().Background(t.Bg).Padding(0, 1).Render(h.FullHelpView(fullHelp()) + "\n\n" + extra)
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
	if m.usage.Raw == "" {
		msg := "consultando claude -p /usage…"
		if m.usageErr != nil {
			msg = "não consegui ler o /usage: " + m.usageErr.Error()
		}
		return b.Foreground(t.Muted).Padding(0, 1).Render(msg)
	}
	var lines []string
	for _, l := range m.usage.Limits {
		lines = append(lines, b.Foreground(t.Text).Bold(true).Width(30).Render(" "+l.Name)+
			costBar(t, float64(l.Pct), 100, 24, b)+
			b.Foreground(usageColor(t, l.Pct)).Bold(true).Render(fmt.Sprintf(" %3d%%", l.Pct))+
			b.Foreground(t.Muted).Render("  reseta "+l.Resets))
	}
	lines = append(lines, "")
	rest := m.usage.Raw
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
	lines = append(lines, "", b.Foreground(t.Muted).Italic(true).Render(fmt.Sprintf(" atualizado há %s · a cada %.0fs", ago(m.usage.At), m.cfg.UsageRefresh)))
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
	q := "sair do neko?"
	if running > 0 {
		q = fmt.Sprintf("sair do neko? %d sessões seguem rodando em background (claude attach volta nelas)", running)
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

// paletteItems: every key action (run by synthesizing its key), every visible
// session and task to jump to, and every theme.
func (m *Model) paletteItems() []paletteItem {
	var items []paletteItem
	seen := map[string]bool{}
	for _, group := range fullHelp() {
		for _, b := range group {
			h := b.Help()
			if seen[h.Desc] || len(b.Keys()) == 0 || b.Keys()[0] == ":" || h.Key == "gg" || h.Key == "dd" {
				continue
			}
			seen[h.Desc] = true
			k := keyPress(b.Keys()[0])
			items = append(items, paletteItem{title: h.Desc, hint: h.Key, run: func(m *Model) tea.Cmd {
				_, cmd := m.keyNormal(k)
				return cmd
			}})
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
	for _, tag := range m.grp.Tags() {
		tg := tag
		items = append(items, paletteItem{title: fmt.Sprintf("desfazer grupo #%s (%d sessões)", tg, len(m.grp.Groups[tg])), hint: "grupos", run: func(m *Model) tea.Cmd {
			m.confirmThen("desfazer o grupo #"+tg+"? as sessões são avisadas na próxima ferramenta", func(m *Model) tea.Cmd {
				m.grp = groups.Load()
				delete(m.grp.Groups, tg)
				if err := m.grp.Save(); err != nil {
					return toastCmd(err.Error(), true)
				}
				return toastCmd("#"+tg+" desfeito", false)
			})
			return nil
		}})
	}
	items = append(items, paletteItem{title: "limpar chats encerrados (claude rm)", hint: "processos que sobraram",
		run: func(m *Model) tea.Cmd { return m.removeEnded() }})
	items = append(items, paletteItem{title: "arquivar todas as sessões paradas", hint: fmt.Sprintf("> %.0fd sem atividade", m.cfg.StaleDays),
		run: func(m *Model) tea.Cmd { return m.archiveStale() }})
	for i := range m.snap.Sessions {
		s := m.snap.Sessions[i]
		if s.Stale && !m.showAll {
			continue
		}
		sid := s.SessionID
		items = append(items, paletteItem{title: "ir: " + s.Label(), hint: s.Context + " · " + config.Tilde(s.Cwd), run: func(m *Model) tea.Cmd {
			m.tab = tabSessions
			for j, r := range m.rows {
				if r.kind == "sess" && r.sess.SessionID == sid {
					m.cursor = j
				}
			}
			return nil
		}})
	}
	for i, r := range m.todos {
		idx := i
		title, hint := "", ""
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
		if it := m.groupCommand(q); it != nil {
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
	d.cur = clamp(d.cur, 0, len(d.matches)-1)
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
		d.cur = clamp(d.cur+1, 0, len(d.matches)-1)
		return false, nil
	case "up", "ctrl+p", "ctrl+k":
		d.cur = clamp(d.cur-1, 0, len(d.matches)-1)
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
		for j, r := range []rune(truncate(it.title, w-24)) {
			c := st.Foreground(t.Text)
			if hit[j] {
				c = st.Foreground(t.Accent).Bold(true)
			}
			title.WriteString(c.Render(string(r)))
		}
		hint := st.Foreground(t.Muted).Render(truncate(it.hint, 20))
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
					if r.kind == "sess" && r.sess.SessionID == sess.SessionID {
						m.cursor = j
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
