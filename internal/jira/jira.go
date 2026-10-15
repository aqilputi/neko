// Package jira reads and creates Jira issues by delegating to a headless
// `claude -p` that uses the Atlassian connector the user already has. neko
// never holds a Jira token, and the guarded-access hook still applies: reads
// pass, writes need the user's approval window
// (`guarded-access.py approve-writes jira --minutes 30`).
//
// The model is only asked to call one tool; neko parses the tool's raw
// result from the stream-json output instead of trusting the model's text.
package jira

import (
	"bufio"
	"bytes"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"strings"
	"time"
)

type Config struct {
	Enabled        bool     `toml:"enabled"`
	Site           string   `toml:"site"`     // e.g. acme.atlassian.net
	CloudID        string   `toml:"cloud_id"` // UUID or site
	AccountID      string   `toml:"account_id"`
	Projects       []string `toml:"projects"`
	DefaultProject string   `toml:"default_project"`
	DefaultType    string   `toml:"default_type"`
	JQL            string   `toml:"jql"`
	Model          string   `toml:"model"`
	MaxBudgetUSD   float64  `toml:"max_budget_usd"`
	CacheMinutes   int      `toml:"cache_minutes"`
	// ApproveHint is shown when a write is denied by the guard.
	ApproveHint string `toml:"approve_hint"`
	// AgentDir is where agents dispatched for a Jira issue start.
	AgentDir string `toml:"agent_dir"`
}

func (c Config) WithDefaults() Config {
	if c.JQL == "" {
		c.JQL = "assignee = currentUser() AND resolution = Unresolved ORDER BY updated DESC"
	}
	if c.Model == "" {
		c.Model = "haiku"
	}
	if c.MaxBudgetUSD == 0 {
		c.MaxBudgetUSD = 0.3
	}
	if c.CacheMinutes == 0 {
		c.CacheMinutes = 30
	}
	if c.DefaultType == "" {
		c.DefaultType = "Task"
	}
	if c.CloudID == "" {
		c.CloudID = c.Site
	}
	return c
}

type Issue struct {
	Key       string `json:"key"`
	Summary   string `json:"summary"`
	Status    string `json:"status"`
	StatusCat string `json:"status_cat"` // new | indeterminate | done
	Type      string `json:"type"`
	Priority  string `json:"priority"`
	Updated   string `json:"updated"`
	URL       string `json:"url"`
	Desc      string `json:"desc,omitempty"` // markdown
}

func (i Issue) UpdatedAt() time.Time {
	t, _ := time.Parse("2006-01-02T15:04:05.000-0700", i.Updated)
	return t
}

// Cache is what neko keeps between runs so opening it costs nothing.
type Cache struct {
	FetchedAt time.Time `json:"fetched_at"`
	Issues    []Issue   `json:"issues"`
	CostUSD   float64   `json:"cost_usd"`
}

func cachePath() string {
	dir := os.Getenv("XDG_CACHE_HOME")
	if dir == "" {
		home, _ := os.UserHomeDir()
		dir = filepath.Join(home, ".cache")
	}
	return filepath.Join(dir, "neko", "jira.json")
}

func LoadCache() Cache {
	var c Cache
	if b, err := os.ReadFile(cachePath()); err == nil {
		_ = json.Unmarshal(b, &c)
	}
	return c
}

func (c Cache) Fresh(cfg Config) bool {
	return time.Since(c.FetchedAt) < time.Duration(cfg.CacheMinutes)*time.Minute
}

func saveCache(c Cache) {
	_ = os.MkdirAll(filepath.Dir(cachePath()), 0o755)
	if b, err := json.Marshal(c); err == nil {
		_ = os.WriteFile(cachePath(), b, 0o600)
	}
}

const toolPrefix = "mcp__claude_ai_Atlassian_Rovo__"

// ErrNeedsApproval means the guard blocked a write.
var ErrNeedsApproval = errors.New("escrita no Jira bloqueada pelo guard")

type runResult struct {
	toolText string
	toolErr  bool
	cost     float64
	denied   bool
	final    string
}

// run asks a headless claude to call exactly one Atlassian tool and returns
// the tool's raw result.
func run(cfg Config, tool, instruction string) (runResult, error) {
	var r runResult
	// --no-session-persistence: these calls are neko's plumbing, not sessions
	// of yours to list.
	args := []string{"-p", "--no-session-persistence", "--model", cfg.Model, "--output-format", "stream-json", "--verbose",
		"--max-budget-usd", fmt.Sprint(cfg.MaxBudgetUSD), "--tools", "",
		"--allowedTools=" + toolPrefix + tool, instruction}
	cmd := exec.Command(claudeBin(), args...)
	cmd.Env = append(os.Environ(),
		"ENABLE_CLAUDEAI_MCP_SERVERS=true",
		"MCP_CONNECTION_NONBLOCKING=false",
		"MCP_CONNECTOR_PREWAIT_MS=15000",
		"MCP_STARTUP_WAIT_MS=15000")
	home, _ := os.UserHomeDir()
	cmd.Dir = home
	out, err := cmd.Output()
	if err != nil && len(out) == 0 {
		return r, fmt.Errorf("claude -p: %w", err)
	}
	sc := bufio.NewScanner(bytes.NewReader(out))
	sc.Buffer(make([]byte, 1<<20), 64<<20)
	toolIDs := map[string]bool{}
	for sc.Scan() {
		line := sc.Bytes()
		if len(line) == 0 || line[0] != '{' {
			continue
		}
		var ev struct {
			Type    string `json:"type"`
			Message struct {
				Content []struct {
					Type      string          `json:"type"`
					ID        string          `json:"id"`
					Name      string          `json:"name"`
					ToolUseID string          `json:"tool_use_id"`
					IsError   bool            `json:"is_error"`
					Content   json.RawMessage `json:"content"`
				} `json:"content"`
			} `json:"message"`
			TotalCost  float64                         `json:"total_cost_usd"`
			Result     string                          `json:"result"`
			Denials    []json.RawMessage               `json:"permission_denials"`
			MCPServers []struct{ Name, Status string } `json:"mcp_servers"`
		}
		if json.Unmarshal(line, &ev) != nil {
			continue
		}
		switch ev.Type {
		case "assistant":
			for _, c := range ev.Message.Content {
				if c.Type == "tool_use" && strings.HasSuffix(c.Name, tool) {
					toolIDs[c.ID] = true
				}
			}
		case "user":
			for _, c := range ev.Message.Content {
				if c.Type == "tool_result" && toolIDs[c.ToolUseID] {
					r.toolText, r.toolErr = rawText(c.Content), c.IsError
				}
			}
		case "result":
			r.cost, r.final, r.denied = ev.TotalCost, ev.Result, len(ev.Denials) > 0
		}
	}
	r.toolText = spilled(r.toolText)
	if r.toolText == "" {
		if r.denied {
			return r, ErrNeedsApproval
		}
		return r, fmt.Errorf("o claude não chamou %s (conector Atlassian conectado?): %s", tool, oneLine(r.final, 160))
	}
	if r.toolErr {
		low := strings.ToLower(r.toolText)
		if strings.Contains(low, "guard") || strings.Contains(low, "approve") || strings.Contains(low, "denied") || strings.Contains(low, "blocked") {
			return r, ErrNeedsApproval
		}
		return r, fmt.Errorf("%s: %s", tool, oneLine(r.toolText, 200))
	}
	return r, nil
}

var spilledRe = regexp.MustCompile(`^Error: result \(.*\) exceeds maximum allowed tokens\. Output has been saved to (\S+?\.txt)`)

// spilled follows Claude Code's "output too large" notice to the file it
// saved the full tool result in, so big results (e.g. issues with their
// descriptions) reach neko without the model ingesting them.
func spilled(text string) string {
	m := spilledRe.FindStringSubmatch(text)
	if m == nil {
		return text
	}
	b, err := os.ReadFile(m[1])
	if err != nil {
		return text
	}
	_ = os.Remove(m[1])
	return string(b)
}

func claudeBin() string {
	if b := os.Getenv("NEKO_CLAUDE"); b != "" {
		return b
	}
	return "claude"
}

// rawText flattens a tool_result content (string or [{type:text,text}]).
func rawText(raw json.RawMessage) string {
	var s string
	if json.Unmarshal(raw, &s) == nil {
		return s
	}
	var parts []struct {
		Text string `json:"text"`
	}
	_ = json.Unmarshal(raw, &parts)
	var b strings.Builder
	for _, p := range parts {
		b.WriteString(p.Text)
	}
	return b.String()
}

func oneLine(s string, n int) string {
	s = strings.Join(strings.Fields(s), " ")
	if r := []rune(s); len(r) > n {
		return string(r[:n-1]) + "…"
	}
	return s
}

// Fetch runs the configured JQL and refreshes the cache.
func Fetch(cfg Config) (Cache, error) {
	cfg = cfg.WithDefaults()
	instr := fmt.Sprintf("Chame searchJiraIssuesUsingJql exatamente uma vez com cloudId=%q, jql=%q, "+
		"fields=[\"summary\",\"description\",\"status\",\"issuetype\",\"priority\",\"updated\"], "+
		"responseContentFormat=\"markdown\", maxResults=100. Depois responda apenas 'ok'.",
		cfg.CloudID, cfg.JQL)
	r, err := run(cfg, "searchJiraIssuesUsingJql", instr)
	if err != nil {
		return Cache{}, err
	}
	var res struct {
		Issues struct {
			Nodes []struct {
				Key    string `json:"key"`
				WebURL string `json:"webUrl"`
				Fields struct {
					Summary string          `json:"summary"`
					Desc    json.RawMessage `json:"description"`
					Updated string          `json:"updated"`
					Status  struct {
						Name string `json:"name"`
						Cat  struct {
							Key string `json:"key"`
						} `json:"statusCategory"`
					} `json:"status"`
					Type struct {
						Name string `json:"name"`
					} `json:"issuetype"`
					Priority struct {
						Name string `json:"name"`
					} `json:"priority"`
				} `json:"fields"`
			} `json:"nodes"`
		} `json:"issues"`
	}
	if err := json.Unmarshal([]byte(r.toolText), &res); err != nil {
		return Cache{}, fmt.Errorf("resposta do Jira inesperada: %v: %s", err, oneLine(r.toolText, 300))
	}
	c := Cache{FetchedAt: time.Now(), CostUSD: r.cost}
	for _, n := range res.Issues.Nodes {
		url := n.WebURL
		if url == "" && cfg.Site != "" {
			url = "https://" + cfg.Site + "/browse/" + n.Key
		}
		c.Issues = append(c.Issues, Issue{
			Key: n.Key, Summary: n.Fields.Summary, Status: n.Fields.Status.Name, StatusCat: n.Fields.Status.Cat.Key,
			Type: n.Fields.Type.Name, Priority: n.Fields.Priority.Name, Updated: n.Fields.Updated, URL: url,
			Desc: cleanDesc(n.Fields.Desc),
		})
	}
	saveCache(c)
	return c, nil
}

var (
	mdEscapeRe = regexp.MustCompile(`\\([\\\[\]~*_#+\-.!|{}()<>` + "`" + `])`)
	blobImgRe  = regexp.MustCompile(`!\[[^\]]*\]\(blob:[^)]*\)`)
	blankRunRe = regexp.MustCompile(`\n{3,}`)
)

// cleanDesc turns the markdown description into something readable in a
// terminal: no backslash escapes, no blob: image links (they only resolve in
// the browser). A null or ADF (non-string) description becomes "".
func cleanDesc(raw json.RawMessage) string {
	var s string
	if json.Unmarshal(raw, &s) != nil {
		return ""
	}
	s = blobImgRe.ReplaceAllString(s, "[imagem]")
	s = mdEscapeRe.ReplaceAllString(s, "$1")
	s = strings.ReplaceAll(s, "\u200c", "")
	s = blankRunRe.ReplaceAllString(s, "\n\n")
	return strings.TrimSpace(s)
}

var keyRe = regexp.MustCompile(`\b([A-Z][A-Z0-9]+-\d+)\b`)

// NewIssue is what the user confirmed in neko before anything is written.
type NewIssue struct {
	Project     string
	Type        string
	Summary     string
	Description string
	Labels      []string
}

// Create creates the issue assigned to the user and returns its key.
func Create(cfg Config, n NewIssue) (string, float64, error) {
	cfg = cfg.WithDefaults()
	args := map[string]any{
		"cloudId": cfg.CloudID, "projectKey": n.Project, "issueTypeName": n.Type,
		"summary": n.Summary, "description": n.Description, "contentFormat": "markdown",
	}
	if cfg.AccountID != "" {
		args["assignee_account_id"] = cfg.AccountID
	}
	if len(n.Labels) > 0 {
		args["additional_fields"] = map[string]any{"labels": n.Labels}
	}
	b, _ := json.Marshal(args)
	instr := "Chame createJiraIssue exatamente uma vez com exatamente estes argumentos (JSON), sem alterar nada: " +
		string(b) + "\nDepois responda apenas com a chave da issue criada."
	r, err := run(cfg, "createJiraIssue", instr)
	if err != nil {
		return "", r.cost, err
	}
	var res struct {
		Key string `json:"key"`
	}
	if json.Unmarshal([]byte(r.toolText), &res) == nil && res.Key != "" {
		return res.Key, r.cost, nil
	}
	if m := keyRe.FindStringSubmatch(r.toolText); m != nil {
		return m[1], r.cost, nil
	}
	return "", r.cost, fmt.Errorf("issue criada? não achei a chave na resposta: %s", oneLine(r.toolText, 160))
}

// AgentPrompt is the prompt for an agent dispatched to work on an issue.
func AgentPrompt(i Issue) string {
	desc := ""
	if i.Desc != "" {
		desc = "Descrição (cache do neko, pode estar desatualizada):\n" + i.Desc + "\n\n"
	}
	return fmt.Sprintf("Trabalhe na issue %s do Jira: %s\nLink: %s\n\n%s"+
		"Leia a issue atual (descrição e comentários) com as ferramentas do Atlassian antes de começar. "+
		"Antes de qualquer ação irreversível ou externa (merge, push, deploy, comentário ou transição no Jira), pare e me pergunte.",
		i.Key, i.Summary, i.URL, desc)
}
