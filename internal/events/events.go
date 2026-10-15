// Package events carries Claude Code hook events to neko in real time.
//
// `neko hook` is installed as a Claude Code hook command (Notification,
// Stop, SubagentStop, UserPromptSubmit, SessionStart, SessionEnd). It reads
// the hook payload from stdin and appends one compact line to
// ~/.local/state/neko/events.jsonl; neko tails that file and refreshes at
// once instead of waiting for the next poll. The hook prints nothing and
// always exits 0, so it can never block or alter a Claude session.
package events

import (
	"bufio"
	"bytes"
	"encoding/json"
	"io"
	"os"
	"path/filepath"
	"time"
)

type Event struct {
	At        time.Time `json:"at"`
	Name      string    `json:"event"` // hook_event_name
	SessionID string    `json:"session_id"`
	Cwd       string    `json:"cwd,omitempty"`
	Type      string    `json:"type,omitempty"` // notification_type
	Message   string    `json:"message,omitempty"`
	AgentID   string    `json:"agent_id,omitempty"`
}

// NeedsYou reports whether the event means a session is waiting on the user.
func (e Event) NeedsYou() bool {
	if e.Name != "Notification" {
		return false
	}
	switch e.Type {
	case "permission_prompt", "idle_prompt", "agent_needs_input", "elicitation_dialog", "worker_permission_prompt":
		return true
	}
	return e.Type == "" // older payloads: any notification asks for attention
}

func Path() string {
	dir := os.Getenv("XDG_STATE_HOME")
	if dir == "" {
		home, _ := os.UserHomeDir()
		dir = filepath.Join(home, ".local", "state")
	}
	return filepath.Join(dir, "neko", "events.jsonl")
}

const maxSize = 4 << 20

// Record is the body of `neko hook`: payload in, one line out, never fails.
func Record(stdin io.Reader) {
	raw, _ := io.ReadAll(io.LimitReader(stdin, 1<<20))
	var p struct {
		Event   string `json:"hook_event_name"`
		Session string `json:"session_id"`
		Cwd     string `json:"cwd"`
		Type    string `json:"notification_type"`
		Message string `json:"message"`
		AgentID string `json:"agent_id"`
	}
	if json.Unmarshal(raw, &p) != nil || p.Event == "" {
		return
	}
	ev := Event{At: time.Now(), Name: p.Event, SessionID: p.Session, Cwd: p.Cwd, Type: p.Type, Message: p.Message, AgentID: p.AgentID}
	if r := []rune(ev.Message); len(r) > 300 {
		ev.Message = string(r[:300])
	}
	line, err := json.Marshal(ev)
	if err != nil {
		return
	}
	path := Path()
	_ = os.MkdirAll(filepath.Dir(path), 0o755)
	if st, err := os.Stat(path); err == nil && st.Size() > maxSize {
		_ = os.Rename(path, path+".1")
	}
	// One small O_APPEND write per event: lines from parallel sessions never interleave.
	f, err := os.OpenFile(path, os.O_CREATE|os.O_APPEND|os.O_WRONLY, 0o600)
	if err != nil {
		return
	}
	_, _ = f.Write(append(line, '\n'))
	_ = f.Close()
}

// Tail reads events appended since the last call.
type Tail struct {
	off     int64
	partial []byte
	started bool
}

// Poll returns new events. The first call skips history: only events that
// happen while neko runs matter.
func (t *Tail) Poll() []Event {
	f, err := os.Open(Path())
	if err != nil {
		return nil
	}
	defer f.Close()
	st, err := f.Stat()
	if err != nil {
		return nil
	}
	if !t.started {
		t.started, t.off = true, st.Size()
		return nil
	}
	if st.Size() < t.off { // rotated
		t.off, t.partial = 0, nil
	}
	if st.Size() == t.off {
		return nil
	}
	if _, err := f.Seek(t.off, io.SeekStart); err != nil {
		return nil
	}
	data, _ := io.ReadAll(f)
	t.off += int64(len(data))
	data = append(t.partial, data...)
	last := bytes.LastIndexByte(data, '\n')
	if last < 0 {
		t.partial = data
		return nil
	}
	t.partial = append([]byte(nil), data[last+1:]...)
	var out []Event
	sc := bufio.NewScanner(bytes.NewReader(data[:last]))
	for sc.Scan() {
		var e Event
		if json.Unmarshal(sc.Bytes(), &e) == nil {
			out = append(out, e)
		}
	}
	return out
}
