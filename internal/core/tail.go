package core

import "time"

// TailMax is the most lines of a session's activity tail a client can get
// (the transcript keeps as many: claude.TailKeep).
const TailMax = 200

// TailLine is one row of a session's activity tail: what the GUI scrolls on
// the cat's monitor instead of attaching a terminal. Kind is one of tool,
// text, user, result or sub (claude.TailTool…); Text is already rendered the
// way Claude Code shows it ("● Bash(go test ./...)", "  ⎿ 3 lines", "> …").
type TailLine struct {
	At   time.Time `json:"at"`
	Kind string    `json:"kind"`
	Text string    `json:"text"`
}

// ClampTail bounds how many tail lines a client asks for to 1..TailMax.
func ClampTail(n int) int { return min(max(n, 1), TailMax) }
