package rpc

import (
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"strings"
	"testing"
	"time"

	"github.com/aqilputi/nekuroddo/internal/app"
	"github.com/aqilputi/nekuroddo/internal/claude"
	"github.com/aqilputi/nekuroddo/internal/rpc/proto"
	"github.com/aqilputi/nekuroddo/internal/store"
)

func someTail(n int) []claude.TailLine {
	out := make([]claude.TailLine, n)
	for i := range out {
		out[i] = claude.TailLine{At: time.Unix(int64(i), 0).UTC(), Kind: claude.TailTool, Text: fmt.Sprintf("● Bash(%d)", i)}
	}
	return out
}

func TestSessionTailEndpoint(t *testing.T) {
	e := newEnv(t)
	e.f.Sessions.SetTail(bgSession.SessionID, someTail(120))

	lines, err := e.c.SessionTail(ctxT(t), bgSession.SessionID, 3)
	if err != nil {
		t.Fatal(err)
	}
	if len(lines) != 3 || lines[2].Text != "● Bash(119)" || lines[2].Kind != "tool" || !lines[2].At.Equal(time.Unix(119, 0)) {
		t.Fatalf("lines = %+v", lines)
	}
	// no n: the default 80
	if lines, err := e.c.SessionTail(ctxT(t), bgSession.SessionID, 0); err != nil || len(lines) != proto.DefaultTail {
		t.Fatalf("default: %d lines, %v", len(lines), err)
	}
	// a known session with nothing: lines is [] on the wire
	resp, err := e.rawUnix().Get("http://nk/v1/sessions/" + idleSession.SessionID + "/tail")
	if err != nil {
		t.Fatal(err)
	}
	b, _ := io.ReadAll(resp.Body)
	resp.Body.Close()
	if resp.StatusCode != http.StatusOK || strings.TrimSpace(string(b)) != `{"lines":[]}` {
		t.Fatalf("empty tail: %d %s", resp.StatusCode, b)
	}
	// the shape of one line
	resp, err = e.rawUnix().Get("http://nk/v1/sessions/" + bgSession.SessionID + "/tail?n=1")
	if err != nil {
		t.Fatal(err)
	}
	b, _ = io.ReadAll(resp.Body)
	resp.Body.Close()
	if want := `{"lines":[{"at":"1970-01-01T00:01:59Z","kind":"tool","text":"● Bash(119)"}]}`; strings.TrimSpace(string(b)) != want {
		t.Fatalf("body = %s, want %s", b, want)
	}
}

func TestSessionTailErrors(t *testing.T) {
	e := newEnv(t)
	if _, err := e.c.SessionTail(ctxT(t), "nope", 5); !IsCode(err, proto.CodeNotFound) {
		t.Fatalf("unknown session: %v", err)
	}
	resp, err := e.rawUnix().Get("http://nk/v1/sessions/" + bgSession.SessionID + "/tail?n=muitas")
	if err != nil {
		t.Fatal(err)
	}
	resp.Body.Close()
	if resp.StatusCode != http.StatusBadRequest {
		t.Fatalf("bad n: %d", resp.StatusCode)
	}
	e.f.Sessions.TailErr = errors.New("boom")
	var api *APIError
	if _, err := e.c.SessionTail(ctxT(t), bgSession.SessionID, 5); !errors.As(err, &api) || api.Status != http.StatusUnprocessableEntity {
		t.Fatalf("port error: %v", err)
	}
	if st, code := appErr(app.ErrUnknownSession); st != http.StatusNotFound || code != proto.CodeNotFound {
		t.Fatalf("appErr = %d %s", st, code)
	}
}

// session.tail reaches SSE clients, so they refetch only the tails that grew.
func TestSessionTailEvent(t *testing.T) {
	e := newEnv(t)
	s, err := e.c.Events(ctxT(t), "")
	if err != nil {
		t.Fatal(err)
	}
	defer s.Close()
	grown := bgSession
	grown.TailSeq = 4
	e.f.Sessions.SetSnapshot(store.Snapshot{Sessions: []store.Session{grown, idleSession, histSession}})
	e.a.Refresh()
	ev := nextEvent(t, s, app.EvSessionTail)
	var d app.TailChanged
	if err := json.Unmarshal(ev.Data, &d); err != nil || d.ID != bgSession.SessionID || d.NNew != 4 {
		t.Fatalf("event = %s (%v)", ev.Data, err)
	}
	if string(ev.Data) != `{"id":"11111111-aaaa","n_new":4}` {
		t.Fatalf("data = %s", ev.Data)
	}
}
