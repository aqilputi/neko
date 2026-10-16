package app_test

import (
	"errors"
	"strings"
	"testing"
	"time"

	"github.com/aqilputi/nekuroddo/internal/app"
	"github.com/aqilputi/nekuroddo/internal/app/apptest"
	"github.com/aqilputi/nekuroddo/internal/config"
	"github.com/aqilputi/nekuroddo/internal/core"
	"github.com/aqilputi/nekuroddo/internal/events"
	"github.com/aqilputi/nekuroddo/internal/groups"
	"github.com/aqilputi/nekuroddo/internal/store"
)

var errGrpBoom = errors.New("boom")

func grpSessions() []store.Session {
	return []store.Session{
		{SessionID: "a", Name: "deploy rc 1", Cwd: "/a", State: "busy"},
		{SessionID: "b", Name: "deploy rc 2", Cwd: "/b", State: "busy"},
		{SessionID: "c", Name: "outra coisa", Cwd: "/c"},
	}
}

// grpApp is an app whose snapshot (taken by a real Refresh) holds grpSessions.
func grpApp(t *testing.T) (*app.App, *apptest.Fakes) {
	t.Helper()
	a, f := newApp(t)
	f.Sessions.SetSnapshot(store.Snapshot{Sessions: grpSessions()})
	a.Refresh()
	return a, f
}

func grpSeed(t *testing.T, f *apptest.Fakes, tag string, ids ...string) {
	t.Helper()
	g := f.Groups.Load()
	for _, id := range ids {
		g.Add(tag, groups.Member{SessionID: id, Name: "n-" + id})
	}
	if err := f.Groups.Save(g); err != nil {
		t.Fatal(err)
	}
}

// grpDrain collects every event already published.
func grpDrain(ch <-chan core.Event) []core.Event {
	var out []core.Event
	for {
		select {
		case e := <-ch:
			out = append(out, e)
		case <-time.After(50 * time.Millisecond):
			return out
		}
	}
}

func grpNamed(evs []core.Event, name string) []core.Event {
	var out []core.Event
	for _, e := range evs {
		if e.Name == name {
			out = append(out, e)
		}
	}
	return out
}

func TestGroupTargetsAddsPickedAndPublishes(t *testing.T) {
	a, f := grpApp(t)
	ev := a.Subscribe(t.Context())
	res, err := a.GroupTargets(app.Context{SessionID: "c", Picked: []string{"a", "b"}}, "#Deploy RC")
	if err != nil || res.Err {
		t.Fatal(res, err)
	}
	if !strings.Contains(res.Toast, "#deploy-rc: 2 sessões") {
		t.Fatalf("toast %q", res.Toast)
	}
	got := f.Groups.Load().Groups["deploy-rc"]
	if len(got) != 2 || got[0].Name != "deploy rc 1" || got[0].Cwd != "/a" || got[1].SessionID != "b" {
		t.Fatalf("members: %+v", got)
	}
	if g := a.Groups().Groups["deploy-rc"]; len(g) != 2 {
		t.Fatal("app state follows the store")
	}
	es := grpNamed(grpDrain(ev), app.EvGroupChanged)
	if len(es) != 1 || es[0].Data.(app.GroupChanged).Tag != "deploy-rc" {
		t.Fatalf("group.changed: %+v", es)
	}
}

func TestGroupTargetsFromSnapshot(t *testing.T) {
	a, f := newApp(t)
	f.Sessions.SetSnapshot(store.Snapshot{Sessions: append(grpSessions(), store.Session{SessionID: "d", CustomName: "meu"})})
	a.Refresh()
	// the app's snapshot is used; unknown ids are skipped
	if _, err := a.GroupTargets(app.Context{Picked: []string{"c", "zz"}}, "x"); err != nil {
		t.Fatal(err)
	}
	if got := f.Groups.Load().Groups["x"]; len(got) != 1 || got[0].SessionID != "c" || got[0].Name != "outra coisa" {
		t.Fatalf("%+v", got)
	}
	// a session with no Claude name is kept under its label
	if _, err := a.GroupTargets(app.Context{SessionID: "d"}, "y"); err != nil {
		t.Fatal(err)
	}
	if got := f.Groups.Load().Groups["y"]; len(got) != 1 || got[0].Name != "meu" {
		t.Fatalf("%+v", got)
	}
}

func TestGroupTargetsErrors(t *testing.T) {
	a, f := grpApp(t)
	for _, c := range []struct {
		ctx app.Context
		tag string
	}{{app.Context{}, "x"}, {app.Context{SessionID: "a"}, " # "}, {app.Context{SessionID: "zz"}, "x"}} {
		res, err := a.GroupTargets(c.ctx, c.tag)
		if !errors.Is(err, app.ErrNoTargets) || !res.Err || !strings.HasPrefix(res.Toast, "group: escolha") {
			t.Fatalf("%+v: %+v %v", c, res, err)
		}
	}
	f.Groups.Errs = map[string]error{"Save": errGrpBoom}
	res, err := a.GroupTargets(app.Context{SessionID: "a"}, "x")
	if !errors.Is(err, errGrpBoom) || res.Toast != "group: boom" || !res.Err {
		t.Fatal(res, err)
	}
}

func TestUngroupTargets(t *testing.T) {
	a, f := newApp(t)
	grpSeed(t, f, "t", "a", "b")
	grpSeed(t, f, "u", "a")
	ev := a.Subscribe(t.Context())
	res, err := a.UngroupTargets(app.Context{SessionID: "a"}, "")
	if err != nil || res.Err || res.Toast != "2 saíram do grupo" {
		t.Fatal(res, err)
	}
	g := f.Groups.Load()
	if len(g.Of("a")) != 0 || len(g.Groups["t"]) != 1 || g.Groups["u"] != nil {
		t.Fatalf("%+v", g)
	}
	if len(grpNamed(grpDrain(ev), app.EvGroupChanged)) != 1 {
		t.Fatal("group.changed")
	}
	res, err = a.UngroupTargets(app.Context{SessionID: "a"}, "#T")
	if err != nil || !res.Err || res.Toast != "ungroup: nada a tirar" {
		t.Fatal(res, err)
	}
	f.Groups.Errs = map[string]error{"Save": errGrpBoom}
	res, err = a.UngroupTargets(app.Context{SessionID: "b"}, "t")
	if !errors.Is(err, errGrpBoom) || res.Toast != "ungroup: boom" {
		t.Fatal(res, err)
	}
}

func TestDissolveGroup(t *testing.T) {
	a, f := newApp(t)
	grpSeed(t, f, "t", "a", "b")
	ev := a.Subscribe(t.Context())
	res, err := a.DissolveGroup("#t")
	if err != nil || res.Toast != "#t desfeito" || f.Groups.Load().Groups["t"] != nil {
		t.Fatal(res, err)
	}
	if len(grpNamed(grpDrain(ev), app.EvGroupChanged)) != 1 {
		t.Fatal("group.changed")
	}
	if _, err := a.DissolveGroup(""); !errors.Is(err, app.ErrNoGroup) {
		t.Fatal(err)
	}
	f.Groups.Errs = map[string]error{"Save": errGrpBoom}
	if res, err := a.DissolveGroup("t"); !errors.Is(err, errGrpBoom) || !res.Err || res.Toast != "boom" {
		t.Fatal(res, err)
	}
}

func TestNotifyGroupQueuesAndCountsStopped(t *testing.T) {
	a, f := grpApp(t)
	grpSeed(t, f, "deploy-rc", "a", "c", "gone")
	ev := a.Subscribe(t.Context())
	res, err := a.NotifyGroup("deploy-rc", "o bucket mudou")
	if err != nil || res.Err {
		t.Fatal(res, err)
	}
	if res.Toast != "✉ #deploy-rc: aviso na fila de 3 sessões (2 paradas recebem quando voltarem a rodar)" {
		t.Fatalf("toast %q", res.Toast)
	}
	if d, ok := res.Data.(app.NotifyResult); !ok || d.Members != 3 || d.Stopped != 2 || d.Tag != "deploy-rc" {
		t.Fatalf("data %+v", res.Data)
	}
	for _, id := range []string{"a", "c", "gone"} {
		q := f.Groups.Queued(id)
		if len(q) != 1 || q[0].Text != "o bucket mudou" || q[0].Tag != "deploy-rc" || !q[0].At.Equal(apptest.Epoch) {
			t.Fatalf("%s: %+v", id, q)
		}
	}
	mail := a.Mail()
	if c, ok := mail["a"]; !ok || c.Sent || !c.At.Equal(apptest.Epoch) {
		t.Fatalf("mail %+v", mail)
	}
	msgs := a.Messages()
	if len(msgs) != 3 || msgs[0].FromName != "você" || msgs[0].Text != "[#deploy-rc] o bucket mudou" || msgs[0].To != "a" {
		t.Fatalf("msgs %+v", msgs)
	}
	evs := grpDrain(ev)
	if len(grpNamed(evs, app.EvMailCue)) != 3 || len(grpNamed(evs, app.EvPeerMessage)) != 3 {
		t.Fatalf("events %+v", evs)
	}
	// all running: no "paradas"
	grpSeed(t, f, "run", "a", "b")
	if res, _ := a.NotifyGroup("#run", "x"); strings.Contains(res.Toast, "paradas") || res.Err {
		t.Fatal(res.Toast)
	}
}

func TestNotifyGroupErrors(t *testing.T) {
	a, f := newApp(t)
	res, err := a.NotifyGroup("nope", "x")
	if !errors.Is(err, app.ErrNoGroup) || !res.Err || res.Toast != "notify: #nope não existe" {
		t.Fatal(res, err)
	}
	grpSeed(t, f, "t", "a")
	f.Groups.Errs = map[string]error{"Queue": errGrpBoom}
	res, err = a.NotifyGroup("t", "x")
	if !errors.Is(err, errGrpBoom) || res.Toast != "notify: boom" {
		t.Fatal(res, err)
	}
}

func TestOnPeerResolvesSender(t *testing.T) {
	a, _ := grpApp(t)
	ev := a.Subscribe(t.Context())
	at := apptest.Epoch.Add(-time.Minute)
	seen := a.OnPeer(events.Event{At: at, SessionID: "b", Type: "peer", From: "deploy rc…", Message: "oi"})
	if seen.FromLabel != "deploy rc 1" || seen.ToLabel != "deploy rc 2" || seen.Msg.From != "a" || seen.Msg.Text != "oi" {
		t.Fatalf("%+v", seen)
	}
	mail := a.Mail()
	if c := mail["b"]; c.Sent || !c.At.Equal(at) {
		t.Fatalf("receiver %+v", c)
	}
	if c := mail["a"]; !c.Sent {
		t.Fatalf("sender %+v", c)
	}
	evs := grpDrain(ev)
	pm := grpNamed(evs, app.EvPeerMessage)
	if len(pm) != 1 || pm[0].Data.(app.PeerMessage).FromID != "a" || len(grpNamed(evs, app.EvMailCue)) != 2 {
		t.Fatalf("%+v", evs)
	}
	// unknown sender and receiver; zero time uses the clock
	seen = a.OnPeer(events.Event{SessionID: "zz", From: "ninguém", Message: "x"})
	if seen.FromLabel != "ninguém" || seen.ToLabel != "?" || seen.Msg.From != "" || !seen.Msg.At.Equal(apptest.Epoch) {
		t.Fatalf("%+v", seen)
	}
}

func TestSessionNamed(t *testing.T) {
	ss := grpSessions()
	ss[2].CustomName = "meu nome"
	for _, c := range []struct{ in, want string }{
		{"deploy rc 2", "b"}, {"meu nome", "c"}, {"outra…", "c"}, {"…", ""}, {"", ""}, {"nada", ""},
	} {
		got := ""
		if s := app.SessionNamed(ss, c.in); s != nil {
			got = s.SessionID
		}
		if got != c.want {
			t.Errorf("SessionNamed(%q)=%q want %q", c.in, got, c.want)
		}
	}
}

func TestMailCueByClock(t *testing.T) {
	a, f := grpApp(t)
	a.OnPeer(events.Event{SessionID: "b", From: "x"})
	now := func() time.Time { return a.Clock().Now() }
	c := a.Mail()["b"]
	if c.State(now()) != app.CueBlink || !a.MailBlinking() {
		t.Fatal("fresh cue blinks")
	}
	f.Clock.Advance(app.MailCueFor + time.Second)
	if c.State(now()) != app.CueDim || a.MailBlinking() {
		t.Fatal("after 90 s it is dim")
	}
	f.Clock.Advance(app.MailKeep)
	if c.State(now()) != app.CueNone {
		t.Fatal("after 15 min it is gone")
	}
	if (app.MailCue{}).State(now()) != app.CueNone {
		t.Fatal("zero cue is none")
	}
}

func TestMessagesKeepLast100(t *testing.T) {
	a, _ := grpApp(t)
	for i := 0; i < 105; i++ {
		a.OnPeer(events.Event{SessionID: "b", From: "x", Message: string(rune('a' + i%26))})
	}
	if n := len(a.Messages()); n != 100 {
		t.Fatal(n)
	}
}

func TestSyncGroupNamesSavesOnlyOnChange(t *testing.T) {
	a, f := newApp(t)
	g := f.Groups.Load()
	g.Add("t", groups.Member{SessionID: "a", Name: "velho"}, groups.Member{SessionID: "gone", Name: "x"})
	if err := f.Groups.Save(g); err != nil {
		t.Fatal(err)
	}
	f.Groups.Reset()
	ev := a.Subscribe(t.Context())
	changed, err := a.SyncGroupNames(grpSessions())
	if !changed || err != nil {
		t.Fatal(changed, err)
	}
	got := f.Groups.Load().Groups["t"]
	if len(got) != 2 || got[0].Name != "deploy rc 1" || got[0].Cwd != "/a" || got[1].Name != "x" {
		t.Fatalf("%+v", got)
	}
	if len(grpNamed(grpDrain(ev), app.EvGroupChanged)) != 1 {
		t.Fatal("group.changed on a rename")
	}
	f.Groups.Reset()
	if changed, _ := a.SyncGroupNames(grpSessions()); changed || f.Groups.Count("Save") != 0 {
		t.Fatal("nothing changed: no save")
	}
	// the store changed behind our back: the app reloads it
	if err := f.Groups.Save(groups.File{Groups: map[string][]groups.Member{}}); err != nil {
		t.Fatal(err)
	}
	if _, _ = a.SyncGroupNames(nil); len(a.Groups().Groups) != 0 {
		t.Fatal("reload")
	}
	grpSeed(t, f, "t", "a")
	f.Groups.Errs = map[string]error{"Save": errGrpBoom}
	if changed, err := a.SyncGroupNames(grpSessions()); !changed || !errors.Is(err, errGrpBoom) {
		t.Fatal(changed, err)
	}
}

func TestPeers(t *testing.T) {
	a, f := newApp(t)
	grpSeed(t, f, "t", "a", "b")
	grpSeed(t, f, "u", "a", "c")
	grpSeed(t, f, "v", "d")
	if _, err := a.SyncGroupNames(nil); err != nil {
		t.Fatal(err)
	}
	if got := a.Peers("a"); strings.Join(got, ",") != "a,b,c" {
		t.Fatalf("peers of a: %v", got)
	}
	if got := a.Peers("b"); strings.Join(got, ",") != "a,b" {
		t.Fatal(got)
	}
	if a.Peers("zz") != nil || a.Peers("") != nil {
		t.Fatal("no group, no peers")
	}
	g := a.Groups()
	g.Add("w", groups.Member{SessionID: "x"})
	if len(a.Groups().Groups["w"]) != 0 {
		t.Fatal("Groups returns a copy")
	}
}

func TestGroupCommandsRegistered(t *testing.T) {
	a, f := newApp(t)
	if err := a.RegisterGroupCommands(); err != nil {
		t.Fatal(err)
	}
	if err := a.RegisterGroupCommands(); !errors.Is(err, app.ErrDuplicateCommand) {
		t.Fatalf("registering twice: %v", err)
	}
	for _, n := range []string{"group.add", "group.remove", "group.notify", "group.dissolve"} {
		if _, ok := a.Registry().Lookup(n); !ok {
			t.Fatalf("%s not registered", n)
		}
	}
	f.Sessions.SetSnapshot(store.Snapshot{Sessions: grpSessions()})
	a.Refresh()
	ctx := app.Context{SessionID: "a"}
	if _, err := a.Run(ctx, "group.add", app.Args{"tag": "t"}); err != nil {
		t.Fatal(err)
	}
	if len(f.Groups.Load().Groups["t"]) != 1 {
		t.Fatal("group.add")
	}
	// notify without a tag uses the cursor's first group
	res, err := a.Run(ctx, "group.notify", app.Args{"text": "oi"})
	if err != nil || len(f.Groups.Queued("a")) != 1 {
		t.Fatal(res, err)
	}
	if _, err := a.Run(app.Context{SessionID: "c"}, "group.notify", app.Args{"text": "oi"}); !errors.Is(err, app.ErrNoGroup) {
		t.Fatal(err)
	}
	if _, err := a.Run(ctx, "group.notify", app.Args{}); !errors.Is(err, app.ErrMissingArg) {
		t.Fatal(err)
	}
	res, err = a.Run(ctx, "group.dissolve", app.Args{"tag": "t"})
	if err != nil || res.Pending == nil || !strings.Contains(res.Pending.Question, "#t") {
		t.Fatal(res, err)
	}
	if _, err := a.Confirm(res.Pending.Token); err != nil || f.Groups.Load().Groups["t"] != nil {
		t.Fatal(err)
	}
	if _, err := a.Run(ctx, "group.add", app.Args{"tag": "t"}); err != nil {
		t.Fatal(err)
	}
	if res, err := a.Run(ctx, "group.remove", app.Args{}); err != nil || res.Toast != "1 saíram do grupo" {
		t.Fatal(res, err)
	}
}

func TestGroupsNoLazyRegistration(t *testing.T) {
	a, _ := newApp(t)
	_ = a.Groups() // using the slice registers nothing: RegisterBuiltins does
	if len(a.Registry().List(app.Context{})) != 0 {
		t.Fatal("the registry stays empty until asked")
	}
}

func TestReloadGroupsFollowsTheStore(t *testing.T) {
	a, f := newApp(t)
	grpSeed(t, f, "t", "a")
	if len(a.Groups().Groups["t"]) != 0 {
		t.Fatal("the core keeps what it read until it reloads")
	}
	if got := a.ReloadGroups(); len(got.Groups["t"]) != 1 || len(a.Groups().Groups["t"]) != 1 {
		t.Fatalf("reloaded: %+v", got)
	}
	if tags := a.TagsOf("a"); len(tags) != 1 || tags[0] != "t" || a.TagsOf("zz") != nil {
		t.Fatalf("TagsOf: %v", tags)
	}
	if _, ok := a.MailCueOf("a"); ok {
		t.Fatal("no cue yet")
	}
	a.OnPeer(events.Event{SessionID: "a", From: "x"})
	if c, ok := a.MailCueOf("a"); !ok || c.Sent {
		t.Fatalf("cue: %+v", c)
	}
}

func TestNewWithoutGroupStore(t *testing.T) {
	p := apptest.Ports()
	p.Groups = nil
	a := app.New(config.Default(), p)
	defer a.Close()
	if len(a.Groups().Groups) != 0 {
		t.Fatal("no store: no groups")
	}
	a.Refresh() // no SyncGroupNames without a store
}
