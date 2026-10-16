package rpc

// GET /v1/events: the app's bus as Server-Sent Events. One pump follows the
// bus and keeps the last Options.Backlog events, already encoded, in a ring;
// each SSE client replays what it missed (Last-Event-ID or ?since=) and then
// gets the live events. A client too slow to keep up is cut and reconnects
// with Last-Event-ID, which the ring usually covers.

import (
	"encoding/json"
	"fmt"
	"net/http"
	"strconv"
	"sync"
	"time"

	"github.com/aqilputi/nekuroddo/internal/core"
	"github.com/aqilputi/nekuroddo/internal/rpc/proto"
)

// listenerBuffer is how many events an SSE client may lag behind: a whole
// burst the pump may deliver at once (the bus's SubscriberBuffer) and then
// some, so a burst alone never cuts a client.
const listenerBuffer = 1024

// frame is one encoded event.
type frame struct {
	seq  uint64
	name string
	data []byte // proto.Event as JSON
}

// frameOf encodes e; a payload that does not marshal is sent as
// {"error": "..."} rather than dropped.
func frameOf(e core.Event) frame {
	raw, err := json.Marshal(e.Data)
	if err != nil {
		raw, _ = json.Marshal(map[string]string{"error": err.Error()})
	}
	b, _ := json.Marshal(proto.Event{Seq: e.Seq, At: e.At, Name: e.Name, Data: raw})
	return frame{seq: e.Seq, name: e.Name, data: b}
}

// backlog is a ring of the last n frames.
type backlog struct {
	buf   []frame
	start int // index of the oldest
	n     int // frames held
}

func newBacklog(size int) *backlog { return &backlog{buf: make([]frame, size)} }

func (b *backlog) add(f frame) {
	if b.n < len(b.buf) {
		b.buf[(b.start+b.n)%len(b.buf)] = f
		b.n++
		return
	}
	b.buf[b.start] = f
	b.start = (b.start + 1) % len(b.buf)
}

func (b *backlog) at(i int) frame { return b.buf[(b.start+i)%len(b.buf)] }

// oldest and last are the sequence numbers held (0 when empty).
func (b *backlog) oldest() uint64 {
	if b.n == 0 {
		return 0
	}
	return b.at(0).seq
}

func (b *backlog) last() uint64 {
	if b.n == 0 {
		return 0
	}
	return b.at(b.n - 1).seq
}

// since returns the frames after seq, oldest first.
func (b *backlog) since(seq uint64) []frame {
	var out []frame
	for i := range b.n {
		if f := b.at(i); f.seq > seq {
			out = append(out, f)
		}
	}
	return out
}

// listener is one SSE client.
type listener struct {
	ch     chan frame
	replay []frame
	gap    *proto.Gap
}

// eventHub holds the backlog and the SSE clients.
type eventHub struct {
	mu     sync.Mutex
	b      *backlog
	subs   map[*listener]struct{}
	closed bool
}

func newEventHub(size int) *eventHub {
	return &eventHub{b: newBacklog(size), subs: map[*listener]struct{}{}}
}

// subscribe adds a client. With resume it replays what came after since,
// and reports a gap when events after since are no longer held (or since is
// from a previous server: ahead of the last one). nil once the hub closed.
func (h *eventHub) subscribe(since uint64, resume bool) *listener {
	h.mu.Lock()
	defer h.mu.Unlock()
	if h.closed {
		return nil
	}
	l := &listener{ch: make(chan frame, listenerBuffer)}
	if resume {
		// since 0 means "all you have": no gap for what came before the server
		if old, last := h.b.oldest(), h.b.last(); h.b.n > 0 && since > 0 && (since+1 < old || since > last) {
			l.gap = &proto.Gap{Since: since, Oldest: old}
		}
		l.replay = h.b.since(since)
	}
	h.subs[l] = struct{}{}
	return l
}

func (h *eventHub) unsubscribe(l *listener) {
	h.mu.Lock()
	defer h.mu.Unlock()
	if _, ok := h.subs[l]; ok {
		delete(h.subs, l)
		close(l.ch)
	}
}

// publish keeps f and hands it to every client; a full client is cut.
func (h *eventHub) publish(f frame) {
	h.mu.Lock()
	defer h.mu.Unlock()
	h.b.add(f)
	for l := range h.subs {
		select {
		case l.ch <- f:
		default:
			delete(h.subs, l)
			close(l.ch)
		}
	}
}

// lost tells every client that events after since were dropped (the pump
// lagged behind the bus): a gap event, as for a resume too old.
func (h *eventHub) lost(since uint64) {
	d, _ := json.Marshal(proto.Gap{Since: since})
	b, _ := json.Marshal(proto.Event{Name: proto.EvGap, Data: d})
	f := frame{name: proto.EvGap, data: b}
	h.mu.Lock()
	defer h.mu.Unlock()
	for l := range h.subs {
		select {
		case l.ch <- f:
		default:
			delete(h.subs, l)
			close(l.ch)
		}
	}
}

func (h *eventHub) close() {
	h.mu.Lock()
	defer h.mu.Unlock()
	h.closed = true
	for l := range h.subs {
		delete(h.subs, l)
		close(l.ch)
	}
}

// last is the newest sequence number held.
func (h *eventHub) last() uint64 {
	h.mu.Lock()
	defer h.mu.Unlock()
	return h.b.last()
}

// startPump subscribes to the bus now (so nothing published after New is
// missed) and copies it into the hub until Close or the App closes. If the
// bus cuts the pump for lagging, it subscribes again.
func (s *Server) startPump() {
	ch := s.a.Subscribe(s.ctx)
	s.goBackground(func() {
		defer s.events.close()
		for {
			select {
			case <-s.ctx.Done():
				return
			case e, ok := <-ch:
				if ok {
					s.events.publish(frameOf(e))
					continue
				}
				if s.ctx.Err() != nil {
					return
				}
				// cut for lagging (or the App closed): follow again
				last := s.events.last()
				ch = s.a.Subscribe(s.ctx)
				select {
				case e, ok := <-ch:
					if !ok { // the App is closed: no more events, ever
						return
					}
					s.events.lost(last)
					s.events.publish(frameOf(e))
				default:
					s.events.lost(last)
				}
			}
		}
	})
}

// serveEvents is GET /v1/events.
func (s *Server) serveEvents(w http.ResponseWriter, r *http.Request) {
	from := r.Header.Get("Last-Event-ID")
	if from == "" {
		from = r.URL.Query().Get("since")
	}
	var since uint64
	resume := from != ""
	if resume {
		n, err := strconv.ParseUint(from, 10, 64)
		if err != nil {
			writeErr(w, http.StatusBadRequest, proto.CodeBadRequest, "since/Last-Event-ID: "+err.Error())
			return
		}
		since = n
	}
	l := s.events.subscribe(since, resume)
	if l == nil {
		writeErr(w, http.StatusServiceUnavailable, proto.CodeFailed, "servidor encerrando")
		return
	}
	defer s.events.unsubscribe(l)

	rc := http.NewResponseController(w)
	h := w.Header()
	h.Set("Content-Type", "text/event-stream")
	h.Set("Cache-Control", "no-cache")
	h.Set("X-Accel-Buffering", "no")
	w.WriteHeader(http.StatusOK)
	fmt.Fprint(w, "retry: 1000\n: ok\n\n")
	if l.gap != nil {
		b, _ := json.Marshal(l.gap)
		g, _ := json.Marshal(proto.Event{Name: proto.EvGap, Data: b})
		writeFrame(w, frame{name: proto.EvGap, data: g})
	}
	for _, f := range l.replay {
		writeFrame(w, f)
	}
	if rc.Flush() != nil {
		return
	}
	beat := time.NewTicker(s.o.Heartbeat)
	defer beat.Stop()
	for {
		select {
		case <-r.Context().Done():
			return
		case f, ok := <-l.ch:
			if !ok {
				return
			}
			writeFrame(w, f)
		case <-beat.C:
			fmt.Fprint(w, ": ping\n\n")
		}
		if rc.Flush() != nil {
			return
		}
	}
}

// writeFrame writes one SSE message; a gap has no id (it is not an event a
// client could resume from).
func writeFrame(w http.ResponseWriter, f frame) {
	if f.seq != 0 {
		fmt.Fprintf(w, "id: %d\n", f.seq)
	}
	fmt.Fprintf(w, "event: %s\ndata: %s\n\n", f.name, f.data)
}
