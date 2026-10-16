package claude

import (
	"os"
	"path/filepath"
	"testing"
)

func writeFile(t *testing.T, p, s string) {
	t.Helper()
	if err := os.MkdirAll(filepath.Dir(p), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(p, []byte(s), 0o644); err != nil {
		t.Fatal(err)
	}
}

func TestClaudeDir(t *testing.T) {
	home := t.TempDir()
	t.Setenv("HOME", home)
	t.Setenv("CLAUDE_CONFIG_DIR", "")
	if claudeDir() != filepath.Join(home, ".claude") {
		t.Fatalf("claudeDir = %q", claudeDir())
	}
	t.Setenv("CLAUDE_CONFIG_DIR", "/cfg")
	if claudeDir() != "/cfg" {
		t.Fatalf("claudeDir = %q", claudeDir())
	}
}

func TestReadJob(t *testing.T) {
	dir := t.TempDir()
	t.Setenv("CLAUDE_CONFIG_DIR", dir)
	writeFile(t, filepath.Join(dir, "jobs", "abcd1234", "state.json"), `{
	  "state":"blocked","detail":"precisa de permissão","tempo":"waiting","needs":"approval",
	  "suggestedReply":"sim","tokens":1234,
	  "children":[{"id":"c1","href":"h","kind":"agent","title":"sub"}],
	  "block":{"questions":[{"question":"Posso?","options":[{"label":"Sim","description":"vai"},{"label":"Não"}]}]},
	  "inFlight":{"tasks":2,"queued":1},
	  "output":{"result":"ok"}
	}`)
	writeFile(t, filepath.Join(dir, "jobs", "bad", "state.json"), `{"state":`)
	writeFile(t, filepath.Join(dir, "jobs", "min", "state.json"), `{"state":"running"}`)

	j := ReadJob("abcd1234")
	if j == nil {
		t.Fatal("job not read")
	}
	if j.State != "blocked" || j.Detail != "precisa de permissão" || j.Tempo != "waiting" || j.Needs != "approval" ||
		j.SuggestedReply != "sim" || j.Tokens != 1234 {
		t.Errorf("job = %+v", *j)
	}
	if len(j.Children) != 1 || j.Children[0] != (Child{ID: "c1", Href: "h", Kind: "agent", Title: "sub"}) {
		t.Errorf("children = %+v", j.Children)
	}
	if j.Block == nil || len(j.Block.Questions) != 1 || j.Block.Questions[0].Question != "Posso?" ||
		len(j.Block.Questions[0].Options) != 2 || j.Block.Questions[0].Options[0] != (Option{"Sim", "vai"}) {
		t.Errorf("block = %+v", j.Block)
	}
	if j.InFlight == nil || j.InFlight.Tasks != 2 || j.InFlight.Queued != 1 || j.Output == nil || j.Output.Result != "ok" {
		t.Errorf("inFlight/output = %+v %+v", j.InFlight, j.Output)
	}

	if m := ReadJob("min"); m == nil || m.Block != nil || m.InFlight != nil || m.Output != nil {
		t.Errorf("minimal job = %+v", m)
	}
	for _, id := range []string{"bad", "missing", ""} {
		if ReadJob(id) != nil {
			t.Errorf("ReadJob(%q) should be nil", id)
		}
	}
	// The id is reduced to its base name: no escaping the jobs folder.
	writeFile(t, filepath.Join(dir, "state.json"), `{"state":"running"}`)
	writeFile(t, filepath.Join(dir, "jobs", "x", "state.json"), `{"state":"done"}`)
	if j := ReadJob("../x"); j == nil || j.State != "done" {
		t.Errorf("ReadJob(../x) = %+v, want the jobs/x file", j)
	}
}

func TestReadLive(t *testing.T) {
	dir := t.TempDir()
	t.Setenv("CLAUDE_CONFIG_DIR", dir)
	writeFile(t, filepath.Join(dir, "sessions", "42.json"), `{"status":"waiting","waitingFor":"permission prompt"}`)
	writeFile(t, filepath.Join(dir, "sessions", "43.json"), `nope`)
	writeFile(t, filepath.Join(dir, "sessions", "0.json"), `{"status":"busy"}`)

	if l := ReadLive(42); l == nil || *l != (Live{Status: "waiting", WaitingFor: "permission prompt"}) {
		t.Fatalf("ReadLive(42) = %+v", l)
	}
	for _, pid := range []int{43, 44, 0, -1} {
		if l := ReadLive(pid); l != nil {
			t.Errorf("ReadLive(%d) = %+v, want nil", pid, l)
		}
	}
}
