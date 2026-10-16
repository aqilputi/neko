package app_test

import (
	"context"
	"testing"
	"time"

	"github.com/aqilputi/nekuroddo/internal/app"
	"github.com/aqilputi/nekuroddo/internal/app/apptest"
	"github.com/aqilputi/nekuroddo/internal/config"
	"github.com/aqilputi/nekuroddo/internal/store"
)

func summonApp(t *testing.T, ss ...store.Session) *app.App {
	t.Helper()
	f := apptest.New()
	a := app.New(config.Default(), f.Ports())
	t.Cleanup(a.Close)
	if err := a.RegisterBuiltins(); err != nil {
		t.Fatal(err)
	}
	f.Sessions.SetSnapshot(store.Snapshot{Sessions: ss})
	a.Refresh()
	return a
}

// inbox.summon (shift+space in the GUI, `nk run inbox.summon` from a
// compositor shortcut) picks the most urgent session: blocked before
// returned before idle, then the most recent activity.
func TestInboxSummonPicksTheMostUrgent(t *testing.T) {
	now := apptest.Epoch
	a := summonApp(t,
		store.Session{SessionID: "ret", Attention: store.AttnReturned, LastAt: now},
		store.Session{SessionID: "old-block", Attention: store.AttnBlocked, LastAt: now.Add(-time.Hour)},
		store.Session{SessionID: "new-block", Attention: store.AttnBlocked, LastAt: now.Add(-time.Minute)},
		store.Session{SessionID: "calm"},
	)
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	bus := a.Subscribe(ctx)

	res, err := a.Run(app.Context{}, "inbox.summon", nil)
	if err != nil {
		t.Fatal(err)
	}
	got, ok := res.Data.(app.InboxSummoned)
	if !ok || got.SessionID != "new-block" {
		t.Fatalf("Data = %#v, want the most recent blocked session", res.Data)
	}
	deadline := time.After(time.Second)
	for {
		select {
		case e := <-bus:
			if e.Name == app.EvInboxSummoned {
				if e.Data.(app.InboxSummoned).SessionID != "new-block" {
					t.Fatalf("event = %#v", e.Data)
				}
				return
			}
		case <-deadline:
			t.Fatal("inbox.summoned not published: clients that did not run it must open it too")
		}
	}
}

func TestInboxSummonWithNobodyWaiting(t *testing.T) {
	a := summonApp(t, store.Session{SessionID: "calm"})
	res, err := a.Run(app.Context{}, "inbox.summon", nil)
	if err != nil {
		t.Fatal(err)
	}
	if res.Data != nil || res.Toast == "" {
		t.Fatalf("nobody waiting: a toast and no target, got %#v", res)
	}
}
