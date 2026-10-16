package core

import (
	"reflect"
	"testing"
	"time"

	"github.com/aqilputi/nekuroddo/internal/store"
)

// fixture mirrors ui's bhSessions: two contexts; "a" waits on you, "b" is
// busy with subagents, "c" returned, "d" is stale.
func fixture() []store.Session {
	now := time.Now()
	return []store.Session{
		{SessionID: "a", ShortID: "aaaa1111", Name: "alpha", Context: "c1", Kind: "background", State: "blocked",
			Attention: store.AttnBlocked, LastAt: now, Cwd: "/tmp", Cost: 2},
		{SessionID: "b", Name: "bravo", Context: "c1", Kind: "interactive", State: "busy", PID: 42, LastAt: now.Add(-time.Minute), Cwd: "/tmp",
			Cost: 3, Subs: []store.Sub{{ID: "s1"}, {ID: "s2", Done: true}}},
		{SessionID: "c", Name: "charlie", Context: "c2", Kind: "history", State: "done",
			Attention: store.AttnReturned, LastAt: now.Add(-2 * time.Minute), Cwd: "/tmp", Cost: 1},
		{SessionID: "d", Name: "delta", Context: "c2", Kind: "history", State: "idle", Stale: true,
			LastAt: now.Add(-100 * time.Hour), Cwd: "/tmp"},
	}
}

func keys(rows []Row) []string {
	out := []string{}
	for _, r := range rows {
		out = append(out, RowKey(r))
	}
	return out
}

func sessIDs(rows []Row) []string {
	out := []string{}
	for _, r := range rows {
		if r.Kind == RowSess {
			out = append(out, r.Sess.SessionID)
		}
	}
	return out
}

func inboxIDs(vs ViewState) []string {
	out := []string{}
	for _, s := range vs.Inbox {
		out = append(out, s.SessionID)
	}
	return out
}

// selectSess points the cursor at the session row with that id, the way the
// TUI will before rebuilding.
func selectSess(t *testing.T, rows []Row, vs ViewState, id string) ViewState {
	t.Helper()
	for i, r := range rows {
		if r.Kind == RowSess && r.Sess.SessionID == id {
			return vs.WithCursor(rows, i)
		}
	}
	t.Fatalf("no row for %s in %v", id, keys(rows))
	return vs
}

func TestBuildTreeCases(t *testing.T) {
	cases := []struct {
		name      string
		sessions  []store.Session
		edit      func(*ViewState)
		wantKeys  []string
		wantInbox []string
		hidden    int
		archived  int
	}{
		{
			name:      "layout: contexts, sessions, running subs; stale hidden; inbox blocked > returned",
			sessions:  fixture(),
			wantKeys:  []string{"ctx:c1", "sess:a", "sess:b", "sub:s1", "ctx:c2", "sess:c"},
			wantInbox: []string{"a", "c"},
			hidden:    1,
		},
		{
			name:      "filter finds stale sessions too",
			sessions:  fixture(),
			edit:      func(vs *ViewState) { vs.Filter = "delta" },
			wantKeys:  []string{"ctx:c2", "sess:d"},
			wantInbox: []string{},
		},
		{
			name:      "filter matches nothing",
			sessions:  fixture(),
			edit:      func(vs *ViewState) { vs.Filter = "zzzzqqq" },
			wantKeys:  []string{},
			wantInbox: []string{},
		},
		{
			name:      "filter is case-insensitive and looks at the context and cwd",
			sessions:  fixture(),
			edit:      func(vs *ViewState) { vs.Filter = "C2" },
			wantKeys:  []string{"ctx:c2", "sess:c", "sess:d"},
			wantInbox: []string{"c"},
		},
		{
			name:     "show all: stale sessions and finished subagents",
			sessions: fixture(),
			edit:     func(vs *ViewState) { vs.ShowAll = true },
			wantKeys: []string{"ctx:c1", "sess:a", "sess:b", "sub:s1", "sub:s2", "ctx:c2", "sess:c", "sess:d"},
			// d is idle: no attention
			wantInbox: []string{"a", "c"},
		},
		{
			name:      "folded context keeps its row only",
			sessions:  fixture(),
			edit:      func(vs *ViewState) { vs.Collapsed = map[string]bool{"ctx:c1": true} },
			wantKeys:  []string{"ctx:c1", "ctx:c2", "sess:c"},
			wantInbox: []string{"a", "c"},
			hidden:    1,
		},
		{
			name:      "folded session hides its subagents",
			sessions:  fixture(),
			edit:      func(vs *ViewState) { vs.Collapsed = map[string]bool{"sess:b": true} },
			wantKeys:  []string{"ctx:c1", "sess:a", "sess:b", "ctx:c2", "sess:c"},
			wantInbox: []string{"a", "c"},
			hidden:    1,
		},
		{
			name: "archive tab lists only archived sessions, stale or not",
			sessions: append(fixture(),
				store.Session{SessionID: "z", Context: "old", Archived: true, Stale: true},
				store.Session{SessionID: "y", Context: "old", Archived: true, Attention: store.AttnIdle}),
			edit:      func(vs *ViewState) { vs.Tab = TabArchive },
			wantKeys:  []string{"ctx:old", "sess:z", "sess:y"}, // equal activity: stable
			wantInbox: []string{"y"},
			archived:  2,
		},
		{
			name: "other tabs hide archived (stale) sessions but count them",
			sessions: append(fixture(),
				store.Session{SessionID: "z", Context: "old", Archived: true, Stale: true}),
			edit:      func(vs *ViewState) { vs.Tab = TabInbox },
			wantKeys:  []string{"ctx:c1", "sess:a", "sess:b", "sub:s1", "ctx:c2", "sess:c"},
			wantInbox: []string{"a", "c"},
			hidden:    2,
			archived:  1,
		},
		{
			name:      "no sessions",
			wantKeys:  []string{},
			wantInbox: []string{},
		},
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			vs := NewViewState("")
			if c.edit != nil {
				c.edit(&vs)
			}
			rows, out := BuildTree(c.sessions, vs)
			if got := keys(rows); !reflect.DeepEqual(got, c.wantKeys) {
				t.Fatalf("rows %v want %v", got, c.wantKeys)
			}
			if got := inboxIDs(out); !reflect.DeepEqual(got, c.wantInbox) {
				t.Fatalf("inbox %v want %v", got, c.wantInbox)
			}
			if out.Hidden != c.hidden || out.ArchivedN != c.archived {
				t.Fatalf("hidden %d archived %d, want %d %d", out.Hidden, out.ArchivedN, c.hidden, c.archived)
			}
		})
	}
}

func TestBuildTreeContextRow(t *testing.T) {
	rows, _ := BuildTree(fixture(), NewViewState(""))
	r := rows[0]
	if r.Kind != RowCtx || r.Ctx != "c1" || r.CtxN != 2 || r.CtxCost != 5 || r.Depth != 0 {
		t.Fatalf("context row %+v", r)
	}
	if rows[1].Depth != 1 || rows[1].Ctx != "c1" || rows[3].Depth != 2 || rows[3].Sub.ID != "s1" || rows[3].Sess.SessionID != "b" {
		t.Fatalf("session/sub rows %+v %+v", rows[1], rows[3])
	}
	// rows point into the caller's slice, like the TUI's did
	ss := fixture()
	rows, _ = BuildTree(ss, NewViewState(""))
	if rows[1].Sess != &ss[0] {
		t.Fatal("rows must point at the given sessions")
	}
}

// The cursor follows the selected row when rows move.
func TestBuildTreeCursorFollowsRow(t *testing.T) {
	ss := fixture()
	rows, vs := BuildTree(ss, NewViewState(""))
	vs = selectSess(t, rows, vs, "c")
	vs.Collapsed["ctx:c1"] = true
	rows, vs = BuildTree(ss, vs)
	if r := rows[vs.Cursor]; r.Sess == nil || r.Sess.SessionID != "c" || vs.CursorKey != "sess:c" {
		t.Fatalf("cursor should stay on c: %v at %d", keys(rows), vs.Cursor)
	}
}

// Without a key (or when the row is gone) the cursor index is clamped.
func TestBuildTreeCursorClamp(t *testing.T) {
	ss := fixture()
	for _, c := range []struct {
		name   string
		cursor int
		key    string
		filter string
		want   int
		wkey   string
	}{
		{"index past the end", 99, "", "", 5, "sess:c"},
		{"index kept", 2, "", "", 2, "sess:b"},
		{"key gone: clamp", 4, "sess:a", "charl", 1, "sess:c"},
		{"no rows", 3, "sess:a", "zzzzqqq", 0, ""},
	} {
		t.Run(c.name, func(t *testing.T) {
			vs := NewViewState("")
			vs.Cursor, vs.CursorKey, vs.Filter = c.cursor, c.key, c.filter
			_, out := BuildTree(ss, vs)
			if out.Cursor != c.want || out.CursorKey != c.wkey {
				t.Fatalf("cursor %d %q want %d %q", out.Cursor, out.CursorKey, c.want, c.wkey)
			}
		})
	}
}

func TestBuildTreeInboxCursorClamp(t *testing.T) {
	vs := NewViewState("")
	vs.InboxCursor = 9
	_, out := BuildTree(fixture(), vs)
	if out.InboxCursor != 1 {
		t.Fatalf("inbox cursor clamps to the last: %d", out.InboxCursor)
	}
	vs.Filter = "zzzzqqq"
	_, out = BuildTree(fixture(), vs)
	if out.InboxCursor != 0 {
		t.Fatalf("empty inbox: cursor 0, got %d", out.InboxCursor)
	}
}

func TestBuildTreeSortModes(t *testing.T) {
	now := time.Now()
	ss := []store.Session{
		{SessionID: "x", Name: "zulu", Context: "k2", Cost: 1, LastAt: now},
		{SessionID: "y", Name: "alpha", Context: "k1", Cost: 9, LastAt: now.Add(-time.Hour)},
		{SessionID: "z", Name: "mike", Context: "k1", Cost: 5, LastAt: now.Add(-2 * time.Hour)},
	}
	for _, c := range []struct {
		mode string
		want []string
	}{
		{"atividade", []string{"x", "y", "z"}},
		{"custo", []string{"y", "z", "x"}}, // k1 costs 14 > k2 1
		{"nome", []string{"y", "z", "x"}},  // k1 < k2, alpha < mike
	} {
		t.Run(c.mode, func(t *testing.T) {
			rows, _ := BuildTree(ss, NewViewState(c.mode))
			if got := sessIDs(rows); !reflect.DeepEqual(got, c.want) {
				t.Fatalf("got %v want %v", got, c.want)
			}
		})
	}
	// cycling S: next criterion with a reset order re-sorts
	_, vs := BuildTree(ss, NewViewState(""))
	for _, c := range []struct {
		mode string
		want []string
	}{
		{"custo", []string{"y", "z", "x"}},
		{"nome", []string{"y", "z", "x"}},
		{"atividade", []string{"x", "y", "z"}},
	} {
		vs = vs.NextSort()
		if SortModes[vs.SortMode] != c.mode {
			t.Fatalf("NextSort: %s want %s", SortModes[vs.SortMode], c.mode)
		}
		var rows []Row
		rows, vs = BuildTree(ss, vs)
		if got := sessIDs(rows); !reflect.DeepEqual(got, c.want) {
			t.Fatalf("%s: got %v want %v", c.mode, got, c.want)
		}
	}
	// without a reset, a new criterion does not move anything (frozen)
	vs.SortMode = SortIndex("custo")
	rows, _ := BuildTree(ss, vs)
	if got := sessIDs(rows); !reflect.DeepEqual(got, []string{"x", "y", "z"}) {
		t.Fatalf("frozen order should hold: %v", got)
	}
}

// Activity must not reshuffle the tree; newcomers go on top; a reset re-sorts.
func TestBuildTreeOrderIsFrozen(t *testing.T) {
	now := time.Now()
	ss := []store.Session{
		{SessionID: "a", Context: "c", LastAt: now.Add(-1 * time.Minute)},
		{SessionID: "b", Context: "c", LastAt: now.Add(-2 * time.Minute)},
		{SessionID: "c", Context: "c", LastAt: now.Add(-3 * time.Minute),
			Subs: []store.Sub{{ID: "s1", Done: true}, {ID: "s2"}}},
	}
	rows, vs := BuildTree(ss, NewViewState(""))
	if got := sessIDs(rows); !reflect.DeepEqual(got, []string{"a", "b", "c"}) {
		t.Fatalf("initial order %v", got)
	}
	ss[2].LastAt = now
	rows, vs = BuildTree(ss, vs)
	if got := sessIDs(rows); !reflect.DeepEqual(got, []string{"a", "b", "c"}) {
		t.Fatalf("order jumped: %v", got)
	}
	subs := 0
	for _, r := range rows {
		if r.Kind == RowSub {
			subs++
			if r.Sub.Done {
				t.Fatal("finished subagent shown")
			}
		}
	}
	if subs != 1 {
		t.Fatalf("want 1 running subagent row, got %d", subs)
	}
	ss = append(ss, store.Session{SessionID: "d", Context: "c", LastAt: now.Add(-time.Hour)},
		store.Session{SessionID: "e", Context: "c", LastAt: now.Add(-30 * time.Minute)})
	rows, vs = BuildTree(ss, vs)
	// newcomers on top, among themselves in criterion order (e is newer)
	if got := sessIDs(rows); !reflect.DeepEqual(got, []string{"e", "d", "a", "b", "c"}) {
		t.Fatalf("newcomer placement %v", got)
	}
	if vs.NewRank != -2 {
		t.Fatalf("newRank %d", vs.NewRank)
	}
	// a new context also lands on top
	ss = append(ss, store.Session{SessionID: "f", Context: "zz", LastAt: now.Add(-48 * time.Hour)})
	rows, vs = BuildTree(ss, vs)
	if got := keys(rows); got[0] != "ctx:zz" {
		t.Fatalf("new context should be first: %v", got)
	}
	rows, _ = BuildTree(ss, vs.ResetOrder())
	if got := sessIDs(rows); got[0] != "c" {
		t.Fatalf("after reset want c first, got %v", got)
	}
}

func TestBuildTreeFreshRanks(t *testing.T) {
	_, vs := BuildTree(fixture(), NewViewState(""))
	// contexts first (0..k-1), then each context's sessions, globally
	want := map[string]int{"ctx:c1": 0, "ctx:c2": 1, "sess:a": 2, "sess:b": 3, "sess:c": 4}
	if !reflect.DeepEqual(vs.Order, want) {
		t.Fatalf("order %v want %v", vs.Order, want)
	}
}

// BuildTree is pure: the maps it was given come back untouched.
func TestBuildTreeDoesNotMutateInput(t *testing.T) {
	vs := NewViewState("")
	vs.Order["sess:zzz"] = 7
	vs.CtxColor["elsewhere"] = 4
	vs.Collapsed["ctx:c1"] = true
	_, out := BuildTree(fixture(), vs)
	if len(vs.Order) != 1 || len(vs.CtxColor) != 1 || len(vs.Collapsed) != 1 {
		t.Fatalf("input maps mutated: %v %v %v", vs.Order, vs.CtxColor, vs.Collapsed)
	}
	if len(out.Order) <= 1 || out.CtxColor["c1"] != 1 || out.CtxColor["c2"] != 2 || out.CtxColor["elsewhere"] != 4 {
		t.Fatalf("output state %v %v", out.Order, out.CtxColor)
	}
	// nil maps in a zero ViewState are fine too
	rows, out := BuildTree(fixture(), ViewState{})
	if len(rows) == 0 || out.Order == nil || out.CtxColor == nil || out.Collapsed == nil {
		t.Fatalf("zero ViewState: rows %v state %+v", keys(rows), out)
	}
}

// A context keeps the color it got the first time it showed up.
func TestBuildTreeContextColors(t *testing.T) {
	ss := fixture()
	vs := NewViewState("")
	vs.Filter = "charlie"
	_, vs = BuildTree(ss, vs)
	if !reflect.DeepEqual(vs.CtxColor, map[string]int{"c2": 0}) {
		t.Fatalf("filtered contexts get no color: %v", vs.CtxColor)
	}
	vs.Filter = ""
	_, vs = BuildTree(ss, vs)
	if !reflect.DeepEqual(vs.CtxColor, map[string]int{"c2": 0, "c1": 1}) {
		t.Fatalf("colors %v", vs.CtxColor)
	}
}

func TestViewStateCursorHelpers(t *testing.T) {
	rows, vs := BuildTree(fixture(), NewViewState(""))
	vs = vs.WithCursor(rows, 3)
	if vs.Cursor != 3 || vs.CursorKey != "sub:s1" {
		t.Fatalf("WithCursor: %d %q", vs.Cursor, vs.CursorKey)
	}
	vs = vs.WithCursor(rows, 99)
	if vs.Cursor != 99 || vs.CursorKey != "" {
		t.Fatalf("out of range: no key: %d %q", vs.Cursor, vs.CursorKey)
	}
	vs = vs.WithCursor(rows, -1)
	if vs.CursorKey != "" {
		t.Fatal("negative: no key")
	}
}

func TestSortIndex(t *testing.T) {
	for _, c := range []struct {
		in   string
		want int
	}{{"atividade", 0}, {"custo", 1}, {"nome", 2}, {"", 0}, {"bogus", 0}} {
		if got := SortIndex(c.in); got != c.want {
			t.Errorf("SortIndex(%q)=%d want %d", c.in, got, c.want)
		}
	}
	if vs := NewViewState("nome"); vs.SortMode != 2 || vs.Order == nil || vs.Collapsed == nil || vs.CtxColor == nil {
		t.Fatalf("NewViewState %+v", vs)
	}
}

func TestResetOrder(t *testing.T) {
	vs := NewViewState("")
	vs.Order["sess:zzz"] = 99
	vs.NewRank = -4
	r := vs.ResetOrder()
	if len(r.Order) != 0 || r.NewRank != 0 {
		t.Fatalf("reset: %v %d", r.Order, r.NewRank)
	}
	if vs.Order["sess:zzz"] != 99 {
		t.Fatal("reset must not touch the original map")
	}
}
