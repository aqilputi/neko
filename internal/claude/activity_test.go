package claude

import (
	"fmt"
	"strings"
	"sync"
	"testing"
	"unicode/utf8"
)

func TestActivityFixture(t *testing.T) {
	p := copyFixture(t, "activity")
	tr := NewTranscript(p)
	if err := tr.Refresh(flat); err != nil {
		t.Fatal(err)
	}
	ts := func(s string) string { return "10:00:" + s }
	want := []struct{ at, kind, text string }{
		{ts("02"), TailUser, "> corrija o bug do login"},
		{ts("02"), TailUser, "  e roda os testes"},
		{ts("03"), TailText, "Vou olhar."},
		{ts("03"), TailText, "  if err != nil {"},
		{ts("03"), TailTool, "● Bash(go test ./...)"},
		{ts("04"), TailResult, "  ⎿ ok    pkg/a  0.1s … +1 line"},
		{ts("05"), TailTool, "● Read(internal/app/app.go)"},
		{ts("06"), TailResult, "  ⎿ 3 lines"},
		{ts("07"), TailTool, `● Grep("func Login")`},
		{ts("07"), TailTool, "● Edit(/elsewhere/x.go)"},
		{ts("08"), TailResult, "  ⎿ internal/auth/login.go"},
		{ts("08"), TailResult, "  ⎿ Error: String to replace not found in file. … +1 line"},
		{ts("09"), TailSub, "◆ Explore: Procure os testes"},
		{ts("09"), TailSub, "◆ Task: Revisar o diff"},
		{ts("09"), TailTool, `● WebSearch("go 1.26 release")`},
		{ts("09"), TailTool, "● TodoWrite(2 todos)"},
		{ts("09"), TailTool, "● Mystery"},
		{ts("09"), TailTool, "● WebFetch(https://go.dev)"},
		{ts("09"), TailTool, "● Glob(**/*.go)"},
		{ts("09"), TailTool, "● mcp__x__lookup(quem é o dono?)"},
		{ts("10"), TailResult, "  ⎿ (no output)"},
		{ts("10"), TailResult, "  ⎿ 2 lines"},
		{ts("10"), TailResult, "  ⎿ orphan"},
		{ts("10"), TailUser, "> agora o logout"},
		{ts("10"), TailTool, "● AskUserQuestion(Aplico a migração?)"}, // bad timestamp: the last one seen
	}
	got := tr.Activity(0)
	if len(got) != len(want) {
		for _, l := range got {
			t.Logf("%s %-6s %s", l.At.Format("15:04:05"), l.Kind, l.Text)
		}
		t.Fatalf("%d lines, want %d", len(got), len(want))
	}
	for i, w := range want {
		g := got[i]
		if g.At.Format("15:04:05") != w.at || g.Kind != w.kind || g.Text != w.text {
			t.Errorf("line %d = %s %s %q, want %s %s %q", i, g.At.Format("15:04:05"), g.Kind, g.Text, w.at, w.kind, w.text)
		}
	}
	if tr.ActivitySeq() != len(want) {
		t.Errorf("ActivitySeq = %d", tr.ActivitySeq())
	}
	if l := tr.Activity(2); len(l) != 2 || l[1].Text != want[len(want)-1].text {
		t.Errorf("Activity(2) = %+v", l)
	}
	if l := tr.Activity(-1); len(l) != len(want) {
		t.Errorf("Activity(-1) = %d lines", len(l))
	}
}

// Lines are parsed as the transcript grows, never from the start again.
func TestActivityIncremental(t *testing.T) {
	p := copyFixture(t, "activity")
	tr := NewTranscript(p)
	_ = tr.Refresh(flat)
	n := tr.ActivitySeq()
	_ = tr.Refresh(flat) // nothing new
	if tr.ActivitySeq() != n {
		t.Fatalf("seq %d → %d with nothing appended", n, tr.ActivitySeq())
	}
	appendLine(t, p, `{"type":"assistant","timestamp":"2026-10-15T10:01:00Z","message":{"content":[{"type":"text","text":"feito"}]}}`)
	_ = tr.Refresh(flat)
	got := tr.Activity(1)
	if tr.ActivitySeq() != n+1 || len(got) != 1 || got[0].Text != "feito" || got[0].Kind != TailText {
		t.Fatalf("seq %d, last %+v", tr.ActivitySeq(), got)
	}
}

// The ring keeps the last TailKeep lines however long the session gets.
func TestActivityBounded(t *testing.T) {
	p := copyFixture(t, "activity")
	tr := NewTranscript(p)
	_ = tr.Refresh(flat)
	base := tr.ActivitySeq()
	const extra = 3*TailKeep + 7
	var b strings.Builder
	for i := range extra {
		fmt.Fprintf(&b, `{"type":"assistant","timestamp":"2026-10-15T11:00:00Z","message":{"content":[{"type":"text","text":"linha %d"}]}}`+"\n", i)
	}
	appendLine(t, p, strings.TrimSuffix(b.String(), "\n"))
	_ = tr.Refresh(flat)
	got := tr.Activity(TailKeep + 50)
	if len(got) != TailKeep {
		t.Fatalf("%d lines kept, want %d", len(got), TailKeep)
	}
	if got[len(got)-1].Text != fmt.Sprintf("linha %d", extra-1) || got[0].Text != fmt.Sprintf("linha %d", extra-TailKeep) {
		t.Errorf("window %q … %q", got[0].Text, got[len(got)-1].Text)
	}
	if tr.ActivitySeq() != base+extra {
		t.Errorf("seq = %d, want %d", tr.ActivitySeq(), base+extra)
	}
	if cap(tr.act) > 2*TailKeep+1 {
		t.Errorf("ring grew to cap %d", cap(tr.act))
	}
}

func TestActivityTextShaping(t *testing.T) {
	long := strings.Repeat("palavra ", 30) // 240 runes
	for _, tc := range []struct {
		name      string
		text      string
		max       int
		wantLines int
		check     func([]string) string
	}{
		{"wraps at a word", long, 40, 3, func(ls []string) string {
			for _, l := range ls {
				if utf8.RuneCountInString(l) > TailWidth || strings.HasSuffix(l, " ") {
					return "bad line " + l
				}
			}
			return ""
		}},
		{"hard-cuts a word with no space", strings.Repeat("x", 250), 40, 3, func(ls []string) string {
			if utf8.RuneCountInString(ls[0]) != TailWidth || utf8.RuneCountInString(ls[2]) != 50 {
				return fmt.Sprintf("%d / %d", len(ls[0]), len(ls[2]))
			}
			return ""
		}},
		{"capped with a count", strings.Repeat("a\n", 10), 4, 4, func(ls []string) string {
			if ls[3] != "… +7 lines" {
				return ls[3]
			}
			return ""
		}},
		{"blank lines and trailing space dropped, indent kept", "  a  \n\n\t\tb\n", 9, 2, func(ls []string) string {
			if ls[0] != "  a" || ls[1] != "    b" {
				return fmt.Sprintf("%q", ls)
			}
			return ""
		}},
		{"empty", " \n ", 9, 0, func([]string) string { return "" }},
	} {
		t.Run(tc.name, func(t *testing.T) {
			ls := shapeText(tc.text, TailWidth, tc.max)
			if len(ls) != tc.wantLines {
				t.Fatalf("%d lines: %q", len(ls), ls)
			}
			if msg := tc.check(ls); msg != "" {
				t.Error(msg)
			}
		})
	}
	if got := clip(strings.Repeat("é", 120), 10); got != strings.Repeat("é", 9)+"…" {
		t.Errorf("clip = %q", got)
	}
	if got := clip("curto", 10); got != "curto" {
		t.Errorf("clip = %q", got)
	}
}

// Tool lines are one terminal row: long arguments are cut.
func TestActivityToolLineWidth(t *testing.T) {
	cmd := strings.Repeat("x", 300)
	l := toolLine("/r", block{Name: "Bash", Input: []byte(`{"command":"` + cmd + `"}`)})
	if utf8.RuneCountInString(l) != TailWidth || !strings.HasPrefix(l, "● Bash(xxx") || !strings.HasSuffix(l, "…") {
		t.Errorf("toolLine = %q", l)
	}
	if l := toolLine("/r", block{Name: "Read", Input: []byte(`{"file_path":"/r"}`)}); l != "● Read(/r)" {
		t.Errorf("cwd itself: %q", l)
	}
	if l := toolLine("", block{Name: "Read", Input: []byte(`{"file_path":"/a/b"}`)}); l != "● Read(/a/b)" {
		t.Errorf("no cwd: %q", l)
	}
	if l := toolLine("/r", block{Name: "NotebookEdit", Input: []byte(`{"notebook_path":"/r/n.ipynb"}`)}); l != "● NotebookEdit(n.ipynb)" {
		t.Errorf("notebook: %q", l)
	}
	if l := toolLine("/r", block{Name: "AskUserQuestion", Input: []byte(`{"questions":[]}`)}); l != "● AskUserQuestion" {
		t.Errorf("no question: %q", l)
	}
	if l := toolLine("/r", block{Name: "Bash", Input: []byte(`not json`)}); l != "● Bash" {
		t.Errorf("bad input: %q", l)
	}
}

func TestActivityResultShapes(t *testing.T) {
	for _, tc := range []struct {
		tool, content string
		isErr         bool
		want          string
	}{
		{"Bash", `"one"`, false, "  ⎿ one"},
		{"Bash", `"a\nb\nc"`, false, "  ⎿ a … +2 lines"},
		{"Read", `"x"`, false, "  ⎿ x"},
		{"Grep", `"a\nb"`, false, "  ⎿ 2 lines"},
		{"Bash", `"   "`, false, "  ⎿ (no output)"},
		{"Bash", `"boom"`, true, "  ⎿ Error: boom"},
		{"Bash", `""`, true, "  ⎿ Error"},
		{"Bash", `[{"type":"text","text":"a"},{"type":"text","text":"b"}]`, false, "  ⎿ a … +1 line"},
		{"Bash", `42`, false, "  ⎿ (no output)"},
		{"Bash", `"` + strings.Repeat("y", 200) + `"`, false, "  ⎿ " + strings.Repeat("y", TailWidth-5) + "…"},
	} {
		if got := resultLine(tc.tool, []byte(tc.content), tc.isErr); got != tc.want {
			t.Errorf("resultLine(%s, %s, %v) = %q, want %q", tc.tool, tc.content, tc.isErr, got, tc.want)
		}
	}
}

// Activity may be read while a refresh parses new lines (the store serves
// it to clients off the refresh goroutine).
func TestActivityConcurrentRead(t *testing.T) {
	p := copyFixture(t, "activity")
	tr := NewTranscript(p)
	var wg sync.WaitGroup
	wg.Add(1)
	go func() {
		defer wg.Done()
		for range 200 {
			_ = tr.Activity(10)
			_ = tr.ActivitySeq()
		}
	}()
	for i := range 50 {
		appendLine(t, p, fmt.Sprintf(`{"type":"assistant","timestamp":"2026-10-15T11:00:00Z","message":{"content":[{"type":"text","text":"l%d"}]}}`, i))
		_ = tr.Refresh(flat)
	}
	wg.Wait()
}
