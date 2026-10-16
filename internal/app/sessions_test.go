package app_test

import (
	"context"
	"errors"
	"reflect"
	"strings"
	"testing"
	"time"

	"github.com/aqilputi/nekuroddo/internal/app"
	"github.com/aqilputi/nekuroddo/internal/app/apptest"
	"github.com/aqilputi/nekuroddo/internal/config"
	"github.com/aqilputi/nekuroddo/internal/store"
)

// sessApp is an app over fakes holding ss as its current snapshot.
func sessApp(t *testing.T, ss ...store.Session) (*app.App, *apptest.Fakes) {
	t.Helper()
	f := apptest.New()
	a := app.New(config.Default(), f.Ports())
	t.Cleanup(a.Close)
	a.UseSnapshot(store.Snapshot{Sessions: ss})
	return a, f
}

// sessFixture: "a" background waiting, "b" interactive busy, "c" and "d"
// in another context, "d" stale, "e" stale but busy, "f" stale and archived.
func sessFixture() []store.Session {
	return []store.Session{
		{SessionID: "a", ShortID: "aaaa1111", Name: "alpha", Context: "c1", State: "blocked"},
		{SessionID: "b", Name: "bravo", Context: "c1", State: "busy"},
		{SessionID: "c", Name: "charlie", Context: "c2", State: "done"},
		{SessionID: "d", Name: "delta", Context: "c2", State: "idle", Stale: true},
		{SessionID: "e", Name: "echo", Context: "c3", State: "busy", Stale: true},
		{SessionID: "f", Name: "fox", Context: "c3", State: "idle", Stale: true, Archived: true},
	}
}

func sessEffect(t *testing.T, res app.Result) app.SessionEffect {
	t.Helper()
	e, ok := res.Data.(app.SessionEffect)
	if !ok {
		t.Fatalf("Data is %T, want SessionEffect", res.Data)
	}
	return e
}

func sessByID(a *app.App, id string) store.Session {
	for _, s := range a.Snapshot().Sessions {
		if s.SessionID == id {
			return s
		}
	}
	return store.Session{}
}

// watch collects event names published while the test runs.
func sessWatch(t *testing.T, a *app.App) func() []string {
	t.Helper()
	ctx, cancel := context.WithCancel(context.Background())
	t.Cleanup(cancel)
	ch := a.Subscribe(ctx)
	return func() []string {
		var out []string
		for {
			select {
			case ev := <-ch:
				out = append(out, ev.Name)
			case <-time.After(20 * time.Millisecond):
				return out
			}
		}
	}
}

func sessHas(xs []string, x string) bool {
	for _, v := range xs {
		if v == x {
			return true
		}
	}
	return false
}

func TestToggleArchiveSession(t *testing.T) {
	a, f := sessApp(t, sessFixture()...)
	events := sessWatch(t, a)

	res, err := a.ToggleArchive(app.Context{SessionID: "c"}, "")
	if err != nil || res.Pending != nil || res.Err {
		t.Fatalf("archiving one session is immediate: %+v %v", res, err)
	}
	if !strings.Contains(res.Toast, "arquivada (aba 4 Arquivo)") || !sessEffect(t, res).Refresh {
		t.Fatalf("toast: %+v", res)
	}
	if got := f.Sessions.Last(); got.Method != "Archive" || !reflect.DeepEqual(got.Args[0], []string{"c"}) {
		t.Fatalf("store call: %+v", got)
	}
	if s := sessByID(a, "c"); !s.Archived || !s.Stale {
		t.Fatalf("snapshot shows it archived right away: %+v", s)
	}
	if !sessHas(events(), app.EvSnapshotChanged) {
		t.Fatal("snapshot.changed published")
	}

	// a busy one warns it comes back
	res, _ = a.ToggleArchive(app.Context{SessionID: "b"}, "")
	if !strings.Contains(res.Toast, "está rodando") {
		t.Fatalf("busy: %q", res.Toast)
	}

	// an archived one is restored
	res, _ = a.ToggleArchive(app.Context{SessionID: "f"}, "")
	if res.Toast != "restaurada" || f.Sessions.Last().Method != "Unarchive" {
		t.Fatalf("restore: %+v %+v", res, f.Sessions.Last())
	}
	if s := sessByID(a, "f"); s.Archived || s.Stale {
		t.Fatalf("restored in the snapshot: %+v", s)
	}

	// unknown session
	if _, err := a.ToggleArchive(app.Context{SessionID: "zz"}, ""); !errors.Is(err, app.ErrNoSession) {
		t.Fatalf("unknown: %v", err)
	}
	if _, err := a.ToggleArchive(app.Context{}, ""); !errors.Is(err, app.ErrNoSession) {
		t.Fatalf("nothing selected: %v", err)
	}
}

func TestToggleArchiveContextAsks(t *testing.T) {
	a, f := sessApp(t, sessFixture()...)

	// the client lists what is under the section (Picked)
	res, err := a.ToggleArchive(app.Context{Picked: []string{"a", "b"}}, "c1")
	if err != nil || res.Pending == nil {
		t.Fatalf("a whole context asks: %+v %v", res, err)
	}
	if res.Pending.Question != `arquivar as 2 sessões de "c1"?` {
		t.Fatalf("question: %q", res.Pending.Question)
	}
	if f.Sessions.Count("Archive") != 0 {
		t.Fatal("nothing archived before yes")
	}
	res, err = a.Confirm(res.Pending.Token)
	if err != nil || res.Toast != "2 sessões: arquivar ✓" || !sessEffect(t, res).Refresh {
		t.Fatalf("yes: %+v %v", res, err)
	}
	if got := f.Sessions.Last(); got.Method != "Archive" || !reflect.DeepEqual(got.Args[0], []string{"a", "b"}) {
		t.Fatalf("store: %+v", got)
	}

	// in the archive tab it restores; without Picked the core lists them
	res, _ = a.ToggleArchive(app.Context{Tab: "archive"}, "c3")
	if res.Pending == nil || res.Pending.Question != `restaurar as 1 sessões de "c3"?` {
		t.Fatalf("restore asks: %+v", res.Pending)
	}
	res, _ = a.Confirm(res.Pending.Token)
	if res.Toast != "1 sessões: restaurar ✓" {
		t.Fatalf("restored: %q", res.Toast)
	}
	if got := f.Sessions.Last(); got.Method != "Unarchive" || !reflect.DeepEqual(got.Args[0], []string{"f"}) {
		t.Fatalf("store: %+v", got)
	}

	// a section with nothing listed does nothing
	res, err = a.ToggleArchive(app.Context{}, "nope")
	if err != nil || res.Pending != nil || res.Toast != "" {
		t.Fatalf("empty section: %+v %v", res, err)
	}
}

func TestArchiveStale(t *testing.T) {
	a, f := sessApp(t, sessFixture()...)
	res, err := a.ArchiveStale()
	if err != nil || res.Pending == nil {
		t.Fatalf("asks: %+v %v", res, err)
	}
	if res.Pending.Question != "arquivar as 1 sessões paradas há mais de 3d?" {
		t.Fatalf("question: %q", res.Pending.Question)
	}
	res, _ = a.Confirm(res.Pending.Token)
	if res.Toast != "1 sessões arquivadas ✓" || !sessEffect(t, res).Refresh {
		t.Fatalf("yes: %+v", res)
	}
	// only d: e is busy, f already archived
	if got := f.Sessions.Last(); got.Method != "Archive" || !reflect.DeepEqual(got.Args[0], []string{"d"}) {
		t.Fatalf("store: %+v", got)
	}

	b, _ := sessApp(t, sessFixture()[:3]...)
	res, err = b.ArchiveStale()
	if err != nil || res.Pending != nil || res.Toast != "nenhuma sessão parada para arquivar" {
		t.Fatalf("nothing stale: %+v %v", res, err)
	}
}

func TestRemoveEnded(t *testing.T) {
	ss := sessFixture()
	ss[0].Ended = true
	ss = append(ss, store.Session{SessionID: "g", ShortID: "gggg2222", Ended: true},
		store.Session{SessionID: "h", Ended: true}) // no process: skipped
	a, f := sessApp(t, ss...)
	res, err := a.RemoveEnded()
	if err != nil || res.Pending == nil {
		t.Fatalf("asks: %+v %v", res, err)
	}
	if !strings.Contains(res.Pending.Question, "remover 2 chats encerrados (claude rm)?") {
		t.Fatalf("question: %q", res.Pending.Question)
	}
	want := []string{"aaaa1111", "gggg2222"}
	if got := sessEffect(t, res).ClosePanes; !reflect.DeepEqual(got, want) {
		t.Fatalf("panes to close on yes: %v", got)
	}
	if f.Claude.Count("Remove") != 0 {
		t.Fatal("nothing removed before yes")
	}
	res, err = a.Confirm(res.Pending.Token)
	if err != nil || res.Toast != "" || res.Err {
		t.Fatalf("yes: %+v %v", res, err)
	}
	if e := sessEffect(t, res); !e.Refresh || !reflect.DeepEqual(e.ClosePanes, want) {
		t.Fatalf("effect: %+v", e)
	}
	if f.Claude.Count("Remove") != 2 {
		t.Fatalf("claude rm each: %v", f.Claude.Calls())
	}

	// partial failures are reported; the rest still runs
	f.Claude.Errs = map[string]error{"Remove": errors.New("boom")}
	res, _ = a.RemoveEnded()
	res, _ = a.Confirm(res.Pending.Token)
	if !res.Err || res.Toast != "falhou: aaaa1111 gggg2222" {
		t.Fatalf("failures: %+v", res)
	}

	b, _ := sessApp(t, sessFixture()...)
	res, _ = b.RemoveEnded()
	if res.Pending != nil || res.Toast != "nenhum chat encerrado com processo sobrando" {
		t.Fatalf("nothing ended: %+v", res)
	}
}

func TestStopAsks(t *testing.T) {
	a, f := sessApp(t, sessFixture()...)
	res, err := a.Stop(app.Context{SessionID: "a"})
	if err != nil || res.Pending == nil || res.Pending.Question != `parar "alpha"? (a conversa fica salva)` {
		t.Fatalf("asks: %+v %v", res, err)
	}
	if f.Claude.Count("Stop") != 0 {
		t.Fatal("not before yes")
	}
	res, _ = a.Confirm(res.Pending.Token)
	if res.Toast != "" || !sessEffect(t, res).Refresh {
		t.Fatalf("yes: %+v", res)
	}
	if got := f.Claude.Last(); got.Method != "Stop" || got.Args[0] != "aaaa1111" {
		t.Fatalf("claude stop: %+v", got)
	}

	f.Claude.Errs = map[string]error{"Stop": errors.New("claude stop aaaa1111: nope")}
	res, _ = a.Stop(app.Context{SessionID: "a"})
	res, _ = a.Confirm(res.Pending.Token)
	if !res.Err || res.Toast != "claude stop aaaa1111: nope" {
		t.Fatalf("failure: %+v", res)
	}

	// only background sessions
	res, err = a.Stop(app.Context{SessionID: "b"})
	if err != nil || res.Pending != nil || !res.Err || !strings.Contains(res.Toast, "só sessões em background") {
		t.Fatalf("interactive: %+v %v", res, err)
	}
	res, _ = a.Stop(app.Context{SessionID: "zz"})
	if res.Pending != nil || !res.Err {
		t.Fatalf("unknown: %+v", res)
	}
}

func TestRemoveAsks(t *testing.T) {
	a, f := sessApp(t, sessFixture()...)
	res, err := a.Remove(app.Context{SessionID: "a"})
	if err != nil || res.Pending == nil || res.Pending.Question != `remover "alpha" (e o worktree, se seguro)?` {
		t.Fatalf("asks: %+v %v", res, err)
	}
	if got := sessEffect(t, res).ClosePanes; !reflect.DeepEqual(got, []string{"aaaa1111"}) {
		t.Fatalf("pane to close: %v", got)
	}
	res, _ = a.Confirm(res.Pending.Token)
	if res.Toast != "" || !sessEffect(t, res).Refresh {
		t.Fatalf("yes: %+v", res)
	}
	if got := f.Claude.Last(); got.Method != "Remove" || got.Args[0] != "aaaa1111" {
		t.Fatalf("claude rm: %+v", got)
	}

	f.Claude.Errs = map[string]error{"Remove": errors.New("claude rm aaaa1111: nope")}
	res, _ = a.Remove(app.Context{SessionID: "a"})
	res, _ = a.Confirm(res.Pending.Token)
	if !res.Err || res.Toast != "claude rm aaaa1111: nope" {
		t.Fatalf("failure: %+v", res)
	}

	res, _ = a.Remove(app.Context{SessionID: "b"})
	if res.Pending != nil || !res.Err || res.Toast != "só sessões em background podem ser removidas" {
		t.Fatalf("interactive: %+v", res)
	}
}

func TestRenameMoveRenameContext(t *testing.T) {
	a, f := sessApp(t, sessFixture()...)
	events := sessWatch(t, a)

	res, err := a.Rename("a", "investigação")
	if err != nil || res.Toast != "renomeada" || !sessEffect(t, res).Refresh {
		t.Fatalf("rename: %+v %v", res, err)
	}
	calls := f.Sessions.Calls()
	if len(calls) != 2 || calls[0].Method != "Rename" || calls[0].Args[0] != "a" || calls[0].Args[1] != "investigação" ||
		calls[1].Method != "Reapply" || len(calls[1].Args[0].([]store.Session)) != 6 {
		t.Fatalf("rename then reapply: %+v", calls)
	}
	if !sessHas(events(), app.EvSnapshotChanged) {
		t.Fatal("snapshot.changed")
	}

	f.Sessions.Reset()
	res, _ = a.Move("b", "visio")
	if res.Toast != "movida para visio" {
		t.Fatalf("move: %q", res.Toast)
	}
	if c := f.Sessions.Calls(); c[0].Method != "SetContext" || c[0].Args[1] != "visio" || c[1].Method != "Reapply" {
		t.Fatalf("move calls: %+v", c)
	}
	res, _ = a.Move("b", "")
	if res.Toast != "contexto volta à regra do diretório" {
		t.Fatalf("move back: %q", res.Toast)
	}
	if !sessHas(events(), app.EvSnapshotChanged) {
		t.Fatal("snapshot.changed")
	}

	f.Sessions.Reset()
	res, _ = a.RenameContext("c1", "pesquisa")
	if res.Toast != "seção renomeada" {
		t.Fatalf("section: %q", res.Toast)
	}
	if c := f.Sessions.Calls(); c[0].Method != "RenameContext" || c[0].Args[0] != "c1" || c[1].Method != "Reapply" {
		t.Fatalf("section calls: %+v", c)
	}
	f.Sessions.Reset()
	res, err = a.RenameContext("c1", "c1")
	if err != nil || res.Toast != "" || f.Sessions.Count("RenameContext") != 0 {
		t.Fatalf("same name does nothing: %+v %v", res, err)
	}

	for _, bad := range []func() (app.Result, error){
		func() (app.Result, error) { return a.Rename("", "x") },
		func() (app.Result, error) { return a.Move("", "x") },
	} {
		if _, err := bad(); !errors.Is(err, app.ErrNoSession) {
			t.Fatalf("no session: %v", err)
		}
	}
}

func TestSessionCommandsRegistered(t *testing.T) {
	a, f := sessApp(t, sessFixture()...)
	if err := a.RegisterSessionCommands(); err != nil {
		t.Fatal(err)
	}
	if err := a.RegisterSessionCommands(); !errors.Is(err, app.ErrDuplicateCommand) {
		t.Fatalf("twice: %v", err)
	}
	for _, n := range []string{"session.archive", "sessions.archive_stale", "sessions.remove_ended",
		"session.stop", "session.remove", "session.rename", "session.move"} {
		c, ok := a.Registry().Lookup(n)
		if !ok || c.Title == "" {
			t.Fatalf("%s not registered: %+v", n, c)
		}
	}
	if c, _ := a.Registry().Lookup("session.stop"); !c.Confirm || !reflect.DeepEqual(c.Keys, []string{"x"}) {
		t.Fatalf("stop: %+v", c)
	}
	ctx := app.Context{SessionID: "a"}
	run := func(name string, args app.Args) app.Result {
		t.Helper()
		res, err := a.Run(ctx, name, args)
		if err != nil {
			t.Fatalf("%s: %v", name, err)
		}
		return res
	}
	if res := run("session.archive", nil); !strings.Contains(res.Toast, "arquivada") {
		t.Fatalf("archive: %+v", res)
	}
	if res := run("session.archive", app.Args{"context": "c2"}); res.Pending == nil {
		t.Fatalf("archive context: %+v", res)
	}
	if res := run("sessions.archive_stale", nil); res.Pending == nil {
		t.Fatalf("archive_stale: %+v", res)
	}
	if res := run("sessions.remove_ended", nil); res.Toast == "" {
		t.Fatalf("remove_ended: %+v", res)
	}
	if res := run("session.stop", nil); res.Pending == nil {
		t.Fatalf("stop: %+v", res)
	}
	if res := run("session.remove", nil); res.Pending == nil {
		t.Fatalf("remove: %+v", res)
	}
	if res := run("session.rename", app.Args{"name": "n"}); res.Toast != "renomeada" {
		t.Fatalf("rename: %+v", res)
	}
	if res := run("session.move", app.Args{"context": "x"}); res.Toast != "movida para x" {
		t.Fatalf("move: %+v", res)
	}
	if f.Sessions.Count("Rename") != 1 || f.Sessions.Count("SetContext") != 1 {
		t.Fatalf("store: %+v", f.Sessions.Calls())
	}
	// session commands need a session
	if _, err := a.Run(app.Context{}, "session.stop", nil); !errors.Is(err, app.ErrUnavailable) {
		t.Fatalf("no selection: %v", err)
	}
	if _, err := a.Run(app.Context{}, "sessions.archive_stale", nil); err != nil {
		t.Fatalf("bulk needs no selection: %v", err)
	}
}
