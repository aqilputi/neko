// Package proto holds the wire types of the core's protocol v1 (plan §3):
// what the server in internal/rpc writes and its Go client reads. Within v1
// changes are additive only: new fields are optional and clients ignore what
// they do not know.
package proto

import (
	"encoding/json"
	"time"

	"github.com/aqilputi/nekuroddo/internal/claude"
	"github.com/aqilputi/nekuroddo/internal/core"
	"github.com/aqilputi/nekuroddo/internal/groups"
	"github.com/aqilputi/nekuroddo/internal/jira"
	"github.com/aqilputi/nekuroddo/internal/store"
	"github.com/aqilputi/nekuroddo/internal/todo"
)

// Version is the protocol version in GET /v1/hello and in the /v1 prefix.
const Version = 1

// AppName is hello.app.
const AppName = "nekuroddo"

// Capabilities a server may announce in hello.
const (
	CapPanes = "panes"
	CapLua   = "lua"
	CapDemo  = "demo"
)

// Error codes in Error.Code.
const (
	CodeBadRequest     = "bad_request"
	CodeUnauthorized   = "unauthorized"
	CodeNotFound       = "not_found"
	CodeUnknownCommand = "unknown_command"
	CodeClientCommand  = "client_command"
	CodeUnavailable    = "unavailable"
	CodeMissingArg     = "missing_arg"
	CodeFailed         = "failed"
	CodeUnknownToken   = "unknown_token"
	CodeExpired        = "expired"
	CodeNotImplemented = "not_implemented"
)

// Error is the body of every non-2xx answer.
type Error struct {
	Error string `json:"error"`
	Code  string `json:"code"`
}

// Hello is GET /v1/hello.
type Hello struct {
	App          string   `json:"app"`
	Version      string   `json:"version"`
	Protocol     int      `json:"protocol"`
	Capabilities []string `json:"capabilities"`
	PID          int      `json:"pid"`
}

// Context is the calling client's selection (app.Context on the wire).
type Context struct {
	SessionID string   `json:"session_id,omitempty"`
	Picked    []string `json:"picked,omitempty"`
	Tab       string   `json:"tab,omitempty"`
	TodoID    int      `json:"todo_id,omitempty"`
	IssueKey  string   `json:"issue_key,omitempty"`
}

// CommandInfo is one entry of GET /v1/commands: the command plus whether it
// is available for the context the client sent.
type CommandInfo struct {
	core.Command
	Available bool `json:"available"`
}

// RunRequest is the body of POST /v1/commands/{name}.
type RunRequest struct {
	Args    map[string]string `json:"args,omitempty"`
	Context Context           `json:"context"`
}

// ConfirmRequest is the body of POST /v1/confirm/{token}.
type ConfirmRequest struct {
	Yes bool `json:"yes"`
}

// Result is what a command (or a confirmation) hands back.
type Result struct {
	Toast   string                    `json:"toast,omitempty"`
	Err     bool                      `json:"err,omitempty"`
	Pending *core.PendingConfirmation `json:"pending,omitempty"`
	Exec    *core.ExecRequest         `json:"exec,omitempty"`
	Data    json.RawMessage           `json:"data,omitempty"`
}

// Event is the data of one SSE message on GET /v1/events (its id: is Seq and
// its event: is Name).
type Event struct {
	Seq  uint64          `json:"seq"`
	At   time.Time       `json:"at"`
	Name string          `json:"name"`
	Data json.RawMessage `json:"data,omitempty"`
}

// EvGap is the SSE event sent (without an id) when a client resumes from a
// sequence number the server no longer keeps: events between Since and
// Oldest were lost, and the client should re-read the snapshot.
const EvGap = "gap"

// Gap is the data of a gap event.
type Gap struct {
	Since  uint64 `json:"since"`
	Oldest uint64 `json:"oldest"`
}

// Sub is a session's subagent.
type Sub struct {
	ID          string    `json:"id"`
	Description string    `json:"description,omitempty"`
	Type        string    `json:"type,omitempty"`
	Model       string    `json:"model,omitempty"`
	LastText    string    `json:"last_text,omitempty"`
	Cost        float64   `json:"cost,omitempty"`
	LastAt      time.Time `json:"last_at,omitzero"`
	Done        bool      `json:"done,omitempty"`
}

// Session is store.Session on the wire.
type Session struct {
	SessionID      string            `json:"session_id"`
	ShortID        string            `json:"short_id,omitempty"`
	Label          string            `json:"label"`
	Name           string            `json:"name,omitempty"`
	Title          string            `json:"title,omitempty"`
	CustomName     string            `json:"custom_name,omitempty"`
	Cwd            string            `json:"cwd"`
	Context        string            `json:"context,omitempty"`
	ContextSet     bool              `json:"context_set,omitempty"`
	Kind           string            `json:"kind"`
	State          string            `json:"state"`
	Attention      int               `json:"attention"`
	PID            int               `json:"pid,omitempty"`
	Started        time.Time         `json:"started,omitzero"`
	LastAt         time.Time         `json:"last_at,omitzero"`
	Model          string            `json:"model,omitempty"`
	Cost           float64           `json:"cost"`
	WindowCost     float64           `json:"window_cost,omitempty"`
	CostToday      float64           `json:"cost_today,omitempty"`
	Tokens         int64             `json:"tokens,omitempty"`
	FirstUser      string            `json:"first_user,omitempty"`
	LastUser       string            `json:"last_user,omitempty"`
	LastText       string            `json:"last_text,omitempty"`
	LastTool       string            `json:"last_tool,omitempty"`
	WaitingTool    string            `json:"waiting_tool,omitempty"`
	WaitingFor     string            `json:"waiting_for,omitempty"`
	Detail         string            `json:"detail,omitempty"`
	Needs          string            `json:"needs,omitempty"`
	SuggestedReply string            `json:"suggested_reply,omitempty"`
	Questions      []claude.Question `json:"questions,omitempty"`
	Links          []claude.Child    `json:"links,omitempty"`
	Result         string            `json:"result,omitempty"`
	Subs           []Sub             `json:"subs,omitempty"`
	Stale          bool              `json:"stale,omitempty"`
	Archived       bool              `json:"archived,omitempty"`
	ArchivedAt     time.Time         `json:"archived_at,omitzero"`
	Ended          bool              `json:"ended,omitempty"`
	Finished       bool              `json:"finished,omitempty"`
}

// FromSession converts a store session.
func FromSession(s store.Session) Session {
	out := Session{
		SessionID: s.SessionID, ShortID: s.ShortID, Label: s.Label(), Name: s.Name, Title: s.Title,
		CustomName: s.CustomName, Cwd: s.Cwd, Context: s.Context, ContextSet: s.ContextSet,
		Kind: s.Kind, State: s.State, Attention: int(s.Attention), PID: s.PID,
		Started: s.Started, LastAt: s.LastAt, Model: s.Model, Cost: s.Cost, WindowCost: s.WindowCost,
		CostToday: s.CostToday, Tokens: s.Tokens, FirstUser: s.FirstUser, LastUser: s.LastUser,
		LastText: s.LastText, LastTool: s.LastTool, WaitingTool: s.WaitingTool, WaitingFor: s.WaitingFor,
		Detail: s.Detail, Needs: s.Needs, SuggestedReply: s.SuggestedReply, Questions: s.Questions,
		Links: s.Links, Result: s.Result, Stale: s.Stale, Archived: s.Archived, ArchivedAt: s.ArchivedAt,
		Ended: s.Ended, Finished: s.Finished,
	}
	for _, b := range s.Subs {
		out.Subs = append(out.Subs, Sub{ID: b.ID, Description: b.Description, Type: b.Type, Model: b.Model,
			LastText: b.LastText, Cost: b.Cost, LastAt: b.LastAt, Done: b.Done})
	}
	return out
}

// MailCue is the ✉ next to a session.
type MailCue struct {
	At   time.Time `json:"at"`
	Sent bool      `json:"sent"`
}

// Limit is one line of the /usage report.
type Limit struct {
	Name   string `json:"name"`
	Pct    int    `json:"pct"`
	Resets string `json:"resets,omitempty"`
}

// Usage is the last /usage report.
type Usage struct {
	Limits []Limit   `json:"limits"`
	At     time.Time `json:"at,omitzero"`
	Err    string    `json:"err,omitempty"`
}

// FromUsage converts a /usage report and its fetch error.
func FromUsage(r claude.UsageReport, err error) Usage {
	u := Usage{Limits: []Limit{}, At: r.At}
	for _, l := range r.Limits {
		u.Limits = append(u.Limits, Limit{Name: l.Name, Pct: l.Pct, Resets: l.Resets})
	}
	if err != nil {
		u.Err = err.Error()
	}
	return u
}

// Todos is the todo list as last loaded.
type Todos struct {
	Groups []todo.Group `json:"groups"`
	Err    string       `json:"err,omitempty"`
}

// Snapshot is GET /v1/snapshot: everything a client needs to draw.
type Snapshot struct {
	Sessions []Session                  `json:"sessions"`
	Inbox    []string                   `json:"inbox"`
	Groups   map[string][]groups.Member `json:"groups"`
	Mail     map[string]MailCue         `json:"mail"`
	Usage    Usage                      `json:"usage"`
	Todos    Todos                      `json:"todos"`
	Jira     jira.Cache                 `json:"jira"`
	Pending  []core.PendingConfirmation `json:"pending"`
	At       time.Time                  `json:"at"`
	Err      string                     `json:"err,omitempty"`
	Today    float64                    `json:"today_cost"`
}

// PaneOpenRequest is the body of POST /v1/panes.
type PaneOpenRequest struct {
	SessionID string `json:"session_id"`
	Cols      int    `json:"cols,omitempty"`
	Rows      int    `json:"rows,omitempty"`
	Reply     string `json:"reply,omitempty"`
}

// PaneOpened is the answer of POST /v1/panes.
type PaneOpened struct {
	PaneID string `json:"pane_id"`
	Reused bool   `json:"reused,omitempty"`
}

// Pane is one entry of GET /v1/panes.
type Pane struct {
	ID        string `json:"id"`
	SessionID string `json:"session_id,omitempty"`
	Title     string `json:"title,omitempty"`
	Cols      int    `json:"cols"`
	Rows      int    `json:"rows"`
}

// Resize is the body of POST /v1/panes/{id}/resize.
type Resize struct {
	Cols int `json:"cols"`
	Rows int `json:"rows"`
}

// Control frame types (text frames of the pane WebSocket).
const (
	CtlResize = "resize" // client → server
	CtlTitle  = "title"  // server → client
	CtlExit   = "exit"   // server → client, last frame
)

// Control is a text frame of WS /v1/panes/{id}/ws.
type Control struct {
	Type string `json:"type"`
	Cols int    `json:"cols,omitempty"`
	Rows int    `json:"rows,omitempty"`
	Text string `json:"text,omitempty"`
	Err  string `json:"err,omitempty"`
}

// EvPaneResized is published when a pane's size changes (last write wins).
const EvPaneResized = "pane.resized"

// PaneResized is the payload of pane.resized.
type PaneResized struct {
	ID   string `json:"id"`
	Cols int    `json:"cols"`
	Rows int    `json:"rows"`
}
