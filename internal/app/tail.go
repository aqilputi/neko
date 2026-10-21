package app

// Slice "tail": each session's activity tail, the code-like lines the GUI
// scrolls on a cat's monitor so it never attaches a terminal just to show
// that a session is working. The lines come from the session repository
// (store: parsed incrementally from the transcript; demo: scripted); app
// clamps the request and tells clients, after each refresh, which tails
// grew (session.tail), so they refetch only those.

import (
	"errors"
	"sync"

	"github.com/aqilputi/nekuroddo/internal/core"
	"github.com/aqilputi/nekuroddo/internal/store"
)

// EvSessionTail is published after a refresh for each session whose tail
// got new lines.
const EvSessionTail = "session.tail"

// ErrUnknownSession: no session with that id in the core's snapshot.
var ErrUnknownSession = errors.New("sessão desconhecida")

// TailChanged is the payload of session.tail. NNew is how many lines were
// added since the last refresh, at most core.TailMax (the most a client can
// fetch with SessionTail).
type TailChanged struct {
	ID   string `json:"id"`
	NNew int    `json:"n_new"`
}

// tailState remembers each session's TailSeq as of the last refresh.
type tailState struct {
	mu   sync.Mutex
	seen bool // a first refresh happened: it only records
	seq  map[string]int
}

// SessionTail is the last n lines of a session's activity tail, oldest
// first; n is clamped to 1..core.TailMax. A session in the snapshot with no
// lines yet has an empty list; one that is not there is ErrUnknownSession.
func (a *App) SessionTail(sessionID string, n int) ([]core.TailLine, error) {
	if _, ok := a.lookup(sessionID); !ok {
		return nil, ErrUnknownSession
	}
	lines, err := a.ports.Sessions.Tail(sessionID, core.ClampTail(n))
	if err != nil {
		return nil, err
	}
	out := make([]core.TailLine, 0, len(lines))
	for _, l := range lines {
		out = append(out, core.TailLine{At: l.At, Kind: l.Kind, Text: l.Text})
	}
	return out, nil
}

// observeTails publishes session.tail for every session whose TailSeq grew
// since the last refresh. The first refresh only records (clients read every
// tail they show when they start); a session seen for the first time after
// that, or whose counter went back (a rewritten transcript), counts all of
// its lines as new.
func (a *App) observeTails(sessions []store.Session) {
	t := &a.tails
	var out []TailChanged
	t.mu.Lock()
	next := make(map[string]int, len(sessions))
	for _, s := range sessions {
		next[s.SessionID] = s.TailSeq
		prev, had := t.seq[s.SessionID]
		if !t.seen || s.TailSeq == prev {
			continue
		}
		n := s.TailSeq - prev
		if !had || n < 0 {
			n = s.TailSeq
		}
		if n > 0 {
			out = append(out, TailChanged{ID: s.SessionID, NNew: min(n, core.TailMax)})
		}
	}
	t.seq, t.seen = next, true
	t.mu.Unlock()
	for _, c := range out {
		a.Publish(EvSessionTail, c)
	}
}
