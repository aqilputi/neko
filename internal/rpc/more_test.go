package rpc

import (
	"context"
	"errors"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"sync"
	"testing"
	"time"

	"github.com/aqilputi/nekuroddo/internal/app"
	"github.com/aqilputi/nekuroddo/internal/core"
	"github.com/aqilputi/nekuroddo/internal/rpc/proto"
)

func TestHandlerInProcess(t *testing.T) {
	e := newEnv(t)
	ts := httptest.NewServer(e.srv.Handler())
	defer ts.Close()
	c, err := Dial(ts.URL, "")
	if err != nil {
		t.Fatal(err)
	}
	defer c.Close()
	if _, err := c.Hello(ctxT(t)); err != nil {
		t.Fatal(err)
	}
}

func TestClientErrorsWithoutJSONBody(t *testing.T) {
	e := newEnv(t)
	err := e.c.do(ctxT(t), http.MethodGet, "/v2/nada", nil, nil)
	var api *APIError
	if !errors.As(err, &api) || api.Status != http.StatusNotFound || api.Code != "Not Found" {
		t.Fatalf("err = %v", err)
	}
	if err := e.c.do(ctxT(t), http.MethodGet, "/v1/hello", func() {}, nil); err == nil {
		t.Fatal("marshalled a func")
	}
	if err := e.c.do(ctxT(t), "BAD METHOD", "/v1/hello", nil, nil); err == nil {
		t.Fatal("bad method worked")
	}
	if _, err := e.c.Events(ctxT(t), "x"); !IsCode(err, proto.CodeBadRequest) {
		t.Fatalf("events bad id err = %v", err)
	}
	dead, _ := Dial("unix:///nonexistent/nk.sock", "")
	if _, err := dead.Events(ctxT(t), ""); err == nil {
		t.Fatal("events on a dead socket worked")
	}
}

func TestCommandsBadQuery(t *testing.T) {
	e := newEnv(t)
	resp, err := e.rawUnix().Get("http://nk/v1/commands?todo_id=x")
	if err != nil {
		t.Fatal(err)
	}
	resp.Body.Close()
	if resp.StatusCode != http.StatusBadRequest {
		t.Fatalf("status %d", resp.StatusCode)
	}
	// picked may also come comma-separated and repeated
	resp, err = e.rawUnix().Get("http://nk/v1/commands?picked=a,,b&picked=c")
	if err != nil {
		t.Fatal(err)
	}
	resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		t.Fatalf("status %d", resp.StatusCode)
	}
	pc, _ := contextFromQuery(map[string][]string{"picked": {"a,,b", "c"}})
	if len(pc.Picked) != 3 {
		t.Fatalf("picked = %v", pc.Picked)
	}
}

func TestEffectsFromPointerAndUnmarshalable(t *testing.T) {
	e := newEnv(t)
	id, fp := openBg(t, e)
	reg := e.a.Registry()
	_ = reg.Register(core.Command{Name: "x.close"}, func(app.Context, app.Args) (app.Result, error) {
		return app.Result{Data: &app.SessionEffect{ClosePanes: []string{id}}}, nil
	})
	_ = reg.Register(core.Command{Name: "x.nil"}, func(app.Context, app.Args) (app.Result, error) {
		var eff *app.SessionEffect
		return app.Result{Data: eff}, nil
	})
	_ = reg.Register(core.Command{Name: "x.bad"}, func(app.Context, app.Args) (app.Result, error) {
		return app.Result{Data: func() {}}, nil
	})
	if _, err := e.c.Run(ctxT(t), "x.close", nil, proto.Context{}); err != nil {
		t.Fatal(err)
	}
	if !fp.Closed() {
		t.Fatal("pointer effect ignored")
	}
	if _, err := e.c.Run(ctxT(t), "x.nil", nil, proto.Context{}); err != nil {
		t.Fatal(err)
	}
	if _, err := e.c.Run(ctxT(t), "x.bad", nil, proto.Context{}); !IsCode(err, proto.CodeFailed) {
		t.Fatalf("unmarshalable data err = %v", err)
	}
}

func TestPaneDeletedWhileWatched(t *testing.T) {
	e := newEnv(t)
	id, _ := openBg(t, e)
	pc, err := e.c.AttachPane(ctxT(t), id)
	if err != nil {
		t.Fatal(err)
	}
	defer pc.Close()
	if err := e.c.ClosePane(ctxT(t), id); err != nil {
		t.Fatal(err)
	}
	if c := readControl(t, pc, proto.CtlExit); c.Err != "" {
		t.Fatalf("exit = %+v", c)
	}
}

func TestPaneStreamEndsBeforeExit(t *testing.T) {
	e := newEnv(t)
	fp := newCutPane("cut1")
	e.srv.AttachPane(fp, "", 80, 24)
	pc, err := e.c.AttachPane(ctxT(t), "cut1")
	if err != nil {
		t.Fatal(err)
	}
	defer pc.Close()
	eventually(t, "viewer subscribed", func() bool { return fp.subscribed() })
	// the byte stream ends, then the program exits: the viewer still gets
	// the exit status
	fp.cut()
	time.Sleep(10 * time.Millisecond)
	fp.Exit(errors.New("boom"))
	if c := readControl(t, pc, proto.CtlExit); c.Err != "boom" {
		t.Fatalf("exit = %+v", c)
	}
}

func TestPaneLaggingViewerIsAskedToRetry(t *testing.T) {
	e := newEnv(t, func(o *Options, _ *fakesT) { o.exitWait = 20 * time.Millisecond })
	fp := newCutPane("cut2")
	e.srv.AttachPane(fp, "", 80, 24)
	pc, err := e.c.AttachPane(ctxT(t), "cut2")
	if err != nil {
		t.Fatal(err)
	}
	defer pc.Close()
	eventually(t, "viewer subscribed", func() bool { return fp.subscribed() })
	fp.cut() // stream ends, program keeps running
	ctx, cancel := context.WithTimeout(t.Context(), 5*time.Second)
	defer cancel()
	if _, err := pc.Read(ctx); err == nil {
		t.Fatal("lagging viewer kept reading")
	}
	if fp.Done() {
		t.Fatal("pane ended")
	}
}

func TestWriteTokenFileErrors(t *testing.T) {
	dir := shortDir(t)
	ro := filepath.Join(dir, "ro")
	if err := os.Mkdir(ro, 0o500); err != nil {
		t.Fatal(err)
	}
	if os.Getuid() == 0 {
		t.Skip("root writes anywhere")
	}
	if err := WriteTokenFile(filepath.Join(ro, "tok"), "x"); err == nil {
		t.Fatal("wrote into a read-only dir")
	}
	f := filepath.Join(dir, "file")
	_ = os.WriteFile(f, nil, 0o600)
	if err := WriteTokenFile(filepath.Join(f, "tok"), "x"); err == nil {
		t.Fatal("wrote under a file")
	}
	if _, err := ListenUnix(filepath.Join(f, "s")); err == nil {
		t.Fatal("listened under a file")
	}
}

func TestEventStreamErr(t *testing.T) {
	e := newEnv(t)
	s, err := e.c.Events(ctxT(t), "")
	if err != nil {
		t.Fatal(err)
	}
	if s.Err() != nil {
		t.Fatal("error while running")
	}
	_ = e.srv.Close()
	for range s.C {
	}
	if s.Err() == nil {
		t.Fatal("no error after the server went away")
	}
	s.Close()
}

// cutPane is a pane whose byte stream can end while the program runs (what
// term.Process does to a subscriber that lags too far behind).
type cutPane struct {
	id     string
	mu     sync.Mutex
	ch     chan []byte
	cutOne sync.Once
	exited chan error
	done   bool
}

func newCutPane(id string) *cutPane { return &cutPane{id: id, exited: make(chan error, 1)} }

func (p *cutPane) ID() string                  { return p.id }
func (p *cutPane) Write(b []byte) (int, error) { return len(b), nil }
func (p *cutPane) Resize(int, int)             {}
func (p *cutPane) Output() <-chan struct{}     { return nil }
func (p *cutPane) Exited() <-chan error        { return p.exited }
func (p *cutPane) Title() string               { return "" }
func (p *cutPane) Close()                      {}

func (p *cutPane) Subscribe() (<-chan []byte, func()) {
	p.mu.Lock()
	defer p.mu.Unlock()
	p.ch = make(chan []byte, 1)
	return p.ch, p.cut
}

func (p *cutPane) subscribed() bool {
	p.mu.Lock()
	defer p.mu.Unlock()
	return p.ch != nil
}

func (p *cutPane) cut() {
	p.cutOne.Do(func() {
		p.mu.Lock()
		close(p.ch)
		p.mu.Unlock()
	})
}

func (p *cutPane) Done() bool {
	p.mu.Lock()
	defer p.mu.Unlock()
	return p.done
}

func (p *cutPane) Exit(err error) {
	p.mu.Lock()
	p.done = true
	p.mu.Unlock()
	p.exited <- err
	close(p.exited)
}
