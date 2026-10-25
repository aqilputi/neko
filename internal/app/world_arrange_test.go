package app_test

import (
	"slices"
	"testing"

	"github.com/aqilputi/nekuroddo/internal/core"
)

// S32 (wave 5): the arguments the new commands take. The layout names are
// asked for; the frame and new-screen ones are optional, never prompted
// (the pointer, the balloon and the phone pass them).
var s32Args = map[string][]core.Arg{
	"world.layout_save":   {{Name: "name", Type: core.ArgString, Required: true}},
	"world.layout_load":   {{Name: "name", Type: core.ArgString, Required: true}},
	"world.layout_delete": {{Name: "name", Type: core.ArgString, Required: true}},
	"world.layout_rename": {{Name: "name", Type: core.ArgString, Required: true},
		{Name: "to", Type: core.ArgString, Required: true}},
	"world.cluster": {{Name: "name", Type: core.ArgString}},
	"world.new_screen": {{Name: "at", Type: core.ArgString}, {Name: "app", Type: core.ArgString},
		{Name: "frame", Type: core.ArgString}},
	"world.frame": {{Name: "at", Type: core.ArgString}},
}

// s30Titles are S30's titles, verbatim (a pair "a · b" is one row: both
// commands carry it, each with its hint).
var s30Titles = map[string]string{
	"world.new_screen":     "nova tela aqui (escolhe o app)",
	"world.frame":          "moldura vazia aqui (a próxima janela vai nela)",
	"world.terminal_air":   "terminal no ar, na mira",
	"world.arrange":        "ARRUMAR a tela",
	"world.undo":           "desfaz · refaz (arrumação)",
	"world.redo":           "desfaz · refaz (arrumação)",
	"world.dock":           "ancora na vista / desancora",
	"world.menu":           "menu da tela",
	"world.select_toggle":  "marca/desmarca a tela",
	"world.select_clear":   "limpa a seleção",
	"world.cluster":        "agrupa a seleção · desfaz o grupo",
	"world.uncluster":      "agrupa a seleção · desfaz o grupo",
	"world.layout_save":    "salva o arranjo da sala",
	"world.layout_load":    "carrega · apaga um arranjo",
	"world.layout_delete":  "carrega · apaga um arranjo",
	"world.layout_rename":  "renomeia um arranjo",
	"world.nudge_left":     "move 10 cm",
	"world.nudge_down":     "move 10 cm",
	"world.nudge_up_big":   "move 50 cm",
	"world.nudge_up_fine":  "move 1 cm",
	"world.nearer_big":     "50 cm mais perto · longe",
	"world.farther_big":    "50 cm mais perto · longe",
	"world.yaw_left":       "gira 15°",
	"world.yaw_right_fine": "gira 5°",
	"world.pitch_up":       "inclina 5°",
	"world.shorter":        "altura (preset)",
	"world.taller":         "altura (preset)",
	"world.size_preset_1":  "largura 1/3 · 1/2 · 2/3 · 1",
	"world.size_preset_4":  "largura 1/3 · 1/2 · 2/3 · 1",
	"world.read_distance":  "ponto de leitura (1:1)",
	"world.scale_reset":    "escala visual",
	"world.snap_toggle":    "grade · sem ímã",
	"world.magnets_toggle": "grade · sem ímã",
	"world.to_wall":        "encosta na parede · vai para a mesa · fica no ar",
	"world.to_air":         "encosta na parede · vai para a mesa · fica no ar",
	"world.arrange_next":   "próxima · anterior tela",
	"world.arrange_done":   "confirma · cancela",
	"world.arrange_cancel": "confirma · cancela",
	"world.drag_cancel":    "cancela o arrasto",
	// the existing ones keep their first row's title
	"world.nearer":   "tela na mão: mais perto / mais longe",
	"world.narrower": "mesa mais estreita",
}

// s32NotCore are the nkw's internal names: never core commands.
var s32NotCore = []string{"world.goto_frame:1", "world.launch_air:firefox", "world.goto_frame", "world.launch_air"}

func TestWorldArrangeModes(t *testing.T) {
	for m, want := range map[string]string{
		core.ModeWorldArrange: "world.arrange", core.ModeWorldDrag: "world.drag", core.ModeWorldMenu: "world.menu",
	} {
		if m != want || !core.IsWorldMode(m) || !slices.Contains(core.WorldModes, m) {
			t.Errorf("mode %q (want %q) is not a listed world mode", m, want)
		}
		if p := core.ParentMode(m); p != "" {
			t.Errorf("ParentMode(%s) = %q, want no fallback", m, p)
		}
	}
}

// Every new key resolves in its mode (through the core's keymap), and the
// three modes have no fallback: NAV's and NORMAL's keys are not there.
func TestWorldArrangeKeysResolve(t *testing.T) {
	a, _ := cmdApp(t)
	km := a.Keymap()
	n := 0
	for _, b := range s9 {
		switch b.mode {
		case core.ModeWorldArrange, core.ModeWorldDrag, core.ModeWorldMenu:
		default:
			continue
		}
		for _, k := range b.keys {
			n++
			if got, _ := km.Resolve(b.mode, k); got != b.cmd {
				t.Errorf("%s %q = %q, want %s", b.mode, k, got, b.cmd)
			}
		}
	}
	if n != 68 {
		t.Fatalf("only %d keys in the three modes", n)
	}
	for _, mode := range []string{core.ModeWorldArrange, core.ModeWorldDrag, core.ModeWorldMenu} {
		for _, k := range []string{"o", "t", "f", "ctrl+p", "ctrl+c", ":", "z"} {
			if got, ok := km.Resolve(mode, k); ok {
				t.Errorf("%s %q falls back to %s", mode, k, got)
			}
		}
	}
	// ctrl+k is the pitch in ARRUMAR, not the switcher ("*" does not reach it)
	if got, _ := km.Resolve(core.ModeWorldArrange, "ctrl+k"); got != "world.pitch_up" {
		t.Fatalf("world.arrange ctrl+k = %q", got)
	}
}

// N, v, u, P, ctrl+n, ctrl+r, shift+f10 are bound once in normal.world, and
// the TUI's NORMAL resolves them as before.
func TestWorldArrangeNavKeysOnce(t *testing.T) {
	a, _ := cmdApp(t)
	km := a.Keymap()
	want := map[string]string{
		"N": "world.new_screen", "v": "world.arrange", "u": "world.undo", "P": "world.dock",
		"ctrl+n": "world.frame", "ctrl+r": "world.redo", "shift+f10": "world.menu", "Menu": "world.menu",
	}
	for k, cmd := range want {
		n := 0
		for _, b := range km {
			if b.Mode == core.ModeNormalWorld && slices.Contains(b.Keys, k) {
				n++
			}
		}
		if n != 1 {
			t.Errorf("%q bound %d times in normal.world", k, n)
		}
		if got, _ := km.Resolve(core.ModeNormalWorld, k); got != cmd {
			t.Errorf("normal.world %q = %q, want %s", k, got, cmd)
		}
	}
	// NORMAL (the TUI) is untouched: what it resolved before, it resolves now
	for k, before := range map[string]string{"u": "usage.show", "N": "", "v": "", "P": "", "ctrl+n": "",
		"ctrl+r": "", "shift+f10": ""} {
		if got, _ := km.Resolve(core.ModeNormal, k); got != before {
			t.Errorf("normal %q = %q, want %q", k, got, before)
		}
	}
	// ANDAR: N, ctrl+n, v; the global chord in every world mode
	for k, cmd := range map[string]string{"N": "world.new_screen", "ctrl+n": "world.frame", "v": "world.arrange"} {
		if got, _ := km.Resolve(core.ModeWorldWalk, k); got != cmd {
			t.Errorf("world.walk %q = %q, want %s", k, got, cmd)
		}
	}
	if got, _ := km.Resolve(core.ModeWorld, "world+shift+enter"); got != "world.terminal_air" {
		t.Errorf("world+shift+enter = %q", got)
	}
}

func TestWorldArrangeTitles(t *testing.T) {
	a := worldApp(t)
	for name, want := range s30Titles {
		c, ok := a.Registry().Lookup(name)
		if !ok || c.Title != want || c.Scope != core.ScopeClient {
			t.Errorf("%s = %q (%v, %q), want %q", name, c.Title, ok, c.Scope, want)
		}
	}
	for _, n := range s32NotCore {
		if _, ok := a.Registry().Lookup(n); ok {
			t.Errorf("%s is the nkw's, not a core command", n)
		}
	}
	// the optional args are never asked for (the palette asks the required)
	for name, args := range s32Args {
		c, _ := a.Registry().Lookup(name)
		for i, w := range args {
			if i < len(c.Args) && c.Args[i].Required != w.Required {
				t.Errorf("%s %s required = %v", name, w.Name, c.Args[i].Required)
			}
		}
	}
}

// One command, one binding per mode: nearer in ANDAR, ARRUMAR and the drag.
func TestWorldArrangeSharedCommands(t *testing.T) {
	a := worldApp(t)
	for name, modes := range map[string][]string{
		"world.nearer":      {core.ModeWorldWalk, core.ModeWorldArrange, core.ModeWorldDrag},
		"world.yaw_left":    {core.ModeWorldArrange, core.ModeWorldDrag},
		"world.dock":        {core.ModeNormalWorld, core.ModeWorldArrange},
		"world.undo":        {core.ModeNormalWorld, core.ModeWorldArrange},
		"world.narrower":    {core.ModeNormalWorld, core.ModeWorldWalk, core.ModeWorldArrange},
		"world.snap_toggle": {core.ModeWorldArrange, core.ModeWorldDrag},
	} {
		var got []string
		for _, b := range a.Keymap() {
			if b.Command == name {
				got = append(got, b.Mode)
			}
		}
		if !slices.Equal(got, modes) {
			t.Errorf("%s: bindings in %v, want %v", name, got, modes)
		}
	}
	// P in NAV, p in ARRUMAR (p stays world.pin in NAV)
	km := a.Keymap()
	for _, c := range []struct{ mode, key, want string }{
		{core.ModeNormalWorld, "P", "world.dock"}, {core.ModeWorldArrange, "p", "world.dock"},
		{core.ModeNormalWorld, "p", "world.pin"},
	} {
		if got, _ := km.Resolve(c.mode, c.key); got != c.want {
			t.Errorf("%s %q = %q, want %s", c.mode, c.key, got, c.want)
		}
	}
}
