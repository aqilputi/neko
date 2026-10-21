package app_test

import (
	"errors"
	"fmt"
	"testing"
	"time"

	"github.com/aqilputi/nekuroddo/internal/app"
	"github.com/aqilputi/nekuroddo/internal/claude"
	"github.com/aqilputi/nekuroddo/internal/core"
	"github.com/aqilputi/nekuroddo/internal/store"
)

func tailLines(n int) []claude.TailLine {
	out := make([]claude.TailLine, n)
	for i := range out {
		out[i] = claude.TailLine{At: time.Unix(int64(i), 0).UTC(), Kind: claude.TailText, Text: fmt.Sprintf("l%d", i)}
	}
	return out
}

func TestSessionTail(t *testing.T) {
	a, f := newApp(t)
	f.Sessions.SetSnapshot(store.Snapshot{Sessions: []store.Session{{SessionID: "a"}, {SessionID: "empty"}}})
	f.Sessions.SetTail("a", tailLines(250))
	a.Refresh()

	got, err := a.SessionTail("a", 3)
	if err != nil {
		t.Fatal(err)
	}
	want := []core.TailLine{
		{At: time.Unix(247, 0).UTC(), Kind: "text", Text: "l247"},
		{At: time.Unix(248, 0).UTC(), Kind: "text", Text: "l248"},
		{At: time.Unix(249, 0).UTC(), Kind: "text", Text: "l249"},
	}
	if fmt.Sprint(got) != fmt.Sprint(want) {
		t.Fatalf("SessionTail = %+v", got)
	}
	for n, wantLen := range map[int]int{0: 1, -3: 1, 80: 80, 1000: core.TailMax} {
		got, _ := a.SessionTail("a", n)
		if len(got) != wantLen {
			t.Errorf("SessionTail(a, %d) = %d lines, want %d", n, len(got), wantLen)
		}
		if last := f.Sessions.Last(); last.Method != "Tail" || last.Args[1] != wantLen {
			t.Errorf("port asked %+v, want n=%d", last, wantLen)
		}
	}

	// a known session with nothing yet: an empty (non-nil) list
	if got, err := a.SessionTail("empty", 10); err != nil || got == nil || len(got) != 0 {
		t.Fatalf("empty = %#v, %v", got, err)
	}
	if _, err := a.SessionTail("nope", 10); !errors.Is(err, app.ErrUnknownSession) {
		t.Fatalf("unknown session: %v", err)
	}
	f.Sessions.TailErr = errors.New("boom")
	if _, err := a.SessionTail("a", 10); err == nil || err.Error() != "boom" {
		t.Fatalf("port error: %v", err)
	}
}

func TestSessionTailEvent(t *testing.T) {
	a, f := newApp(t)
	ev := a.Subscribe(t.Context())
	snap := func(seqs map[string]int) {
		var ss []store.Session
		for _, id := range []string{"a", "b", "c", "d"} {
			if seq, ok := seqs[id]; ok {
				ss = append(ss, store.Session{SessionID: id, State: "busy", TailSeq: seq})
			}
		}
		f.Sessions.SetSnapshot(store.Snapshot{Sessions: ss})
		a.Refresh()
	}
	tails := func() map[string]int {
		out := map[string]int{}
		for _, e := range watchNamed(watchDrain(ev), app.EvSessionTail) {
			d := e.Data.(app.TailChanged)
			out[d.ID] = d.NNew
		}
		return out
	}

	snap(map[string]int{"a": 5, "b": 3})
	if got := tails(); len(got) != 0 {
		t.Fatalf("the first refresh only records: %v", got)
	}
	snap(map[string]int{"a": 7, "b": 3, "c": 4, "d": 0})
	if got := tails(); fmt.Sprint(got) != "map[a:2 c:4]" {
		t.Fatalf("second refresh: %v (a grew by 2, c is new with 4, b and d have nothing new)", got)
	}
	snap(map[string]int{"a": 7 + 3*core.TailMax, "c": 1})
	if got := tails(); fmt.Sprint(got) != "map[a:200 c:1]" {
		t.Fatalf("third refresh: %v (capped at TailMax; a counter that went back starts over)", got)
	}
	snap(map[string]int{"a": 7 + 3*core.TailMax, "b": 3})
	if got := tails(); fmt.Sprint(got) != "map[b:3]" {
		t.Fatalf("a session that left and came back is new again: %v", got)
	}
}
