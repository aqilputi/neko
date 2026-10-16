package core

import "time"

// Event is one message on app's bus (plan §2.5): "snapshot.changed",
// "session.state", "toast", … Seq grows monotonically per broker.
type Event struct {
	Seq  uint64    `json:"seq"`
	At   time.Time `json:"at"`
	Name string    `json:"name"`
	Data any       `json:"data"`
}

// PendingConfirmation is a command waiting for a yes (plan §2.6): any client
// may answer it by token, once, before it expires.
type PendingConfirmation struct {
	Token     string    `json:"token"`
	Question  string    `json:"question"`
	ExpiresAt time.Time `json:"expires_at"`
}

// ExecRequest asks the client to hand the whole terminal to a program
// (fullscreen attach) and come back when it exits.
type ExecRequest struct {
	Argv []string `json:"argv"`
	Dir  string   `json:"dir"`
}
