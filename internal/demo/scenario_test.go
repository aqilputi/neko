package demo

import (
	"reflect"
	"strings"
	"testing"
	"time"

	"github.com/aqilputi/nekuroddo/internal/app"
	"github.com/aqilputi/nekuroddo/internal/app/apptest"
	"github.com/aqilputi/nekuroddo/internal/config"
	"github.com/aqilputi/nekuroddo/internal/groups"
	"github.com/aqilputi/nekuroddo/internal/jira"
	"github.com/aqilputi/nekuroddo/internal/store"
)

func newScenario(t *testing.T) (*Scenario, *apptest.FakeClock) {
	t.Helper()
	clk := apptest.NewFakeClock(t0)
	return New(clk), clk
}

func find(t *testing.T, ss []store.Session, id string) store.Session {
	t.Helper()
	for _, s := range ss {
		if s.SessionID == id {
			return s
		}
	}
	t.Fatalf("no session %s", id)
	return store.Session{}
}

func has(ss []store.Session, id string) bool {
	for _, s := range ss {
		if s.SessionID == id {
			return true
		}
	}
	return false
}

func TestPortsAreAllSet(t *testing.T) {
	for _, p := range []app.Ports{Ports(apptest.NewFakeClock(t0)), Ports(nil)} {
		v := reflect.ValueOf(p)
		for i := range v.NumField() {
			if v.Field(i).IsNil() {
				t.Errorf("Ports.%s is nil", v.Type().Field(i).Name)
			}
		}
	}
	// nil clock: the wall clock
	s := New(nil)
	if d := time.Since(s.Start()); d < 0 || d > time.Minute {
		t.Errorf("start = %v", s.Start())
	}
	if s.Elapsed() < 0 {
		t.Errorf("elapsed = %v", s.Elapsed())
	}
}

func TestRefreshFollowsTheClock(t *testing.T) {
	s, clk := newScenario(t)
	repo := s.Ports().Sessions
	snap := repo.Refresh()
	if len(snap.Sessions) != 7 || !snap.At.Equal(t0) || snap.Err != nil {
		t.Fatalf("snapshot = %d sessions at %v", len(snap.Sessions), snap.At)
	}
	var today float64
	for _, x := range snap.Sessions {
		today += x.CostToday
	}
	if snap.TodayCost != today || today == 0 {
		t.Errorf("today = %v, want %v", snap.TodayCost, today)
	}
	if find(t, snap.Sessions, IDGateway).State != "busy" {
		t.Error("gateway should start busy")
	}
	clk.Advance(StateEvery)
	if s.Elapsed() != StateEvery {
		t.Errorf("elapsed = %v", s.Elapsed())
	}
	if got := find(t, repo.Refresh().Sessions, IDGateway); got.State != "blocked" {
		t.Errorf("after 15 s gateway = %s", got.State)
	}
	clk.Set(t0.Add(-time.Hour)) // a clock before the start is the start
	if s.Elapsed() != 0 {
		t.Errorf("elapsed before start = %v", s.Elapsed())
	}
}

func TestSessionOverrides(t *testing.T) {
	s, clk := newScenario(t)
	repo := s.Ports().Sessions
	repo.Rename(IDDocs, "documentação")
	repo.SetContext(IDDotfiles, "casa")
	repo.RenameContext(CtxOSS, "open source")
	repo.Archive(IDBlog, IDReport)
	repo.SetWindowStart(t0) // accepted, no effect
	ss := repo.Refresh().Sessions
	if d := find(t, ss, IDDocs); d.CustomName != "documentação" || d.Label() != "documentação" || d.Context != "open source" {
		t.Errorf("docs = %q in %q", d.Label(), d.Context)
	}
	if d := find(t, ss, IDDotfiles); d.Context != "casa" || !d.ContextSet {
		t.Errorf("dotfiles context = %q %v", d.Context, d.ContextSet)
	}
	if r := find(t, ss, IDReport); !r.Archived || !r.Stale || !r.ArchivedAt.Equal(t0) {
		t.Errorf("report archive = %+v", r)
	}
	repo.Unarchive(IDReport)
	repo.Rename(IDDocs, "")
	repo.SetContext(IDDotfiles, "")
	repo.RenameContext(CtxOSS, CtxOSS)
	ss = repo.Refresh().Sessions
	if r := find(t, ss, IDReport); r.Archived {
		t.Error("report still archived")
	}
	if d := find(t, ss, IDDocs); d.CustomName != "" || d.Context != CtxOSS {
		t.Errorf("docs after undo = %q in %q", d.CustomName, d.Context)
	}
	if d := find(t, ss, IDDotfiles); d.Context != CtxPessoal || d.ContextSet {
		t.Errorf("dotfiles after undo = %q", d.Context)
	}
	if !find(t, ss, IDBlog).Archived {
		t.Error("blog lost its archive")
	}

	// MarkSeen: a returned session stops asking until it returns again
	clk.Advance(StateEvery) // docs: done
	if d := find(t, repo.Refresh().Sessions, IDDocs); d.Attention != store.AttnReturned {
		t.Fatalf("docs at step 1 = %+v", d)
	}
	repo.MarkSeen(IDDocs, clk.Now())
	repo.MarkSeen(IDReport, clk.Now())
	ss = repo.Refresh().Sessions
	if d := find(t, ss, IDDocs); d.Attention != store.AttnNone || !d.Finished {
		t.Errorf("seen docs = %+v", d)
	}
	if r := find(t, ss, IDReport); r.Attention != store.AttnNone {
		t.Errorf("seen report = %+v", r)
	}
	clk.Advance(3 * StateEvery) // step 4: done again, after the look
	if d := find(t, repo.Refresh().Sessions, IDDocs); d.Attention != store.AttnReturned || d.Finished {
		t.Errorf("docs done again = %+v", d)
	}
}

func TestReapplyInPlace(t *testing.T) {
	s, _ := newScenario(t)
	repo := s.Ports().Sessions
	ss := repo.Refresh().Sessions
	repo.Rename(IDGateway, "portão")
	repo.SetContext(IDGateway, "infra")
	repo.Archive(IDGateway)
	repo.Reapply(ss)
	g := find(t, ss, IDGateway)
	if g.CustomName != "portão" || g.Context != "infra" || !g.ContextSet || !g.Archived {
		t.Errorf("reapplied = %+v", g)
	}
	repo.Reapply([]store.Session{{SessionID: "unknown"}}) // ignored
}

func TestClaudePort(t *testing.T) {
	s, clk := newScenario(t)
	p := s.Ports()
	ags, err := p.Claude.ListAgents()
	if err != nil || len(ags) != 5 {
		t.Fatalf("agents = %+v, %v", ags, err)
	}
	states := map[string]string{}
	for _, a := range ags {
		states[a.SessionID] = a.State
		if a.ID == "" || a.Kind != "background" || a.StartedAt == 0 {
			t.Errorf("agent = %+v", a)
		}
	}
	if states[IDGateway] != "running" || states[IDMigration] != "blocked" || states[IDReport] != "done" {
		t.Errorf("agent states = %v", states)
	}
	if argv := p.Claude.AttachArgv("dmgatewy"); strings.Join(argv, " ") != "claude attach dmgatewy" {
		t.Errorf("argv = %v", argv)
	}
	u, err := p.Claude.FetchUsage()
	if err != nil || len(u.Limits) != 3 || u.Limits[0].Pct != 60 || !u.At.Equal(t0) || u.Raw == "" {
		t.Errorf("usage = %+v, %v", u, err)
	}

	// dispatch: a new busy session, done after DispatchBusy
	id, err := p.Claude.Dispatch("/demo/oss/novo", "nova coisa", "faz uma coisa nova")
	if err != nil || len(id) != 8 {
		t.Fatalf("dispatch = %q, %v", id, err)
	}
	id2, _ := p.Claude.Dispatch("/tmp/x", "", "um prompt bem comprido que vira o nome da sessão nova")
	if id2 == id {
		t.Error("dispatch ids repeat")
	}
	ss := p.Sessions.Refresh().Sessions
	if len(ss) != 9 {
		t.Fatalf("%d sessions after dispatch", len(ss))
	}
	var nova, other store.Session
	for _, x := range ss {
		switch x.ShortID {
		case id:
			nova = x
		case id2:
			other = x
		}
	}
	if nova.State != "busy" || nova.Context != CtxOSS || nova.Label() != "nova coisa" || nova.Kind != "background" {
		t.Errorf("dispatched = %+v", nova)
	}
	if other.Context != "demo" || other.Label() == "" {
		t.Errorf("dispatched outside /demo = %+v", other)
	}
	clk.Advance(DispatchBusy)
	for _, x := range p.Sessions.Refresh().Sessions {
		if x.ShortID == id && (x.State != "done" || x.Attention != store.AttnReturned) {
			t.Errorf("dispatched after %v = %s/%d", DispatchBusy, x.State, x.Attention)
		}
	}

	// resume a history session into the background
	short, err := p.Claude.ResumeInBackground("/demo/pessoal/blog", IDBlog)
	if err != nil || short == "" {
		t.Fatalf("resume = %q, %v", short, err)
	}
	if b := find(t, p.Sessions.Refresh().Sessions, IDBlog); b.ShortID != short || b.State != "busy" || b.Kind != "background" || b.Stale {
		t.Errorf("resumed = %+v", b)
	}
	if again, _ := p.Claude.ResumeInBackground("", IDBlog); again != short {
		t.Errorf("resume twice = %q, want %q", again, short)
	}
	if _, err := p.Claude.ResumeInBackground("", "nope"); err == nil {
		t.Error("resume of an unknown session")
	}
	clk.Advance(DispatchBusy)
	if b := find(t, p.Sessions.Refresh().Sessions, IDBlog); b.State != "idle" {
		t.Errorf("resumed later = %s", b.State)
	}

	// stop and remove go by short id
	if err := p.Claude.Stop("dmindexr"); err != nil {
		t.Fatal(err)
	}
	idx := find(t, p.Sessions.Refresh().Sessions, IDIndexer)
	if idx.State != "stopped" || idx.Attention != store.AttnNone {
		t.Errorf("stopped = %+v", idx)
	}
	for _, b := range idx.Subs {
		if !b.Done {
			t.Errorf("stopped session's sub still running: %+v", b)
		}
	}
	if err := p.Claude.Remove("dmmigrat"); err != nil {
		t.Fatal(err)
	}
	if has(p.Sessions.Refresh().Sessions, IDMigration) {
		t.Error("removed session is still listed")
	}
	if err := p.Claude.Stop("nope"); err == nil {
		t.Error("stop of an unknown id")
	}
	if err := p.Claude.Remove("dmmigrat"); err == nil {
		t.Error("remove twice")
	}
	ags, _ = p.Claude.ListAgents()
	for _, a := range ags {
		if a.SessionID == IDIndexer && a.State != "stopped" {
			t.Errorf("stopped agent = %+v", a)
		}
		if a.SessionID == IDBlog && a.State != "idle" {
			t.Errorf("resumed agent = %+v", a)
		}
	}
}

func TestGroupsPort(t *testing.T) {
	s, _ := newScenario(t)
	g := s.Ports().Groups
	f := g.Load()
	ms := f.Groups[GroupTag]
	if len(ms) != 4 {
		t.Fatalf("deploy-rc = %+v", f.Groups)
	}
	for i, m := range ms {
		if m.SessionID != GroupMembers[i] || m.Name == "" {
			t.Errorf("member %d = %+v", i, m)
		}
	}
	ms[0].Name = "mexido" // Load hands out a copy
	if g.Load().Groups[GroupTag][0].Name == "mexido" {
		t.Error("Load shares its maps")
	}
	f.Add("outro", groups.Member{SessionID: IDDocs, Name: "site de docs"})
	if err := g.Save(f); err != nil {
		t.Fatal(err)
	}
	if len(g.Load().Groups) != 2 {
		t.Errorf("after save = %+v", g.Load().Groups)
	}
	for i := range maxQueued + 3 {
		if err := g.Queue(IDDocs, groups.Note{Tag: GroupTag, Text: "n" + time.Duration(i).String()}); err != nil {
			t.Fatal(err)
		}
	}
	if q := s.Queued(IDDocs); len(q) != maxQueued || q[len(q)-1].Text != "n"+time.Duration(maxQueued+2).String() {
		t.Errorf("queued = %d, last %+v", len(q), q[len(q)-1])
	}
}

func TestEventsPort(t *testing.T) {
	s, clk := newScenario(t)
	ev := s.Ports().Events
	if got := ev.Poll(); len(got) != 0 {
		t.Errorf("first poll = %+v", got)
	}
	clk.Advance(20 * time.Second) // the gateway blocks at 15 s, the first ✉ at 20 s
	got := ev.Poll()
	if len(got) != 2 {
		t.Fatalf("poll at 20 s = %+v", got)
	}
	if a := got[0]; a.Name != "Notification" || a.Type != "permission_prompt" || a.SessionID != IDGateway ||
		!a.NeedsYou() || !a.At.Equal(t0.Add(15*time.Second)) {
		t.Errorf("alert = %+v", a)
	}
	m := got[1]
	if m.Type != "peer" || m.Name != "UserPromptSubmit" || m.SessionID != IDMigration || m.From != "api-gateway" ||
		!strings.Contains(m.Message, GroupTag) || !m.At.Equal(t0.Add(20*time.Second)) {
		t.Errorf("peer = %+v", m)
	}
	if again := ev.Poll(); len(again) != 0 {
		t.Errorf("poll again = %+v", again)
	}
	clk.Advance(time.Hour) // a long jump keeps only the latest burst
	burst := ev.Poll()
	if len(burst) != maxBurst || !burst[len(burst)-1].At.Equal(clk.Now()) {
		t.Errorf("burst = %d events, last at %v", len(burst), burst[len(burst)-1].At)
	}
	clk.Set(t0) // back in time: nothing replays
	if got := ev.Poll(); len(got) != 0 {
		t.Errorf("after going back = %+v", got)
	}
}

func TestNotifyOpenJiraTodo(t *testing.T) {
	s, _ := newScenario(t)
	p := s.Ports()
	for i := range maxKept + 2 {
		_ = p.Notify.Notify("nk", "corpo"+time.Duration(i).String())
		_ = p.Open.OpenURL("https://example.invalid/" + time.Duration(i).String())
	}
	if n := s.Notes(); len(n) != maxKept || n[0].Body != "corpo2ns" {
		t.Errorf("notes = %d, first %+v", len(n), n[0])
	}
	if u := s.URLs(); len(u) != maxKept || !strings.HasSuffix(u[maxKept-1], "/"+time.Duration(maxKept+1).String()) {
		t.Errorf("urls = %d, last %v", len(u), u[maxKept-1])
	}

	c, err := p.Jira.Fetch(jira.Config{})
	if err != nil || len(c.Issues) != 4 || !c.FetchedAt.Equal(t0) || c.Issues[0].URL == "" {
		t.Fatalf("jira = %+v, %v", c, err)
	}
	key, cost, err := p.Jira.Create(jira.Config{}, jira.NewIssue{Summary: "nova"})
	if err != nil || key != "DEMO-201" || cost <= 0 {
		t.Errorf("create = %q %v %v", key, cost, err)
	}
	if c, _ := p.Jira.Fetch(jira.Config{}); len(c.Issues) != 5 || c.Issues[4].Key != "DEMO-201" || c.Issues[4].Summary != "nova" {
		t.Errorf("after create = %+v", c.Issues)
	}

	td := p.Todo
	if !td.Available() {
		t.Fatal("todo unavailable")
	}
	gs, err := td.List()
	if err != nil || len(gs) != 2 || len(gs[0].Tasks) != 3 {
		t.Fatalf("todos = %+v, %v", gs, err)
	}
	gs[0].Tasks[0].Title = "mexido"
	if g2, _ := td.List(); g2[0].Tasks[0].Title == "mexido" {
		t.Error("List shares its tasks")
	}
	if err := td.Start(4); err != nil {
		t.Fatal(err)
	}
	if err := td.AddNote(4, "começou"); err != nil {
		t.Fatal(err)
	}
	if err := td.LinkJira(4, "DEMO-104"); err != nil {
		t.Fatal(err)
	}
	if err := td.Done(1); err != nil {
		t.Fatal(err)
	}
	if err := td.AddFromJira("DEMO-103", "corrigir o alerta"); err != nil {
		t.Fatal(err)
	}
	gs, _ = td.List()
	t4 := gs[1].Tasks[0]
	if t4.ID != 4 || t4.Status != "doing" || len(t4.Notes) != 1 || t4.Notes[0].Text != "começou" || t4.JiraKey() != "DEMO-104" {
		t.Errorf("task 4 = %+v", t4)
	}
	if len(gs[0].Tasks) != 3 || gs[0].Tasks[0].ID == 1 {
		t.Errorf("hoje after done/add = %+v", gs[0].Tasks)
	}
	if last := gs[0].Tasks[2]; last.Title != "corrigir o alerta" || last.JiraKey() != "DEMO-103" || last.ID != 6 {
		t.Errorf("from jira = %+v", last)
	}
	for name, err := range map[string]error{"start": td.Start(99), "done": td.Done(99), "link": td.LinkJira(99, "X"),
		"note": td.AddNote(99, "x")} {
		if err == nil {
			t.Errorf("%s of an unknown task", name)
		}
	}
}

// The whole scenario through a real app: refresh, ✉, inbox, usage.
func TestThroughApp(t *testing.T) {
	t.Setenv("XDG_CACHE_HOME", t.TempDir())
	s, clk := newScenario(t)
	a := app.New(config.Default(), s.Ports())
	defer a.Close()
	if err := a.RegisterBuiltins(); err != nil {
		t.Fatal(err)
	}
	a.PollHookEvents()
	a.Refresh()
	if got := len(a.Snapshot().Sessions); got != 7 {
		t.Fatalf("app sees %d sessions", got)
	}
	if ms := a.Groups().Groups[GroupTag]; len(ms) != 4 {
		t.Errorf("app groups = %+v", a.Groups().Groups)
	}
	clk.Advance(MailEvery)
	a.OnHookEvents(a.PollHookEvents())
	a.Refresh()
	if c, ok := a.MailCueOf(IDMigration); !ok || c.Sent {
		t.Errorf("migration cue = %+v %v", c, ok)
	}
	if c, ok := a.MailCueOf(IDGateway); !ok || !c.Sent {
		t.Errorf("gateway cue = %+v %v", c, ok)
	}
	if msgs := a.Messages(); len(msgs) != 1 || msgs[0].From != IDGateway {
		t.Errorf("messages = %+v", msgs)
	}
	if rep, err := a.FetchUsage(); err != nil || rep.Limits[0].Pct != UsagePct(1) {
		t.Errorf("usage = %+v, %v", rep, err)
	}
	if find(t, a.Snapshot().Sessions, IDGateway).State != "blocked" {
		t.Error("gateway should be blocked at 20 s")
	}
	// a session command through the registry reaches the scenario
	if _, err := a.Run(app.Context{SessionID: IDDocs}, "session.rename", app.Args{"name": "docs novas"}); err != nil {
		t.Fatal(err)
	}
	a.Refresh()
	if d := find(t, a.Snapshot().Sessions, IDDocs); d.Label() != "docs novas" {
		t.Errorf("renamed = %q", d.Label())
	}
}
