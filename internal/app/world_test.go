package app_test

import (
	"errors"
	"maps"
	"slices"
	"strings"
	"testing"

	"github.com/aqilputi/nekuroddo/internal/app"
	"github.com/aqilputi/nekuroddo/internal/app/apptest"
	"github.com/aqilputi/nekuroddo/internal/config"
	"github.com/aqilputi/nekuroddo/internal/core"
)

// s9 is the S9 table (the world's commands, plan-mundo §3.12) written out
// again, binding by binding, so world.go cannot drift from it: mode, the
// command, its keys there.
var s9 = []struct {
	mode, cmd string
	keys      []string
}{
	{"normal.world", "world.left", []string{"h", "left"}},
	{"normal.world", "world.right", []string{"l", "right"}},
	{"normal.world", "world.down", []string{"j", "down"}},
	{"normal.world", "world.up", []string{"k", "up"}},
	{"normal.world", "world.room_next", []string{"J", "world+shift+j"}},
	{"normal.world", "world.room_prev", []string{"K", "world+shift+k"}},
	{"normal.world", "world.room_1", []string{"1"}},
	{"normal.world", "world.room_2", []string{"2"}},
	{"normal.world", "world.room_3", []string{"3"}},
	{"normal.world", "world.room_4", []string{"4"}},
	{"normal.world", "world.room_5", []string{"5"}},
	{"normal.world", "world.room_6", []string{"6"}},
	{"normal.world", "world.room_7", []string{"7"}},
	{"normal.world", "world.room_8", []string{"8"}},
	{"normal.world", "world.room_9", []string{"9"}},
	{"normal.world", "world.first", []string{"gg"}},
	{"normal.world", "world.last", []string{"G"}},
	{"normal.world", "world.focus", []string{"enter", "i"}},
	{"normal.world", "world.talk", []string{"t"}},
	{"normal.world", "world.walk", []string{"w", "world+w"}},
	{"normal.world", "world.hint", []string{"f", "world+f"}},
	{"normal.world", "world.hint_focus", []string{"F"}},
	{"normal.world", "world.overview", []string{"o", "world+o"}},
	{"normal.world", "world.library", []string{"b"}},
	{"normal.world", "world.move_left", []string{"H"}},
	{"normal.world", "world.move_right", []string{"L"}},
	{"normal.world", "world.consume", []string{","}},
	{"normal.world", "world.expel", []string{"."}},
	{"normal.world", "world.narrower", []string{"-"}},
	{"normal.world", "world.wider", []string{"="}},
	{"normal.world", "world.pin", []string{"p"}},
	{"normal.world", "world.send_room", []string{"M"}},
	{"normal.world", "world.terminal", []string{"T", "world+enter"}},
	{"normal.world", "world.close", []string{"Q", "world+q"}},
	{"normal.world", "world.summon", []string{"shift+space", "world+space"}},
	{"normal.world", "world.palette", []string{":", "ctrl+p", "world+p"}},
	{"world", "world.focus_left", []string{"world+h"}},
	{"world", "world.focus_right", []string{"world+l"}},
	{"world", "world.focus_up", []string{"world+k"}},
	{"world", "world.focus_down", []string{"world+j"}},
	{"world", "world.launcher", []string{"world+d"}},
	{"world", "world.quit", []string{"world+shift+e"}},
	{"world", "media.play_pause", []string{"XF86AudioPlay"}},
	{"world", "media.next", []string{"XF86AudioNext"}},
	{"world", "media.prev", []string{"XF86AudioPrev"}},
	{"world", "volume.up", []string{"XF86AudioRaiseVolume"}},
	{"world", "volume.down", []string{"XF86AudioLowerVolume"}},
	{"world", "volume.mute", []string{"XF86AudioMute"}},
	{"world.walk", "world.forward", []string{"w"}},
	{"world.walk", "world.back", []string{"s"}},
	{"world.walk", "world.strafe_left", []string{"a"}},
	{"world.walk", "world.strafe_right", []string{"d"}},
	{"world.walk", "world.turn_left", []string{"q", "left"}},
	{"world.walk", "world.turn_right", []string{"e", "right"}},
	{"world.walk", "world.look_up", []string{"up"}},
	{"world.walk", "world.look_down", []string{"down"}},
	{"world.walk", "world.recenter", []string{"c"}},
	{"world.walk", "world.interact", []string{"enter", "space"}},
	{"world.walk", "world.grab", []string{"g"}},
	{"world.walk", "world.nearer", []string{"["}},
	{"world.walk", "world.farther", []string{"]"}},
	{"world.walk", "world.adjust_down", []string{"j"}},
	{"world.walk", "world.adjust_up", []string{"k"}},
	{"world.walk", "world.prev", []string{"h"}},
	{"world.walk", "world.next", []string{"l"}},
	{"world.walk", "world.hint", []string{"f"}},
	{"world.walk", "world.walk_exit", []string{"esc"}},
	{"world.walk", "world.narrower", []string{"-"}},
	{"world.walk", "world.wider", []string{"="}},
	{"world.focus", "world.back", []string{"ctrl+]"}},
	{"world.talk", "world.talk_next", []string{"space", "enter"}},
	{"world.talk", "world.talk_reply", []string{"i", "r"}},
	{"world.talk", "world.talk_sit", []string{"f"}},
	{"world.talk", "world.talk_end", []string{"esc", "q"}},
	{"world.talk", "world.choice_down", []string{"j"}},
	{"world.talk", "world.choice_up", []string{"k"}},
	{"world.talk", "world.choice_1", []string{"1"}},
	{"world.talk", "world.choice_2", []string{"2"}},
	{"world.talk", "world.choice_3", []string{"3"}},
	{"world.talk", "world.choice_4", []string{"4"}},
	{"world.talk", "world.choice_5", []string{"5"}},
	{"world.talk", "world.choice_6", []string{"6"}},
	{"world.talk", "world.choice_7", []string{"7"}},
	{"world.talk", "world.choice_8", []string{"8"}},
	{"world.talk", "world.choice_9", []string{"9"}},
	{"world.hint", "world.hint_cancel", []string{"esc"}},
	{"world.hint", "world.hint_backspace", []string{"backspace"}},
	{"world.palette", "world.palette_cancel", []string{"esc"}},
	{"world.palette", "world.palette_run", []string{"enter"}},
	{"world.palette", "world.palette_complete", []string{"tab"}},
	{"world.palette", "world.palette_prev", []string{"up", "ctrl+p"}},
	{"world.palette", "world.palette_next", []string{"down", "ctrl+n"}},
	{"world.line", "world.line_home", []string{"ctrl+a"}},
	{"world.line", "world.line_end", []string{"ctrl+e"}},
	{"world.line", "world.line_delete_word", []string{"ctrl+w"}},
	{"world.line", "world.line_clear", []string{"ctrl+u"}},
	{"world.palette", "world.line_home", []string{"ctrl+a"}},
	{"world.palette", "world.line_end", []string{"ctrl+e"}},
	{"world.palette", "world.line_delete_word", []string{"ctrl+w"}},
	{"world.palette", "world.line_clear", []string{"ctrl+u"}},
	{"world.line", "world.line_cancel", []string{"esc"}},
	{"world.line", "world.line_send", []string{"enter"}},
	{"world.line", "world.line_newline", []string{"shift+enter"}},
	{"world.library", "world.book_prev", []string{"h", "left"}},
	{"world.library", "world.book_next", []string{"l", "right"}},
	{"world.library", "world.shelf_down", []string{"j", "down"}},
	{"world.library", "world.shelf_up", []string{"k", "up"}},
	{"world.library", "world.book_open", []string{"enter"}},
	{"world.library", "world.shelf_back", []string{"backspace"}},
	{"world.library", "world.library_search", []string{"/"}},
	{"world.library", "world.book_edit", []string{"e"}},
	{"world.library", "world.book_preview", []string{"p"}},
	{"world.library", "world.book_copy_path", []string{"y"}},
	{"world.library", "world.library_leave", []string{"esc"}},
	// S26 (wave 4): the phone, the help and the HUD
	{"normal.world", "world.phone", []string{"tab"}},
	{"world.walk", "world.phone", []string{"tab"}},
	{"world", "world.phone", []string{"world+n"}},
	{"normal.world", "world.help", []string{"?"}},
	{"normal.world", "world.hud", []string{"z"}},
	{"world.phone", "world.phone_left", []string{"h", "left"}},
	{"world.phone", "world.phone_right", []string{"l", "right"}},
	{"world.phone", "world.phone_down", []string{"j", "down"}},
	{"world.phone", "world.phone_up", []string{"k", "up"}},
	{"world.phone", "world.phone_open", []string{"enter", "space"}},
	{"world.phone", "world.phone_back", []string{"backspace"}},
	{"world.phone", "world.phone_close", []string{"esc", "tab"}},
	{"world.phone", "world.phone_search", []string{"/"}},
	{"world.phone", "world.phone_app_1", []string{"1"}},
	{"world.phone", "world.phone_app_2", []string{"2"}},
	{"world.phone", "world.phone_app_3", []string{"3"}},
	{"world.phone", "world.phone_app_4", []string{"4"}},
	{"world.phone", "world.phone_app_5", []string{"5"}},
	{"world.phone", "world.phone_app_6", []string{"6"}},
	{"world.phone", "world.phone_app_7", []string{"7"}},
	{"world.phone", "world.phone_app_8", []string{"8"}},
	{"world.phone", "world.phone_app_9", []string{"9"}},
}

// s26Titles are S26's titles (the ones the contract names).
var s26Titles = map[string]string{
	"world.phone":        "telefone",
	"world.help":         "ajuda",
	"world.hud":          "esconde/mostra o HUD",
	"world.phone_open":   "abre",
	"world.phone_back":   "volta",
	"world.phone_close":  "fecha",
	"world.phone_search": "busca",
	"world.phone_app_1":  "app 1",
	"world.phone_app_9":  "app 9",
	"world.tour":         "tour da recepcionista",
}

// s26Args are the arguments S26 adds (world.talk's is optional).
var s26Args = map[string][]core.Arg{
	"world.walk_set": {{Name: "key", Type: core.ArgString, Required: true},
		{Name: "value", Type: core.ArgString, Required: true}},
	"world.launch_isolated": {{Name: "id", Type: core.ArgString, Required: true}},
	"world.xwayland":        {{Name: "on", Type: core.ArgString, Required: true}},
	"world.goto_cat":        {{Name: "cat", Type: core.ArgString, Required: true}},
	"world.focus_cat":       {{Name: "cat", Type: core.ArgString, Required: true}},
	"world.talk":            {{Name: "cat", Type: core.ArgString, Required: false}},
}

// s19Titles are S19's titles: the library mode's commands, the two
// palette-only ones and the titles phases 3 and 4 changed.
var s19Titles = map[string]string{
	"world.book_prev":      "livro anterior",
	"world.book_next":      "próximo livro",
	"world.shelf_down":     "prateleira de baixo",
	"world.shelf_up":       "prateleira de cima",
	"world.book_open":      "abre o livro ou entra na pasta",
	"world.shelf_back":     "sobe um nível",
	"world.library_search": "busca na estante",
	"world.book_edit":      "abre no editor",
	"world.book_preview":   "prévia no atril",
	"world.book_copy_path": "copia o caminho",
	"world.library_leave":  "sai da estante",
	"world.open":           "abre um arquivo no mundo",
	"world.vault_lock":     "tranca o cofre",
	"world.library":        "biblioteca",
	"world.grab":           "pegar/soltar a tela",
	"world.nearer":         "tela na mão: mais perto / mais longe",
	"world.farther":        "tela na mão: mais perto / mais longe",
}

// s9PaletteOnly are S9's commands with no keys.
var s9PaletteOnly = strings.Fields(`world.fullscreen world.maximize world.room_new world.goto_room
	world.goto_object brightness.up brightness.down world.open world.vault_lock
	world.tour world.walk_set world.launch_isolated world.xwayland world.goto_cat world.focus_cat`)

// s26NotCore are the nkw's internal names (S26): never core commands.
var s26NotCore = strings.Fields(`world.tour_done phone.sub phone.app`)

// worldNames is every command name S9 has, keys or not.
func worldNames() []string {
	var out []string
	for _, b := range s9 {
		if !slices.Contains(out, b.cmd) {
			out = append(out, b.cmd)
		}
	}
	return append(out, s9PaletteOnly...)
}

// isWorldCommand is how the tests tell a world command among the builtins.
func isWorldCommand(name string) bool {
	for _, p := range []string{"world.", "media.", "volume.", "brightness."} {
		if strings.HasPrefix(name, p) {
			return true
		}
	}
	return false
}

// worldApp is a core with only the world's commands and the key commands
// (the normal ones normal.world falls back to).
func worldApp(t *testing.T) *app.App {
	t.Helper()
	a := app.New(config.Default(), apptest.New().Ports())
	t.Cleanup(a.Close)
	if err := a.RegisterKeyCommands(); err != nil {
		t.Fatal(err)
	}
	if err := a.RegisterWorldCommands(); err != nil {
		t.Fatal(err)
	}
	return a
}

func TestWorldCommandsAreS9(t *testing.T) {
	a, _ := cmdApp(t) // through RegisterBuiltins
	var got []string
	for _, c := range a.Registry().All() {
		if !isWorldCommand(c.Name) {
			continue
		}
		got = append(got, c.Name)
		if c.Scope != core.ScopeClient || strings.TrimSpace(c.Title) == "" || c.Source != core.SourceBuiltin {
			t.Errorf("%s: scope %q title %q source %q", c.Name, c.Scope, c.Title, c.Source)
		}
		if !core.IsWorldMode(c.Mode) {
			t.Errorf("%s: mode %q is not the world's", c.Name, c.Mode)
		}
		if _, err := a.Registry().Run(app.Context{}, c.Name, nil); !errors.Is(err, app.ErrClientCommand) {
			t.Errorf("%s runs in the core: %v", c.Name, err)
		}
	}
	want := worldNames()
	slices.Sort(got)
	slices.Sort(want)
	if !slices.Equal(got, want) {
		t.Fatalf("world commands:\n got %v\nwant %v", got, want)
	}
	for _, n := range s9PaletteOnly {
		if c, _ := a.Registry().Lookup(n); len(c.Keys) != 0 {
			t.Errorf("%s is palette-only, has %v", n, c.Keys)
		}
	}
}

// Every S9 binding is in the keymap, and the keymap has no other world
// binding.
func TestWorldKeymapIsS9(t *testing.T) {
	a := worldApp(t)
	type slot struct{ mode, key string }
	want := map[slot]string{}
	for _, b := range s9 {
		for _, k := range b.keys {
			want[slot{b.mode, k}] = b.cmd
		}
	}
	got := map[slot]string{}
	for _, b := range a.Keymap() {
		if !core.IsWorldMode(b.Mode) {
			continue
		}
		if b.Help.Desc == "" || b.Help.Key != strings.Join(b.Keys, "/") {
			t.Errorf("%s [%s] help = %+v", b.Command, b.Mode, b.Help)
		}
		for _, k := range b.Keys {
			got[slot{b.Mode, k}] = b.Command
		}
	}
	for s, cmd := range want {
		if got[s] != cmd {
			t.Errorf("%s %q = %q, want %s", s.mode, s.key, got[s], cmd)
		}
	}
	for s, cmd := range got {
		if _, ok := want[s]; !ok {
			t.Errorf("%s %q = %s is not in S9", s.mode, s.key, cmd)
		}
	}
}

// A command with keys in two modes is one command (GET /v1/commands lists
// it once) with one binding per mode (GET /v1/keymap).
func TestWorldCommandInTwoModes(t *testing.T) {
	a := worldApp(t)
	for name, modes := range map[string][]string{
		"world.hint":      {core.ModeNormalWorld, core.ModeWorldWalk},
		"world.back":      {core.ModeWorldWalk, core.ModeWorldFocus},
		"world.line_home": {core.ModeWorldLine, core.ModeWorldPalette},
		"world.phone":     {core.ModeNormalWorld, core.ModeWorldWalk, core.ModeWorld},
	} {
		n := 0
		for _, c := range a.Registry().All() {
			if c.Name == name {
				n++
			}
		}
		var got []string
		for _, b := range a.Keymap() {
			if b.Command == name {
				got = append(got, b.Mode)
			}
		}
		if n != 1 || !slices.Equal(got, modes) {
			t.Errorf("%s: %d commands, bindings in %v, want 1 in %v", name, n, got, modes)
		}
	}
}

func TestWorldKeymapRules(t *testing.T) {
	a, _ := cmdApp(t)
	km := a.Keymap()
	if c := km.Collisions(); len(c) != 0 {
		t.Fatalf("keys bound twice in a mode: %v", c)
	}
	chords := map[string]string{}
	for _, b := range km {
		for _, k := range b.Keys {
			if strings.Contains(k, "world+") {
				// a chord is global: one command in every world mode
				if prev, dup := chords[k]; dup {
					t.Errorf("chord %s bound to %s and %s", k, prev, b.Command)
				}
				chords[k] = b.Command
				if !core.IsWorldMode(b.Mode) {
					t.Errorf("chord %s in mode %s", k, b.Mode)
				}
			}
			// the world layer holds chords and the media keys only (both
			// global: nobody types them into a window)
			if b.Mode == core.ModeWorld && !strings.Contains(k, "world+") && !strings.HasPrefix(k, "XF86") {
				t.Errorf("%s: key %q in mode world has no world+", b.Command, k)
			}
		}
	}
	if len(chords) != 16 {
		t.Errorf("%d chords: %v", len(chords), chords)
	}
	// world.quit asks first; the arguments the palette asks for
	if c, _ := a.Registry().Lookup("world.quit"); !c.Confirm {
		t.Error("world.quit does not confirm")
	}
	args := map[string][]core.Arg{
		"world.send_room":      {{Name: "room", Type: core.ArgRoom, Required: true}},
		"world.goto_room":      {{Name: "room", Type: core.ArgRoom, Required: true}},
		"world.room_new":       {{Name: "name", Type: core.ArgString, Required: true}},
		"world.goto_object":    {{Name: "name", Type: core.ArgString, Required: true}},
		"world.library_search": {{Name: "q", Type: core.ArgString, Required: true}},
		"world.open":           {{Name: "path", Type: core.ArgString, Required: true}},
	}
	maps.Copy(args, s26Args)
	for name, want := range args {
		c, _ := a.Registry().Lookup(name)
		if len(c.Args) != len(want) {
			t.Errorf("%s args = %+v", name, c.Args)
			continue
		}
		for i, w := range want {
			g := c.Args[i]
			if g.Name != w.Name || g.Type != w.Type || g.Required != w.Required || g.Prompt == "" {
				t.Errorf("%s arg %d = %+v, want %+v", name, i, g, w)
			}
		}
	}
	// no other world command takes arguments or confirms
	for _, c := range a.Registry().All() {
		if _, ok := args[c.Name]; ok || c.Name == "world.quit" {
			continue
		}
		if isWorldCommand(c.Name) && (len(c.Args) != 0 || c.Confirm) {
			t.Errorf("%s: args %v confirm %v", c.Name, c.Args, c.Confirm)
		}
	}
	// the world never binds app.quit
	for _, b := range km {
		if b.Command == "app.quit" && core.IsWorldMode(b.Mode) {
			t.Errorf("app.quit bound in %s", b.Mode)
		}
	}
}

// S19's titles, and no title still says which phase it waits for.
func TestWorldTitlesS19(t *testing.T) {
	a := worldApp(t)
	titles := maps.Clone(s19Titles)
	maps.Copy(titles, s26Titles)
	for name, want := range titles {
		c, ok := a.Registry().Lookup(name)
		if !ok || c.Title != want {
			t.Errorf("%s title = %q, want %q", name, c.Title, want)
		}
	}
	for _, c := range a.Registry().All() {
		if isWorldCommand(c.Name) && strings.Contains(c.Title, "fase") {
			t.Errorf("%s title %q names a phase", c.Name, c.Title)
		}
	}
}

// world.library is a sub-mode of NAV: the shelf keys shadow NAV's, the
// rest is NAV's, then NORMAL's (the station's cat).
func TestWorldLibraryFallsBackToNav(t *testing.T) {
	a, _ := cmdApp(t)
	km := a.Keymap()
	if p := core.ParentMode(core.ModeWorldLibrary); p != core.ModeNormalWorld {
		t.Fatalf("ParentMode(world.library) = %q", p)
	}
	for key, want := range map[string]string{
		"h": "world.book_prev", "enter": "world.book_open", "p": "world.book_preview", "esc": "world.library_leave",
		"t": "world.talk", "o": "world.overview", "r": "session.reply", "ctrl+k": "switcher.open",
	} {
		if got, _ := km.Resolve(core.ModeWorldLibrary, key); got != want {
			t.Errorf("world.library %q = %q, want %s", key, got, want)
		}
	}
	if got, _ := km.Resolve(core.ModeNormalWorld, "p"); got != "world.pin" {
		t.Fatalf("NAV p = %q", got)
	}
}

// normal.world is a sub-mode of normal: its keys shadow normal's, the rest
// (the station's cat: r, X, dd, …) falls through.
func TestWorldNormalFallsBackToNormal(t *testing.T) {
	a, _ := cmdApp(t)
	km := a.Keymap()
	if p := core.ParentMode(core.ModeNormalWorld); p != core.ModeNormal {
		t.Fatalf("ParentMode(normal.world) = %q", p)
	}
	for key, want := range map[string]string{
		"h": "world.left", "i": "world.focus", "gg": "world.first", "shift+space": "world.summon",
		"r": "session.reply", "X": "session.archive", "dd": "session.remove", "ctrl+k": "switcher.open",
	} {
		if got, _ := km.Resolve(core.ModeNormalWorld, key); got != want {
			t.Errorf("normal.world %q = %q, want %s", key, got, want)
		}
	}
	// NORMAL itself is untouched by the world
	if got, _ := km.Resolve(core.ModeNormal, "h"); got != "nav.collapse" {
		t.Fatalf("normal h = %q", got)
	}
	if !km.StartsSequence(core.ModeNormalWorld, "g") {
		t.Fatal("g does not start gg in normal.world")
	}
}

// Registry.SetKeys rebinds a world command like any other, in its mode or
// into another world mode; the binding a second mode carries stays.
func TestWorldSetKeys(t *testing.T) {
	a := worldApp(t)
	if err := a.Registry().SetKeys("world.right", core.ModeNormalWorld, []string{"n"}); err != nil {
		t.Fatal(err)
	}
	km := a.Keymap()
	if got, _ := km.Resolve(core.ModeNormalWorld, "n"); got != "world.right" {
		t.Fatalf("normal.world n = %q", got)
	}
	if got, ok := km.Resolve(core.ModeNormalWorld, "l"); ok && got == "world.right" {
		t.Fatal("l still bound")
	}
	if err := a.Registry().SetKeys("world.hint", core.ModeWorldTalk, []string{"s"}); err != nil {
		t.Fatal(err)
	}
	km = a.Keymap()
	if got, _ := km.Resolve(core.ModeWorldTalk, "s"); got != "world.hint" {
		t.Fatalf("world.talk s = %q", got)
	}
	if got, _ := km.Resolve(core.ModeWorldWalk, "f"); got != "world.hint" {
		t.Fatalf("world.walk f = %q", got)
	}
}

// Without RegisterWorldCommands the keymap has no world binding at all,
// and registering twice fails like every builtin.
func TestWorldCommandsRegisterOnce(t *testing.T) {
	a := app.New(config.Default(), apptest.New().Ports())
	t.Cleanup(a.Close)
	if err := a.RegisterKeyCommands(); err != nil {
		t.Fatal(err)
	}
	for _, b := range a.Keymap() {
		if core.IsWorldMode(b.Mode) {
			t.Fatalf("world binding without the world: %+v", b)
		}
	}
	if err := a.RegisterWorldCommands(); err != nil {
		t.Fatal(err)
	}
	if err := a.RegisterWorldCommands(); !errors.Is(err, app.ErrDuplicateCommand) {
		t.Fatalf("second register = %v", err)
	}
	// an extra binding goes with its command
	a.Registry().Unregister("world.hint")
	for _, b := range a.Keymap() {
		if b.Command == "world.hint" {
			t.Fatalf("binding of an unregistered command: %+v", b)
		}
	}
}

// S26: every key of the phone resolves in world.phone, and only those (the
// mode has no fallback; the nkw chains "world" before it).
func TestWorldPhoneKeys(t *testing.T) {
	a, _ := cmdApp(t)
	km := a.Keymap()
	n := 0
	for _, b := range s9 {
		if b.mode != core.ModeWorldPhone {
			continue
		}
		for _, k := range b.keys {
			n++
			if got, _ := km.Resolve(core.ModeWorldPhone, k); got != b.cmd {
				t.Errorf("world.phone %q = %q, want %s", k, got, b.cmd)
			}
		}
	}
	if n != 23 {
		t.Fatalf("%d phone keys", n)
	}
	for _, k := range []string{"x", "q", "t", "world+n", "ctrl+k", ":"} {
		if got, ok := km.Resolve(core.ModeWorldPhone, k); ok {
			t.Errorf("world.phone %q falls back to %s", k, got)
		}
	}
	for _, name := range s26NotCore {
		if _, ok := a.Registry().Lookup(name); ok {
			t.Errorf("%s is the nkw's, not a core command", name)
		}
	}
}

// tab opens the phone in NAV and ANDAR (one binding each), world+n
// anywhere; ? and z shadow help.open and pane.zoom inside the world only:
// the TUI resolves them as before.
func TestWorldPhoneHelpHudKeys(t *testing.T) {
	a, _ := cmdApp(t)
	km := a.Keymap()
	for _, c := range []struct{ mode, key, want string }{
		{core.ModeNormalWorld, "tab", "world.phone"},
		{core.ModeWorldWalk, "tab", "world.phone"},
		{core.ModeWorld, "world+n", "world.phone"},
		{core.ModeNormalWorld, "?", "world.help"},
		{core.ModeNormalWorld, "z", "world.hud"},
		{core.ModeWorldLibrary, "?", "world.help"}, // the shelf falls back to NAV
		{core.ModeNormal, "tab", "tab.next"},
		{core.ModeNormal, "?", "help.open"},
		{core.ModeNormal, "z", "pane.zoom"},
		{core.ModeNormalTodos, "?", "help.open"},
	} {
		if got, _ := km.Resolve(c.mode, c.key); got != c.want {
			t.Errorf("%s %q = %q, want %s", c.mode, c.key, got, c.want)
		}
	}
	for _, mode := range []string{core.ModeNormalWorld, core.ModeWorldWalk} {
		n := 0
		for _, b := range km {
			if b.Mode == mode && slices.Contains(b.Keys, "tab") {
				n++
			}
		}
		if n != 1 {
			t.Errorf("tab bound %d times in %s", n, mode)
		}
	}
}
