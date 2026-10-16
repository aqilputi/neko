package app

// Slice "sessions": archive, stop, remove, rename and move sessions
// (WP-2.2). The texts are the TUI's, unchanged.

import (
	"errors"
	"fmt"
	"strings"

	"github.com/aqilputi/nekuroddo/internal/core"
	"github.com/aqilputi/nekuroddo/internal/store"
)

// ErrNoSession: the command needs a session the snapshot has.
var ErrNoSession = errors.New("nenhuma sessão selecionada")

// TabArchive is Context.Tab in the archive tab (a section there restores).
const TabArchive = "archive"

// SessionEffect is the Data of a session command's Result: what the client
// should do besides showing the toast. On a Result that asks (Pending),
// ClosePanes lists the panes to close when the client says yes, before the
// core runs the action; on the final Result it repeats them.
type SessionEffect struct {
	ClosePanes []string `json:"close_panes,omitempty"` // pane (short) ids
	Refresh    bool     `json:"refresh,omitempty"`     // the effect shows after the next refresh
}

func refreshed(toast string) Result { return Result{Toast: toast, Data: SessionEffect{Refresh: true}} }

// session finds id in the snapshot; a.mu must be held.
func (a *App) sessionLocked(id string) *store.Session {
	if id == "" {
		return nil
	}
	for i := range a.snap.Sessions {
		if a.snap.Sessions[i].SessionID == id {
			return &a.snap.Sessions[i]
		}
	}
	return nil
}

// lookup copies a session out of the snapshot.
func (a *App) lookup(id string) (store.Session, bool) {
	a.mu.RLock()
	defer a.mu.RUnlock()
	if s := a.sessionLocked(id); s != nil {
		return *s, true
	}
	return store.Session{}, false
}

// sessionsWhere lists the ids of the snapshot's sessions that keep says yes to.
func (a *App) sessionsWhere(id func(store.Session) string) []string {
	a.mu.RLock()
	defer a.mu.RUnlock()
	var out []string
	for _, s := range a.snap.Sessions {
		if v := id(s); v != "" {
			out = append(out, v)
		}
	}
	return out
}

// snapshotChanged publishes snapshot.changed for an in-place change.
func (a *App) snapshotChanged() {
	a.mu.RLock()
	d := SnapshotChanged{At: a.clock.Now(), Sessions: len(a.snap.Sessions)}
	a.mu.RUnlock()
	a.Publish(EvSnapshotChanged, d)
}

// ToggleArchive is X. With section == "" it archives (or restores) the
// session under the cursor right away. With a section it asks, then archives
// every session listed under it (ctx.Picked; if empty, the snapshot's
// sessions in that context), or restores them in the archive tab.
func (a *App) ToggleArchive(ctx Context, section string) (Result, error) {
	if section != "" {
		return a.archiveSection(ctx, section), nil
	}
	a.mu.Lock()
	s := a.sessionLocked(ctx.SessionID)
	if s == nil {
		a.mu.Unlock()
		return Result{}, fmt.Errorf("%w: %q", ErrNoSession, ctx.SessionID)
	}
	id, restore, busy := s.SessionID, s.Archived, s.State == "busy"
	s.Archived, s.Stale = !restore, !restore
	a.mu.Unlock()
	if restore {
		a.ports.Sessions.Unarchive(id)
		a.snapshotChanged()
		return refreshed("restaurada"), nil
	}
	a.ports.Sessions.Archive(id)
	a.snapshotChanged()
	if busy {
		return refreshed("arquivada, mas está rodando: volta assim que tiver atividade nova"), nil
	}
	return refreshed("arquivada (aba 4 Arquivo) — volta sozinha se tiver atividade nova"), nil
}

func (a *App) archiveSection(ctx Context, section string) Result {
	restore := ctx.Tab == TabArchive
	ids := ctx.Picked
	if len(ids) == 0 {
		ids = a.sessionsWhere(func(s store.Session) string {
			if s.Context == section && s.Archived == restore {
				return s.SessionID
			}
			return ""
		})
	}
	if len(ids) == 0 {
		return Result{}
	}
	ids = append([]string(nil), ids...)
	verb := "arquivar"
	if restore {
		verb = "restaurar"
	}
	return a.Ask(fmt.Sprintf("%s as %d sessões de \"%s\"?", verb, len(ids), section), func() (Result, error) {
		if restore {
			a.ports.Sessions.Unarchive(ids...)
		} else {
			a.ports.Sessions.Archive(ids...)
		}
		return refreshed(fmt.Sprintf("%d sessões: %s ✓", len(ids), verb)), nil
	})
}

// ArchiveStale asks, then archives every stale session that is neither
// archived yet nor busy.
func (a *App) ArchiveStale() (Result, error) {
	ids := a.sessionsWhere(func(s store.Session) string {
		if s.Stale && !s.Archived && s.State != "busy" {
			return s.SessionID
		}
		return ""
	})
	if len(ids) == 0 {
		return Info("nenhuma sessão parada para arquivar"), nil
	}
	q := fmt.Sprintf("arquivar as %d sessões paradas há mais de %.0fd?", len(ids), a.cfg.StaleDays)
	return a.Ask(q, func() (Result, error) {
		a.ports.Sessions.Archive(ids...)
		return refreshed(fmt.Sprintf("%d sessões arquivadas ✓", len(ids))), nil
	}), nil
}

// RemoveEnded asks, then runs `claude rm` on background chats whose
// conversation was ended (they take no more messages, but their process
// lingers). A failing rm does not stop the others; the toast lists them.
func (a *App) RemoveEnded() (Result, error) {
	ids := a.sessionsWhere(func(s store.Session) string {
		if s.Ended {
			return s.ShortID
		}
		return ""
	})
	if len(ids) == 0 {
		return Info("nenhum chat encerrado com processo sobrando"), nil
	}
	q := fmt.Sprintf("remover %d chats encerrados (claude rm)? a conversa já não aceita mensagens", len(ids))
	res := a.Ask(q, func() (Result, error) {
		var failed []string
		for _, id := range ids {
			if err := a.ports.Claude.Remove(id); err != nil {
				failed = append(failed, id)
			}
		}
		if len(failed) > 0 {
			return Result{Toast: "falhou: " + strings.Join(failed, " "), Err: true,
				Data: SessionEffect{ClosePanes: ids}}, nil
		}
		return Result{Data: SessionEffect{ClosePanes: ids, Refresh: true}}, nil
	})
	res.Data = SessionEffect{ClosePanes: ids}
	return res, nil
}

// background is the selected background session, or a failing Result.
func (a *App) background(ctx Context, verb string) (store.Session, *Result) {
	s, ok := a.lookup(ctx.SessionID)
	if !ok || s.ShortID == "" {
		r := Result{Toast: "só sessões em background podem ser " + verb, Err: true}
		return s, &r
	}
	return s, nil
}

// Stop asks, then stops the selected background session (`claude stop`;
// the conversation stays saved).
func (a *App) Stop(ctx Context) (Result, error) {
	s, bad := a.background(ctx, "paradas")
	if bad != nil {
		return *bad, nil
	}
	id := s.ShortID
	return a.Ask("parar \""+core.Truncate(s.Label(), 50)+"\"? (a conversa fica salva)", func() (Result, error) {
		if err := a.ports.Claude.Stop(id); err != nil {
			return Fail(err), nil
		}
		return Result{Data: SessionEffect{Refresh: true}}, nil
	}), nil
}

// Remove asks, then removes the selected background session (`claude rm`,
// which also drops its worktree when safe). Its pane closes on yes.
func (a *App) Remove(ctx Context) (Result, error) {
	s, bad := a.background(ctx, "removidas")
	if bad != nil {
		return *bad, nil
	}
	id := s.ShortID
	panes := []string{id}
	res := a.Ask("remover \""+core.Truncate(s.Label(), 50)+"\" (e o worktree, se seguro)?", func() (Result, error) {
		if err := a.ports.Claude.Remove(id); err != nil {
			r := Fail(err)
			r.Data = SessionEffect{ClosePanes: panes}
			return r, nil
		}
		return Result{Data: SessionEffect{ClosePanes: panes, Refresh: true}}, nil
	})
	res.Data = SessionEffect{ClosePanes: panes}
	return res, nil
}

// Rename names a session ("" goes back to claude's name).
func (a *App) Rename(sessionID, name string) (Result, error) {
	if sessionID == "" {
		return Result{}, ErrNoSession
	}
	a.ports.Sessions.Rename(sessionID, name)
	return a.afterOverride("renomeada"), nil
}

// Move puts a session in another context ("" goes back to the directory rule).
func (a *App) Move(sessionID, context string) (Result, error) {
	if sessionID == "" {
		return Result{}, ErrNoSession
	}
	a.ports.Sessions.SetContext(sessionID, context)
	if context == "" {
		return a.afterOverride("contexto volta à regra do diretório"), nil
	}
	return a.afterOverride("movida para " + context), nil
}

// RenameContext renames a section of the tree, as shown now ("" undoes a
// rename). The same name does nothing.
func (a *App) RenameContext(shown, name string) (Result, error) {
	if name == shown {
		return Result{}, nil
	}
	a.ports.Sessions.RenameContext(shown, name)
	return a.afterOverride("seção renomeada"), nil
}

// afterOverride shows a name/context change right away, without waiting for
// the next refresh: the snapshot is reapplied in place.
func (a *App) afterOverride(toast string) Result {
	a.mu.Lock()
	a.ports.Sessions.Reapply(a.snap.Sessions)
	a.mu.Unlock()
	a.snapshotChanged()
	return refreshed(toast)
}

// RegisterSessionCommands adds the slice's commands to the registry
// (plan §2.4 names).
func (a *App) RegisterSessionCommands() error {
	hasSession := WithAvailable(func(c Context) bool { return c.SessionID != "" })
	type reg struct {
		cmd  core.Command
		run  Runner
		opts []Option
	}
	regs := []reg{
		{core.Command{Name: "session.archive", Title: "arquiva/restaura (no contexto: todas)", Keys: []string{"X"},
			Mode: core.ModeNormal, Confirm: true,
			Args: []core.Arg{{Name: "context", Type: core.ArgString, Prompt: "contexto"}}},
			func(c Context, args Args) (Result, error) { return a.ToggleArchive(c, args.String("context")) }, nil},
		{core.Command{Name: "sessions.archive_stale", Title: "arquivar todas as sessões paradas", Confirm: true},
			func(Context, Args) (Result, error) { return a.ArchiveStale() }, nil},
		{core.Command{Name: "sessions.remove_ended", Title: "limpar chats encerrados (claude rm)", Confirm: true},
			func(Context, Args) (Result, error) { return a.RemoveEnded() }, nil},
		{core.Command{Name: "session.stop", Title: "parar a sessão em background", Keys: []string{"x"},
			Mode: core.ModeNormal, Confirm: true},
			func(c Context, _ Args) (Result, error) { return a.Stop(c) }, []Option{hasSession}},
		{core.Command{Name: "session.remove", Title: "remover a sessão em background", Keys: []string{"dd"},
			Mode: core.ModeNormal, Confirm: true},
			func(c Context, _ Args) (Result, error) { return a.Remove(c) }, []Option{hasSession}},
		{core.Command{Name: "session.rename", Title: "renomear a sessão", Keys: []string{"n"}, Mode: core.ModeNormal,
			Args: []core.Arg{{Name: "name", Type: core.ArgString, Prompt: "nome"}}},
			func(c Context, args Args) (Result, error) { return a.Rename(c.SessionID, args.String("name")) },
			[]Option{hasSession}},
		{core.Command{Name: "session.move", Title: "mover a sessão para outro contexto", Keys: []string{"m"},
			Mode: core.ModeNormal, Args: []core.Arg{{Name: "context", Type: core.ArgString, Prompt: "contexto"}}},
			func(c Context, args Args) (Result, error) { return a.Move(c.SessionID, args.String("context")) },
			[]Option{hasSession}},
	}
	for _, r := range regs {
		if err := a.reg.Register(r.cmd, r.run, r.opts...); err != nil {
			return err
		}
	}
	return nil
}
