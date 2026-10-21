package demo

import (
	"fmt"
	"reflect"
	"strings"
	"testing"
	"time"
	"unicode/utf8"

	"github.com/aqilputi/nekuroddo/internal/app"
	"github.com/aqilputi/nekuroddo/internal/claude"
	"github.com/aqilputi/nekuroddo/internal/config"
	"github.com/aqilputi/nekuroddo/internal/core"
)

var scripted = []string{IDGateway, IDMigration, IDReport, IDDotfiles, IDBlog, IDIndexer, IDDocs}

func last(ls []claude.TailLine) claude.TailLine {
	if len(ls) == 0 {
		return claude.TailLine{}
	}
	return ls[len(ls)-1]
}

func TestTailShape(t *testing.T) {
	kinds := map[string]bool{claude.TailTool: true, claude.TailText: true, claude.TailUser: true,
		claude.TailResult: true, claude.TailSub: true}
	for _, e := range []time.Duration{0, 7 * time.Second, StateEvery + 3*time.Second, 10 * time.Minute, 26 * time.Hour} {
		for _, id := range scripted {
			ls := Tail(t0, e, id, core.TailMax)
			if len(ls) == 0 {
				t.Fatalf("%s at %v: empty tail", id, e)
			}
			for i, l := range ls {
				if !kinds[l.Kind] || l.Text == "" || utf8.RuneCountInString(l.Text) > claude.TailWidth {
					t.Errorf("%s at %v line %d: %+v", id, e, i, l)
				}
				if l.At.After(t0.Add(e)) {
					t.Errorf("%s at %v line %d is in the future: %v", id, e, i, l.At)
				}
				if i > 0 && l.At.Before(ls[i-1].At) {
					t.Errorf("%s at %v line %d goes back in time", id, e, i)
				}
			}
			if n := TailSeq(t0, e, id); n < len(ls) {
				t.Errorf("%s at %v: seq %d < %d lines", id, e, n, len(ls))
			}
		}
	}
	if Tail(t0, 0, "nope", 10) != nil || TailSeq(t0, 0, "nope") != 0 {
		t.Error("an unknown session has no tail")
	}
	if got := Tail(t0, time.Hour, IDIndexer, 5); len(got) != 5 {
		t.Errorf("Tail(5) = %d lines", len(got))
	}
}

// The tail is a pure function of the clock, and the counter agrees with it:
// every line counted is one that shows up.
func TestTailDeterministicAndCounted(t *testing.T) {
	for _, id := range scripted {
		var prevSeq int
		var prev []claude.TailLine
		for e := time.Duration(0); e <= 4*time.Minute; e += 700 * time.Millisecond {
			ls, seq := Tail(t0, e, id, core.TailMax), TailSeq(t0, e, id)
			if !reflect.DeepEqual(ls, Tail(t0, e, id, core.TailMax)) {
				t.Fatalf("%s at %v: not deterministic", id, e)
			}
			if seq < prevSeq {
				t.Fatalf("%s at %v: seq went back %d → %d", id, e, prevSeq, seq)
			}
			// the new lines are exactly the ones appended since the last look
			if d := seq - prevSeq; d > 0 && d < len(ls) && len(prev) > 0 {
				if !reflect.DeepEqual(ls[len(ls)-d-1], last(prev)) {
					t.Fatalf("%s at %v: %d new lines but the old tail does not line up", id, e, d)
				}
			}
			if seq == prevSeq && len(prev) > 0 && !reflect.DeepEqual(ls, prev) {
				t.Fatalf("%s at %v: tail changed with no new lines", id, e)
			}
			prev, prevSeq = ls, seq
		}
	}
}

func TestTailBusyKeepsWriting(t *testing.T) {
	for _, id := range []string{IDIndexer, IDGateway, IDDocs} {
		// within a busy step a line comes every 1–2 s
		var gaps []time.Duration
		ls := Tail(t0, StateEvery-time.Millisecond, id, core.TailMax)
		for i := 1; i < len(ls); i++ {
			if !ls[i].At.Before(t0) && !ls[i-1].At.Before(t0) {
				gaps = append(gaps, ls[i].At.Sub(ls[i-1].At))
			}
		}
		if len(gaps) < 8 {
			t.Fatalf("%s: %d lines in the first busy step", id, len(gaps)+1)
		}
		for _, g := range gaps {
			if g < 500*time.Millisecond || g > 2500*time.Millisecond {
				t.Errorf("%s: gap %v", id, g)
			}
		}
	}
	// code-like activity: tool calls, results, diffs, test output
	all := ""
	for _, l := range Tail(t0, 10*time.Minute, IDIndexer, core.TailMax) {
		all += l.Text + "\n"
	}
	for _, want := range []string{"● Bash(go test", "● Edit(", "● Read(", "  ⎿ ", "  +  ", "  -  ", "◆ "} {
		if !strings.Contains(all, want) {
			t.Errorf("indexer tail has no %q", want)
		}
	}
}

func TestTailEndsByState(t *testing.T) {
	blocked := StateEvery + time.Second // gateway blocked, docs done
	gw := Tail(t0, blocked, IDGateway, core.TailMax)
	if l := last(gw); l.Text != "    3. esperar o review" {
		t.Errorf("blocked gateway ends with %q", l.Text)
	}
	if q := gw[len(gw)-4]; q.Kind != claude.TailTool || q.Text != "● AskUserQuestion(Aplico a migração 0042 no staging?)" {
		t.Errorf("blocked gateway asks %+v", q)
	}
	// the question stays the last thing until it is answered
	if !reflect.DeepEqual(Tail(t0, 2*StateEvery-time.Millisecond, IDGateway, core.TailMax), gw) {
		t.Error("a blocked session writes nothing")
	}
	if l := Tail(t0, 2*StateEvery+2*time.Second, IDGateway, 20); !hasText(l, "> sim, aplicar agora") {
		t.Errorf("the answer comes back as a prompt: %+v", l)
	}
	if l := last(Tail(t0, blocked, IDDocs, 5)); l.Kind != claude.TailText || !strings.Contains(l.Text, "arquivo") {
		t.Errorf("done docs ends with %+v", l)
	}
	if l := last(Tail(t0, 2*StateEvery+time.Second, IDDocs, 5)); l.Kind != claude.TailUser || l.Text != ">" {
		t.Errorf("idle docs ends with %+v", l)
	}
	if l := Tail(t0, 3*StateEvery+2*time.Second, IDDocs, 5); !hasText(l, "> revisa de novo a página de instalação") {
		t.Errorf("docs gets a new request: %+v", l)
	}
	if l := last(Tail(t0, time.Hour, IDMigration, 5)); !strings.HasPrefix(l.Text, "    3. copiar") {
		t.Errorf("migration ends with %+v", l)
	}
	if l := last(Tail(t0, time.Hour, IDReport, 5)); l.Kind != claude.TailText {
		t.Errorf("report ends with %+v", l)
	}
	if l := last(Tail(t0, time.Hour, IDDotfiles, 5)); l.Text != ">" {
		t.Errorf("dotfiles ends with %+v", l)
	}
	// static sessions never change
	if TailSeq(t0, 0, IDMigration) != TailSeq(t0, time.Hour, IDMigration) {
		t.Error("migration's tail grew")
	}
}

func hasText(ls []claude.TailLine, s string) bool {
	for _, l := range ls {
		if l.Text == s {
			return true
		}
	}
	return false
}

func TestScenarioTail(t *testing.T) {
	s, clk := newScenario(t)
	repo := s.Ports().Sessions
	snap := repo.Refresh()
	for _, x := range snap.Sessions {
		if x.TailSeq != TailSeq(t0, 0, x.SessionID) || x.TailSeq == 0 {
			t.Errorf("%s: TailSeq %d", x.SessionID, x.TailSeq)
		}
	}
	if ls, err := repo.Tail(IDIndexer, 3); err != nil || !reflect.DeepEqual(ls, Tail(t0, 0, IDIndexer, 3)) {
		t.Fatalf("Tail = %+v, %v", ls, err)
	}
	if ls, err := repo.Tail("nope", 3); err != nil || ls != nil {
		t.Fatalf("unknown = %+v, %v", ls, err)
	}

	// a dispatched session: its prompt, some work, then done
	if _, err := s.Ports().Claude.Dispatch("/demo/oss/x", "nova", "faz a coisa"); err != nil {
		t.Fatal(err)
	}
	id := extraID(0)
	if ls, _ := repo.Tail(id, 10); len(ls) != 1 || ls[0].Text != "> faz a coisa" || ls[0].Kind != claude.TailUser {
		t.Fatalf("new session tail = %+v", ls)
	}
	clk.Advance(5 * time.Second)
	busy, _ := repo.Tail(id, core.TailMax)
	if len(busy) < 3 {
		t.Fatalf("dispatched session after 5s: %+v", busy)
	}
	if got := find(t, repo.Refresh().Sessions, id).TailSeq; got != len(busy) {
		t.Errorf("dispatched TailSeq = %d, want %d", got, len(busy))
	}
	clk.Advance(DispatchBusy)
	done, _ := repo.Tail(id, core.TailMax)
	if l := last(done); l.Kind != claude.TailText || !strings.Contains(l.Text, "pronto") {
		t.Fatalf("dispatched session done: %+v", l)
	}
	clk.Advance(time.Hour)
	if later, _ := repo.Tail(id, core.TailMax); !reflect.DeepEqual(later, done) {
		t.Error("a finished dispatch keeps writing")
	}
}

// Through app: session.tail goes out for the sessions that wrote something,
// as on the real path.
func TestTailEventsThroughApp(t *testing.T) {
	t.Setenv("XDG_CACHE_HOME", t.TempDir())
	s, clk := newScenario(t)
	a := app.New(config.Default(), s.Ports())
	defer a.Close()
	ev := a.Subscribe(t.Context())
	a.Refresh()
	clk.Advance(3 * time.Second)
	a.Refresh()
	got := map[string]int{}
	for {
		select {
		case e := <-ev:
			if e.Name == app.EvSessionTail {
				d := e.Data.(app.TailChanged)
				got[d.ID] = d.NNew
			}
			continue
		default:
		}
		break
	}
	want := map[string]int{}
	for _, id := range []string{IDGateway, IDIndexer, IDDocs} {
		want[id] = TailSeq(t0, 3*time.Second, id) - TailSeq(t0, 0, id)
	}
	if fmt.Sprint(got) != fmt.Sprint(want) {
		t.Fatalf("session.tail = %v, want %v (only the busy ones)", got, want)
	}
	ls, err := a.SessionTail(IDIndexer, 2)
	if err != nil || len(ls) != 2 || ls[1].Text != last(Tail(t0, 3*time.Second, IDIndexer, 1)).Text {
		t.Fatalf("SessionTail = %+v, %v", ls, err)
	}
}
