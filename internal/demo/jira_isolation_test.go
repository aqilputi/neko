package demo_test

import (
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/aqilputi/nekuroddo/internal/app"
	"github.com/aqilputi/nekuroddo/internal/config"
	"github.com/aqilputi/nekuroddo/internal/demo"
	"github.com/aqilputi/nekuroddo/internal/jira"
)

// nk serve --demo is meant for screenshots and GUI tests: it must never show
// the user's real Jira issues from the cache on disk, only the demo's.
func TestDemoNeverReadsTheUsersJiraCache(t *testing.T) {
	cache := t.TempDir()
	t.Setenv("XDG_CACHE_HOME", cache)
	t.Setenv("HOME", t.TempDir())
	onDisk, err := json.Marshal(jira.Cache{Issues: []jira.Issue{{Key: "REAL-1", Summary: "segredo da empresa"}}})
	if err != nil {
		t.Fatal(err)
	}
	if err := os.MkdirAll(filepath.Join(cache, "nekuroddo"), 0o700); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(cache, "nekuroddo", "jira.json"), onDisk, 0o600); err != nil {
		t.Fatal(err)
	}
	if len(jira.LoadCache().Issues) != 1 {
		t.Fatal("the real cache should be readable from disk for this test")
	}

	a := app.New(config.Default(), demo.Ports(nil))
	defer a.Close()
	for _, is := range a.JiraCache().Issues {
		if !strings.HasPrefix(is.Key, "DEMO-") {
			t.Fatalf("demo shows a non-demo issue: %s %q", is.Key, is.Summary)
		}
	}
	if len(a.JiraCache().Issues) == 0 {
		t.Fatal("demo should show its own issues")
	}
}
