package demo

import (
	"time"

	"github.com/aqilputi/nekuroddo/internal/claude"
	"github.com/aqilputi/nekuroddo/internal/store"
)

// The script's pace.
const (
	StateEvery = 15 * time.Second // the cycling sessions change state
	MailEvery  = 20 * time.Second // a ✉ goes around #deploy-rc
)

// Contexts of the demo world.
const (
	CtxAcme    = "acme"
	CtxPessoal = "pessoal"
	CtxOSS     = "oss"
)

// GroupTag is the demo's group (shown as #deploy-rc).
const GroupTag = "deploy-rc"

// Session ids of the scripted sessions (stable, for tests and the GUI's).
const (
	IDGateway   = "d3e0a001-0000-4000-8000-000000000001" // acme: busy ⇄ blocked every 15 s
	IDMigration = "d3e0a002-0000-4000-8000-000000000002" // acme: blocked on a question with options
	IDReport    = "d3e0a003-0000-4000-8000-000000000003" // acme: done, returned to you
	IDDotfiles  = "d3e0a004-0000-4000-8000-000000000004" // pessoal: interactive, idle at the prompt
	IDBlog      = "d3e0a005-0000-4000-8000-000000000005" // pessoal: stale history
	IDIndexer   = "d3e0a006-0000-4000-8000-000000000006" // oss: busy with 2 subagents running
	IDDocs      = "d3e0a007-0000-4000-8000-000000000007" // oss: busy → done → idle every 15 s
)

// GroupMembers are the #deploy-rc sessions, in the order the ✉ goes round.
var GroupMembers = []string{IDGateway, IDMigration, IDIndexer, IDDocs}

// Step is which StateEvery slice of the timeline e falls in (0 before the
// start).
func Step(e time.Duration) int {
	if e < 0 {
		return 0
	}
	return int(e / StateEvery)
}

// gatewayBlocked: the gateway asks on odd steps.
func gatewayBlocked(k int) bool { return k%2 == 1 }

// docsPhase cycles busy → done → idle.
func docsPhase(k int) string { return [...]string{"busy", "done", "idle"}[k%3] }

// Sessions is the scripted world e after t0, before anything the user did
// (renames, archive, …: see Scenario). Pure: the same (t0, e) is always the
// same world.
func Sessions(t0 time.Time, e time.Duration) []store.Session {
	e = max(e, 0)
	k := Step(e)
	now := t0.Add(e)
	stepAt := t0.Add(time.Duration(k) * StateEvery)
	grow := func(base float64, n int) float64 { return base + 0.03*float64(n) }

	gw := store.Session{SessionID: IDGateway, ShortID: "dmgatewy", Name: "api-gateway", Cwd: "/demo/acme/gateway",
		Context: CtxAcme, Kind: "background", Started: t0.Add(-2 * time.Hour), Model: "claude-sonnet-5",
		FirstUser: "implementa rate limit por tenant no gateway", Cost: grow(2.10, (k+1)/2), Tokens: 182_000,
		LastUser: "roda a suíte de integração antes de subir", LastTool: "Bash"}
	if gatewayBlocked(k) {
		gw.State, gw.Attention, gw.LastAt = "blocked", store.AttnBlocked, stepAt
		gw.Needs, gw.WaitingTool = "aprovar a migração 0042 no staging", "AskUserQuestion"
		gw.Questions = []claude.Question{{Question: "Aplico a migração 0042 no staging?", Options: []claude.Option{
			{Label: "sim, aplicar agora", Description: "roda o migrate up no staging"},
			{Label: "não, só gerar o SQL", Description: "deixa o arquivo em migrations/ para revisão"},
			{Label: "esperar o review", Description: "pausa até o PR ter aprovação"}}}}
	} else {
		gw.State, gw.LastAt = "busy", now
		gw.Detail, gw.LastText = "rodando a suíte de integração (312 testes)", "Rodando go test ./... -tags integration"
	}

	mig := store.Session{SessionID: IDMigration, ShortID: "dmmigrat", Name: "migração do banco", Cwd: "/demo/acme/db",
		Context: CtxAcme, Kind: "background", State: "blocked", Attention: store.AttnBlocked,
		Started: t0.Add(-3 * time.Hour), LastAt: t0.Add(-4 * time.Minute), Model: "claude-opus-5", Cost: 3.40,
		FirstUser: "planeja a migração da tabela users", Needs: "decidir o destino da coluna legada",
		WaitingTool: "AskUserQuestion", LastText: "Encontrei 3 serviços que ainda leem user_ref.",
		Questions: []claude.Question{{Question: "Como tratar a coluna legada user_ref?", Options: []claude.Option{
			{Label: "manter e marcar deprecated", Description: "nenhum serviço quebra"},
			{Label: "remover agora", Description: "3 serviços precisam de deploy junto"},
			{Label: "copiar para user_id e remover depois", Description: "duas migrações"}}}}}

	rep := store.Session{SessionID: IDReport, ShortID: "dmreport", Name: "relatório Q3", Cwd: "/demo/acme/relatorios",
		Context: CtxAcme, Kind: "background", State: "done", Attention: store.AttnReturned,
		Started: t0.Add(-5 * time.Hour), LastAt: t0.Add(-12 * time.Minute), Model: "claude-sonnet-5", Cost: 1.25,
		FirstUser: "gera o relatório de uso do Q3", Result: "Relatório gerado: 12 páginas, 4 gráficos, PR aberto.",
		LastText: "Pronto: relatório em reports/q3.pdf e PR #418 aberto.",
		Links:    []claude.Child{{ID: "418", Kind: "pr", Title: "relatório Q3", Href: "https://example.invalid/pr/418"}}}

	dot := store.Session{SessionID: IDDotfiles, Name: "dotfiles", Cwd: "/demo/pessoal/dotfiles", Context: CtxPessoal,
		Kind: "interactive", State: "idle", Attention: store.AttnIdle, PID: 4242, Started: t0.Add(-50 * time.Minute),
		LastAt: t0.Add(-25 * time.Minute), Model: "claude-haiku-5", Cost: 0.18, FirstUser: "arruma meu tmux.conf",
		WaitingFor: "input needed", LastText: "Feito. Quer que eu também ajuste o tema do Neovim?"}

	blog := store.Session{SessionID: IDBlog, Name: "blog antigo", Cwd: "/demo/pessoal/blog", Context: CtxPessoal,
		Kind: "history", State: "history", Stale: true, Started: t0.Add(-31 * 24 * time.Hour),
		LastAt: t0.Add(-30 * 24 * time.Hour), Model: "claude-sonnet-5", Cost: 0.72, FirstUser: "migra o blog para Hugo"}

	idx := store.Session{SessionID: IDIndexer, ShortID: "dmindexr", Name: "indexador", Cwd: "/demo/oss/indexer",
		Context: CtxOSS, Kind: "background", State: "busy", Started: t0.Add(-90 * time.Minute), LastAt: now,
		Model: "claude-opus-5", Cost: grow(1.80, k), FirstUser: "reescreve o tokenizer do indexador",
		Detail: "2 subagents trabalhando", LastTool: "Task",
		Subs: []store.Sub{
			{ID: "agent-idx-a", Description: "mapear os módulos", Type: "Explore", Model: "claude-haiku-5",
				Cost: grow(0.20, k), LastAt: now, LastText: "Lendo internal/tokenize/…"},
			{ID: "agent-idx-b", Description: "escrever os testes do tokenizer", Type: "general-purpose",
				Model: "claude-sonnet-5", Cost: grow(0.35, k), LastAt: now, LastText: "Escrevendo tokenize_test.go"},
			{ID: "agent-idx-c", Description: "medir o baseline", Type: "general-purpose", Model: "claude-haiku-5",
				Cost: 0.08, LastAt: t0.Add(-10 * time.Minute), Done: true}}}

	docs := store.Session{SessionID: IDDocs, ShortID: "dmdocsit", Name: "site de docs", Cwd: "/demo/oss/docs",
		Context: CtxOSS, Kind: "background", Started: t0.Add(-40 * time.Minute), Model: "claude-sonnet-5",
		Cost: grow(0.60, k/3), FirstUser: "atualiza a página de instalação"}
	switch docsPhase(k) {
	case "busy":
		docs.State, docs.LastAt, docs.Detail = "busy", now, "reescrevendo docs/instalacao.md"
	case "done":
		docs.State, docs.Attention, docs.LastAt = "done", store.AttnReturned, stepAt
		docs.Result, docs.LastText = "Página de instalação atualizada.", "Pronto: docs/instalacao.md revisado."
	default:
		docs.State, docs.LastAt = "idle", stepAt
	}

	out := []store.Session{gw, mig, rep, dot, blog, idx, docs}
	for i := range out {
		out[i].WindowCost = out[i].Cost * 0.6
		out[i].CostToday = out[i].Cost * 0.4
	}
	return out
}

// Mail is the n-th scripted message between #deploy-rc members.
type Mail struct {
	N        int
	At       time.Duration // after the start
	From, To string        // session ids
	FromName string        // the sender's name (its SendMessage address)
	Text     string
}

var mailTexts = []string{
	"[#" + GroupTag + "] build 1.42.0-rc1 verde, pode seguir",
	"[#" + GroupTag + "] migração travada esperando aprovação",
	"[#" + GroupTag + "] índice reconstruído, latência ok",
	"[#" + GroupTag + "] changelog do rc atualizado",
	"[#" + GroupTag + "] alguém viu o alerta de memória do worker?",
}

var memberNames = map[string]string{IDGateway: "api-gateway", IDMigration: "migração do banco",
	IDIndexer: "indexador", IDDocs: "site de docs"}

// MailN is message n (n ≥ 1), sent at n×MailEvery: member n-1 writes to
// member n, round the group.
func MailN(n int) Mail {
	from, to := GroupMembers[(n-1)%len(GroupMembers)], GroupMembers[n%len(GroupMembers)]
	return Mail{N: n, At: time.Duration(n) * MailEvery, From: from, To: to, FromName: memberNames[from],
		Text: mailTexts[(n-1)%len(mailTexts)]}
}

// MailsBetween lists the messages sent in (a, b].
func MailsBetween(a, b time.Duration) []Mail {
	var out []Mail
	for n := max(int(a/MailEvery)+1, 1); time.Duration(n)*MailEvery <= b; n++ {
		if time.Duration(n)*MailEvery > a {
			out = append(out, MailN(n))
		}
	}
	return out
}

// Alert is a "needs you" notification the script raises.
type Alert struct {
	At        time.Duration
	SessionID string
	Message   string
}

// AlertsBetween lists the notifications raised in (a, b]: the gateway asks
// for permission each time it blocks.
func AlertsBetween(a, b time.Duration) []Alert {
	var out []Alert
	for k := max(Step(a), 1); time.Duration(k)*StateEvery <= b; k++ {
		at := time.Duration(k) * StateEvery
		if gatewayBlocked(k) && at > a {
			out = append(out, Alert{At: at, SessionID: IDGateway, Message: "api-gateway quer aplicar a migração 0042"})
		}
	}
	return out
}

// UsagePct is the 5-hour window's use at step k: around 60%.
func UsagePct(k int) int {
	return 60 + [...]int{0, 1, 2, 1, 0, -1, -2, -1}[max(k, 0)%8]
}
