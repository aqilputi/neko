package claude

import (
	"errors"
	"os"
	"os/exec"
	"path/filepath"
	"reflect"
	"strconv"
	"strings"
	"testing"
	"time"
)

// fakeClaude installs a shell script as the claude binary (via NEKO_CLAUDE)
// that records its argv (NUL-separated) and working directory, prints
// out and exits with code. It returns the file the argv is recorded in.
func fakeClaude(t *testing.T, out string, code int) (argvFile string) {
	t.Helper()
	dir := t.TempDir()
	argvFile = filepath.Join(dir, "argv")
	outFile := filepath.Join(dir, "out")
	if err := os.WriteFile(outFile, []byte(out), 0o644); err != nil {
		t.Fatal(err)
	}
	script := "#!/bin/sh\n" +
		"pwd > '" + argvFile + ".cwd'\n" +
		"for a in \"$@\"; do printf '%s\\0' \"$a\"; done > '" + argvFile + "'\n" +
		"cat '" + outFile + "'\n" +
		"exit " + strconv.Itoa(code) + "\n"
	bin := filepath.Join(dir, "claude")
	if err := os.WriteFile(bin, []byte(script), 0o755); err != nil {
		t.Fatal(err)
	}
	t.Setenv("NEKO_CLAUDE", bin)
	return argvFile
}

func readArgv(t *testing.T, f string) []string {
	t.Helper()
	b, err := os.ReadFile(f)
	if err != nil {
		t.Fatalf("fake claude was not run: %v", err)
	}
	return strings.Split(strings.TrimSuffix(string(b), "\x00"), "\x00")
}

func readCwd(t *testing.T, f string) string {
	t.Helper()
	b, _ := os.ReadFile(f + ".cwd")
	return strings.TrimSpace(string(b))
}

func TestBin(t *testing.T) {
	t.Setenv("NEKO_CLAUDE", "")
	if Bin() != "claude" {
		t.Fatalf("default bin = %q", Bin())
	}
	t.Setenv("NEKO_CLAUDE", "/opt/x/claude")
	if Bin() != "/opt/x/claude" {
		t.Fatalf("override bin = %q", Bin())
	}
	if got := AttachArgv("abcd1234"); !reflect.DeepEqual(got, []string{"/opt/x/claude", "attach", "abcd1234"}) {
		t.Fatalf("AttachArgv = %v", got)
	}
}

func TestListAgents(t *testing.T) {
	argv := fakeClaude(t, `[
	 {"id":"a1b2c3d4","pid":42,"cwd":"/w","kind":"background","startedAt":1760000000000,"sessionId":"s-1","name":"gato","state":"running"},
	 {"pid":7,"cwd":"/x","kind":"interactive","sessionId":"s-2","status":"busy","unknown":true}
	]`, 0)
	as, err := ListAgents()
	if err != nil {
		t.Fatal(err)
	}
	if got := readArgv(t, argv); !reflect.DeepEqual(got, []string{"agents", "--json", "--all"}) {
		t.Fatalf("argv = %v", got)
	}
	want := []Agent{
		{ID: "a1b2c3d4", PID: 42, Cwd: "/w", Kind: "background", StartedAt: 1760000000000, SessionID: "s-1", Name: "gato", State: "running"},
		{PID: 7, Cwd: "/x", Kind: "interactive", SessionID: "s-2", Status: "busy"},
	}
	if !reflect.DeepEqual(as, want) {
		t.Fatalf("agents = %+v", as)
	}
}

func TestListAgentsErrors(t *testing.T) {
	t.Run("exit status", func(t *testing.T) {
		fakeClaude(t, "", 3)
		_, err := ListAgents()
		var ee *exec.ExitError
		if !errors.As(err, &ee) || ee.ExitCode() != 3 {
			t.Fatalf("err = %v, want wrapped *exec.ExitError(3)", err)
		}
		if !strings.HasPrefix(err.Error(), "claude agents --json:") {
			t.Fatalf("err = %q", err)
		}
	})
	t.Run("bad json", func(t *testing.T) {
		fakeClaude(t, "not json", 0)
		_, err := ListAgents()
		if err == nil || !strings.Contains(err.Error(), "claude agents --json:") {
			t.Fatalf("err = %v", err)
		}
	})
	t.Run("missing binary", func(t *testing.T) {
		t.Setenv("NEKO_CLAUDE", filepath.Join(t.TempDir(), "nope"))
		_, err := ListAgents()
		if !errors.Is(err, os.ErrNotExist) {
			t.Fatalf("err = %v, want wrapped ErrNotExist", err)
		}
	})
}

func TestDispatch(t *testing.T) {
	long := "Revise o módulo de faturamento e escreva testes para todos os casos de borda\nsegunda linha"
	cases := []struct {
		name, sessName, prompt string
		extra                  []string
		wantArgs               []string
	}{
		{
			name: "explicit name", sessName: "meu-gato", prompt: "faz x",
			wantArgs: []string{"--bg", AllowBypass, "--name", "meu-gato", "faz x"},
		},
		{
			name: "named after prompt", prompt: long,
			wantArgs: []string{"--bg", AllowBypass, "--name", PromptName(long), long},
		},
		{
			name: "empty prompt has no name", prompt: "",
			wantArgs: []string{"--bg", AllowBypass, ""},
		},
		{
			name: "extra flags go before the prompt", sessName: "n", prompt: "p", extra: []string{"--model", "opus"},
			wantArgs: []string{"--bg", AllowBypass, "--name", "n", "--model", "opus", "p"},
		},
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			argv := fakeClaude(t, "Started background session deadbeef (use claude attach deadbeef)\n", 0)
			dir := t.TempDir()
			id, err := Dispatch(dir, c.sessName, c.prompt, c.extra...)
			if err != nil || id != "deadbeef" {
				t.Fatalf("id=%q err=%v", id, err)
			}
			if got := readArgv(t, argv); !reflect.DeepEqual(got, c.wantArgs) {
				t.Fatalf("argv = %q\nwant  %q", got, c.wantArgs)
			}
			if got, _ := filepath.EvalSymlinks(readCwd(t, argv)); got != mustEval(t, dir) {
				t.Fatalf("cwd = %q, want %q", got, dir)
			}
		})
	}
}

func mustEval(t *testing.T, p string) string {
	t.Helper()
	r, err := filepath.EvalSymlinks(p)
	if err != nil {
		t.Fatal(err)
	}
	return r
}

func TestDispatchErrors(t *testing.T) {
	t.Run("failure carries output and exit error", func(t *testing.T) {
		fakeClaude(t, "  not logged in  \n", 1)
		_, err := Dispatch(t.TempDir(), "n", "p")
		var ee *exec.ExitError
		if !errors.As(err, &ee) {
			t.Fatalf("err = %v, want wrapped *exec.ExitError", err)
		}
		if !strings.HasPrefix(err.Error(), "claude --bg: ") || !strings.HasSuffix(err.Error(), ": not logged in") {
			t.Fatalf("err = %q", err)
		}
	})
	t.Run("no id in output", func(t *testing.T) {
		fakeClaude(t, "started, id XYZ\n", 0)
		_, err := Dispatch(t.TempDir(), "n", "p")
		if err == nil || err.Error() != "claude --bg: no id in output: started, id XYZ" {
			t.Fatalf("err = %v", err)
		}
	})
}

func TestResumeInBackground(t *testing.T) {
	argv := fakeClaude(t, "resumed as 0123abcd\n", 0)
	id, err := ResumeInBackground(t.TempDir(), "sess-uuid")
	if err != nil || id != "0123abcd" {
		t.Fatalf("id=%q err=%v", id, err)
	}
	if got, want := readArgv(t, argv), []string{"--bg", AllowBypass, "--resume", "sess-uuid"}; !reflect.DeepEqual(got, want) {
		t.Fatalf("argv = %q", got)
	}

	fakeClaude(t, "boom\n", 2)
	_, err = ResumeInBackground(t.TempDir(), "s")
	var ee *exec.ExitError
	if !errors.As(err, &ee) || ee.ExitCode() != 2 || !strings.HasPrefix(err.Error(), "claude --bg --resume: ") || !strings.HasSuffix(err.Error(), ": boom") {
		t.Fatalf("err = %v", err)
	}

	fakeClaude(t, "nothing here", 0)
	if _, err := ResumeInBackground(t.TempDir(), "s"); err == nil || !strings.Contains(err.Error(), "no id in output: nothing here") {
		t.Fatalf("err = %v", err)
	}
}

func TestStopRemove(t *testing.T) {
	for _, c := range []struct {
		fn   func(string) error
		verb string
	}{{Stop, "stop"}, {Remove, "rm"}} {
		t.Run(c.verb, func(t *testing.T) {
			argv := fakeClaude(t, "", 0)
			if err := c.fn("abcd1234"); err != nil {
				t.Fatal(err)
			}
			if got := readArgv(t, argv); !reflect.DeepEqual(got, []string{c.verb, "abcd1234"}) {
				t.Fatalf("argv = %q", got)
			}

			fakeClaude(t, "no such session\n", 1)
			err := c.fn("abcd1234")
			var ee *exec.ExitError
			if !errors.As(err, &ee) {
				t.Fatalf("err = %v, want wrapped *exec.ExitError", err)
			}
			if want := "claude " + c.verb + " abcd1234: "; !strings.HasPrefix(err.Error(), want) || !strings.HasSuffix(err.Error(), ": no such session") {
				t.Fatalf("err = %q", err)
			}
		})
	}
}

func TestProjectsDirAndTranscriptPath(t *testing.T) {
	cfg := t.TempDir()
	t.Setenv("CLAUDE_CONFIG_DIR", cfg)
	if ProjectsDir() != filepath.Join(cfg, "projects") {
		t.Fatalf("ProjectsDir = %q", ProjectsDir())
	}

	home := t.TempDir()
	t.Setenv("CLAUDE_CONFIG_DIR", "")
	t.Setenv("HOME", home)
	if ProjectsDir() != filepath.Join(home, ".claude", "projects") {
		t.Fatalf("ProjectsDir = %q", ProjectsDir())
	}
	t.Setenv("CLAUDE_CONFIG_DIR", cfg)

	// The cwd maps to a folder with every non-alphanumeric replaced by '-'.
	direct := filepath.Join(cfg, "projects", "-home-u-my-repo-v2", "s1.jsonl")
	moved := filepath.Join(cfg, "projects", "-elsewhere", "s2.jsonl")
	for _, p := range []string{direct, moved} {
		os.MkdirAll(filepath.Dir(p), 0o755)
		os.WriteFile(p, []byte("{}\n"), 0o644)
	}
	cases := []struct{ cwd, sid, want string }{
		{"/home/u/my_repo.v2", "s1", direct},
		{"/somewhere/else", "s2", moved}, // glob fallback
		{"/home/u/my_repo.v2", "missing", ""},
	}
	for _, c := range cases {
		if got := TranscriptPath(c.cwd, c.sid); got != c.want {
			t.Errorf("TranscriptPath(%q,%q) = %q, want %q", c.cwd, c.sid, got, c.want)
		}
	}

	old := time.Now().Add(-time.Hour)
	os.Chtimes(moved, old, old)
	got := RecentTranscripts(time.Now().Add(-time.Minute))
	if !reflect.DeepEqual(got, []string{direct}) {
		t.Fatalf("RecentTranscripts = %v", got)
	}
	if got := RecentTranscripts(time.Now().Add(time.Hour)); len(got) != 0 {
		t.Fatalf("future since: %v", got)
	}
}
