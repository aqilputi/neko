package lua

import (
	"errors"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"github.com/aqilputi/nekuroddo/internal/app"
)

func (e *env) register() {
	e.t.Helper()
	if err := e.rt.RegisterCommands(); err != nil {
		e.t.Fatal(err)
	}
}

func (e *env) run(name string, args app.Args) (app.Result, error) {
	return e.app.Run(app.Context{}, name, args)
}

func TestRegisterCommandsListsThem(t *testing.T) {
	e := newEnv(t, Options{})
	e.register()
	for _, name := range []string{CmdReload, CmdMessages, CmdEnable} {
		c, ok := commandNamed(e.app.Registry().List(app.Context{}), name)
		if !ok || c.Title == "" || strings.HasPrefix(c.Source, sourcePrefix) {
			t.Fatalf("%s: %+v %v", name, c, ok)
		}
	}
	if err := e.rt.RegisterCommands(); err == nil {
		t.Fatal("registering twice clashes")
	}
	// they are not Lua commands: closing the runtime keeps them
	e.rt.Close()
	if _, ok := e.app.Registry().Lookup(CmdMessages); !ok {
		t.Fatal("Close must not drop the runtime's own commands")
	}
}

func TestReloadCommand(t *testing.T) {
	e := newEnv(t, Options{})
	e.register()
	e.write("init.lua", `nk.command("x", function() end)`)
	res, err := e.run(CmdReload, nil)
	if err != nil || res.Toast != "lua recarregado (1 arquivos)" {
		t.Fatalf("%+v %v", res, err)
	}
	e.write("init.lua", `nk.command(`)
	res, err = e.run(CmdReload, nil)
	if err == nil || !res.Err || !strings.Contains(res.Toast, "init.lua") {
		t.Fatalf("a broken file is an error result: %+v %v", res, err)
	}
}

func TestMessagesCommand(t *testing.T) {
	e := newEnv(t, Options{})
	e.register()
	res, err := e.run(CmdMessages, nil)
	if err != nil || res.Toast != "lua: nenhuma mensagem" || len(res.Data.([]Message)) != 0 {
		t.Fatalf("empty ring: %+v %v", res, err)
	}
	e.eval(`nk.log("warn", "primeira")`)
	_, _ = e.rt.Eval(`error("segunda")`)
	res, err = e.run(CmdMessages, nil)
	msgs, _ := res.Data.([]Message)
	if err != nil || len(msgs) != 2 || msgs[0].Text != "primeira" || !strings.Contains(msgs[1].Text, "segunda") {
		t.Fatalf("ring as data: %+v %v", res, err)
	}
	if !strings.HasPrefix(res.Toast, "lua: 2 mensagens · última: ") || !strings.Contains(res.Toast, "segunda") || res.Err {
		t.Fatalf("toast summarizes: %q", res.Toast)
	}
}

func TestMessagesSummaryNamesTheFile(t *testing.T) {
	if got := messagesSummary([]Message{{File: "init.lua", Level: "error", Text: "x"}}); got != "lua: 1 mensagem · última: init.lua: x" {
		t.Fatal(got)
	}
}

func TestEnableCommand(t *testing.T) {
	e := newEnv(t, Options{})
	e.register()
	c, _ := e.app.Registry().Lookup(CmdEnable)
	if len(c.Args) != 1 || c.Args[0].Name != "file" || !c.Args[0].Required {
		t.Fatalf("args: %+v", c.Args)
	}
	if _, err := e.run(CmdEnable, nil); !errors.Is(err, app.ErrMissingArg) {
		t.Fatalf("file is required: %v", err)
	}
	res, err := e.run(CmdEnable, app.Args{"file": "init.lua"})
	if err == nil || !res.Err || !strings.Contains(res.Toast, "não está desativado") {
		t.Fatalf("not disabled: %+v %v", res, err)
	}

	// three failing handlers disable the file; lua.enable turns it back on
	e.write("plugins/a.lua", `nk.on("ping", function() error("x") end)`)
	e.load()
	for range MaxFailures {
		e.app.Publish("ping", nil)
	}
	eventually(t, "disabled", func() bool { return len(e.rt.Disabled()) == 1 })
	e.write("plugins/a.lua", `nk.command("ok", function() end)`)
	res, err = e.run(CmdEnable, app.Args{"file": "plugins/a.lua"})
	if err != nil || res.Toast != "lua: plugins/a.lua religado" {
		t.Fatalf("%+v %v", res, err)
	}
	if _, ok := e.app.Registry().Lookup("ok"); !ok || len(e.rt.Disabled()) != 0 {
		t.Fatal("enabled and reloaded")
	}
	// the hint lists what is disabled
	if got := enableHint(nil); got != "nenhum arquivo desativado" {
		t.Fatal(got)
	}
	if got := enableHint([]string{"a.lua", "b.lua"}); got != "desativados: a.lua, b.lua" {
		t.Fatal(got)
	}
}

func TestDisabledLoadsNothing(t *testing.T) {
	e := newEnv(t, Options{Disabled: true})
	e.write("init.lua", `nk.command("x", function() end)`)
	if err := e.rt.Load(); err != nil {
		t.Fatalf("Load of a disabled runtime is a no-op: %v", err)
	}
	if _, ok := e.app.Registry().Lookup("x"); ok || len(e.rt.Files()) != 0 {
		t.Fatal("nothing loads")
	}
	if err := e.rt.Reload(); !errors.Is(err, ErrDisabled) {
		t.Fatalf("Reload says why: %v", err)
	}
}

func TestAutoReloadPicksUpChanges(t *testing.T) {
	e := newEnv(t, Options{})
	e.write("init.lua", `nk.command("a", function() end)`)
	e.load()
	e.rt.AutoReload(5 * time.Millisecond)
	e.rt.AutoReload(5 * time.Millisecond) // a second call is a no-op

	// a changed file (newer mtime)
	e.write("init.lua", `nk.command("b", function() end)`)
	future := time.Now().Add(time.Hour)
	if err := os.Chtimes(filepath.Join(e.dir, "init.lua"), future, future); err != nil {
		t.Fatal(err)
	}
	eventually(t, "b", func() bool { _, ok := e.app.Registry().Lookup("b"); return ok })
	// a new plugin
	e.write("plugins/p.lua", `nk.command("p", function() end)`)
	eventually(t, "p", func() bool { _, ok := e.app.Registry().Lookup("p"); return ok })
	// a removed plugin
	if err := os.Remove(filepath.Join(e.dir, "plugins", "p.lua")); err != nil {
		t.Fatal(err)
	}
	eventually(t, "p gone", func() bool { _, ok := e.app.Registry().Lookup("p"); return !ok })
}

// A broken file during auto-reload is reported, the last good one stays,
// and the watcher keeps going.
func TestAutoReloadKeepsLastGood(t *testing.T) {
	e := newEnv(t, Options{})
	rec := e.record()
	e.write("init.lua", `nk.command("a", function() end)`)
	e.load()
	e.rt.AutoReload(5 * time.Millisecond)
	e.write("init.lua", `nk.command(`)
	bump(t, filepath.Join(e.dir, "init.lua"), 1)
	eventually(t, "error toast", func() bool { return rec.hasToast("init.lua") })
	if _, ok := e.app.Registry().Lookup("a"); !ok {
		t.Fatal("last good kept")
	}
	e.write("init.lua", `nk.command("c", function() end)`)
	bump(t, filepath.Join(e.dir, "init.lua"), 2)
	eventually(t, "c", func() bool { _, ok := e.app.Registry().Lookup("c"); return ok })
}

func TestAutoReloadOffWhenDisabledOrClosed(t *testing.T) {
	e := newEnv(t, Options{Disabled: true})
	e.rt.AutoReload(time.Millisecond) // disabled: no watcher
	e.rt.Close()
	e2 := newEnv(t, Options{})
	e2.rt.Close()
	e2.rt.AutoReload(time.Millisecond) // closed: no watcher, no leak
	e2.rt.AutoReload(0)                // non-positive period: ignored
}

// A config dir that cannot be listed (a broken glob) never reloads.
func TestFingerprintError(t *testing.T) {
	e := newEnv(t, Options{ConfigDir: "[" + t.TempDir()})
	if _, err := e.rt.fingerprint(); err == nil {
		t.Fatal("a bad glob is an error")
	}
	e.rt.AutoReload(time.Millisecond)
	time.Sleep(10 * time.Millisecond) // the watcher just skips the round
}

// bump moves a file's mtime n hours ahead (a write within the filesystem's
// mtime granularity would look unchanged).
func bump(t *testing.T, p string, n int) {
	t.Helper()
	at := time.Now().Add(time.Duration(n) * time.Hour)
	if err := os.Chtimes(p, at, at); err != nil {
		t.Fatal(err)
	}
}
