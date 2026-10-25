package app

// The world's commands (the nkw compositor, plan-mundo §3.12 and §6.9; the
// S9 table, plus S19's library mode): client-scope, so the core only
// lists them, in GET /v1/commands for the nkw's palette and in GET
// /v1/keymap in the world's modes. The TUI and the GUI never show them
// (core.IsWorldMode).
//
// A command may have keys in more than one mode (world.hint in NAV and in
// ANDAR): the registry holds the command once, with its first mode's keys;
// the other modes' bindings are worldExtraBindings, appended by App.Keymap
// while the command is registered.

import (
	"fmt"
	"strings"

	"github.com/aqilputi/nekuroddo/internal/core"
)

// worldKeys is one command of a row and its keys there.
type worldKeys struct {
	name string
	keys []string
	hint string
}

// worldRow is a row of S9: the modes it binds in, its pt-BR title and its
// commands.
type worldRow struct {
	modes []string
	title string
	cmds  []worldKeys
}

func wk(name, hint string, keys ...string) worldKeys {
	return worldKeys{name: name, keys: keys, hint: hint}
}

func row(mode, title string, cmds ...worldKeys) worldRow {
	return worldRow{modes: strings.Split(mode, ","), title: title, cmds: cmds}
}

// numbered is name_1 … name_9 bound to 1 … 9, titled "<title> N".
func numbered(mode, name, title string) []worldRow {
	var out []worldRow
	for i := 1; i <= 9; i++ {
		n := fmt.Sprint(i)
		out = append(out, row(mode, title+" "+n, wk(name+"_"+n, "", n)))
	}
	return out
}

// worldRows is S9, row by row, in its order.
func worldRows() []worldRow {
	const nav, glob, walk = core.ModeNormalWorld, core.ModeWorld, core.ModeWorldWalk
	const focus, talk, hint = core.ModeWorldFocus, core.ModeWorldTalk, core.ModeWorldHint
	const pal, line, lib, phone = core.ModeWorldPalette, core.ModeWorldLine, core.ModeWorldLibrary, core.ModeWorldPhone
	rows := []worldRow{
		row(nav, "estação anterior", wk("world.left", "", "h", "left")),
		row(nav, "próxima estação", wk("world.right", "", "l", "right")),
		row(nav, "tela de baixo", wk("world.down", "", "j", "down")),
		row(nav, "tela de cima", wk("world.up", "", "k", "up")),
		row(nav, "próxima sala", wk("world.room_next", "", "J", "world+shift+j")),
		row(nav, "sala anterior", wk("world.room_prev", "", "K", "world+shift+k")),
	}
	rows = append(rows, numbered(nav, "world.room", "sala")...)
	rows = append(rows,
		row(nav, "primeira mesa", wk("world.first", "", "gg")),
		row(nav, "última mesa", wk("world.last", "", "G")),
		row(nav, "FOCO na tela (ou no terminal do gato)", wk("world.focus", "", "enter", "i")),
		row(nav, "conversa com o gato", wk("world.talk", "", "t")),
		row(nav, "ANDAR (e volta)", wk("world.walk", "", "w", "world+w")),
		row(nav, "SALTO", wk("world.hint", "", "f", "world+f")),
		row(nav, "SALTO direto para o FOCO", wk("world.hint_focus", "", "F")),
		row(nav, "mapa", wk("world.overview", "", "o", "world+o")),
		row(nav, "biblioteca", wk("world.library", "", "b")),
		row(nav, "tela para a mesa à esquerda", wk("world.move_left", "", "H")),
		row(nav, "tela para a mesa à direita", wk("world.move_right", "", "L")),
		row(nav, "junta na pilha à esquerda", wk("world.consume", "", ",")),
		row(nav, "tira para uma mesa só dela", wk("world.expel", "", ".")),
		row(nav+","+walk, "mesa mais estreita", wk("world.narrower", "", "-")), // ANDAR: the screen in hand
		row(nav+","+walk, "mesa mais larga", wk("world.wider", "", "=")),
		row(nav, "prende a tela no ar", wk("world.pin", "", "p")),
		row(nav, "manda a tela para outra sala", wk("world.send_room", "", "M")),
		row(nav, "terminal novo", wk("world.terminal", "", "T", "world+enter")),
		row(nav, "fecha a janela", wk("world.close", "", "Q", "world+q")),
		row(nav, "chama o gato mais urgente", wk("world.summon", "", "shift+space", "world+space")),
		row(nav, "COMANDOS", wk("world.palette", "", ":", "ctrl+p", "world+p")),
		row(nav, "tela cheia (TV)", wk("world.fullscreen", "")),
		row(nav, "maximiza (largura 1)", wk("world.maximize", "")),
		row(nav, "sala nova", wk("world.room_new", "")),
		row(nav, "ir para a sala", wk("world.goto_room", "")),
		row(nav, "ir até o objeto", wk("world.goto_object", "")),
		row(nav, "abre um arquivo no mundo", wk("world.open", "")),
		row(nav, "tranca o cofre", wk("world.vault_lock", "")),
		// S26 (wave 4): the phone, the help, the HUD and the palette-only ones
		row(nav+","+walk, "telefone", wk("world.phone", "", "tab")),
		row(glob, "telefone", wk("world.phone", "", "world+n")),
		row(nav, "ajuda", wk("world.help", "", "?")),
		row(nav, "esconde/mostra o HUD", wk("world.hud", "", "z")),
		row(nav, "tour da recepcionista", wk("world.tour", "")),
		row(nav, "ajusta o ANDAR (não salva)", wk("world.walk_set", "")),
		row(nav, "abre o app com perfil isolado", wk("world.launch_isolated", "")),
		row(nav, "liga/desliga o X11 (xwayland-satellite)", wk("world.xwayland", "")),
		row(nav, "ir até o gato", wk("world.goto_cat", "")),
		row(nav, "FOCO no terminal do gato", wk("world.focus_cat", "")),
		row(glob, "FOCO na vizinha", wk("world.focus_left", "à esquerda", "world+h"),
			wk("world.focus_right", "à direita", "world+l"), wk("world.focus_up", "de cima", "world+k"),
			wk("world.focus_down", "de baixo", "world+j")),
		row(glob, "apps", wk("world.launcher", "", "world+d")),
		row(glob, "sair do mundo (confirma)", wk("world.quit", "", "world+shift+e")),
		row(glob, "tocar/pausar", wk("media.play_pause", "", "XF86AudioPlay")),
		row(glob, "faixa", wk("media.next", "próxima", "XF86AudioNext"), wk("media.prev", "anterior", "XF86AudioPrev")),
		row(glob, "volume", wk("volume.up", "aumenta", "XF86AudioRaiseVolume"),
			wk("volume.down", "diminui", "XF86AudioLowerVolume"), wk("volume.mute", "mudo", "XF86AudioMute")),
		row(glob, "brilho", wk("brightness.up", "aumenta"), wk("brightness.down", "diminui")),
		row(walk, "andar", wk("world.forward", "para a frente", "w"), wk("world.back", "para trás (no FOCO: sai)", "s"),
			wk("world.strafe_left", "de lado, à esquerda", "a"), wk("world.strafe_right", "de lado, à direita", "d")),
		row(walk, "girar", wk("world.turn_left", "à esquerda", "q", "left"), wk("world.turn_right", "à direita", "e", "right")),
		row(walk, "olhar", wk("world.look_up", "para cima", "up"), wk("world.look_down", "para baixo", "down")),
		row(walk, "horizonte", wk("world.recenter", "", "c")),
		row(walk, "usar o que está na mira", wk("world.interact", "", "enter", "space")),
		row(walk, "pegar/soltar a tela", wk("world.grab", "", "g")),
		row(walk, "tela na mão: mais perto / mais longe", wk("world.nearer", "mais perto", "["),
			wk("world.farther", "mais longe", "]")),
		row(walk, "ajustar o objeto na mira", wk("world.adjust_down", "para baixo", "j"), wk("world.adjust_up", "para cima", "k")),
		row(walk, "anterior/próximo no objeto na mira", wk("world.prev", "anterior", "h"), wk("world.next", "próximo", "l")),
		row(walk, "SALTO", wk("world.hint", "", "f")),
		row(walk, "volta ao NAV", wk("world.walk_exit", "", "esc")),
		row(focus, "sai do FOCO", wk("world.back", "", "ctrl+]")),
		row(talk, "avança", wk("world.talk_next", "", "space", "enter")),
		row(talk, "responder (linha)", wk("world.talk_reply", "", "i", "r")),
		row(talk, "FOCO no terminal do gato", wk("world.talk_sit", "", "f")),
		row(talk, "encerra", wk("world.talk_end", "", "esc", "q")),
		row(talk, "escolha", wk("world.choice_down", "a de baixo", "j"), wk("world.choice_up", "a de cima", "k")),
	)
	rows = append(rows, numbered(talk, "world.choice", "escolha")...)
	// the phone (S26): open over COMANDOS; its keys only, no fallback
	rows = append(rows,
		row(phone, "item à esquerda", wk("world.phone_left", "", "h", "left")),
		row(phone, "item à direita", wk("world.phone_right", "", "l", "right")),
		row(phone, "item de baixo", wk("world.phone_down", "", "j", "down")),
		row(phone, "item de cima", wk("world.phone_up", "", "k", "up")),
		row(phone, "abre", wk("world.phone_open", "", "enter", "space")),
		row(phone, "volta", wk("world.phone_back", "", "backspace")),
		row(phone, "fecha", wk("world.phone_close", "", "esc", "tab")),
		row(phone, "busca", wk("world.phone_search", "", "/")),
	)
	rows = append(rows, numbered(phone, "world.phone_app", "app")...)
	rows = append(rows, arrangeRows()...)
	return append(rows,
		row(hint, "cancela", wk("world.hint_cancel", "", "esc")),
		row(hint, "apaga", wk("world.hint_backspace", "", "backspace")),
		row(pal, "paleta", wk("world.palette_cancel", "cancela", "esc"), wk("world.palette_run", "executa", "enter"),
			wk("world.palette_complete", "completa", "tab")),
		row(pal, "paleta", wk("world.palette_prev", "anterior", "up", "ctrl+p"), wk("world.palette_next", "próximo", "down", "ctrl+n")),
		row(line+","+pal, "edição", wk("world.line_home", "início", "ctrl+a"), wk("world.line_end", "fim", "ctrl+e"),
			wk("world.line_delete_word", "apaga a palavra", "ctrl+w"), wk("world.line_clear", "apaga a linha", "ctrl+u")),
		row(line, "linha", wk("world.line_cancel", "cancela", "esc"), wk("world.line_send", "envia", "enter"),
			wk("world.line_newline", "quebra a linha", "shift+enter")),
		// the library (plan-mundo §7): browsing a shelf; the rest is NAV's
		row(lib, "livro anterior", wk("world.book_prev", "", "h", "left")),
		row(lib, "próximo livro", wk("world.book_next", "", "l", "right")),
		row(lib, "prateleira de baixo", wk("world.shelf_down", "", "j", "down")),
		row(lib, "prateleira de cima", wk("world.shelf_up", "", "k", "up")),
		row(lib, "abre o livro ou entra na pasta", wk("world.book_open", "", "enter")),
		row(lib, "sobe um nível", wk("world.shelf_back", "", "backspace")),
		row(lib, "busca na estante", wk("world.library_search", "", "/")),
		row(lib, "abre no editor", wk("world.book_edit", "", "e")),
		row(lib, "prévia no atril", wk("world.book_preview", "", "p")),
		row(lib, "copia o caminho", wk("world.book_copy_path", "", "y")),
		row(lib, "sai da estante", wk("world.library_leave", "", "esc")),
	)
}

// arrangeRows are S30's rows (wave 5, screens in space): frames, ARRUMAR
// and its keys, the mouse drag's keys and the balloon's. A pair "a · b" is
// one row, each command with its hint. world.nearer/farther and
// world.narrower/wider already exist: these rows only add their bindings
// in the new modes.
func arrangeRows() []worldRow {
	const nav, glob, walk = core.ModeNormalWorld, core.ModeWorld, core.ModeWorldWalk
	const arr, drag, menu = core.ModeWorldArrange, core.ModeWorldDrag, core.ModeWorldMenu
	return []worldRow{
		row(nav+","+walk, "nova tela aqui (escolhe o app)", wk("world.new_screen", "", "N")),
		row(nav+","+walk, "moldura vazia aqui (a próxima janela vai nela)", wk("world.frame", "", "ctrl+n")),
		row(glob, "terminal no ar, na mira", wk("world.terminal_air", "", "world+shift+enter")),
		row(nav+","+walk, "ARRUMAR a tela", wk("world.arrange", "", "v")),
		row(nav+","+arr, "desfaz · refaz (arrumação)", wk("world.undo", "desfaz", "u"),
			wk("world.redo", "refaz", "ctrl+r")),
		row(nav, "ancora na vista / desancora", wk("world.dock", "", "P")),
		row(arr, "ancora na vista / desancora", wk("world.dock", "", "p")),
		row(nav, "menu da tela", wk("world.menu", "", "shift+f10", "Menu")),
		row(nav, "limpa a seleção", wk("world.select_clear", "")),
		row(nav, "carrega · apaga um arranjo", wk("world.layout_load", "carrega"),
			wk("world.layout_delete", "apaga")),
		row(nav, "renomeia um arranjo", wk("world.layout_rename", "")),
		row(arr, "marca/desmarca a tela", wk("world.select_toggle", "", "space")),
		row(arr, "agrupa a seleção · desfaz o grupo", wk("world.cluster", "agrupa", "c"),
			wk("world.uncluster", "desfaz o grupo", "C")),
		row(arr, "salva o arranjo da sala", wk("world.layout_save", "", "s")),
		row(arr, "move 10 cm", wk("world.nudge_left", "à esquerda", "h", "left"),
			wk("world.nudge_right", "à direita", "l", "right"), wk("world.nudge_up", "para cima", "k", "up"),
			wk("world.nudge_down", "para baixo", "j", "down")),
		row(arr, "move 50 cm", wk("world.nudge_left_big", "à esquerda", "H"),
			wk("world.nudge_right_big", "à direita", "L"), wk("world.nudge_up_big", "para cima", "K"),
			wk("world.nudge_down_big", "para baixo", "J")),
		row(arr, "move 1 cm", wk("world.nudge_left_fine", "à esquerda", "alt+h"),
			wk("world.nudge_right_fine", "à direita", "alt+l"), wk("world.nudge_up_fine", "para cima", "alt+k"),
			wk("world.nudge_down_fine", "para baixo", "alt+j")),
		row(arr+","+drag, "mais perto · mais longe", wk("world.nearer", "mais perto", "["),
			wk("world.farther", "mais longe", "]")),
		row(arr, "50 cm mais perto · longe", wk("world.nearer_big", "mais perto", "{"),
			wk("world.farther_big", "mais longe", "}")),
		row(arr+","+drag, "gira 15°", wk("world.yaw_left", "à esquerda", "q"), wk("world.yaw_right", "à direita", "e")),
		row(arr, "gira 5°", wk("world.yaw_left_fine", "à esquerda", "alt+q"),
			wk("world.yaw_right_fine", "à direita", "alt+e")),
		row(arr, "inclina 5°", wk("world.pitch_up", "para cima", "ctrl+k"), wk("world.pitch_down", "para baixo", "ctrl+j")),
		row(arr, "largura (preset)", wk("world.narrower", "mais estreita", "-"), wk("world.wider", "mais larga", "=")),
		row(arr, "altura (preset)", wk("world.shorter", "mais baixa", "_"), wk("world.taller", "mais alta", "+")),
		row(arr, "largura 1/3 · 1/2 · 2/3 · 1", wk("world.size_preset_1", "1/3", "1"),
			wk("world.size_preset_2", "1/2", "2"), wk("world.size_preset_3", "2/3", "3"),
			wk("world.size_preset_4", "1", "4")),
		row(arr, "ponto de leitura (1:1)", wk("world.read_distance", "", "0")),
		row(arr, "escala visual", wk("world.scale_down", "menor", "ctrl+-"), wk("world.scale_up", "maior", "ctrl+="),
			wk("world.scale_reset", "original", "ctrl+0")),
		row(arr+","+drag, "grade · sem ímã", wk("world.snap_toggle", "grade", "g"),
			wk("world.magnets_toggle", "sem ímã", "x")),
		row(arr, "encosta na parede · vai para a mesa · fica no ar", wk("world.to_wall", "parede", "w"),
			wk("world.to_desk", "mesa", "d"), wk("world.to_air", "ar", "a")),
		row(arr, "próxima · anterior tela", wk("world.arrange_next", "próxima", "tab"),
			wk("world.arrange_prev", "anterior", "shift+tab")),
		row(arr, "confirma · cancela", wk("world.arrange_done", "confirma", "enter"),
			wk("world.arrange_cancel", "cancela", "esc")),
		row(drag, "cancela o arrasto", wk("world.drag_cancel", "", "esc")),
		row(menu, "item de cima", wk("world.menu_up", "", "k", "up")),
		row(menu, "item de baixo", wk("world.menu_down", "", "j", "down")),
		row(menu, "abre o item", wk("world.menu_open", "", "enter", "space")),
		row(menu, "fecha o menu", wk("world.menu_close", "", "esc")),
	}
}

// worldArgs are the arguments the palette asks for, in order (it asks the
// required ones only); world.quit confirms.
var worldArgs = map[string][]core.Arg{
	"world.send_room":      {{Name: "room", Type: core.ArgRoom, Prompt: "sala › ", Required: true}},
	"world.goto_room":      {{Name: "room", Type: core.ArgRoom, Prompt: "sala › ", Required: true}},
	"world.room_new":       {{Name: "name", Type: core.ArgString, Prompt: "nome da sala › ", Required: true}},
	"world.goto_object":    {{Name: "name", Type: core.ArgString, Prompt: "objeto › ", Required: true}},
	"world.library_search": {{Name: "q", Type: core.ArgString, Prompt: "busca › ", Required: true}},
	"world.open":           {{Name: "path", Type: core.ArgString, Prompt: "arquivo › ", Required: true}},
	// S26
	"world.walk_set": {
		{Name: "key", Type: core.ArgString, Prompt: "opção (turn, mouse_look, comfort_vignette, speed…) › ", Required: true},
		{Name: "value", Type: core.ArgString, Prompt: "valor › ", Required: true},
	},
	"world.launch_isolated": {{Name: "id", Type: core.ArgString, Prompt: "app › ", Required: true}},
	"world.xwayland":        {{Name: "on", Type: core.ArgString, Prompt: "ligar? (true/false) › ", Required: true}},
	"world.goto_cat":        {{Name: "cat", Type: core.ArgString, Prompt: "gato › ", Required: true}},
	"world.focus_cat":       {{Name: "cat", Type: core.ArgString, Prompt: "gato › ", Required: true}},
	"world.talk":            {{Name: "cat", Type: core.ArgString, Prompt: "gato › "}}, // none: the station's
	// S32 (wave 5): the layouts' names are asked; the rest is optional and
	// never prompted (the pointer, the balloon and the phone pass them)
	"world.layout_save":   {{Name: "name", Type: core.ArgString, Prompt: "nome do arranjo › ", Required: true}},
	"world.layout_load":   {{Name: "name", Type: core.ArgString, Prompt: "arranjo › ", Required: true}},
	"world.layout_delete": {{Name: "name", Type: core.ArgString, Prompt: "arranjo › ", Required: true}},
	"world.layout_rename": {
		{Name: "name", Type: core.ArgString, Prompt: "arranjo › ", Required: true},
		{Name: "to", Type: core.ArgString, Prompt: "novo nome › ", Required: true},
	},
	"world.cluster": {{Name: "name", Type: core.ArgString, Prompt: "nome do grupo › "}},
	"world.new_screen": {
		{Name: "at", Type: core.ArgString, Prompt: "posição (x_mm,y_mm,z_mm,yaw_mdeg) › "},
		{Name: "app", Type: core.ArgString, Prompt: "app › "},
		{Name: "frame", Type: core.ArgString, Prompt: "moldura › "},
	},
	"world.frame": {{Name: "at", Type: core.ArgString, Prompt: "posição (x_mm,y_mm,z_mm,yaw_mdeg) › "}},
}

// worldCommands builds the commands from worldRows (each once, in its
// first row's mode, with that row's title) and the bindings of the modes
// after the first.
func worldCommands() ([]core.Command, core.Keymap) {
	var cmds []core.Command
	var extra core.Keymap
	seen := map[string]bool{}
	for _, r := range worldRows() {
		for _, c := range r.cmds {
			for _, mode := range r.modes {
				if seen[c.name] {
					extra = append(extra, core.Binding{Command: c.name, Keys: c.keys, Mode: mode,
						Help: core.Help{Key: strings.Join(c.keys, "/"), Desc: r.title}})
					continue
				}
				seen[c.name] = true
				cmd := core.Command{Name: c.name, Title: r.title, Hint: c.hint, Keys: c.keys, Mode: mode,
					Scope: core.ScopeClient, Confirm: c.name == "world.quit"}
				if a, ok := worldArgs[c.name]; ok {
					cmd.Args = append([]core.Arg(nil), a...)
				}
				cmds = append(cmds, cmd)
			}
		}
	}
	return cmds, extra
}

// RegisterWorldCommands registers the world's client-scope commands (S9).
func (a *App) RegisterWorldCommands() error {
	cmds, _ := worldCommands()
	for _, c := range cmds {
		if err := a.reg.Register(c, nil); err != nil {
			return err
		}
	}
	return nil
}

// worldExtraBindings are the second-mode bindings of the world commands
// still registered.
func (a *App) worldExtraBindings() core.Keymap {
	_, extra := worldCommands()
	var out core.Keymap
	for _, b := range extra {
		if _, ok := a.reg.Lookup(b.Command); ok {
			out = append(out, b)
		}
	}
	return out
}
