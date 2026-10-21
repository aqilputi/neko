package config

import (
	"os"
	"path/filepath"
	"reflect"
	"strings"
	"testing"
)

func TestLoadMissingFileGivesDefaults(t *testing.T) {
	setBases(t)
	c, err := Load()
	if err != nil {
		t.Fatal(err)
	}
	d := Default()
	if c.BarMaxUSD != d.BarMaxUSD || c.Refresh != d.Refresh || c.Sort != d.Sort || c.ShowHistoryDays != d.ShowHistoryDays {
		t.Errorf("scalars differ from Default(): %+v", c)
	}
	if len(c.Rules) != len(d.Rules) || !reflect.DeepEqual(c.Prices, d.Prices) {
		t.Errorf("rules/prices differ from Default()")
	}
	for i, r := range c.Rules {
		if r.re == nil {
			t.Errorf("rule %d (%s) not compiled", i, r.Match)
		}
	}
	// Jira defaults are filled in even without a [jira] table.
	if c.Jira.Model != "haiku" || c.Jira.CacheMinutes != 30 || c.Jira.DefaultType != "Task" {
		t.Errorf("jira defaults not applied: %+v", c.Jira)
	}
}

const fullTOML = `
theme = "rose-pine-dawn"
bar_max_usd = 5
daily_budget_usd = 40
refresh_seconds = 1.5
usage_refresh_seconds = 0
show_history_days = 7
stale_days = 1
sort = "custo"
permission_mode = "plan"
repo_roots = ["~/src"]

[todo_repos]
"@api" = "~/work/api"

[jira]
enabled = true
site = "acme.atlassian.net"
projects = ["ABC", "DEF"]
default_project = "ABC"
cache_minutes = 5

[[context]]
match = '^~/work/([^/]+)'
name = "work · $1"

[[context]]
match = '.*'
name = "misc"

[[price]]
match = "opus"
input = 1
output = 2
cache_write = 3
cache_read = 4
`

func TestLoadFullFile(t *testing.T) {
	cfg, _, _ := setBases(t)
	write(t, filepath.Join(cfg, "nekuroddo", "config.toml"), fullTOML)

	c, err := Load()
	if err != nil {
		t.Fatal(err)
	}
	if c.Theme != "rose-pine-dawn" || c.BarMaxUSD != 5 || c.DailyBudgetUSD != 40 || c.Refresh != 1.5 ||
		c.UsageRefresh != 0 || c.ShowHistoryDays != 7 || c.StaleDays != 1 || c.Sort != "custo" || c.PermissionMode != "plan" {
		t.Errorf("scalars not read: %+v", c)
	}
	if !reflect.DeepEqual(c.RepoRoots, []string{"~/src"}) || c.TodoRepos["@api"] != "~/work/api" {
		t.Errorf("repos not read: %v %v", c.RepoRoots, c.TodoRepos)
	}
	j := c.Jira
	if !j.Enabled || j.Site != "acme.atlassian.net" || j.DefaultProject != "ABC" || len(j.Projects) != 2 || j.CacheMinutes != 5 {
		t.Errorf("jira not read: %+v", j)
	}
	if j.CloudID != "acme.atlassian.net" || j.Model != "haiku" {
		t.Errorf("jira defaults not applied over the file: %+v", j)
	}
	// User rules replace the built-in ones entirely.
	if len(c.Rules) != 2 {
		t.Fatalf("rules = %d, want 2 (user rules replace defaults)", len(c.Rules))
	}
	// User prices come first, then the built-in ones as a fallback.
	if len(c.Prices) != 1+len(Default().Prices) {
		t.Fatalf("prices = %d, want user + defaults", len(c.Prices))
	}
	if p := c.PriceFor("claude-opus-5-5"); p.Input != 1 {
		t.Errorf("user price must override built-in: %+v", p)
	}
	if p := c.PriceFor("claude-haiku-4"); p.Input != 1 || p.Output != 5 {
		t.Errorf("built-in prices must remain as fallback: %+v", p)
	}
	if got := c.ContextFor(filepath.Join(home(t), "work", "api", "sub")); got != "work · api" {
		t.Errorf("ContextFor = %q", got)
	}
}

func TestLoadPartialFileKeepsDefaultRules(t *testing.T) {
	cfg, _, _ := setBases(t)
	write(t, filepath.Join(cfg, "nekuroddo", "config.toml"), "theme = \"x\"\n")
	c, err := Load()
	if err != nil {
		t.Fatal(err)
	}
	if c.Theme != "x" {
		t.Errorf("theme = %q", c.Theme)
	}
	if len(c.Rules) != len(Default().Rules) || !reflect.DeepEqual(c.Prices, Default().Prices) {
		t.Errorf("file without rules/prices must keep the defaults")
	}
	if c.BarMaxUSD != Default().BarMaxUSD {
		t.Errorf("unset keys must keep defaults, bar = %v", c.BarMaxUSD)
	}
}

func TestLoadInvalidTOML(t *testing.T) {
	cfg, _, _ := setBases(t)
	write(t, filepath.Join(cfg, "nekuroddo", "config.toml"), "theme = \n[[[")
	c, err := Load()
	if err == nil {
		t.Fatal("want an error for invalid TOML")
	}
	if c.BarMaxUSD != Default().BarMaxUSD || len(c.Rules) != len(Default().Rules) {
		t.Errorf("invalid file must fall back to Default(): %+v", c)
	}
}

func TestLoadWrongTypeIsError(t *testing.T) {
	cfg, _, _ := setBases(t)
	write(t, filepath.Join(cfg, "nekuroddo", "config.toml"), "bar_max_usd = \"lots\"\n")
	if _, err := Load(); err == nil {
		t.Fatal("want an error when a key has the wrong type")
	}
}

func TestLoadInvalidRegexDoesNotPanic(t *testing.T) {
	cfg, _, _ := setBases(t)
	write(t, filepath.Join(cfg, "nekuroddo", "config.toml"), "[[context]]\nmatch = '('\nname = 'x'\n")
	defer func() {
		if r := recover(); r != nil {
			t.Fatalf("Load panicked: %v", r)
		}
	}()
	c, err := Load()
	if err == nil || !strings.Contains(err.Error(), "(") {
		t.Fatalf("want an error naming the invalid regexp, got %v", err)
	}
	if c.ContextFor("/tmp/x") == "" {
		t.Fatal("an invalid file falls back to the default rules")
	}
}

func TestContextForSkipsInvalidLiteralRule(t *testing.T) {
	c := Config{Rules: []Rule{{Match: `(`, Name: "quebrada"}, {Match: `.*`, Name: "ok"}}}
	if got := c.ContextFor("/tmp/x"); got != "ok" {
		t.Fatalf("an invalid rule should be skipped, got %q", got)
	}
}

func home(t *testing.T) string {
	t.Helper()
	h, err := os.UserHomeDir()
	if err != nil {
		t.Fatal(err)
	}
	return h
}

func mkdir(t *testing.T, d string) {
	t.Helper()
	if err := os.MkdirAll(d, 0o755); err != nil {
		t.Fatal(err)
	}
}

func TestContextForDefaultRules(t *testing.T) {
	setBases(t)
	h := home(t)
	c := Default() // rules without compiled regexps: ContextFor compiles them
	cases := []struct{ cwd, want string }{
		{h + "/dev/worktrees/PROJ-123/x", "dev · PROJ-123"},
		{h + "/dev/nekuroddo/internal", "dev · nekuroddo"},
		{h + "/src/api", "src · api"},
		{h + "/code/web/app", "code · web"},
		{h + "/projects/site", "projects · site"},
		{h + "/work/billing", "work · billing"},
		{h + "/Documents/x", "Documents"},
		{h + "/.config/x", "home"},
		{h, "home"},
		{"/tmp/elsewhere", "home"},
	}
	for _, tc := range cases {
		if got := c.ContextFor(tc.cwd); got != tc.want {
			t.Errorf("ContextFor(%q) = %q, want %q", tc.cwd, got, tc.want)
		}
	}
}

func TestContextForNoMatch(t *testing.T) {
	setBases(t)
	c := Config{Rules: []Rule{{Match: `^~/only`, Name: "only"}}}
	if got := c.ContextFor("/srv/x"); got != "outros" {
		t.Errorf("ContextFor = %q, want outros", got)
	}
	if got := (Config{}).ContextFor("/x"); got != "outros" {
		t.Errorf("no rules: %q", got)
	}
}

func TestPriceFor(t *testing.T) {
	c := Default()
	cases := []struct {
		model string
		in    float64
		read  float64
	}{
		{"claude-opus-5-5[1m]", 4, 0.2},
		{"claude-opus-4-1", 5, 0.5},
		{"Claude-SONNET-5", 2, 0.2}, // case-insensitive
		{"claude-sonnet-4-5", 3, 0.3},
		{"claude-haiku-4-5", 1, 0.1},
		{"claude-fable-5-1", 10, 0.25},
		{"claude-fable-5", 10, 1},
		{"gpt-whatever", 5, 0.5}, // fallback row
		{"", 5, 0.5},
	}
	for _, tc := range cases {
		p := c.PriceFor(tc.model)
		if p.Input != tc.in || p.CacheRead != tc.read {
			t.Errorf("PriceFor(%q) = %+v, want input %v cache_read %v", tc.model, p, tc.in, tc.read)
		}
	}
}

func TestTildeExpand(t *testing.T) {
	setBases(t)
	h := home(t)
	tilde := []struct{ in, want string }{
		{h, "~"},
		{h + "/a/b", "~/a/b"},
		{h + "x/sibling", h + "x/sibling"}, // prefix without a slash is not home
		{"/etc", "/etc"},
		{"", ""},
	}
	for _, tc := range tilde {
		if got := Tilde(tc.in); got != tc.want {
			t.Errorf("Tilde(%q) = %q, want %q", tc.in, got, tc.want)
		}
	}
	expand := []struct{ in, want string }{
		{"~", h},
		{"~/a/b", h + "/a/b"},
		{"~user/x", "~user/x"},
		{"/abs", "/abs"},
		{"rel/~", "rel/~"},
	}
	for _, tc := range expand {
		if got := Expand(tc.in); got != tc.want {
			t.Errorf("Expand(%q) = %q, want %q", tc.in, got, tc.want)
		}
		if tc.in == "~" || len(tc.in) > 1 && tc.in[:2] == "~/" {
			if back := Tilde(Expand(tc.in)); back != tc.in {
				t.Errorf("Tilde(Expand(%q)) = %q", tc.in, back)
			}
		}
	}
}

func TestRepoFor(t *testing.T) {
	setBases(t)
	h := home(t)
	mkdir(t, filepath.Join(h, "dev", "api"))
	mkdir(t, filepath.Join(h, "src", "api")) // second root: first root wins
	mkdir(t, filepath.Join(h, "src", "web"))
	write(t, filepath.Join(h, "dev", "notadir"), "x")

	c := Config{
		TodoRepos: map[string]string{"@mapped": "~/elsewhere/mapped", "@abs": "/opt/abs"},
		RepoRoots: []string{"~/dev", "~/src"},
	}
	cases := []struct{ ctx, want string }{
		{"@mapped", filepath.Join(h, "elsewhere", "mapped")}, // map wins, not required to exist
		{"@abs", "/opt/abs"},
		{"@api", filepath.Join(h, "dev", "api")},
		{"api", filepath.Join(h, "dev", "api")}, // @ is optional
		{"@web", filepath.Join(h, "src", "web")},
		{"@missing", ""},
		{"@notadir", ""},
		{"@", ""},
		{"", ""},
		{"@../dev", ""}, // no path traversal
		{"@a/b", ""},
		{"@.hidden", ""},
	}
	for _, tc := range cases {
		if got := c.RepoFor(tc.ctx); got != tc.want {
			t.Errorf("RepoFor(%q) = %q, want %q", tc.ctx, got, tc.want)
		}
	}
}
