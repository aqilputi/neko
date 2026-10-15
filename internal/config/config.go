// Package config loads ~/.config/neko/config.toml: context rules, theme,
// prices and the budget used to scale the cost bars.
package config

import (
	"os"
	"path/filepath"
	"regexp"
	"strings"

	"github.com/BurntSushi/toml"

	"github.com/aqilputi/neko/internal/jira"
)

// Rule maps sessions whose cwd matches Match (a regexp on the cwd with ~
// expanded) to a context. Name may use $1, $2… from the regexp.
type Rule struct {
	Match string `toml:"match"`
	Name  string `toml:"name"`
	re    *regexp.Regexp
}

// Price is USD per million tokens for models whose id contains Match.
// The first matching row wins, so list specific ids before generic ones.
type Price struct {
	Match      string  `toml:"match"`
	Input      float64 `toml:"input"`
	Output     float64 `toml:"output"`
	CacheWrite float64 `toml:"cache_write"`
	CacheRead  float64 `toml:"cache_read"`
}

type Config struct {
	Theme string `toml:"theme"`
	// BarMaxUSD is the cost that fills a bar completely.
	BarMaxUSD float64 `toml:"bar_max_usd"`
	// DailyBudgetUSD draws the header gauge; 0 hides it.
	DailyBudgetUSD float64 `toml:"daily_budget_usd"`
	// Refresh is the poll interval for `claude agents --json`, in seconds.
	Refresh float64 `toml:"refresh_seconds"`
	// UsageRefresh is how often `claude -p /usage` (the 5-hour and weekly
	// subscription limits) is polled, in seconds. 0 turns it off.
	UsageRefresh float64 `toml:"usage_refresh_seconds"`
	// ShowHistoryDays also lists finished sessions from the transcripts of the
	// last N days.
	ShowHistoryDays int `toml:"show_history_days"`
	// StaleDays: sessions idle longer than this stop asking for attention and
	// are hidden until you press H (busy ones always show).
	StaleDays float64 `toml:"stale_days"`
	// Sort is the initial order: atividade | custo | nome.
	Sort string `toml:"sort"`
	// PermissionMode is the default for agents neko dispatches: "" (Claude's
	// own default), acceptEdits, auto, plan or bypassPermissions. tab cycles
	// it in the dispatch line.
	PermissionMode string `toml:"permission_mode"`

	Jira jira.Config `toml:"jira"`
	// TodoRepos maps a todo @context (e.g. "@ai_registry") to the directory an
	// agent dispatched for that task should start in.
	TodoRepos map[string]string `toml:"todo_repos"`
	// RepoRoots are searched for a directory named like the @context when it
	// is not in TodoRepos.
	RepoRoots []string `toml:"repo_roots"`

	Rules  []Rule  `toml:"context"`
	Prices []Price `toml:"price"`
}

// Default rules reflect how ~/ is organized (mapped from real usage): Visio
// work under ~/visio, personal projects grouped by theme, the rest by folder.
func Default() Config {
	return Config{
		Theme:           "", // follow the terminal: dark → catppuccin-mocha, light → rose-pine-dawn
		BarMaxUSD:       20,
		DailyBudgetUSD:  0,
		Refresh:         2,
		UsageRefresh:    60,
		ShowHistoryDays: 2,
		StaleDays:       3,
		Sort:            "atividade",
		RepoRoots:       []string{"~/visio/dev", "~/dev", "~"},
		Rules: []Rule{
			{Match: `^~/visio/dev/worktrees/([A-Z]+-\d+)`, Name: "visio · $1"},
			{Match: `^~/visio/dev/visio-processing-context`, Name: "visio · processing-context"},
			{Match: `^~/visio/(dev/visio-research-context|visio-research)`, Name: "visio · research-context"},
			{Match: `^~/visio/dev/([^/]+)`, Name: "visio · $1"},
			{Match: `^~/visio`, Name: "visio · outros"},
			{Match: `^~/(dev/taiko-fu|papparapa-anim|dev/tengoku)`, Name: "pessoal · taiko"},
			{Match: `^~/(dev/apify|dev/petpromo|renda-extra-research)`, Name: "pessoal · renda-extra"},
			{Match: `^~/(prettyface-hunt|otto-triin)`, Name: "pessoal · lost-media"},
			{Match: `^~/(bta-server|minecraft-server|dev/osrs-farm-test|dev/rsps|dev/sombra)`, Name: "pessoal · games"},
			{Match: `^~/dev/([^/]+)`, Name: "pessoal · $1"},
			{Match: `^~/([^/.][^/]*)`, Name: "pessoal · $1"},
			{Match: `.*`, Name: "pessoal · home"},
		},

		// Anthropic first-party list prices (USD/MTok). Cache write = 5-minute TTL.
		Prices: []Price{
			{Match: "fable-5-1", Input: 10, Output: 50, CacheWrite: 12.5, CacheRead: 0.25},
			{Match: "fable", Input: 10, Output: 50, CacheWrite: 12.5, CacheRead: 1},
			{Match: "mythos", Input: 10, Output: 50, CacheWrite: 12.5, CacheRead: 1},
			{Match: "opus-5-5", Input: 4, Output: 20, CacheWrite: 5, CacheRead: 0.2},
			{Match: "opus", Input: 5, Output: 25, CacheWrite: 6.25, CacheRead: 0.5},
			{Match: "sonnet-5", Input: 2, Output: 10, CacheWrite: 2.5, CacheRead: 0.2},
			{Match: "sonnet", Input: 3, Output: 15, CacheWrite: 3.75, CacheRead: 0.3},
			{Match: "haiku", Input: 1, Output: 5, CacheWrite: 1.25, CacheRead: 0.1},
		},
	}
}

func Path() string {
	dir := os.Getenv("XDG_CONFIG_HOME")
	if dir == "" {
		home, _ := os.UserHomeDir()
		dir = filepath.Join(home, ".config")
	}
	return filepath.Join(dir, "neko", "config.toml")
}

// Load reads the config file over the defaults. A missing file is fine.
func Load() (Config, error) {
	c := Default()
	if b, err := os.ReadFile(Path()); err == nil {
		def := c.Rules
		c.Rules, c.Prices = nil, nil
		if _, err := toml.Decode(string(b), &c); err != nil {
			return Default(), err
		}
		if len(c.Rules) == 0 {
			c.Rules = def
		}
		// User rows come first so they override the built-in ones.
		c.Prices = append(c.Prices, Default().Prices...)
	}
	c.Jira = c.Jira.WithDefaults()
	for i := range c.Rules {
		c.Rules[i].re = regexp.MustCompile(c.Rules[i].Match)
	}
	return c, nil
}

// Tilde shortens /home/user/x to ~/x.
func Tilde(p string) string {
	home, _ := os.UserHomeDir()
	if p == home {
		return "~"
	}
	if strings.HasPrefix(p, home+"/") {
		return "~" + p[len(home):]
	}
	return p
}

// Expand turns ~/x into /home/user/x.
func Expand(p string) string {
	if p == "~" || strings.HasPrefix(p, "~/") {
		home, _ := os.UserHomeDir()
		return home + p[1:]
	}
	return p
}

// ContextFor names the context of a session started in cwd.
func (c Config) ContextFor(cwd string) string {
	t := Tilde(cwd)
	for _, r := range c.Rules {
		if r.re == nil { // rules from Default() or a literal Config
			r.re = regexp.MustCompile(r.Match)
		}
		if m := r.re.FindStringSubmatchIndex(t); m != nil {
			return string(r.re.ExpandString(nil, r.Name, t, m))
		}
	}
	return "outros"
}

// PriceFor finds the price row whose key appears in the model id.
func (c Config) PriceFor(model string) Price {
	model = strings.ToLower(model)
	for _, p := range c.Prices {
		if strings.Contains(model, p.Match) {
			return p
		}
	}
	return Price{Input: 5, Output: 25, CacheWrite: 6.25, CacheRead: 0.5}
}

// RepoFor resolves a todo @context to a directory, or "" if unknown.
func (c Config) RepoFor(ctx string) string {
	if d, ok := c.TodoRepos[ctx]; ok {
		return Expand(d)
	}
	name := strings.TrimPrefix(ctx, "@")
	if name == "" || strings.ContainsAny(name, "/.") {
		return ""
	}
	for _, root := range c.RepoRoots {
		d := filepath.Join(Expand(root), name)
		if st, err := os.Stat(d); err == nil && st.IsDir() {
			return d
		}
	}
	return ""
}
