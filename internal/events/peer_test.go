package events

import (
	"encoding/json"
	"os"
	"strings"
	"testing"
)

const realPeerPrompt = "Another Claude session sent a message:\n" +
	"<cross-session-message from=\"uds:/run/user/1000/cc-socks/2819821.sock\" from-name=\"gostaria de saber se existe alguma maneira e…\" from-mode=\"prompting\">\n" +
	"[#probe] teste do neko: responda apenas 'recebido'.\n" +
	"</cross-session-message>\n\n" +
	"This came from another Claude session — not typed by your user..."

func TestPeerMessage(t *testing.T) {
	cases := []struct {
		name, prompt, from, line string
		ok                       bool
	}{
		{"real payload", realPeerPrompt,
			"gostaria de saber se existe alguma maneira e…", "[#probe] teste do neko: responda apenas 'recebido'.", true},
		{"no from-name", "<cross-session-message from=\"uds:x\">\nhello\nworld\n</cross-session-message>",
			"", "hello", true},
		{"html entities", "<cross-session-message from-name=\"say &quot;hi&quot; &amp; bye\">\n  [#t] body  \n</cross-session-message>",
			`say "hi" & bye`, "[#t] body", true},
		{"empty body", "<cross-session-message from-name=\"x\">\n\n  \n</cross-session-message> trailing text",
			"x", "", true},
		{"no tag", "<cross-session-message from-name=\"x\">plain message</cross-session-message>",
			"x", "plain message", true},
		{"no closing tag", "<cross-session-message from-name=\"x\">\nfirst\nsecond",
			"x", "first", true},
		{"unterminated open tag", "<cross-session-message from-name=\"x\"", "", "", false},
		{"plain prompt", "just a prompt from the user", "", "", false},
		{"empty", "", "", "", false},
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			from, line, ok := PeerMessage(c.prompt)
			if from != c.from || line != c.line || ok != c.ok {
				t.Fatalf("PeerMessage = (%q, %q, %v), want (%q, %q, %v)", from, line, ok, c.from, c.line, c.ok)
			}
		})
	}
}

func TestNeedsYou(t *testing.T) {
	cases := []struct {
		e    Event
		want bool
	}{
		{Event{Name: "Notification", Type: "permission_prompt"}, true},
		{Event{Name: "Notification", Type: "idle_prompt"}, true},
		{Event{Name: "Notification"}, true},
		{Event{Name: "Notification", Type: "auth_success"}, false},
		{Event{Name: "Stop"}, false},
		{Event{Name: "UserPromptSubmit", Type: "peer"}, false},
	}
	for _, c := range cases {
		if got := c.e.NeedsYou(); got != c.want {
			t.Errorf("%+v.NeedsYou() = %v, want %v", c.e, got, c.want)
		}
	}
}

func record(t *testing.T, v map[string]string) {
	t.Helper()
	b, err := json.Marshal(v)
	if err != nil {
		t.Fatal(err)
	}
	Record(strings.NewReader(string(b)))
}

func TestRecordTail(t *testing.T) {
	t.Setenv("XDG_STATE_HOME", t.TempDir())
	var tail Tail
	if tail.Poll() != nil {
		t.Fatal("Poll without a file should be nil")
	}

	// History before the first successful Poll is skipped.
	record(t, map[string]string{"hook_event_name": "Stop", "session_id": "old"})
	if got := tail.Poll(); got != nil {
		t.Fatalf("first Poll returned history: %+v", got)
	}
	if got := tail.Poll(); got != nil {
		t.Fatalf("Poll with nothing new returned %+v", got)
	}

	// Ignored payloads write nothing.
	Record(strings.NewReader("not json"))
	Record(strings.NewReader(`{"session_id":"x"}`))

	record(t, map[string]string{
		"hook_event_name": "UserPromptSubmit", "session_id": "A", "cwd": "/w", "prompt": realPeerPrompt,
	})
	record(t, map[string]string{
		"hook_event_name": "Notification", "session_id": "B",
		"notification_type": "permission_prompt", "message": strings.Repeat("é", 400),
	})
	got := tail.Poll()
	if len(got) != 2 {
		t.Fatalf("Poll = %+v, want 2 events", got)
	}
	p := got[0]
	if p.Name != "UserPromptSubmit" || p.SessionID != "A" || p.Cwd != "/w" || p.Type != "peer" ||
		p.From != "gostaria de saber se existe alguma maneira e…" ||
		p.Message != "[#probe] teste do neko: responda apenas 'recebido'." || p.At.IsZero() {
		t.Fatalf("peer event = %+v", p)
	}
	n := got[1]
	if !n.NeedsYou() || n.SessionID != "B" || len([]rune(n.Message)) != 300 {
		t.Fatalf("notification event = %+v (message runes %d)", n, len([]rune(n.Message)))
	}

	// A partial line is held until its newline arrives.
	f, err := os.OpenFile(Path(), os.O_APPEND|os.O_WRONLY, 0o600)
	if err != nil {
		t.Fatal(err)
	}
	if _, err := f.WriteString(`{"event":"Stop","session_id":"C"`); err != nil {
		t.Fatal(err)
	}
	if got := tail.Poll(); got != nil {
		t.Fatalf("partial line returned %+v", got)
	}
	if _, err := f.WriteString("}\n"); err != nil {
		t.Fatal(err)
	}
	_ = f.Close()
	if got := tail.Poll(); len(got) != 1 || got[0].SessionID != "C" {
		t.Fatalf("completed line = %+v", got)
	}

	// Rotation (file shrinks): reading restarts from the top.
	if err := os.WriteFile(Path(), nil, 0o600); err != nil {
		t.Fatal(err)
	}
	record(t, map[string]string{"hook_event_name": "SessionEnd", "session_id": "D"})
	if got := tail.Poll(); len(got) != 1 || got[0].SessionID != "D" {
		t.Fatalf("after rotation = %+v", got)
	}
}
