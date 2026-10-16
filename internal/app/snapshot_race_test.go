package app_test

import (
	"sync"
	"testing"

	"github.com/aqilputi/nekuroddo/internal/app"
)

// A client reading the snapshot (the rpc server, another goroutine of the
// TUI) while a command changes a session must not race: Snapshot hands out
// a copy, never the slice the commands write to. Run with -race.
func TestSnapshotIsSafeToReadWhileCommandsRun(t *testing.T) {
	a, _ := sessApp(t, sessFixture()...)
	var wg sync.WaitGroup
	wg.Add(2)
	go func() {
		defer wg.Done()
		for range 200 {
			for _, s := range a.Snapshot().Sessions {
				_ = s.Archived
				_ = s.CustomName
				_ = s.Context
			}
		}
	}()
	go func() {
		defer wg.Done()
		for i := range 200 {
			if _, err := a.ToggleArchive(app.Context{SessionID: "c"}, ""); err != nil {
				t.Error(err)
				return
			}
			if _, err := a.Rename("a", []string{"x", "y"}[i%2]); err != nil {
				t.Error(err)
				return
			}
		}
	}()
	wg.Wait()
}

// Changing a session through a command must not change a snapshot a client
// already holds (it asks for a new one).
func TestSnapshotIsACopy(t *testing.T) {
	a, _ := sessApp(t, sessFixture()...)
	held := a.Snapshot()
	if _, err := a.ToggleArchive(app.Context{SessionID: "c"}, ""); err != nil {
		t.Fatal(err)
	}
	for _, s := range held.Sessions {
		if s.SessionID == "c" && s.Archived {
			t.Fatal("a snapshot already handed out changed under its holder")
		}
	}
	if !sessByID(a, "c").Archived {
		t.Fatal("the core's own snapshot must show the change")
	}
}
