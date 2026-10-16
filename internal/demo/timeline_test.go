package demo

import (
	"slices"
	"strings"
	"testing"
	"time"

	"github.com/aqilputi/nekuroddo/internal/store"
)

var t0 = time.Date(2026, 3, 1, 9, 0, 0, 0, time.UTC)

func byID(ss []store.Session) map[string]store.Session {
	m := map[string]store.Session{}
	for _, s := range ss {
		m[s.SessionID] = s
	}
	return m
}

func TestStep(t *testing.T) {
	for _, tc := range []struct {
		e    time.Duration
		want int
	}{{-time.Second, 0}, {0, 0}, {14 * time.Second, 0}, {15 * time.Second, 1}, {44 * time.Second, 2}, {time.Hour, 240}} {
		if got := Step(tc.e); got != tc.want {
			t.Errorf("Step(%v) = %d, want %d", tc.e, got, tc.want)
		}
	}
}

// The scripted timeline: what each session is at each moment.
func TestTimelineStates(t *testing.T) {
	type want struct {
		state string
		attn  store.Attention
	}
	for _, tc := range []struct {
		e    time.Duration
		want map[string]want
	}{
		{0, map[string]want{
			IDGateway: {"busy", store.AttnNone}, IDMigration: {"blocked", store.AttnBlocked},
			IDReport: {"done", store.AttnReturned}, IDDotfiles: {"idle", store.AttnIdle},
			IDBlog: {"history", store.AttnNone}, IDIndexer: {"busy", store.AttnNone}, IDDocs: {"busy", store.AttnNone}}},
		{15 * time.Second, map[string]want{
			IDGateway: {"blocked", store.AttnBlocked}, IDDocs: {"done", store.AttnReturned}}},
		{30 * time.Second, map[string]want{
			IDGateway: {"busy", store.AttnNone}, IDDocs: {"idle", store.AttnNone}}},
		{45 * time.Second, map[string]want{
			IDGateway: {"blocked", store.AttnBlocked}, IDDocs: {"busy", store.AttnNone}}},
		{10 * time.Minute, map[string]want{ // step 40
			IDGateway: {"busy", store.AttnNone}, IDDocs: {"done", store.AttnReturned},
			IDMigration: {"blocked", store.AttnBlocked}}},
	} {
		got := byID(Sessions(t0, tc.e))
		for id, w := range tc.want {
			s, ok := got[id]
			if !ok {
				t.Fatalf("e=%v: no session %s", tc.e, id)
			}
			if s.State != w.state || s.Attention != w.attn {
				t.Errorf("e=%v %s: %s/%d, want %s/%d", tc.e, s.Name, s.State, s.Attention, w.state, w.attn)
			}
		}
	}
}

func TestTimelineShape(t *testing.T) {
	ss := Sessions(t0, 0)
	if len(ss) != 7 {
		t.Fatalf("%d sessions, want 7", len(ss))
	}
	ctxs := map[string]int{}
	labels := map[string]bool{}
	for _, s := range ss {
		ctxs[s.Context]++
		if labels[s.Label()] {
			t.Errorf("duplicate label %q", s.Label())
		}
		labels[s.Label()] = true
		if s.SessionID == "" || s.Cwd == "" || s.Kind == "" || s.Started.IsZero() || s.LastAt.IsZero() {
			t.Errorf("incomplete session %+v", s)
		}
		if s.Kind == "background" && len(s.ShortID) != 8 {
			t.Errorf("%s: short id %q", s.Name, s.ShortID)
		}
		if s.Label() != s.Name {
			t.Errorf("label %q != name %q (the name must not look derived)", s.Label(), s.Name)
		}
	}
	if len(ctxs) != 3 || ctxs[CtxVisio] != 3 || ctxs[CtxPessoal] != 2 || ctxs[CtxOSS] != 2 {
		t.Errorf("contexts = %v", ctxs)
	}
	m := byID(ss)
	mig := m[IDMigration]
	if len(mig.Questions) != 1 || len(mig.Questions[0].Options) < 2 || mig.Needs == "" || mig.WaitingTool == "" {
		t.Errorf("migration question = %+v", mig)
	}
	gw := byID(Sessions(t0, 15*time.Second))[IDGateway]
	if len(gw.Questions) != 1 || len(gw.Questions[0].Options) != 3 {
		t.Errorf("blocked gateway question = %+v", gw.Questions)
	}
	if gw := m[IDGateway]; gw.Questions != nil || gw.Detail == "" {
		t.Errorf("busy gateway = %+v", gw)
	}
	idx := m[IDIndexer]
	running := 0
	for _, b := range idx.Subs {
		if !b.Done {
			running++
		}
	}
	if running != 2 || len(idx.Subs) != 3 {
		t.Errorf("indexer subs = %+v", idx.Subs)
	}
	if blog := m[IDBlog]; !blog.Stale || blog.ShortID != "" {
		t.Errorf("blog = %+v", blog)
	}
	if rep := m[IDReport]; rep.Result == "" || len(rep.Links) == 0 {
		t.Errorf("report = %+v", rep)
	}
	if dot := m[IDDotfiles]; dot.Kind != "interactive" || dot.WaitingFor == "" {
		t.Errorf("dotfiles = %+v", dot)
	}
}

func TestTimelineDeterministic(t *testing.T) {
	for _, e := range []time.Duration{0, 7 * time.Second, 16 * time.Second, 3 * time.Minute} {
		a, b := Sessions(t0, e), Sessions(t0, e)
		for i := range a {
			if a[i].SessionID != b[i].SessionID || a[i].State != b[i].State || a[i].Cost != b[i].Cost ||
				!a[i].LastAt.Equal(b[i].LastAt) {
				t.Errorf("e=%v: %+v != %+v", e, a[i], b[i])
			}
		}
	}
	// busy sessions cost more as time passes; LastAt never goes past now
	early, late := byID(Sessions(t0, 0)), byID(Sessions(t0, 5*time.Minute))
	if late[IDIndexer].Cost <= early[IDIndexer].Cost {
		t.Errorf("indexer cost %v → %v", early[IDIndexer].Cost, late[IDIndexer].Cost)
	}
	for _, s := range Sessions(t0, 5*time.Minute) {
		if s.LastAt.After(t0.Add(5 * time.Minute)) {
			t.Errorf("%s: LastAt in the future", s.Name)
		}
	}
}

func TestMailTimeline(t *testing.T) {
	for _, tc := range []struct {
		n        int
		from, to string
	}{
		{1, IDGateway, IDMigration}, {2, IDMigration, IDIndexer}, {3, IDIndexer, IDDocs}, {4, IDDocs, IDGateway},
		{5, IDGateway, IDMigration},
	} {
		m := MailN(tc.n)
		if m.From != tc.from || m.To != tc.to || m.At != time.Duration(tc.n)*MailEvery {
			t.Errorf("MailN(%d) = %+v", tc.n, m)
		}
		if !strings.HasPrefix(m.Text, "[#"+GroupTag+"]") || m.FromName == "" {
			t.Errorf("MailN(%d) text %q from %q", tc.n, m.Text, m.FromName)
		}
	}
	for _, tc := range []struct {
		a, b time.Duration
		want []int
	}{
		{0, 19 * time.Second, nil},
		{0, 20 * time.Second, []int{1}},
		{20 * time.Second, 40 * time.Second, []int{2}},
		{19 * time.Second, 61 * time.Second, []int{1, 2, 3}},
		{-time.Minute, 20 * time.Second, []int{1}}, // nothing before the start
		{time.Minute, time.Minute, nil},
		{time.Minute, 0, nil},
	} {
		var got []int
		for _, m := range MailsBetween(tc.a, tc.b) {
			got = append(got, m.N)
		}
		if !slices.Equal(got, tc.want) {
			t.Errorf("MailsBetween(%v, %v) = %v, want %v", tc.a, tc.b, got, tc.want)
		}
	}
}

func TestBlockedAlerts(t *testing.T) {
	for _, tc := range []struct {
		a, b time.Duration
		want int
	}{
		{0, 14 * time.Second, 0},
		{0, 15 * time.Second, 1},
		{15 * time.Second, 30 * time.Second, 0},
		{0, 46 * time.Second, 2},
	} {
		if got := len(AlertsBetween(tc.a, tc.b)); got != tc.want {
			t.Errorf("AlertsBetween(%v, %v) = %d, want %d", tc.a, tc.b, got, tc.want)
		}
	}
	al := AlertsBetween(0, 15*time.Second)[0]
	if al.At != 15*time.Second || al.SessionID != IDGateway || al.Message == "" {
		t.Errorf("alert = %+v", al)
	}
}

func TestUsagePct(t *testing.T) {
	for k := range 40 {
		if p := UsagePct(k); p < 57 || p > 63 {
			t.Errorf("UsagePct(%d) = %d", k, p)
		}
	}
	if UsagePct(0) != 60 {
		t.Errorf("UsagePct(0) = %d", UsagePct(0))
	}
	if UsagePct(-3) != 60 {
		t.Errorf("UsagePct(-3) = %d", UsagePct(-3))
	}
}
