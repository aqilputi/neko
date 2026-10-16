package ui

import (
	"errors"
	"strings"
	"testing"
	"time"

	tea "charm.land/bubbletea/v2"
	"charm.land/lipgloss/v2"
	"github.com/charmbracelet/x/ansi"

	"github.com/aqilputi/nekuroddo/internal/claude"
	"github.com/aqilputi/nekuroddo/internal/store"
	"github.com/aqilputi/nekuroddo/internal/term"
)

// rhScreen checks the whole frame is exactly w×h cells and returns it stripped.
func rhScreen(t *testing.T, m Model) string {
	t.Helper()
	out := m.View().Content
	lines := strings.Split(out, "\n")
	if len(lines) != m.h {
		t.Fatalf("frame has %d lines, want %d", len(lines), m.h)
	}
	for i, l := range lines {
		if w := lipgloss.Width(l); w != m.w {
			t.Fatalf("frame line %d is %d wide, want %d: %q", i, w, m.w, ansi.Strip(l))
		}
	}
	return ansi.Strip(out)
}

func TestRhViewBeforeFirstResize(t *testing.T) {
	m := rhModel(t)
	m.w = 0
	v := m.View()
	if !strings.Contains(v.Content, "carregando") {
		t.Fatalf("no size yet: %q", v.Content)
	}
}

func TestRhViewFrameAndTitle(t *testing.T) {
	m := rhFilled(t)
	rhCursor(&m, "a")
	out := rhScreen(t, m)
	first := strings.Split(out, "\n")[0]
	for _, w := range []string{"nekuroddo", "● 1 rodando", "◆ 2 te esperando", "✓ 0 voltaram", "5h", "40%", "semana 75%"} {
		if !strings.Contains(first, w) {
			t.Fatalf("header misses %q: %q", w, first)
		}
	}
	rhHas(t, out, " sessões ", " detalhes ", "NORMAL")
	v := m.View()
	if v.WindowTitle != "nekuroddo · 2 esperando · 1 rodando" {
		t.Fatalf("window title %q", v.WindowTitle)
	}
	if v.ProgramStatus == nil || v.ProgramStatus.State != tea.ProgramStateBlocked || !strings.Contains(v.ProgramStatus.Message, "2 te esperando") {
		t.Fatalf("someone waiting: blocked status, got %+v", v.ProgramStatus)
	}
}

func TestRhProgramStatusStates(t *testing.T) {
	m := rhModel(t)
	m.snap.Sessions = []store.Session{{SessionID: "x", State: "busy"}}
	if ps := m.View().ProgramStatus; ps.State != tea.ProgramStateWorking || ps.Message != "1 rodando" {
		t.Fatalf("busy only: %+v", ps)
	}
	m.snap.Sessions = nil
	if ps := m.View().ProgramStatus; ps.State != tea.ProgramStateIdle {
		t.Fatalf("nothing: idle, got %+v", ps)
	}
}

func TestRhHeaderWithoutUsage(t *testing.T) {
	m := rhModel(t)
	m.snap.TodayCost = 1.5
	h := m.header(m.t())
	rhFits(t, h, m.w)
	rhHas(t, h, "hoje ≈$1.50", m.t().Name)
	rhHasNot(t, h, "/$")
	m.cfg.DailyBudgetUSD = 10
	rhHas(t, m.header(m.t()), "/$10")
	m.snap.Err = errors.New("ps falhou")
	rhHas(t, m.header(m.t()), "⚠ ps falhou")
}

// A header wider than the screen must be cut, not wrapped: a second header
// line pushes the tabs down (mouse rects assume tabs on row 1) and the
// status line falls off the bottom.
func TestRhHeaderNarrowStaysOneLine(t *testing.T) {
	m := rhFilled(t)
	m.w, m.h = 100, 24
	h := m.header(m.t())
	if n := len(rhLines(h)); n != 1 {
		t.Fatalf("header must be one line, got %d", n)
	}
	lines := rhLines(m.View().Content)
	if !strings.Contains(lines[1], "1 Sessões") || !strings.Contains(lines[len(lines)-1], "NORMAL") {
		t.Fatalf("tabs on row 1 and status on the last row:\n%s", strings.Join(lines, "\n"))
	}
}

func TestRhTabs(t *testing.T) {
	m := rhFilled(t)
	out := m.tabs(m.t())
	rhFits(t, out, m.w)
	rhHas(t, out, "1 Sessões", "2 Inbox (2)", "3 Todos (0)", "4 Arquivo (0)", "ativas ≤3d", "ordem: atividade", "1 ocultas (H)")
	rhHasNot(t, out, "filtro")
	m.showAll, m.filter = true, "dep"
	m.rebuild()
	out = m.tabs(m.t())
	rhHas(t, out, "tudo", "filtro: /dep")
	rhHasNot(t, out, "ocultas")
	m.w = 30
	if out := m.tabs(m.t()); len(rhLines(out)) != 1 || lipgloss.Width(out) > 30 {
		t.Fatalf("tabs must stay on one line within the width: %q", ansi.Strip(out))
	}
}

func TestRhStatusLineModes(t *testing.T) {
	m := rhFilled(t)
	st := m.status(m.t())
	rhFits(t, st, m.w)
	rhHas(t, st, "NORMAL", "ctrl+/", "todos os comandos", "ᓚᘏᗢ")
	rhHasNot(t, st, "↯")

	m.hooksSeen = true
	rhHas(t, m.status(m.t()), "↯")

	m.toast = "arquivada ✓"
	rhHas(t, m.status(m.t()), "arquivada ✓")
	m.toastErr = true
	rhHas(t, m.status(m.t()), "arquivada ✓")

	m.mode = modeInsert
	rhHas(t, m.status(m.t()), "INSERT")

	m.mode = modeInput
	m.input.Prompt = "responder › "
	m.input.SetValue("ok, pode seguir")
	st = m.status(m.t())
	rhHas(t, st, "INPUT", "responder ›", "ok, pode seguir")
	rhHasNot(t, st, "ctrl+/")
	rhFits(t, st, m.w)
}

func TestRhStatusLineCatMeows(t *testing.T) {
	m := rhFilled(t)
	m.cat.Meow("miau! deploy voltou")
	rhHas(t, m.status(m.t()), "miau! deploy voltou")
}

func TestRhInboxView(t *testing.T) {
	m := rhFilled(t)
	// a session asking only through Questions, and one waiting on a tool
	m.snap.Sessions = append(m.snap.Sessions,
		store.Session{SessionID: "q", Name: "pergunta", Context: "gamma", State: "blocked", Attention: store.AttnBlocked,
			LastAt: time.Now(), Questions: []claude.Question{{Question: "Posso apagar?"}}},
		store.Session{SessionID: "w", Name: "permissão", Context: "gamma", State: "blocked", Attention: store.AttnBlocked,
			LastAt: time.Now(), WaitingTool: "Bash(ls)"},
		store.Session{SessionID: "r", Name: "voltou", Context: "gamma", State: "done", Attention: store.AttnReturned,
			LastAt: time.Now(), LastText: "pronto, terminei"},
	)
	m.mail["q"] = mailCue{at: time.Now()}
	m.tab = tabInbox
	m.rebuild()
	out := m.inboxView(m.t(), 60, 40)
	rhFits(t, out, 60)
	rhHas(t, out, "❓ qual bucket usar?", "❓ Posso apagar?", "🔐 Bash(ls)", "pronto, terminei", "no prompt", "✉←")
	// 4 lines per item
	if n := len(rhLines(out)); n != 4*len(m.inbox) {
		t.Fatalf("want %d lines, got %d", 4*len(m.inbox), n)
	}
	// short panel: only as many items as fit, around the cursor
	m.inboxCur = len(m.inbox) - 1
	out = m.inboxView(m.t(), 60, 8)
	if n := len(rhLines(out)); n != 8 {
		t.Fatalf("2 items of 4 lines expected, got %d", n)
	}
	rhHas(t, out, m.inbox[len(m.inbox)-1].Label())

	rhHas(t, m.left(m.t(), 50, 20), "inbox · quem te espera")
}

func TestRhInboxEmpty(t *testing.T) {
	m := rhModel(t)
	m.rebuild()
	rhHas(t, m.inboxView(m.t(), 40, 10), "ninguém te esperando")
}

func TestRhViewWithDialogs(t *testing.T) {
	m := rhFilled(t)
	m.confirmThen("arquivar as 3 sessões de \"alpha\" e todas as outras?", func(*Model) tea.Cmd { return nil })
	out := rhScreen(t, m)
	rhHas(t, out, "confirma?", "arquivar as 3 sessões", "y/enter sim", "qualquer outra tecla: não")
	m.pop()

	m.push(helpDialog{})
	out = rhScreen(t, m)
	rhHas(t, out, "ajuda · qualquer tecla fecha", "mouse: clique seleciona")
	m.pop()

	m.push(usageDialog{})
	out = rhScreen(t, m)
	rhHas(t, out, "uso · /usage", "Current session", "40%", "reseta Oct 15, 11:30am", "What's contributing", "linha dois", "atualizado há agora")
	m.pop()
}

func TestRhUsageDialogStates(t *testing.T) {
	m := rhModel(t)
	d := usageDialog{}
	rhHas(t, d.view(&m, m.t(), 80), "consultando claude -p /usage")
	m.usageErr = errors.New("exit status 1")
	rhHas(t, d.view(&m, m.t(), 80), "não consegui ler o /usage: exit status 1")

	// a long report is cut with … so the dialog fits the screen
	m.usage = rhUsage()
	m.usage.Raw = "What's contributing\n" + strings.Repeat("linha\n", 200)
	out := d.view(&m, m.t(), 80)
	rhHas(t, out, " …")
	if n := len(rhLines(out)); n > m.h {
		t.Fatalf("usage dialog taller than the screen: %d", n)
	}
}

func TestRhConfirmDialogWidth(t *testing.T) {
	m := rhModel(t)
	short := &confirmDialog{q: "sair?"}
	if w := short.width(&m); w != confirmFooterW+6 {
		t.Fatalf("short question: as wide as the footer (%d), got %d", confirmFooterW+6, w)
	}
	long := &confirmDialog{q: strings.Repeat("x", 300)}
	if w := long.width(&m); w != m.w-4 {
		t.Fatalf("long question: capped at screen-4, got %d", w)
	}
	rhFits(t, long.view(&m, m.t(), long.width(&m)), long.width(&m))
}

func TestRhZoomWithoutPaneShowsDetailsFullWidth(t *testing.T) {
	m := rhFilled(t)
	rhCursor(&m, "a")
	m.zoom, m.active = true, "gone"
	out := rhScreen(t, m)
	rhHas(t, out, " detalhes ")
	rhHasNot(t, out, " sessões ")
}

func TestRhPaneTitleAndHints(t *testing.T) {
	m := rhFilled(t)
	r := m.rects().pane
	p, err := term.Start("bg1", "deploy do rc", "/", r.Dx(), r.Dy(), "sh", "-c", "sleep 5")
	if err != nil {
		t.Fatal(err)
	}
	defer p.Close()
	m.panes["bg1"], m.active = p, "bg1"
	out := rhScreen(t, m)
	rhHas(t, out, "▸ deploy do rc", "i entra · w fecha · z zoom")

	m.mode = modeInsert
	out = rhScreen(t, m)
	rhHas(t, out, "INSERT (ctrl+] sai)")

	// zoomed: the list is gone, the pane takes the whole width
	m.zoom = true
	out = rhScreen(t, m)
	rhHasNot(t, out, " sessões ")
	if lw, rw, _ := m.layout(); lw != 0 || rw != m.w {
		t.Fatalf("zoom layout: %d/%d", lw, rw)
	}
}

func TestRhLayoutNarrowScreen(t *testing.T) {
	m := rhFilled(t)
	m.w, m.h = 60, 3
	lw, rw, bh := m.layout()
	if lw != 30 || rw != 30 || bh != 5 {
		t.Fatalf("narrow: list gets half, body at least 5 lines: %d %d %d", lw, rw, bh)
	}
	m.w = 140
	if lw, _, _ := m.layout(); lw != 46 {
		t.Fatalf("list gets a third: %d", lw)
	}
	m.w = 90
	if lw, _, _ := m.layout(); lw != 36 {
		t.Fatalf("list at least 36: %d", lw)
	}
}

func TestRhSmallFramesStayInBounds(t *testing.T) {
	for _, sz := range [][2]int{{80, 24}, {100, 30}, {200, 50}} {
		m := rhFilled(t)
		m.w, m.h = sz[0], sz[1]
		rhCursor(&m, "b")
		rhScreen(t, m)
		m.tab = tabInbox
		m.rebuild()
		rhScreen(t, m)
	}
}

// The confirm footer ("y/enter sim   qualquer outra tecla: não") is 41
// cells; a short question gives a 40-wide box (38 inside), so the footer
// wraps and, the box height being computed before the wrap, its bottom
// border is cut off.
func TestRhConfirmShortQuestionKeepsBox(t *testing.T) {
	m := rhFilled(t)
	m.confirmThen("sair?", func(*Model) tea.Cmd { return nil })
	out := rhScreen(t, m)
	rhHas(t, out, "qualquer outra tecla: não")
	d := m.top()
	box := dialogBox(m.t(), d.title(), d.view(&m, m.t(), d.width(&m)), d.width(&m))
	lines := rhLines(box)
	if !strings.HasPrefix(lines[len(lines)-1], "╰") {
		t.Fatalf("dialog box lost its bottom border:\n%s", strings.Join(lines, "\n"))
	}
}

func TestRhPaletteView(t *testing.T) {
	m := rhFilled(t)
	m.openPalette()
	out := rhScreen(t, m)
	rhHas(t, out, "╭─ comandos ", ": ", "↑/↓ ctrl+n/p · enter executa · esc fecha")

	d := m.top().(*paletteDialog)
	w := d.width(&m)
	d.input.SetValue("group Deploy RC")
	d.filter(&m)
	view := d.view(&m, m.t(), w)
	rhFits(t, view, w)
	if first := rhLines(view)[2]; !strings.Contains(first, "▶ group #deploy-rc") {
		t.Fatalf("a complete command goes on top: %q", first)
	}

	d.input.SetValue("zzzzqqqq")
	d.filter(&m)
	rhHas(t, d.view(&m, m.t(), w), "nada encontrado")

	// many matches: the list scrolls around the cursor and keeps its size
	d.input.SetValue("")
	d.filter(&m)
	d.cur = len(d.matches) - 1
	if n := len(rhLines(d.view(&m, m.t(), w))); n > 12+4 {
		t.Fatalf("palette shows at most 12 items: %d lines", n)
	}
}

func TestRhSwitcherView(t *testing.T) {
	m := rhFilled(t)
	m.openSwitcher()
	out := rhScreen(t, m)
	rhHas(t, out, "ir para chat · ctrl+k", "migração", "te esperando · alpha", "deploy do rc")
	rhHasNot(t, m.top().view(&m, m.t(), 80), "antiga") // history: not live
}

func TestRhUsageColorThresholds(t *testing.T) {
	m := rhModel(t)
	th := m.t()
	if usageColor(th, 10) != th.Done || usageColor(th, 70) != th.Accent || usageColor(th, 95) != th.Error {
		t.Fatal("green below 70, yellow from 70, red from 90")
	}
}
