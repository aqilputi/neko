package main

import (
	"context"
	"errors"
	"io"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"
)

func TestFindAppBrowser(t *testing.T) {
	have := func(names ...string) func(string) (string, error) {
		return func(n string) (string, error) {
			for _, x := range names {
				if x == n {
					return "/usr/bin/" + n, nil
				}
			}
			return "", errors.New("not found")
		}
	}
	for _, c := range []struct {
		name, env string
		look      func(string) (string, error)
		want      string
		err       bool
	}{
		{"chrome first", "", have("chromium", "google-chrome-stable"), "/usr/bin/google-chrome-stable", false},
		{"chromium", "", have("chromium"), "/usr/bin/chromium", false},
		{"brave", "", have("brave"), "/usr/bin/brave", false},
		{"override wins", "/opt/x/browser", have("google-chrome-stable"), "/opt/x/browser", false},
		{"firefox has no app mode", "", have("firefox"), "", true},
	} {
		got, err := findAppBrowser(c.look, c.env)
		if (err != nil) != c.err || got != c.want {
			t.Errorf("%s: %q %v", c.name, got, err)
		}
	}
}

func TestAppArgs(t *testing.T) {
	args := appArgs("http://127.0.0.1:9/?nk=k", "/p")
	for _, want := range []string{"--app=http://127.0.0.1:9/?nk=k", "--user-data-dir=/p", "--class=nekuroddo", "--no-first-run"} {
		if !strings.Contains(strings.Join(args, " "), want) {
			t.Errorf("args lack %q: %v", want, args)
		}
	}
}

// fakeAppBrowser is a "browser" that writes its argv and stays open for
// stay (a window the user closes after a while), then exits.
func fakeAppBrowser(t *testing.T, stay string) (bin, argv string) {
	t.Helper()
	d := t.TempDir()
	bin, argv = filepath.Join(d, "browser"), filepath.Join(d, "argv")
	script := "#!/bin/sh\nfor a in \"$@\"; do echo \"$a\"; done > " + argv + "\nsleep " + stay + "\n"
	if err := os.WriteFile(bin, []byte(script), 0o755); err != nil {
		t.Fatal(err)
	}
	return bin, argv
}

// --app: the page in its own browser window (no tabs, no URL bar, its own
// profile); closing the window ends nkg.
func TestAppModeRunsUntilTheWindowCloses(t *testing.T) {
	isolate(t)
	bin, argv := fakeAppBrowser(t, "1")
	t.Setenv("NKG_BROWSER", bin)
	old := appHandoff
	appHandoff = 100 * time.Millisecond
	t.Cleanup(func() { appHandoff = old })

	done := make(chan int, 1)
	go func() {
		done <- run(context.Background(), []string{"--app", "--demo", "--assets", distDir(t)}, io.Discard, io.Discard)
	}()
	select {
	case c := <-done:
		if c != 0 {
			t.Fatalf("run = %d", c)
		}
	case <-time.After(15 * time.Second):
		t.Fatal("nkg did not end when the window closed")
	}
	b, err := os.ReadFile(argv)
	if err != nil {
		t.Fatal(err)
	}
	got := string(b)
	if !strings.Contains(got, "--app=http://127.0.0.1:") || !strings.Contains(got, "/?nk=") {
		t.Fatalf("not opened as an app on the one-time link: %s", got)
	}
	profile := filepath.Join(os.Getenv("XDG_DATA_HOME"), "nekuroddo", "app")
	if !strings.Contains(got, "--user-data-dir="+profile) {
		t.Fatalf("own profile %s expected: %s", profile, got)
	}
}

// A binary with no native window (built without -tags wails) opens as an
// app by default instead of failing.
func TestNoWindowDefaultsToApp(t *testing.T) {
	if hasWindow {
		t.Skip("built with the native window")
	}
	isolate(t)
	bin, argv := fakeAppBrowser(t, "0")
	t.Setenv("NKG_BROWSER", bin)
	old := appHandoff
	appHandoff = 0 // the fake window "closes" at once: that is a close, not a handoff
	t.Cleanup(func() { appHandoff = old })
	if c := run(context.Background(), []string{"--demo", "--assets", distDir(t)}, io.Discard, io.Discard); c != 0 {
		t.Fatalf("run = %d", c)
	}
	if b, _ := os.ReadFile(argv); !strings.Contains(string(b), "--app=") {
		t.Fatalf("default should be app mode: %s", b)
	}
}

// A browser that dies at once with an error is a failure, not a handoff to
// a running instance (which would keep nkg serving a window that isn't there).
func TestAppModeBrowserFails(t *testing.T) {
	isolate(t)
	d := t.TempDir()
	bin := filepath.Join(d, "browser")
	if err := os.WriteFile(bin, []byte("#!/bin/sh\nexit 3\n"), 0o755); err != nil {
		t.Fatal(err)
	}
	t.Setenv("NKG_BROWSER", bin)
	var errb strings.Builder
	if c := run(context.Background(), []string{"--app", "--demo", "--assets", distDir(t)}, io.Discard, &errb); c != 1 || !strings.Contains(errb.String(), "não abriu") {
		t.Fatalf("run = %d, stderr %q", c, errb.String())
	}
}

// No Chromium-family browser: a clear error naming --browser as the way out.
func TestAppModeWithoutBrowser(t *testing.T) {
	isolate(t)
	t.Setenv("NKG_BROWSER", "")
	t.Setenv("PATH", t.TempDir())
	var errb strings.Builder
	if c := run(context.Background(), []string{"--app", "--demo", "--assets", distDir(t)}, io.Discard, &errb); c != 1 || !strings.Contains(errb.String(), "--browser") {
		t.Fatalf("run = %d, stderr %q", c, errb.String())
	}
}
