package store

import (
	"testing"
	"time"

	"github.com/aqilputi/nekuroddo/internal/config"
)

func TestOverrides(t *testing.T) {
	now := time.Now()
	fakeAgents(t, now, now)
	cfg := config.Default()
	cfg.ShowHistoryDays = 0
	s := New(cfg)
	rule := cfg.ContextFor("/tmp")
	byID := func() map[string]Session {
		out := map[string]Session{}
		for _, ss := range s.Refresh().Sessions {
			out[ss.SessionID] = ss
		}
		return out
	}

	s.Rename("aaaa1111-x", "meu nome")
	s.SetContext("bbbb2222-x", "acme")
	got := byID()
	if got["aaaa1111-x"].Label() != "meu nome" || got["aaaa1111-x"].Context != rule {
		t.Fatalf("rename: %+v", got["aaaa1111-x"])
	}
	if got["bbbb2222-x"].Context != "acme" || !got["bbbb2222-x"].ContextSet {
		t.Fatalf("move: %+v", got["bbbb2222-x"])
	}

	// renaming a section renames it for its sessions, and survives a restart
	s.RenameContext(rule, "temp")
	s.RenameContext("temp", "temporário") // renaming the renamed one
	s = New(cfg)
	if got = byID(); got["aaaa1111-x"].Context != "temporário" || got["bbbb2222-x"].Context != "acme" {
		t.Fatalf("section rename: a=%q b=%q", got["aaaa1111-x"].Context, got["bbbb2222-x"].Context)
	}
	// sessions moved by hand into a section follow its rename
	s.RenameContext("acme", "trabalho")
	if got = byID(); got["bbbb2222-x"].Context != "trabalho" {
		t.Fatalf("moved session should follow: %q", got["bbbb2222-x"].Context)
	}

	// empty undoes everything
	s.Rename("aaaa1111-x", "")
	s.SetContext("bbbb2222-x", " ")
	s.RenameContext("temporário", "")
	got = byID()
	if got["aaaa1111-x"].Label() != "a" || got["aaaa1111-x"].Context != rule || got["bbbb2222-x"].Context != rule {
		t.Fatalf("undo: %+v", got)
	}
}
