package ui

import (
	"charm.land/bubbles/v2/key"
	tea "charm.land/bubbletea/v2"
)

// keyMap is the single source of neko's keys: matching, the status-line
// hints, the help dialog and the command palette all read from it.
type keyMap struct {
	Up, Down, Top, Bottom, HalfDown, HalfUp, PageDown, PageUp     key.Binding
	Collapse, Expand, Toggle, NextAttn, PrevAttn                  key.Binding
	TabSessions, TabInbox, TabTodos, TabArchive, NextTab, PrevTab key.Binding
	Filter, ShowAll, Sort, Archive                                key.Binding

	Open, Preview, Full, Insert, Reply, New, Stop, Remove, Close, Zoom key.Binding

	Dispatch, Start, Done, Jira, JiraFetch, OpenURL key.Binding

	Palette, Switch, Usage, Theme, Refresh, Help, Quit key.Binding
}

func kb(keys []string, k, desc string) key.Binding {
	return key.NewBinding(key.WithKeys(keys...), key.WithHelp(k, desc))
}

func ks(k ...string) []string { return k }

var keys = keyMap{
	Up:       kb(ks("k", "up"), "k", "sobe"),
	Down:     kb(ks("j", "down"), "j", "desce"),
	Top:      kb(ks("g"), "gg", "topo"),
	Bottom:   kb(ks("G"), "G", "fim"),
	HalfDown: kb(ks("ctrl+d"), "ctrl+d", "meia página ↓"),
	HalfUp:   kb(ks("ctrl+u"), "ctrl+u", "meia página ↑"),
	PageDown: kb(ks("ctrl+f", "pgdown"), "ctrl+f", "página ↓"),
	PageUp:   kb(ks("ctrl+b", "pgup"), "ctrl+b", "página ↑"),

	Collapse: kb(ks("h", "left"), "h", "fecha nó"),
	Expand:   kb(ks("l", "right"), "l", "abre nó"),
	Toggle:   kb(ks("space"), "space", "alterna nó"),
	NextAttn: kb(ks("a"), "a", "próxima te esperando"),
	PrevAttn: kb(ks("A"), "A", "anterior te esperando"),

	TabSessions: kb(ks("1"), "1", "sessões"),
	TabInbox:    kb(ks("2"), "2", "inbox"),
	TabTodos:    kb(ks("3"), "3", "todos + jira"),
	TabArchive:  kb(ks("4"), "4", "arquivo"),
	NextTab:     kb(ks("tab"), "tab", "próxima aba"),
	PrevTab:     kb(ks("shift+tab"), "shift+tab", "aba anterior"),

	Filter:  kb(ks("/"), "/", "filtro fuzzy"),
	ShowAll: kb(ks("H"), "H", "mostra paradas/arquivadas/subagents concluídos"),
	Sort:    kb(ks("S"), "S", "ordem: atividade/custo/nome"),
	Archive: kb(ks("X"), "X", "arquiva/restaura (no contexto: todas)"),

	Open:    kb(ks("enter"), "enter", "abre e foca"),
	Preview: kb(ks("o"), "o", "abre sem focar"),
	Full:    kb(ks("O"), "O", "tela cheia"),
	Insert:  kb(ks("i"), "i", "INSERT no painel"),
	Reply:   kb(ks("r"), "r", "resposta rápida"),
	New:     kb(ks("c"), "c", "nova sessão"),
	Stop:    kb(ks("x"), "x", "stop"),
	Remove:  kb(ks("d"), "dd", "remove (rm)"),
	Close:   kb(ks("w"), "w", "fecha painel"),
	Zoom:    kb(ks("z"), "z", "zoom do painel"),

	Dispatch:  kb(ks("enter"), "enter", "dispara agent"),
	Start:     kb(ks("s"), "s", "start"),
	Done:      kb(ks("D"), "D", "done"),
	Jira:      kb(ks("J"), "J", "todo↔jira"),
	JiraFetch: kb(ks("F"), "F", "busca jira"),
	OpenURL:   kb(ks("o"), "o", "abre no browser"),

	Palette: kb(ks(":", "ctrl+p"), ":", "comandos"),
	Usage:   kb(ks("u"), "u", "uso (/usage)"),
	Switch:  kb(ks("ctrl+k"), "ctrl+k", "ir p/ chat rodando/esperando"),
	Theme:   kb(ks("T"), "T", "tema"),
	Refresh: kb(ks("R"), "R", "atualiza e reordena"),
	// ctrl+/ arrives as "ctrl+/" with the kitty keyboard protocol and as
	// "ctrl+_" (0x1f) in legacy mode.
	Help: kb(ks("?", "ctrl+/", "ctrl+_"), "ctrl+/", "todos os comandos"),
	Quit: kb(ks("q", "ctrl+c"), "q", "sai"),
}

// leaveInsert hands the keyboard back to neko while the embedded claude has
// focus. Esc is not used: Claude Code needs it.
var leaveInsert = kb(ks("ctrl+]"), "ctrl+]", "volta pro neko")

// shortHelp is what the status line shows, by tab and mode, most useful
// first: it is cut from the right when the terminal is narrow. Movement keys
// (j/k/h/l) are left out, and ctrl+/ is drawn apart so it always shows.
func (m *Model) shortHelp() []key.Binding {
	k := keys
	if m.mode == modeInsert {
		sw := keys.Switch
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
		b = append(b, k.Reply, k.Open, k.Archive, k.Full)
	case tabArchive:
		r := k.Archive
		r.SetHelp("X", "restaura (no contexto: todas)")
		b = append(b, r, k.Open, k.Filter)
	default:
		b = append(b, k.Open, k.Reply, k.Archive, k.NextAttn, k.New, k.Filter, k.ShowAll, k.Sort)
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
	"tela cheia": "tela cheia",
}

// fullHelp is the help dialog, in columns.
func fullHelp() [][]key.Binding {
	k := keys
	return [][]key.Binding{
		{k.Down, k.Up, k.Top, k.Bottom, k.HalfDown, k.HalfUp, k.Collapse, k.Expand, k.Toggle, k.NextAttn, k.PrevAttn},
		{k.Open, k.Preview, k.Full, k.Insert, leaveInsert, k.Reply, k.New, k.Stop, k.Remove, k.Close, k.Zoom},
		{k.TabSessions, k.TabInbox, k.TabTodos, k.TabArchive, k.NextTab, k.Filter, k.ShowAll, k.Sort, k.Archive},
		{k.Dispatch, k.Start, k.Done, k.Jira, k.JiraFetch, k.OpenURL, k.Switch, k.Palette, k.Usage, k.Theme, k.Refresh, k.Help, k.Quit},
	}
}

// keyPress builds the key event for a binding's first key, so the palette
// can run any action exactly as if it was typed.
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
