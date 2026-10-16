package store

import (
	"fmt"
	"os"
	"path/filepath"
	"testing"
	"time"

	"github.com/aqilputi/nekuroddo/internal/config"
)

// fakeAgents makes `claude agents --json` report two background sessions
// whose last activity is their start time.
func fakeAgents(t *testing.T, startA, startB time.Time) {
	dir := t.TempDir()
	bin := filepath.Join(dir, "claude")
	js := fmt.Sprintf(`[{"id":"aaaa1111","cwd":"/tmp","kind":"background","startedAt":%d,"sessionId":"aaaa1111-x","name":"a","state":"done"},`+
		`{"id":"bbbb2222","cwd":"/tmp","kind":"background","startedAt":%d,"sessionId":"bbbb2222-x","name":"b","state":"done"}]`,
		startA.UnixMilli(), startB.UnixMilli())
	os.WriteFile(bin, []byte("#!/bin/sh\necho '"+js+"'\n"), 0o755)
	t.Setenv("NEKO_CLAUDE", bin)
	t.Setenv("XDG_STATE_HOME", dir)
	t.Setenv("CLAUDE_CONFIG_DIR", dir)
}

func TestArchiveComesBackOnActivity(t *testing.T) {
	now := time.Now()
	fakeAgents(t, now.Add(-2*time.Hour), now) // b had activity just now
	cfg := config.Default()
	cfg.ShowHistoryDays = 0
	// v1 file format (sessionId → true) must still load
	os.MkdirAll(filepath.Join(os.Getenv("XDG_STATE_HOME"), "nekuroddo"), 0o755)
	os.WriteFile(filepath.Join(os.Getenv("XDG_STATE_HOME"), "nekuroddo", "archived.json"), []byte(`{"aaaa1111-x":true}`), 0o644)
	s := New(cfg)
	s.archived["bbbb2222-x"] = now.Add(-time.Hour) // archived before its latest activity

	snap := s.Refresh()
	got := map[string]bool{}
	for _, ss := range snap.Sessions {
		got[ss.SessionID] = ss.Archived
	}
	if !got["aaaa1111-x"] {
		t.Fatal("a (no activity since archiving) should stay archived")
	}
	if got["bbbb2222-x"] {
		t.Fatal("b (active after archiving) should come back")
	}
}
