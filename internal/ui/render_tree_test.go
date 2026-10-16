package ui

import (
	"fmt"
	"strings"
	"testing"
	"time"

	"github.com/charmbracelet/x/ansi"

	"github.com/aqilputi/nekuroddo/internal/groups"
	"github.com/aqilputi/nekuroddo/internal/store"
)

func TestRhTreeShowsContextsSessionsAndSubagents(t *testing.T) {
	m := rhFilled(t)
	out := m.treeView(m.t(), 60, 30)
	rhFits(t, out, 60)
	rhHas(t, out, "▾ alpha", "(2)", "▾ beta", "deploy do rc", "migração", "antiga", "terminal")
	rhHasNot(t, out, "esquecida") // stale: hidden unless H

	// one running and one finished subagent are counted on the session line
	a := rhLineWith(out, "deploy do rc")
	if !strings.Contains(a, "⑂1") || !strings.Contains(a, "✓1") {
		t.Fatalf("session line should count subagents: %q", a)
	}
	// only the running one gets its own row, with the model's short name
	sub := rhLineWith(out, "explorar código")
	if !strings.Contains(sub, "└") || !strings.Contains(sub, "◌") || !strings.Contains(sub, "haiku-4-5") || strings.Contains(sub, "20251001") {
		t.Fatalf("sub row: %q", sub)
	}
	rhHasNot(t, out, "revisar")

	// the window share: a used 3/4 of 40%, the context sums to 40%
	if !strings.Contains(a, "30%") {
		t.Fatalf("a should show 30%% of the window: %q", a)
	}
	if l := rhLineWith(out, "▾ alpha"); !strings.Contains(l, "40%") {
		t.Fatalf("context row should sum its sessions: %q", l)
	}
	// icons: blocked ◆/◇, history ·, idle at prompt ❯
	if l := rhLineWith(out, "migração"); !strings.ContainsAny(l, "◆◇") {
		t.Fatalf("blocked icon missing: %q", l)
	}
	if l := rhLineWith(out, "terminal"); !strings.Contains(l, "❯") {
		t.Fatalf("idle icon missing: %q", l)
	}
}

func TestRhTreeCursorBar(t *testing.T) {
	m := rhFilled(t)
	rhCursor(&m, "b")
	lines := rhLines(m.treeView(m.t(), 60, 30))
	marked := 0
	for _, l := range lines {
		if strings.HasPrefix(l, "▌") {
			marked++
			if !strings.Contains(l, "migração") {
				t.Fatalf("cursor bar on the wrong row: %q", l)
			}
		}
	}
	if marked != 1 {
		t.Fatalf("want exactly one cursor row, got %d", marked)
	}
}

func TestRhTreeCollapsedContext(t *testing.T) {
	m := rhFilled(t)
	m.view.Collapsed["ctx:alpha"] = true
	m.rebuild()
	out := m.treeView(m.t(), 60, 30)
	rhHas(t, out, "▸ alpha", "▾ beta")
	rhHasNot(t, out, "deploy do rc", "migração", "explorar código")
}

func TestRhTreeShowAllRevealsStaleAndDoneSubs(t *testing.T) {
	m := rhFilled(t)
	m.view.ShowAll = true
	m.rebuild()
	out := m.treeView(m.t(), 70, 30)
	rhHas(t, out, "esquecida", "revisar")
	if l := rhLineWith(out, "esquecida"); !strings.Contains(l, "z") {
		t.Fatalf("stale session should show z: %q", l)
	}
	if l := rhLineWith(out, "revisar"); !strings.Contains(l, "●") {
		t.Fatalf("finished subagent should show ●: %q", l)
	}
}

func TestRhTreeEmptyMessages(t *testing.T) {
	m := rhModel(t)
	m.rebuild()
	rhHas(t, m.treeView(m.t(), 50, 10), "nenhuma sessão ainda")
	m.refreshing = true
	rhHas(t, m.treeView(m.t(), 50, 10), "procurando sessões")
	m.refreshing = false
	m.tab = tabArchive
	rhHas(t, m.treeView(m.t(), 60, 10), "nada arquivado")
}

func TestRhTreeArchiveTab(t *testing.T) {
	m := rhFilled(t)
	m.snap.Sessions[3].Archived = true
	m.snap.Sessions[3].ArchivedAt = time.Now()
	m.tab = tabArchive
	m.rebuild()
	out := m.left(m.t(), 50, 12)
	rhHas(t, out, "arquivo · X restaura", "esquecida", "⌫")
	rhHasNot(t, out, "deploy do rc")
	rhFits(t, out, 50)
}

func TestRhTreePickedAndGroups(t *testing.T) {
	m := rhFilled(t)
	m.grp.Add("deploy-rc", groups.Member{SessionID: "a", Name: "deploy do rc"}, groups.Member{SessionID: "b", Name: "migração"})
	m.grp.Add("outro", groups.Member{SessionID: "a", Name: "deploy do rc"})
	m.picked["b"] = true
	rhCursor(&m, "a")
	out := m.treeView(m.t(), 80, 30)
	rhFits(t, out, 80)
	a := rhLineWith(out, "deploy do rc")
	if !strings.Contains(a, "#") || !strings.Contains(a, "+1") {
		t.Fatalf("a is in two groups: first chip and +1 expected: %q", a)
	}
	b := rhLineWith(out, "migração")
	if !strings.Contains(b, "◆ migração") || !strings.Contains(b, "#deploy-rc") {
		t.Fatalf("picked b shows ◆ and its chip: %q", b)
	}
	if !strings.Contains(b, "┃") {
		t.Fatalf("b is a peer of the cursor: highlighted bar expected: %q", b)
	}
	// with the cursor elsewhere the bar is the plain ▍
	rhCursor(&m, "e")
	if b := rhLineWith(m.treeView(m.t(), 80, 30), "migração"); !strings.Contains(b, "▍") || strings.Contains(b, "┃") {
		t.Fatalf("not a peer of the cursor: %q", b)
	}
}

func TestRhTreeMailMark(t *testing.T) {
	m := rhFilled(t)
	m.mail["a"] = mailCue{at: time.Now(), sent: true}
	m.mail["b"] = mailCue{at: time.Now().Add(-5 * time.Minute)} // dim, still shown
	m.mail["e"] = mailCue{at: time.Now().Add(-time.Hour)}       // gone
	out := m.treeView(m.t(), 80, 30)
	if !strings.Contains(rhLineWith(out, "deploy do rc"), "✉→") || !strings.Contains(rhLineWith(out, "migração"), "✉←") {
		t.Fatalf("mail cues missing:\n%s", ansi.Strip(out))
	}
	if strings.Contains(rhLineWith(out, "terminal"), "✉") {
		t.Fatal("an old cue fades out")
	}
}

func TestRhTreeScrollsToCursor(t *testing.T) {
	m := rhModel(t)
	var ss []store.Session
	for i := 0; i < 30; i++ {
		ss = append(ss, store.Session{SessionID: fmt.Sprint("s", i), Name: fmt.Sprintf("sessão %02d", i), Context: "c",
			LastAt: time.Now().Add(-time.Duration(i) * time.Minute)})
	}
	m.snap.Sessions = ss
	m.rebuild()
	rhCursor(&m, "s25")
	lines := rhLines(m.treeView(m.t(), 50, 6))
	if len(lines) != 6 {
		t.Fatalf("want 6 lines, got %d", len(lines))
	}
	if !strings.Contains(strings.Join(lines, "\n"), "sessão 25") || strings.Contains(strings.Join(lines, "\n"), "sessão 01") {
		t.Fatalf("view should follow the cursor:\n%s", strings.Join(lines, "\n"))
	}
}

func TestRhTreeNarrowTruncates(t *testing.T) {
	m := rhFilled(t)
	m.snap.Sessions[0].CustomName = strings.Repeat("nome muito comprido ", 6)
	m.snap.Sessions[0].Subs[0].Description = ""
	m.snap.Sessions[0].Subs[0].Type = "general-purpose"
	m.rebuild()
	out := m.treeView(m.t(), 40, 30)
	rhFits(t, out, 40)
	rhHas(t, out, "…", "general-purpose") // no description: the subagent's type
}

func TestRhStyledLabels(t *testing.T) {
	// history, stale and attention labels all still render their names
	m := rhFilled(t)
	m.view.ShowAll = true
	m.snap.Sessions[0].Attention = store.AttnReturned
	m.snap.Sessions[0].State = "done"
	m.rebuild()
	out := m.treeView(m.t(), 70, 30)
	if l := rhLineWith(out, "deploy do rc"); !strings.Contains(l, "✓") {
		t.Fatalf("returned session shows ✓: %q", l)
	}
	rhHas(t, out, "antiga", "esquecida")
}
