package store

import (
	"sync"
	"testing"
	"time"

	"github.com/aqilputi/nekuroddo/internal/claude"
)

func TestTail(t *testing.T) {
	w := newWorld(t)
	now := time.Now()
	w.agents(claude.Agent{ID: "abcd1234", Kind: "background", Cwd: w.cwd, SessionID: "abcd1234-s",
		Name: "abcd1234", State: "running", StartedAt: now.Add(-time.Hour).UnixMilli()},
		claude.Agent{ID: "fresh123", Kind: "background", Cwd: w.cwd, SessionID: "fresh-s", State: "running",
			StartedAt: now.UnixMilli()})
	recs := []string{
		userRec(w.cwd, now.Add(-50*time.Minute), "arruma o build"),
		asstRec(now.Add(-40*time.Minute), "r1", "claude-opus-4", 1000, 10, "pronto"),
	}
	p := w.transcript("abcd1234-s", recs...)
	st := New(testCfg())
	ss := byID(st.Refresh())
	if got := ss["abcd1234-s"].TailSeq; got != 2 {
		t.Fatalf("TailSeq = %d, want 2", got)
	}
	if got := ss["fresh-s"].TailSeq; got != 0 {
		t.Fatalf("no transcript: TailSeq = %d", got)
	}
	lines, err := st.Tail("abcd1234-s", 10)
	if err != nil || len(lines) != 2 || lines[0].Text != "> arruma o build" || lines[1].Kind != claude.TailText {
		t.Fatalf("Tail = %+v, %v", lines, err)
	}
	if lines, err := st.Tail("abcd1234-s", 1); err != nil || len(lines) != 1 || lines[0].Text != "pronto" {
		t.Fatalf("Tail(1) = %+v, %v", lines, err)
	}
	// a session the store knows but with no transcript yet, and one it never saw
	for _, id := range []string{"fresh-s", "nope"} {
		if lines, err := st.Tail(id, 10); err != nil || len(lines) != 0 {
			t.Fatalf("Tail(%s) = %+v, %v", id, lines, err)
		}
	}

	w.write(p, joinLines(append(recs, asstRec(now.Add(-time.Minute), "r2", "claude-opus-4", 1000, 10, "mais"))))
	if got := byID(st.Refresh())["abcd1234-s"].TailSeq; got != 3 {
		t.Fatalf("after append TailSeq = %d, want 3", got)
	}
}

// Tail is served to clients while the refresh goroutine runs.
func TestTailConcurrentWithRefresh(t *testing.T) {
	w := newWorld(t)
	now := time.Now()
	w.agents(claude.Agent{ID: "abcd1234", Kind: "background", Cwd: w.cwd, SessionID: "abcd1234-s",
		State: "running", StartedAt: now.UnixMilli()})
	w.transcript("abcd1234-s", userRec(w.cwd, now, "oi"))
	st := New(testCfg())
	var wg sync.WaitGroup
	wg.Add(1)
	go func() {
		defer wg.Done()
		for range 50 {
			_, _ = st.Tail("abcd1234-s", 5)
		}
	}()
	for range 5 {
		st.Refresh()
	}
	wg.Wait()
}
