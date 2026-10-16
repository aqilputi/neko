package main

import (
	"bytes"
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/aqilputi/nekuroddo/internal/groups"
)

func isolate(t *testing.T) string {
	t.Helper()
	root := t.TempDir()
	t.Setenv("HOME", root)
	for _, v := range []string{"XDG_STATE_HOME", "XDG_CONFIG_HOME", "XDG_CACHE_HOME"} {
		t.Setenv(v, filepath.Join(root, strings.ToLower(v)))
	}
	t.Setenv("NEKO_CLAUDE", "/bin/false")
	return root
}

func TestHookRecordsEventSilently(t *testing.T) {
	isolate(t)
	var out, errb bytes.Buffer
	in := strings.NewReader(`{"hook_event_name":"Stop","session_id":"s1","cwd":"/tmp/x"}`)
	if code := run([]string{"hook"}, in, &out, &errb); code != 0 {
		t.Fatalf("hook must exit 0, got %d", code)
	}
	if out.Len() != 0 || errb.Len() != 0 {
		t.Fatalf("hook must print nothing: out=%q err=%q", out.String(), errb.String())
	}
	raw, err := os.ReadFile(filepath.Join(os.Getenv("XDG_STATE_HOME"), "nekuroddo", "events.jsonl"))
	if err != nil {
		t.Fatalf("event file not written: %v", err)
	}
	var ev struct {
		Event     string `json:"event"`
		SessionID string `json:"session_id"`
		Cwd       string `json:"cwd"`
	}
	if err := json.Unmarshal(bytes.TrimSpace(raw), &ev); err != nil {
		t.Fatalf("event line is not JSON: %q", raw)
	}
	if ev.Event != "Stop" || ev.SessionID != "s1" || ev.Cwd != "/tmp/x" {
		t.Fatalf("wrong event recorded: %+v", ev)
	}
}

func TestHookIgnoresGarbage(t *testing.T) {
	isolate(t)
	var out, errb bytes.Buffer
	if code := run([]string{"hook"}, strings.NewReader("not json"), &out, &errb); code != 0 {
		t.Fatalf("garbage on stdin must still exit 0, got %d", code)
	}
	if _, err := os.Stat(filepath.Join(os.Getenv("XDG_STATE_HOME"), "nekuroddo", "events.jsonl")); err == nil {
		t.Fatal("garbage must not produce an event")
	}
}

func TestHookContextForGroupedSession(t *testing.T) {
	isolate(t)
	g := groups.Load()
	g.Add("deploy-rc", groups.Member{SessionID: "a", Name: "deploy rc 1"}, groups.Member{SessionID: "b", Name: "deploy rc 2"})
	if err := g.Save(); err != nil {
		t.Fatal(err)
	}
	var out, errb bytes.Buffer
	in := strings.NewReader(`{"hook_event_name":"SessionStart","session_id":"a","source":"startup"}`)
	if code := run([]string{"hook-context"}, in, &out, &errb); code != 0 {
		t.Fatalf("hook-context must exit 0, got %d (%s)", code, errb.String())
	}
	var resp struct {
		HookSpecificOutput struct {
			HookEventName     string `json:"hookEventName"`
			AdditionalContext string `json:"additionalContext"`
		} `json:"hookSpecificOutput"`
	}
	if err := json.Unmarshal(out.Bytes(), &resp); err != nil {
		t.Fatalf("hook-context should print JSON, got %q", out.String())
	}
	h := resp.HookSpecificOutput
	if h.HookEventName != "SessionStart" || !strings.Contains(h.AdditionalContext, "deploy rc 2") {
		t.Fatalf("context should name the peer: %+v", h)
	}
}

func TestHookContextUngroupedPrintsNothing(t *testing.T) {
	isolate(t)
	var out, errb bytes.Buffer
	in := strings.NewReader(`{"hook_event_name":"PostToolUse","session_id":"lonely"}`)
	if code := run([]string{"hook-context"}, in, &out, &errb); code != 0 || out.Len() != 0 {
		t.Fatalf("ungrouped session: want exit 0 and no output, got %d %q", code, out.String())
	}
}

func TestUnknownSubcommand(t *testing.T) {
	isolate(t)
	var out, errb bytes.Buffer
	code := run([]string{"hok"}, strings.NewReader(""), &out, &errb)
	if code != 1 {
		t.Fatalf("unknown subcommand must exit 1 (never 2, which blocks a hook), got %d", code)
	}
	if !strings.Contains(errb.String(), `"hok"`) || out.Len() != 0 {
		t.Fatalf("message belongs on stderr naming the command: out=%q err=%q", out.String(), errb.String())
	}
}

func TestMigratesLegacyDirsFirst(t *testing.T) {
	isolate(t)
	legacy := filepath.Join(os.Getenv("XDG_STATE_HOME"), "neko")
	if err := os.MkdirAll(legacy, 0o755); err != nil {
		t.Fatal(err)
	}
	var out, errb bytes.Buffer
	run([]string{"hook"}, strings.NewReader(`{"hook_event_name":"Stop","session_id":"s"}`), &out, &errb)
	if _, err := os.Stat(legacy); !os.IsNotExist(err) {
		t.Fatal("legacy state dir should have been moved")
	}
	if _, err := os.Stat(filepath.Join(os.Getenv("XDG_STATE_HOME"), "nekuroddo", "events.jsonl")); err != nil {
		t.Fatal("event should land in the migrated dir")
	}
}
