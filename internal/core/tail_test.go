package core

import (
	"encoding/json"
	"testing"
	"time"
)

func TestTailLineJSON(t *testing.T) {
	l := TailLine{At: time.Date(2026, 10, 15, 10, 0, 3, 0, time.UTC), Kind: "tool", Text: "● Bash(go test ./...)"}
	b, err := json.Marshal(l)
	if err != nil {
		t.Fatal(err)
	}
	if want := `{"at":"2026-10-15T10:00:03Z","kind":"tool","text":"● Bash(go test ./...)"}`; string(b) != want {
		t.Fatalf("json = %s, want %s", b, want)
	}
}

func TestClampTail(t *testing.T) {
	for in, want := range map[int]int{-5: 1, 0: 1, 1: 1, 80: 80, TailMax: TailMax, TailMax + 1: TailMax, 1 << 30: TailMax} {
		if got := ClampTail(in); got != want {
			t.Errorf("ClampTail(%d) = %d, want %d", in, got, want)
		}
	}
}
