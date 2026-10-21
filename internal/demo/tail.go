package demo

// The activity tails of the demo sessions: the lines the GUI scrolls on each
// cat's monitor. Like the timeline, a pure function of the time since the
// start: a busy session writes a line every ~1.5 s (tool calls, results,
// diffs, test output), a blocked one ends with its question and options, a
// done one with a short summary, an idle one at the prompt.

import (
	"time"

	"github.com/aqilputi/nekuroddo/internal/claude"
	"github.com/aqilputi/nekuroddo/internal/core"
)

// tailSlots is how many lines a busy session writes per StateEvery step.
const tailSlots = 10

// line is a tail line before it gets a time.
type line struct{ kind, text string }

func tool(s string) line   { return line{claude.TailTool, s} }
func text(s string) line   { return line{claude.TailText, s} }
func user(s string) line   { return line{claude.TailUser, s} }
func result(s string) line { return line{claude.TailResult, "  ⎿ " + s} }
func sub(s string) line    { return line{claude.TailSub, "◆ " + s} }

// prompt is the empty input line of a session waiting at the prompt.
var prompt = user(">")

// Phases of a step.
const (
	phBusy    = "busy"
	phBlocked = "blocked"
	phDone    = "done"
	phIdle    = "idle"
	phEnd     = "" // a finite script is over: nothing more
)

// script is one session's tail: hist was written before start; from start
// on, step k (StateEvery long) does phase(k). A busy step writes tailSlots
// lines of work (cycled); any other phase writes on[phase] at its start. A
// busy step right after a non-busy one opens with resume (the answer, the
// next request) when set.
type script struct {
	start   time.Time
	histEnd time.Time // when the last hist line was written
	hist    []line
	work    []line
	on      map[string][]line
	resume  line
	phase   func(k int) string
	period  int // phase(k) == phase(k % period); 0 with steps set
	steps   int // a finite script: steps after start (0: none, or periodic)
	salt    int // varies the jitter between sessions
}

// slotAt is when slot j of step k is written, from the step's start:
// 0.5 s + j×1.4 s + a jitter of up to 0.7 s, so consecutive lines are
// 0.7–2.1 s apart.
func (s *script) slotAt(k, j int) time.Duration {
	jit := ((k*7 + j*3 + s.salt) % 8) * 100 * int(time.Millisecond)
	return 500*time.Millisecond + time.Duration(j)*1400*time.Millisecond + time.Duration(jit)
}

func (s *script) stepCount() int {
	if s.steps > 0 {
		return s.steps
	}
	if s.period > 0 {
		return -1 // forever
	}
	return 0
}

// busyBefore counts the busy steps before step k.
func (s *script) busyBefore(k int) int {
	count := func(from, to int) int {
		n := 0
		for i := from; i < to; i++ {
			if s.phase(i) == phBusy {
				n++
			}
		}
		return n
	}
	if s.steps > 0 || s.period == 0 {
		return count(0, k)
	}
	return k/s.period*count(0, s.period) + count(0, k%s.period)
}

// stepLines is what step k writes, up to `upto` into it.
func (s *script) stepLines(k int, upto time.Duration, out []claude.TailLine) []claude.TailLine {
	at := s.start.Add(time.Duration(k) * StateEvery)
	ph := s.phase(k)
	if ph != phBusy { // all written at the step's start
		for _, l := range s.on[ph] {
			out = append(out, claude.TailLine{At: at, Kind: l.kind, Text: l.text})
		}
		return out
	}
	base := s.busyBefore(k) * tailSlots
	for j := range tailSlots {
		off := s.slotAt(k, j)
		if off > upto {
			break
		}
		l := s.work[(base+j)%len(s.work)]
		if j == 0 && k > 0 && s.resume.text != "" && s.phase(k-1) != phBusy {
			l = s.resume
		}
		out = append(out, claude.TailLine{At: at.Add(off), Kind: l.kind, Text: l.text})
	}
	return out
}

// stepLen is how many lines step k writes up to `upto` into it (stepLines
// without the strings).
func (s *script) stepLen(k int, upto time.Duration) int {
	if ph := s.phase(k); ph != phBusy {
		return len(s.on[ph])
	}
	n := 0
	for j := range tailSlots {
		if s.slotAt(k, j) <= upto {
			n++
		}
	}
	return n
}

// current is the step `now` falls in and how far into it (-1 steps before
// the start).
func (s *script) current(now time.Time) (int, time.Duration) {
	e := now.Sub(s.start)
	if e < 0 {
		return -1, 0
	}
	k := int(e / StateEvery)
	if n := s.stepCount(); n >= 0 && k >= n {
		return n - 1, StateEvery // the script is over: its last step in full
	}
	return k, e - time.Duration(k)*StateEvery
}

func (s *script) histLines() []claude.TailLine {
	out := make([]claude.TailLine, len(s.hist))
	for i, l := range s.hist {
		out[i] = claude.TailLine{At: s.histEnd.Add(-time.Duration(len(s.hist)-1-i) * 4 * time.Second),
			Kind: l.kind, Text: l.text}
	}
	return out
}

// tail is the last n lines written by now.
func (s *script) tail(now time.Time, n int) []claude.TailLine {
	k, in := s.current(now)
	var steps [][]claude.TailLine
	got := 0
	for i := k; i >= 0 && got < n; i-- {
		upto := StateEvery
		if i == k {
			upto = in
		}
		ls := s.stepLines(i, upto, nil)
		steps = append(steps, ls)
		got += len(ls)
	}
	out := make([]claude.TailLine, 0, got+len(s.hist))
	if got < n {
		out = append(out, s.histLines()...)
	}
	for i := len(steps) - 1; i >= 0; i-- {
		out = append(out, steps[i]...)
	}
	if len(out) > n {
		out = out[len(out)-n:]
	}
	return out
}

// seq counts every line written by now.
func (s *script) seq(now time.Time) int {
	k, in := s.current(now)
	if k < 0 {
		return len(s.hist)
	}
	n := len(s.hist) + s.stepLen(k, in)
	if s.steps > 0 || s.period == 0 {
		for i := range k {
			n += s.stepLen(i, StateEvery)
		}
		return n
	}
	per := 0
	for i := range s.period {
		per += s.stepLen(i, StateEvery)
	}
	n += k / s.period * per
	for i := range k % s.period {
		n += s.stepLen(i, StateEvery)
	}
	return n
}

// scriptOf is the script of a scripted session (nil for any other id).
func scriptOf(t0 time.Time, id string) *script {
	s := &script{start: t0, histEnd: t0.Add(-time.Second), phase: func(int) string { return phEnd }}
	switch id {
	case IDGateway:
		s.salt, s.period = 1, 2
		s.phase = func(k int) string {
			if gatewayBlocked(k) {
				return phBlocked
			}
			return phBusy
		}
		s.hist = []line{user("> implementa rate limit por tenant no gateway"), text("Vou começar pelo limiter."),
			tool("● Read(internal/ratelimit/limiter.go)"), result("96 lines"),
			user("> roda a suíte de integração antes de subir")}
		s.work = gatewayWork
		s.on = map[string][]line{phBlocked: {tool("● AskUserQuestion(Aplico a migração 0042 no staging?)"),
			text("  ❯ 1. sim, aplicar agora"), text("    2. não, só gerar o SQL"), text("    3. esperar o review")}}
		s.resume = user("> sim, aplicar agora")
	case IDIndexer:
		s.salt, s.period, s.phase = 2, 1, func(int) string { return phBusy }
		s.hist = []line{user("> reescreve o tokenizer do indexador"), sub("general-purpose: medir o baseline"),
			result("Done (8 tool uses · 41.2k tokens · 1m 12s)"), text("Baseline: 212 alocações por documento.")}
		s.work = indexerWork
	case IDDocs:
		s.salt, s.period = 3, 3
		s.phase = func(k int) string { return docsPhase(k) }
		s.hist = []line{user("> atualiza a página de instalação")}
		s.work = docsWork
		s.on = map[string][]line{
			phDone: {text("Pronto: docs/instalacao.md revisado."), text("  1 arquivo alterado, +12 −7 · site gerado sem erros")},
			phIdle: {prompt}}
		s.resume = user("> revisa de novo a página de instalação")
	case IDMigration:
		s.histEnd = t0.Add(-4 * time.Minute)
		s.hist = []line{user("> planeja a migração da tabela users"), tool(`● Grep("user_ref")`), result("14 lines"),
			sub("Explore: achar quem lê user_ref"), result("Done (12 tool uses · 30.8k tokens · 48s)"),
			text("Encontrei 3 serviços que ainda leem user_ref."),
			tool("● AskUserQuestion(Como tratar a coluna legada user_ref?)"),
			text("  ❯ 1. manter e marcar deprecated"), text("    2. remover agora"),
			text("    3. copiar para user_id e remover depois")}
	case IDReport:
		s.histEnd = t0.Add(-12 * time.Minute)
		s.hist = []line{user("> gera o relatório de uso do Q3"), tool("● Bash(python scripts/uso.py --trimestre 3)"),
			result("4 gráficos em reports/q3/ … +2 lines"), tool("● Write(reports/q3.md)"), result("Wrote 214 lines"),
			tool("● Bash(pandoc reports/q3.md -o reports/q3.pdf)"), result("(no output)"),
			tool(`● Bash(gh pr create --title "relatório Q3")`), result("https://example.invalid/pr/418"),
			text("Pronto: relatório em reports/q3.pdf e PR #418 aberto."), text("12 páginas, 4 gráficos.")}
	case IDDotfiles:
		s.histEnd = t0.Add(-25 * time.Minute)
		s.hist = []line{user("> arruma meu tmux.conf"), tool("● Read(.tmux.conf)"), result("64 lines"),
			tool("● Edit(.tmux.conf)"), text("  -  set -g prefix C-b"), text("  +  set -g prefix C-a"),
			tool("● Bash(tmux source-file ~/.tmux.conf)"), result("(no output)"),
			text("Feito. Quer que eu também ajuste o tema do Neovim?"), prompt}
	case IDBlog:
		s.histEnd = t0.Add(-30 * 24 * time.Hour)
		s.hist = []line{user("> migra o blog para Hugo"), tool("● Bash(hugo new site blog)"),
			result("Congratulations! Your new Hugo site was created … +2 lines"),
			text("Migrei 23 posts; os shortcodes antigos viraram partials.")}
	default:
		return nil
	}
	return s
}

// extraScript is a dispatched session: its prompt, DispatchBusy of work,
// then a summary.
func extraScript(x extra) *script {
	busySteps := int(DispatchBusy / StateEvery)
	return &script{start: x.at, histEnd: x.at, hist: []line{user("> " + core.Truncate(x.prompt, claude.TailWidth-2))},
		work: extraWork, steps: busySteps + 1, salt: 4,
		on: map[string][]line{phDone: {text("(demo) pronto — nada foi feito de verdade.")}},
		phase: func(k int) string {
			if k < busySteps {
				return phBusy
			}
			return phDone
		}}
}

// Tail is the last n (≤ core.TailMax) lines of a scripted session's
// activity e after t0; nil for any other id. Pure, like Sessions.
func Tail(t0 time.Time, e time.Duration, id string, n int) []claude.TailLine {
	s := scriptOf(t0, id)
	if s == nil {
		return nil
	}
	return s.tail(t0.Add(max(e, 0)), core.ClampTail(n))
}

// TailSeq counts the lines a scripted session wrote by e after t0 (what
// store.Session.TailSeq is on the real path).
func TailSeq(t0 time.Time, e time.Duration, id string) int {
	s := scriptOf(t0, id)
	if s == nil {
		return 0
	}
	return s.seq(t0.Add(max(e, 0)))
}

// scriptLocked finds the script of a listed session: a scripted one or a
// dispatched one.
func (s *Scenario) scriptLocked(id string) *script {
	if sc := scriptOf(s.t0, id); sc != nil {
		return sc
	}
	for i, x := range s.extras {
		if extraID(i) == id {
			return extraScript(x)
		}
	}
	return nil
}

// Tail is the SessionRepository's: the session's lines at the clock's time.
func (p sessionsPort) Tail(id string, n int) ([]claude.TailLine, error) {
	p.s.mu.Lock()
	defer p.s.mu.Unlock()
	sc := p.s.scriptLocked(id)
	if sc == nil || p.s.removed[id] {
		return nil, nil
	}
	return sc.tail(p.s.clock.Now(), core.ClampTail(n)), nil
}

var gatewayWork = []line{
	tool("● Read(internal/ratelimit/bucket.go)"), result("142 lines"),
	text("O bucket ainda é global; vou indexar por tenant."),
	tool("● Edit(internal/ratelimit/bucket.go)"),
	text(`  -  b := l.buckets["global"]`),
	text("  +  b, ok := l.buckets[tenant]"),
	text("  +  if !ok { b = newBucket(l.cfg.For(tenant)); l.buckets[tenant] = b }"),
	tool("● Bash(go test ./internal/ratelimit/...)"), result("ok    gateway/internal/ratelimit  0.84s"),
	tool(`● Grep("NewLimiter")`), result("6 lines"),
	tool("● Edit(cmd/gateway/main.go)"), result("Updated cmd/gateway/main.go with 2 additions and 1 removal"),
	tool("● Bash(go test ./... -tags integration)"),
	result("--- FAIL: TestBurstPerTenant (0.02s) … +4 lines"),
	text("Um teste falhou: o burst do tenant padrão ficou zerado."),
	tool("● Read(internal/ratelimit/config.go)"), result("58 lines"),
	tool("● Edit(internal/ratelimit/config.go)"),
	text("  +  if c.Burst == 0 { c.Burst = defaultBurst }"),
	tool("● Bash(go test ./... -tags integration)"), result("ok    gateway/...  312 passed in 41.2s"),
	text("Suíte verde. Falta só a migração 0042 no staging."),
}

var indexerWork = []line{
	sub("Explore: mapear os módulos"),
	tool("● Read(internal/tokenize/tokenize.go)"), result("311 lines"),
	text("O tokenizer aloca uma string por token; dá para guardar só os índices."),
	tool("● Edit(internal/tokenize/tokenize.go)"),
	text("  -  out = append(out, string(buf[i:j]))"),
	text("  +  out = append(out, Token{Start: i, End: j})"),
	tool("● Bash(go test -run Tokenize -bench . ./internal/tokenize/)"),
	result("BenchmarkTokenize-16   41823   28411 ns/op   0 allocs/op … +2 lines"),
	text("Zero alocações por token agora (antes: 212)."),
	sub("general-purpose: escrever os testes do tokenizer"),
	tool("● Bash(go vet ./...)"), result("(no output)"),
	tool(`● Grep("tokenize.Split")`), result("9 lines"),
	tool("● Edit(internal/index/build.go)"), result("Updated internal/index/build.go with 4 additions and 4 removals"),
	tool("● Bash(go test ./...)"), result("ok    indexer/internal/index  1.92s … +6 lines"),
}

var docsWork = []line{
	tool("● Read(docs/instalacao.md)"), result("87 lines"),
	text("A seção de Linux ainda manda baixar o binário à mão."),
	tool("● Edit(docs/instalacao.md)"),
	text("  -  curl -LO https://example.invalid/nk.tar.gz"),
	text("  +  go install example.invalid/nk@latest"),
	tool("● Bash(hugo --minify)"), result("Total in 412 ms … +3 lines"),
	tool("● WebFetch(https://example.invalid/docs/instalacao)"), result("200 OK"),
	tool(`● Grep("tar.gz")`), result("No matches found"),
}

var extraWork = []line{
	tool("● Read(README.md)"), result("40 lines"),
	text("Entendi o pedido; vou olhar o código antes de mexer."),
	tool(`● Grep("TODO")`), result("3 lines"),
	tool("● Edit(main.go)"), text("  +  // (demo) nada muda de verdade"),
	tool("● Bash(go test ./...)"), result("ok    demo  0.31s"),
}
