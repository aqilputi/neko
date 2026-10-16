package wire

import (
	"os"
	"path/filepath"
	"reflect"
	"strconv"
	"strings"
	"testing"
	"time"

	"github.com/aqilputi/nekuroddo/internal/app"
	"github.com/aqilputi/nekuroddo/internal/config"
	"github.com/aqilputi/nekuroddo/internal/events"
	"github.com/aqilputi/nekuroddo/internal/groups"
	"github.com/aqilputi/nekuroddo/internal/jira"
	"github.com/aqilputi/nekuroddo/internal/paths"
	"github.com/aqilputi/nekuroddo/internal/store"
)

// Compile-time: the real packages satisfy the ports they are said to.
var (
	_ app.SessionRepository = (*store.Store)(nil)
	_ app.EventLog          = (*events.Tail)(nil)
)

// isolate points HOME and the XDG dirs at a temp dir, so nothing touches the
// user's real state.
func isolate(t *testing.T) string {
	t.Helper()
	root := t.TempDir()
	t.Setenv("HOME", root)
	t.Setenv("XDG_CONFIG_HOME", filepath.Join(root, "config"))
	t.Setenv("XDG_STATE_HOME", filepath.Join(root, "state"))
	t.Setenv("XDG_CACHE_HOME", filepath.Join(root, "cache"))
	return root
}

// fakeClaude installs a stand-in for the claude binary (NEKO_CLAUDE): every
// call appends its argv (args joined by \x1f) as one line to the returned
// file, prints the contents of out and exits with code.
func fakeClaude(t *testing.T, out string, code int) (argvFile string) {
	t.Helper()
	dir := t.TempDir()
	argvFile = filepath.Join(dir, "argv")
	outFile := filepath.Join(dir, "out")
	if err := os.WriteFile(outFile, []byte(out), 0o644); err != nil {
		t.Fatal(err)
	}
	script := "#!/bin/sh\n" +
		"sep=''; for a in \"$@\"; do printf '%s%s' \"$sep\" \"$a\"; sep=$(printf '\\037'); done >> '" + argvFile + "'\n" +
		"echo >> '" + argvFile + "'\n" +
		"cat '" + outFile + "'\n" +
		"exit " + strconv.Itoa(code) + "\n"
	bin := filepath.Join(dir, "claude")
	if err := os.WriteFile(bin, []byte(script), 0o755); err != nil {
		t.Fatal(err)
	}
	t.Setenv("NEKO_CLAUDE", bin)
	return argvFile
}

// calls reads back every recorded invocation, one argv per call.
func calls(t *testing.T, f string) [][]string {
	t.Helper()
	b, err := os.ReadFile(f)
	if err != nil {
		t.Fatalf("fake was not run: %v", err)
	}
	var cs [][]string
	for _, l := range strings.Split(strings.TrimSuffix(string(b), "\n"), "\n") {
		cs = append(cs, strings.Split(l, "\x1f"))
	}
	return cs
}

// fakeBin puts an executable named name in a fresh dir that records its argv
// like fakeClaude, and makes that dir the whole PATH. It returns the argv file.
func fakeBin(t *testing.T, name string) string {
	t.Helper()
	dir := t.TempDir()
	argvFile := filepath.Join(dir, "argv")
	script := "#!/bin/sh\n" +
		"sep=''; for a in \"$@\"; do printf '%s%s' \"$sep\" \"$a\"; sep=$(printf '\\037'); done >> '" + argvFile + ".tmp'\n" +
		"echo >> '" + argvFile + ".tmp'\n" +
		"/bin/mv '" + argvFile + ".tmp' '" + argvFile + "'\n"
	if err := os.WriteFile(filepath.Join(dir, name), []byte(script), 0o755); err != nil {
		t.Fatal(err)
	}
	t.Setenv("PATH", dir)
	return argvFile
}

func TestDefaultBindsEveryPort(t *testing.T) {
	isolate(t)
	p := Default(config.Default())
	v := reflect.ValueOf(p)
	for i := range v.NumField() {
		if v.Field(i).IsNil() {
			t.Errorf("port %s is nil", v.Type().Field(i).Name)
		}
	}
	if _, ok := p.Sessions.(*store.Store); !ok {
		t.Errorf("Sessions = %T, want *store.Store", p.Sessions)
	}
	if _, ok := p.Events.(*events.Tail); !ok {
		t.Errorf("Events = %T, want *events.Tail", p.Events)
	}
}

func TestClock(t *testing.T) {
	before := time.Now()
	got := Clock{}.Now()
	if got.Before(before) || got.After(time.Now()) {
		t.Fatalf("Now = %v", got)
	}
}

func TestClaude(t *testing.T) {
	isolate(t)
	var c app.ClaudeCLI = Claude{}

	fakeClaude(t, `[{"id":"abcdef12","sessionId":"s1","kind":"background","state":"running"}]`, 0)
	as, err := c.ListAgents()
	if err != nil || len(as) != 1 || as[0].SessionID != "s1" {
		t.Fatalf("ListAgents = %+v, %v", as, err)
	}

	dir := t.TempDir()
	argv := fakeClaude(t, "started 0123abcd\n", 0)
	id, err := c.Dispatch(dir, "nome", "faz", "--model", "x")
	if err != nil || id != "0123abcd" {
		t.Fatalf("Dispatch = %q, %v", id, err)
	}
	id, err = c.ResumeInBackground(dir, "sess")
	if err != nil || id != "0123abcd" {
		t.Fatalf("ResumeInBackground = %q, %v", id, err)
	}
	if err := c.Stop("0123abcd"); err != nil {
		t.Fatal(err)
	}
	if err := c.Remove("0123abcd"); err != nil {
		t.Fatal(err)
	}
	cs := calls(t, argv)
	want := [][]string{
		{"--bg", "--allow-dangerously-skip-permissions", "--name", "nome", "--model", "x", "faz"},
		{"--bg", "--allow-dangerously-skip-permissions", "--resume", "sess"},
		{"stop", "0123abcd"},
		{"rm", "0123abcd"},
	}
	if !reflect.DeepEqual(cs, want) {
		t.Fatalf("calls = %q", cs)
	}
	if got := c.AttachArgv("0123abcd"); len(got) != 3 || got[1] != "attach" || got[2] != "0123abcd" {
		t.Fatalf("AttachArgv = %q", got)
	}

	fakeClaude(t, "Current session: 33% used · resets Oct 15, 11:30am (UTC)\n", 0)
	u, err := c.FetchUsage()
	if l, ok := u.Find("session"); err != nil || !ok || l.Pct != 33 {
		t.Fatalf("FetchUsage = %+v, %v", u, err)
	}

	fakeClaude(t, "boom", 1)
	if err := c.Stop("x"); err == nil {
		t.Fatal("Stop with failing claude: want error")
	}
}

func TestSessions(t *testing.T) {
	isolate(t)
	fakeClaude(t, `[]`, 0)
	s := Default(config.Default()).Sessions
	snap := s.Refresh()
	if snap.Err != nil || len(snap.Sessions) != 0 {
		t.Fatalf("Refresh = %+v", snap)
	}
	s.Archive("a")
	s.Unarchive("a")
	s.Rename("a", "nome")
	s.MarkSeen("a", time.Now())
	if _, err := os.Stat(filepath.Join(paths.StateDir(), "overrides.json")); err != nil {
		t.Fatalf("Rename did not persist: %v", err)
	}
}

func TestGroups(t *testing.T) {
	isolate(t)
	var g app.GroupStore = Groups{}
	f := g.Load()
	f.Add("deploy", groups.Member{SessionID: "s1", Name: "a"})
	if err := g.Save(f); err != nil {
		t.Fatal(err)
	}
	if got := g.Load(); len(got.Groups["deploy"]) != 1 {
		t.Fatalf("Load after Save = %+v", got)
	}
	if err := g.Queue("s1", groups.Note{Tag: "deploy", Text: "oi"}); err != nil {
		t.Fatal(err)
	}
	b, err := os.ReadFile(filepath.Join(paths.StateDir(), "mail", "s1.jsonl"))
	if err != nil || !strings.Contains(string(b), `"oi"`) {
		t.Fatalf("queued note = %q, %v", b, err)
	}
}

func TestEvents(t *testing.T) {
	isolate(t)
	ev := Default(config.Default()).Events
	record := func(s string) { events.Record(strings.NewReader(s)) }
	record(`{"hook_event_name":"Stop","session_id":"old"}`)
	if got := ev.Poll(); got != nil {
		t.Fatalf("first Poll must skip history, got %+v", got)
	}
	record(`{"hook_event_name":"Stop","session_id":"new"}`)
	got := ev.Poll()
	if len(got) != 1 || got[0].SessionID != "new" {
		t.Fatalf("Poll = %+v", got)
	}
}

func TestPanes(t *testing.T) {
	var ps app.PTYSpawner = Panes{}
	p, err := ps.Start("id", "título", t.TempDir(), 80, 24, "/bin/sh", "-c", "read x; printf hi-$x")
	if err != nil {
		t.Fatal(err)
	}
	defer p.Close()
	if p.ID() != "id" {
		t.Fatalf("ID = %q", p.ID())
	}
	bs, cancel := p.Subscribe()
	defer cancel()
	p.Resize(100, 30)
	if _, err := p.Write([]byte("go\r")); err != nil {
		t.Fatal(err)
	}
	var out strings.Builder
	deadline := time.After(5 * time.Second)
	for !strings.Contains(out.String(), "hi-go") {
		select {
		case b, ok := <-bs:
			if !ok {
				t.Fatalf("output ended without hi-go: %q", out.String())
			}
			out.Write(b)
		case <-deadline:
			t.Fatalf("no hi-go in %q", out.String())
		}
	}
	select {
	case err := <-p.Exited():
		if err != nil {
			t.Fatalf("exit err = %v", err)
		}
	case <-time.After(5 * time.Second):
		t.Fatal("process did not exit")
	}
	if !p.Done() {
		t.Fatal("Done after exit")
	}

	if _, err := ps.Start("x", "", t.TempDir(), 80, 24, "/nonexistent/prog"); err == nil {
		t.Fatal("Start of a missing program: want error")
	}
}

func TestNotifier(t *testing.T) {
	argv := fakeBin(t, "notify-send")
	if err := (Notifier{}).Notify("título", "corpo"); err != nil {
		t.Fatal(err)
	}
	if got := calls(t, argv); !reflect.DeepEqual(got, [][]string{{"-a", "nekuroddo", "título", "corpo"}}) {
		t.Fatalf("argv = %q", got)
	}

	t.Setenv("PATH", t.TempDir())
	if err := (Notifier{}).Notify("a", "b"); err == nil {
		t.Fatal("without notify-send: want error")
	}
}

func TestOpener(t *testing.T) {
	argv := fakeBin(t, "xdg-open")
	if err := (Opener{}).OpenURL("https://x.test/a"); err != nil {
		t.Fatal(err)
	}
	// xdg-open is started, not waited for: poll for its record.
	deadline := time.Now().Add(5 * time.Second)
	for {
		if _, err := os.Stat(argv); err == nil {
			break
		}
		if time.Now().After(deadline) {
			t.Fatal("xdg-open was not run")
		}
		time.Sleep(10 * time.Millisecond)
	}
	if got := calls(t, argv); !reflect.DeepEqual(got, [][]string{{"https://x.test/a"}}) {
		t.Fatalf("argv = %q", got)
	}

	t.Setenv("PATH", t.TempDir())
	if err := (Opener{}).OpenURL("https://x.test"); err == nil {
		t.Fatal("without xdg-open: want error")
	}
}

func fixture(t *testing.T, name string) string {
	t.Helper()
	b, err := os.ReadFile(filepath.Join("..", "jira", "testdata", name))
	if err != nil {
		t.Fatal(err)
	}
	return string(b)
}

func TestJira(t *testing.T) {
	isolate(t)
	var j app.JiraClient = Jira{}
	cfg := jira.Config{Site: "acme.atlassian.net", CloudID: "c"}

	fakeClaude(t, fixture(t, "search_ok.jsonl"), 0)
	c, err := j.Fetch(cfg)
	if err != nil || len(c.Issues) == 0 || c.Issues[0].Key != "ABC-1" {
		t.Fatalf("Fetch = %+v, %v", c, err)
	}

	fakeClaude(t, fixture(t, "create_ok.jsonl"), 0)
	key, _, err := j.Create(cfg, jira.NewIssue{Project: "ABC", Type: "Task", Summary: "s"})
	if err != nil || key != "ABC-42" {
		t.Fatalf("Create = %q, %v", key, err)
	}
}

// fakeTodo installs a stand-in for the todo skill's script (NEKO_TODO_SCRIPT)
// that records its argv per call and prints out.
func fakeTodo(t *testing.T, out string) (argvFile string) {
	t.Helper()
	dir := t.TempDir()
	argvFile = filepath.Join(dir, "argv")
	outFile := filepath.Join(dir, "out")
	if err := os.WriteFile(outFile, []byte(out), 0o644); err != nil {
		t.Fatal(err)
	}
	script := "import sys\n" +
		"open(" + pyStr(argvFile) + ", 'a').write('\\x1f'.join(sys.argv[1:]) + '\\n')\n" +
		"sys.stdout.write(open(" + pyStr(outFile) + ").read())\n"
	s := filepath.Join(dir, "todo.py")
	if err := os.WriteFile(s, []byte(script), 0o644); err != nil {
		t.Fatal(err)
	}
	t.Setenv("NEKO_TODO_SCRIPT", s)
	return argvFile
}

func pyStr(s string) string { return "'" + strings.ReplaceAll(s, "'", "\\'") + "'" }

func TestTodo(t *testing.T) {
	isolate(t)
	var c app.TodoClient = Todo{}

	t.Setenv("NEKO_TODO_SCRIPT", filepath.Join(t.TempDir(), "missing.py"))
	if c.Available() {
		t.Fatal("Available with no script")
	}

	argv := fakeTodo(t, `[{"group":"g","tasks":[{"id":7,"title":"x"}]}]`)
	if !c.Available() {
		t.Fatal("not Available with a script")
	}
	gs, err := c.List()
	if err != nil || len(gs) != 1 || gs[0].Tasks[0].ID != 7 {
		t.Fatalf("List = %+v, %v", gs, err)
	}
	for _, err := range []error{
		c.Start(7), c.Done(7), c.LinkJira(7, "ABC-1"), c.AddFromJira("ABC-2", "s"), c.AddNote(7, "n"),
	} {
		if err != nil {
			t.Fatal(err)
		}
	}
	want := [][]string{
		{"list", "--format", "json"},
		{"start", "7"},
		{"done", "7"},
		{"edit", "7", "-r", "jira:ABC-1", "--add-tag", "jira"},
		{"add", "ABC-2 s", "-r", "jira:ABC-2", "-t", "jira", "-c", "@jira"},
		{"edit", "7", "--note", "n"},
	}
	if got := calls(t, argv); !reflect.DeepEqual(got, want) {
		t.Fatalf("calls = %q", got)
	}
}
