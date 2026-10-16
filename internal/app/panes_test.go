package app_test

import (
	"errors"
	"reflect"
	"strings"
	"testing"
	"time"

	"github.com/aqilputi/nekuroddo/internal/app"
	"github.com/aqilputi/nekuroddo/internal/app/apptest"
	"github.com/aqilputi/nekuroddo/internal/config"
	"github.com/aqilputi/nekuroddo/internal/core"
	"github.com/aqilputi/nekuroddo/internal/store"
)

func paneApp(t *testing.T, ss ...store.Session) (*app.App, *apptest.Fakes) {
	t.Helper()
	f := apptest.New()
	f.Sessions.SetSnapshot(store.Snapshot{Sessions: ss})
	a := app.New(config.Default(), f.Ports())
	t.Cleanup(a.Close)
	a.Refresh()
	return a, f
}

var (
	paneBg      = store.Session{SessionID: "a", ShortID: "aaaa1111", Name: "alpha", Kind: "background", Cwd: "/w"}
	paneHist    = store.Session{SessionID: "h", Name: "hist", Kind: "history", Cwd: "/old"}
	paneInter   = store.Session{SessionID: "i", Kind: "interactive", PID: 42, Cwd: "/x"}
	paneNothing = func(string) bool { return false }
)

func TestPaneOpenNewPlansAttach(t *testing.T) {
	a, f := paneApp(t)
	pm := a.Panes()
	plan, err := pm.Open(paneBg, true, "", paneNothing)
	if err != nil {
		t.Fatal(err)
	}
	want := app.AttachPlan{SessionID: "a", ID: "aaaa1111", Title: paneBg.Label(), Dir: "/w",
		Argv: []string{"claude", "attach", "aaaa1111"}, Focus: true}
	if !reflect.DeepEqual(plan, want) {
		t.Fatalf("plan = %+v", plan)
	}
	if pm.PaneOf("a") != "" || f.Sessions.Count("MarkSeen") != 0 {
		t.Fatal("nothing is bound before the client starts the pane")
	}
}

func TestPaneStartedBindsMarksSeenAndPublishes(t *testing.T) {
	a, f := paneApp(t)
	clock := f.Clock
	ev := a.Subscribe(t.Context())
	pm := a.Panes()
	plan, _ := pm.Open(paneBg, false, "oi", paneNothing)
	if plan.ReplyIn != 1500*time.Millisecond || plan.Reply != "oi" {
		t.Fatalf("a new pane types the reply after 1.5s: %+v", plan)
	}
	pm.Started(plan)
	if pm.PaneOf("a") != "aaaa1111" || !pm.IsOpen("aaaa1111") {
		t.Fatal("started binds the session to its pane")
	}
	if c := f.Sessions.Last(); c.Method != "MarkSeen" || c.Args[0] != "a" || c.Args[1] != clock.Now() {
		t.Fatalf("seen at the clock's time: %+v", c)
	}
	r, ok := pm.Reply("aaaa1111")
	if !ok || r.Text != "oi" || !r.Due.Equal(clock.Now().Add(1500*time.Millisecond)) {
		t.Fatalf("pending reply %+v %v", r, ok)
	}
	if got := pm.Replies(); !reflect.DeepEqual(got, map[string]string{"aaaa1111": "oi"}) {
		t.Fatalf("replies %v", got)
	}
	if got := pm.Bindings(); !reflect.DeepEqual(got, map[string]string{"a": "aaaa1111"}) {
		t.Fatalf("bindings %v", got)
	}
	if e := next(t, ev, app.EvPaneOpened); e.Data != (app.PaneEvent{ID: "aaaa1111", SessionID: "a"}) {
		t.Fatalf("opened = %+v", e.Data)
	}
}

func TestPaneOpenTwiceReuses(t *testing.T) {
	a, f := paneApp(t)
	pm := a.Panes()
	plan, _ := pm.Open(paneBg, false, "", paneNothing)
	pm.Started(plan)
	f.Clock.Advance(time.Minute)
	// the default liveness is "started and not closed"
	again, err := pm.Open(paneBg, true, "sim", nil)
	if err != nil {
		t.Fatal(err)
	}
	if !again.Reuse || again.ID != "aaaa1111" || again.Argv != nil || !again.Focus {
		t.Fatalf("reuse plan %+v", again)
	}
	if again.ReplyIn != 50*time.Millisecond {
		t.Fatalf("a reused pane types after 50ms: %v", again.ReplyIn)
	}
	if r, ok := pm.Reply("aaaa1111"); !ok || r.Text != "sim" || !r.Due.Equal(f.Clock.Now().Add(50*time.Millisecond)) {
		t.Fatalf("reuse leaves the reply pending: %+v", r)
	}
	// reused without a reply: nothing pending
	pm.TakeReply("aaaa1111")
	if again, _ := pm.Open(paneBg, false, "", nil); !again.Reuse || again.ReplyIn != 0 {
		t.Fatalf("plain reuse %+v", again)
	}
	if _, ok := pm.Reply("aaaa1111"); ok {
		t.Fatal("no reply, nothing pending")
	}
}

func TestPaneOpenReusesByClientLiveness(t *testing.T) {
	a, _ := paneApp(t)
	pm := a.Panes()
	// the client says the session's own short id is alive: reuse and bind
	plan, err := pm.Open(paneBg, false, "x", func(id string) bool { return id == "aaaa1111" })
	if err != nil || !plan.Reuse {
		t.Fatalf("%+v %v", plan, err)
	}
	if pm.PaneOf("a") != "aaaa1111" {
		t.Fatal("reusing binds the session")
	}
	// a dead bound pane is started again
	plan, _ = pm.Open(paneBg, false, "", paneNothing)
	if plan.Reuse || plan.Argv == nil {
		t.Fatalf("dead pane: new attach %+v", plan)
	}
}

func TestPaneOpenInteractiveRefuses(t *testing.T) {
	a, _ := paneApp(t)
	_, err := a.Panes().Open(paneInter, true, "", paneNothing)
	if err == nil || err.Error() != "sessão interativa aberta em outro terminal (pid 42) — use o terminal dela" {
		t.Fatalf("err = %v", err)
	}
}

func TestPaneOpenHistoryResumes(t *testing.T) {
	a, f := paneApp(t)
	pm := a.Panes()
	plan, err := pm.Open(paneHist, true, "continua", paneNothing)
	if err != nil || !plan.Resume || plan.ID != "" || plan.Argv != nil {
		t.Fatalf("history needs a resume first: %+v %v", plan, err)
	}
	if f.Claude.Count("ResumeInBackground") != 0 {
		t.Fatal("Open never runs claude itself (the client runs Resume off its UI thread)")
	}
	f.Claude.NextID = "nnnn2222"
	id, err := pm.Resume(plan.Dir, plan.SessionID)
	if err != nil || id != "nnnn2222" {
		t.Fatalf("resume %q %v", id, err)
	}
	if c := f.Claude.Last(); c.Method != "ResumeInBackground" || !reflect.DeepEqual(c.Args, []any{"/old", "h"}) {
		t.Fatalf("claude call %+v", c)
	}
	att := pm.Attach(paneHist, id, true, "continua")
	if att.ID != "nnnn2222" || att.SessionID != "h" || !reflect.DeepEqual(att.Argv, []string{"claude", "attach", "nnnn2222"}) || att.ReplyIn != app.ReplyDelayNew {
		t.Fatalf("attach %+v", att)
	}
	pm.Started(att)
	if pm.PaneOf("h") != "nnnn2222" {
		t.Fatal("the history session keeps its new pane")
	}
	// opened again: the new pane is reused, though the session has no short id
	if again, _ := pm.Open(paneHist, false, "", nil); !again.Reuse || again.ID != "nnnn2222" {
		t.Fatalf("reuse %+v", again)
	}
	f.Claude.Errs = map[string]error{"ResumeInBackground": errors.New("claude --bg --resume: boom")}
	if _, err := pm.Resume("/old", "h"); err == nil || !strings.Contains(err.Error(), "boom") {
		t.Fatalf("resume error %v", err)
	}
}

func TestPaneTakeReplyOnce(t *testing.T) {
	a, _ := paneApp(t)
	pm := a.Panes()
	plan, _ := pm.Open(paneBg, false, "talvez", paneNothing)
	pm.Started(plan)
	if text, ok := pm.TakeReply("aaaa1111"); !ok || text != "talvez" {
		t.Fatalf("take %q %v", text, ok)
	}
	if _, ok := pm.TakeReply("aaaa1111"); ok {
		t.Fatal("typed once")
	}
	if _, ok := pm.TakeReply("nope"); ok {
		t.Fatal("unknown pane")
	}
}

func TestPaneCloseCleans(t *testing.T) {
	a, _ := paneApp(t)
	ev := a.Subscribe(t.Context())
	pm := a.Panes()
	plan, _ := pm.Open(paneBg, false, "x", paneNothing)
	pm.Started(plan)
	pm.Close("aaaa1111")
	if pm.PaneOf("a") != "" || pm.IsOpen("aaaa1111") || len(pm.Bindings()) != 0 || len(pm.Replies()) != 0 {
		t.Fatal("close forgets the pane, its session and its reply")
	}
	if e := next(t, ev, app.EvPaneClosed); e.Data != (app.PaneEvent{ID: "aaaa1111", SessionID: "a"}) {
		t.Fatalf("closed = %+v", e.Data)
	}
	pm.Close("aaaa1111") // unknown: nothing to say
	pm.Close("other")
	a.Publish("marker", nil)
	if e := recv(t, ev); e.Name != "marker" {
		t.Fatalf("closing an unknown pane publishes nothing: %s", e.Name)
	}
}

func TestPaneExitedPublishesAndCleans(t *testing.T) {
	a, _ := paneApp(t)
	ev := a.Subscribe(t.Context())
	pm := a.Panes()
	plan, _ := pm.Open(paneBg, false, "", paneNothing)
	pm.Started(plan)
	pm.Exited("aaaa1111", errors.New("exit status 1"))
	if e := next(t, ev, app.EvPaneExited); e.Data != (app.PaneEvent{ID: "aaaa1111", SessionID: "a", Err: "exit status 1"}) {
		t.Fatalf("exited = %+v", e.Data)
	}
	if pm.PaneOf("a") != "" {
		t.Fatal("an exited pane is forgotten")
	}
	pm.Exited("zzz", nil)
	if e := next(t, ev, app.EvPaneExited); e.Data != (app.PaneEvent{ID: "zzz"}) {
		t.Fatalf("exited = %+v", e.Data)
	}
}

func TestPanesPerApp(t *testing.T) {
	a, _ := paneApp(t)
	b, _ := paneApp(t)
	first := a.Panes()
	if a.Panes() != first || first == b.Panes() {
		t.Fatal("one pane table per core")
	}
}

func TestFullscreen(t *testing.T) {
	a, f := paneApp(t)
	pm := a.Panes()
	plan, _ := pm.Open(paneBg, false, "", paneNothing)
	pm.Started(plan)
	f.Sessions.Reset()
	res, err := a.Fullscreen(paneBg)
	if err != nil || res.Exec == nil {
		t.Fatalf("%+v %v", res, err)
	}
	if !reflect.DeepEqual(*res.Exec, core.ExecRequest{Argv: []string{"claude", "attach", "aaaa1111"}, Dir: "/w"}) {
		t.Fatalf("exec %+v", res.Exec)
	}
	if pm.PaneOf("a") != "" {
		t.Fatal("the embedded pane is given up")
	}
	if c := f.Sessions.Last(); c.Method != "MarkSeen" || c.Args[0] != "a" {
		t.Fatalf("seen: %+v", c)
	}
	res, err = a.Fullscreen(paneHist)
	if err != nil || !reflect.DeepEqual(*res.Exec, core.ExecRequest{
		Argv: []string{"claude", "--allow-dangerously-skip-permissions", "--resume", "h"}, Dir: "/old"}) {
		t.Fatalf("history resumes fullscreen: %+v %v", res.Exec, err)
	}
	if _, err := a.Fullscreen(paneInter); err == nil || err.Error() != "sessão interativa aberta em outro terminal" {
		t.Fatalf("interactive: %v", err)
	}
}

func TestPaneCommands(t *testing.T) {
	a, f := paneApp(t, paneBg, paneHist, paneInter)
	if err := a.RegisterPanes(); err != nil {
		t.Fatal(err)
	}
	if err := a.RegisterPanes(); err == nil {
		t.Fatal("registering twice fails")
	}
	for name, scope := range map[string]string{
		"session.open": core.ScopeCore, "session.preview": core.ScopeCore, "session.fullscreen": core.ScopeCore,
		"pane.close": core.ScopeClient, "pane.zoom": core.ScopeClient,
	} {
		c, ok := a.Registry().Lookup(name)
		if !ok || c.Scope != scope || len(c.Keys) == 0 {
			t.Errorf("%s: %+v %v", name, c, ok)
		}
	}
	// session.open on a background session: a focused attach plan
	res, err := a.Run(app.Context{SessionID: "a"}, "session.open", nil)
	if err != nil {
		t.Fatal(err)
	}
	if p, ok := res.Data.(app.AttachPlan); !ok || p.ID != "aaaa1111" || !p.Focus || p.Argv == nil {
		t.Fatalf("open: %+v", res.Data)
	}
	// session.preview on a history session resumes it (the command runs claude)
	f.Claude.NextID = "rrrr3333"
	res, err = a.Run(app.Context{SessionID: "h"}, "session.preview", nil)
	if err != nil {
		t.Fatal(err)
	}
	if p, ok := res.Data.(app.AttachPlan); !ok || p.ID != "rrrr3333" || p.Focus || p.Resume {
		t.Fatalf("preview: %+v", res.Data)
	}
	f.Claude.Errs = map[string]error{"ResumeInBackground": errors.New("boom")}
	if _, err := a.Run(app.Context{SessionID: "h"}, "session.open", nil); err == nil {
		t.Fatal("resume errors come back")
	}
	if _, err := a.Run(app.Context{SessionID: "i"}, "session.open", nil); err == nil {
		t.Fatal("interactive refuses")
	}
	res, err = a.Run(app.Context{SessionID: "a"}, "session.fullscreen", nil)
	if err != nil || res.Exec == nil {
		t.Fatalf("fullscreen %+v %v", res, err)
	}
	// unknown or missing session: unavailable
	for _, name := range []string{"session.open", "session.preview", "session.fullscreen"} {
		if _, err := a.Run(app.Context{SessionID: "nope"}, name, nil); !errors.Is(err, app.ErrUnavailable) {
			t.Errorf("%s on a missing session: %v", name, err)
		}
	}
}
