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
	const pal, line, lib = core.ModeWorldPalette, core.ModeWorldLine, core.ModeWorldLibrary
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

// worldArgs are the arguments the palette asks for; world.quit confirms.
var worldArgs = map[string]core.Arg{
	"world.send_room":      {Name: "room", Type: core.ArgRoom, Prompt: "sala › ", Required: true},
	"world.goto_room":      {Name: "room", Type: core.ArgRoom, Prompt: "sala › ", Required: true},
	"world.room_new":       {Name: "name", Type: core.ArgString, Prompt: "nome da sala › ", Required: true},
	"world.goto_object":    {Name: "name", Type: core.ArgString, Prompt: "objeto › ", Required: true},
	"world.library_search": {Name: "q", Type: core.ArgString, Prompt: "busca › ", Required: true},
	"world.open":           {Name: "path", Type: core.ArgString, Prompt: "arquivo › ", Required: true},
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
					cmd.Args = []core.Arg{a}
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
