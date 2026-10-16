package tui

import (
	"strings"
	"testing"
	"time"

	"github.com/aqilputi/nekuroddo/internal/claude"
	"github.com/aqilputi/nekuroddo/internal/events"
	"github.com/aqilputi/nekuroddo/internal/groups"
	"github.com/aqilputi/nekuroddo/internal/store"
)

func TestRhSessDetailBlockedSession(t *testing.T) {
	m := rhFilled(t)
	s := m.session("b")
	out := m.sessDetail(m.t(), s, 70)
	rhFits(t, out, 70)
	rhHas(t, out, "migração", "te esperando", "alpha", "tipo", "background · bg2",
		"status", "esperando decisão",
		"❓ precisa de você", "qual bucket usar?",
		"Qual região?", "us-east1 — mais barato", "sa-east1", "Apagar o antigo?",
		"💡 resposta sugerida", "us-east1, sem apagar",
		"⇡ PR migra bucket", "🔗 relatório",
		"enter abre aqui")
	// options are numbered across questions: 1, 2 for the first, 3 for the second
	if l := rhLineWith(out, "sim"); !strings.Contains(l, " 3 ") {
		t.Fatalf("second question's option should be #3: %q", l)
	}
	if l := rhLineWith(out, "sa-east1"); !strings.Contains(l, " 2 ") {
		t.Fatalf("want option #2: %q", l)
	}
	// Needs / Questions present: the pending-tool section stays out
	rhHasNot(t, out, "🔐")
}

func TestRhSessDetailBusyWithSubagents(t *testing.T) {
	m := rhFilled(t)
	out := m.sessDetail(m.t(), m.session("a"), 70)
	rhFits(t, out, 70)
	rhHas(t, out, "rodando", "/srv/alpha", "opus-4-1", "1234 out",
		"você pediu", "faz o deploy", "claude disse por último", "rodando os testes",
		"subagents (2)", "◌ explorar código · haiku-4-5", "● revisar · sonnet-4-5",
		"≈30% de 40% usados")
}

func TestRhSessDetailPendingTool(t *testing.T) {
	m := rhFilled(t)
	s := &store.Session{SessionID: "x", Name: "perm", Kind: "background", State: "blocked",
		Attention: store.AttnBlocked, WaitingTool: "Bash(rm -rf build)", WaitingFor: "permission prompt"}
	out := m.sessDetail(m.t(), s, 60)
	rhHas(t, out, "te esperando · permission prompt", "🔐 última ferramenta pendente", "Bash(rm -rf build)")
}

func TestRhSessDetailArchivedHookAndHints(t *testing.T) {
	m := rhFilled(t)
	s := m.session("e")
	s.Archived, s.ArchivedAt = true, time.Date(2026, 3, 4, 5, 6, 0, 0, time.Local)
	m.app.OnHookEvents([]events.Event{{At: time.Now(), Name: "Notification", SessionID: "e", Message: "Claude needs your permission"}})
	out := m.sessDetail(m.t(), s, 70)
	rhHas(t, out, "arquivada", "04/03 05:06 · X restaura", "hook", "Notification · Claude needs your permission",
		"aberta num terminal (pid 42)")

	out = m.sessDetail(m.t(), m.session("c"), 70)
	rhHas(t, out, "histórico", "enter retoma em background")
}

func TestRhSessDetailGroups(t *testing.T) {
	m := rhFilled(t)
	bhSaveGroup(t, &m, "deploy-rc", groups.Member{SessionID: "a", Name: "deploy do rc", Label: "deploy do rc"}, groups.Member{SessionID: "b", Name: "migração"})
	m.app.OnPeer(events.Event{At: time.Now(), Type: "peer", SessionID: "b", From: "deploy do rc", Message: "bucket mudou"})
	out := m.sessDetail(m.t(), m.session("a"), 70)
	rhHas(t, out, "▍#deploy-rc", "com migração", "✉ mensagens", "→ migração", "bucket mudou")
	out = m.sessDetail(m.t(), m.session("b"), 70)
	rhHas(t, out, "com deploy do rc", "← deploy do rc")
}

func TestRhWindowLine(t *testing.T) {
	m := rhFilled(t)
	b := m.session("a")
	rhHas(t, m.windowLine(m.t(), b, rhBase(m)), "≈30% de 40% usados")
	rhHas(t, m.windowLine(m.t(), m.session("c"), rhBase(m)), "sem uso nesta janela (total 40%)")
	m.app.OnUsage(claude.UsageReport{}, nil)
	rhHas(t, m.windowLine(m.t(), b, rhBase(m)), "— (sem /usage)")
}

func TestRhCtxDetail(t *testing.T) {
	m := rhFilled(t)
	out := m.ctxDetail(m.t(), "alpha", 60)
	rhFits(t, out, 60)
	rhHas(t, out, "alpha", "uso da janela de 5h por sessão", "deploy do rc", "migração")
	for k, v := range map[string]string{"sessões": "2", "rodando": "1", "esperando": "1"} {
		if l := rhLineWith(out, k); !strings.HasSuffix(strings.TrimSpace(l), v) {
			t.Fatalf("%s should be %s: %q", k, v, l)
		}
	}
	if l := rhLineWith(out, "janela 5h"); !strings.Contains(l, "≈40%") {
		t.Fatalf("context window total: %q", l)
	}
	// beta: no window use at all, stale one excluded
	out = m.ctxDetail(m.t(), "beta", 60)
	if l := rhLineWith(out, "sessões"); !strings.HasSuffix(strings.TrimSpace(l), "2") {
		t.Fatalf("stale sessions don't count: %q", l)
	}
	if l := rhLineWith(out, "janela 5h"); !strings.Contains(l, "≈—") {
		t.Fatalf("no use shows a dash: %q", l)
	}
	m.view.ShowAll = true
	if l := rhLineWith(m.ctxDetail(m.t(), "beta", 60), "sessões"); !strings.HasSuffix(strings.TrimSpace(l), "3") {
		t.Fatalf("H counts stale ones too: %q", l)
	}
}

func TestRhRightPanelFollowsCursor(t *testing.T) {
	m := rhFilled(t)
	rhCursorCtx(&m, "alpha")
	out := m.right(m.t(), 80, 30)
	rhFits(t, out, 80)
	if n := len(rhLines(out)); n != 30 {
		t.Fatalf("panel must be exactly h lines, got %d", n)
	}
	rhHas(t, out, "╭─ alpha ", "uso da janela de 5h")

	rhCursor(&m, "a")
	rhHas(t, m.right(m.t(), 80, 30), " detalhes ", "deploy do rc")

	m.rows = nil
	m.view.Cursor = 0
	out = m.right(m.t(), 80, 30)
	rhHasNot(t, out, "detalhes")
	if n := len(rhLines(out)); n != 30 {
		t.Fatalf("empty panel keeps its height, got %d", n)
	}
}

func TestRhRightPanelLongDetailIsClipped(t *testing.T) {
	m := rhFilled(t)
	s := m.session("a")
	s.LastText = strings.Repeat("uma linha longa do claude ", 200)
	rhCursor(&m, "a")
	out := m.right(m.t(), 60, 20)
	rhFits(t, out, 60)
	if n := len(rhLines(out)); n != 20 {
		t.Fatalf("a long detail must not grow the panel: %d lines", n)
	}
}
