package proto

import (
	"encoding/json"
	"errors"
	"strings"
	"testing"
	"time"

	"github.com/aqilputi/nekuroddo/internal/claude"
	"github.com/aqilputi/nekuroddo/internal/store"
)

func TestFromSession(t *testing.T) {
	at := time.Date(2026, 1, 2, 3, 4, 5, 0, time.UTC)
	s := store.Session{SessionID: "sid", ShortID: "bg", Name: "nome", CustomName: "meu", Cwd: "/r", Kind: "background",
		State: "blocked", Attention: store.AttnBlocked, Cost: 1.5, LastAt: at,
		Subs: []store.Sub{{ID: "a1", Description: "busca", Cost: 0.2, Done: true}}}
	got := FromSession(s)
	if got.SessionID != "sid" || got.Label != "meu" || got.Attention != 1 || got.Cost != 1.5 || !got.LastAt.Equal(at) {
		t.Fatalf("session = %+v", got)
	}
	if len(got.Subs) != 1 || got.Subs[0].ID != "a1" || !got.Subs[0].Done {
		t.Fatalf("subs = %+v", got.Subs)
	}
	b, _ := json.Marshal(got)
	for _, k := range []string{`"session_id":"sid"`, `"short_id":"bg"`, `"label":"meu"`, `"attention":1`} {
		if !strings.Contains(string(b), k) {
			t.Errorf("%s missing in %s", k, b)
		}
	}
	// zero times are left out
	if strings.Contains(string(b), `"started"`) {
		t.Errorf("zero started in %s", b)
	}
}

func TestFromUsage(t *testing.T) {
	u := FromUsage(claude.UsageReport{Limits: []claude.Limit{{Name: "Current session", Pct: 40, Resets: "1pm"}}}, nil)
	if len(u.Limits) != 1 || u.Limits[0].Pct != 40 || u.Err != "" {
		t.Fatalf("usage = %+v", u)
	}
	u = FromUsage(claude.UsageReport{}, errors.New("sem rede"))
	if u.Limits == nil || len(u.Limits) != 0 || u.Err != "sem rede" {
		t.Fatalf("usage err = %+v", u)
	}
}
