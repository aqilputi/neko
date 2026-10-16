package lua

import (
	"strings"
	"testing"
	"time"

	"github.com/aqilputi/nekuroddo/internal/app"
	"github.com/aqilputi/nekuroddo/internal/store"
)

func TestLuaErrorHandlerFailureDoesNotFeedItself(t *testing.T) {
	e := newEnv(t, Options{})
	rec := e.record()
	e.write("init.lua", `nk.on("lua.error", function() error("no handler") end)`)
	e.load()
	_, _ = e.rt.Eval(`error("primeiro")`)
	eventually(t, "handler failure toast", func() bool { return rec.hasToast("no handler") })
	time.Sleep(20 * time.Millisecond)
	if n := len(rec.named(EvLuaError)); n != 1 {
		t.Fatalf("lua.error published %d times, want 1", n)
	}
}

func TestCommandResultsCarryPending(t *testing.T) {
	e := newEnv(t, Options{})
	e.write("init.lua", `
nk.command("risky", function()
  return nk.dispatch{ dir = "/w", prompt = "p", perm = "bypassPermissions" }
end)
nk.command("wrapped", function()
  local p = nk.dispatch{ dir = "/w", prompt = "p", perm = "bypassPermissions" }
  return { toast = "pergunta", pending = p }
end)
nk.command("number", function() return 42 end)
nk.command("nested", function() return nk.run("risky").toast end)`)
	e.load()
	for _, name := range []string{"risky", "wrapped"} {
		res, err := e.app.Run(app.Context{}, name, nil)
		if err != nil || res.Pending == nil || res.Pending.Token == "" || !strings.Contains(res.Pending.Question, "BYPASS") ||
			res.Pending.ExpiresAt.IsZero() {
			t.Fatalf("%s = %+v, %v", name, res, err)
		}
	}
	if res, _ := e.app.Run(app.Context{}, "number", nil); res.Toast != "42" {
		t.Fatalf("number = %+v", res)
	}
	if res, err := e.app.Run(app.Context{}, "nested", nil); err != nil || res.Toast != "" {
		t.Fatalf("nested = %+v, %v", res, err)
	}
}

func TestLoadTimeRunAndStagedKeys(t *testing.T) {
	e := newEnv(t, Options{})
	e.write("init.lua", `
nk.command("early", function(args, ctx)
  return args.x .. ctx.session_id .. ctx.picked[1] .. ctx.tab .. ctx.todo_id .. ctx.issue_key
end)
loaded = nk.run("early", { x = 1 }, { session_id = "s", picked = { "p" }, tab = "t", todo_id = 7, issue_key = "K-1" }).toast
nk.keymap.set("normal", "e", "early")
nk.keymap.set("normal", "e", "early")
nk.on("ping", function() end)
nk.off("ping")`)
	e.load()
	if got := e.eval("loaded"); got != "1spt7K-1" {
		t.Fatalf("loaded = %q", got)
	}
	if km := e.rt.Keymap(); len(km) != 1 {
		t.Fatalf("keymap = %+v", km)
	}
	if _, err := e.rt.Eval(`nk.run("early", { x = {} })`); err == nil {
		t.Fatal("table argument: want error")
	}
}

func TestIdleAttention(t *testing.T) {
	e := newEnv(t, Options{})
	e.sessions(store.Session{SessionID: "i", Attention: store.AttnIdle})
	if got := e.eval(`nk.session("i").attention`); got != "idle" {
		t.Fatalf("attention = %q", got)
	}
}
