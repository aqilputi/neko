package claude

import (
	"math"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"
	"unicode/utf8"
)

// copyFixture copies testdata/<name>.jsonl and its <name>/ folder into a
// temp dir: transcripts are tracked by path (subagent offsets are global),
// so each test needs its own copy.
func copyFixture(t *testing.T, name string) string {
	t.Helper()
	dst := t.TempDir()
	src := "testdata"
	err := filepath.Walk(src, func(p string, info os.FileInfo, err error) error {
		if err != nil {
			return err
		}
		rel, _ := filepath.Rel(src, p)
		if rel != name+".jsonl" && rel != name && !strings.HasPrefix(rel, name+string(filepath.Separator)) {
			return nil
		}
		if info.IsDir() {
			return os.MkdirAll(filepath.Join(dst, rel), 0o755)
		}
		b, err := os.ReadFile(p)
		if err != nil {
			return err
		}
		return os.WriteFile(filepath.Join(dst, rel), b, 0o644)
	})
	if err != nil {
		t.Fatal(err)
	}
	return filepath.Join(dst, name+".jsonl")
}

func near(a, b float64) bool { return math.Abs(a-b) < 1e-12 }

func appendLine(t *testing.T, p, line string) {
	t.Helper()
	f, err := os.OpenFile(p, os.O_APPEND|os.O_WRONLY, 0)
	if err != nil {
		t.Fatal(err)
	}
	defer f.Close()
	if _, err := f.WriteString(line + "\n"); err != nil {
		t.Fatal(err)
	}
}

func TestTranscriptFixture(t *testing.T) {
	p := copyFixture(t, "session")
	tr := NewTranscript(p)
	if tr.SessionID != "session" {
		t.Fatalf("SessionID = %q", tr.SessionID)
	}
	if err := tr.Refresh(flat); err != nil {
		t.Fatal(err)
	}
	ts := func(s string) time.Time { return parseTime("2026-10-15T" + s + "Z") }

	checks := []struct {
		name      string
		got, want any
	}{
		{"Cwd (first seen wins)", tr.Cwd, "/home/u/repo"},
		{"Title (empty ai-title ignored)", tr.Title, "Bug do login"},
		{"Model (<synthetic> ignored)", tr.Model, "claude-opus-5-5"},
		{"Requests (requestId, else message id)", tr.Requests, 2},
		{"Tokens", tr.Tokens, int64(60)},
		{"FirstUser (skill preamble and <tags> skipped, spaces folded)", tr.FirstUser, "Corrija o bug do login"},
		{"LastUser", tr.LastUser, "Corrija o bug do login"},
		{"LastText (blank text ignored)", tr.LastText, "Vou olhar."},
		{"LastTool", tr.LastTool, "Read"},
		{"WaitingTool (tu1 answered, tu2 not)", tr.WaitingTool, "Read"},
		{"Started", tr.Started, ts("10:00:00")},
		{"LastAt (bad timestamp ignored)", tr.LastAt, ts("10:00:09")},
		{"Ended", tr.Ended, false},
		{"subagents", len(tr.Subagents), 2},
	}
	for _, c := range checks {
		if c.got != c.want {
			t.Errorf("%s = %#v, want %#v", c.name, c.got, c.want)
		}
	}
	// 100 in × 4 + 50 out × 20 + 10 out × 20, per MTok.
	if !near(tr.Cost, 0.0016) {
		t.Errorf("Cost = %v", tr.Cost)
	}

	// Sorted by last activity: b2 (10:00:40) before a1 (10:01:05).
	b2, a1 := tr.Subagents[0], tr.Subagents[1]
	if b2.ID != "b2" || b2.Description != "Revisar o diff" || b2.Type != "general-purpose" ||
		b2.Model != "claude-sonnet-4-5" || !near(b2.Cost, 0.008) || b2.LastText != "feito" || !b2.Done {
		t.Errorf("b2 = %+v", *b2)
	}
	// a1 has no description in its meta: it is named after its task. Its
	// duplicated request is counted once, even though b2 shares the id.
	if a1.ID != "a1" || a1.Description != "Procure os testes de login" || a1.Type != "Explore" ||
		a1.Model != "claude-haiku-4-5" || !near(a1.Cost, 0.004) || a1.LastText != "Achei 3 testes." ||
		!a1.LastAt.Equal(ts("10:01:05")) || !a1.Done {
		t.Errorf("a1 = %+v", *a1)
	}

	if !near(tr.TotalCost(), 0.0136) {
		t.Errorf("TotalCost = %v", tr.TotalCost())
	}
	var byDay float64
	for _, v := range tr.CostByDay {
		byDay += v
	}
	if !near(byDay, 0.0136) {
		t.Errorf("CostByDay = %v", tr.CostByDay)
	}
	if got := tr.CostSince(ts("09:59:00")); !near(got, 0.0136) {
		t.Errorf("CostSince(before) = %v", got)
	}
	if got := tr.CostSince(ts("10:05:00")); got != 0 {
		t.Errorf("CostSince(after) = %v", got)
	}

	// Nothing new: a second refresh changes nothing.
	tr.Refresh(flat)
	if !near(tr.TotalCost(), 0.0136) || tr.Requests != 2 {
		t.Fatalf("refresh without changes: cost=%v requests=%d", tr.TotalCost(), tr.Requests)
	}

	// New activity in a subagent is picked up incrementally.
	sub := filepath.Join(strings.TrimSuffix(p, ".jsonl"), "subagents", "agent-a1.jsonl")
	appendLine(t, sub, `{"type":"assistant","requestId":"s2","timestamp":"2026-10-15T10:02:00Z","message":{"model":"claude-haiku-4-5","usage":{"input_tokens":500},"content":[{"type":"text","text":"mais um"}]}}`)
	tr.Refresh(flat)
	if !near(a1.Cost, 0.006) || a1.LastText != "mais um" {
		t.Fatalf("a1 after append = %+v", *a1)
	}

	// A user reply clears the pending tool.
	appendLine(t, p, `{"type":"user","timestamp":"2026-10-15T10:03:00Z","message":{"role":"user","content":"continua"}}`)
	tr.Refresh(flat)
	if tr.WaitingTool != "" || tr.LastUser != "continua" || tr.FirstUser != "Corrija o bug do login" {
		t.Fatalf("after reply: waiting=%q last=%q first=%q", tr.WaitingTool, tr.LastUser, tr.FirstUser)
	}
}

func TestTranscriptRewritten(t *testing.T) {
	p := filepath.Join(t.TempDir(), "s.jsonl")
	rec := func(req string) string {
		return `{"type":"assistant","requestId":"` + req + `","timestamp":"2026-10-15T10:00:00Z","message":{"model":"m","usage":{"output_tokens":1000000},"content":[]}}` + "\n"
	}
	os.WriteFile(p, []byte(rec("r1")+rec("r2")), 0o644)
	tr := NewTranscript(p)
	tr.Refresh(flat)
	if tr.Requests != 2 {
		t.Fatalf("requests = %d", tr.Requests)
	}
	// The file shrinks (rewritten): it is read again from the start, but
	// requests already counted are not counted twice.
	os.WriteFile(p, []byte(rec("r1")), 0o644)
	tr.Refresh(flat)
	if tr.Requests != 2 || !near(tr.Cost, 40) {
		t.Fatalf("after rewrite: requests=%d cost=%v", tr.Requests, tr.Cost)
	}
	appendLine(t, p, strings.TrimSuffix(rec("r3"), "\n"))
	tr.Refresh(flat)
	if tr.Requests != 3 || !near(tr.Cost, 60) {
		t.Fatalf("after append: requests=%d cost=%v", tr.Requests, tr.Cost)
	}
}

func TestTranscriptMissingFile(t *testing.T) {
	tr := NewTranscript(filepath.Join(t.TempDir(), "gone.jsonl"))
	if err := tr.Refresh(flat); !os.IsNotExist(err) {
		t.Fatalf("err = %v", err)
	}
}

func TestTranscriptWaitingAndHints(t *testing.T) {
	cases := []struct {
		name, input, want string
	}{
		{"description wins over command", `{"description":"roda testes","command":"go test"}`, "Bash: roda testes"},
		{"command", `{"command":"ls   -la\n/tmp"}`, "Bash: ls -la /tmp"},
		{"file_path", `{"file_path":"/a/b.go"}`, "Bash: /a/b.go"},
		{"empty string skipped", `{"description":"","pattern":"TODO"}`, "Bash: TODO"},
		{"non-string ignored", `{"command":42}`, "Bash"},
		{"long input cut at 60 runes", `{"prompt":"` + strings.Repeat("é", 100) + `"}`, "Bash: " + strings.Repeat("é", 59) + "…"},
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			p := filepath.Join(t.TempDir(), "s.jsonl")
			os.WriteFile(p, []byte(`{"type":"assistant","timestamp":"2026-10-15T10:00:00Z","message":{"content":[{"type":"tool_use","id":"t1","name":"Bash","input":`+c.input+`}]}}`+"\n"), 0o644)
			tr := NewTranscript(p)
			tr.Refresh(flat)
			if tr.LastTool != c.want || tr.WaitingTool != c.want {
				t.Fatalf("LastTool=%q WaitingTool=%q, want %q", tr.LastTool, tr.WaitingTool, c.want)
			}
		})
	}
}

func TestTranscriptParallelTools(t *testing.T) {
	// Two tool_use in flight: answering one keeps waiting; a later text clears it.
	p := filepath.Join(t.TempDir(), "s.jsonl")
	os.WriteFile(p, []byte(
		`{"type":"assistant","message":{"content":[{"type":"tool_use","id":"a","name":"Read"},{"type":"tool_use","id":"b","name":"Grep"}]}}`+"\n"+
			`{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"a"}]}}`+"\n"), 0o644)
	tr := NewTranscript(p)
	tr.Refresh(flat)
	if tr.WaitingTool != "Grep" {
		t.Fatalf("WaitingTool = %q", tr.WaitingTool)
	}
	appendLine(t, p, `{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"b"}]}}`)
	tr.Refresh(flat)
	if tr.WaitingTool != "" {
		t.Fatalf("WaitingTool after all results = %q", tr.WaitingTool)
	}
	appendLine(t, p, `{"type":"assistant","message":{"content":[{"type":"tool_use","id":"c","name":"Edit"},{"type":"text","text":"pronto"}]}}`)
	tr.Refresh(flat)
	if tr.WaitingTool != "" || tr.LastText != "pronto" {
		t.Fatalf("text after tool_use: waiting=%q last=%q", tr.WaitingTool, tr.LastText)
	}
}

func TestTranscriptLongTextTruncated(t *testing.T) {
	p := filepath.Join(t.TempDir(), "s.jsonl")
	os.WriteFile(p, []byte(`{"type":"assistant","message":{"content":[{"type":"text","text":"`+strings.Repeat("ã ", 500)+`"}]}}`+"\n"), 0o644)
	tr := NewTranscript(p)
	tr.Refresh(flat)
	if n := utf8.RuneCountInString(tr.LastText); n != 400 || !strings.HasSuffix(tr.LastText, "…") {
		t.Fatalf("LastText has %d runes: %q", n, tr.LastText[len(tr.LastText)-10:])
	}
}

func TestFirstUserSkipsNoise(t *testing.T) {
	for _, noise := range []string{
		"Base directory for this skill: /x",
		"[Request interrupted by user]",
		"Caveat: the messages below were generated…",
		"[structured-output] …",
		"<local-command-stdout>",
	} {
		p := filepath.Join(t.TempDir(), "s.jsonl")
		os.WriteFile(p, []byte(`{"type":"user","message":{"content":[{"type":"text","text":"`+noise+`"},{"type":"text","text":"pedido real"}]}}`+"\n"), 0o644)
		tr := NewTranscript(p)
		tr.Refresh(flat)
		if tr.FirstUser != "pedido real" {
			t.Errorf("noise %q: FirstUser = %q", noise, tr.FirstUser)
		}
	}
}

func TestUsageCost(t *testing.T) {
	cases := []struct {
		name string
		u    Usage
		want float64
	}{
		{"zero", Usage{}, 0},
		{"input and output", Usage{Input: 1e6, Output: 1e6}, 24},
		{"cache write with no split is priced as 5m", Usage{CacheWrite: 1e6}, 5},
		{"cache read", Usage{CacheRead: 1e6}, 0.2},
		{"split overrides the total", func() Usage {
			u := Usage{CacheWrite: 9e6}
			u.CacheCreated.W5m, u.CacheCreated.W1h = 1e6, 1e6
			return u
		}(), 5 + 8},
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			var gotModel string
			got := c.u.Cost("claude-x", func(m string) (float64, float64, float64, float64) {
				gotModel = m
				return flat(m)
			})
			if !near(got, c.want) || gotModel != "claude-x" {
				t.Fatalf("cost = %v (model %q), want %v", got, gotModel, c.want)
			}
		})
	}
}

func TestPromptName(t *testing.T) {
	cases := []struct{ in, want string }{
		{"", ""},
		{"  faz   x  ", "faz x"},
		{"primeira linha\nsegunda linha", "primeira linha"},
		{strings.Repeat("a", 48), strings.Repeat("a", 48)},
		// No space past the middle: cut hard at 48 runes.
		{strings.Repeat("a", 60), strings.Repeat("a", 48) + "…"},
		// Cut at the last word boundary, trailing punctuation dropped.
		{"Revise o módulo de faturamento, escreva testes para todos os casos", "Revise o módulo de faturamento, escreva testes…"},
		{"Revise o modulo de faturamento e escreva teste; para todos os casos", "Revise o modulo de faturamento e escreva teste…"},
		{strings.Repeat("ção ", 20), "ção ção ção ção ção ção ção ção ção ção ção ção…"},
	}
	for _, c := range cases {
		got := PromptName(c.in)
		if got != c.want {
			t.Errorf("PromptName(%q) = %q, want %q", c.in, got, c.want)
		}
		if utf8.RuneCountInString(got) > 49 {
			t.Errorf("PromptName(%q) too long: %d runes", c.in, utf8.RuneCountInString(got))
		}
	}
}

func TestNekoInternal(t *testing.T) {
	cases := []struct {
		in   string
		want bool
	}{
		{"Chame createJiraIssue exatamente uma vez com estes campos", true},
		{"Use searchJiraIssuesUsingJql no site x.atlassian.net", true},
		{"Chame o Silmar", false},
		{"Por favor, Chame createJiraIssue exatamente uma vez com x", false},
		{"", false},
	}
	for _, c := range cases {
		if got := NekoInternal(c.in); got != c.want {
			t.Errorf("NekoInternal(%q) = %v", c.in, got)
		}
	}
}

func TestSubagentActiveAndNoMeta(t *testing.T) {
	p := filepath.Join(t.TempDir(), "s.jsonl")
	os.WriteFile(p, nil, 0o644)
	sub := filepath.Join(strings.TrimSuffix(p, ".jsonl"), "subagents")
	os.MkdirAll(sub, 0o755)
	now := time.Now().UTC().Format(time.RFC3339Nano)
	os.WriteFile(filepath.Join(sub, "agent-x.jsonl"), []byte(
		`{"type":"user","timestamp":"`+now+`","message":{"content":[{"type":"text","text":"<system>"}]}}`+"\n"+
			`{"type":"user","timestamp":"`+now+`","message":{"content":[{"type":"text","text":"Ache o bug"}]}}`+"\n"), 0o644)
	os.WriteFile(filepath.Join(sub, "agent-x.meta.json"), []byte("{broken"), 0o644)
	tr := NewTranscript(p)
	if err := tr.Refresh(flat); err != nil {
		t.Fatal(err)
	}
	if len(tr.Subagents) != 1 {
		t.Fatalf("subagents = %d", len(tr.Subagents))
	}
	s := tr.Subagents[0]
	if s.Done || s.Description != "Ache o bug" || s.Type != "" {
		t.Fatalf("subagent = %+v", *s)
	}
}

func TestSubagentCostSurvivesNewTranscript(t *testing.T) {
	p := copyFixture(t, "session")
	NewTranscript(p).Refresh(flat)
	tr := NewTranscript(p)
	tr.Refresh(flat)
	if !near(tr.TotalCost(), 0.0136) {
		t.Fatalf("TotalCost of a fresh Transcript = %v, want 0.0136", tr.TotalCost())
	}
}
