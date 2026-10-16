package proto

import (
	"fmt"
	"maps"

	"github.com/aqilputi/nekuroddo/internal/core"
	"github.com/aqilputi/nekuroddo/internal/store"
)

// Tab names on the wire, in core.Tab order (the same names as the TUI's
// tab.* commands and app.Context.Tab).
var tabNames = [core.NumTabs]string{"sessions", "inbox", "todos", "archive"}

// TabName is the wire name of a tab ("sessions" for one out of range).
func TabName(t core.Tab) string {
	if t < 0 || t >= core.NumTabs {
		return tabNames[0]
	}
	return tabNames[t]
}

// TabOf parses a wire tab name.
func TabOf(name string) (core.Tab, bool) {
	for i, n := range tabNames {
		if n == name {
			return core.Tab(i), true
		}
	}
	return 0, false
}

// ViewState is a client's tree state (core.ViewState on the wire). The
// server keeps none: the client sends the state it got back last time and
// the next build continues from it (frozen order, palette, folding, cursor).
type ViewState struct {
	Tab     string `json:"tab,omitempty"` // sessions (default) | inbox | todos | archive
	Filter  string `json:"filter,omitempty"`
	ShowAll bool   `json:"show_all,omitempty"` // also stale/archived sessions and finished subagents
	Sort    string `json:"sort,omitempty"`     // atividade | custo | nome (default: the config's sort)
	// Order is the frozen rank of every row key ("ctx:<name>", "sess:<id>");
	// NewRank is the last rank given to a newcomer (they go on top).
	Order     map[string]int  `json:"order,omitempty"`
	NewRank   int             `json:"new_rank,omitempty"`
	Collapsed map[string]bool `json:"collapsed,omitempty"` // folded rows by key
	CtxColor  map[string]int  `json:"ctx_color,omitempty"` // context → palette index, in order of first sight
	Cursor    int             `json:"cursor"`
	// CursorKey is the selected row's key: the next build follows the row
	// if it moved.
	CursorKey   string `json:"cursor_key,omitempty"`
	InboxCursor int    `json:"inbox_cursor"`
	// ResetOrder (request only) re-sorts by the current criterion, like S/R
	// in the TUI; send it once with the new sort after changing it.
	ResetOrder bool `json:"reset_order,omitempty"`
}

// Core converts the wire state; defSort names the sort used when Sort is
// empty (the config's). Unknown tab or sort names are an error.
func (v ViewState) Core(defSort string) (core.ViewState, error) {
	vs := core.NewViewState(defSort)
	if v.Tab != "" {
		t, ok := TabOf(v.Tab)
		if !ok {
			return vs, fmt.Errorf("tab %q desconhecida (use sessions, inbox, todos ou archive)", v.Tab)
		}
		vs.Tab = t
	}
	if v.Sort != "" {
		i := core.SortIndex(v.Sort)
		if core.SortModes[i] != v.Sort {
			return vs, fmt.Errorf("sort %q desconhecido (use atividade, custo ou nome)", v.Sort)
		}
		vs.SortMode = i
	}
	vs.Filter, vs.ShowAll = v.Filter, v.ShowAll
	if v.Order != nil {
		vs.Order = maps.Clone(v.Order)
	}
	if v.Collapsed != nil {
		vs.Collapsed = maps.Clone(v.Collapsed)
	}
	if v.CtxColor != nil {
		vs.CtxColor = maps.Clone(v.CtxColor)
	}
	vs.NewRank, vs.Cursor, vs.CursorKey, vs.InboxCursor = v.NewRank, v.Cursor, v.CursorKey, v.InboxCursor
	if v.ResetOrder {
		vs = vs.ResetOrder()
	}
	return vs, nil
}

// FromViewState converts a built state back for the client.
func FromViewState(vs core.ViewState) ViewState {
	return ViewState{
		Tab: TabName(vs.Tab), Filter: vs.Filter, ShowAll: vs.ShowAll,
		Sort:  core.SortModes[core.Clamp(vs.SortMode, 0, len(core.SortModes)-1)],
		Order: vs.Order, NewRank: vs.NewRank, Collapsed: vs.Collapsed, CtxColor: vs.CtxColor,
		Cursor: vs.Cursor, CursorKey: vs.CursorKey, InboxCursor: vs.InboxCursor,
	}
}

// TreeRequest is the body of POST /v1/view/tree.
type TreeRequest struct {
	State ViewState `json:"state"`
}

// RowGroup is a group a session belongs to, with its collar color (an index
// into the theme's context palette: core.GroupColorIndex).
type RowGroup struct {
	Tag   string `json:"tag"`
	Color int    `json:"color"`
}

// TreeRow is one line of the tree (core.Row on the wire). Every row has
// key, kind, ctx, depth and color; ctx rows add ctx_cost/ctx_n; session rows
// add the session's fields (the rest is in the snapshot, by session_id);
// subagent rows carry the parent's session_id/short_id, the subagent's
// label (description), state (SubState) and cost, and sub.
type TreeRow struct {
	Key   string `json:"key"`   // core.RowKey: "ctx:<name>" | "sess:<id>" | "sub:<id>"
	Kind  string `json:"kind"`  // ctx | sess | sub
	Ctx   string `json:"ctx"`   // the context the row is under
	Depth int    `json:"depth"` // 0 ctx, 1 session, 2 subagent
	Color int    `json:"color"` // the context's palette index (ctx_color[ctx])
	// Collapsed: this ctx/session row is folded (its children are not listed).
	Collapsed bool `json:"collapsed,omitempty"`

	CtxCost float64 `json:"ctx_cost,omitempty"` // ctx: cost of the sessions listed under it
	CtxN    int     `json:"ctx_n,omitempty"`    // ctx: how many sessions it lists

	// Session rows (sub rows: see above).
	SessionID   string     `json:"session_id,omitempty"`
	ShortID     string     `json:"short_id,omitempty"`
	Label       string     `json:"label,omitempty"`
	SessionKind string     `json:"session_kind,omitempty"` // background | interactive | history
	State       string     `json:"state,omitempty"`        // busy | blocked | done | stopped | idle | history
	Attention   int        `json:"attention"`              // 0 none, 1 blocked, 2 returned, 3 idle
	Cost        float64    `json:"cost,omitempty"`
	Stale       bool       `json:"stale,omitempty"`
	Archived    bool       `json:"archived,omitempty"`
	Ended       bool       `json:"ended,omitempty"`
	Finished    bool       `json:"finished,omitempty"`
	SubsRunning int        `json:"subs_running,omitempty"` // subagents not done
	Options     int        `json:"options,omitempty"`      // numbered options of the pending questions
	Groups      []RowGroup `json:"groups,omitempty"`
	Mail        *MailCue   `json:"mail,omitempty"`       // ✉ cue (sent or received), while it shows
	MailState   string     `json:"mail_state,omitempty"` // MailBlink | MailDim

	Sub *Sub `json:"sub,omitempty"` // sub rows only
}

// How a ✉ cue shows on a row (app.CueBlink / app.CueDim); a cue past
// app.MailKeep is left out of the row.
const (
	MailBlink = "blink"
	MailDim   = "dim"
)

// SubState is a subagent row's state: "done" or "busy".
func SubState(b store.Sub) string {
	if b.Done {
		return "done"
	}
	return "busy"
}

// Tree is the answer of POST /v1/view/tree: the rows, the state to send
// back next time, and the build's side outputs.
type Tree struct {
	Rows     []TreeRow `json:"rows"`
	State    ViewState `json:"state"`
	Inbox    []string  `json:"inbox"`    // session ids needing attention, core.SortInbox order
	Hidden   int       `json:"hidden"`   // sessions the stale filter hid
	Archived int       `json:"archived"` // sessions archived by hand
}

// Theme is GET /v1/theme: one palette as "#rrggbb" tokens plus the names
// of every theme (for a picker).
type Theme struct {
	Name     string            `json:"name"`
	Tokens   map[string]string `json:"tokens"`   // bg, surface, …, cat_eyes
	Contexts []string          `json:"contexts"` // context/group palette; entry i % len
	Themes   []string          `json:"themes"`   // every theme name, in cycling order
	Active   string            `json:"active"`   // the theme the core's config asks for
}

// LuaEvalRequest is the body of POST /v1/lua/eval.
type LuaEvalRequest struct {
	Code string `json:"code"`
}

// LuaEval is the answer of POST /v1/lua/eval: the result's text, or the
// Lua error (both with 200: the call itself worked).
type LuaEval struct {
	Result string `json:"result,omitempty"`
	Error  string `json:"error,omitempty"`
}
