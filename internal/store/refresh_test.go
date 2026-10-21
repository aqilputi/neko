package store

import (
	"encoding/json"
	"fmt"
	"math"
	"os"
	"path/filepath"
	"testing"
	"time"

	"github.com/aqilputi/nekuroddo/internal/claude"
	"github.com/aqilputi/nekuroddo/internal/config"
)

// world is a fake machine: HOME, ~/.claude and the XDG dirs all live in a
// temp dir, and `claude` is a script that prints agents.json.
type world struct {
	t      *testing.T
	home   string
	claude string // CLAUDE_CONFIG_DIR
	proj   string // the one project folder transcripts go in
	cwd    string // where the sessions run
}

func newWorld(t *testing.T) *world {
	t.Helper()
	home := t.TempDir()
	w := &world{t: t, home: home, claude: filepath.Join(home, ".claude")}
	w.cwd = filepath.Join(home, "dev", "neko")
	w.proj = filepath.Join(w.claude, "projects", "-home-dev-neko")
	for _, d := range []string{w.proj, filepath.Join(w.claude, "jobs"), filepath.Join(w.claude, "sessions")} {
		if err := os.MkdirAll(d, 0o755); err != nil {
			t.Fatal(err)
		}
	}
	t.Setenv("HOME", home)
	t.Setenv("CLAUDE_CONFIG_DIR", w.claude)
	t.Setenv("XDG_STATE_HOME", filepath.Join(home, ".local", "state"))
	t.Setenv("XDG_CONFIG_HOME", filepath.Join(home, ".config"))
	t.Setenv("XDG_CACHE_HOME", filepath.Join(home, ".cache"))
	bin := filepath.Join(home, "fake-claude")
	script := "#!/bin/sh\ncat '" + filepath.Join(home, "agents.json") + "'\n"
	if err := os.WriteFile(bin, []byte(script), 0o755); err != nil {
		t.Fatal(err)
	}
	t.Setenv("NEKO_CLAUDE", bin)
	w.agents()
	return w
}

// agents sets what `claude agents --json` reports.
func (w *world) agents(as ...claude.Agent) {
	w.t.Helper()
	if as == nil {
		as = []claude.Agent{}
	}
	b, _ := json.Marshal(as)
	w.write(filepath.Join(w.home, "agents.json"), string(b))
}

func (w *world) write(p, s string) {
	w.t.Helper()
	if err := os.MkdirAll(filepath.Dir(p), 0o755); err != nil {
		w.t.Fatal(err)
	}
	if err := os.WriteFile(p, []byte(s), 0o644); err != nil {
		w.t.Fatal(err)
	}
}

func (w *world) writeJSON(p string, v any) {
	w.t.Helper()
	b, err := json.Marshal(v)
	if err != nil {
		w.t.Fatal(err)
	}
	w.write(p, string(b))
}

// transcript writes <proj>/<sid>.jsonl from records.
func (w *world) transcript(sid string, recs ...string) string {
	p := filepath.Join(w.proj, sid+".jsonl")
	w.write(p, joinLines(recs))
	return p
}

func joinLines(recs []string) string {
	s := ""
	for _, r := range recs {
		s += r + "\n"
	}
	return s
}

func ts(t time.Time) string { return t.UTC().Format(time.RFC3339Nano) }

func userRec(cwd string, at time.Time, text string) string {
	b, _ := json.Marshal(map[string]any{
		"type": "user", "timestamp": ts(at), "cwd": cwd,
		"message": map[string]any{"role": "user", "content": text},
	})
	return string(b)
}

// asstRec is one API request of `in` input and `out` output tokens.
func asstRec(at time.Time, req, model string, in, out int64, text string) string {
	b, _ := json.Marshal(map[string]any{
		"type": "assistant", "timestamp": ts(at), "requestId": req,
		"message": map[string]any{
			"id": "m-" + req, "role": "assistant", "model": model,
			"usage":   map[string]any{"input_tokens": in, "output_tokens": out},
			"content": []map[string]any{{"type": "text", "text": text}},
		},
	})
	return string(b)
}

func titleRec(title string) string {
	return fmt.Sprintf(`{"type":"ai-title","aiTitle":%q}`, title)
}

func testCfg() config.Config {
	cfg := config.Default()
	cfg.ShowHistoryDays = 0
	return cfg
}

func byID(snap Snapshot) map[string]Session {
	out := map[string]Session{}
	for _, s := range snap.Sessions {
		out[s.SessionID] = s
	}
	return out
}

func near(a, b float64) bool { return math.Abs(a-b) < 1e-9 }

func TestNormState(t *testing.T) {
	cases := map[string]string{
		"busy": "busy", "shell": "busy", "running": "busy", "working": "busy", "active": "busy", "starting": "busy",
		"blocked": "blocked", "waiting": "blocked", "needs_input": "blocked", "permission": "blocked",
		"done": "done", "completed": "done", "finished": "done", "exited": "done",
		"stopped": "stopped", "killed": "stopped",
		"failed": "failed", "crashed": "failed",
		"idle": "idle", "ready": "idle", "": "idle",
		"something-new": "something-new", // unknown states pass through
	}
	for in, want := range cases {
		if got := normState(in); got != want {
			t.Errorf("normState(%q) = %q, want %q", in, got, want)
		}
	}
}

func TestRefreshListAgentsError(t *testing.T) {
	for name, script := range map[string]string{
		"exit status": "#!/bin/sh\nexit 3\n",
		"bad json":    "#!/bin/sh\necho 'not json'\n",
	} {
		t.Run(name, func(t *testing.T) {
			w := newWorld(t)
			bin := filepath.Join(w.home, "broken-claude")
			w.write(bin, script)
			os.Chmod(bin, 0o755)
			t.Setenv("NEKO_CLAUDE", bin)
			snap := New(testCfg()).Refresh()
			if snap.Err == nil {
				t.Fatal("want an error from a failing `claude agents`")
			}
			if len(snap.Sessions) != 0 {
				t.Fatalf("no sessions on error, got %d", len(snap.Sessions))
			}
			if snap.At.IsZero() {
				t.Fatal("snapshot time not set")
			}
		})
	}
}

func TestRefreshBackgroundJob(t *testing.T) {
	w := newWorld(t)
	now := time.Now()
	w.agents(claude.Agent{ID: "abcd1234", Kind: "background", Cwd: w.cwd, SessionID: "abcd1234-s",
		Name: "abcd1234", State: "needs_input", StartedAt: now.Add(-time.Hour).UnixMilli()})
	w.writeJSON(filepath.Join(w.claude, "jobs", "abcd1234", "state.json"), map[string]any{
		"state": "blocked", "detail": "waiting on review", "needs": "an answer", "suggestedReply": "yes",
		"children": []map[string]any{{"id": "c1", "href": "https://x/pr/1", "kind": "pr", "title": "PR 1"}},
		"block":    map[string]any{"questions": []map[string]any{{"question": "go?", "options": []map[string]any{{"label": "yes"}}}}},
		"output":   map[string]any{"result": "all good"},
	})
	w.transcript("abcd1234-s",
		userRec(w.cwd, now.Add(-50*time.Minute), "arruma o build"),
		titleRec("Arrumar build"),
		asstRec(now.Add(-40*time.Minute), "r1", "claude-opus-4", 1000, 10, "pronto"),
	)

	ss := byID(New(testCfg()).Refresh())["abcd1234-s"]
	if ss.ShortID != "abcd1234" || !ss.Attachable() {
		t.Fatalf("background session must be attachable by its short id: %+v", ss)
	}
	if ss.State != "blocked" || ss.Attention != AttnBlocked {
		t.Fatalf("state %q attention %d, want blocked/AttnBlocked", ss.State, ss.Attention)
	}
	if ss.Detail != "waiting on review" || ss.Needs != "an answer" || ss.SuggestedReply != "yes" || ss.Result != "all good" {
		t.Fatalf("job fields not copied: %+v", ss)
	}
	if len(ss.Links) != 1 || ss.Links[0].Href != "https://x/pr/1" {
		t.Fatalf("links: %+v", ss.Links)
	}
	if len(ss.Questions) != 1 || ss.Questions[0].Question != "go?" {
		t.Fatalf("questions: %+v", ss.Questions)
	}
	if ss.Title != "Arrumar build" || ss.Label() != "Arrumar build" {
		t.Fatalf("title %q label %q", ss.Title, ss.Label())
	}
	if ss.FirstUser != "arruma o build" || ss.LastText != "pronto" || ss.Model != "claude-opus-4" {
		t.Fatalf("transcript fields: %+v", ss)
	}
	if ss.LastAt.Sub(now.Add(-40*time.Minute)).Abs() > time.Millisecond {
		t.Fatalf("LastAt %v", ss.LastAt)
	}
	if ss.Context != "dev · neko" {
		t.Fatalf("context from the cwd rule: %q", ss.Context)
	}
}

func TestRefreshInteractive(t *testing.T) {
	w := newWorld(t)
	now := time.Now()
	w.agents(
		// live state file overrides the CLI status
		claude.Agent{Kind: "interactive", PID: 4242, Cwd: w.cwd, SessionID: "live-s", Status: "busy", StartedAt: now.UnixMilli()},
		// no live file: the CLI status, normalized
		claude.Agent{Kind: "interactive", PID: 4343, Cwd: w.cwd, SessionID: "plain-s", Status: "working", StartedAt: now.UnixMilli()},
	)
	w.writeJSON(filepath.Join(w.claude, "sessions", "4242.json"), map[string]string{"status": "waiting", "waitingFor": "permission prompt"})

	got := byID(New(testCfg()).Refresh())
	live := got["live-s"]
	if live.State != "blocked" || live.WaitingFor != "permission prompt" || live.Attention != AttnBlocked {
		t.Fatalf("live: %+v", live)
	}
	if live.ShortID != "" || live.Attachable() {
		t.Fatal("interactive sessions are not attachable")
	}
	plain := got["plain-s"]
	if plain.State != "busy" || plain.Attention != AttnNone {
		t.Fatalf("plain: state %q attn %d", plain.State, plain.Attention)
	}
	// no transcript yet: its start is its last activity
	if plain.LastAt.UnixMilli() != now.UnixMilli() {
		t.Fatalf("LastAt %v, want start %v", plain.LastAt, now)
	}
	if plain.Label() != "conversa vazia em neko" {
		t.Fatalf("label %q", plain.Label())
	}
}

func TestRefreshHistory(t *testing.T) {
	w := newWorld(t)
	now := time.Now()
	w.agents(claude.Agent{Kind: "interactive", PID: 1, Cwd: w.cwd, SessionID: "running-s", Status: "idle", StartedAt: now.UnixMilli()})
	w.transcript("running-s", userRec(w.cwd, now, "oi"), asstRec(now, "r0", "claude-sonnet-4", 1, 1, "oi"))
	w.transcript("hist-s", userRec(w.cwd, now.Add(-time.Hour), "faz o relatório"), asstRec(now.Add(-time.Hour), "r1", "claude-sonnet-4", 10, 10, "feito"))
	w.transcript("noreq-s", userRec(w.cwd, now, "só pergunta, sem resposta"))
	w.transcript("neko-s", userRec(w.cwd, now, `Chame searchJiraIssuesUsingJql exatamente uma vez com cloudId="x"`), asstRec(now, "r2", "claude-haiku-4", 1, 1, "ok"))
	w.transcript("nocwd-s", asstRec(now, "r3", "claude-haiku-4", 1, 1, "sem cwd"))
	old := w.transcript("old-s", userRec(w.cwd, now.Add(-10*24*time.Hour), "velho"), asstRec(now.Add(-10*24*time.Hour), "r4", "claude-haiku-4", 1, 1, "ok"))
	os.Chtimes(old, now.Add(-10*24*time.Hour), now.Add(-10*24*time.Hour))

	cfg := testCfg()
	cfg.ShowHistoryDays = 2
	got := byID(New(cfg).Refresh())
	if len(got) != 2 {
		t.Fatalf("want the running session and one history session, got %v", keys(got))
	}
	if got["running-s"].Kind != "interactive" {
		t.Fatalf("a live session must not be duplicated as history: %+v", got["running-s"])
	}
	h, ok := got["hist-s"]
	if !ok {
		t.Fatalf("hist-s missing: %v", keys(got))
	}
	if h.Kind != "history" || h.State != "history" || h.Cwd != w.cwd || h.Attention != AttnNone {
		t.Fatalf("history session: %+v", h)
	}
	if h.Label() != "faz o relatório" {
		t.Fatalf("history label %q", h.Label())
	}

	// show_history_days = 0 turns history off
	if got := byID(New(testCfg()).Refresh()); len(got) != 1 {
		t.Fatalf("history off: %v", keys(got))
	}
}

func keys(m map[string]Session) []string {
	var out []string
	for k := range m {
		out = append(out, k)
	}
	return out
}

func TestRefreshAttentionAndStale(t *testing.T) {
	now := time.Now()
	old := now.Add(-5 * 24 * time.Hour) // older than the default stale_days (3)
	cases := []struct {
		name      string
		agent     claude.Agent
		seen      bool // MarkSeen at its last activity
		staleDays float64
		wantState string
		wantAttn  Attention
		wantStale bool
		wantFin   bool
	}{
		{"bg blocked", claude.Agent{Kind: "background", ID: "a", State: "blocked", StartedAt: now.UnixMilli()}, false, 3, "blocked", AttnBlocked, false, false},
		{"bg done unseen", claude.Agent{Kind: "background", ID: "a", State: "completed", StartedAt: now.UnixMilli()}, false, 3, "done", AttnReturned, false, false},
		{"bg done seen is finished", claude.Agent{Kind: "background", ID: "a", State: "done", StartedAt: now.UnixMilli()}, true, 3, "done", AttnNone, true, true},
		{"bg stopped is finished", claude.Agent{Kind: "background", ID: "a", State: "killed", StartedAt: now.UnixMilli()}, false, 3, "stopped", AttnNone, true, true},
		{"bg failed is finished", claude.Agent{Kind: "background", ID: "a", State: "crashed", StartedAt: now.UnixMilli()}, false, 3, "failed", AttnNone, true, true},
		{"bg idle never asks", claude.Agent{Kind: "background", ID: "a", State: "idle", StartedAt: now.UnixMilli()}, false, 3, "idle", AttnNone, false, false},
		{"interactive idle unseen", claude.Agent{Kind: "interactive", Status: "idle", StartedAt: now.UnixMilli()}, false, 3, "idle", AttnIdle, false, false},
		{"interactive idle seen", claude.Agent{Kind: "interactive", Status: "ready", StartedAt: now.UnixMilli()}, true, 3, "idle", AttnNone, false, false},
		{"interactive done is not finished", claude.Agent{Kind: "interactive", Status: "done", StartedAt: now.UnixMilli()}, true, 3, "done", AttnNone, false, false},
		{"old idle is stale", claude.Agent{Kind: "interactive", Status: "idle", StartedAt: old.UnixMilli()}, false, 3, "idle", AttnNone, true, false},
		{"old blocked is stale, no attention", claude.Agent{Kind: "background", ID: "a", State: "blocked", StartedAt: old.UnixMilli()}, false, 3, "blocked", AttnNone, true, false},
		{"old busy is never stale", claude.Agent{Kind: "background", ID: "a", State: "running", StartedAt: old.UnixMilli()}, false, 3, "busy", AttnNone, false, false},
		{"stale_days 0 disables staleness", claude.Agent{Kind: "interactive", Status: "idle", StartedAt: old.UnixMilli()}, false, 0, "idle", AttnIdle, false, false},
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			w := newWorld(t)
			c.agent.SessionID, c.agent.Cwd = "s1", w.cwd
			w.agents(c.agent)
			cfg := testCfg()
			cfg.StaleDays = c.staleDays
			s := New(cfg)
			if c.seen {
				s.MarkSeen("s1", time.UnixMilli(c.agent.StartedAt))
			}
			ss := byID(s.Refresh())["s1"]
			if ss.State != c.wantState || ss.Attention != c.wantAttn || ss.Stale != c.wantStale || ss.Finished != c.wantFin {
				t.Fatalf("state=%q attn=%d stale=%v finished=%v; want %q %d %v %v",
					ss.State, ss.Attention, ss.Stale, ss.Finished, c.wantState, c.wantAttn, c.wantStale, c.wantFin)
			}
		})
	}
}

func TestRefreshEndedConversation(t *testing.T) {
	w := newWorld(t)
	now := time.Now()
	// the daemon still reports it as running; the transcript says it ended
	w.agents(claude.Agent{Kind: "background", ID: "e1", SessionID: "end-s", Cwd: w.cwd, State: "running", StartedAt: now.UnixMilli()})
	w.transcript("end-s", userRec(w.cwd, now, "tchau"), `{"type":"ended-by-model","timestamp":"`+ts(now)+`"}`)
	ss := byID(New(testCfg()).Refresh())["end-s"]
	if !ss.Ended || ss.State != "ended" || !ss.Stale || ss.Attention != AttnNone {
		t.Fatalf("ended: %+v", ss)
	}
}

func TestMarkSeenPersists(t *testing.T) {
	w := newWorld(t)
	now := time.Now()
	w.agents(claude.Agent{Kind: "background", ID: "d1", SessionID: "done-s", Cwd: w.cwd, State: "done", StartedAt: now.UnixMilli()})

	s := New(testCfg())
	if a := byID(s.Refresh())["done-s"].Attention; a != AttnReturned {
		t.Fatalf("unseen done job: attention %d", a)
	}
	s.MarkSeen("done-s", now)
	// a new store (a restart) still knows it was seen
	if ss := byID(New(testCfg()).Refresh())["done-s"]; ss.Attention != AttnNone || !ss.Finished {
		t.Fatalf("after MarkSeen + restart: attn %d finished %v", ss.Attention, ss.Finished)
	}
	// seen up to an earlier moment: newer activity asks again
	s.MarkSeen("done-s", now.Add(-time.Minute))
	if a := byID(New(testCfg()).Refresh())["done-s"].Attention; a != AttnReturned {
		t.Fatalf("activity after the seen mark: attention %d", a)
	}
}

func TestArchiveUnarchivePersist(t *testing.T) {
	w := newWorld(t)
	start := time.Now().Add(-time.Hour)
	w.agents(claude.Agent{Kind: "background", ID: "b1", SessionID: "arch-s", Cwd: w.cwd, State: "blocked", StartedAt: start.UnixMilli()})

	s := New(testCfg())
	s.Archive("arch-s")
	s2 := New(testCfg()) // reload from archived.json (v2: time per id)
	ss := byID(s2.Refresh())["arch-s"]
	if !ss.Archived || !ss.Stale || ss.Attention != AttnNone || ss.ArchivedAt.IsZero() {
		t.Fatalf("archived: %+v", ss)
	}
	s2.Unarchive("arch-s")
	ss = byID(New(testCfg()).Refresh())["arch-s"]
	if ss.Archived || ss.Stale || ss.Attention != AttnBlocked {
		t.Fatalf("unarchived: archived=%v stale=%v attn=%d", ss.Archived, ss.Stale, ss.Attention)
	}
}

func TestArchiveCorruptTimeFallsBackToNow(t *testing.T) {
	w := newWorld(t)
	w.agents(claude.Agent{Kind: "background", ID: "b1", SessionID: "x-s", Cwd: w.cwd, State: "done", StartedAt: time.Now().Add(-time.Hour).UnixMilli()})
	w.write(filepath.Join(w.home, ".local", "state", "nekuroddo", "archived.json"), `{"x-s":"not a time"}`)
	before := time.Now()
	s := New(testCfg())
	if at := s.archived["x-s"]; at.Before(before) {
		t.Fatalf("an unreadable archive time loads as now, got %v", at)
	}
	if !byID(s.Refresh())["x-s"].Archived {
		t.Fatal("still archived")
	}
}

func TestRefreshCosts(t *testing.T) {
	w := newWorld(t)
	now := time.Now()
	// opus: $5/MTok in, $25/MTok out (config defaults)
	w.agents(
		claude.Agent{Kind: "background", ID: "c1", SessionID: "cost-s", Cwd: w.cwd, State: "running", StartedAt: now.Add(-72 * time.Hour).UnixMilli()},
		claude.Agent{Kind: "background", ID: "c2", SessionID: "cost2-s", Cwd: w.cwd, State: "running", StartedAt: now.UnixMilli()},
	)
	w.transcript("cost-s",
		userRec(w.cwd, now.Add(-48*time.Hour), "começa"),
		asstRec(now.Add(-48*time.Hour), "old", "claude-opus-4", 1_000_000, 0, "a"), // $5, two days ago
		asstRec(now.Add(-3*time.Hour), "mid", "claude-opus-4", 0, 40_000, "b"),     // $1, 3h ago
		asstRec(now.Add(-3*time.Hour), "mid", "claude-opus-4", 0, 40_000, "b"),     // same request again: counted once
		asstRec(now, "new", "claude-opus-4", 200_000, 0, "c"),                      // $1, now
	)
	w.transcript("cost2-s", asstRec(now, "x", "claude-haiku-4", 1_000_000, 0, "h")) // $1 haiku

	// default window: the last 5 hours
	s := New(testCfg())
	snap := s.Refresh()
	ss := byID(snap)["cost-s"]
	if !near(ss.Cost, 7) {
		t.Fatalf("total cost %v, want 7", ss.Cost)
	}
	if !near(ss.WindowCost, 2) {
		t.Fatalf("window cost (5h default) %v, want 2", ss.WindowCost)
	}
	if ss.Tokens != 40_000 {
		t.Fatalf("output tokens %d", ss.Tokens)
	}
	wantToday := 1.0
	if now.Add(-3*time.Hour).Format("2006-01-02") == now.Format("2006-01-02") {
		wantToday = 2
	}
	if !near(ss.CostToday, wantToday) {
		t.Fatalf("today %v, want %v", ss.CostToday, wantToday)
	}
	if !near(snap.TodayCost, wantToday+1) {
		t.Fatalf("snapshot today %v, want %v", snap.TodayCost, wantToday+1)
	}

	// /usage says the window began an hour ago
	s.SetWindowStart(now.Add(-time.Hour))
	if wc := byID(s.Refresh())["cost-s"].WindowCost; !near(wc, 1) {
		t.Fatalf("window cost since -1h %v, want 1", wc)
	}
}

func TestRefreshSubagents(t *testing.T) {
	w := newWorld(t)
	now := time.Now()
	w.agents(claude.Agent{Kind: "background", ID: "p1", SessionID: "parent-s", Cwd: w.cwd, State: "running", StartedAt: now.UnixMilli()})
	w.transcript("parent-s", userRec(w.cwd, now, "pai"), asstRec(now, "p", "claude-opus-4", 200_000, 0, "delegando")) // $1
	sub := filepath.Join(w.proj, "parent-s", "subagents")
	w.write(filepath.Join(sub, "agent-old.jsonl"), joinLines([]string{
		userRec(w.cwd, now.Add(-10*time.Minute), "procura os testes"),
		asstRec(now.Add(-10*time.Minute), "s1", "claude-haiku-4", 1_000_000, 0, "achei"), // $1
	}))
	w.writeJSON(filepath.Join(sub, "agent-old.meta.json"), map[string]string{"agentType": "Explore", "description": "achar testes"})
	w.write(filepath.Join(sub, "agent-new.jsonl"), joinLines([]string{
		userRec(w.cwd, now, "revisa o diff\ncom calma"),
		asstRec(now, "s2", "claude-sonnet-4", 0, 0, "revisando"),
	}))

	ss := byID(New(testCfg()).Refresh())["parent-s"]
	if len(ss.Subs) != 2 {
		t.Fatalf("subs: %+v", ss.Subs)
	}
	o, n := ss.Subs[0], ss.Subs[1] // oldest activity first
	if o.ID != "old" || o.Type != "Explore" || o.Description != "achar testes" || !o.Done || !near(o.Cost, 1) || o.LastText != "achei" {
		t.Fatalf("old sub: %+v", o)
	}
	if n.ID != "new" || n.Description != "revisa o diff" || n.Done || n.Model != "claude-sonnet-4" {
		t.Fatalf("new sub (described by its task): %+v", n)
	}
	if !near(ss.Cost, 2) {
		t.Fatalf("session cost includes subagents: %v", ss.Cost)
	}
}

func TestReapply(t *testing.T) {
	w := newWorld(t)
	s := New(testCfg())
	sessions := []Session{
		{SessionID: "a", Cwd: w.cwd, Context: "stale value", ContextSet: true},
		{SessionID: "b", Cwd: filepath.Join(w.home, "work", "api")},
	}
	s.Rename("a", "  meu  ")
	s.SetContext("b", "outro lugar")
	s.Reapply(sessions)
	if sessions[0].CustomName != "meu" || sessions[0].Label() != "meu" {
		t.Fatalf("rename is trimmed and applied: %+v", sessions[0])
	}
	if sessions[0].Context != "dev · neko" || sessions[0].ContextSet {
		t.Fatalf("context goes back to the cwd rule: %+v", sessions[0])
	}
	if sessions[1].Context != "outro lugar" || !sessions[1].ContextSet {
		t.Fatalf("moved by hand: %+v", sessions[1])
	}
	s.SetContext("b", "")
	s.RenameContext("work · api", "API")
	s.Reapply(sessions)
	if sessions[1].Context != "API" || sessions[1].ContextSet {
		t.Fatalf("section rename: %+v", sessions[1])
	}
	// renaming back to the original name drops the override
	s.RenameContext("API", "work · api")
	if _, ok := s.ov.ContextName["work · api"]; ok {
		t.Fatal("renaming to the original should delete the override")
	}
	s.Reapply(sessions)
	if sessions[1].Context != "work · api" {
		t.Fatalf("undo by original name: %q", sessions[1].Context)
	}
}

func TestOverridesFileSurvivesRestart(t *testing.T) {
	newWorld(t)
	s := New(testCfg())
	s.Rename("a", "nome")
	s.SetContext("a", "ctx")
	s.RenameContext("dev · neko", "neko")
	s2 := New(testCfg())
	if s2.ov.Names["a"] != "nome" || s2.ov.Contexts["a"] != "ctx" || s2.ov.ContextName["dev · neko"] != "neko" {
		t.Fatalf("reloaded overrides: %+v", s2.ov)
	}
}

func TestWindowDefault(t *testing.T) {
	newWorld(t)
	s := New(testCfg())
	if d := time.Since(s.window()); d < 5*time.Hour-time.Minute || d > 5*time.Hour+time.Minute {
		t.Fatalf("default window starts ~5h ago, got %v", d)
	}
	at := time.Date(2026, 1, 2, 3, 4, 0, 0, time.UTC)
	s.SetWindowStart(at)
	if !s.window().Equal(at) {
		t.Fatalf("window %v", s.window())
	}
	s.SetWindowStart(time.Time{})
	if time.Since(s.window()) < 4*time.Hour {
		t.Fatal("zero window start falls back to 5h ago")
	}
}

func TestLabelRemainingRules(t *testing.T) {
	cases := []struct {
		name string
		s    Session
		want string
	}{
		// Claude's "<dir>-xx" name with nothing better is still shown
		{"derived dir name, no title or prompt", Session{SessionID: "s", Name: "neko-3b", Cwd: "/x/neko"}, "neko-3b"},
		{"derived dir name with prompt", Session{SessionID: "s", Name: "neko-3b", Cwd: "/x/neko", FirstUser: "faz algo"}, "faz algo"},
		{"short name equal to id prefix is an id", Session{SessionID: "abcdef12-rest", Name: "abcdef12", Cwd: "/x/y"}, "conversa vazia em y"},
		{"short name under 8 chars is a real name", Session{SessionID: "abc-rest", Name: "abc", Cwd: "/x/y"}, "abc"},
	}
	for _, c := range cases {
		if got := c.s.Label(); got != c.want {
			t.Errorf("%s: got %q, want %q", c.name, got, c.want)
		}
	}
}
