package app_test

import (
	"errors"
	"reflect"
	"testing"

	"github.com/aqilputi/nekuroddo/internal/app"
	"github.com/aqilputi/nekuroddo/internal/core"
)

func echo(text string) app.Runner {
	return func(app.Context, app.Args) (app.Result, error) { return app.Info(text), nil }
}

func names(cs []core.Command) []string {
	var out []string
	for _, c := range cs {
		out = append(out, c.Name)
	}
	return out
}

func TestRegistryRegisterAndRun(t *testing.T) {
	r := app.NewRegistry()
	var gotCtx app.Context
	var gotArgs app.Args
	err := r.Register(core.Command{Name: "session.archive", Title: "arquiva"},
		func(ctx app.Context, args app.Args) (app.Result, error) {
			gotCtx, gotArgs = ctx, args
			return app.Info("ok"), nil
		})
	if err != nil {
		t.Fatal(err)
	}
	ctx := app.Context{SessionID: "s1", Picked: []string{"a"}, Tab: "sessions", TodoID: 3, IssueKey: "A-1"}
	res, err := r.Run(ctx, "session.archive", app.Args{"x": "1"})
	if err != nil || res.Toast != "ok" {
		t.Fatalf("Run = %+v, %v", res, err)
	}
	if !reflect.DeepEqual(gotCtx, ctx) || gotArgs["x"] != "1" {
		t.Fatalf("runner got %+v %+v", gotCtx, gotArgs)
	}

	c, ok := r.Lookup("session.archive")
	if !ok || c.Source != core.SourceBuiltin || c.Scope != core.ScopeCore {
		t.Fatalf("defaults not applied: %+v", c)
	}
	if _, ok := r.Lookup("nope"); ok {
		t.Fatal("Lookup unknown")
	}
}

func TestRegistryRejects(t *testing.T) {
	r := app.NewRegistry()
	if err := r.Register(core.Command{Name: "a"}, echo("")); err != nil {
		t.Fatal(err)
	}
	if err := r.Register(core.Command{Name: "a"}, echo("")); !errors.Is(err, app.ErrDuplicateCommand) {
		t.Fatalf("duplicate: %v", err)
	}
	if err := r.Register(core.Command{}, echo("")); err == nil {
		t.Fatal("empty name accepted")
	}
	if err := r.Register(core.Command{Name: "b"}, nil); err == nil {
		t.Fatal("core command without runner accepted")
	}
	if err := r.Register(core.Command{Name: "nav.up", Scope: core.ScopeClient}, nil); err != nil {
		t.Fatalf("client command needs no runner: %v", err)
	}
}

func TestRegistryRunErrors(t *testing.T) {
	r := app.NewRegistry()
	if _, err := r.Run(app.Context{}, "nope", nil); !errors.Is(err, app.ErrUnknownCommand) {
		t.Fatalf("unknown: %v", err)
	}
	_ = r.Register(core.Command{Name: "nav.up", Scope: core.ScopeClient}, nil)
	if _, err := r.Run(app.Context{}, "nav.up", nil); !errors.Is(err, app.ErrClientCommand) {
		t.Fatalf("client scope: %v", err)
	}
	_ = r.Register(core.Command{Name: "session.rename", Args: []core.Arg{{Name: "name", Required: true}}}, echo(""))
	if _, err := r.Run(app.Context{}, "session.rename", app.Args{}); !errors.Is(err, app.ErrMissingArg) {
		t.Fatalf("missing arg: %v", err)
	}
	if _, err := r.Run(app.Context{}, "session.rename", app.Args{"name": "x"}); err != nil {
		t.Fatal(err)
	}
	_ = r.Register(core.Command{Name: "jira.fetch"}, echo(""), app.WithAvailable(func(app.Context) bool { return false }))
	if _, err := r.Run(app.Context{}, "jira.fetch", nil); !errors.Is(err, app.ErrUnavailable) {
		t.Fatalf("unavailable: %v", err)
	}
	boom := errors.New("boom")
	_ = r.Register(core.Command{Name: "fails"}, func(app.Context, app.Args) (app.Result, error) { return app.Result{}, boom })
	if _, err := r.Run(app.Context{}, "fails", nil); !errors.Is(err, boom) {
		t.Fatalf("runner error: %v", err)
	}
}

func TestRegistryListFiltersAvailableInOrder(t *testing.T) {
	r := app.NewRegistry()
	_ = r.Register(core.Command{Name: "b"}, echo(""))
	_ = r.Register(core.Command{Name: "todo.start"}, echo(""),
		app.WithAvailable(func(ctx app.Context) bool { return ctx.TodoID != 0 }))
	_ = r.Register(core.Command{Name: "a"}, echo(""))

	if got := names(r.List(app.Context{})); !reflect.DeepEqual(got, []string{"b", "a"}) {
		t.Fatalf("List without todo = %v", got)
	}
	if got := names(r.List(app.Context{TodoID: 7})); !reflect.DeepEqual(got, []string{"b", "todo.start", "a"}) {
		t.Fatalf("List with todo = %v", got)
	}
}

func TestRegistryDynamicLookup(t *testing.T) {
	r := app.NewRegistry()
	_ = r.Register(core.Command{Name: "x"}, echo("v1"))

	// Wrap (what Lua will do) changes what the next Run executes.
	err := r.Wrap("x", func(next app.Runner) app.Runner {
		return func(ctx app.Context, args app.Args) (app.Result, error) {
			res, err := next(ctx, args)
			res.Toast += "+lua"
			return res, err
		}
	})
	if err != nil {
		t.Fatal(err)
	}
	if res, _ := r.Run(app.Context{}, "x", nil); res.Toast != "v1+lua" {
		t.Fatalf("after Wrap = %q", res.Toast)
	}
	if err := r.Wrap("nope", func(n app.Runner) app.Runner { return n }); !errors.Is(err, app.ErrUnknownCommand) {
		t.Fatalf("Wrap unknown: %v", err)
	}
	_ = r.Register(core.Command{Name: "nav.up", Scope: core.ScopeClient}, nil)
	if err := r.Wrap("nav.up", func(n app.Runner) app.Runner { return n }); !errors.Is(err, app.ErrClientCommand) {
		t.Fatalf("Wrap client: %v", err)
	}

	// Unregister + Register replaces it (Lua reload).
	if !r.Unregister("x") || r.Unregister("x") {
		t.Fatal("Unregister")
	}
	if _, err := r.Run(app.Context{}, "x", nil); !errors.Is(err, app.ErrUnknownCommand) {
		t.Fatalf("after Unregister: %v", err)
	}
	_ = r.Register(core.Command{Name: "x", Source: "lua:init.lua"}, echo("v2"))
	if res, _ := r.Run(app.Context{}, "x", nil); res.Toast != "v2" {
		t.Fatalf("after re-Register = %q", res.Toast)
	}
	if c, _ := r.Lookup("x"); c.Source != "lua:init.lua" {
		t.Fatalf("Source = %q", c.Source)
	}
	if got := names(r.List(app.Context{})); !reflect.DeepEqual(got, []string{"nav.up", "x"}) {
		t.Fatalf("order after re-Register = %v", got)
	}
}

func TestRegistryRunnerMayUseRegistry(t *testing.T) {
	// No lock is held while a runner runs: it may call back into the registry.
	r := app.NewRegistry()
	_ = r.Register(core.Command{Name: "inner"}, echo("in"))
	_ = r.Register(core.Command{Name: "outer"}, func(ctx app.Context, args app.Args) (app.Result, error) {
		return r.Run(ctx, "inner", args)
	})
	if res, err := r.Run(app.Context{}, "outer", nil); err != nil || res.Toast != "in" {
		t.Fatalf("nested Run = %+v, %v", res, err)
	}
}
