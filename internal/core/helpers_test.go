package core

import (
	"encoding/json"
	"hash/fnv"
	"reflect"
	"testing"
	"time"

	"github.com/aqilputi/nekuroddo/internal/claude"
	"github.com/aqilputi/nekuroddo/internal/store"
)

func TestTruncate(t *testing.T) {
	for _, c := range []struct {
		s    string
		n    int
		want string
	}{
		{"abc", 5, "abc"}, {"abc", 3, "abc"}, {"abcdef", 4, "abc…"}, {"abcdef", 1, "…"},
		{"abcdef", 0, "…"}, {"ãõéçü", 3, "ãõ…"}, {"abcd", 3, "ab…"}, {"ação!", 4, "açã…"}, {"", 0, ""},
	} {
		if got := Truncate(c.s, c.n); got != c.want {
			t.Errorf("Truncate(%q,%d)=%q want %q", c.s, c.n, got, c.want)
		}
	}
}

func TestClamp(t *testing.T) {
	for _, c := range []struct{ v, lo, hi, want int }{
		{5, 0, 3, 3}, {-1, 0, 3, 0}, {2, 0, 3, 2}, {7, 0, -1, 0},
	} {
		if got := Clamp(c.v, c.lo, c.hi); got != c.want {
			t.Errorf("Clamp(%d,%d,%d)=%d want %d", c.v, c.lo, c.hi, got, c.want)
		}
	}
}

func TestPctText(t *testing.T) {
	for _, c := range []struct {
		p    float64
		want string
	}{{0, ""}, {-1, ""}, {0.2, "<1%"}, {0.4, "<1%"}, {1, "1%"}, {12.4, "12%"}, {12.6, "13%"}, {99.6, "100%"}} {
		if got := PctText(c.p); got != c.want {
			t.Errorf("PctText(%v)=%q want %q", c.p, got, c.want)
		}
	}
}

func questions() []claude.Question {
	return []claude.Question{
		{Question: "q1", Options: []claude.Option{{Label: "sim"}, {Label: "não"}}},
		{Question: "q2", Options: []claude.Option{{Label: "talvez"}}},
	}
}

func TestOptionCount(t *testing.T) {
	if n := OptionCount(questions()); n != 3 {
		t.Fatalf("options across questions: %d", n)
	}
	if n := OptionCount(nil); n != 0 {
		t.Fatalf("no questions: %d", n)
	}
}

func TestResolveOption(t *testing.T) {
	qs := questions()
	for _, c := range []struct{ in, want string }{
		{"1", "sim"},
		{"2", "não"},
		{"3", "talvez"}, // numbering continues into the next question
		{"4", "4"},      // out of range: sent as typed
		{"0", "0"},
		{"02", "02"}, // not a bare number
		{"2 por favor", "2 por favor"},
		{"ok", "ok"},
		{"", ""},
	} {
		if got := ResolveOption(qs, c.in); got != c.want {
			t.Errorf("ResolveOption(%q)=%q want %q", c.in, got, c.want)
		}
	}
}

// The window's % is split by what each session consumed in the window.
func TestWindowPct(t *testing.T) {
	all := []float64{3, 1, 0}
	for _, c := range []struct {
		name  string
		pct   int
		cost  float64
		costs []float64
		want  float64
	}{
		{"a", 12, 3, all, 9},
		{"b", 12, 1, all, 3},
		{"idle session", 12, 0, all, 0},
		{"no usage report (pct 0)", 0, 3, all, 0},
		{"nothing used the window", 40, 2, nil, 0},
		{"zero total", 40, 2, []float64{0, 0}, 0},
		{"share", 40, 6, []float64{2, 6}, 30},
	} {
		if got := WindowPct(c.pct, c.cost, c.costs); got != c.want {
			t.Errorf("%s: WindowPct=%v want %v", c.name, got, c.want)
		}
	}
}

func TestRowKey(t *testing.T) {
	s := &store.Session{SessionID: "abc"}
	for _, c := range []struct {
		r    Row
		want string
	}{
		{Row{Kind: RowCtx, Ctx: "work"}, "ctx:work"},
		{Row{Kind: RowSess, Ctx: "work", Sess: s}, "sess:abc"},
		{Row{Kind: RowSub, Ctx: "work", Sess: s, Sub: &store.Sub{ID: "s1"}}, "sub:s1"},
	} {
		if got := RowKey(c.r); got != c.want {
			t.Errorf("RowKey(%v)=%q want %q", c.r.Kind, got, c.want)
		}
	}
}

// GroupColorIndex must match ui.groupColor's fnv hash + offset exactly so
// tags keep their colors when the ui switches to core.
func TestGroupColorIndex(t *testing.T) {
	ref := func(tag string) int {
		h := fnv.New32a()
		h.Write([]byte(tag))
		return int(h.Sum32()%97) + 3
	}
	for _, tag := range []string{"", "deploy-rc", "a", "refactor", "ção"} {
		got := GroupColorIndex(tag)
		if got != ref(tag) {
			t.Errorf("GroupColorIndex(%q)=%d want %d", tag, got, ref(tag))
		}
		if got < 3 || got > 99 {
			t.Errorf("GroupColorIndex(%q)=%d out of [3,99]", tag, got)
		}
		if got != GroupColorIndex(tag) {
			t.Errorf("GroupColorIndex(%q) not stable", tag)
		}
	}
	// pinned values: a change here changes every user's group colors
	for tag, want := range map[string]int{"": 67, "deploy-rc": 18, "refactor": 76} {
		if GroupColorIndex(tag) != want {
			t.Errorf("pinned %q: %d want %d", tag, GroupColorIndex(tag), want)
		}
	}
}

func TestSortInbox(t *testing.T) {
	a := &store.Session{SessionID: "a", Attention: store.AttnIdle}
	b := &store.Session{SessionID: "b", Attention: store.AttnBlocked}
	c := &store.Session{SessionID: "c", Attention: store.AttnBlocked}
	d := &store.Session{SessionID: "d", Attention: store.AttnReturned}
	for _, tc := range []struct {
		name  string
		in    []*store.Session
		order map[string]int
		want  []string
	}{
		{"attention, then frozen order", []*store.Session{a, b, c, d}, map[string]int{"sess:c": -1, "sess:b": 5}, []string{"c", "b", "d", "a"}},
		{"no order: stable", []*store.Session{a, c, b, d}, nil, []string{"c", "b", "d", "a"}},
		{"empty", nil, nil, []string{}},
	} {
		t.Run(tc.name, func(t *testing.T) {
			xs := append([]*store.Session(nil), tc.in...)
			SortInbox(xs, tc.order)
			got := []string{}
			for _, s := range xs {
				got = append(got, s.SessionID)
			}
			if !reflect.DeepEqual(got, tc.want) {
				t.Fatalf("got %v want %v", got, tc.want)
			}
		})
	}
}

// The data types travel over RPC as JSON with the plan's field names (§2.4–2.7, §3).
func TestDataTypesJSON(t *testing.T) {
	at := time.Date(2026, 10, 16, 12, 0, 0, 0, time.UTC)
	for _, c := range []struct {
		name string
		v    any
		want string
	}{
		{"command", Command{
			Name: "session.archive", Title: "arquiva/restaura", Hint: "X", Keys: []string{"X"}, Mode: ModeNormal,
			Args: []Arg{{Name: "id", Type: ArgSession, Prompt: "sessão", Required: true}}, Scope: ScopeCore, Confirm: true, Source: SourceBuiltin,
		}, `{"name":"session.archive","title":"arquiva/restaura","hint":"X","keys":["X"],"mode":"normal",` +
			`"args":[{"name":"id","type":"session","prompt":"sessão","required":true}],"scope":"core","confirm":true,"source":"builtin"}`},
		{"binding", Keymap{{Command: "nav.down", Keys: []string{"j", "down"}, Mode: ModeNormal, Help: Help{Key: "j/↓", Desc: "desce"}}},
			`[{"command":"nav.down","keys":["j","down"],"mode":"normal","help":{"key":"j/↓","desc":"desce"}}]`},
		{"event", Event{Seq: 7, At: at, Name: "toast", Data: map[string]any{"text": "oi"}},
			`{"seq":7,"at":"2026-10-16T12:00:00Z","name":"toast","data":{"text":"oi"}}`},
		{"pending", PendingConfirmation{Token: "t0k", Question: "arquivar?", ExpiresAt: at},
			`{"token":"t0k","question":"arquivar?","expires_at":"2026-10-16T12:00:00Z"}`},
		{"exec", ExecRequest{Argv: []string{"claude", "attach", "x"}, Dir: "/tmp"},
			`{"argv":["claude","attach","x"],"dir":"/tmp"}`},
	} {
		b, err := json.Marshal(c.v)
		if err != nil {
			t.Fatal(err)
		}
		if string(b) != c.want {
			t.Errorf("%s:\n got %s\nwant %s", c.name, b, c.want)
		}
	}
}

// ViewState is what POST /v1/view/tree sends back and forth.
func TestViewStateJSON(t *testing.T) {
	vs := NewViewState("custo")
	vs.Tab, vs.Filter, vs.ShowAll = TabArchive, "x", true
	vs.Collapsed["ctx:a"] = true
	vs.Order["ctx:a"] = 0
	b, err := json.Marshal(vs)
	if err != nil {
		t.Fatal(err)
	}
	var back ViewState
	if err := json.Unmarshal(b, &back); err != nil {
		t.Fatal(err)
	}
	if back.Tab != TabArchive || back.Filter != "x" || !back.ShowAll || back.SortMode != 1 ||
		!back.Collapsed["ctx:a"] || back.Order["ctx:a"] != 0 {
		t.Fatalf("round trip %s → %+v", b, back)
	}
	var m map[string]any
	_ = json.Unmarshal(b, &m)
	for _, k := range []string{"tab", "filter", "show_all", "sort", "order", "collapsed"} {
		if _, ok := m[k]; !ok {
			t.Errorf("missing %q in %s", k, b)
		}
	}
	if _, ok := m["inbox"]; ok {
		t.Error("inbox is derived; it does not travel with the state")
	}
}
