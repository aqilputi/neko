package claude

import (
	"encoding/json"
	"fmt"
	"path/filepath"
	"strings"
	"time"
	"unicode/utf8"
)

// Kinds of activity line.
const (
	TailTool   = "tool"   // ● Bash(go test ./...)
	TailText   = "text"   // assistant text, one row per line
	TailUser   = "user"   // > the prompt
	TailResult = "result" //   ⎿ a short summary of a tool result
	TailSub    = "sub"    // ◆ Explore: a subagent started
)

const (
	// TailKeep is how many activity lines a transcript keeps.
	TailKeep = 200
	// TailWidth is the widest a line gets, in runes.
	TailWidth = 100

	textMaxLines = 8 // rows one assistant text block may take
	userMaxLines = 4 // rows one prompt may take
)

// TailLine is one row of a session's activity, rendered the way Claude Code
// shows it in the terminal.
type TailLine struct {
	At   time.Time
	Kind string
	Text string
}

// Activity returns the last n activity lines, oldest first (n <= 0: all
// that are kept). Safe to call while Refresh runs.
func (t *Transcript) Activity(n int) []TailLine {
	t.actMu.Lock()
	defer t.actMu.Unlock()
	keep := min(len(t.act), TailKeep)
	if n <= 0 || n > keep {
		n = keep
	}
	return append([]TailLine(nil), t.act[len(t.act)-n:]...)
}

// ActivitySeq counts every activity line ever parsed, so a caller can tell
// whether new ones arrived since it last looked.
func (t *Transcript) ActivitySeq() int {
	t.actMu.Lock()
	defer t.actMu.Unlock()
	return t.actSeq
}

// push appends to the ring. It holds up to 2×TailKeep lines and compacts to
// the last TailKeep when full, so the backing array never grows.
func (t *Transcript) push(at time.Time, kind string, texts ...string) {
	t.actMu.Lock()
	defer t.actMu.Unlock()
	for _, s := range texts {
		if t.act == nil {
			t.act = make([]TailLine, 0, 2*TailKeep)
		}
		if len(t.act) == cap(t.act) {
			t.act = append(t.act[:0], t.act[len(t.act)-TailKeep:]...)
		}
		t.act = append(t.act, TailLine{At: at, Kind: kind, Text: s})
		t.actSeq++
	}
}

// activityAssistant renders an assistant message's text and tool calls.
func (t *Transcript) activityAssistant(at time.Time, bs []block) {
	for _, b := range bs {
		switch b.Type {
		case "text":
			t.push(at, TailText, shapeText(b.Text, TailWidth, textMaxLines)...)
		case "tool_use":
			if t.toolNames == nil {
				t.toolNames = map[string]string{}
			}
			t.toolNames[b.ID] = b.Name
			if line, ok := subLine(b); ok {
				t.push(at, TailSub, line)
				continue
			}
			t.push(at, TailTool, toolLine(t.Cwd, b))
		}
	}
}

// activityUser renders a user record: a typed prompt or tool results.
func (t *Transcript) activityUser(at time.Time, bs []block, s string) {
	if s != "" {
		t.prompt(at, s)
		return
	}
	for _, b := range bs {
		switch b.Type {
		case "tool_result":
			name := t.toolNames[b.ToolUseID]
			delete(t.toolNames, b.ToolUseID)
			t.push(at, TailResult, resultLine(name, b.Content, b.IsError))
		case "text":
			t.prompt(at, b.Text)
		}
	}
}

// prompt renders what the user typed; harness-injected text is skipped.
func (t *Transcript) prompt(at time.Time, s string) {
	s = strings.TrimSpace(s)
	if s == "" || strings.HasPrefix(s, "<") || preamble(s) {
		return
	}
	// a prompt's own indentation is noise: each row gets the "> " gutter
	rows := strings.Split(s, "\n")
	for i := range rows {
		rows[i] = strings.TrimSpace(rows[i])
	}
	ls := shapeText(strings.Join(rows, "\n"), TailWidth-2, userMaxLines)
	for i := range ls {
		if i == 0 {
			ls[i] = "> " + ls[i]
		} else {
			ls[i] = "  " + ls[i]
		}
	}
	t.push(at, TailUser, ls...)
}

// preamble: text the harness puts in a user turn that nobody typed.
func preamble(s string) bool {
	for _, p := range []string{"Base directory for this skill:", "[Request interrupted", "Caveat:", "[structured-output"} {
		if strings.HasPrefix(s, p) {
			return true
		}
	}
	return false
}

// subLine renders an Agent/Task call as a subagent start.
func subLine(b block) (string, bool) {
	if b.Name != "Agent" && b.Name != "Task" {
		return "", false
	}
	var in struct {
		Description string `json:"description"`
		Type        string `json:"subagent_type"`
	}
	_ = json.Unmarshal(b.Input, &in)
	typ := in.Type
	if typ == "" {
		typ = b.Name
	}
	return clip("◆ "+typ+": "+strings.Join(strings.Fields(in.Description), " "), TailWidth), true
}

// toolLine renders a tool call as Claude Code does: ● Name(argument).
func toolLine(cwd string, b block) string {
	var in map[string]any
	_ = json.Unmarshal(b.Input, &in)
	str := func(k string) string { v, _ := in[k].(string); return v }
	var arg string
	switch b.Name {
	case "Read", "Write", "Edit", "MultiEdit":
		arg = relPath(cwd, str("file_path"))
	case "NotebookEdit", "NotebookRead":
		arg = relPath(cwd, str("notebook_path"))
	case "Grep":
		if p := str("pattern"); p != "" {
			arg = fmt.Sprintf("%q", p)
		}
	case "WebSearch":
		if q := str("query"); q != "" {
			arg = fmt.Sprintf("%q", q)
		}
	case "TodoWrite":
		if todos, ok := in["todos"].([]any); ok {
			arg = fmt.Sprintf("%d todos", len(todos))
		}
	case "AskUserQuestion":
		if qs, ok := in["questions"].([]any); ok && len(qs) > 0 {
			if q, ok := qs[0].(map[string]any); ok {
				arg, _ = q["question"].(string)
			}
		}
	default:
		for _, k := range []string{"command", "file_path", "pattern", "url", "query", "description", "prompt"} {
			if arg = str(k); arg != "" {
				break
			}
		}
	}
	arg = strings.Join(strings.Fields(arg), " ")
	if arg == "" {
		return "● " + b.Name
	}
	return clip("● "+b.Name+"("+arg+")", TailWidth)
}

// relPath shows a path under cwd relative to it, as Claude Code does.
func relPath(cwd, p string) string {
	if cwd == "" || p == "" {
		return p
	}
	if r, err := filepath.Rel(cwd, p); err == nil && r != "." && !strings.HasPrefix(r, "..") {
		return r
	}
	return p
}

// readLike tools return content whose size, not first line, is the news.
var readLike = map[string]bool{"Read": true, "Grep": true, "Glob": true, "NotebookRead": true, "LS": true}

// resultLine summarises a tool result in one row: its first line (and how
// many more), its line count for read-like tools, or the error.
func resultLine(tool string, content json.RawMessage, isErr bool) string {
	var text string
	if json.Unmarshal(content, &text) != nil {
		var bs []block
		if json.Unmarshal(content, &bs) == nil {
			var parts []string
			for _, b := range bs {
				if b.Type == "text" {
					parts = append(parts, b.Text)
				}
			}
			text = strings.Join(parts, "\n")
		}
	}
	var lines []string
	for l := range strings.SplitSeq(text, "\n") {
		if l = expand(l); strings.TrimSpace(l) != "" {
			lines = append(lines, strings.TrimRight(l, " "))
		}
	}
	var s string
	switch {
	case isErr && len(lines) == 0:
		s = "Error"
	case isErr:
		s = "Error: " + strings.TrimSpace(lines[0]) + more(len(lines)-1)
	case len(lines) == 0:
		s = "(no output)"
	case len(lines) > 1 && readLike[tool]:
		s = fmt.Sprintf("%d lines", len(lines))
	default:
		s = strings.TrimSpace(lines[0]) + more(len(lines)-1)
	}
	return clip("  ⎿ "+s, TailWidth)
}

func more(n int) string {
	switch n {
	case 0:
		return ""
	case 1:
		return " … +1 line"
	}
	return fmt.Sprintf(" … +%d lines", n)
}

// expand turns tabs into two spaces, as the terminal shows code.
func expand(s string) string { return strings.ReplaceAll(s, "\t", "  ") }

// shapeText splits text into rows of at most width runes: blank lines go,
// trailing space goes, indentation stays, long lines wrap at a word (or
// are cut). Past maxRows rows, the last one says how many were left out.
func shapeText(text string, width, maxRows int) []string {
	var out []string
	for l := range strings.SplitSeq(text, "\n") {
		l = strings.TrimRight(expand(l), " \r")
		if strings.TrimSpace(l) == "" {
			continue
		}
		out = append(out, wrap(l, width)...)
	}
	if len(out) > maxRows {
		out = append(out[:maxRows-1], fmt.Sprintf("… +%d lines", len(out)-maxRows+1))
	}
	return out
}

func wrap(l string, width int) []string {
	var out []string
	for utf8.RuneCountInString(l) > width {
		r := []rune(l)
		cut := width
		if i := strings.LastIndex(string(r[:width+1]), " "); i > 0 && strings.TrimSpace(l[:i]) != "" {
			cut = utf8.RuneCountInString(l[:i])
		}
		out = append(out, strings.TrimRight(string(r[:cut]), " "))
		l = strings.TrimLeft(string(r[cut:]), " ")
	}
	if l != "" {
		out = append(out, l)
	}
	return out
}

// clip cuts s to n runes, the last one an ellipsis.
func clip(s string, n int) string {
	if r := []rune(s); len(r) > n {
		return string(r[:n-1]) + "…"
	}
	return s
}
