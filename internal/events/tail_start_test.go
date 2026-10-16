package events

import (
	"strings"
	"testing"
)

// A fresh install: no events.jsonl yet when nekuroddo starts. The first
// event a hook writes afterwards is new, not history, and must come out.
func TestTailFirstEventAfterMissingFile(t *testing.T) {
	t.Setenv("XDG_STATE_HOME", t.TempDir())
	var tail Tail
	if evs := tail.Poll(); len(evs) != 0 {
		t.Fatalf("no file: want nothing, got %v", evs)
	}
	Record(strings.NewReader(`{"hook_event_name":"Stop","session_id":"s1"}`))
	evs := tail.Poll()
	if len(evs) != 1 || evs[0].SessionID != "s1" {
		t.Fatalf("the first event after the file appeared was lost: %v", evs)
	}
}
