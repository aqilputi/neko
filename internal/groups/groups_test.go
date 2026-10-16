package groups

import (
	"bytes"
	"encoding/json"
	"reflect"
	"strings"
	"testing"
	"time"
)

func TestCleanTag(t *testing.T) {
	cases := map[string]string{
		"#Deploy RC":      "deploy-rc",
		"  # Deploy  RC ": "deploy-rc",
		"deploy":          "deploy",
		"":                "",
		"#":               "",
		"A\tB  c":         "a-b-c",
	}
	for in, want := range cases {
		if got := CleanTag(in); got != want {
			t.Errorf("CleanTag(%q) = %q, want %q", in, got, want)
		}
	}
}

func newFile() File { return File{Groups: map[string][]Member{}} }

func TestAddDedupesAndUpdates(t *testing.T) {
	f := newFile()
	f.Add("x", Member{SessionID: "A", Name: "a"}, Member{SessionID: "B", Name: "b"})
	f.Add("x", Member{SessionID: "A", Name: "a2"}, Member{SessionID: "C", Name: "c"})
	got := f.Groups["x"]
	want := []Member{{SessionID: "A", Name: "a2"}, {SessionID: "B", Name: "b"}, {SessionID: "C", Name: "c"}}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("members = %+v, want %+v", got, want)
	}
}

func TestRemove(t *testing.T) {
	f := newFile()
	f.Add("x", Member{SessionID: "A"}, Member{SessionID: "B"})
	f.Add("y", Member{SessionID: "A"})
	f.Add("z", Member{SessionID: "B"}, Member{SessionID: "C"})

	if n := f.Remove("x", "A"); n != 1 {
		t.Fatalf("Remove(x, A) = %d, want 1", n)
	}
	if _, ok := f.Groups["y"]; !ok {
		t.Fatal("Remove with a tag touched another group")
	}
	if n := f.Remove("y", "A"); n != 1 {
		t.Fatalf("Remove(y, A) = %d, want 1", n)
	}
	if _, ok := f.Groups["y"]; ok {
		t.Fatal("empty group y was not dropped")
	}
	if n := f.Remove("", "B"); n != 2 {
		t.Fatalf(`Remove("", B) = %d, want 2`, n)
	}
	if _, ok := f.Groups["x"]; ok {
		t.Fatal("empty group x was not dropped")
	}
	if got := f.Groups["z"]; len(got) != 1 || got[0].SessionID != "C" {
		t.Fatalf("z = %+v, want only C", got)
	}
	if n := f.Remove("", "nobody"); n != 0 {
		t.Fatalf("Remove of unknown id = %d, want 0", n)
	}
}

func TestOfAndTags(t *testing.T) {
	f := newFile()
	if f.Tags() != nil || f.Of("A") != nil {
		t.Fatal("empty file should have no tags")
	}
	f.Add("zeta", Member{SessionID: "A"})
	f.Add("alpha", Member{SessionID: "A"}, Member{SessionID: "B"})
	f.Add("mid", Member{SessionID: "B"})
	if got, want := f.Of("A"), []string{"alpha", "zeta"}; !reflect.DeepEqual(got, want) {
		t.Fatalf("Of(A) = %v, want %v", got, want)
	}
	if got, want := f.Tags(), []string{"alpha", "mid", "zeta"}; !reflect.DeepEqual(got, want) {
		t.Fatalf("Tags() = %v, want %v", got, want)
	}
}

func TestContext(t *testing.T) {
	f := newFile()
	if f.Context("A") != "" {
		t.Fatal("ungrouped session should get no context")
	}
	f.Add("deploy", Member{SessionID: "A", Name: "self-name"},
		Member{SessionID: "B", Name: "peer-b", Label: "Peer B label", Cwd: "/srv/b"},
		Member{SessionID: "C", Name: "peer-c", Label: "peer-c"})
	ctx := f.Context("A")
	for _, s := range []string{"#deploy", `"peer-b"`, "Peer B label", "/srv/b", `"peer-c"`} {
		if !strings.Contains(ctx, s) {
			t.Errorf("context lacks %q:\n%s", s, ctx)
		}
	}
	if strings.Contains(ctx, "self-name") {
		t.Errorf("context lists the session itself:\n%s", ctx)
	}
	if strings.Count(ctx, "peer-c") != 1 {
		t.Errorf("label equal to name should not be repeated:\n%s", ctx)
	}
	if h := Hash(ctx); h != Hash(strings.Clone(ctx)) || h == Hash(ctx+"x") || len(h) != 16 {
		t.Error("Hash is not a stable 16-hex digest")
	}
}

func TestSaveLoad(t *testing.T) {
	t.Setenv("XDG_STATE_HOME", t.TempDir())
	if f := Load(); f.Groups == nil || len(f.Groups) != 0 {
		t.Fatalf("Load without file = %+v", f)
	}
	f := newFile()
	f.Add("g", Member{SessionID: "A", Name: "a", Cwd: "/x"})
	if err := f.Save(); err != nil {
		t.Fatal(err)
	}
	if got := Load(); !reflect.DeepEqual(got, f) {
		t.Fatalf("Load = %+v, want %+v", got, f)
	}
}

func TestQueueTake(t *testing.T) {
	t.Setenv("XDG_STATE_HOME", t.TempDir())
	if take("A") != nil {
		t.Fatal("take with no mail should be nil")
	}
	n1 := Note{At: time.Unix(1, 0).UTC(), Tag: "g", Text: "one"}
	n2 := Note{At: time.Unix(2, 0).UTC(), Tag: "g", Text: "two"}
	for _, n := range []Note{n1, n2, {Tag: "g"}} { // empty text is skipped
		if err := Queue("A", n); err != nil {
			t.Fatal(err)
		}
	}
	got := take("A")
	if !reflect.DeepEqual(got, []Note{n1, n2}) {
		t.Fatalf("take = %+v", got)
	}
	if take("A") != nil {
		t.Fatal("notes delivered twice")
	}
}

// hook runs Hook and returns additionalContext ("" when nothing printed).
func hook(t *testing.T, payload string) string {
	t.Helper()
	var out bytes.Buffer
	Hook(strings.NewReader(payload), &out)
	if out.Len() == 0 {
		return ""
	}
	var resp struct {
		H struct {
			Event string `json:"hookEventName"`
			Ctx   string `json:"additionalContext"`
		} `json:"hookSpecificOutput"`
	}
	if err := json.Unmarshal(out.Bytes(), &resp); err != nil {
		t.Fatalf("bad hook output %q: %v", out.String(), err)
	}
	if resp.H.Ctx == "" {
		t.Fatalf("hook printed output without context: %s", out.String())
	}
	if !strings.Contains(payload, resp.H.Event) {
		t.Fatalf("hookEventName %q does not match payload %s", resp.H.Event, payload)
	}
	return resp.H.Ctx
}

func TestHook(t *testing.T) {
	t.Setenv("XDG_STATE_HOME", t.TempDir())
	const (
		post  = `{"hook_event_name":"PostToolUse","session_id":"A"}`
		start = `{"hook_event_name":"SessionStart","session_id":"A","source":"resume"}`
		promp = `{"hook_event_name":"UserPromptSubmit","session_id":"A"}`
	)

	// Not grouped: nothing.
	if got := hook(t, post); got != "" {
		t.Fatalf("ungrouped session got %q", got)
	}

	f := newFile()
	f.Add("deploy", Member{SessionID: "A", Name: "me"}, Member{SessionID: "B", Name: "the-peer"})
	if err := f.Save(); err != nil {
		t.Fatal(err)
	}

	got := hook(t, post)
	if !strings.Contains(got, "the-peer") || !strings.Contains(got, "#deploy") {
		t.Fatalf("first call lacks peer context: %q", got)
	}
	if got := hook(t, post); got != "" {
		t.Fatalf("unchanged context repeated: %q", got)
	}
	if got := hook(t, start); !strings.Contains(got, "the-peer") {
		t.Fatalf("SessionStart should resend context, got %q", got)
	}
	if got := hook(t, promp); got != "" {
		t.Fatalf("context repeated after SessionStart: %q", got)
	}

	// A change in the group is told again.
	f.Add("deploy", Member{SessionID: "C", Name: "newcomer"})
	if err := f.Save(); err != nil {
		t.Fatal(err)
	}
	if got := hook(t, promp); !strings.Contains(got, "newcomer") {
		t.Fatalf("changed context not sent: %q", got)
	}

	// Queued note: delivered once, alone (context unchanged).
	if err := Queue("A", Note{Tag: "deploy", Text: "watch the migration"}); err != nil {
		t.Fatal(err)
	}
	got = hook(t, post)
	if !strings.Contains(got, "watch the migration") || !strings.Contains(got, "#deploy") {
		t.Fatalf("note not delivered: %q", got)
	}
	if strings.Contains(got, "the-peer") {
		t.Fatalf("note delivery repeated the unchanged context: %q", got)
	}
	if got := hook(t, post); got != "" {
		t.Fatalf("note delivered twice: %q", got)
	}

	// Removed: one "tirou" message, then nothing.
	f.Remove("", "A")
	if err := f.Save(); err != nil {
		t.Fatal(err)
	}
	if got := hook(t, post); !strings.Contains(got, "tirou") {
		t.Fatalf("removal not told: %q", got)
	}
	if got := hook(t, post); got != "" {
		t.Fatalf("removal told twice: %q", got)
	}
}

func TestHookIgnores(t *testing.T) {
	t.Setenv("XDG_STATE_HOME", t.TempDir())
	f := newFile()
	f.Add("g", Member{SessionID: "A", Name: "me"}, Member{SessionID: "B", Name: "peer"})
	if err := f.Save(); err != nil {
		t.Fatal(err)
	}
	for _, p := range []string{
		`not json`,
		``,
		`{"hook_event_name":"Stop","session_id":"A"}`,
		`{"hook_event_name":"Notification","session_id":"A"}`,
		`{"hook_event_name":"PostToolUse"}`,
	} {
		if got := hook(t, p); got != "" {
			t.Errorf("payload %q produced %q", p, got)
		}
	}
}
