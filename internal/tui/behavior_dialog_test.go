package tui

import (
	"os"
	"strings"
	"testing"
	"time"

	tea "charm.land/bubbletea/v2"

	"github.com/aqilputi/nekuroddo/internal/groups"
	"github.com/aqilputi/nekuroddo/internal/store"
	"github.com/aqilputi/nekuroddo/internal/theme"
	"github.com/aqilputi/nekuroddo/internal/todo"
)

func TestBehaviorConfirmDialogKeys(t *testing.T) {
	for _, c := range []struct {
		k   tea.KeyPressMsg
		yes bool
	}{
		{bhRune('y'), true},
		{bhRune('Y'), true},
		{tea.KeyPressMsg{Code: tea.KeyEnter}, true},
		{bhRune('n'), false},
		{tea.KeyPressMsg{Code: tea.KeyEscape}, false},
		{bhRune('q'), false},
	} {
		m := bhModel(t)
		ran := false
		m.confirmThen("tem certeza?", func(*Model) tea.Cmd { ran = true; return nil })
		if d, ok := m.top().(*confirmDialog); !ok || d.title() != "confirma?" {
			t.Fatal("confirmThen pushes a confirm dialog")
		}
		m.key(c.k)
		if ran != c.yes {
			t.Errorf("%s: ran=%v want %v", c.k.String(), ran, c.yes)
		}
		if m.top() != nil {
			t.Errorf("%s: any key closes the dialog", c.k.String())
		}
	}
}

// A dialog's yes may open another dialog: it must stay.
func TestBehaviorConfirmChainKeepsNewDialog(t *testing.T) {
	m := bhModel(t)
	m.confirmThen("primeira?", func(m *Model) tea.Cmd {
		m.confirmThen("segunda?", func(*Model) tea.Cmd { return nil })
		return nil
	})
	m.key(bhRune('y'))
	d, ok := m.top().(*confirmDialog)
	if !ok || d.q != "segunda?" || len(m.dialogs) != 2 {
		t.Fatalf("the second question must stay open: %d dialogs", len(m.dialogs))
	}
}

func TestBehaviorQuit(t *testing.T) {
	m := bhModel(t, bhSessions()...)
	m.keyNormal(bhRune('q'))
	d, ok := m.top().(*confirmDialog)
	if !ok || !strings.Contains(d.q, "2 sessões seguem rodando") {
		t.Fatalf("q asks, naming what keeps running: %+v", m.top())
	}
	bhPane(t, &m, "p1", "a")
	cmd := m.key(bhRune('y'))
	if msgs := bhRun(cmd); len(msgs) != 1 {
		t.Fatalf("y quits: %v", msgs)
	} else if _, ok := msgs[0].(tea.QuitMsg); !ok {
		t.Fatalf("y quits: %T", msgs[0])
	}
	if len(m.panes) != 0 {
		t.Fatal("quitting detaches every pane")
	}
	idle := bhModel(t)
	idle.keyNormal(bhRune('q'))
	if d := idle.top().(*confirmDialog); d.q != "sair do nekuroddo?" {
		t.Fatalf("plain question when nothing runs: %q", d.q)
	}
}

func TestBehaviorDoubleCtrlCQuits(t *testing.T) {
	m := bhModel(t, bhSessions()...)
	bhPane(t, &m, "p1", "a")
	ctrlC := tea.KeyPressMsg{Code: 'c', Mod: tea.ModCtrl}
	_, cmd := m.keyNormal(ctrlC)
	if cmd != nil || m.top() == nil {
		t.Fatal("first ctrl+c asks")
	}
	m.pop()
	_, cmd = m.keyNormal(ctrlC)
	if cmd == nil || len(m.panes) != 0 {
		t.Fatal("second ctrl+c within a second quits")
	}
	if _, ok := cmd().(tea.QuitMsg); !ok {
		t.Fatal("quit")
	}
	// slow second ctrl+c asks again
	m.lastCtrlC = time.Now().Add(-2 * time.Second)
	if _, cmd := m.keyNormal(ctrlC); cmd != nil {
		t.Fatal("a late ctrl+c only asks")
	}
}

func TestBehaviorHelpAndUsageDialogs(t *testing.T) {
	m := bhModel(t)
	m.keyNormal(bhRune('?'))
	if _, ok := m.top().(helpDialog); !ok {
		t.Fatal("? opens help")
	}
	m.key(bhRune('x'))
	if m.top() != nil {
		t.Fatal("any key closes help")
	}
	_, cmd := m.keyNormal(bhRune('u'))
	if _, ok := m.top().(usageDialog); !ok {
		t.Fatal("u opens usage")
	}
	msgs := bhRun(cmd)
	if len(msgs) != 1 {
		t.Fatalf("u fetches /usage even when polling is off: %v", msgs)
	}
	um, ok := msgs[0].(usageMsg)
	if !ok || !um.manual || um.err == nil {
		t.Fatalf("manual fetch with a failing claude: %+v", msgs[0])
	}
	if next := m.update(um); next != nil {
		t.Fatal("a manual fetch schedules nothing")
	}
	if _, err := m.app.Usage(); err == nil {
		t.Fatal("the error is kept for the dialog")
	}
	m.key(bhRune('x'))
	if m.top() != nil {
		t.Fatal("any key closes usage")
	}
}

func TestBehaviorPaletteNavigateAndRunAction(t *testing.T) {
	m := bhModel(t, bhSessions()...)
	m.keyNormal(bhRune(':'))
	d, ok := m.top().(*paletteDialog)
	if !ok || d.title() != "comandos" || len(d.matches) != len(d.items) {
		t.Fatal(": opens the palette listing everything")
	}
	m.key(tea.KeyPressMsg{Code: tea.KeyDown})
	m.key(tea.KeyPressMsg{Code: 'n', Mod: tea.ModCtrl})
	m.key(tea.KeyPressMsg{Code: 'j', Mod: tea.ModCtrl})
	if d.cur != 3 {
		t.Fatalf("down/ctrl+n/ctrl+j move: %d", d.cur)
	}
	m.key(tea.KeyPressMsg{Code: tea.KeyUp})
	m.key(tea.KeyPressMsg{Code: 'p', Mod: tea.ModCtrl})
	m.key(tea.KeyPressMsg{Code: 'k', Mod: tea.ModCtrl})
	if d.cur != 0 {
		t.Fatalf("up/ctrl+p/ctrl+k move back: %d", d.cur)
	}
	m.key(tea.KeyPressMsg{Code: tea.KeyUp})
	if d.cur != 0 {
		t.Fatal("clamped at the top")
	}
	// the first action is "desce": running it moves the cursor like j
	if d.items[d.matches[0].Index].title != "desce" {
		t.Fatalf("first item: %q", d.items[d.matches[0].Index].title)
	}
	m.view.Cursor = 0
	bhEnter(&m)
	if m.top() != nil || m.view.Cursor != 1 {
		t.Fatalf("enter runs the action and closes: cursor %d", m.view.Cursor)
	}
	// esc and ctrl+c close it
	for _, k := range []tea.KeyPressMsg{{Code: tea.KeyEscape}, {Code: 'c', Mod: tea.ModCtrl}} {
		m.keyNormal(bhRune(':'))
		m.key(k)
		if m.top() != nil {
			t.Fatalf("%s closes the palette", k.String())
		}
	}
	// nothing matches: enter just closes
	m.keyNormal(bhRune(':'))
	bhType(&m, "qqqqqqqqzzzzzz")
	if d := m.top().(*paletteDialog); len(d.matches) != 0 {
		t.Fatal("nothing matches")
	}
	bhEnter(&m)
	if m.top() != nil {
		t.Fatal("enter with no match closes")
	}
}

func TestBehaviorPaletteItems(t *testing.T) {
	m := bhModel(t, bhSessions()...)
	bhSetTodos(t, &m, []todo.Group{{Group: "g", Tasks: []todo.Task{{ID: 5, Title: "tarefa"}}}})
	m.rebuildTodos()
	items := m.paletteItems()
	find := func(prefix string) *paletteItem {
		for i := range items {
			if strings.HasPrefix(items[i].title, prefix) {
				return &items[i]
			}
		}
		return nil
	}
	for _, unwanted := range []string{"topo", "remove (rm)"} {
		if find(unwanted) != nil {
			t.Errorf("two-key actions (gg, dd) are left out: %q", unwanted)
		}
	}
	if find("ir: delta") != nil {
		t.Fatal("stale sessions are not offered without H")
	}
	// jump to a session
	it := find("ir: charlie")
	if it == nil {
		t.Fatal("sessions are offered")
	}
	m.tab = tabInbox
	it.run(&m)
	if r := m.cur(); m.tab != tabSessions || r.Sess == nil || r.Sess.SessionID != "c" {
		t.Fatal("ir: selects the session")
	}
	// jump to a task
	it = find("todo #5 tarefa")
	if it == nil {
		t.Fatal("tasks are offered")
	}
	it.run(&m)
	if m.tab != tabTodos || m.todoCur != 0 {
		t.Fatal("a task item opens the todos tab on it")
	}
	// themes
	it = find("tema: " + theme.All[len(theme.All)-1].Name)
	if it == nil {
		t.Fatal("themes are offered")
	}
	it.run(&m)
	if m.th != len(theme.All)-1 || !m.themeSet {
		t.Fatal("theme item sets the theme")
	}
	// group commands reopen the palette with the command typed
	for _, c := range []struct{ prefix, typed string }{
		{"group <tag>", "group "}, {"ungroup [tag]", "ungroup "}, {"notify #tag", "notify #"},
	} {
		m.dialogs = nil
		it = find(c.prefix)
		if it == nil {
			t.Fatalf("%s offered", c.prefix)
		}
		it.run(&m)
		d, ok := m.top().(*paletteDialog)
		if !ok || d.input.Value() != c.typed {
			t.Fatalf("%s reopens with %q", c.prefix, c.typed)
		}
	}
	// with picks the hint counts them
	m.picked = map[string]bool{"a": true, "b": true}
	found := false
	for _, it := range m.paletteItems() {
		if strings.Contains(it.title, "as 2 selecionadas") {
			found = true
		}
	}
	if !found {
		t.Fatal("picks are named in the group items")
	}
}

func TestBehaviorPaletteUndoGroup(t *testing.T) {
	m := bhModel(t, bhSessions()...)
	bhSaveGroup(t, &m, "deploy", groups.Member{SessionID: "a"}, groups.Member{SessionID: "c"})
	var undo *paletteItem
	for _, it := range m.paletteItems() {
		if strings.HasPrefix(it.title, "desfazer grupo #deploy (2 sessões)") {
			it := it
			undo = &it
		}
	}
	if undo == nil {
		t.Fatal("each group can be undone from the palette")
	}
	if undo.run(&m) != nil || m.top() == nil {
		t.Fatal("undoing asks first")
	}
	cmd := m.key(bhRune('y'))
	if _, ok := bhHasToast(cmd, "#deploy desfeito"); !ok {
		t.Fatal("undone")
	}
	if len(groups.Load().Groups["deploy"]) != 0 {
		t.Fatal("the group is gone from disk")
	}
}

func TestBehaviorPaletteGroupCommands(t *testing.T) {
	m := bhModel(t, bhSessions()...)
	if m.groupCommand("   ") != nil || m.groupCommand("desce") != nil {
		t.Fatal("non-commands fall back to fuzzy search")
	}
	if it := m.groupCommand("g"); it == nil || it.run != nil || !strings.Contains(it.hint, "a sessão do cursor") {
		t.Fatal("bare group waits for the tag")
	}
	if it := m.groupCommand("ug foo"); it == nil || !strings.Contains(it.title, "#foo") {
		t.Fatal("ungroup with a tag names it")
	}
	if it := m.groupCommand("notify"); it == nil || it.run != nil {
		t.Fatal("notify without a group waits")
	}
	// notify without a tag uses the cursor's group
	bhSaveGroup(t, &m, "t1", groups.Member{SessionID: "a"})
	bhSelect(t, &m, "a")
	it := m.groupCommand("n oi pessoal")
	if it == nil || it.run == nil || !strings.Contains(it.title, "#t1: oi pessoal") {
		t.Fatalf("notify defaults to the cursor's group: %+v", it)
	}
	// group with nothing selected
	m.view.Cursor = 0
	if tm, ok := bhHasToast(m.groupTargets("x"), "group: escolha"); !ok || !tm.err {
		t.Fatal("group needs a target")
	}
	if _, ok := bhHasToast(m.ungroupTargets(""), "nada a tirar"); !ok {
		t.Fatal("ungroup with nothing to remove says so")
	}
	if tm, ok := bhHasToast(m.notifyGroup("nope", "x"), "não existe"); !ok || !tm.err {
		t.Fatal("notify to an unknown group")
	}
}

func TestBehaviorPalettePaste(t *testing.T) {
	m := bhModel(t, bhSessions()...)
	m.keyNormal(bhRune(':'))
	m.update(tea.PasteMsg{Content: "charlie"})
	d := m.top().(*paletteDialog)
	if d.input.Value() != "charlie" || len(d.matches) == 0 || len(d.matches) == len(d.base) {
		t.Fatalf("paste filters the palette: %q %d", d.input.Value(), len(d.matches))
	}
}

func TestBehaviorPaletteBulkActions(t *testing.T) {
	m := bhModel(t, bhSessions()...)
	if _, ok := bhHasToast(m.archiveStale(), "nenhuma sessão parada"); ok {
		t.Fatal("d is stale: there is something to archive")
	}
	d, ok := m.top().(*confirmDialog)
	if !ok || !strings.Contains(d.q, "arquivar as 1 sessões paradas há mais de 3d") {
		t.Fatalf("archiveStale asks: %+v", m.top())
	}
	if _, ok := bhHasToast(m.key(bhRune('y')), "1 sessões arquivadas"); !ok {
		t.Fatal("archived")
	}
	fresh := bhModel(t, bhSessions()[:3]...)
	if _, ok := bhHasToast(fresh.archiveStale(), "nenhuma sessão parada"); !ok {
		t.Fatal("nothing stale: says so")
	}
	if _, ok := bhHasToast(fresh.removeEnded(), "nenhum chat encerrado"); !ok {
		t.Fatal("nothing ended: says so")
	}
}

func TestBehaviorRemoveEnded(t *testing.T) {
	ss := bhSessions()
	ss[0].Ended = true
	m := bhModel(t, ss...)
	bhPane(t, &m, "aaaa1111", "a")
	if m.removeEnded() != nil {
		t.Fatal("removing asks first")
	}
	if d := m.top().(*confirmDialog); !strings.Contains(d.q, "remover 1 chats") {
		t.Fatalf("question: %q", d.q)
	}
	cmd := m.key(bhRune('y'))
	if m.panes["aaaa1111"] != nil {
		t.Fatal("their panes close")
	}
	if tm, ok := bhHasToast(cmd, "falhou: aaaa1111"); !ok || !tm.err {
		t.Fatal("a failing claude rm is reported")
	}
	// with a working claude it just refreshes
	m2 := bhModel(t, ss...)
	fakeClaude(t)
	m2.removeEnded()
	msgs := bhRun(m2.key(bhRune('y')))
	if len(msgs) != 1 {
		t.Fatalf("rm: %v", msgs)
	}
	if _, ok := msgs[0].(tickMsg); !ok {
		t.Fatalf("rm then refresh: %T", msgs[0])
	}
}

func TestBehaviorStopAndRemoveSelected(t *testing.T) {
	m := bhModel(t, bhSessions()...)
	bhSelect(t, &m, "a")
	m.keyNormal(bhRune('x'))
	d, ok := m.top().(*confirmDialog)
	if !ok || !strings.Contains(d.q, "parar \"alpha\"") {
		t.Fatal("x asks before stopping")
	}
	if tm, ok := bhHasToast(m.key(bhRune('y')), "claude stop aaaa1111"); !ok || !tm.err {
		t.Fatal("a failing stop is reported")
	}
	// dd removes (asks first, closes the pane)
	bhPane(t, &m, "aaaa1111", "a")
	m.keyNormal(bhRune('d'))
	if !m.pendingD {
		t.Fatal("d waits for the second d")
	}
	m.keyNormal(bhRune('d'))
	if d, ok := m.top().(*confirmDialog); !ok || !strings.Contains(d.q, "remover \"alpha\"") {
		t.Fatal("dd asks")
	}
	if tm, ok := bhHasToast(m.key(bhRune('y')), "claude rm aaaa1111"); !ok || !tm.err {
		t.Fatal("a failing rm is reported")
	}
	if m.panes["aaaa1111"] != nil {
		t.Fatal("removing closes the pane")
	}
	// d then another key is just that key
	m.keyNormal(bhRune('d'))
	m.keyNormal(bhRune('j'))
	if m.pendingD || m.top() != nil {
		t.Fatal("d followed by j does not remove")
	}
	// x / dd on an interactive session
	bhSelect(t, &m, "b")
	m.keyNormal(bhRune('x'))
	if m.top() != nil {
		t.Fatal("x needs a background session")
	}
	m.keyNormal(bhRune('d'))
	_, cmd := m.keyNormal(bhRune('d'))
	if tm, ok := bhHasToast(cmd, "só sessões em background"); !ok || !tm.err {
		t.Fatal("dd on an interactive session refuses")
	}
	// a working stop refreshes
	fakeClaude(t)
	bhSelect(t, &m, "a")
	m.keyNormal(bhRune('x'))
	if msgs := bhRun(m.key(bhRune('y'))); len(msgs) != 1 {
		t.Fatalf("stop: %v", msgs)
	} else if _, ok := msgs[0].(tickMsg); !ok {
		t.Fatalf("stop then refresh: %T", msgs[0])
	}
}

func TestBehaviorSwitcherRunsItem(t *testing.T) {
	m := bhModel(t, bhSessions()...)
	t.Cleanup(func() {
		for id := range m.panes {
			m.closePane(id)
		}
	})
	m.openSwitcher()
	d := m.top().(*paletteDialog)
	if len(d.items) != 3 || !strings.Contains(d.items[2].title, "bravo") {
		t.Fatalf("waiting a and c first, then busy b; stale d left out: %d", len(d.items))
	}
	// pick the interactive one: no pane, a toast
	bhType(&m, "bravo")
	cmd := bhEnter(&m)
	if _, ok := bhHasToast(cmd, "pid 42"); !ok {
		t.Fatal("interactive chat: points to its terminal")
	}
	if r := m.cur(); r.Sess.SessionID != "b" {
		t.Fatal("selects it in the tree")
	}
	// the background one attaches focused
	m.openSwitcher()
	bhType(&m, "alpha")
	bhEnter(&m)
	if m.panes["aaaa1111"] == nil || m.mode != modeInsert {
		t.Fatal("background chat attaches focused")
	}
}

func TestBehaviorToggleArchiveSession(t *testing.T) {
	m := bhModel(t, bhSessions()...)
	// busy session: archived with a warning
	bhSelect(t, &m, "b")
	cmd := m.toggleArchive()
	if _, ok := bhHasToast(cmd, "está rodando"); !ok {
		t.Fatal("archiving a busy session warns it comes back")
	}
	if bhContains(sessOrder(&m), "b") {
		t.Fatal("archived session leaves the tree")
	}
	// in the inbox X archives the selected one
	m.tab = tabInbox
	m.view.InboxCursor = 0
	_, cmd = m.keyNormal(bhRune('X'))
	if _, ok := bhHasToast(cmd, "aba 4 Arquivo"); !ok {
		t.Fatal("X in the inbox archives")
	}
	if m.snap.Sessions[0].Archived != true {
		t.Fatal("a is archived")
	}
	// in the archive tab X restores
	m.tab = tabArchive
	m.rebuild()
	bhSelect(t, &m, "a")
	if _, ok := bhHasToast(m.toggleArchive(), "restaurada"); !ok {
		t.Fatal("X restores")
	}
	// context row in the archive tab: restore all, asking
	m.rebuild()
	m.view.Cursor = 0
	if m.toggleArchive() != nil {
		t.Fatal("asks first")
	}
	if d := m.top().(*confirmDialog); !strings.Contains(d.q, "restaurar as 1 sessões") {
		t.Fatalf("restore question: %q", d.q)
	}
	if _, ok := bhHasToast(m.key(bhRune('y')), "1 sessões: restaurar ✓"); !ok {
		t.Fatal("restored")
	}
	// nothing under the cursor
	empty := bhModel(t)
	if empty.toggleArchive() != nil {
		t.Fatal("nothing to archive")
	}
	// a context row with all its sessions folded away has none listed
	m2 := bhModel(t, bhSessions()...)
	m2.view.Collapsed["ctx:c1"] = true
	m2.rebuild()
	m2.view.Cursor = 0
	if m2.toggleArchive() != nil || m2.top() != nil {
		t.Fatal("a folded context lists no sessions to archive")
	}
}

func bhContains(xs []string, x string) bool {
	for _, v := range xs {
		if v == x {
			return true
		}
	}
	return false
}

func TestBehaviorExitBanner(t *testing.T) {
	m := bhModel(t)
	if got := m.ExitBanner(); got != "ᓚᘏᗢ até mais\n" {
		t.Fatalf("nothing running: %q", got)
	}
	m.snap.Sessions = []store.Session{
		{ShortID: "aaaa1111", State: "busy", Name: "rodando"},
		{ShortID: "bbbb2222", State: "blocked", Attention: store.AttnBlocked, Name: "esperando"},
		{ShortID: "cccc3333", State: "idle", Name: "parada"},
		{State: "busy", Name: "interativa"},
	}
	got := m.ExitBanner()
	for _, want := range []string{"seguem em background", "claude attach aaaa1111", "claude attach bbbb2222"} {
		if !strings.Contains(got, want) {
			t.Errorf("banner lacks %q:\n%s", want, got)
		}
	}
	if strings.Contains(got, "cccc3333") || strings.Contains(got, "interativa") {
		t.Errorf("only background sessions still running or waiting:\n%s", got)
	}
}

func TestBehaviorSyncGroupNames(t *testing.T) {
	m := bhModel(t, bhSessions()...)
	g := groups.Load()
	g.Add("t", groups.Member{SessionID: "a", Name: "velho"}, groups.Member{SessionID: "gone", Name: "x"})
	if err := g.Save(); err != nil {
		t.Fatal(err)
	}
	m.update(bhRefreshMsg(t, &m)) // the refresh syncs them (app.RefreshNotices)
	got := groups.Load().Groups["t"]
	if len(got) != 2 || got[0].Name != "alpha" || got[0].Cwd != "/tmp" || got[1].Name != "x" {
		t.Fatalf("names follow the sessions, unknown members stay: %+v", got)
	}
	// every refresh does
	if err := os.Remove(groups.Path()); err != nil {
		t.Fatal(err)
	}
	m.update(bhRefreshMsg(t, &m))
	if len(m.app.Groups().Groups) != 0 {
		t.Fatal("snapMsg reloads groups.json")
	}
}

func TestBehaviorSessionNamed(t *testing.T) {
	m := bhModel(t, bhSessions()...)
	m.snap.Sessions[2].CustomName = "meu nome"
	for _, c := range []struct{ in, want string }{
		{"alpha", "a"}, {"meu nome", "c"}, {"brav…", "b"}, {"…", ""}, {"", ""}, {"nada", ""},
	} {
		got := ""
		if s := m.sessionNamed(c.in); s != nil {
			got = s.SessionID
		}
		if got != c.want {
			t.Errorf("sessionNamed(%q)=%q want %q", c.in, got, c.want)
		}
	}
	if m.labelOf("a") != "alpha" || m.labelOf("zz") != "" {
		t.Fatal("labelOf")
	}
}
