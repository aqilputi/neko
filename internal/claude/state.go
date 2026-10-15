package claude

import (
	"encoding/json"
	"os"
	"path/filepath"
	"strconv"
)

// These files are written by Claude Code itself and are read-only for neko.
// The `<pid>.<hash>.key` files next to sessions/<pid>.json are socket
// secrets and are never opened.

func claudeDir() string {
	if d := os.Getenv("CLAUDE_CONFIG_DIR"); d != "" {
		return d
	}
	home, _ := os.UserHomeDir()
	return filepath.Join(home, ".claude")
}

type Option struct {
	Label       string `json:"label"`
	Description string `json:"description"`
}

type Question struct {
	Question string   `json:"question"`
	Options  []Option `json:"options"`
}

type Child struct {
	ID    string `json:"id"`
	Href  string `json:"href"`
	Kind  string `json:"kind"`
	Title string `json:"title"`
}

// Job is ~/.claude/jobs/<short-id>/state.json of a background session.
type Job struct {
	State          string  `json:"state"`
	Detail         string  `json:"detail"`
	Tempo          string  `json:"tempo"`
	Needs          string  `json:"needs"`
	SuggestedReply string  `json:"suggestedReply"`
	Tokens         int64   `json:"tokens"`
	Children       []Child `json:"children"`
	Block          *struct {
		Questions []Question `json:"questions"`
	} `json:"block"`
	InFlight *struct {
		Tasks  int `json:"tasks"`
		Queued int `json:"queued"`
	} `json:"inFlight"`
	Output *struct {
		Result string `json:"result"`
	} `json:"output"`
}

// ReadJob returns nil when the job file is missing or unreadable.
func ReadJob(shortID string) *Job {
	b, err := os.ReadFile(filepath.Join(claudeDir(), "jobs", filepath.Base(shortID), "state.json"))
	if err != nil {
		return nil
	}
	var j Job
	if json.Unmarshal(b, &j) != nil {
		return nil
	}
	return &j
}

// Live is ~/.claude/sessions/<pid>.json of a running interactive session.
type Live struct {
	Status     string `json:"status"`     // busy | shell | idle | waiting
	WaitingFor string `json:"waitingFor"` // "permission prompt", "input needed", …
}

func ReadLive(pid int) *Live {
	if pid <= 0 {
		return nil
	}
	b, err := os.ReadFile(filepath.Join(claudeDir(), "sessions", strconv.Itoa(pid)+".json"))
	if err != nil {
		return nil
	}
	var l Live
	if json.Unmarshal(b, &l) != nil {
		return nil
	}
	return &l
}
