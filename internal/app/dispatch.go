package app

import (
	"fmt"

	"github.com/aqilputi/nekuroddo/internal/config"
	"github.com/aqilputi/nekuroddo/internal/core"
)

// EvDispatched is published when a background agent starts; clients
// refresh their session list on it.
const EvDispatched = "dispatched"

// PermBypass is the permission mode that skips every permission check.
const PermBypass = "bypassPermissions"

// DispatchReq is one agent to launch in the background.
type DispatchReq struct {
	Dir      string `json:"dir"`
	Name     string `json:"name,omitempty"`      // claude --name ("" = from the prompt)
	Prompt   string `json:"prompt"`              // first message
	PermMode string `json:"perm_mode,omitempty"` // --permission-mode ("" = Claude Code's settings)
	TaskID   int    `json:"task_id,omitempty"`   // todo the agent works on (started and noted)
}

// Dispatched is the payload of dispatched and the Data of a successful
// Dispatch.
type Dispatched struct {
	ID     string `json:"id"` // short agent id (claude attach <id>)
	Dir    string `json:"dir"`
	Name   string `json:"name,omitempty"`
	TaskID int    `json:"task_id,omitempty"`
}

// DispatchNeedsConfirm reports whether dispatching with mode asks first:
// bypass does, unless it is the configured default (permission_mode).
// Clients use it to know Dispatch will only park a confirmation (and so
// returns at once) instead of launching claude.
func (a *App) DispatchNeedsConfirm(mode string) bool {
	return mode == PermBypass && a.cfg.PermissionMode != PermBypass
}

// Dispatch launches a background agent. A bypass that is not the
// configured default comes back as a Pending confirmation; otherwise (or on
// Confirm) claude runs, dispatched is published and the todo
// in TaskID, if any, is started and gets a note with the attach command.
// The toast is only in the Result: the client that ran it shows it, and the
// other clients learn of the agent from dispatched (one source per toast).
func (a *App) Dispatch(req DispatchReq) (Result, error) {
	if a.DispatchNeedsConfirm(req.PermMode) {
		q := fmt.Sprintf("disparar em BYPASS (sem nenhuma checagem de permissão) em %s?\nos hooks, como o guarded-access, continuam valendo", config.Tilde(req.Dir))
		return a.Ask(q, func() (Result, error) { return a.dispatchNow(req) }), nil
	}
	return a.dispatchNow(req)
}

func (a *App) dispatchNow(req DispatchReq) (Result, error) {
	var extra []string
	if req.PermMode != "" {
		extra = []string{"--permission-mode", req.PermMode}
	}
	id, err := a.ports.Claude.Dispatch(req.Dir, req.Name, req.Prompt, extra...)
	if err != nil {
		return Result{}, err
	}
	d := Dispatched{ID: id, Dir: req.Dir, Name: req.Name, TaskID: req.TaskID}
	text := "agent " + id + " disparado"
	a.Publish(EvDispatched, d)
	if req.TaskID > 0 {
		// best effort, as before: the agent runs even if the todo skill is missing
		_ = a.ports.Todo.Start(req.TaskID)
		_ = a.ports.Todo.AddNote(req.TaskID, "nekuroddo: agent "+id+" disparado (claude attach "+id+")")
	}
	return Result{Toast: text, Data: d}, nil
}

// RegisterDispatch adds session.new (args dir, prompt, perm, name) to the
// registry. todo.dispatch lives with the todo use cases and calls Dispatch.
func (a *App) RegisterDispatch() error {
	return a.reg.Register(core.Command{
		Name: "session.new", Title: "novo agent em background", Keys: []string{"c"}, Mode: core.ModeNormal, Confirm: true,
		Hint: "bypass pede confirmação, salvo se for o permission_mode configurado",
		Args: []core.Arg{
			{Name: "dir", Type: core.ArgDir, Prompt: "diretório › ", Required: true},
			{Name: "prompt", Type: core.ArgString, Prompt: "prompt › ", Required: true},
			{Name: "perm", Type: core.ArgString, Prompt: "permission-mode › "},
			{Name: "name", Type: core.ArgString, Prompt: "nome › "},
		},
	}, func(_ Context, args Args) (Result, error) {
		return a.Dispatch(DispatchReq{
			Dir: config.Expand(args.String("dir")), Name: args.String("name"),
			Prompt: args.String("prompt"), PermMode: args.String("perm"),
		})
	})
}
