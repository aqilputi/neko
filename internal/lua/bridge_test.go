package lua

import (
	"errors"
	"fmt"
	"path/filepath"
	"reflect"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/aqilputi/nekuroddo/internal/app"
	"github.com/aqilputi/nekuroddo/internal/core"
	"github.com/aqilputi/nekuroddo/internal/groups"
	"github.com/aqilputi/nekuroddo/internal/store"
)

func TestLuaErrorBecomesGoErrorAndToast(t *testing.T) {
	e := newEnv(t, Options{})
	rec := e.record()
	_, err := e.rt.Eval(`error("boom")`)
	if err == nil || !strings.Contains(err.Error(), "boom") || !strings.Contains(err.Error(), "<eval>:1") {
		t.Fatalf("err = %v, want <eval>:1 … boom", err)
	}
	eventually(t, "toast and lua.error", func() bool {
		return rec.hasToast("boom") && len(rec.named(EvLuaError)) == 1
	})
	if ev := rec.named(EvLuaError)[0].Data.(ErrorEvent); !strings.Contains(ev.Error, "boom") || ev.File != evalFile {
		t.Fatalf("lua.error = %+v", ev)
	}
	if ms := e.rt.Messages(); len(ms) != 1 || ms[0].Level != "error" {
		t.Fatalf("Messages = %+v", ms)
	}
	// a syntax error too
	if _, err := e.rt.Eval("if then"); err == nil {
		t.Fatal("syntax error: want an error")
	}
}

func TestEvalResults(t *testing.T) {
	e := newEnv(t, Options{})
	for code, want := range map[string]string{
		"1 + 1":                        "2",
		"local x = 3; return x * 2":    "6",
		"nil":                          "nil",
		`"oi"`:                         "oi",
		"{1, 2}":                       "[1 2]",
		"{a = true}":                   "map[a:true]",
		"os.exit == nil and io == nil": "true",
	} {
		if got := e.eval(code); got != want {
			t.Errorf("Eval(%q) = %q, want %q", code, got, want)
		}
	}
}

const helloLua = `
nk.command("hello", function(args, ctx)
  return "oi " .. args.who .. " " .. ctx.session_id .. " " .. #ctx.picked .. " " .. ctx.tab
end, { doc = "diz oi", args = { "who", { name = "n", type = "int", prompt = "n › ", required = false } },
       keys = { "ctrl+h" }, mode = "insert", hint = "exemplo", confirm = true })

nk.command("rich", function()
  return { toast = "feito", err = true, data = { n = 1, xs = { "a", "b" } } }
end)

nk.command("silent", function() end)

nk.command("broken", function() error("quebrou") end)
`

func TestCommandIsRegisteredAndRuns(t *testing.T) {
	e := newEnv(t, Options{})
	e.write("init.lua", helloLua)
	e.load()

	c, ok := e.app.Registry().Lookup("hello")
	if !ok {
		t.Fatal("hello not registered")
	}
	want := core.Command{
		Name: "hello", Title: "diz oi", Hint: "exemplo", Keys: []string{"ctrl+h"}, Mode: core.ModeInsert,
		Args: []core.Arg{
			{Name: "who", Type: core.ArgString, Prompt: "who › "},
			{Name: "n", Type: core.ArgInt, Prompt: "n › "},
		},
		Scope: core.ScopeCore, Confirm: true, Source: "lua:init.lua",
	}
	if !reflect.DeepEqual(c, want) {
		t.Fatalf("command = %+v\nwant      %+v", c, want)
	}
	if _, ok := commandNamed(e.app.Registry().List(app.Context{}), "hello"); !ok {
		t.Fatal("hello not in List")
	}
	ctx := app.Context{SessionID: "s1", Picked: []string{"a", "b"}, Tab: "inbox"}
	res, err := e.app.Registry().Run(ctx, "hello", app.Args{"who": "ana"})
	if err != nil || res.Toast != "oi ana s1 2 inbox" {
		t.Fatalf("Run hello = %+v, %v", res, err)
	}

	res, err = e.app.Run(app.Context{}, "rich", nil)
	if err != nil || res.Toast != "feito" || !res.Err {
		t.Fatalf("Run rich = %+v, %v", res, err)
	}
	if data := fmt.Sprint(res.Data); data != "map[n:1 xs:[a b]]" {
		t.Fatalf("rich data = %s", data)
	}
	if res, err := e.app.Run(app.Context{}, "silent", nil); err != nil || res.Toast != "" {
		t.Fatalf("Run silent = %+v, %v", res, err)
	}

	res, err = e.app.Run(app.Context{}, "broken", nil)
	if err == nil || !res.Err || !strings.Contains(res.Toast, "init.lua:13: quebrou") {
		t.Fatalf("Run broken = %+v, %v", res, err)
	}
}

func TestCommandBudget(t *testing.T) {
	e := newEnv(t, Options{CommandTimeout: 30 * time.Millisecond})
	e.write("init.lua", `nk.command("spin", function() while true do end end)`)
	e.load()
	start := time.Now()
	_, err := e.app.Run(app.Context{}, "spin", nil)
	if !errors.Is(err, ErrBudget) || time.Since(start) > 30*time.Millisecond+slack {
		t.Fatalf("spin: %v after %v", err, time.Since(start))
	}
}

func TestCommandValidation(t *testing.T) {
	e := newEnv(t, Options{})
	if err := e.app.RegisterBuiltins(); err != nil {
		t.Fatal(err)
	}
	for _, code := range []string{
		`nk.command("", function() end)`,
		`nk.command("x", 1)`,
		`nk.command("x", function() end, { args = { 3 } })`,
		`nk.command("x", function() end, { args = { { type = "string" } } })`,
		`nk.command("x", function() end, { keys = "ctrl+x" })`,
		`nk.command("group.add", function() end)`,
	} {
		if _, err := e.rt.Eval(code); err == nil {
			t.Errorf("Eval(%q): want error", code)
		}
	}
	if _, ok := e.app.Registry().Lookup("x"); ok {
		t.Fatal("x registered despite errors")
	}
	// Eval may define commands too (attributed to <eval>), and redefine them
	e.eval(`nk.command("e", function() return "1" end)`)
	e.eval(`nk.command("e", function() return "2" end)`)
	if c, _ := e.app.Registry().Lookup("e"); c.Source != "lua:<eval>" {
		t.Fatalf("e source = %q", c.Source)
	}
	if res, _ := e.app.Run(app.Context{}, "e", nil); res.Toast != "2" {
		t.Fatalf("e = %q", res.Toast)
	}
}

func TestConcurrentRunsAreSerialized(t *testing.T) {
	e := newEnv(t, Options{})
	e.write("init.lua", `count = 0
nk.command("inc", function() local c = count; for i = 1, 100 do end; count = c + 1 end)`)
	e.load()
	var wg sync.WaitGroup
	for range 40 {
		wg.Add(1)
		go func() {
			defer wg.Done()
			if _, err := e.app.Registry().Run(app.Context{}, "inc", nil); err != nil {
				t.Error(err)
			}
		}()
	}
	wg.Wait()
	if got := e.eval("count"); got != "40" {
		t.Fatalf("count = %s", got)
	}
}

func TestRun(t *testing.T) {
	e := newEnv(t, Options{})
	if err := e.app.RegisterBuiltins(); err != nil {
		t.Fatal(err)
	}
	e.sessions(store.Session{SessionID: "s1", Name: "um"})
	e.write("init.lua", helloLua+`
nk.command("outer", function(args, ctx)
  return "outer: " .. nk.run("hello", { who = "bia" }).toast
end)`)
	e.load()
	// a Lua command from a Lua command runs inline, with the caller's selection
	res, err := e.app.Run(app.Context{SessionID: "s9", Tab: "sessions"}, "outer", nil)
	if err != nil || res.Toast != "outer: oi bia s9 0 sessions" {
		t.Fatalf("outer = %+v, %v", res, err)
	}
	// a builtin, with an explicit selection
	e.eval(`nk.run("group.add", { tag = "x" }, { picked = { "s1" } })`)
	if ms := e.app.Groups().Groups["x"]; len(ms) != 1 || ms[0].SessionID != "s1" {
		t.Fatalf("groups = %+v", e.app.Groups())
	}
	// a builtin's Result comes back as a table
	if got := e.eval(`nk.run("group.dissolve", { tag = "x" }).pending.question`); !strings.Contains(got, "#x") {
		t.Fatalf("pending question = %q", got)
	}
	for _, code := range []string{`nk.run("nope")`, `nk.run("broken")`, `nk.run("group.add", {})`} {
		if _, err := e.rt.Eval(code); err == nil {
			t.Errorf("Eval(%q): want error", code)
		}
	}
}

func TestKeymap(t *testing.T) {
	e := newEnv(t, Options{})
	rec := e.record()
	e.write("init.lua", helloLua+`
nk.keymap.set("normal", "ctrl+k", "hello")
nk.keymap.set("normal", "g", function(ctx) nk.ui.toast("g em " .. ctx.session_id) end)
nk.keymap.set("insert", "ctrl+k", "rich")
nk.keymap.del("normal", "x")
nk.keymap.set("normal", "s", function() while true do end end)
`)
	e.load()
	km := e.rt.Keymap()
	want := core.Keymap{
		{Command: "hello", Keys: []string{"ctrl+k"}, Mode: "normal", Help: core.Help{Key: "ctrl+k", Desc: "diz oi"}},
		{Command: "lua.key:normal:g", Keys: []string{"g"}, Mode: "normal", Help: core.Help{Key: "g", Desc: "lua"}},
		{Command: "rich", Keys: []string{"ctrl+k"}, Mode: "insert", Help: core.Help{Key: "ctrl+k", Desc: "rich"}},
		{Command: "", Keys: []string{"x"}, Mode: "normal"},
		{Command: "lua.key:normal:s", Keys: []string{"s"}, Mode: "normal", Help: core.Help{Key: "s", Desc: "lua"}},
	}
	if !reflect.DeepEqual(km, want) {
		t.Fatalf("keymap =\n%+v\nwant\n%+v", km, want)
	}
	eventually(t, "keymap.changed", func() bool { return len(rec.named(EvKeymapChanged)) > 0 })

	if _, err := e.rt.RunBinding(app.Context{SessionID: "s1"}, "lua.key:normal:g"); err != nil {
		t.Fatal(err)
	}
	eventually(t, "g toast", func() bool { return rec.hasToast("g em s1") })
	// a binding to a command name runs that command
	if res, err := e.rt.RunBinding(app.Context{}, "rich"); err != nil || res.Toast != "feito" {
		t.Fatalf("RunBinding rich = %+v, %v", res, err)
	}
	// key functions get the 50 ms budget
	start := time.Now()
	if _, err := e.rt.RunBinding(app.Context{}, "lua.key:normal:s"); !errors.Is(err, ErrBudget) {
		t.Fatalf("spin key: %v", err)
	}
	if d := time.Since(start); d > DefaultCallbackTimeout+slack {
		t.Fatalf("spin key took %v", d)
	}
	if _, err := e.rt.RunBinding(app.Context{}, "lua.key:normal:zz"); err == nil {
		t.Fatal("unknown key binding: want error")
	}

	// overriding and deleting at runtime
	e.eval(`nk.keymap.set("normal", "ctrl+k", "rich"); nk.keymap.del("normal", "g")`)
	km = e.rt.Keymap()
	if km[0].Command != "rich" || km[1].Command != "" || km[1].Keys[0] != "g" {
		t.Fatalf("after override: %+v", km)
	}
	for _, code := range []string{`nk.keymap.set("normal", "", "x")`, `nk.keymap.set("normal", "k", 3)`, `nk.keymap.del("", "k")`} {
		if _, err := e.rt.Eval(code); err == nil {
			t.Errorf("Eval(%q): want error", code)
		}
	}
}

func TestOnReceivesBusEvents(t *testing.T) {
	e := newEnv(t, Options{})
	e.write("init.lua", `
got = {}
function onpeer(ev, name) got[#got + 1] = name .. ":" .. ev.from .. ":" .. ev.text end
nk.on("peer.message", onpeer)
nk.on("toast", function(ev) last_toast = ev.text end)
`)
	e.load()
	e.app.Publish(app.EvPeerMessage, app.PeerMessage{To: "s1", From: "ana", Text: "[#deploy] subiu"})
	eventually(t, "handler", func() bool { return e.eval("#got") == "1" })
	if got := e.eval("got[1]"); got != "peer.message:ana:[#deploy] subiu" {
		t.Fatalf("got[1] = %q", got)
	}
	e.app.Toast("olá", false)
	eventually(t, "toast handler", func() bool { return e.eval("last_toast") == "olá" })

	e.eval(`nk.off("peer.message", onpeer)`)
	e.app.Publish(app.EvPeerMessage, app.PeerMessage{From: "bia", Text: "x"})
	e.app.Toast("marca", false)
	eventually(t, "marker", func() bool { return e.eval("last_toast") == "marca" })
	if got := e.eval("#got"); got != "1" {
		t.Fatalf("handler ran after nk.off: #got = %s", got)
	}
	// nk.off(name) drops every handler of that event
	e.eval(`nk.off("toast")`)
	e.app.Toast("depois", false)
	e.app.Publish(app.EvPeerMessage, app.PeerMessage{From: "c", Text: "y"})
	time.Sleep(20 * time.Millisecond)
	if got := e.eval("last_toast"); got != "marca" {
		t.Fatalf("toast handler ran after nk.off: %q", got)
	}
	for _, code := range []string{`nk.on("", function() end)`, `nk.on("x", 1)`} {
		if _, err := e.rt.Eval(code); err == nil {
			t.Errorf("Eval(%q): want error", code)
		}
	}
}

func TestFailingHandlerDisablesItsFileAfterThree(t *testing.T) {
	e := newEnv(t, Options{})
	rec := e.record()
	e.write("init.lua", `nk.command("keep", function() return "ok" end)
calls = 0
nk.on("ping", function() calls = calls + 1 end)`)
	e.write("plugins/bad.lua", `nk.command("bad.cmd", function() return "x" end)
nk.keymap.set("normal", "b", "bad.cmd")
nk.on("ping", function() while true do end end)`)
	e.load()
	for i := 1; i <= 3; i++ {
		e.app.Publish("ping", nil)
		eventually(t, "lua.error", func() bool { return len(rec.named(EvLuaError)) == i })
	}
	eventually(t, "disabled", func() bool { return len(e.rt.Disabled()) == 1 })
	if d := e.rt.Disabled(); d[0] != "plugins/bad.lua" {
		t.Fatalf("Disabled = %v", d)
	}
	if !rec.hasToast("desativado") {
		t.Fatalf("toasts = %v", rec.toasts())
	}
	if _, ok := e.app.Registry().Lookup("bad.cmd"); ok {
		t.Fatal("bad.cmd still registered")
	}
	if len(e.rt.Keymap()) != 0 {
		t.Fatalf("keymap = %+v", e.rt.Keymap())
	}
	if _, ok := e.app.Registry().Lookup("keep"); !ok {
		t.Fatal("init.lua's command lost")
	}
	if got := e.eval("calls"); got != "3" {
		t.Fatalf("good handler calls = %s", got)
	}
	// reload keeps it off; Enable turns it back on
	if err := e.rt.Reload(); err != nil {
		t.Fatal(err)
	}
	if _, ok := e.app.Registry().Lookup("bad.cmd"); ok {
		t.Fatal("reload revived a disabled file")
	}
	if err := e.rt.Enable("plugins/bad.lua"); err != nil {
		t.Fatal(err)
	}
	if _, ok := e.app.Registry().Lookup("bad.cmd"); !ok {
		t.Fatal("Enable did not reload the file")
	}
	if err := e.rt.Enable("plugins/bad.lua"); err == nil {
		t.Fatal("Enable of an enabled file: want error")
	}
}

func TestSuccessResetsFailures(t *testing.T) {
	e := newEnv(t, Options{})
	e.write("init.lua", `nk.command("flaky", function(args) if args.fail == "1" then error("x") end end)`)
	e.load()
	for _, fail := range []string{"1", "1", "0", "1", "1", "0", "1"} {
		_, _ = e.app.Run(app.Context{}, "flaky", app.Args{"fail": fail})
	}
	if d := e.rt.Disabled(); len(d) != 0 {
		t.Fatalf("disabled after non-consecutive failures: %v", d)
	}
	for range 2 {
		_, _ = e.app.Run(app.Context{}, "flaky", app.Args{"fail": "1"})
	}
	if d := e.rt.Disabled(); len(d) != 1 {
		t.Fatalf("Disabled = %v", d)
	}
	if _, err := e.app.Run(app.Context{}, "flaky", nil); !errors.Is(err, app.ErrUnknownCommand) {
		t.Fatalf("run after disable: %v", err)
	}
}

var twoSessions = []store.Session{
	{SessionID: "s1", ShortID: "a1", Name: "um", Cwd: "/w/a", Context: "w", Kind: "background", State: "busy",
		Model: "opus", Cost: 1.5, Attention: store.AttnReturned, LastAt: time.Unix(100, 0)},
	{SessionID: "s2", Name: "dois", Cwd: "/w/b", Kind: "interactive", State: "blocked", Attention: store.AttnBlocked, Stale: true},
	{SessionID: "s3", Name: "tres", Kind: "interactive", State: "idle"},
}

func TestSessionsReadTheSnapshot(t *testing.T) {
	e := newEnv(t, Options{})
	e.sessions(twoSessions...)
	if got := e.eval("#nk.sessions()"); got != "3" {
		t.Fatalf("#sessions = %s", got)
	}
	for code, want := range map[string]string{
		`nk.sessions()[1].id`:           "s1",
		`nk.session("s1").short_id`:     "a1",
		`nk.session("a1").name`:         "um",
		`nk.session("s1").cwd`:          "/w/a",
		`nk.session("s1").attention`:    "returned",
		`nk.session("s2").attention`:    "blocked",
		`nk.session("s3").attention`:    "",
		`nk.session("s1").cost`:         "1.5",
		`nk.session("s1").last_at`:      "100",
		`nk.session("s2").stale`:        "true",
		`nk.session("s1").label`:        "um",
		`nk.session("zz")`:              "nil",
		`table.concat(nk.inbox(), ",")`: "s2,s1",
	} {
		if got := e.eval(code); got != want {
			t.Errorf("%s = %q, want %q", code, got, want)
		}
	}
	// a copy: changing it does not touch the core
	e.eval(`nk.session("s1").name = "mudou"`)
	if e.app.Snapshot().Sessions[0].Name != "um" {
		t.Fatal("snapshot changed through Lua")
	}
}

func TestDispatch(t *testing.T) {
	e := newEnv(t, Options{})
	e.f.Claude.NextID = "ab12"
	got := e.eval(`nk.dispatch{ dir = "/w/x", prompt = "faz", perm = "acceptEdits", name = "n" }`)
	if got != "ab12" {
		t.Fatalf("dispatch = %q", got)
	}
	c := e.f.Claude.Last()
	if c.Method != "Dispatch" || c.Args[0] != "/w/x" || c.Args[1] != "n" || c.Args[2] != "faz" ||
		!reflect.DeepEqual(c.Args[3], []string{"--permission-mode", "acceptEdits"}) {
		t.Fatalf("Dispatch call = %+v", c)
	}
	// bypass parks a confirmation and hands its token back
	tok := e.eval(`local p = nk.dispatch{ dir = "/w/x", prompt = "faz", perm = "bypassPermissions" }; return p.token`)
	if ps := e.app.Pending(); len(ps) != 1 || ps[0].Token != tok {
		t.Fatalf("pending = %+v, token %q", ps, tok)
	}
	if e.f.Claude.Count("Dispatch") != 1 {
		t.Fatal("bypass dispatched without confirmation")
	}
	for _, code := range []string{
		`nk.dispatch{ prompt = "p" }`,
		`nk.dispatch{ dir = "/w", prompt = "p", perm = "yolo" }`,
		`nk.dispatch{ dir = "/w", prompt = "p", extra = { "--x" } }`,
		`nk.dispatch("x")`,
	} {
		if _, err := e.rt.Eval(code); err == nil {
			t.Errorf("Eval(%q): want error", code)
		}
	}
	e.f.Claude.Errs = map[string]error{"Dispatch": errors.New("sem claude")}
	if _, err := e.rt.Eval(`nk.dispatch{ dir = "/w", prompt = "p" }`); err == nil || !strings.Contains(err.Error(), "sem claude") {
		t.Fatalf("dispatch failure: %v", err)
	}
}

func TestStopAndRemoveAsk(t *testing.T) {
	e := newEnv(t, Options{})
	e.sessions(twoSessions...)
	if tok := e.eval(`nk.stop("a1").token`); tok == "" || tok == "nil" {
		t.Fatalf("stop token = %q", tok)
	}
	if tok := e.eval(`nk.remove("s1").token`); tok == "" || tok == "nil" {
		t.Fatalf("remove token = %q", tok)
	}
	if len(e.app.Pending()) != 2 {
		t.Fatalf("pending = %+v", e.app.Pending())
	}
	// not a background session: the toast comes back as an error
	if _, err := e.rt.Eval(`nk.stop("s2")`); err == nil {
		t.Fatal("stop of an interactive session: want error")
	}
}

func TestGroups(t *testing.T) {
	e := newEnv(t, Options{})
	e.sessions(twoSessions...)
	e.eval(`nk.group.add("deploy", { "a1", "s2" })`)
	ms := e.app.Groups().Groups["deploy"]
	if len(ms) != 2 || ms[0].SessionID != "s1" || ms[1].SessionID != "s2" {
		t.Fatalf("deploy = %+v", ms)
	}
	e.eval(`nk.group.add("#ops", "s3")`)
	if got := e.eval(`table.concat(nk.group.tags(), ",")`); got != "deploy,ops" {
		t.Fatalf("tags = %q", got)
	}
	if got := e.eval(`table.concat(nk.group.of("a1"), ",")`); got != "deploy" {
		t.Fatalf("of a1 = %q", got)
	}
	e.eval(`nk.notify("deploy", "subam")`)
	if n := e.f.Groups.Count("Queue"); n != 2 {
		t.Fatalf("Queue calls = %d", n)
	}
	e.eval(`nk.group.notify("ops", "oi")`)
	if n := e.f.Groups.Count("Queue"); n != 3 {
		t.Fatalf("Queue calls = %d", n)
	}
	e.eval(`nk.group.remove("deploy", { "s2" })`)
	if ms := e.app.Groups().Groups["deploy"]; len(ms) != 1 {
		t.Fatalf("deploy after remove = %+v", ms)
	}
	e.eval(`nk.group.dissolve("deploy")`)
	if _, ok := e.app.Groups().Groups["deploy"]; ok {
		t.Fatal("deploy not dissolved")
	}
	for _, code := range []string{
		`nk.group.add("x", { "nobody" })`, `nk.notify("nope", "x")`, `nk.group.dissolve("")`,
		`nk.group.add("x", 3)`,
	} {
		if _, err := e.rt.Eval(code); err == nil {
			t.Errorf("Eval(%q): want error", code)
		}
	}
}

func TestGroupAddRefreshesForFreshAgents(t *testing.T) {
	e := newEnv(t, Options{})
	e.sessions(twoSessions...)
	// the agent just dispatched is not in the snapshot yet; the next refresh has it
	e.f.Sessions.SetSnapshot(store.Snapshot{Sessions: append(append([]store.Session(nil), twoSessions...),
		store.Session{SessionID: "s9", ShortID: "n9", Kind: "background"})})
	e.eval(`nk.group.add("novo", { "n9" })`)
	if ms := e.app.Groups().Groups["novo"]; len(ms) != 1 || ms[0].SessionID != "s9" {
		t.Fatalf("novo = %+v", e.app.Groups().Groups)
	}
}

func TestUI(t *testing.T) {
	e := newEnv(t, Options{Caps: []string{"toast", "cat", "palette", "notify", "confirm"}})
	rec := e.record()
	if got := e.eval(`tostring(nk.ui.has("cat")) .. tostring(nk.ui.has("scene"))`); got != "truefalse" {
		t.Fatalf("has = %s", got)
	}
	if !e.rt.HasCap("cat") || e.rt.HasCap("scene") {
		t.Fatal("HasCap")
	}
	e.eval(`nk.ui.toast("ruim", true); nk.ui.toast("bom")`)
	eventually(t, "toasts", func() bool { return len(rec.named(app.EvToast)) == 2 })
	if ts := rec.named(app.EvToast); !ts[0].Data.(app.ToastData).Err || ts[1].Data.(app.ToastData).Err {
		t.Fatalf("toasts = %+v", ts)
	}
	e.eval(`nk.ui.palette_open("session."); nk.ui.cat.meow("miau")`)
	eventually(t, "ui events", func() bool {
		return len(rec.named(EvUIPaletteOpen)) == 1 && len(rec.named(EvUICatMeow)) == 1
	})
	if d := rec.named(EvUIPaletteOpen)[0].Data.(map[string]string); d["prefill"] != "session." {
		t.Fatalf("palette_open = %+v", d)
	}
	e.eval(`nk.ui.notify("deploy ok")`)
	eventually(t, "desktop notification", func() bool { return len(e.f.Notify.Notes()) == 1 })

	// confirm: yes runs the function on the VM, under the command budget
	tok := e.eval(`return nk.ui.confirm("seguir?", function() confirmed = "sim" end)`)
	if _, err := e.app.Confirm(tok); err != nil {
		t.Fatal(err)
	}
	if got := e.eval("confirmed"); got != "sim" {
		t.Fatalf("confirmed = %q", got)
	}
	tok = e.eval(`return nk.ui.confirm("de novo?", function() error("não") end)`)
	if res, err := e.app.Confirm(tok); err == nil || !res.Err {
		t.Fatalf("failing confirm = %+v, %v", res, err)
	}
}

func TestUIWithoutCaps(t *testing.T) {
	e := newEnv(t, Options{})
	rec := e.record()
	if got := e.eval(`nk.ui.has("toast")`); got != "false" {
		t.Fatalf("has toast = %s", got)
	}
	// without the capability these are no-ops that say so
	for _, code := range []string{`nk.ui.palette_open("x")`, `nk.ui.cat.meow("x")`, `nk.ui.notify("x")`} {
		if got := e.eval(code); got != "false" {
			t.Errorf("%s = %s, want false", code, got)
		}
	}
	// toast still goes on the bus: it is how clients learn of it
	e.eval(`nk.ui.toast("x")`)
	eventually(t, "toast", func() bool { return len(rec.named(app.EvToast)) == 1 })
	time.Sleep(10 * time.Millisecond)
	if len(rec.named(EvUIPaletteOpen))+len(rec.named(EvUICatMeow)) != 0 || len(e.f.Notify.Notes()) != 0 {
		t.Fatal("capability-less calls had effects")
	}
}

func TestMiscAPI(t *testing.T) {
	e := newEnv(t, Options{})
	if got := e.eval("nk.config_dir()"); got != e.dir {
		t.Fatalf("config_dir = %q, want %q", got, e.dir)
	}
	if got := e.eval("nk.state_dir()"); filepath.Base(got) != "nekuroddo" {
		t.Fatalf("state_dir = %q", got)
	}
	e.eval(`nk.log("info", "olá"); nk.log("so a mensagem")`)
	ms := e.rt.Messages()
	if len(ms) != 2 || ms[0].Level != "info" || ms[0].Text != "olá" || ms[1].Level != "info" || ms[1].Text != "so a mensagem" {
		t.Fatalf("messages = %+v", ms)
	}
	if got := e.eval("nk.version"); got != "1" {
		t.Fatalf("version = %q", got)
	}
}

func TestMessageRingKeepsTheLast100(t *testing.T) {
	e := newEnv(t, Options{})
	e.eval(`for i = 1, 130 do nk.log("info", tostring(i)) end`)
	ms := e.rt.Messages()
	if len(ms) != MaxMessages || ms[0].Text != "31" || ms[99].Text != "130" {
		t.Fatalf("ring: %d, first %q", len(ms), ms[0].Text)
	}
}

func TestEventPayloadConversion(t *testing.T) {
	e := newEnv(t, Options{})
	e.write("init.lua", `nk.on("custom", function(ev) seen = ev end)`)
	e.load()
	e.app.Publish("custom", map[string]any{
		"s": "x", "n": 2, "b": true, "list": []string{"a", "b"},
		"nested": map[string]any{"k": 1.5}, "nothing": nil,
		"group": groups.Member{SessionID: "s1"},
	})
	eventually(t, "custom", func() bool { return e.eval("seen ~= nil") == "true" })
	for code, want := range map[string]string{
		"seen.s": "x", "seen.n": "2", "seen.b": "true", "seen.list[2]": "b", "seen.nested.k": "1.5",
		"seen.nothing": "nil", "seen.group.session_id": "s1",
	} {
		if got := e.eval(code); got != want {
			t.Errorf("%s = %q, want %q", code, got, want)
		}
	}
	// a payload JSON cannot carry arrives as nil, not as a crash
	e.app.Publish("custom", func() {})
	e.app.Publish("custom", "texto")
	eventually(t, "string payload", func() bool { return e.eval("seen") == "texto" })
}
