package app

import (
	"fmt"
	"strconv"
)

// Context is the calling client's selection. The core never knows where
// anyone's cursor is: every command gets it from the client that ran it.
type Context struct {
	SessionID string   `json:"session_id,omitempty"` // session under the cursor
	Picked    []string `json:"picked,omitempty"`     // sessions picked with space
	Tab       string   `json:"tab,omitempty"`        // sessions | inbox | todos | archive
	TodoID    int      `json:"todo_id,omitempty"`    // todo under the cursor (todos tab)
	IssueKey  string   `json:"issue_key,omitempty"`  // Jira issue under the cursor
}

// Targets is what a session command acts on: the picked sessions if any,
// else the one under the cursor.
func (c Context) Targets() []string {
	if len(c.Picked) > 0 {
		return c.Picked
	}
	if c.SessionID != "" {
		return []string{c.SessionID}
	}
	return nil
}

// Args are a command's named arguments, as text (they come from a prompt, a
// key binding, Lua or JSON).
type Args map[string]string

// String is the argument or "".
func (a Args) String(name string) string { return a[name] }

// Int parses an integer argument.
func (a Args) Int(name string) (int, error) {
	s, ok := a[name]
	if !ok || s == "" {
		return 0, fmt.Errorf("%w: %s", ErrMissingArg, name)
	}
	n, err := strconv.Atoi(s)
	if err != nil {
		return 0, fmt.Errorf("argumento %s: %w", name, err)
	}
	return n, nil
}
