package lua

import (
	"errors"
	"os"
	"path/filepath"
	"reflect"
	"strings"
	"testing"
	"time"

	"github.com/aqilputi/nekuroddo/internal/app"
	"github.com/aqilputi/nekuroddo/internal/store"
)

func TestNoFilesChangesNothing(t *testing.T) {
	e := newEnv(t, Options{})
	if err := e.app.RegisterBuiltins(); err != nil {
		t.Fatal(err)
	}
	before := e.app.Registry().List(app.Context{})
	rec := e.record()
	e.load()
	if err := e.rt.Reload(); err != nil {
		t.Fatal(err)
	}
	if after := e.app.Registry().List(app.Context{}); !reflect.DeepEqual(before, after) {
		t.Fatal("registry changed with no Lua files")
	}
	if len(e.rt.Keymap()) != 0 || len(e.rt.Files()) != 0 || len(e.rt.Messages()) != 0 {
		t.Fatalf("keymap %v, files %v, messages %v", e.rt.Keymap(), e.rt.Files(), e.rt.Messages())
	}
	e.app.Toast("marca", false)
	eventually(t, "marker", func() bool { return rec.hasToast("marca") })
	rec.mu.Lock()
	n := len(rec.evs)
	rec.mu.Unlock()
	if n != 1 {
		t.Fatalf("events published with no Lua files: %+v", rec.evs)
	}
}

func TestLoadOrderAndSources(t *testing.T) {
	e := newEnv(t, Options{})
	e.write("init.lua", `order = { "init" }; nk.command("c.init", function() end)`)
	e.write("plugins/b.lua", `order[#order + 1] = "b"; nk.command("c.b", function() end)`)
	e.write("plugins/a.lua", `order[#order + 1] = "a"; nk.command("c.a", function() end)`)
	e.write("plugins/notes.txt", `not lua`)
	if err := os.MkdirAll(filepath.Join(e.dir, "plugins", "dir.lua"), 0o755); err != nil {
		t.Fatal(err)
	}
	e.load()
	if got := e.eval(`table.concat(order, ",")`); got != "init,a,b" {
		t.Fatalf("order = %s", got)
	}
	if got := e.rt.Files(); !reflect.DeepEqual(got, []string{"init.lua", "plugins/a.lua", "plugins/b.lua"}) {
		t.Fatalf("Files = %v", got)
	}
	for name, src := range map[string]string{"c.init": "lua:init.lua", "c.a": "lua:plugins/a.lua", "c.b": "lua:plugins/b.lua"} {
		if c, ok := e.app.Registry().Lookup(name); !ok || c.Source != src {
			t.Errorf("%s: %+v", name, c)
		}
	}
}

func TestPluginsLoadWithoutInit(t *testing.T) {
	e := newEnv(t, Options{})
	e.write("plugins/p.lua", `nk.command("p", function() end)`)
	e.load()
	if _, ok := e.app.Registry().Lookup("p"); !ok {
		t.Fatal("plugin not loaded")
	}
}

func TestBrokenReloadKeepsTheLastGood(t *testing.T) {
	e := newEnv(t, Options{})
	rec := e.record()
	e.write("init.lua", `nk.command("x", function() return "v1" end)
nk.keymap.set("normal", "X", "x")
nk.on("ping", function() pinged = "v1" end)`)
	e.load()

	e.write("init.lua", `nk.command("x", function() return "v2" end)
nk.command("y", function() end)
nk.keymap.set("normal", "Y", "y")
nk.on("ping", function() pinged = "v2" end)
error("quebrado na linha 5")`)
	err := e.rt.Reload()
	if err == nil || !strings.Contains(err.Error(), "init.lua:5:") {
		t.Fatalf("Reload = %v, want init.lua:5", err)
	}
	if res, _ := e.app.Run(app.Context{}, "x", nil); res.Toast != "v1" {
		t.Fatalf("x = %q after broken reload", res.Toast)
	}
	if _, ok := e.app.Registry().Lookup("y"); ok {
		t.Fatal("staged y leaked from a broken file")
	}
	if km := e.rt.Keymap(); len(km) != 1 || km[0].Keys[0] != "X" {
		t.Fatalf("keymap = %+v", km)
	}
	e.app.Publish("ping", nil)
	eventually(t, "old handler", func() bool { return e.eval("pinged") == "v1" })
	eventually(t, "error toast", func() bool { return rec.hasToast("init.lua:5:") })
	if len(e.rt.Disabled()) != 0 {
		t.Fatal("load errors must not count towards disabling")
	}

	// a syntax error is attributed too
	e.write("init.lua", "nk.command(\n\"z\",")
	if err := e.rt.Reload(); err == nil || !strings.Contains(err.Error(), "init.lua") {
		t.Fatalf("syntax error: %v", err)
	}

	// fixed: the new version replaces the old one entirely
	e.write("init.lua", `nk.command("y", function() return "y" end)`)
	if err := e.rt.Reload(); err != nil {
		t.Fatal(err)
	}
	if _, ok := e.app.Registry().Lookup("x"); ok {
		t.Fatal("x survived a good reload that dropped it")
	}
	if res, _ := e.app.Run(app.Context{}, "y", nil); res.Toast != "y" {
		t.Fatalf("y = %+v", res)
	}
	if len(e.rt.Keymap()) != 0 {
		t.Fatalf("keymap = %+v", e.rt.Keymap())
	}
}

func TestBrokenFileDoesNotStopTheOthers(t *testing.T) {
	e := newEnv(t, Options{})
	e.write("init.lua", `nk.command("good", function() end)`)
	e.write("plugins/bad.lua", `nk.command("bad", function() end); error("x")`)
	err := e.rt.Load()
	if err == nil || !strings.Contains(err.Error(), "plugins/bad.lua:1:") {
		t.Fatalf("Load = %v", err)
	}
	if _, ok := e.app.Registry().Lookup("good"); !ok {
		t.Fatal("good not loaded")
	}
	if _, ok := e.app.Registry().Lookup("bad"); ok {
		t.Fatal("bad leaked")
	}
	if got := e.rt.Files(); !reflect.DeepEqual(got, []string{"init.lua"}) {
		t.Fatalf("Files = %v", got)
	}
}

func TestRemovedFileIsDropped(t *testing.T) {
	e := newEnv(t, Options{})
	e.write("init.lua", `nk.command("i", function() end)`)
	e.write("plugins/p.lua", `nk.command("p", function() end)`)
	e.load()
	if err := os.Remove(filepath.Join(e.dir, "plugins", "p.lua")); err != nil {
		t.Fatal(err)
	}
	if err := e.rt.Reload(); err != nil {
		t.Fatal(err)
	}
	if _, ok := e.app.Registry().Lookup("p"); ok {
		t.Fatal("p survived its file")
	}
	if _, ok := e.app.Registry().Lookup("i"); !ok {
		t.Fatal("i lost")
	}
}

func TestLoadBudgetAndConflicts(t *testing.T) {
	e := newEnv(t, Options{CommandTimeout: 30 * time.Millisecond})
	if err := e.app.RegisterBuiltins(); err != nil {
		t.Fatal(err)
	}
	e.write("init.lua", `nk.command("mine", function() end)`)
	e.write("plugins/a.lua", `nk.command("mine", function() end)`)
	e.write("plugins/b.lua", `nk.command("session.new", function() end)`)
	e.write("plugins/c.lua", `while true do end`)
	err := e.rt.Load()
	for _, want := range []string{"plugins/a.lua", "plugins/b.lua", "plugins/c.lua"} {
		if err == nil || !strings.Contains(err.Error(), want) {
			t.Fatalf("Load = %v, want mention of %s", err, want)
		}
	}
	if c, _ := e.app.Registry().Lookup("mine"); c.Source != "lua:init.lua" {
		t.Fatalf("mine = %+v", c)
	}
	if c, _ := e.app.Registry().Lookup("session.new"); c.Source != "builtin" {
		t.Fatalf("session.new = %+v", c)
	}
}

func TestUnreadableFile(t *testing.T) {
	e := newEnv(t, Options{})
	e.write("init.lua", `x = 1`)
	p := filepath.Join(e.dir, "init.lua")
	if err := os.Chmod(p, 0); err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = os.Chmod(p, 0o644) })
	if f, err := os.Open(p); err == nil { // running as root
		_ = f.Close()
		t.Skip("file readable despite chmod 0")
	}
	if err := e.rt.Load(); err == nil || !strings.Contains(err.Error(), "init.lua") {
		t.Fatalf("Load = %v", err)
	}
}

func TestClosedRuntime(t *testing.T) {
	e := newEnv(t, Options{})
	e.write("init.lua", `nk.command("c", function() end)`)
	e.load()
	e.rt.Close()
	e.rt.Close()
	if _, ok := e.app.Registry().Lookup("c"); ok {
		t.Fatal("Close left a Lua command in the registry")
	}
	if _, err := e.rt.Eval("1"); !errors.Is(err, ErrClosed) {
		t.Fatalf("Eval after Close: %v", err)
	}
	if err := e.rt.Reload(); !errors.Is(err, ErrClosed) {
		t.Fatalf("Reload after Close: %v", err)
	}
	if _, err := e.rt.RunBinding(app.Context{}, "lua.key:normal:x"); !errors.Is(err, ErrClosed) {
		t.Fatalf("RunBinding after Close: %v", err)
	}
}

func TestNewNeedsApp(t *testing.T) {
	if _, err := New(nil, Options{}); err == nil {
		t.Fatal("New(nil): want error")
	}
}

// The examples in plan §5 and testdata/init.lua, against fakes.
func TestExampleInit(t *testing.T) {
	e := newEnv(t, Options{Caps: []string{"toast", "notify"}})
	src, err := os.ReadFile(filepath.Join("testdata", "init.lua"))
	if err != nil {
		t.Fatal(err)
	}
	e.write("init.lua", string(src))
	if err := e.app.RegisterBuiltins(); err != nil {
		t.Fatal(err)
	}
	rec := e.record()
	e.load()

	// 1. sessions under a svc-* directory are grouped in #svc on every snapshot
	e.sessions(
		store.Session{SessionID: "s1", Name: "a", Cwd: "/home/u/dev/svc-a", Kind: "interactive"},
		store.Session{SessionID: "s2", Name: "b", Cwd: "/home/u/dev/svc-b/sub", Kind: "interactive"},
		store.Session{SessionID: "s3", Name: "c", Cwd: "/home/u/notes", Kind: "interactive"},
	)
	eventually(t, "#svc", func() bool { return len(e.app.Groups().Groups["svc"]) == 2 })

	// 2. a [#deploy] peer message becomes a desktop notification
	e.app.Publish(app.EvPeerMessage, app.PeerMessage{To: "s1", From: "b", Text: "[#deploy] staging ok"})
	e.app.Publish(app.EvPeerMessage, app.PeerMessage{To: "s1", From: "b", Text: "conversa comum"})
	eventually(t, "notification", func() bool { return len(e.f.Notify.Notes()) == 1 })
	if body := e.f.Notify.Notes()[0].Body; !strings.Contains(body, "deploy: b: [#deploy] staging ok") {
		t.Fatalf("notification = %q", body)
	}

	// 3. deploy-all dispatches four agents and groups them in #deploy
	if c, ok := e.app.Registry().Lookup("deploy-all"); !ok || c.Source != "lua:init.lua" || len(c.Keys) != 1 {
		t.Fatalf("deploy-all = %+v", c)
	}
	e.f.Claude.NextID = "d1"
	e.f.Sessions.SetSnapshot(store.Snapshot{Sessions: []store.Session{
		{SessionID: "s1", Name: "a", Cwd: "/home/u/dev/svc-a", Kind: "interactive"},
		{SessionID: "sd1", ShortID: "d1", Name: "deploy", Cwd: "/home/u/dev/svc-a", Kind: "background"},
	}})
	res, err := e.app.Run(app.Context{}, "deploy-all", nil)
	if err != nil {
		t.Fatal(err)
	}
	if n := e.f.Claude.Count("Dispatch"); n != 4 {
		t.Fatalf("Dispatch calls = %d", n)
	}
	if ms := e.app.Groups().Groups["deploy"]; len(ms) != 1 || ms[0].SessionID != "sd1" {
		t.Fatalf("#deploy = %+v", e.app.Groups().Groups)
	}
	eventually(t, "deploy toast", func() bool { return rec.hasToast("deploy-all: 4 agents em #deploy") })
	_ = res
	if len(e.rt.Messages()) != 0 {
		t.Fatalf("errors running the example: %+v", e.rt.Messages())
	}
}
