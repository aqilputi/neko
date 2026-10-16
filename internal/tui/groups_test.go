package tui

import (
	"errors"
	"path/filepath"
	"strings"
	"testing"
	"time"

	tea "charm.land/bubbletea/v2"
	"charm.land/lipgloss/v2"
	"github.com/charmbracelet/x/ansi"

	"github.com/aqilputi/nekuroddo/internal/app"
	"github.com/aqilputi/nekuroddo/internal/config"
	"github.com/aqilputi/nekuroddo/internal/core"
	"github.com/aqilputi/nekuroddo/internal/events"
	"github.com/aqilputi/nekuroddo/internal/groups"
	"github.com/aqilputi/nekuroddo/internal/store"
)

func groupModel(t *testing.T) Model {
	t.Helper()
	t.Setenv("XDG_STATE_HOME", filepath.Join(t.TempDir(), "state"))
	t.Setenv("NEKO_CLAUDE", "/bin/false") // refresh must not touch the real claude
	m := newTestModel(config.Default())
	m.w, m.h = 140, 40
	setSnap(&m, store.Snapshot{Sessions: []store.Session{
		{SessionID: "a", Name: "deploy rc 1", Context: "ctx", State: "busy", LastAt: time.Now()},
		{SessionID: "b", Name: "deploy rc 2", Context: "ctx", State: "busy", LastAt: time.Now().Add(-time.Minute)},
		{SessionID: "c", Name: "outra coisa", Context: "ctx", LastAt: time.Now().Add(-2 * time.Minute)},
	}})
	m.rebuild()
	return m
}

func (m *Model) cursorOn(id string) {
	for i, r := range m.rows {
		if r.Kind == core.RowSess && r.Sess.SessionID == id {
			m.view.Cursor = i
		}
	}
}

// typePalette opens the palette, types q and hits enter.
func typePalette(m *Model, q string) {
	m.keyNormal(keyPress(":"))
	for _, r := range q {
		m.key(tea.KeyPressMsg{Code: r, Text: string(r)})
	}
	m.key(tea.KeyPressMsg{Code: tea.KeyEnter})
}

func TestPickAndGroup(t *testing.T) {
	m := groupModel(t)
	m.cursorOn("a")
	m.key(keyPress("space")) // picks a, moves to b
	m.key(keyPress("space")) // picks b
	if !m.picked["a"] || !m.picked["b"] || m.picked["c"] {
		t.Fatalf("space should pick a and b: %v", m.picked)
	}
	typePalette(&m, "group Deploy RC")
	if m.top() != nil {
		t.Fatal("enter on a complete command should close the palette")
	}
	g := groups.Load()
	if got := g.Of("a"); len(got) != 1 || got[0] != "deploy-rc" {
		t.Fatalf("a should be in #deploy-rc, got %v", got)
	}
	if len(g.Groups["deploy-rc"]) != 2 || len(m.picked) != 0 {
		t.Fatalf("group should have a and b and clear the picks: %v %v", g.Groups, m.picked)
	}
	if g.Groups["deploy-rc"][0].Name == "" {
		t.Fatal("members keep their SendMessage name")
	}

	// the tree shows the chip; the cursor's peers are highlighted
	m.cursorOn("a")
	out := ansi.Strip(m.treeView(m.t(), 70, 20))
	if strings.Count(out, "#deploy-rc") != 2 || !strings.Contains(out, "┃") {
		t.Fatalf("tree should show the chip twice and a highlighted peer:\n%s", out)
	}

	// ungroup the one under the cursor
	typePalette(&m, "ungroup")
	if got := groups.Load().Of("a"); len(got) != 0 {
		t.Fatalf("a should have left the group: %v", got)
	}
}

func TestEscClearsPicksFirst(t *testing.T) {
	m := groupModel(t)
	m.cursorOn("a")
	m.key(keyPress("space"))
	m.view.Filter = "x"
	m.key(tea.KeyPressMsg{Code: tea.KeyEscape})
	if len(m.picked) != 0 || m.view.Filter != "x" {
		t.Fatalf("esc should drop the picks and keep the filter: %v %q", m.picked, m.view.Filter)
	}
}

func TestSpaceOnContextFolds(t *testing.T) {
	m := groupModel(t)
	m.view.Cursor = 0
	m.key(keyPress("space"))
	if !m.view.Collapsed["ctx:ctx"] || len(m.picked) != 0 {
		t.Fatal("space on a context row should fold it, not pick")
	}
}

func TestPaletteIncompleteCommandStaysOpen(t *testing.T) {
	m := groupModel(t)
	typePalette(&m, "group")
	if _, ok := m.top().(*paletteDialog); !ok {
		t.Fatal("a command missing its argument must keep the palette open")
	}
}

func TestPeerMessageCue(t *testing.T) {
	m := groupModel(t)
	m.onEvents([]events.Event{{At: time.Now(), Name: "UserPromptSubmit", SessionID: "b", Type: "peer",
		From: "deploy rc 1", Message: "[#deploy-rc] bug: tag errada"}})
	if c, ok := m.app.MailCueOf("b"); !ok || c.Sent {
		t.Fatal("receiver should get ✉←")
	}
	if c, ok := m.app.MailCueOf("a"); !ok || !c.Sent {
		t.Fatal("sender, found by name, should get ✉→")
	}
	if !m.mailBlinking() {
		t.Fatal("a fresh message blinks")
	}
	out := ansi.Strip(m.treeView(m.t(), 70, 20))
	if !strings.Contains(out, "✉→") || !strings.Contains(out, "✉←") {
		t.Fatalf("tree should show both cues:\n%s", out)
	}
	s := m.session("b")
	detail := ansi.Strip(strings.Join(m.groupsDetail(m.t(), s, 60), "\n"))
	if !strings.Contains(detail, "tag errada") {
		t.Fatalf("details should list the message:\n%s", detail)
	}
	// old cues fade out
	cue(t, &m, "b", time.Now().Add(-mailKeep-time.Second), false)
	if mark, _ := m.mailMark(m.t(), "b"); mark != "" {
		t.Fatal("cue should be gone after mailKeep")
	}
}

func TestNotifyQueuesForMembers(t *testing.T) {
	m := groupModel(t)
	bhSaveGroup(t, &m, "deploy-rc", groups.Member{SessionID: "a", Name: "deploy rc 1"}, groups.Member{SessionID: "b", Name: "deploy rc 2"})
	typePalette(&m, "notify #deploy-rc o bucket mudou")
	for _, id := range []string{"a", "b"} {
		var out strings.Builder
		groups.Hook(strings.NewReader(`{"hook_event_name":"PostToolUse","session_id":"`+id+`"}`), &out)
		if !strings.Contains(out.String(), "o bucket mudou") {
			t.Fatalf("%s should get the note: %s", id, out.String())
		}
	}
	if mark, _ := m.mailMark(m.t(), "a"); mark == "" {
		t.Fatal("notify should show ✉ on the members")
	}
}

func TestGroupColorStable(t *testing.T) {
	m := groupModel(t)
	a, b := groupColor(m.t(), "deploy-rc"), groupColor(m.t(), "deploy-rc")
	if lipgloss.NewStyle().Foreground(a).Render("x") != lipgloss.NewStyle().Foreground(b).Render("x") {
		t.Fatal("a tag always gets the same color")
	}
}

func TestGroupResultCmd(t *testing.T) {
	if resultCmd(app.Result{}, nil) != nil {
		t.Fatal("nothing to say")
	}
	if tm, ok := bhHasToast(resultCmd(app.Result{}, errors.New("falhou")), "falhou"); !ok || !tm.err {
		t.Fatal("a bare error becomes an error toast")
	}
	if tm, ok := bhHasToast(resultCmd(app.Result{Toast: "x"}, errors.New("y")), "x"); !ok || !tm.err {
		t.Fatal("the result's toast wins, as an error")
	}
}
