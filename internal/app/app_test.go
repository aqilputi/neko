package app_test

import (
	"errors"
	"testing"
	"time"

	"github.com/aqilputi/nekuroddo/internal/app"
	"github.com/aqilputi/nekuroddo/internal/app/apptest"
	"github.com/aqilputi/nekuroddo/internal/config"
	"github.com/aqilputi/nekuroddo/internal/core"
	"github.com/aqilputi/nekuroddo/internal/store"
)

func TestNewKeepsConfigPortsAndRegistry(t *testing.T) {
	cfg := config.Default()
	cfg.Sort = "nome-test"
	f := apptest.New()
	a := app.New(cfg, f.Ports())
	defer a.Close()
	if a.Config().Sort != "nome-test" {
		t.Fatal("Config")
	}
	if a.Ports().Todo != f.Todo || a.Clock() != f.Clock {
		t.Fatal("Ports/Clock")
	}
	if a.Registry() == nil || len(a.Registry().List(app.Context{})) != 0 {
		t.Fatal("Registry starts empty")
	}
}

func TestNewWithoutClockUsesWallClock(t *testing.T) {
	p := apptest.Ports()
	p.Clock = nil
	a := app.New(config.Default(), p)
	defer a.Close()
	if d := time.Since(a.Clock().Now()); d < 0 || d > time.Minute {
		t.Fatalf("clock off by %v", d)
	}
}

func TestRefreshPublishesSnapshot(t *testing.T) {
	a, f := newApp(t)
	if got := a.Snapshot(); len(got.Sessions) != 0 || !got.At.IsZero() {
		t.Fatalf("Snapshot before Refresh = %+v", got)
	}
	ev := a.Subscribe(t.Context())
	snap := store.Snapshot{Sessions: []store.Session{{SessionID: "a"}, {SessionID: "b"}}, At: apptest.Epoch}
	f.Sessions.SetSnapshot(snap)

	got := a.Refresh()
	if len(got.Sessions) != 2 || f.Sessions.Count("Refresh") != 1 {
		t.Fatalf("Refresh = %+v", got)
	}
	if s := a.Snapshot(); len(s.Sessions) != 2 {
		t.Fatalf("Snapshot after Refresh = %+v", s)
	}
	e := next(t, ev, app.EvSnapshotChanged)
	if e.Data != (app.SnapshotChanged{At: apptest.Epoch, Sessions: 2}) {
		t.Fatalf("snapshot.changed = %+v", e.Data)
	}

	f.Sessions.SetSnapshot(store.Snapshot{Err: errors.New("claude agents: boom")})
	a.Refresh()
	if e := next(t, ev, app.EvSnapshotChanged); e.Data.(app.SnapshotChanged).Err != "claude agents: boom" {
		t.Fatalf("error not carried: %+v", e.Data)
	}
}

func TestPublishAndToast(t *testing.T) {
	a, _ := newApp(t)
	ev := a.Subscribe(t.Context())
	a.Publish("custom", 42)
	if e := recv(t, ev); e.Name != "custom" || e.Data != 42 || e.Seq == 0 || !e.At.Equal(apptest.Epoch) {
		t.Fatalf("Publish = %+v", e)
	}
	a.Toast("falhou", true)
	if e := recv(t, ev); e.Name != app.EvToast || e.Data != (app.ToastData{Text: "falhou", Err: true}) {
		t.Fatalf("Toast = %+v", e)
	}
}

func TestAppRunPublishesCommandRan(t *testing.T) {
	a, _ := newApp(t)
	ev := a.Subscribe(t.Context())
	_ = a.Registry().Register(core.Command{Name: "x"}, echo("ok"))
	res, err := a.Run(app.Context{}, "x", nil)
	if err != nil || res.Toast != "ok" {
		t.Fatal(res, err)
	}
	if e := next(t, ev, app.EvCommandRan); e.Data != (app.CommandRan{Name: "x"}) {
		t.Fatalf("command.ran = %+v", e.Data)
	}
	if _, err := a.Run(app.Context{}, "nope", nil); !errors.Is(err, app.ErrUnknownCommand) {
		t.Fatal(err)
	}
	if e := next(t, ev, app.EvCommandRan); e.Data.(app.CommandRan).Err == "" {
		t.Fatalf("failed run must carry the error: %+v", e.Data)
	}
}

func TestCloseEndsSubscriptions(t *testing.T) {
	a, _ := newApp(t)
	ev := a.Subscribe(t.Context())
	a.Close()
	closedSoon(t, ev)
}

func TestResultHelpers(t *testing.T) {
	if r := app.Info("x"); r.Toast != "x" || r.Err {
		t.Fatal(r)
	}
	if r := app.Fail(errors.New("e")); r.Toast != "e" || !r.Err {
		t.Fatal(r)
	}
}

func TestArgs(t *testing.T) {
	a := app.Args{"name": "x", "n": "7", "bad": "z"}
	if a.String("name") != "x" || a.String("missing") != "" {
		t.Fatal("String")
	}
	if n, err := a.Int("n"); err != nil || n != 7 {
		t.Fatal(n, err)
	}
	if _, err := a.Int("bad"); err == nil {
		t.Fatal("Int of non-number")
	}
	if _, err := a.Int("missing"); !errors.Is(err, app.ErrMissingArg) {
		t.Fatalf("Int missing = %v", err)
	}
	var nilArgs app.Args
	if nilArgs.String("x") != "" {
		t.Fatal("nil Args")
	}
}

func TestContextTargets(t *testing.T) {
	if got := (app.Context{SessionID: "a"}).Targets(); len(got) != 1 || got[0] != "a" {
		t.Fatal(got)
	}
	if got := (app.Context{SessionID: "a", Picked: []string{"b", "c"}}).Targets(); len(got) != 2 || got[0] != "b" {
		t.Fatal(got)
	}
	if got := (app.Context{}).Targets(); got != nil {
		t.Fatal(got)
	}
}
