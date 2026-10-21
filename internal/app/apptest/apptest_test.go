package apptest

import (
	"errors"
	"reflect"
	"testing"
	"time"

	"github.com/aqilputi/nekuroddo/internal/claude"
	"github.com/aqilputi/nekuroddo/internal/events"
	"github.com/aqilputi/nekuroddo/internal/groups"
	"github.com/aqilputi/nekuroddo/internal/jira"
	"github.com/aqilputi/nekuroddo/internal/store"
	"github.com/aqilputi/nekuroddo/internal/todo"
)

var errBoom = errors.New("boom")

func TestPortsAreAllFakes(t *testing.T) {
	p := Ports()
	v := reflect.ValueOf(p)
	for i := range v.NumField() {
		if v.Field(i).IsNil() {
			t.Errorf("port %s is nil", v.Type().Field(i).Name)
		}
	}
	f := New()
	fp := f.Ports()
	if fp.Clock != f.Clock || fp.Claude != f.Claude || fp.Todo != f.Todo || fp.Panes != f.Panes {
		t.Fatal("Fakes.Ports must expose the same fakes")
	}
}

func TestFakeClock(t *testing.T) {
	c := NewFakeClock(time.Time{})
	if !c.Now().Equal(Epoch) {
		t.Fatalf("zero start = %v, want Epoch", c.Now())
	}
	c.Advance(time.Minute)
	if got := c.Now(); !got.Equal(Epoch.Add(time.Minute)) {
		t.Fatalf("after Advance = %v", got)
	}
	at := time.Date(2030, 1, 2, 3, 4, 5, 0, time.UTC)
	c.Set(at)
	if !c.Now().Equal(at) {
		t.Fatalf("after Set = %v", c.Now())
	}
}

func TestRecorder(t *testing.T) {
	var r Recorder
	r.record("A", 1, "x")
	r.record("B")
	r.record("A", 2)
	if r.Count("A") != 2 || r.Count("C") != 0 {
		t.Fatalf("counts: %v", r.Calls())
	}
	if got := r.CallsTo("A"); len(got) != 2 || got[1].Args[0] != 2 {
		t.Fatalf("CallsTo = %+v", got)
	}
	if got := r.Last(); got.Method != "A" {
		t.Fatalf("Last = %+v", got)
	}
	var empty Recorder
	if got := empty.Last(); got.Method != "" {
		t.Fatalf("empty Last = %+v", got)
	}
	r.Reset()
	if len(r.Calls()) != 0 {
		t.Fatal("Reset left calls")
	}
}

func TestFakeClaude(t *testing.T) {
	c := &FakeClaude{Agents: []claude.Agent{{ID: "a"}}, NextID: "0123abcd", Usage: claude.UsageReport{Raw: "u"}}
	if as, err := c.ListAgents(); err != nil || len(as) != 1 {
		t.Fatal(as, err)
	}
	if id, err := c.Dispatch("/d", "n", "p", "--x"); err != nil || id != "0123abcd" {
		t.Fatal(id, err)
	}
	if id, err := c.ResumeInBackground("/d", "s"); err != nil || id != "0123abcd" {
		t.Fatal(id, err)
	}
	if c.Stop("a") != nil || c.Remove("a") != nil {
		t.Fatal("Stop/Remove")
	}
	if got := c.AttachArgv("a"); !reflect.DeepEqual(got, []string{"claude", "attach", "a"}) {
		t.Fatal(got)
	}
	if u, err := c.FetchUsage(); err != nil || u.Raw != "u" {
		t.Fatal(u, err)
	}
	want := []string{"ListAgents", "Dispatch", "ResumeInBackground", "Stop", "Remove", "AttachArgv", "FetchUsage"}
	var got []string
	for _, cl := range c.Calls() {
		got = append(got, cl.Method)
	}
	if !reflect.DeepEqual(got, want) {
		t.Fatal(got)
	}
	if d := c.CallsTo("Dispatch")[0]; !reflect.DeepEqual(d.Args, []any{"/d", "n", "p", []string{"--x"}}) {
		t.Fatalf("Dispatch args = %#v", d.Args)
	}

	c.Errs = map[string]error{"ListAgents": errBoom, "Dispatch": errBoom, "ResumeInBackground": errBoom,
		"Stop": errBoom, "Remove": errBoom, "FetchUsage": errBoom}
	if _, err := c.ListAgents(); !errors.Is(err, errBoom) {
		t.Fatal(err)
	}
	if _, err := c.Dispatch("", "", ""); !errors.Is(err, errBoom) {
		t.Fatal(err)
	}
	if _, err := c.ResumeInBackground("", ""); !errors.Is(err, errBoom) {
		t.Fatal(err)
	}
	if !errors.Is(c.Stop(""), errBoom) || !errors.Is(c.Remove(""), errBoom) {
		t.Fatal("errs")
	}
	if _, err := c.FetchUsage(); !errors.Is(err, errBoom) {
		t.Fatal(err)
	}
}

func TestFakeSessions(t *testing.T) {
	s := &FakeSessions{Snap: store.Snapshot{Sessions: []store.Session{{SessionID: "a"}}}}
	if got := s.Refresh(); len(got.Sessions) != 1 {
		t.Fatal(got)
	}
	s.Reapply(nil)
	s.Archive("a", "b")
	s.Unarchive("a")
	s.Rename("a", "n")
	s.SetContext("a", "c")
	s.RenameContext("c", "d")
	s.MarkSeen("a", Epoch)
	s.SetWindowStart(Epoch)
	if s.Count("Refresh") != 1 || s.Count("Archive") != 1 || len(s.Calls()) != 9 {
		t.Fatalf("calls = %+v", s.Calls())
	}
	if a := s.CallsTo("Archive")[0].Args; !reflect.DeepEqual(a, []any{[]string{"a", "b"}}) {
		t.Fatalf("Archive args = %#v", a)
	}
}

func TestFakeSessionsTail(t *testing.T) {
	s := &FakeSessions{}
	if ls, err := s.Tail("a", 5); err != nil || len(ls) != 0 {
		t.Fatalf("no tail: %+v, %v", ls, err)
	}
	s.SetTail("a", []claude.TailLine{{Text: "1"}, {Text: "2"}, {Text: "3"}})
	if ls, _ := s.Tail("a", 2); len(ls) != 2 || ls[0].Text != "2" {
		t.Fatalf("Tail(2) = %+v", ls)
	}
	if ls, _ := s.Tail("a", 0); len(ls) != 3 {
		t.Fatalf("Tail(0) = %+v", ls)
	}
	s.TailErr = errBoom
	if _, err := s.Tail("a", 1); !errors.Is(err, errBoom) {
		t.Fatal(err)
	}
	if s.Count("Tail") != 4 {
		t.Fatalf("calls = %+v", s.Calls())
	}
}

func TestFakeGroups(t *testing.T) {
	g := NewFakeGroups()
	f := g.Load()
	f.Add("x", groups.Member{SessionID: "s"})
	if len(g.Load().Groups) != 0 {
		t.Fatal("Load must return a copy")
	}
	if err := g.Save(f); err != nil {
		t.Fatal(err)
	}
	f.Add("y", groups.Member{SessionID: "s"})
	if got := g.Load(); len(got.Groups) != 1 || len(got.Groups["x"]) != 1 {
		t.Fatalf("Save must store a copy: %+v", got)
	}
	if err := g.Queue("s", groups.Note{Text: "oi"}); err != nil {
		t.Fatal(err)
	}
	if q := g.Queued("s"); len(q) != 1 || q[0].Text != "oi" {
		t.Fatal(q)
	}
	g.Errs = map[string]error{"Save": errBoom, "Queue": errBoom}
	if !errors.Is(g.Save(f), errBoom) || !errors.Is(g.Queue("s", groups.Note{}), errBoom) {
		t.Fatal("errs")
	}
	if len(g.Queued("s")) != 1 || len(g.Load().Groups) != 1 {
		t.Fatal("failed writes must not change state")
	}
	var zero FakeGroups
	if zero.Load().Groups == nil {
		t.Fatal("zero FakeGroups must load a usable File")
	}
}

func TestFakeEvents(t *testing.T) {
	e := &FakeEvents{}
	if e.Poll() != nil {
		t.Fatal("empty Poll")
	}
	e.Push(events.Event{SessionID: "a"}, events.Event{SessionID: "b"})
	if got := e.Poll(); len(got) != 2 {
		t.Fatal(got)
	}
	if e.Poll() != nil {
		t.Fatal("Poll must drain")
	}
}

func TestFakePanes(t *testing.T) {
	ps := &FakePanes{}
	p, err := ps.Start("id", "t", "/d", 80, 24, "a", "b")
	if err != nil || p.ID() != "id" || p.Title() != "t" {
		t.Fatal(p, err)
	}
	if got := ps.Pane("id"); got == nil || got.Argv[1] != "b" || got.Dir != "/d" {
		t.Fatalf("Pane = %+v", got)
	}
	if ps.Pane("nope") != nil {
		t.Fatal("unknown pane")
	}
	ps.Err = errBoom
	if _, err := ps.Start("x", "", "", 1, 1); !errors.Is(err, errBoom) {
		t.Fatal(err)
	}
}

func TestFakePane(t *testing.T) {
	p := NewFakePane("id", "t")
	if _, err := p.Write([]byte("ls\r")); err != nil {
		t.Fatal(err)
	}
	if p.Written() != "ls\r" {
		t.Fatal(p.Written())
	}
	p.Resize(100, 30)
	if c, r := p.Size(); c != 100 || r != 30 {
		t.Fatal(c, r)
	}
	p.SetTitle("novo")
	if p.Title() != "novo" {
		t.Fatal(p.Title())
	}

	bs, cancel := p.Subscribe()
	p.Emit([]byte("out"))
	if got := <-bs; string(got) != "out" {
		t.Fatal(string(got))
	}
	select {
	case <-p.Output():
	default:
		t.Fatal("Emit must signal Output")
	}
	p.Emit([]byte("a"))
	p.Emit([]byte("b")) // coalesced signal: must not block
	cancel()
	cancel() // idempotent
	if _, ok := <-bs; !ok {
		t.Fatal("buffered bytes are kept after cancel")
	}

	if p.Done() {
		t.Fatal("Done before Exit")
	}
	bs2, _ := p.Subscribe()
	p.Exit(errBoom)
	if err := <-p.Exited(); !errors.Is(err, errBoom) || !p.Done() {
		t.Fatal(err)
	}
	// Like term.Process: later receives get nil, subscribers close at exit.
	if err := <-p.Exited(); err != nil {
		t.Fatalf("second receive = %v", err)
	}
	if _, ok := <-bs2; ok {
		t.Fatal("subscriber open after Exit")
	}
	p.Exit(nil) // idempotent
	if b2, _ := p.Subscribe(); b2 != nil {
		if _, ok := <-b2; ok {
			t.Fatal("Subscribe after Exit must give a closed channel")
		}
	}
	p.Close()
	if !p.Closed() {
		t.Fatal("Close")
	}
	p.Close() // idempotent
	if _, err := p.Write([]byte("x")); err == nil {
		t.Fatal("Write after Close must fail")
	}
}

func TestFakeNotifierOpener(t *testing.T) {
	n := &FakeNotifier{}
	if err := n.Notify("t", "b"); err != nil {
		t.Fatal(err)
	}
	if got := n.Notes(); len(got) != 1 || got[0] != (Note{"t", "b"}) {
		t.Fatal(got)
	}
	n.Err = errBoom
	if !errors.Is(n.Notify("x", "y"), errBoom) || len(n.Notes()) != 1 {
		t.Fatal("Err")
	}

	o := &FakeOpener{}
	if err := o.OpenURL("u"); err != nil {
		t.Fatal(err)
	}
	if got := o.URLs(); !reflect.DeepEqual(got, []string{"u"}) {
		t.Fatal(got)
	}
	o.Err = errBoom
	if !errors.Is(o.OpenURL("v"), errBoom) || len(o.URLs()) != 1 {
		t.Fatal("Err")
	}
}

func TestFakeJira(t *testing.T) {
	j := &FakeJira{Cache: jira.Cache{Issues: []jira.Issue{{Key: "A-1"}}}, Key: "A-2", Cost: 0.1}
	if c, err := j.Fetch(jira.Config{}); err != nil || c.Issues[0].Key != "A-1" {
		t.Fatal(c, err)
	}
	if k, cost, err := j.Create(jira.Config{}, jira.NewIssue{Summary: "s"}); err != nil || k != "A-2" || cost != 0.1 {
		t.Fatal(k, cost, err)
	}
	if j.Count("Create") != 1 || j.CallsTo("Create")[0].Args[1].(jira.NewIssue).Summary != "s" {
		t.Fatal(j.Calls())
	}
	j.Errs = map[string]error{"Fetch": errBoom, "Create": errBoom}
	if _, err := j.Fetch(jira.Config{}); !errors.Is(err, errBoom) {
		t.Fatal(err)
	}
	if _, _, err := j.Create(jira.Config{}, jira.NewIssue{}); !errors.Is(err, errBoom) {
		t.Fatal(err)
	}
}

func TestFakeTodo(t *testing.T) {
	td := &FakeTodo{Avail: true, Groups: []todo.Group{{Group: "g"}}}
	if !td.Available() {
		t.Fatal("Available")
	}
	if gs, err := td.List(); err != nil || len(gs) != 1 {
		t.Fatal(gs, err)
	}
	for _, err := range []error{td.Start(1), td.Done(1), td.LinkJira(1, "A-1"), td.AddFromJira("A-1", "s"), td.AddNote(1, "n")} {
		if err != nil {
			t.Fatal(err)
		}
	}
	if len(td.Calls()) != 7 {
		t.Fatal(td.Calls())
	}
	td.Errs = map[string]error{"List": errBoom, "Start": errBoom, "Done": errBoom, "LinkJira": errBoom,
		"AddFromJira": errBoom, "AddNote": errBoom}
	if _, err := td.List(); !errors.Is(err, errBoom) {
		t.Fatal(err)
	}
	for _, err := range []error{td.Start(1), td.Done(1), td.LinkJira(1, "A-1"), td.AddFromJira("A-1", "s"), td.AddNote(1, "n")} {
		if !errors.Is(err, errBoom) {
			t.Fatal(err)
		}
	}
}
