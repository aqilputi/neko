// Package claude talks to the installed Claude Code CLI (never a copy of
// it): `claude agents --json` for the live session list, `claude --bg` to
// dispatch, `claude stop|rm` to manage. Transcripts under ~/.claude/projects
// supply what the CLI does not: cost, titles, last messages and subagents.
package claude

import (
	"encoding/json"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"strings"
	"time"
)

// Agent is one row of `claude agents --json --all`.
type Agent struct {
	ID        string `json:"id"`
	PID       int    `json:"pid"`
	Cwd       string `json:"cwd"`
	Kind      string `json:"kind"` // interactive | background
	StartedAt int64  `json:"startedAt"`
	SessionID string `json:"sessionId"`
	Name      string `json:"name"`
	State     string `json:"state"`  // background: running | blocked | done | stopped | …
	Status    string `json:"status"` // interactive: busy | idle | …
}

// AllowBypass puts "bypass permissions" in the shift+tab cycle of every
// session neko launches, without starting in it. There is no settings key
// for this in Claude Code: it has to be passed at launch.
const AllowBypass = "--allow-dangerously-skip-permissions"

// Bin is the claude executable; NEKO_CLAUDE overrides it.
func Bin() string {
	if b := os.Getenv("NEKO_CLAUDE"); b != "" {
		return b
	}
	return "claude"
}

// ListAgents returns active interactive and background sessions, plus
// finished background ones.
func ListAgents() ([]Agent, error) {
	out, err := exec.Command(Bin(), "agents", "--json", "--all").Output()
	if err != nil {
		return nil, fmt.Errorf("claude agents --json: %w", err)
	}
	var as []Agent
	if err := json.Unmarshal(out, &as); err != nil {
		return nil, fmt.Errorf("claude agents --json: %w", err)
	}
	return as, nil
}

var shortIDRe = regexp.MustCompile(`\b([0-9a-f]{8})\b`)

// Dispatch starts a background session in dir and returns its short id.
func Dispatch(dir, name, prompt string, extra ...string) (string, error) {
	args := []string{"--bg", AllowBypass}
	if name != "" {
		args = append(args, "--name", name)
	}
	args = append(args, extra...)
	args = append(args, prompt)
	cmd := exec.Command(Bin(), args...)
	cmd.Dir = dir
	out, err := cmd.CombinedOutput()
	if err != nil {
		return "", fmt.Errorf("claude --bg: %v: %s", err, strings.TrimSpace(string(out)))
	}
	if m := shortIDRe.FindStringSubmatch(string(out)); m != nil {
		return m[1], nil
	}
	return "", fmt.Errorf("claude --bg: no id in output: %s", strings.TrimSpace(string(out)))
}

// ResumeInBackground moves a finished/closed conversation into the daemon so
// it can be attached.
func ResumeInBackground(dir, sessionID string) (string, error) {
	cmd := exec.Command(Bin(), "--bg", AllowBypass, "--resume", sessionID)
	cmd.Dir = dir
	out, err := cmd.CombinedOutput()
	if err != nil {
		return "", fmt.Errorf("claude --bg --resume: %v: %s", err, strings.TrimSpace(string(out)))
	}
	if m := shortIDRe.FindStringSubmatch(string(out)); m != nil {
		return m[1], nil
	}
	return "", fmt.Errorf("claude --bg --resume: no id in output: %s", strings.TrimSpace(string(out)))
}

func Stop(id string) error   { return run("stop", id) }
func Remove(id string) error { return run("rm", id) }

func run(args ...string) error {
	out, err := exec.Command(Bin(), args...).CombinedOutput()
	if err != nil {
		return fmt.Errorf("claude %s: %v: %s", strings.Join(args, " "), err, strings.TrimSpace(string(out)))
	}
	return nil
}

// AttachArgv is what the embedded pane runs to open background session id.
func AttachArgv(id string) []string { return []string{Bin(), "attach", id} }

// ProjectsDir is ~/.claude/projects (CLAUDE_CONFIG_DIR respected).
func ProjectsDir() string {
	if d := os.Getenv("CLAUDE_CONFIG_DIR"); d != "" {
		return filepath.Join(d, "projects")
	}
	home, _ := os.UserHomeDir()
	return filepath.Join(home, ".claude", "projects")
}

var nonAlnum = regexp.MustCompile(`[^A-Za-z0-9]`)

// TranscriptPath finds the transcript of sessionID, trying the cwd-derived
// project folder first and falling back to a glob.
func TranscriptPath(cwd, sessionID string) string {
	p := filepath.Join(ProjectsDir(), nonAlnum.ReplaceAllString(cwd, "-"), sessionID+".jsonl")
	if _, err := os.Stat(p); err == nil {
		return p
	}
	if m, _ := filepath.Glob(filepath.Join(ProjectsDir(), "*", sessionID+".jsonl")); len(m) > 0 {
		return m[0]
	}
	return ""
}

// RecentTranscripts lists main-session transcripts modified since `since`.
func RecentTranscripts(since time.Time) []string {
	var out []string
	m, _ := filepath.Glob(filepath.Join(ProjectsDir(), "*", "*.jsonl"))
	for _, p := range m {
		if st, err := os.Stat(p); err == nil && st.ModTime().After(since) {
			out = append(out, p)
		}
	}
	return out
}
