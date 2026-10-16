package rpc

import (
	"io"
	"net/http"
	"strings"
	"testing"
	"time"

	"github.com/aqilputi/nekuroddo/internal/claude"
	"github.com/aqilputi/nekuroddo/internal/core"
	"github.com/aqilputi/nekuroddo/internal/events"
	"github.com/aqilputi/nekuroddo/internal/groups"
	"github.com/aqilputi/nekuroddo/internal/rpc/proto"
	"github.com/aqilputi/nekuroddo/internal/store"
)

// treeSessions: two contexts; "work" is the most recent; one stale, one
// archived, one with a running and a finished subagent.
func treeSessions() []store.Session {
	t0 := time.Date(2026, 1, 1, 12, 0, 0, 0, time.UTC)
	return []store.Session{
		{SessionID: "s-api", ShortID: "bgapi", Name: "api", Context: "work", Kind: "background", State: "blocked",
			Attention: store.AttnBlocked, Cost: 2, LastAt: t0.Add(3 * time.Minute),
			Questions: []claude.Question{{Question: "q?", Options: []claude.Option{{Label: "a"}, {Label: "b"}}}},
			Subs: []store.Sub{{ID: "sub-run", Description: "busca", Type: "Explore", Cost: 0.5},
				{ID: "sub-done", Type: "Plan", Done: true}}},
		{SessionID: "s-web", ShortID: "bgweb", Name: "web", Context: "work", Kind: "background", State: "busy",
			Cost: 1, LastAt: t0.Add(2 * time.Minute)},
		{SessionID: "s-blog", Name: "blog", Context: "home", Kind: "interactive", State: "idle",
			Attention: store.AttnIdle, Cost: 0.25, LastAt: t0.Add(time.Minute)},
		{SessionID: "s-old", Name: "velha", Context: "home", Kind: "history", State: "history", Stale: true,
			LastAt: t0},
		{SessionID: "s-arch", Name: "guardada", Context: "home", Kind: "history", State: "history", Stale: true,
			Archived: true, LastAt: t0},
	}
}

func treeEnv(t *testing.T) *env {
	t.Helper()
	e := newEnv(t, func(_ *Options, f *fakesT) {
		_ = f.Groups.Save(groups.File{Groups: map[string][]groups.Member{
			"deploy": {{SessionID: "s-api", Name: "api"}, {SessionID: "s-web", Name: "web"}}}})
	})
	e.f.Sessions.SetSnapshot(store.Snapshot{At: time.Now(), Sessions: treeSessions()})
	e.a.Refresh()
	return e
}

func keys(rows []proto.TreeRow) []string {
	var out []string
	for _, r := range rows {
		out = append(out, r.Key)
	}
	return out
}

func rowOf(t *testing.T, rows []proto.TreeRow, key string) proto.TreeRow {
	t.Helper()
	for _, r := range rows {
		if r.Key == key {
			return r
		}
	}
	t.Fatalf("no row %s in %v", key, keys(rows))
	return proto.TreeRow{}
}

func TestViewTreeFirstBuild(t *testing.T) {
	e := treeEnv(t)
	tr, err := e.c.ViewTree(ctxT(t), proto.ViewState{})
	if err != nil {
		t.Fatal(err)
	}
	want := "ctx:work sess:s-api sub:sub-run sess:s-web ctx:home sess:s-blog"
	if got := strings.Join(keys(tr.Rows), " "); got != want {
		t.Fatalf("rows = %s\nwant   %s", got, want)
	}
	ctx := rowOf(t, tr.Rows, "ctx:work")
	if ctx.Kind != core.RowCtx || ctx.Depth != 0 || ctx.CtxN != 2 || ctx.CtxCost != 3 || ctx.Color != 0 || ctx.SessionID != "" {
		t.Errorf("ctx row = %+v", ctx)
	}
	if home := rowOf(t, tr.Rows, "ctx:home"); home.Color != 1 || home.CtxN != 1 {
		t.Errorf("home row = %+v", home)
	}
	api := rowOf(t, tr.Rows, "sess:s-api")
	if api.Kind != core.RowSess || api.Depth != 1 || api.SessionID != "s-api" || api.ShortID != "bgapi" ||
		api.Label != "api" || api.State != "blocked" || api.Attention != int(store.AttnBlocked) ||
		api.SessionKind != "background" || api.Cost != 2 || api.SubsRunning != 1 || api.Options != 2 || api.Color != 0 {
		t.Errorf("api row = %+v", api)
	}
	if len(api.Groups) != 1 || api.Groups[0].Tag != "deploy" || api.Groups[0].Color != core.GroupColorIndex("deploy") {
		t.Errorf("api groups = %+v", api.Groups)
	}
	sub := rowOf(t, tr.Rows, "sub:sub-run")
	if sub.Kind != core.RowSub || sub.Depth != 2 || sub.SessionID != "s-api" || sub.Sub == nil ||
		sub.Sub.Description != "busca" || sub.Sub.Type != "Explore" || sub.Sub.Cost != 0.5 || sub.Label != "busca" ||
		sub.State != "busy" || sub.Cost != 0.5 || sub.ShortID != "bgapi" {
		t.Errorf("sub row = %+v (sub %+v)", sub, sub.Sub)
	}
	if blog := rowOf(t, tr.Rows, "sess:s-blog"); len(blog.Groups) != 0 || blog.Mail != nil {
		t.Errorf("blog row = %+v", blog)
	}
	if strings.Join(tr.Inbox, ",") != "s-api,s-blog" {
		t.Errorf("inbox = %v", tr.Inbox)
	}
	if tr.Hidden != 2 || tr.Archived != 1 {
		t.Errorf("hidden %d archived %d", tr.Hidden, tr.Archived)
	}
	st := tr.State
	if st.Tab != "sessions" || st.Sort != "atividade" || st.CursorKey != "ctx:work" || st.Cursor != 0 ||
		len(st.Order) != 5 || st.CtxColor["work"] != 0 || st.CtxColor["home"] != 1 {
		t.Errorf("state = %+v", st)
	}
}

func TestViewTreeStateRoundTrip(t *testing.T) {
	e := treeEnv(t)
	tr, err := e.c.ViewTree(ctxT(t), proto.ViewState{})
	if err != nil {
		t.Fatal(err)
	}
	st := tr.State
	st.Cursor, st.CursorKey = 3, "sess:s-web"
	// home becomes the most recent: the frozen order keeps it where it was
	ss := treeSessions()
	ss[2].LastAt = ss[2].LastAt.Add(time.Hour)
	ss = append(ss, store.Session{SessionID: "s-new", Name: "nova", Context: "work", Kind: "background",
		State: "busy", LastAt: ss[2].LastAt.Add(-30 * time.Minute)})
	e.f.Sessions.SetSnapshot(store.Snapshot{Sessions: ss})
	e.a.Refresh()
	tr2, err := e.c.ViewTree(ctxT(t), st)
	if err != nil {
		t.Fatal(err)
	}
	want := "ctx:work sess:s-new sess:s-api sub:sub-run sess:s-web ctx:home sess:s-blog"
	if got := strings.Join(keys(tr2.Rows), " "); got != want {
		t.Fatalf("rows = %s\nwant   %s", got, want)
	}
	// the cursor followed s-web
	if tr2.State.CursorKey != "sess:s-web" || tr2.State.Cursor != 4 {
		t.Errorf("cursor = %d %s", tr2.State.Cursor, tr2.State.CursorKey)
	}
	// reset_order re-sorts: home is now first
	st = tr2.State
	st.ResetOrder = true
	tr3, err := e.c.ViewTree(ctxT(t), st)
	if err != nil {
		t.Fatal(err)
	}
	if tr3.Rows[0].Key != "ctx:home" || tr3.State.ResetOrder {
		t.Errorf("after reset: %v (reset echoed: %v)", keys(tr3.Rows), tr3.State.ResetOrder)
	}
}

func TestViewTreeOptions(t *testing.T) {
	e := treeEnv(t)
	for _, tc := range []struct {
		name string
		st   proto.ViewState
		want string
	}{
		{"collapsed ctx", proto.ViewState{Collapsed: map[string]bool{"ctx:work": true}}, "ctx:work ctx:home sess:s-blog"},
		{"collapsed sess", proto.ViewState{Collapsed: map[string]bool{"sess:s-api": true}},
			"ctx:work sess:s-api sess:s-web ctx:home sess:s-blog"},
		{"show_all", proto.ViewState{ShowAll: true},
			"ctx:work sess:s-api sub:sub-run sub:sub-done sess:s-web ctx:home sess:s-blog sess:s-old sess:s-arch"},
		{"filter", proto.ViewState{Filter: "blog"}, "ctx:home sess:s-blog"},
		{"archive tab", proto.ViewState{Tab: "archive"}, "ctx:home sess:s-arch"},
		{"sort by name", proto.ViewState{Sort: "nome"}, "ctx:home sess:s-blog ctx:work sess:s-api sub:sub-run sess:s-web"},
	} {
		tr, err := e.c.ViewTree(ctxT(t), tc.st)
		if err != nil {
			t.Fatalf("%s: %v", tc.name, err)
		}
		if got := strings.Join(keys(tr.Rows), " "); got != tc.want {
			t.Errorf("%s: rows = %s\nwant %s", tc.name, got, tc.want)
		}
	}
	tr, _ := e.c.ViewTree(ctxT(t), proto.ViewState{Collapsed: map[string]bool{"ctx:work": true}})
	if !rowOf(t, tr.Rows, "ctx:work").Collapsed || rowOf(t, tr.Rows, "ctx:home").Collapsed {
		t.Errorf("collapsed flags: %+v", tr.Rows)
	}
	tr, _ = e.c.ViewTree(ctxT(t), proto.ViewState{ShowAll: true})
	if r := rowOf(t, tr.Rows, "sess:s-arch"); !r.Archived || !r.Stale {
		t.Errorf("archived row = %+v", r)
	}
	if r := rowOf(t, tr.Rows, "sub:sub-done"); r.Sub == nil || !r.Sub.Done || r.Label != "Plan" || r.State != "done" {
		t.Errorf("done sub row = %+v", r)
	}
}

func TestViewTreeMailCue(t *testing.T) {
	e := treeEnv(t)
	e.a.OnHookEvents([]events.Event{{Name: "UserPromptSubmit", Type: "peer", SessionID: "s-web", From: "api",
		Message: "oi", At: e.f.Clock.Now()}})
	tr, err := e.c.ViewTree(ctxT(t), proto.ViewState{})
	if err != nil {
		t.Fatal(err)
	}
	web, api := rowOf(t, tr.Rows, "sess:s-web"), rowOf(t, tr.Rows, "sess:s-api")
	if web.Mail == nil || web.Mail.Sent || api.Mail == nil || !api.Mail.Sent || web.MailState != proto.MailBlink {
		t.Errorf("mail: web %+v %q api %+v", web.Mail, web.MailState, api.Mail)
	}
	e.f.Clock.Advance(2 * time.Minute) // past MailCueFor: dim
	tr, _ = e.c.ViewTree(ctxT(t), proto.ViewState{})
	if web := rowOf(t, tr.Rows, "sess:s-web"); web.Mail == nil || web.MailState != proto.MailDim {
		t.Errorf("after 2 min: %+v %q", web.Mail, web.MailState)
	}
	e.f.Clock.Advance(20 * time.Minute) // past MailKeep: gone
	tr, _ = e.c.ViewTree(ctxT(t), proto.ViewState{})
	if web := rowOf(t, tr.Rows, "sess:s-web"); web.Mail != nil || web.MailState != "" {
		t.Errorf("after 22 min: %+v %q", web.Mail, web.MailState)
	}
	if sub := rowOf(t, tr.Rows, "sub:sub-run"); sub.Mail != nil || sub.Groups != nil {
		t.Errorf("sub rows carry no cue/groups: %+v", sub)
	}
}

func TestViewTreeEmptyAndDefaultSort(t *testing.T) {
	e := newEnv(t)
	e.f.Sessions.SetSnapshot(store.Snapshot{})
	e.a.Refresh()
	hc := e.rawUnix()
	resp, err := hc.Post("http://nk/v1/view/tree", "application/json", strings.NewReader(""))
	if err != nil {
		t.Fatal(err)
	}
	defer resp.Body.Close()
	body := readAll(t, resp)
	if resp.StatusCode != http.StatusOK || !strings.Contains(body, `"rows":[]`) || !strings.Contains(body, `"inbox":[]`) {
		t.Errorf("empty tree = %d %s", resp.StatusCode, body)
	}
}

func TestViewTreeBadRequests(t *testing.T) {
	e := treeEnv(t)
	for _, st := range []proto.ViewState{{Tab: "nope"}, {Sort: "aleatório"}} {
		if _, err := e.c.ViewTree(ctxT(t), st); !IsCode(err, proto.CodeBadRequest) {
			t.Errorf("%+v: err = %v", st, err)
		}
	}
	resp, err := e.rawUnix().Post("http://nk/v1/view/tree", "application/json", strings.NewReader("{"))
	if err != nil {
		t.Fatal(err)
	}
	resp.Body.Close()
	if resp.StatusCode != http.StatusBadRequest {
		t.Errorf("bad json = %d", resp.StatusCode)
	}
}

func readAll(t *testing.T, resp *http.Response) string {
	t.Helper()
	b, err := io.ReadAll(resp.Body)
	if err != nil {
		t.Fatal(err)
	}
	return string(b)
}
