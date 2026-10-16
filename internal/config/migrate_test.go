package config

import (
	"os"
	"path/filepath"
	"testing"
)

func setBases(t *testing.T) (cfg, state, cache string) {
	t.Helper()
	root := t.TempDir()
	cfg, state, cache = filepath.Join(root, "config"), filepath.Join(root, "state"), filepath.Join(root, "cache")
	for _, d := range []string{cfg, state, cache} {
		if err := os.MkdirAll(d, 0o755); err != nil {
			t.Fatal(err)
		}
	}
	t.Setenv("HOME", root)
	t.Setenv("XDG_CONFIG_HOME", cfg)
	t.Setenv("XDG_STATE_HOME", state)
	t.Setenv("XDG_CACHE_HOME", cache)
	return
}

func write(t *testing.T, p, s string) {
	t.Helper()
	if err := os.MkdirAll(filepath.Dir(p), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(p, []byte(s), 0o644); err != nil {
		t.Fatal(err)
	}
}

func read(t *testing.T, p string) string {
	t.Helper()
	b, err := os.ReadFile(p)
	if err != nil {
		t.Fatalf("read %s: %v", p, err)
	}
	return string(b)
}

func TestMigrateLegacyMovesAll(t *testing.T) {
	cfg, state, cache := setBases(t)
	write(t, filepath.Join(cfg, "neko", "config.toml"), "theme = \"x\"")
	write(t, filepath.Join(state, "neko", "events.jsonl"), "{}")
	write(t, filepath.Join(cache, "neko", "jira.json"), "[]")

	MigrateLegacy()

	if got := read(t, filepath.Join(cfg, "nekuroddo", "config.toml")); got != "theme = \"x\"" {
		t.Fatalf("config = %q", got)
	}
	read(t, filepath.Join(state, "nekuroddo", "events.jsonl"))
	read(t, filepath.Join(cache, "nekuroddo", "jira.json"))
	for _, d := range []string{cfg, state, cache} {
		if _, err := os.Stat(filepath.Join(d, "neko")); !os.IsNotExist(err) {
			t.Fatalf("old dir still in %s: %v", d, err)
		}
	}
	if Path() != filepath.Join(cfg, "nekuroddo", "config.toml") {
		t.Fatalf("Path() = %s", Path())
	}

	MigrateLegacy() // idempotent
	read(t, filepath.Join(state, "nekuroddo", "events.jsonl"))
}

func TestMigrateLegacyKeepsExistingNew(t *testing.T) {
	_, state, _ := setBases(t)
	write(t, filepath.Join(state, "neko", "events.jsonl"), "old")
	write(t, filepath.Join(state, "nekuroddo", "events.jsonl"), "new")

	MigrateLegacy()

	if got := read(t, filepath.Join(state, "nekuroddo", "events.jsonl")); got != "new" {
		t.Fatalf("new dir overwritten: %q", got)
	}
	if got := read(t, filepath.Join(state, "neko", "events.jsonl")); got != "old" {
		t.Fatalf("old dir touched: %q", got)
	}
}

func TestMigrateLegacyNothingToDo(t *testing.T) {
	cfg, state, cache := setBases(t)
	MigrateLegacy()
	for _, d := range []string{cfg, state, cache} {
		if _, err := os.Stat(filepath.Join(d, "nekuroddo")); !os.IsNotExist(err) {
			t.Fatalf("created %s/nekuroddo without a legacy dir", d)
		}
	}
}

func TestMigrateLegacyIgnoresLegacyFile(t *testing.T) {
	_, state, _ := setBases(t)
	write(t, filepath.Join(state, "neko"), "not a dir")
	MigrateLegacy()
	if _, err := os.Stat(filepath.Join(state, "nekuroddo")); !os.IsNotExist(err) {
		t.Fatal("renamed a plain file")
	}
}
