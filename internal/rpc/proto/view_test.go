package proto

import (
	"encoding/json"
	"strings"
	"testing"

	"github.com/aqilputi/nekuroddo/internal/core"
	"github.com/aqilputi/nekuroddo/internal/store"
)

func TestViewStateToCore(t *testing.T) {
	for _, tc := range []struct {
		tab     string
		want    core.Tab
		sort    string
		defSort string
		wantS   int
	}{
		{"", core.TabSessions, "", "custo", 1},
		{"sessions", core.TabSessions, "nome", "", 2},
		{"inbox", core.TabInbox, "atividade", "nome", 0},
		{"todos", core.TabTodos, "", "", 0},
		{"archive", core.TabArchive, "custo", "", 1},
	} {
		vs := ViewState{Tab: tc.tab, Sort: tc.sort, Filter: "f", ShowAll: true, Cursor: 3, CursorKey: "sess:x",
			InboxCursor: 1, NewRank: -2, Order: map[string]int{"ctx:a": 0}, Collapsed: map[string]bool{"ctx:a": true},
			CtxColor: map[string]int{"a": 4}}
		got, err := vs.Core(tc.defSort)
		if err != nil {
			t.Fatalf("%+v: %v", tc, err)
		}
		if got.Tab != tc.want || got.SortMode != tc.wantS || got.Filter != "f" || !got.ShowAll || got.Cursor != 3 ||
			got.CursorKey != "sess:x" || got.InboxCursor != 1 || got.NewRank != -2 || got.Order["ctx:a"] != 0 ||
			!got.Collapsed["ctx:a"] || got.CtxColor["a"] != 4 {
			t.Errorf("%+v → %+v", tc, got)
		}
	}
	// nil maps become empty ones
	got, err := ViewState{}.Core("")
	if err != nil || got.Order == nil || got.Collapsed == nil || got.CtxColor == nil {
		t.Errorf("zero state → %+v, %v", got, err)
	}
	// reset_order drops the frozen order
	got, _ = ViewState{Order: map[string]int{"ctx:a": 1}, NewRank: -3, ResetOrder: true}.Core("")
	if len(got.Order) != 0 || got.NewRank != 0 {
		t.Errorf("reset_order → %+v", got)
	}
	for _, bad := range []ViewState{{Tab: "nope"}, {Sort: "random"}} {
		if _, err := bad.Core(""); err == nil {
			t.Errorf("%+v: want an error", bad)
		}
	}
	// an unknown default sort is the core's fallback, not an error
	if got, err := (ViewState{}).Core("???"); err != nil || got.SortMode != 0 {
		t.Errorf("unknown default sort → %+v, %v", got, err)
	}
}

func TestFromViewState(t *testing.T) {
	vs := core.NewViewState("nome")
	vs.Tab, vs.Filter, vs.ShowAll, vs.Cursor, vs.CursorKey = core.TabArchive, "x", true, 2, "ctx:a"
	vs.Order["ctx:a"], vs.Collapsed["ctx:a"], vs.CtxColor["a"] = 5, true, 1
	vs.NewRank, vs.InboxCursor = -1, 1
	got := FromViewState(vs)
	if got.Tab != "archive" || got.Sort != "nome" || got.Filter != "x" || !got.ShowAll || got.Cursor != 2 ||
		got.CursorKey != "ctx:a" || got.Order["ctx:a"] != 5 || !got.Collapsed["ctx:a"] || got.CtxColor["a"] != 1 ||
		got.NewRank != -1 || got.InboxCursor != 1 || got.ResetOrder {
		t.Errorf("FromViewState = %+v", got)
	}
	// round trip
	back, err := got.Core("")
	if err != nil || back.Tab != vs.Tab || back.SortMode != vs.SortMode {
		t.Errorf("round trip = %+v, %v", back, err)
	}
	// out-of-range values: the first tab; the sort clamps like core.rankNew does
	odd := core.ViewState{Tab: core.Tab(42), SortMode: 99}
	if got := FromViewState(odd); got.Tab != "sessions" || got.Sort != "nome" {
		t.Errorf("odd = %+v", got)
	}
}

func TestTabNames(t *testing.T) {
	for tab := core.TabSessions; tab < core.NumTabs; tab++ {
		name := TabName(tab)
		back, ok := TabOf(name)
		if !ok || back != tab {
			t.Errorf("TabName(%d) = %q → %d, %v", tab, name, back, ok)
		}
	}
	if _, ok := TabOf("x"); ok {
		t.Error("TabOf(x) ok")
	}
}

func TestSubState(t *testing.T) {
	if SubState(store.Sub{}) != "busy" || SubState(store.Sub{Done: true}) != "done" {
		t.Error("SubState")
	}
}

func TestTreeJSON(t *testing.T) {
	b, _ := json.Marshal(Tree{Rows: []TreeRow{{Key: "ctx:a", Kind: "ctx", Ctx: "a", CtxCost: 1.5, CtxN: 2}}, State: ViewState{}, Inbox: []string{}})
	for _, k := range []string{`"key":"ctx:a"`, `"kind":"ctx"`, `"ctx":"a"`, `"ctx_cost":1.5`, `"ctx_n":2`,
		`"depth":0`, `"color":0`, `"attention":0`, `"inbox":[]`, `"hidden":0`, `"archived":0`, `"cursor":0`} {
		if !strings.Contains(string(b), k) {
			t.Errorf("%s missing in %s", k, b)
		}
	}
	for _, k := range []string{`"sub"`, `"mail"`, `"groups"`, `"session_id"`, `"reset_order"`} {
		if strings.Contains(string(b), k) {
			t.Errorf("%s should be omitted from %s", k, b)
		}
	}
}
