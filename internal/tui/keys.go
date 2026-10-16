package tui

import (
	"fmt"

	"charm.land/bubbles/v2/key"
	tea "charm.land/bubbletea/v2"

	"github.com/aqilputi/nekuroddo/internal/core"
)

// keyMap is how the TUI shows its keys: the status-line hints and the help
// dialog. It is derived from the core's keymap (app.Keymap, built from the
// registered commands' Keys/Mode), so a key rebound there shows here too;
// keyNormal resolves keys against that keymap, not against these bindings.
// What stays here is presentation: which binding goes where (shortHelp,
// fullHelp), the TUI's help texts and the bar's short labels.
type keyMap struct {
	Up, Down, Top, Bottom, HalfDown, HalfUp, PageDown, PageUp     key.Binding
	Collapse, Expand, Pick, NextAttn, PrevAttn                    key.Binding
	TabSessions, TabInbox, TabTodos, TabArchive, NextTab, PrevTab key.Binding
	Filter, ShowAll, Sort, Archive, Rename, Move                  key.Binding

	Open, Preview, Full, Insert, Reply, New, Stop, Remove, Close, Zoom key.Binding

	Dispatch, Start, Done, Jira, JiraFetch, OpenURL key.Binding

	Palette, Switch, Usage, Theme, Refresh, Help, Quit key.Binding
}

func kb(keys []string, k, desc string) key.Binding {
	return key.NewBinding(key.WithKeys(keys...), key.WithHelp(k, desc))
}

func ks(k ...string) []string { return k }

// newKeyMap derives the TUI's bindings from the core's keymap.
func newKeyMap(km core.Keymap) keyMap {
	b := func(name string) key.Binding { return binding(km, name) }
	return keyMap{
		Up: b("nav.up"), Down: b("nav.down"), Top: b("nav.top"), Bottom: b("nav.bottom"),
		HalfDown: b("nav.half_down"), HalfUp: b("nav.half_up"), PageDown: b("nav.page_down"), PageUp: b("nav.page_up"),
		Collapse: b("nav.collapse"), Expand: b("nav.expand"), Pick: b("nav.pick"),
		NextAttn: b("nav.next_attention"), PrevAttn: b("nav.prev_attention"),
		TabSessions: b("tab.sessions"), TabInbox: b("tab.inbox"), TabTodos: b("tab.todos"), TabArchive: b("tab.archive"),
		NextTab: b("tab.next"), PrevTab: b("tab.prev"),
		Filter: b("view.filter"), ShowAll: b("view.show_all"), Sort: b("view.sort"), Archive: b("session.archive"),
		Rename: b("session.rename"), Move: b("session.move"),
		Open: b("session.open"), Preview: b("session.preview"), Full: b("session.fullscreen"), Insert: b("session.insert"),
		Reply: b("session.reply"), New: b("session.new"), Stop: b("session.stop"), Remove: b("session.remove"),
		Close: b("pane.close"), Zoom: b("pane.zoom"),
		Dispatch: b("todo.dispatch"), Start: b("todo.start"), Done: b("todo.done"), Jira: b("jira.toggle"),
		JiraFetch: b("jira.fetch"), OpenURL: b("jira.open_url"),
		Palette: b("palette.open"), Switch: b("switcher.open"), Usage: b("usage.show"), Theme: b("view.theme"),
		Refresh: b("sessions.refresh"), Help: b("help.open"), Quit: b("app.quit"),
	}
}

// binding is command name's key.Binding: its keys (a sequence such as "gg"
// matches its first press), its first key shown, with the TUI's help text.
// A command with no keys (unbound) is disabled, so it shows nowhere.
func binding(km core.Keymap, name string) key.Binding {
	bd, ok := km.Find(name)
	if !ok {
		return key.NewBinding(key.WithDisabled())
	}
	keys := make([]string, len(bd.Keys))
	for i, k := range bd.Keys {
		if core.IsSequence(k) {
			k = k[:len(k)/2]
		}
		keys[i] = k
	}
	desc := bd.Help.Desc
	if d, ok := helpDesc[name]; ok {
		desc = d
	}
	return kb(keys, bd.Keys[0], desc)
}

// helpDesc is the TUI's help text for the commands whose title (what the
// palette and the GUI show) is longer than the help columns allow.
var helpDesc = map[string]string{
	"session.stop": "stop", "session.remove": "remove (rm)", "session.new": "nova sessão",
	"session.rename": "renomeia sessão (no contexto: a seção)", "session.move": "muda a sessão de contexto",
	"todo.dispatch": "dispara agent", "todo.start": "start", "todo.done": "done", "jira.toggle": "todo↔jira",
	"jira.fetch": "busca jira", "jira.open_url": "abre no browser",
}

// keys is the TUI's keymap, from the core's.
func (m *Model) keys() keyMap { return newKeyMap(m.app.Keymap()) }

// leaveInsert hands the keyboard back to nekuroddo while the embedded claude has
// focus. Esc is not used: Claude Code needs it. It is the TUI's own (the
// pane is), not a command.
var leaveInsert = kb(ks("ctrl+]"), "ctrl+]", "volta pro nekuroddo")

// shortHelp is what the status line shows, by tab and mode, most useful
// first: it is cut from the right when the terminal is narrow. Movement keys
// (j/k/h/l) are left out, and ctrl+/ is drawn apart so it always shows.
func (m *Model) shortHelp() []key.Binding {
	k := m.keys()
	if m.mode == modeInsert {
		sw := k.Switch
		sw.SetHelp("ctrl+k", "outro chat")
		return []key.Binding{leaveInsert, sw}
	}
	var b []key.Binding
	if m.active != "" {
		b = append(b, k.Insert, k.Close, k.Zoom)
	}
	switch m.tab {
	case tabTodos:
		b = append(b, k.Dispatch, k.Jira, k.Start, k.Done, k.JiraFetch, k.OpenURL)
	case tabInbox:
		b = append(b, k.Reply, k.Open, k.Archive, k.Rename, k.Full)
	case tabArchive:
		r := k.Archive
		r.SetHelp("X", "restaura (no contexto: todas)")
		b = append(b, r, k.Open, k.Filter)
	default:
		b = append(b, k.Open, k.Reply, k.Archive, k.Rename, k.NextAttn, k.New, k.Move, k.Pick, k.Filter, k.ShowAll, k.Sort)
	}
	if len(m.picked) > 0 { // picking: say what to do with them
		g := k.Palette
		g.SetHelp(":group", fmt.Sprintf("agrupa %d", len(m.picked)))
		esc := kb(ks("esc"), "esc", "limpa seleção")
		b = append([]key.Binding{g, esc}, b...)
	}
	b = append(b, k.Switch, k.Palette, k.Usage)
	// short labels for the bar; ctrl+/ shows the full descriptions
	for i := range b {
		if short, ok := barLabels[b[i].Help().Desc]; ok {
			b[i].SetHelp(b[i].Help().Key, short)
		}
	}
	return b
}

var barLabels = map[string]string{
	"abre e foca": "abre", "resposta rápida": "responde", "próxima te esperando": "próx. esperando",
	"arquiva/restaura (no contexto: todas)": "arquiva", "restaura (no contexto: todas)": "restaura",
	"nova sessão": "novo", "filtro fuzzy": "filtro", "mostra paradas/arquivadas/subagents concluídos": "ocultas",
	"ordem: atividade/custo/nome": "ordem", "ir p/ chat rodando/esperando": "chats vivos", "comandos": "comandos",
	"uso (/usage)": "uso", "INSERT no painel": "foca painel", "fecha painel": "fecha", "zoom do painel": "zoom",
	"dispara agent": "agent", "todo↔jira": "jira", "busca jira": "busca jira", "abre no browser": "browser",
	"tela cheia": "tela cheia", "renomeia sessão (no contexto: a seção)": "renomeia",
	"muda a sessão de contexto": "contexto", "seleciona (p/ :group); no contexto: alterna": "seleciona",
}

// helpColumns is the help dialog's layout, by command ("" is leaveInsert).
// The palette lists commands in this order too.
var helpColumns = [][]string{
	{"nav.down", "nav.up", "nav.top", "nav.bottom", "nav.half_down", "nav.half_up", "nav.collapse", "nav.expand",
		"nav.pick", "nav.next_attention", "nav.prev_attention"},
	{"session.open", "session.preview", "session.fullscreen", "session.insert", "", "session.reply", "session.new",
		"session.stop", "session.remove", "pane.close", "pane.zoom"},
	{"tab.sessions", "tab.inbox", "tab.todos", "tab.archive", "tab.next", "view.filter", "view.show_all", "view.sort",
		"session.archive", "session.rename", "session.move"},
	{"todo.dispatch", "todo.start", "todo.done", "jira.toggle", "jira.fetch", "jira.open_url", "switcher.open",
		"palette.open", "usage.show", "view.theme", "sessions.refresh", "help.open", "app.quit"},
}

// fullHelp is the help dialog, in columns.
func (m *Model) fullHelp() [][]key.Binding {
	km := m.app.Keymap()
	cols := make([][]key.Binding, len(helpColumns))
	for i, col := range helpColumns {
		for _, name := range col {
			b := leaveInsert
			if name != "" {
				b = binding(km, name)
			}
			cols[i] = append(cols[i], b)
		}
	}
	return cols
}

// helpRank is a command's place in the help (the palette's order); the
// commands the help leaves out go after, in the core's order.
func helpRank(name string) int {
	n := 0
	for _, col := range helpColumns {
		for _, c := range col {
			if c == name {
				return n
			}
			n++
		}
	}
	return n
}

// keyPress builds the key event for a key as the keymap writes it, so a key
// can be replayed (the mouse's click-to-open, tests) exactly as if typed.
func keyPress(s string) tea.KeyPressMsg {
	switch s {
	case "enter":
		return tea.KeyPressMsg{Code: tea.KeyEnter}
	case "tab":
		return tea.KeyPressMsg{Code: tea.KeyTab}
	case "shift+tab":
		return tea.KeyPressMsg{Code: tea.KeyTab, Mod: tea.ModShift}
	case "space":
		return tea.KeyPressMsg{Code: ' ', Text: " "}
	}
	if len(s) > 5 && s[:5] == "ctrl+" {
		return tea.KeyPressMsg{Code: rune(s[5]), Mod: tea.ModCtrl}
	}
	r := []rune(s)
	return tea.KeyPressMsg{Code: r[0], Text: s}
}
