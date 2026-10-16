package jira

import (
	"encoding/json"
	"errors"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"testing"
	"time"
)

// fake points claudeBin at testdata/fake-claude.sh replaying stream (a file
// in testdata or an absolute path) and isolates HOME and the XDG dirs. It
// returns a function that reads back the recorded argv.
func fake(t *testing.T, stream string, exit int) (args func() string) {
	t.Helper()
	root := t.TempDir()
	t.Setenv("HOME", root)
	t.Setenv("XDG_CONFIG_HOME", filepath.Join(root, "config"))
	t.Setenv("XDG_STATE_HOME", filepath.Join(root, "state"))
	t.Setenv("XDG_CACHE_HOME", filepath.Join(root, "cache"))
	bin, err := filepath.Abs("testdata/fake-claude.sh")
	if err != nil {
		t.Fatal(err)
	}
	t.Setenv("NEKO_CLAUDE", bin)
	out := ""
	if stream != "" {
		out = stream
		if !filepath.IsAbs(out) {
			out, _ = filepath.Abs(filepath.Join("testdata", stream))
		}
	}
	t.Setenv("FAKE_CLAUDE_OUT", out)
	t.Setenv("FAKE_CLAUDE_EXIT", strconv.Itoa(exit))
	argsFile := filepath.Join(root, "argv")
	t.Setenv("FAKE_CLAUDE_ARGS", argsFile)
	return func() string {
		b, err := os.ReadFile(argsFile)
		if err != nil {
			t.Fatalf("fake claude was not called: %v", err)
		}
		return string(b)
	}
}

func TestWithDefaults(t *testing.T) {
	d := Config{Site: "acme.atlassian.net"}.WithDefaults()
	if d.Model != "haiku" || d.MaxBudgetUSD != 0.3 || d.CacheMinutes != 30 || d.DefaultType != "Task" ||
		!strings.Contains(d.JQL, "currentUser()") {
		t.Errorf("defaults not filled: %+v", d)
	}
	if d.CloudID != "acme.atlassian.net" {
		t.Errorf("CloudID must fall back to Site, got %q", d.CloudID)
	}
	set := Config{Site: "s", CloudID: "uuid", Model: "sonnet", MaxBudgetUSD: 1, CacheMinutes: 5, DefaultType: "Bug", JQL: "x"}
	if got := set.WithDefaults(); got.Model != "sonnet" || got.CloudID != "uuid" || got.MaxBudgetUSD != 1 ||
		got.CacheMinutes != 5 || got.DefaultType != "Bug" || got.JQL != "x" {
		t.Errorf("explicit values overwritten: %+v", got)
	}
}

func TestUpdatedAt(t *testing.T) {
	cases := []struct {
		in   string
		want time.Time
	}{
		{"2026-10-15T09:30:00.000-0300", time.Date(2026, 10, 15, 12, 30, 0, 0, time.UTC)},
		{"2026-01-02T03:04:05.678+0000", time.Date(2026, 1, 2, 3, 4, 5, 678e6, time.UTC)},
		{"", time.Time{}},
		{"2026-10-15", time.Time{}},
	}
	for _, tc := range cases {
		if got := (Issue{Updated: tc.in}).UpdatedAt(); !got.Equal(tc.want) {
			t.Errorf("UpdatedAt(%q) = %v, want %v", tc.in, got, tc.want)
		}
	}
}

func TestCacheFresh(t *testing.T) {
	cfg := Config{CacheMinutes: 10}
	cases := []struct {
		age  time.Duration
		want bool
	}{
		{0, true},
		{9 * time.Minute, true},
		{11 * time.Minute, false},
	}
	for _, tc := range cases {
		if got := (Cache{FetchedAt: time.Now().Add(-tc.age)}).Fresh(cfg); got != tc.want {
			t.Errorf("Fresh(age %v) = %v, want %v", tc.age, got, tc.want)
		}
	}
	if (Cache{}).Fresh(cfg) {
		t.Error("a never-fetched cache must not be fresh")
	}
}

func TestCacheRoundTrip(t *testing.T) {
	fake(t, "", 0)
	if c := LoadCache(); !c.FetchedAt.IsZero() || len(c.Issues) != 0 {
		t.Fatalf("missing file must give an empty cache: %+v", c)
	}
	want := Cache{FetchedAt: time.Now().Round(0).UTC(), CostUSD: 0.1, Issues: []Issue{{Key: "A-1", Desc: "d"}}}
	saveCache(want)
	st, err := os.Stat(cachePath())
	if err != nil {
		t.Fatal(err)
	}
	if st.Mode().Perm() != 0o600 {
		t.Errorf("cache perms = %v, want 0600", st.Mode().Perm())
	}
	if !strings.HasPrefix(cachePath(), os.Getenv("XDG_CACHE_HOME")) {
		t.Errorf("cache outside XDG_CACHE_HOME: %s", cachePath())
	}
	got := LoadCache()
	if !got.FetchedAt.Equal(want.FetchedAt) || got.CostUSD != 0.1 || len(got.Issues) != 1 || got.Issues[0] != want.Issues[0] {
		t.Errorf("LoadCache = %+v, want %+v", got, want)
	}
}

func TestLoadCacheCorrupt(t *testing.T) {
	fake(t, "", 0)
	if err := os.MkdirAll(filepath.Dir(cachePath()), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(cachePath(), []byte("{not json"), 0o600); err != nil {
		t.Fatal(err)
	}
	if c := LoadCache(); len(c.Issues) != 0 || !c.FetchedAt.IsZero() {
		t.Errorf("corrupt cache must load empty: %+v", c)
	}
}

func TestFetch(t *testing.T) {
	args := fake(t, "search_ok.jsonl", 0)
	cfg := Config{Site: "acme.atlassian.net", CloudID: "cloud-uuid", JQL: "project = ABC", Model: "sonnet", MaxBudgetUSD: 0.5}
	c, err := Fetch(cfg)
	if err != nil {
		t.Fatal(err)
	}
	if c.CostUSD != 0.042 || time.Since(c.FetchedAt) > time.Minute {
		t.Errorf("cost/time: %+v", c)
	}
	want := []Issue{
		{Key: "ABC-1", Summary: "Corrigir login", Status: "Em andamento", StatusCat: "indeterminate", Type: "Bug",
			Priority: "High", Updated: "2026-10-15T09:30:00.000-0300", URL: "https://acme.atlassian.net/browse/ABC-1",
			Desc: "Passos:\n\n1. abrir [app]"},
		// No webUrl: built from the site. null description: "".
		{Key: "ABC-2", Summary: "Sem link", Status: "To Do", StatusCat: "new", Type: "Task", Priority: "Medium",
			URL: "https://acme.atlassian.net/browse/ABC-2"},
	}
	if len(c.Issues) != len(want) {
		t.Fatalf("issues = %+v", c.Issues)
	}
	for i := range want {
		if c.Issues[i] != want[i] {
			t.Errorf("issue %d:\n got %+v\nwant %+v", i, c.Issues[i], want[i])
		}
	}

	// It refreshes the cache and keeps the raw run for debugging.
	if got := LoadCache(); len(got.Issues) != 2 || got.CostUSD != 0.042 {
		t.Errorf("cache not saved: %+v", got)
	}
	raw, err := os.ReadFile(lastRunPath())
	if err != nil || !strings.Contains(string(raw), "searchJiraIssuesUsingJql") {
		t.Errorf("last run not saved: %v", err)
	}

	// How claude was invoked: one allowed tool, no persistence, default
	// permission mode, the configured model/budget, the JQL in the prompt.
	a := args()
	for _, s := range []string{"-p\n", "--no-session-persistence\n", "--permission-mode\ndefault\n",
		"--model\nsonnet\n", "--output-format\nstream-json\n", "--max-budget-usd\n0.5\n",
		"--allowedTools=mcp__claude_ai_Atlassian_Rovo__searchJiraIssuesUsingJql\n",
		`cloudId="cloud-uuid"`, `jql="project = ABC"`} {
		if !strings.Contains(a, s) {
			t.Errorf("argv missing %q:\n%s", s, a)
		}
	}
	env, _ := os.ReadFile(os.Getenv("FAKE_CLAUDE_ARGS") + ".env")
	if lines := strings.Split(string(env), "\n"); lines[0] != os.Getenv("HOME") || lines[1] != "ENABLE_CLAUDEAI_MCP_SERVERS=true" {
		t.Errorf("cwd/env of claude: %q", env)
	}
}

func TestFetchNoSiteKeepsEmptyURL(t *testing.T) {
	fake(t, "search_ok.jsonl", 0)
	c, err := Fetch(Config{CloudID: "u"})
	if err != nil {
		t.Fatal(err)
	}
	if c.Issues[1].URL != "" {
		t.Errorf("without a site there is no URL to build, got %q", c.Issues[1].URL)
	}
}

func TestFetchErrors(t *testing.T) {
	cases := []struct {
		name   string
		stream string
		exit   int
		want   string
	}{
		{"malformed tool result", "search_malformed.jsonl", 0, "resposta do Jira inesperada"},
		{"tool never called", "no_tool.jsonl", 0, "não chamou searchJiraIssuesUsingJql"},
		{"other tool called", "wrong_tool.jsonl", 0, "não consegui chamar a ferramenta"},
		{"no output and failure", "", 1, "claude -p"},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			fake(t, tc.stream, tc.exit)
			c, err := Fetch(Config{Site: "s"})
			if err == nil || !strings.Contains(err.Error(), tc.want) {
				t.Fatalf("err = %v, want it to contain %q", err, tc.want)
			}
			var d DeniedError
			if errors.As(err, &d) {
				t.Errorf("not a denial: %v", err)
			}
			if len(c.Issues) != 0 {
				t.Errorf("issues on error: %+v", c)
			}
			if _, err := os.Stat(cachePath()); !os.IsNotExist(err) {
				t.Errorf("a failed fetch must not overwrite the cache")
			}
		})
	}
}

func TestFetchNonZeroExitWithOutputStillParses(t *testing.T) {
	// claude -p exits non-zero when it hits --max-budget-usd, but the tool
	// result is already in the stream.
	fake(t, "search_ok.jsonl", 1)
	c, err := Fetch(Config{Site: "s"})
	if err != nil || len(c.Issues) != 2 {
		t.Fatalf("Fetch = %+v, %v", c, err)
	}
}

func TestFetchMissingBinary(t *testing.T) {
	fake(t, "", 0)
	t.Setenv("NEKO_CLAUDE", filepath.Join(t.TempDir(), "nope"))
	if _, err := Fetch(Config{}); err == nil || !strings.Contains(err.Error(), "claude -p") {
		t.Fatalf("err = %v", err)
	}
}

func TestClaudeBin(t *testing.T) {
	t.Setenv("NEKO_CLAUDE", "")
	if claudeBin() != "claude" {
		t.Errorf("default = %q", claudeBin())
	}
	t.Setenv("NEKO_CLAUDE", "/opt/claude")
	if claudeBin() != "/opt/claude" {
		t.Errorf("override = %q", claudeBin())
	}
}

func TestCreate(t *testing.T) {
	cases := []struct {
		name   string
		stream string
		key    string
		cost   float64
	}{
		{"json key", "create_ok.jsonl", "ABC-42", 0.03},
		{"key in text", "create_text_key.jsonl", "ABC-77", 0.03},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			fake(t, tc.stream, 0)
			key, cost, err := Create(Config{CloudID: "u"}, NewIssue{Project: "ABC", Type: "Task", Summary: "s"})
			if err != nil || key != tc.key || cost != tc.cost {
				t.Fatalf("Create = %q, %v, %v; want %q, %v", key, cost, err, tc.key, tc.cost)
			}
		})
	}
}

func TestCreateArguments(t *testing.T) {
	args := fake(t, "create_ok.jsonl", 0)
	n := NewIssue{Project: "ABC", Type: "Bug", Summary: `diz "oi"`, Description: "linha 1\nlinha 2", Labels: []string{"neko", "x"}}
	if _, _, err := Create(Config{CloudID: "cid", AccountID: "acc-1"}, n); err != nil {
		t.Fatal(err)
	}
	a := args()
	if !strings.Contains(a, "--allowedTools=mcp__claude_ai_Atlassian_Rovo__createJiraIssue\n") {
		t.Errorf("only createJiraIssue may be allowed:\n%s", a)
	}
	// The prompt embeds the exact arguments as JSON; parse them back.
	i := strings.Index(a, "{")
	end := strings.Index(a, "\nDepois")
	if i < 0 || end < 0 {
		t.Fatalf("no JSON in prompt:\n%s", a)
	}
	var got map[string]any
	if err := json.Unmarshal([]byte(a[i:end]), &got); err != nil {
		t.Fatalf("prompt JSON: %v\n%s", err, a[i:end])
	}
	want := map[string]any{
		"cloudId": "cid", "projectKey": "ABC", "issueTypeName": "Bug", "summary": `diz "oi"`,
		"description": "linha 1\nlinha 2", "contentFormat": "markdown", "assignee_account_id": "acc-1",
		"additional_fields": map[string]any{"labels": []any{"neko", "x"}},
	}
	gb, _ := json.Marshal(got)
	wb, _ := json.Marshal(want)
	if string(gb) != string(wb) {
		t.Errorf("args:\n got %s\nwant %s", gb, wb)
	}
}

func TestCreateOmitsOptionalArguments(t *testing.T) {
	args := fake(t, "create_ok.jsonl", 0)
	if _, _, err := Create(Config{}, NewIssue{Project: "ABC", Type: "Task", Summary: "s"}); err != nil {
		t.Fatal(err)
	}
	a := args()
	if strings.Contains(a, "assignee_account_id") || strings.Contains(a, "additional_fields") {
		t.Errorf("no account/labels must mean no such arguments:\n%s", a)
	}
}

func TestCreateErrors(t *testing.T) {
	cases := []struct {
		name       string
		stream     string
		denied     bool
		wantReason string // substring of the error
		cost       float64
	}{
		{"hook denied", "create_denied_hook.jsonl", true,
			"PreToolUse:mcp hook error: Jira writes are blocked. Run jira-allow to open a window.", 0.02},
		{"denied without tool result", "denied_no_tool_result.jsonl", true, "Permission denied for createJiraIssue", 0.01},
		{"jira error", "jira_error.jsonl", false, "createJiraIssue: Field 'summary' is required", 0.02},
		{"no key", "create_no_key.jsonl", false, "não achei a chave", 0.03},
		{"not called", "no_tool.jsonl", false, "não chamou createJiraIssue", 0},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			fake(t, tc.stream, 0)
			key, cost, err := Create(Config{}, NewIssue{Project: "ABC"})
			if err == nil || key != "" {
				t.Fatalf("Create = %q, %v; want an error", key, err)
			}
			if !strings.Contains(err.Error(), tc.wantReason) {
				t.Errorf("err = %q, want it to contain %q", err, tc.wantReason)
			}
			if cost != tc.cost {
				t.Errorf("cost = %v, want %v (spent even on failure)", cost, tc.cost)
			}
			var d DeniedError
			if errors.As(err, &d) != tc.denied {
				t.Errorf("DeniedError = %v, want %v (%v)", errors.As(err, &d), tc.denied, err)
			}
			if tc.denied && !strings.Contains(d.Reason, strings.Fields(tc.wantReason)[0]) {
				t.Errorf("Reason = %q", d.Reason)
			}
		})
	}
}

func TestDeniedError(t *testing.T) {
	e := DeniedError{Reason: "hook says no"}
	if e.Error() != "o claude negou a chamada: hook says no" {
		t.Errorf("Error() = %q", e.Error())
	}
}

func TestSpilledResultIsRead(t *testing.T) {
	fake(t, "", 0)
	dir := t.TempDir()
	big := filepath.Join(dir, "tool-result.txt")
	if err := os.WriteFile(big, []byte(`{"key":"ABC-9"}`), 0o600); err != nil {
		t.Fatal(err)
	}
	notice := "Error: result (123,456 tokens) exceeds maximum allowed tokens. Output has been saved to " + big + " — read it in chunks"
	stream := filepath.Join(dir, "stream.jsonl")
	lines := []string{
		`{"type":"assistant","message":{"content":[{"type":"tool_use","id":"c1","name":"mcp__claude_ai_Atlassian_Rovo__createJiraIssue"}]}}`,
		mustJSON(t, map[string]any{"type": "user", "message": map[string]any{"content": []any{
			map[string]any{"type": "tool_result", "tool_use_id": "c1", "content": notice}}}}),
		`{"type":"result","total_cost_usd":0.5,"result":"ok"}`,
	}
	if err := os.WriteFile(stream, []byte(strings.Join(lines, "\n")), 0o600); err != nil {
		t.Fatal(err)
	}
	t.Setenv("FAKE_CLAUDE_OUT", stream)
	key, _, err := Create(Config{}, NewIssue{})
	if err != nil || key != "ABC-9" {
		t.Fatalf("Create = %q, %v", key, err)
	}
	if _, err := os.Stat(big); !os.IsNotExist(err) {
		t.Error("the spilled file must be removed after reading")
	}
}

func TestSpilled(t *testing.T) {
	missing := "Error: result (9 tokens) exceeds maximum allowed tokens. Output has been saved to /nonexistent/x.txt"
	cases := []string{"plain text", "", missing}
	for _, in := range cases {
		if got := spilled(in); got != in {
			t.Errorf("spilled(%q) = %q, want unchanged", in, got)
		}
	}
}

func TestRawText(t *testing.T) {
	cases := map[string]string{
		`"plain"`: "plain",
		`[{"type":"text","text":"a"},{"type":"text","text":"b"}]`: "ab",
		`[]`:   "",
		`null`: "",
		`42`:   "",
	}
	for in, want := range cases {
		if got := rawText(json.RawMessage(in)); got != want {
			t.Errorf("rawText(%s) = %q, want %q", in, got, want)
		}
	}
}

func TestOneLine(t *testing.T) {
	cases := []struct {
		in   string
		n    int
		want string
	}{
		{"a\n  b\tc", 10, "a b c"},
		{"abcdef", 6, "abcdef"},
		{"abcdefg", 6, "abcde…"},
		{"ãçéíõú!", 4, "ãçé…"}, // counts runes, not bytes
		{"", 3, ""},
	}
	for _, tc := range cases {
		if got := oneLine(tc.in, tc.n); got != tc.want {
			t.Errorf("oneLine(%q, %d) = %q, want %q", tc.in, tc.n, got, tc.want)
		}
	}
}

func TestAgentPrompt(t *testing.T) {
	i := Issue{Key: "ABC-1", Summary: "Corrigir login", URL: "https://x/browse/ABC-1"}
	p := AgentPrompt(i)
	for _, s := range []string{"ABC-1", "Corrigir login", "Link: https://x/browse/ABC-1", "pare e me pergunte"} {
		if !strings.Contains(p, s) {
			t.Errorf("prompt missing %q:\n%s", s, p)
		}
	}
	if strings.Contains(p, "Descrição") {
		t.Errorf("no description section without a description:\n%s", p)
	}
	i.Desc = "passo 1"
	p = AgentPrompt(i)
	if !strings.Contains(p, "Descrição (cache do nekuroddo, pode estar desatualizada):\npasso 1\n\n") {
		t.Errorf("description not included:\n%s", p)
	}
	if strings.Index(p, "passo 1") > strings.Index(p, "Leia a issue atual") {
		t.Error("cached description must come before the instructions")
	}
}

func mustJSON(t *testing.T, v any) string {
	t.Helper()
	b, err := json.Marshal(v)
	if err != nil {
		t.Fatal(err)
	}
	return string(b)
}
