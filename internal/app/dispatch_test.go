package app_test

import (
	"errors"
	"reflect"
	"strings"
	"testing"

	"github.com/aqilputi/nekuroddo/internal/app"
	"github.com/aqilputi/nekuroddo/internal/app/apptest"
	"github.com/aqilputi/nekuroddo/internal/config"
)

func newAppCfg(t *testing.T, cfg config.Config) (*app.App, *apptest.Fakes) {
	t.Helper()
	f := apptest.New()
	a := app.New(cfg, f.Ports())
	t.Cleanup(a.Close)
	return a, f
}

func TestDispatchPlainHasNoModeFlag(t *testing.T) {
	a, f := newApp(t)
	f.Claude.NextID = "abcd1234"
	ev := a.Subscribe(t.Context())
	res, err := a.Dispatch(app.DispatchReq{Dir: "/r", Name: "n", Prompt: "oi"})
	if err != nil || res.Pending != nil {
		t.Fatalf("Dispatch = %+v, %v", res, err)
	}
	if res.Toast != "agent abcd1234 disparado" || res.Err {
		t.Fatalf("toast = %+v", res)
	}
	if d, ok := res.Data.(app.Dispatched); !ok || d.ID != "abcd1234" || d.TaskID != 0 {
		t.Fatalf("data = %+v", res.Data)
	}
	c := f.Claude.Last()
	if want := []any{"/r", "n", "oi", []string(nil)}; c.Method != "Dispatch" || !reflect.DeepEqual(c.Args, want) {
		t.Fatalf("claude call = %+v", c)
	}
	if e := next(t, ev, app.EvDispatched); e.Data != (app.Dispatched{ID: "abcd1234", Dir: "/r", Name: "n"}) {
		t.Fatalf("dispatched = %+v", e.Data)
	}
	if e := next(t, ev, app.EvToast); e.Data != (app.ToastData{Text: "agent abcd1234 disparado"}) {
		t.Fatalf("toast = %+v", e.Data)
	}
	if f.Todo.Count("Start") != 0 {
		t.Fatal("no task: the todo list is left alone")
	}
}

func TestDispatchPassesPermissionMode(t *testing.T) {
	a, f := newApp(t)
	f.Claude.NextID = "x1"
	if _, err := a.Dispatch(app.DispatchReq{Dir: "/r", Prompt: "p", PermMode: "acceptEdits"}); err != nil {
		t.Fatal(err)
	}
	if got := f.Claude.Last().Args[3]; !reflect.DeepEqual(got, []string{"--permission-mode", "acceptEdits"}) {
		t.Fatalf("extra = %v", got)
	}
}

func TestDispatchBypassAsksThenRunsOnConfirm(t *testing.T) {
	a, f := newApp(t)
	f.Claude.NextID = "b1"
	if !a.DispatchNeedsConfirm("bypassPermissions") || a.DispatchNeedsConfirm("acceptEdits") || a.DispatchNeedsConfirm("") {
		t.Fatal("only bypass (not the default) needs a confirmation")
	}
	res, err := a.Dispatch(app.DispatchReq{Dir: "/tmp/x", Prompt: "faz tudo", PermMode: "bypassPermissions", TaskID: 7})
	if err != nil || res.Pending == nil {
		t.Fatalf("bypass must ask: %+v, %v", res, err)
	}
	want := "disparar em BYPASS (sem nenhuma checagem de permissão) em /tmp/x?\nos hooks, como o guarded-access, continuam valendo"
	if res.Pending.Question != want {
		t.Fatalf("question = %q", res.Pending.Question)
	}
	if f.Claude.Count("Dispatch") != 0 {
		t.Fatal("nothing runs before the answer")
	}
	out, err := a.Confirm(res.Pending.Token)
	if err != nil || out.Data.(app.Dispatched).ID != "b1" || out.Data.(app.Dispatched).TaskID != 7 {
		t.Fatalf("Confirm = %+v, %v", out, err)
	}
	if got := f.Claude.Last().Args[3]; !reflect.DeepEqual(got, []string{"--permission-mode", "bypassPermissions"}) {
		t.Fatalf("extra = %v", got)
	}
	if _, err := a.Confirm(res.Pending.Token); !errors.Is(err, app.ErrUnknownToken) {
		t.Fatalf("token is single use: %v", err)
	}
}

func TestDispatchBypassQuestionTildesHome(t *testing.T) {
	a, _ := newApp(t)
	t.Setenv("HOME", "/home/u")
	res, _ := a.Dispatch(app.DispatchReq{Dir: "/home/u/r", Prompt: "p", PermMode: "bypassPermissions"})
	if res.Pending == nil || !strings.Contains(res.Pending.Question, " em "+config.Tilde("/home/u/r")+"?") {
		t.Fatalf("question = %+v", res.Pending)
	}
}

func TestDispatchBypassCancelAndExpiry(t *testing.T) {
	a, f := newApp(t)
	res, _ := a.Dispatch(app.DispatchReq{Dir: "/r", Prompt: "p", PermMode: "bypassPermissions"})
	if err := a.Cancel(res.Pending.Token); err != nil {
		t.Fatal(err)
	}
	res, _ = a.Dispatch(app.DispatchReq{Dir: "/r", Prompt: "p", PermMode: "bypassPermissions"})
	f.Clock.Advance(app.ConfirmTTL)
	if _, err := a.Confirm(res.Pending.Token); !errors.Is(err, app.ErrExpired) {
		t.Fatalf("expired token: %v", err)
	}
	if f.Claude.Count("Dispatch") != 0 {
		t.Fatal("cancelled/expired: never dispatched")
	}
}

func TestDispatchBypassAsDefaultRunsStraight(t *testing.T) {
	cfg := config.Default()
	cfg.PermissionMode = "bypassPermissions"
	a, f := newAppCfg(t, cfg)
	f.Claude.NextID = "z9"
	if a.DispatchNeedsConfirm("bypassPermissions") {
		t.Fatal("configured default never asks")
	}
	res, err := a.Dispatch(app.DispatchReq{Dir: "/r", Prompt: "vai", PermMode: "bypassPermissions"})
	if err != nil || res.Pending != nil || f.Claude.Count("Dispatch") != 1 {
		t.Fatalf("Dispatch = %+v, %v", res, err)
	}
}

func TestDispatchLinksTodo(t *testing.T) {
	a, f := newApp(t)
	f.Claude.NextID = "abcd1234"
	f.Todo.Errs = map[string]error{"Start": errors.New("skill missing")} // ignored, as today
	res, err := a.Dispatch(app.DispatchReq{Dir: "/r", Name: "todo #3 x", Prompt: "p", TaskID: 3})
	if err != nil || res.Data.(app.Dispatched).TaskID != 3 {
		t.Fatalf("Dispatch = %+v, %v", res, err)
	}
	if c := f.Todo.CallsTo("Start"); len(c) != 1 || c[0].Args[0] != 3 {
		t.Fatalf("Start = %+v", c)
	}
	notes := f.Todo.CallsTo("AddNote")
	if len(notes) != 1 || notes[0].Args[0] != 3 || notes[0].Args[1] != "nekuroddo: agent abcd1234 disparado (claude attach abcd1234)" {
		t.Fatalf("AddNote = %+v", notes)
	}
}

func TestDispatchFailure(t *testing.T) {
	a, f := newApp(t)
	f.Claude.Errs = map[string]error{"Dispatch": errors.New("boom")}
	ev := a.Subscribe(t.Context())
	res, err := a.Dispatch(app.DispatchReq{Dir: "/r", Prompt: "p", TaskID: 2})
	if err == nil || err.Error() != "boom" || res.Data != nil {
		t.Fatalf("Dispatch = %+v, %v", res, err)
	}
	if f.Todo.Count("Start") != 0 {
		t.Fatal("a failed dispatch does not touch the todo")
	}
	a.Publish("probe", nil)
	if e := recv(t, ev); e.Name != "probe" {
		t.Fatalf("no dispatched/toast on failure, got %s", e.Name)
	}
}

func TestSessionNewCommand(t *testing.T) {
	a, f := newApp(t)
	f.Claude.NextID = "c1"
	if err := a.RegisterDispatch(); err != nil {
		t.Fatal(err)
	}
	c, ok := a.Registry().Lookup("session.new")
	if !ok || !c.Confirm || len(c.Args) != 4 {
		t.Fatalf("session.new = %+v", c)
	}
	if _, err := a.Run(app.Context{}, "session.new", app.Args{"dir": "/r"}); !errors.Is(err, app.ErrMissingArg) {
		t.Fatalf("prompt is required: %v", err)
	}
	res, err := a.Run(app.Context{}, "session.new", app.Args{"dir": "/r", "prompt": "oi", "perm": "plan", "name": "nm"})
	if err != nil || res.Data.(app.Dispatched).ID != "c1" {
		t.Fatalf("Run = %+v, %v", res, err)
	}
	if want := []any{"/r", "nm", "oi", []string{"--permission-mode", "plan"}}; !reflect.DeepEqual(f.Claude.Last().Args, want) {
		t.Fatalf("claude call = %+v", f.Claude.Last())
	}
	res, err = a.Run(app.Context{}, "session.new", app.Args{"dir": "/r", "prompt": "oi", "perm": "bypassPermissions"})
	if err != nil || res.Pending == nil {
		t.Fatalf("bypass via command asks: %+v, %v", res, err)
	}
	if err := a.RegisterDispatch(); !errors.Is(err, app.ErrDuplicateCommand) {
		t.Fatalf("second register: %v", err)
	}
}
