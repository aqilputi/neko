package paths

import (
	"path/filepath"
	"testing"
)

func TestDirs(t *testing.T) {
	cases := []struct {
		name     string
		env      string
		base     func() string
		dir      func() string
		fallback []string
	}{
		{"config", "XDG_CONFIG_HOME", ConfigBase, ConfigDir, []string{".config"}},
		{"state", "XDG_STATE_HOME", StateBase, StateDir, []string{".local", "state"}},
		{"cache", "XDG_CACHE_HOME", CacheBase, CacheDir, []string{".cache"}},
	}
	for _, c := range cases {
		t.Run(c.name+"/xdg", func(t *testing.T) {
			home, xdg := t.TempDir(), t.TempDir()
			t.Setenv("HOME", home)
			t.Setenv(c.env, xdg)
			if got := c.base(); got != xdg {
				t.Errorf("base = %q, want %q", got, xdg)
			}
			if got, want := c.dir(), filepath.Join(xdg, AppName); got != want {
				t.Errorf("dir = %q, want %q", got, want)
			}
		})
		t.Run(c.name+"/home-fallback", func(t *testing.T) {
			home := t.TempDir()
			t.Setenv("HOME", home)
			t.Setenv(c.env, "") // empty counts as unset
			want := filepath.Join(append([]string{home}, c.fallback...)...)
			if got := c.base(); got != want {
				t.Errorf("base = %q, want %q", got, want)
			}
			if got := c.dir(); got != filepath.Join(want, "nekuroddo") {
				t.Errorf("dir = %q, want %q", got, filepath.Join(want, "nekuroddo"))
			}
		})
	}
}

func TestNames(t *testing.T) {
	if AppName == LegacyName {
		t.Fatal("AppName and LegacyName must differ or the migration is a no-op")
	}
}
