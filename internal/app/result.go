package app

import "github.com/aqilputi/nekuroddo/internal/core"

// Result is what a command hands back to the client that ran it.
type Result struct {
	Toast   string                    `json:"toast,omitempty"`
	Err     bool                      `json:"err,omitempty"`
	Pending *core.PendingConfirmation `json:"pending,omitempty"` // {Token, Question, ExpiresAt}
	Exec    *core.ExecRequest         `json:"exec,omitempty"`    // {Argv, Dir}: the client takes over the terminal
	Data    any                       `json:"data,omitempty"`    // command-specific JSON-able payload
}

// Info is a result that only shows a toast.
func Info(text string) Result { return Result{Toast: text} }

// Fail is a result that shows err as an error toast.
func Fail(err error) Result { return Result{Toast: err.Error(), Err: true} }
