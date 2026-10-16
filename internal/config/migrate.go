package config

import (
	"os"
	"path/filepath"

	"github.com/aqilputi/nekuroddo/internal/paths"
)

// MigrateLegacy moves the pre-rename data directories (~/.config/neko,
// ~/.local/state/neko, ~/.cache/neko and their XDG equivalents) to their
// nekuroddo names. It runs at the start of every invocation, hooks
// included, so in the common case it costs three os.Lstat calls: a
// directory is renamed only when the new one does not exist and the old one
// does. It never fails; if a rename fails the program just uses the new
// (still empty) path.
func MigrateLegacy() {
	for _, b := range []string{paths.ConfigBase(), paths.StateBase(), paths.CacheBase()} {
		migrateDir(filepath.Join(b, paths.LegacyName), filepath.Join(b, paths.AppName))
	}
}

func migrateDir(from, to string) {
	if _, err := os.Lstat(to); !os.IsNotExist(err) {
		return // already migrated (or unreadable: leave it alone)
	}
	fi, err := os.Lstat(from)
	if err != nil || !fi.IsDir() {
		return
	}
	_ = os.Rename(from, to)
}
