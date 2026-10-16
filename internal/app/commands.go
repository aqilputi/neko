package app

// The commands the keymap needs that no slice registers (plan §2.4): the
// client ones (navigation, tabs, view, dialogs), listed so every client's
// keymap and palette show them, and three core ones (todo.dispatch,
// session.reply, sessions.refresh). Also: the keymap built from the
// registry (plan §2.7, GET /v1/keymap) and rebinding (Lua nk.keymap.set).

import (
	"fmt"

	"github.com/aqilputi/nekuroddo/internal/config"
	"github.com/aqilputi/nekuroddo/internal/core"
	"github.com/aqilputi/nekuroddo/internal/jira"
	"github.com/aqilputi/nekuroddo/internal/todo"
)

// clientCommand is a client-scope command bound to keys in NORMAL mode.
func clientCommand(name, title string, keys ...string) core.Command {
	return core.Command{Name: name, Title: title, Keys: keys, Mode: core.ModeNormal, Scope: core.ScopeClient}
}

// clientCommands are the plan's client-scope commands, in the order the
// help shows them. Their titles are the help texts.
func clientCommands() []core.Command {
	switcher := clientCommand("switcher.open", "ir p/ chat rodando/esperando", "ctrl+k")
	switcher.Mode = core.ModeAny // INSERT too: jump to another chat without leaving the pane
	return []core.Command{
		clientCommand("nav.down", "desce", "j", "down"),
		clientCommand("nav.up", "sobe", "k", "up"),
		clientCommand("nav.top", "topo", "gg"),
		clientCommand("nav.bottom", "fim", "G"),
		clientCommand("nav.half_down", "meia página ↓", "ctrl+d"),
		clientCommand("nav.half_up", "meia página ↑", "ctrl+u"),
		clientCommand("nav.page_down", "página ↓", "ctrl+f", "pgdown"),
		clientCommand("nav.page_up", "página ↑", "ctrl+b", "pgup"),
		clientCommand("nav.collapse", "fecha nó", "h", "left"),
		clientCommand("nav.expand", "abre nó", "l", "right"),
		clientCommand("nav.pick", "seleciona (p/ :group); no contexto: alterna", "space"),
		clientCommand("nav.next_attention", "próxima te esperando", "a"),
		clientCommand("nav.prev_attention", "anterior te esperando", "A"),
		clientCommand("session.insert", "INSERT no painel", "i"),
		clientCommand("tab.sessions", "sessões", "1"),
		clientCommand("tab.inbox", "inbox", "2"),
		clientCommand("tab.todos", "todos + jira", "3"),
		clientCommand("tab.archive", "arquivo", "4"),
		clientCommand("tab.next", "próxima aba", "tab"),
		clientCommand("tab.prev", "aba anterior", "shift+tab"),
		clientCommand("view.filter", "filtro fuzzy", "/"),
		clientCommand("view.show_all", "mostra paradas/arquivadas/subagents concluídos", "H"),
		clientCommand("view.sort", "ordem: atividade/custo/nome", "S"),
		clientCommand("view.theme", "tema", "T"),
		switcher,
		clientCommand("palette.open", "comandos", ":", "ctrl+p"),
		clientCommand("usage.show", "uso (/usage)", "u"),
		clientCommand("help.open", "todos os comandos", "ctrl+/", "?", "ctrl+_"),
		clientCommand("app.quit", "sai", "q", "ctrl+c"),
	}
}

// RegisterKeyCommands registers the client-scope commands and todo.dispatch,
// session.reply and sessions.refresh.
func (a *App) RegisterKeyCommands() error {
	for _, c := range clientCommands() {
		if err := a.reg.Register(c, nil); err != nil {
			return err
		}
	}
	hasSession := WithAvailable(func(ctx Context) bool { _, ok := a.paneSession(ctx.SessionID); return ok })
	onRow := WithAvailable(func(c Context) bool { return c.TodoID != 0 || c.IssueKey != "" })
	cmds := []struct {
		c    core.Command
		run  Runner
		opts []Option
	}{
		{core.Command{Name: "todo.dispatch", Title: "dispara um agent para a tarefa ou issue", Keys: []string{"enter"},
			Mode: core.ModeNormalTodos, Confirm: true, Hint: "marca start e anota o id do agent",
			Args: []core.Arg{
				{Name: "dir", Type: core.ArgDir, Prompt: "diretório › "},
				{Name: "perm", Type: core.ArgString, Prompt: "permission-mode › "},
			}},
			a.runTodoDispatch, []Option{onRow}},
		{core.Command{Name: "session.reply", Title: "resposta rápida", Keys: []string{"r"}, Mode: core.ModeNormal,
			Hint: "um número sozinho escolhe a opção da pergunta",
			Args: []core.Arg{{Name: "text", Type: core.ArgString, Prompt: "resposta › ", Required: true}}},
			a.runSessionReply, []Option{hasSession}},
		{core.Command{Name: "sessions.refresh", Title: "atualiza e reordena", Keys: []string{"R"}, Mode: core.ModeNormal},
			func(Context, Args) (Result, error) { a.Refresh(); return Result{}, nil }, nil},
		// core-scope so a compositor shortcut (`nk run inbox.summon`) and the
		// GUI's shift+space reach the same pick; the TUI keeps its own
		// handler (the inbox tab)
		{core.Command{Name: "inbox.summon", Title: "inbox: vai para quem te espera", Keys: []string{"shift+space"},
			Mode: core.ModeNormal, Hint: "a sessão mais urgente"},
			func(Context, Args) (Result, error) { return a.InboxSummon(), nil }, nil},
	}
	for _, c := range cmds {
		if err := a.reg.Register(c.c, c.run, c.opts...); err != nil {
			return err
		}
	}
	return nil
}

// runTodoDispatch launches an agent for the selected issue or task, in
// args["dir"] (default DispatchDir) with args["perm"].
func (a *App) runTodoDispatch(ctx Context, args Args) (Result, error) {
	dir := args.String("dir")
	if dir == "" {
		d, err := a.DispatchDir(ctx)
		if err != nil {
			return Result{}, err
		}
		dir = d
	}
	req := DispatchReq{Dir: config.Expand(dir), PermMode: args.String("perm")}
	if ctx.IssueKey != "" {
		is, err := a.issueOf(ctx)
		if err != nil {
			return Result{}, err
		}
		req.Name, req.Prompt = is.Key+" "+core.Truncate(is.Summary, 40), jira.AgentPrompt(is)
		return a.Dispatch(req)
	}
	t, err := a.todoOf(ctx)
	if err != nil {
		return Result{}, err
	}
	req.Name, req.Prompt, req.TaskID = fmt.Sprintf("todo #%d %s", t.ID, core.Truncate(t.Title, 40)), todo.Prompt(t), t.ID
	return a.Dispatch(req)
}

// runSessionReply plans showing the selected session (unfocused) with
// args["text"] typed once it is ready, resuming a history one first.
func (a *App) runSessionReply(ctx Context, args Args) (Result, error) {
	s, _ := a.paneSession(ctx.SessionID)
	text := core.ResolveOption(s.Questions, args.String("text"))
	pm := a.Panes()
	plan, err := pm.Open(s, false, text, nil)
	if err != nil {
		return Result{}, err
	}
	if plan.Resume {
		id, err := pm.Resume(plan.Dir, plan.SessionID)
		if err != nil {
			return Result{}, err
		}
		plan = pm.Attach(s, id, false, text)
	}
	return Result{Data: plan}, nil
}

// All lists every command, available or not, in registration order (the
// keymap binds keys whatever the selection).
func (r *Registry) All() []core.Command {
	r.mu.RLock()
	defer r.mu.RUnlock()
	out := make([]core.Command, 0, len(r.order))
	for _, n := range r.order {
		out = append(out, r.byName[n].cmd)
	}
	return out
}

// SetKeys rebinds a command (Lua nk.keymap.set): keys replace its keys (none
// unbinds it) and mode, when given, its mode.
func (r *Registry) SetKeys(name, mode string, keys []string) error {
	r.mu.Lock()
	defer r.mu.Unlock()
	e, ok := r.byName[name]
	if !ok {
		return fmt.Errorf("%w: %s", ErrUnknownCommand, name)
	}
	ne := *e
	ne.cmd.Keys = append([]string(nil), keys...)
	if mode != "" {
		ne.cmd.Mode = mode
	}
	r.byName[name] = &ne
	return nil
}

// Keymap is the keymap every client resolves keys with (plan §2.7): one
// binding per registered command with keys, whatever the selection.
func (a *App) Keymap() core.Keymap { return core.BuildKeymap(a.reg.All()) }
