package app_test

import (
	"errors"
	"testing"
	"time"

	"github.com/aqilputi/nekuroddo/internal/app"
	"github.com/aqilputi/nekuroddo/internal/app/apptest"
	"github.com/aqilputi/nekuroddo/internal/config"
	"github.com/aqilputi/nekuroddo/internal/core"
)

func newApp(t *testing.T) (*app.App, *apptest.Fakes) {
	t.Helper()
	f := apptest.New()
	a := app.New(config.Default(), f.Ports())
	t.Cleanup(a.Close)
	return a, f
}

// next returns the next event named name, skipping others.
func next(t *testing.T, ch <-chan core.Event, name string) core.Event {
	t.Helper()
	for {
		if e := recv(t, ch); e.Name == name {
			return e
		}
	}
}

func TestAskConfirmRunsOnce(t *testing.T) {
	a, f := newApp(t)
	ev := a.Subscribe(t.Context())
	runs := 0
	res := a.Ask("arquivar 3 sessões?", func() (app.Result, error) {
		runs++
		return app.Info("arquivadas"), nil
	})
	p := res.Pending
	if p == nil || p.Question != "arquivar 3 sessões?" || p.Token == "" {
		t.Fatalf("Ask = %+v", res)
	}
	if !p.ExpiresAt.Equal(f.Clock.Now().Add(app.ConfirmTTL)) {
		t.Fatalf("ExpiresAt = %v", p.ExpiresAt)
	}
	if e := next(t, ev, app.EvConfirmRequested); e.Data.(core.PendingConfirmation) != *p {
		t.Fatalf("confirm.requested data = %+v", e.Data)
	}
	if got := a.Pending(); len(got) != 1 || got[0] != *p {
		t.Fatalf("Pending = %+v", got)
	}

	out, err := a.Confirm(p.Token)
	if err != nil || out.Toast != "arquivadas" || runs != 1 {
		t.Fatalf("Confirm = %+v, %v (runs %d)", out, err, runs)
	}
	if e := next(t, ev, app.EvConfirmResolved); e.Data != (app.ConfirmResolved{Token: p.Token, Outcome: app.OutcomeYes}) {
		t.Fatalf("confirm.resolved data = %+v", e.Data)
	}
	if _, err := a.Confirm(p.Token); !errors.Is(err, app.ErrUnknownToken) || runs != 1 {
		t.Fatalf("second Confirm = %v (runs %d)", err, runs)
	}
	if len(a.Pending()) != 0 {
		t.Fatal("token still pending")
	}
}

func TestConfirmActionError(t *testing.T) {
	a, _ := newApp(t)
	boom := errors.New("boom")
	res := a.Ask("?", func() (app.Result, error) { return app.Result{}, boom })
	if _, err := a.Confirm(res.Pending.Token); !errors.Is(err, boom) {
		t.Fatalf("Confirm err = %v", err)
	}
}

func TestTokensAreUnique(t *testing.T) {
	a, _ := newApp(t)
	seen := map[string]bool{}
	for range 50 {
		tok := a.Ask("?", func() (app.Result, error) { return app.Result{}, nil }).Pending.Token
		if seen[tok] || len(tok) < 16 {
			t.Fatalf("token %q repeated or short", tok)
		}
		seen[tok] = true
	}
}

func TestConfirmExpires(t *testing.T) {
	a, f := newApp(t)
	ev := a.Subscribe(t.Context())
	ran := false
	tok := a.Ask("?", func() (app.Result, error) { ran = true; return app.Result{}, nil }).Pending.Token

	f.Clock.Advance(app.ConfirmTTL - time.Second)
	if len(a.Pending()) != 1 {
		t.Fatal("expired too early")
	}
	f.Clock.Advance(time.Second)
	if _, err := a.Confirm(tok); !errors.Is(err, app.ErrExpired) || ran {
		t.Fatalf("Confirm after TTL = %v (ran %v)", err, ran)
	}
	if e := next(t, ev, app.EvConfirmResolved); e.Data.(app.ConfirmResolved).Outcome != app.OutcomeExpired {
		t.Fatalf("resolved = %+v", e.Data)
	}
	if _, err := a.Confirm(tok); !errors.Is(err, app.ErrUnknownToken) {
		t.Fatalf("expired token must be gone: %v", err)
	}
}

func TestExpiredAreSweptAndHidden(t *testing.T) {
	a, f := newApp(t)
	ev := a.Subscribe(t.Context())
	old := a.Ask("velho", func() (app.Result, error) { return app.Result{}, nil }).Pending.Token
	f.Clock.Advance(app.ConfirmTTL)
	if len(a.Pending()) != 0 {
		t.Fatal("Pending lists an expired confirmation")
	}
	a.Ask("novo", func() (app.Result, error) { return app.Result{}, nil })
	if e := next(t, ev, app.EvConfirmResolved); e.Data != (app.ConfirmResolved{Token: old, Outcome: app.OutcomeExpired}) {
		t.Fatalf("sweep resolved = %+v", e.Data)
	}
	if _, err := a.Confirm(old); !errors.Is(err, app.ErrUnknownToken) {
		t.Fatalf("swept token: %v", err)
	}
}

func TestCancel(t *testing.T) {
	a, _ := newApp(t)
	ev := a.Subscribe(t.Context())
	ran := false
	tok := a.Ask("?", func() (app.Result, error) { ran = true; return app.Result{}, nil }).Pending.Token
	if err := a.Cancel(tok); err != nil {
		t.Fatal(err)
	}
	if e := next(t, ev, app.EvConfirmResolved); e.Data != (app.ConfirmResolved{Token: tok, Outcome: app.OutcomeNo}) {
		t.Fatalf("resolved = %+v", e.Data)
	}
	if _, err := a.Confirm(tok); !errors.Is(err, app.ErrUnknownToken) || ran {
		t.Fatalf("Confirm after Cancel = %v", err)
	}
	if err := a.Cancel(tok); !errors.Is(err, app.ErrUnknownToken) {
		t.Fatalf("second Cancel = %v", err)
	}
}

func TestPendingSortedByExpiry(t *testing.T) {
	a, f := newApp(t)
	first := a.Ask("1", func() (app.Result, error) { return app.Result{}, nil }).Pending.Token
	f.Clock.Advance(time.Second)
	second := a.Ask("2", func() (app.Result, error) { return app.Result{}, nil }).Pending.Token
	p := a.Pending()
	if len(p) != 2 || p[0].Token != first || p[1].Token != second {
		t.Fatalf("Pending = %+v", p)
	}
}
