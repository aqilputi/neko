package core

import (
	"maps"
	"sort"
	"strings"
	"time"

	"github.com/sahilm/fuzzy"

	"github.com/aqilputi/nekuroddo/internal/store"
)

// Tab is the client's current tab. The tree only cares whether it is the
// archive (which lists archived sessions) or not.
type Tab int

// Tabs, in the TUI's order.
const (
	TabSessions Tab = iota
	TabInbox
	TabTodos
	TabArchive
	NumTabs
)

// SortModes are the tree's criteria, cycled by S.
var SortModes = []string{"atividade", "custo", "nome"}

// SortIndex is the index of a sort mode by name (config's `sort`); unknown
// names fall back to activity.
func SortIndex(name string) int {
	for i, n := range SortModes {
		if n == name {
			return i
		}
	}
	return 0
}

// Row kinds.
const (
	RowCtx  = "ctx"
	RowSess = "sess"
	RowSub  = "sub"
)

// Row is one line of the flattened tree. Sess and Sub point into the
// sessions slice given to BuildTree.
type Row struct {
	Kind    string // RowCtx | RowSess | RowSub
	Ctx     string
	Sess    *store.Session
	Sub     *store.Sub
	Depth   int
	CtxCost float64 // context rows: cost of the sessions listed under it
	CtxN    int     // context rows: how many sessions it lists
}

// RowKey identifies a row across rebuilds ("ctx:<name>", "sess:<id>",
// "sub:<id>"); the frozen order and folding use the same keys.
func RowKey(r Row) string {
	switch r.Kind {
	case RowCtx:
		return "ctx:" + r.Ctx
	case RowSess:
		return "sess:" + r.Sess.SessionID
	default:
		return "sub:" + r.Sub.ID
	}
}

// ViewState is everything the tree reads and writes between builds. It is a
// value owned by one client (the TUI today, the GUI over /v1/view/tree
// tomorrow); the core keeps no view state of its own.
type ViewState struct {
	Tab     Tab    `json:"tab"`
	Filter  string `json:"filter"`
	ShowAll bool   `json:"show_all"` // also stale/archived sessions and finished subagents
	// SortMode indexes SortModes.
	SortMode int `json:"sort"`
	// Order freezes the tree: each context/session gets a rank the first
	// time it shows up and keeps it, so rows don't jump as activity changes.
	// ResetOrder (S, R) re-sorts; newcomers go on top (NewRank counts down).
	Order   map[string]int `json:"order"`
	NewRank int            `json:"new_rank"`
	// Collapsed folds rows by key ("ctx:<name>", "sess:<id>").
	Collapsed map[string]bool `json:"collapsed"`
	// CtxColor is each context's palette index, in order of first sight.
	CtxColor map[string]int `json:"ctx_color"`
	// Cursor is the selected row; CursorKey is that row's key, which the
	// next build follows when rows move (set it with WithCursor).
	Cursor      int    `json:"cursor"`
	CursorKey   string `json:"cursor_key"`
	InboxCursor int    `json:"inbox_cursor"`

	// Outputs of the last build.
	Inbox     []*store.Session `json:"-"`        // sessions needing attention, sorted by SortInbox
	Hidden    int              `json:"hidden"`   // sessions the stale filter hid
	ArchivedN int              `json:"archived"` // sessions archived by hand
}

// NewViewState is an empty state sorted by the named criterion.
func NewViewState(sortMode string) ViewState {
	return ViewState{
		SortMode: SortIndex(sortMode),
		Order:    map[string]int{}, Collapsed: map[string]bool{}, CtxColor: map[string]int{},
	}
}

// ResetOrder makes the next build re-sort by the current criterion.
func (vs ViewState) ResetOrder() ViewState {
	vs.Order = map[string]int{}
	vs.NewRank = 0
	return vs
}

// NextSort cycles to the next criterion and resets the order (S).
func (vs ViewState) NextSort() ViewState {
	vs.SortMode = (vs.SortMode + 1) % len(SortModes)
	return vs.ResetOrder()
}

// WithCursor selects rows[i], remembering its key so the next build keeps
// the cursor on that row. Out of range clears the key.
func (vs ViewState) WithCursor(rows []Row, i int) ViewState {
	vs.Cursor, vs.CursorKey = i, ""
	if i >= 0 && i < len(rows) {
		vs.CursorKey = RowKey(rows[i])
	}
	return vs
}

type ctxGroup struct {
	ctx      string
	sessions []*store.Session
	last     time.Time
	cost     float64
}

// BuildTree flattens sessions into the rows a client shows: contexts, their
// sessions and running subagents, in the frozen order. It also fills the
// inbox and the hidden/archived counters, and moves the cursor along with
// the row it was on. Pure: vs's maps are copied before any change.
func BuildTree(sessions []store.Session, vs ViewState) ([]Row, ViewState) {
	vs.Order = cloneOr(vs.Order)
	vs.CtxColor = cloneOr(vs.CtxColor)
	if vs.Collapsed == nil {
		vs.Collapsed = map[string]bool{}
	}
	vs.Inbox = nil
	vs.Hidden, vs.ArchivedN = 0, 0
	f := strings.ToLower(vs.Filter)

	var groups []*ctxGroup
	byCtx := map[string]*ctxGroup{}
	for i := range sessions {
		s := &sessions[i]
		if f != "" && len(fuzzy.Find(f, []string{strings.ToLower(s.Label() + " " + s.Context + " " + s.Name + " " + s.Cwd)})) == 0 {
			continue
		}
		if s.Archived {
			vs.ArchivedN++
		}
		if vs.Tab == TabArchive {
			if !s.Archived {
				continue // the archive tab lists only what you archived
			}
		} else if s.Stale && !vs.ShowAll && f == "" {
			vs.Hidden++
			continue
		}
		if s.Attention != store.AttnNone {
			vs.Inbox = append(vs.Inbox, s)
		}
		g := byCtx[s.Context]
		if g == nil {
			g = &ctxGroup{ctx: s.Context}
			byCtx[s.Context] = g
			groups = append(groups, g)
			if _, ok := vs.CtxColor[s.Context]; !ok {
				vs.CtxColor[s.Context] = len(vs.CtxColor)
			}
		}
		g.sessions = append(g.sessions, s)
		g.cost += s.Cost
		if s.LastAt.After(g.last) {
			g.last = s.LastAt
		}
	}

	vs.rankNew(groups)

	var rows []Row
	sort.SliceStable(groups, func(i, j int) bool { return vs.Order["ctx:"+groups[i].ctx] < vs.Order["ctx:"+groups[j].ctx] })
	for _, g := range groups {
		sort.SliceStable(g.sessions, func(i, j int) bool {
			return vs.Order["sess:"+g.sessions[i].SessionID] < vs.Order["sess:"+g.sessions[j].SessionID]
		})
		rows = append(rows, Row{Kind: RowCtx, Ctx: g.ctx, CtxCost: g.cost, CtxN: len(g.sessions)})
		if vs.Collapsed["ctx:"+g.ctx] {
			continue
		}
		for _, s := range g.sessions {
			rows = append(rows, Row{Kind: RowSess, Ctx: s.Context, Sess: s, Depth: 1})
			if vs.Collapsed["sess:"+s.SessionID] {
				continue
			}
			for j := range s.Subs {
				// finished subagents are noise; H shows them
				if s.Subs[j].Done && !vs.ShowAll {
					continue
				}
				rows = append(rows, Row{Kind: RowSub, Ctx: s.Context, Sess: s, Sub: &s.Subs[j], Depth: 2})
			}
		}
	}
	SortInbox(vs.Inbox, vs.Order)

	vs.Cursor = min(vs.Cursor, max(len(rows)-1, 0))
	if vs.CursorKey != "" {
		for i, r := range rows {
			if RowKey(r) == vs.CursorKey {
				vs.Cursor = i
				break
			}
		}
	}
	vs = vs.WithCursor(rows, vs.Cursor)
	vs.InboxCursor = min(vs.InboxCursor, max(len(vs.Inbox)-1, 0))
	return rows, vs
}

// rankNew ranks whatever is new, in criterion order: on the first build
// everything is new and ranks 0..n; later arrivals get negative ranks (on
// top). vs.Order must already be vs's own copy.
func (vs *ViewState) rankNew(groups []*ctxGroup) {
	mode := SortModes[Clamp(vs.SortMode, 0, len(SortModes)-1)]
	less := func(a, b *store.Session) bool {
		switch mode {
		case "custo":
			return a.Cost > b.Cost
		case "nome":
			return strings.ToLower(a.Label()) < strings.ToLower(b.Label())
		}
		return a.LastAt.After(b.LastAt)
	}
	groupLess := func(a, b *ctxGroup) bool {
		switch mode {
		case "custo":
			return a.cost > b.cost
		case "nome":
			return a.ctx < b.ctx
		}
		return a.last.After(b.last)
	}

	fresh := len(vs.Order) == 0
	var newGroups []*ctxGroup
	for _, g := range groups {
		if _, ok := vs.Order["ctx:"+g.ctx]; !ok {
			newGroups = append(newGroups, g)
		}
	}
	sort.SliceStable(newGroups, func(i, j int) bool { return groupLess(newGroups[i], newGroups[j]) })
	vs.assignRanks(fresh, len(newGroups), func(i int) string { return "ctx:" + newGroups[i].ctx })
	for _, g := range groups {
		var newSess []*store.Session
		for _, s := range g.sessions {
			if _, ok := vs.Order["sess:"+s.SessionID]; !ok {
				newSess = append(newSess, s)
			}
		}
		sort.SliceStable(newSess, func(i, j int) bool { return less(newSess[i], newSess[j]) })
		vs.assignRanks(fresh, len(newSess), func(i int) string { return "sess:" + newSess[i].SessionID })
	}
}

// assignRanks gives n new keys a rank: 0..n-1 on a fresh order, otherwise
// below every existing rank so newcomers show on top, in criterion order.
func (vs *ViewState) assignRanks(fresh bool, n int, key func(int) string) {
	if fresh {
		base := len(vs.Order)
		for i := range n {
			vs.Order[key(i)] = base + i
		}
		return
	}
	for i := n - 1; i >= 0; i-- {
		vs.NewRank--
		vs.Order[key(i)] = vs.NewRank
	}
}

func cloneOr[V any](m map[string]V) map[string]V {
	if m == nil {
		return map[string]V{}
	}
	return maps.Clone(m)
}
