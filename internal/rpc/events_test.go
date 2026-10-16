package rpc

import (
	"bufio"
	"encoding/json"
	"net/http"
	"strconv"
	"strings"
	"testing"
	"time"

	"github.com/aqilputi/nekuroddo/internal/app"
	"github.com/aqilputi/nekuroddo/internal/core"
	"github.com/aqilputi/nekuroddo/internal/rpc/proto"
)

// nextEvent reads the stream until an event named name (or fails).
func nextEvent(t *testing.T, s *EventStream, name string) proto.Event {
	t.Helper()
	timeout := time.After(5 * time.Second)
	for {
		select {
		case ev, ok := <-s.C:
			if !ok {
				t.Fatalf("stream closed waiting for %s: %v", name, s.Err())
			}
			if ev.Name == name {
				return ev
			}
		case <-timeout:
			t.Fatalf("timed out waiting for %s", name)
		}
	}
}

func toastText(t *testing.T, ev proto.Event) string {
	t.Helper()
	var d app.ToastData
	if err := json.Unmarshal(ev.Data, &d); err != nil {
		t.Fatal(err)
	}
	return d.Text
}

// waitSeq waits until the server's backlog has seen seq.
func waitSeq(t *testing.T, e *env, seq uint64) {
	t.Helper()
	eventually(t, "backlog seq "+strconv.FormatUint(seq, 10), func() bool { return e.srv.events.last() >= seq })
}

func TestEventsLive(t *testing.T) {
	e := newEnv(t)
	s, err := e.c.Events(ctxT(t), "")
	if err != nil {
		t.Fatal(err)
	}
	defer s.Close()
	e.a.Toast("oi", false)
	ev := nextEvent(t, s, app.EvToast)
	if toastText(t, ev) != "oi" || ev.Seq == 0 || ev.At.IsZero() {
		t.Fatalf("event = %+v", ev)
	}
	if s.LastID() != strconv.FormatUint(ev.Seq, 10) {
		t.Fatalf("LastID = %s, seq %d", s.LastID(), ev.Seq)
	}
}

func TestEventsResumeWithLastEventID(t *testing.T) {
	e := newEnv(t)
	s, err := e.c.Events(ctxT(t), "")
	if err != nil {
		t.Fatal(err)
	}
	e.a.Toast("um", false)
	e.a.Toast("dois", false)
	nextEvent(t, s, app.EvToast)
	second := nextEvent(t, s, app.EvToast)
	last := s.LastID()
	s.Close()

	e.a.Toast("três", false)
	e.a.Toast("quatro", false)
	waitSeq(t, e, second.Seq+2)

	s2, err := e.c.Events(ctxT(t), last)
	if err != nil {
		t.Fatal(err)
	}
	defer s2.Close()
	if got := toastText(t, nextEvent(t, s2, app.EvToast)); got != "três" {
		t.Fatalf("resumed at %q", got)
	}
	if got := toastText(t, nextEvent(t, s2, app.EvToast)); got != "quatro" {
		t.Fatalf("then %q", got)
	}
	// and then live
	e.a.Toast("cinco", false)
	if got := toastText(t, nextEvent(t, s2, app.EvToast)); got != "cinco" {
		t.Fatalf("live %q", got)
	}
}

func TestEventsSinceQuery(t *testing.T) {
	e := newEnv(t)
	e.a.Toast("antes", false)
	eventually(t, "a backlog", func() bool { return e.srv.events.last() > 0 })
	// since=0 replays everything kept
	resp, err := e.rawUnix().Get("http://nk/v1/events?since=0")
	if err != nil {
		t.Fatal(err)
	}
	defer resp.Body.Close()
	if ct := resp.Header.Get("Content-Type"); !strings.HasPrefix(ct, "text/event-stream") {
		t.Fatalf("content type %q", ct)
	}
	r := bufio.NewReader(resp.Body)
	var lines []string
	for len(lines) < 3 || lines[len(lines)-1] != "" {
		l, err := r.ReadString('\n')
		if err != nil {
			t.Fatal(err)
		}
		l = strings.TrimRight(l, "\n")
		if strings.HasPrefix(l, ":") || strings.HasPrefix(l, "retry:") {
			continue
		}
		if l == "" && len(lines) == 0 {
			continue
		}
		lines = append(lines, l)
	}
	if !strings.HasPrefix(lines[0], "id: ") || !strings.HasPrefix(lines[1], "event: ") || !strings.HasPrefix(lines[2], "data: {") {
		t.Fatalf("frame = %q", lines)
	}
	resp2, err := e.rawUnix().Get("http://nk/v1/events?since=abc")
	if err != nil {
		t.Fatal(err)
	}
	resp2.Body.Close()
	if resp2.StatusCode != http.StatusBadRequest {
		t.Fatalf("bad since = %d", resp2.StatusCode)
	}
}

func TestEventsGapWhenBacklogOverflows(t *testing.T) {
	e := newEnv(t, func(o *Options, _ *fakesT) { o.Backlog = 8 })
	bus := e.a.Subscribe(t.Context())
	for i := range 20 {
		e.a.Toast(strconv.Itoa(i), false)
	}
	var lastSeq uint64
	for ev := range bus {
		if ev.Name == app.EvToast && ev.Data.(app.ToastData).Text == "19" {
			lastSeq = ev.Seq
			break
		}
	}
	waitSeq(t, e, lastSeq)
	s, err := e.c.Events(ctxT(t), "2")
	if err != nil {
		t.Fatal(err)
	}
	defer s.Close()
	gap := nextEvent(t, s, proto.EvGap)
	var g proto.Gap
	if err := json.Unmarshal(gap.Data, &g); err != nil || g.Since != 2 || g.Oldest != e.srv.events.last()-7 {
		t.Fatalf("gap = %+v (%v)", g, err)
	}
	first := nextEvent(t, s, app.EvToast)
	if first.Seq != g.Oldest {
		t.Fatalf("first after gap seq %d, want %d", first.Seq, g.Oldest)
	}
}

func TestEventsHeartbeat(t *testing.T) {
	e := newEnv(t, func(o *Options, _ *fakesT) { o.Heartbeat = 10 * time.Millisecond })
	resp, err := e.rawUnix().Get("http://nk/v1/events")
	if err != nil {
		t.Fatal(err)
	}
	defer resp.Body.Close()
	r := bufio.NewReader(resp.Body)
	for range 4 {
		l, err := r.ReadString('\n')
		if err != nil {
			t.Fatal(err)
		}
		if strings.HasPrefix(l, ": ping") {
			return
		}
	}
	t.Fatal("no heartbeat")
}

func TestEventsSlowClientIsCut(t *testing.T) {
	e := newEnv(t)
	sub := e.srv.events.subscribe(0, false)
	if sub == nil {
		t.Fatal("no subscription")
	}
	for i := range listenerBuffer + 10 {
		e.srv.events.publish(frame{seq: uint64(i + 1000), name: "x", data: []byte("{}")})
	}
	// its channel is closed once it falls behind
	eventually(t, "slow listener cut", func() bool {
		for {
			select {
			case _, ok := <-sub.ch:
				if !ok {
					return true
				}
			default:
				return false
			}
		}
	})
}

func TestEventsPumpCutSendsGap(t *testing.T) {
	e := newEnv(t)
	s, err := e.c.Events(ctxT(t), "")
	if err != nil {
		t.Fatal(err)
	}
	defer s.Close()
	// hold the hub so the pump stalls and the bus cuts it
	e.srv.events.mu.Lock()
	for i := range app.SubscriberBuffer + 50 {
		e.a.Toast(strconv.Itoa(i), false)
	}
	e.srv.events.mu.Unlock()
	nextEvent(t, s, proto.EvGap)
	// and it follows the bus again
	e.a.Toast("de volta", false)
	for toastText(t, nextEvent(t, s, app.EvToast)) != "de volta" {
	}
}

func TestEventsStreamEndsWhenAppCloses(t *testing.T) {
	e := newEnv(t)
	s, err := e.c.Events(ctxT(t), "")
	if err != nil {
		t.Fatal(err)
	}
	defer s.Close()
	e.a.Close()
	select {
	case _, ok := <-s.C:
		for ok {
			_, ok = <-s.C
		}
	case <-time.After(5 * time.Second):
		t.Fatal("stream still open after app closed")
	}
}

func TestBacklogUnit(t *testing.T) {
	b := newBacklog(3)
	if b.last() != 0 || b.oldest() != 0 {
		t.Fatal("empty backlog not zero")
	}
	for i := uint64(1); i <= 5; i++ {
		b.add(frameOf(core.Event{Seq: i, Name: "x", Data: func() {}}))
	}
	if b.last() != 5 || b.oldest() != 3 {
		t.Fatalf("last %d oldest %d", b.last(), b.oldest())
	}
	got := b.since(3)
	if len(got) != 2 || got[0].seq != 4 || got[1].seq != 5 {
		t.Fatalf("since(3) = %+v", got)
	}
	// an unmarshalable payload is sent as an error object
	if !strings.Contains(string(got[0].data), "error") {
		t.Fatalf("data = %s", got[0].data)
	}
}
