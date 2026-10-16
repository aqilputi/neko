// Package paths is the single place that names nekuroddo's data
// directories. It has no internal imports so every package (including the
// hook, which runs on every Claude Code event) can use it cheaply.
package paths

import (
	"os"
	"path/filepath"
)

// AppName is the directory name under the XDG config, state and cache dirs.
const AppName = "nekuroddo"

// LegacyName is the directory name used before the rename; MigrateLegacy in
// package config moves those directories to AppName.
const LegacyName = "neko"

func base(env string, fallback ...string) string {
	if d := os.Getenv(env); d != "" {
		return d
	}
	home, _ := os.UserHomeDir()
	return filepath.Join(append([]string{home}, fallback...)...)
}

// ConfigBase is $XDG_CONFIG_HOME or ~/.config.
func ConfigBase() string { return base("XDG_CONFIG_HOME", ".config") }

// StateBase is $XDG_STATE_HOME or ~/.local/state.
func StateBase() string { return base("XDG_STATE_HOME", ".local", "state") }

// CacheBase is $XDG_CACHE_HOME or ~/.cache.
func CacheBase() string { return base("XDG_CACHE_HOME", ".cache") }

// ConfigDir is ~/.config/nekuroddo (or the XDG equivalent).
func ConfigDir() string { return filepath.Join(ConfigBase(), AppName) }

// StateDir is ~/.local/state/nekuroddo (or the XDG equivalent).
func StateDir() string { return filepath.Join(StateBase(), AppName) }

// CacheDir is ~/.cache/nekuroddo (or the XDG equivalent).
func CacheDir() string { return filepath.Join(CacheBase(), AppName) }
