package tui

import (
	"fmt"
	"os"
	"path/filepath"
	"testing"
)

// TestMain points HOME and every XDG base at a throwaway directory before
// any test runs: a test that forgets its own isolation (a model over the
// real adapters marking a session seen, say) must never write to the
// developer's ~/.local/state/nekuroddo. Tests that set their own dirs with
// t.Setenv still win.
func TestMain(m *testing.M) {
	root, err := os.MkdirTemp("", "nekuroddo-tui-test")
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
	for k, sub := range map[string]string{
		"HOME": "home", "XDG_STATE_HOME": "state", "XDG_CONFIG_HOME": "config",
		"XDG_CACHE_HOME": "cache", "XDG_RUNTIME_DIR": "run", "CLAUDE_CONFIG_DIR": "claude",
	} {
		_ = os.Setenv(k, filepath.Join(root, sub))
	}
	code := m.Run()
	_ = os.RemoveAll(root)
	os.Exit(code)
}
