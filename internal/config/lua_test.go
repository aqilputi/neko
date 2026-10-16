package config

import (
	"path/filepath"
	"strings"
	"testing"
	"time"
)

func TestLuaDefaults(t *testing.T) {
	setBases(t)
	c, err := Load()
	if err != nil {
		t.Fatal(err)
	}
	want := Lua{Enabled: true, CommandTimeoutMS: 5000, CallbackTimeoutMS: 50}
	if c.Lua != want || Default().Lua != want {
		t.Fatalf("[lua] defaults = %+v / %+v, want %+v", c.Lua, Default().Lua, want)
	}
	if c.Lua.CommandTimeout() != 5*time.Second || c.Lua.CallbackTimeout() != 50*time.Millisecond {
		t.Fatalf("durations: %v %v", c.Lua.CommandTimeout(), c.Lua.CallbackTimeout())
	}
}

func TestLuaSection(t *testing.T) {
	cfg, _, _ := setBases(t)
	write(t, filepath.Join(cfg, "nekuroddo", "config.toml"),
		"theme = \"x\"\n[lua]\nenabled = false\ncommand_timeout_ms = 900\ncallback_timeout_ms = 20\nauto_reload = true\n")
	c, err := Load()
	if err != nil {
		t.Fatal(err)
	}
	want := Lua{Enabled: false, CommandTimeoutMS: 900, CallbackTimeoutMS: 20, AutoReload: true}
	if c.Lua != want || c.Theme != "x" {
		t.Fatalf("[lua] = %+v", c.Lua)
	}
}

// A section with only some keys keeps the defaults of the others.
func TestLuaPartialSection(t *testing.T) {
	cfg, _, _ := setBases(t)
	write(t, filepath.Join(cfg, "nekuroddo", "config.toml"), "[lua]\nauto_reload = true\n")
	c, err := Load()
	if err != nil {
		t.Fatal(err)
	}
	if want := (Lua{Enabled: true, CommandTimeoutMS: 5000, CallbackTimeoutMS: 50, AutoReload: true}); c.Lua != want {
		t.Fatalf("[lua] = %+v", c.Lua)
	}
}

// An invalid budget takes the default and is reported, but the rest of the
// file still counts.
func TestLuaInvalidBudgets(t *testing.T) {
	cfg, _, _ := setBases(t)
	write(t, filepath.Join(cfg, "nekuroddo", "config.toml"),
		"theme = \"x\"\n[lua]\ncommand_timeout_ms = 0\ncallback_timeout_ms = -5\nauto_reload = true\n")
	c, err := Load()
	if err == nil || !strings.Contains(err.Error(), "command_timeout_ms") || !strings.Contains(err.Error(), "callback_timeout_ms") {
		t.Fatalf("want an error naming both keys, got %v", err)
	}
	if c.Lua.CommandTimeoutMS != 5000 || c.Lua.CallbackTimeoutMS != 50 || !c.Lua.AutoReload || c.Theme != "x" {
		t.Fatalf("invalid budgets → defaults, the rest kept: %+v theme=%q", c.Lua, c.Theme)
	}
	for i, r := range c.Rules {
		if r.re == nil {
			t.Fatalf("rule %d not compiled", i)
		}
	}
}

func TestLuaWrongTypeIsError(t *testing.T) {
	cfg, _, _ := setBases(t)
	write(t, filepath.Join(cfg, "nekuroddo", "config.toml"), "[lua]\nenabled = \"sim\"\n")
	c, err := Load()
	if err == nil {
		t.Fatal("want an error for a wrong type")
	}
	if c.Lua != Default().Lua {
		t.Fatalf("falls back to Default(): %+v", c.Lua)
	}
}

// The validation also guards a literal Config (zero values).
func TestLuaValidZero(t *testing.T) {
	l, err := Lua{}.valid()
	if err == nil || l.CommandTimeoutMS != 5000 || l.CallbackTimeoutMS != 50 || l.Enabled {
		t.Fatalf("zero budgets → defaults (enabled untouched): %+v %v", l, err)
	}
	l, err = Lua{Enabled: true, CommandTimeoutMS: 1, CallbackTimeoutMS: 1}.valid()
	if err != nil || l.CommandTimeoutMS != 1 {
		t.Fatalf("valid kept: %+v %v", l, err)
	}
	// a zero Lua's durations are the defaults too (a literal Config)
	if (Lua{}).CommandTimeout() != 5*time.Second || (Lua{}).CallbackTimeout() != 50*time.Millisecond {
		t.Fatal("zero Lua durations")
	}
}
