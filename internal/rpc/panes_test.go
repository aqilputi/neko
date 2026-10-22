package rpc

import (
	"bytes"
	"context"
	"errors"
	"net/http"
	"os"
	"slices"
	"strings"
	"testing"
	"time"

	"github.com/aqilputi/nekuroddo/internal/app"
	"github.com/aqilputi/nekuroddo/internal/app/apptest"
	"github.com/aqilputi/nekuroddo/internal/claude"
	"github.com/aqilputi/nekuroddo/internal/rpc/proto"
	"github.com/aqilputi/nekuroddo/internal/store"
	"github.com/aqilputi/nekuroddo/internal/term"
)

// readBytes reads frames until want shows up in the binary output.
func readBytes(t *testing.T, pc *PaneConn, want string) {
	t.Helper()
	var got bytes.Buffer
	ctx, cancel := context.WithTimeout(t.Context(), 5*time.Second)
	defer cancel()
	for !strings.Contains(got.String(), want) {
		f, err := pc.Read(ctx)
		if err != nil {
			t.Fatalf("reading %q (got %q): %v", want, got.String(), err)
		}
		got.Write(f.Data)
	}
}

func readControl(t *testing.T, pc *PaneConn, typ string) proto.Control {
	t.Helper()
	ctx, cancel := context.WithTimeout(t.Context(), 5*time.Second)
	defer cancel()
	for {
		f, err := pc.Read(ctx)
		if err != nil {
			t.Fatalf("waiting for %s: %v", typ, err)
		}
		if f.Control != nil && f.Control.Type == typ {
			return *f.Control
		}
	}
}

func openBg(t *testing.T, e *env) (string, *apptest.FakePane) {
	t.Helper()
	op, err := e.c.OpenPane(ctxT(t), proto.PaneOpenRequest{SessionID: bgSession.SessionID, Cols: 100, Rows: 30})
	if err != nil {
		t.Fatal(err)
	}
	return op.PaneID, e.f.Panes.Pane(op.PaneID)
}

func TestOpenPane(t *testing.T) {
	e := newEnv(t)
	ev := e.a.Subscribe(t.Context())
	id, fp := openBg(t, e)
	if id != "bg01" || fp == nil {
		t.Fatalf("pane %q %v", id, fp)
	}
	call := e.f.Panes.Last()
	if call.Args[2] != "/r/a" || call.Args[3] != 100 || call.Args[4] != 30 ||
		!slices.Equal(call.Args[5].([]string), []string{"claude", "attach", "bg01"}) {
		t.Fatalf("Start = %v", call.Args)
	}
	if e.a.Panes().PaneOf(bgSession.SessionID) != "bg01" || !e.a.Panes().IsOpen("bg01") {
		t.Fatal("app does not know the pane")
	}
	for got := range ev {
		if got.Name == app.EvPaneOpened {
			break
		}
	}
	ps, err := e.c.Panes(ctxT(t))
	if err != nil {
		t.Fatal(err)
	}
	if len(ps) != 1 || ps[0].ID != "bg01" || ps[0].SessionID != bgSession.SessionID || ps[0].Cols != 100 || ps[0].Rows != 30 {
		t.Fatalf("panes = %+v", ps)
	}
	// opening again reuses it
	op, err := e.c.OpenPane(ctxT(t), proto.PaneOpenRequest{SessionID: bgSession.SessionID})
	if err != nil || !op.Reused || op.PaneID != "bg01" || e.f.Panes.Count("Start") != 1 {
		t.Fatalf("reopen = %+v %v (starts %d)", op, err, e.f.Panes.Count("Start"))
	}
}

func TestOpenPaneDefaultsAndErrors(t *testing.T) {
	e := newEnv(t)
	if _, err := e.c.OpenPane(ctxT(t), proto.PaneOpenRequest{SessionID: "nope"}); !IsCode(err, proto.CodeNotFound) {
		t.Fatalf("unknown session err = %v", err)
	}
	if _, err := e.c.OpenPane(ctxT(t), proto.PaneOpenRequest{}); !IsCode(err, proto.CodeBadRequest) {
		t.Fatalf("no session err = %v", err)
	}
	op, err := e.c.OpenPane(ctxT(t), proto.PaneOpenRequest{SessionID: idleSession.SessionID})
	if err != nil {
		t.Fatal(err)
	}
	if c, r := e.f.Panes.Pane(op.PaneID).Size(); c != 80 || r != 24 {
		t.Fatalf("default size %dx%d", c, r)
	}
	e.f.Panes.Err = errors.New("pty: no")
	if _, err := e.c.OpenPane(ctxT(t), proto.PaneOpenRequest{SessionID: bgSession.SessionID}); !IsCode(err, proto.CodeFailed) {
		t.Fatalf("spawn failure err = %v", err)
	}
}

func TestOpenPaneInteractiveFails(t *testing.T) {
	e := newEnv(t)
	e.f.Sessions.SetSnapshot(withSession(e, sessionOf("44444444-dddd", "interactive")))
	e.a.Refresh()
	_, err := e.c.OpenPane(ctxT(t), proto.PaneOpenRequest{SessionID: "44444444-dddd"})
	if !IsCode(err, proto.CodeFailed) {
		t.Fatalf("interactive err = %v", err)
	}
}

func TestOpenPaneResumesHistory(t *testing.T) {
	e := newEnv(t)
	e.f.Claude.NextID = "new1"
	op, err := e.c.OpenPane(ctxT(t), proto.PaneOpenRequest{SessionID: histSession.SessionID})
	if err != nil {
		t.Fatal(err)
	}
	if op.PaneID != "new1" || e.f.Claude.Count("ResumeInBackground") != 1 {
		t.Fatalf("resume = %+v", op)
	}
	e.f.Claude.Errs = map[string]error{"ResumeInBackground": errors.New("busy")}
	e.f.Sessions.SetSnapshot(withSession(e, sessionOf("55555555-eeee", "history")))
	e.a.Refresh()
	if _, err := e.c.OpenPane(ctxT(t), proto.PaneOpenRequest{SessionID: "55555555-eeee"}); !IsCode(err, proto.CodeFailed) {
		t.Fatalf("resume failure err = %v", err)
	}
}

func TestOpenPaneTypesReply(t *testing.T) {
	e := newEnv(t, func(o *Options, _ *fakesT) {
		o.replyDelay = func(time.Duration) time.Duration { return time.Millisecond }
	})
	op, err := e.c.OpenPane(ctxT(t), proto.PaneOpenRequest{SessionID: bgSession.SessionID, Reply: "sim"})
	if err != nil {
		t.Fatal(err)
	}
	fp := e.f.Panes.Pane(op.PaneID)
	eventually(t, "reply typed", func() bool { return fp.Written() == "\x1b[200~sim\x1b[201~\r" })
	// on a reused pane too
	if _, err := e.c.OpenPane(ctxT(t), proto.PaneOpenRequest{SessionID: bgSession.SessionID, Reply: "não"}); err != nil {
		t.Fatal(err)
	}
	eventually(t, "second reply typed", func() bool { return strings.HasSuffix(fp.Written(), "não\x1b[201~\r") })
}

// The reply goes through core.ResolveOption, like session.reply: a number
// alone picks that option of the session's questions; anything else (text,
// out of range) is typed as given.
func TestOpenPaneResolvesReplyOption(t *testing.T) {
	asking := bgSession
	asking.Questions = []claude.Question{{Question: "qual?", Options: []claude.Option{{Label: "alpha"}, {Label: "bravo"}}}}
	e := newEnv(t, func(o *Options, f *fakesT) {
		o.replyDelay = func(time.Duration) time.Duration { return time.Millisecond }
		f.Sessions.SetSnapshot(store.Snapshot{At: apptest.Epoch, Sessions: []store.Session{asking, idleSession}})
	})
	op, err := e.c.OpenPane(ctxT(t), proto.PaneOpenRequest{SessionID: asking.SessionID, Reply: "2"})
	if err != nil {
		t.Fatal(err)
	}
	fp := e.f.Panes.Pane(op.PaneID)
	eventually(t, "option typed", func() bool { return fp.Written() == "\x1b[200~bravo\x1b[201~\r" })
	for _, reply := range []string{"3", "talvez", "02"} {
		before := fp.Written()
		if _, err := e.c.OpenPane(ctxT(t), proto.PaneOpenRequest{SessionID: asking.SessionID, Reply: reply}); err != nil {
			t.Fatal(err)
		}
		want := before + "\x1b[200~" + reply + "\x1b[201~\r"
		eventually(t, "reply "+reply+" as given", func() bool { return fp.Written() == want })
	}
}

func TestPaneWebSocketEcho(t *testing.T) {
	e := newEnv(t)
	id, fp := openBg(t, e)
	fp.SetTitle("✳ fila")
	pc, err := e.c.AttachPane(ctxT(t), id)
	if err != nil {
		t.Fatal(err)
	}
	defer pc.Close()
	// the current title comes first
	if c := readControl(t, pc, proto.CtlTitle); c.Text != "✳ fila" {
		t.Fatalf("title = %+v", c)
	}
	// client → PTY: binary frames are written as is
	if err := pc.Write(ctxT(t), []byte("ls\r")); err != nil {
		t.Fatal(err)
	}
	eventually(t, "bytes reach the pane", func() bool { return fp.Written() == "ls\r" })
	// PTY → client: raw bytes (the title frame came after the subscription)
	fp.Emit([]byte("\x1b[1mhello\x1b[0m"))
	readBytes(t, pc, "\x1b[1mhello\x1b[0m")
	// a new title follows the output that set it
	fp.SetTitle("✳ outra")
	fp.Emit([]byte("x"))
	if c := readControl(t, pc, proto.CtlTitle); c.Text != "✳ outra" {
		t.Fatalf("title = %+v", c)
	}
	// resize control frame
	ev := e.a.Subscribe(t.Context())
	if err := pc.Resize(ctxT(t), 120, 40); err != nil {
		t.Fatal(err)
	}
	eventually(t, "resized", func() bool { c, r := fp.Size(); return c == 120 && r == 40 })
	for got := range ev {
		if got.Name == proto.EvPaneResized {
			if r := got.Data.(proto.PaneResized); r.ID != id || r.Cols != 120 || r.Rows != 40 {
				t.Fatalf("pane.resized = %+v", r)
			}
			break
		}
	}
	// junk control frames are ignored
	if err := pc.sendControl(ctxT(t), proto.Control{Type: "dance"}); err != nil {
		t.Fatal(err)
	}
	if err := pc.writeText(ctxT(t), "{nope"); err != nil {
		t.Fatal(err)
	}
	// exit: a final control frame, then the socket closes
	fp.Exit(errors.New("exit status 1"))
	if c := readControl(t, pc, proto.CtlExit); c.Err != "exit status 1" {
		t.Fatalf("exit = %+v", c)
	}
	if _, err := pc.Read(ctxT(t)); err == nil {
		t.Fatal("read after exit worked")
	}
	eventually(t, "pane forgotten", func() bool { ps, _ := e.c.Panes(ctxT(t)); return len(ps) == 0 })
	if e.a.Panes().IsOpen(id) {
		t.Fatal("app still has the pane")
	}
}

func sessionOf(id, kind string) store.Session {
	return store.Session{SessionID: id, Name: kind, Cwd: "/r/x", Kind: kind, State: "idle", PID: 42}
}

// withSession is the env's snapshot plus s.
func withSession(e *env, s store.Session) store.Snapshot {
	snap := e.a.Snapshot()
	snap.Sessions = append(append([]store.Session{}, snap.Sessions...), s)
	return snap
}

func TestPaneWebSocketUnknownPane(t *testing.T) {
	e := newEnv(t)
	_, err := e.c.AttachPane(ctxT(t), "nope")
	if err == nil {
		t.Fatal("attached to an unknown pane")
	}
}

func TestPaneResizeAndDelete(t *testing.T) {
	e := newEnv(t)
	id, fp := openBg(t, e)
	if err := e.c.ResizePane(ctxT(t), id, 90, 20); err != nil {
		t.Fatal(err)
	}
	if c, r := fp.Size(); c != 90 || r != 20 {
		t.Fatalf("size %dx%d", c, r)
	}
	if err := e.c.ResizePane(ctxT(t), "nope", 90, 20); !IsCode(err, proto.CodeNotFound) {
		t.Fatalf("resize unknown err = %v", err)
	}
	if err := e.c.ResizePane(ctxT(t), id, 0, 20); !IsCode(err, proto.CodeBadRequest) {
		t.Fatalf("resize zero err = %v", err)
	}
	ev := e.a.Subscribe(t.Context())
	if err := e.c.ClosePane(ctxT(t), id); err != nil {
		t.Fatal(err)
	}
	if !fp.Closed() || e.a.Panes().IsOpen(id) {
		t.Fatal("pane not closed")
	}
	for got := range ev {
		if got.Name == app.EvPaneClosed {
			break
		}
		if got.Name == app.EvPaneExited {
			t.Fatal("closing published pane.exited")
		}
	}
	if err := e.c.ClosePane(ctxT(t), id); !IsCode(err, proto.CodeNotFound) {
		t.Fatalf("close twice err = %v", err)
	}
}

func TestPaneWebSocketClientGoesAway(t *testing.T) {
	e := newEnv(t)
	id, fp := openBg(t, e)
	pc, err := e.c.AttachPane(ctxT(t), id)
	if err != nil {
		t.Fatal(err)
	}
	pc.Close()
	// the pane keeps running for other clients
	time.Sleep(20 * time.Millisecond)
	fp.Emit([]byte("still here"))
	if fp.Done() || fp.Closed() {
		t.Fatal("pane ended with its client")
	}
	pc2, err := e.c.AttachPane(ctxT(t), id)
	if err != nil {
		t.Fatal(err)
	}
	defer pc2.Close()
}

func TestServerCloseClosesPanes(t *testing.T) {
	e := newEnv(t)
	_, fp := openBg(t, e)
	_ = e.srv.Close()
	if !fp.Closed() {
		t.Fatal("pane left running after Close")
	}
}

func TestAttachPaneFromHost(t *testing.T) {
	e := newEnv(t)
	fp := apptest.NewFakePane("host1", "h")
	e.srv.AttachPane(fp, "sid-host", 70, 20)
	ps, _ := e.c.Panes(ctxT(t))
	if len(ps) != 1 || ps[0].ID != "host1" || ps[0].SessionID != "sid-host" || ps[0].Title != "h" {
		t.Fatalf("panes = %+v", ps)
	}
}

// shSpawner runs /bin/sh in a real PTY whatever the plan's argv says.
type shSpawner struct{}

func (shSpawner) Start(id, _, _ string, cols, rows int, _ ...string) (app.Pane, error) {
	return term.StartProcess(id, os.TempDir(), cols, rows, "/bin/sh") // the fake cwd does not exist
}

func TestPaneWebSocketRealShell(t *testing.T) {
	e := newEnv(t)
	e.srv.spawner = shSpawner{} // a real PTY instead of the app's fake port
	op, err := e.c.OpenPane(ctxT(t), proto.PaneOpenRequest{SessionID: idleSession.SessionID, Cols: 80, Rows: 24})
	if err != nil {
		t.Fatal(err)
	}
	pc, err := e.c.AttachPane(ctxT(t), op.PaneID)
	if err != nil {
		t.Fatal(err)
	}
	defer pc.Close()
	if err := pc.Write(ctxT(t), []byte("echo nk-$((40+2))\r")); err != nil {
		t.Fatal(err)
	}
	readBytes(t, pc, "nk-42")
	if err := pc.Resize(ctxT(t), 100, 30); err != nil {
		t.Fatal(err)
	}
	if err := pc.Write(ctxT(t), []byte("stty size; exit 3\r")); err != nil {
		t.Fatal(err)
	}
	c := readControl(t, pc, proto.CtlExit)
	if !strings.Contains(c.Err, "3") {
		t.Fatalf("exit = %+v", c)
	}
}

func TestPaneTCPWebSocketNeedsToken(t *testing.T) {
	e := newEnv(t, func(o *Options, _ *fakesT) { o.Token = "tok" })
	id, _ := openBg(t, e)
	base := e.tcp()
	resp, err := http.Get(base + "/v1/panes/" + id + "/ws")
	if err != nil {
		t.Fatal(err)
	}
	resp.Body.Close()
	if resp.StatusCode != http.StatusUnauthorized {
		t.Fatalf("ws without token = %d", resp.StatusCode)
	}
	c, err := Dial(base, "tok")
	if err != nil {
		t.Fatal(err)
	}
	defer c.Close()
	pc, err := c.AttachPane(ctxT(t), id)
	if err != nil {
		t.Fatal(err)
	}
	pc.Close()
}
