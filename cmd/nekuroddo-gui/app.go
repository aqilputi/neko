package main

import (
	"context"
	"errors"
	"fmt"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"time"

	"github.com/aqilputi/nekuroddo/internal/guihost"
)

// appBrowsers are the Chromium-family browsers whose --app mode gives the
// GUI a window of its own (no tabs, no URL bar), most common first. Firefox
// has no app mode.
var appBrowsers = []string{
	"google-chrome-stable", "google-chrome", "chromium", "chromium-browser",
	"brave", "brave-browser", "microsoft-edge-stable", "vivaldi-stable",
}

// appHandoff: a browser that exits sooner handed the window to an
// instance already running with the same profile, so nkg keeps serving.
var appHandoff = 3 * time.Second

// findAppBrowser is $NKG_BROWSER, else the first app-capable browser found.
func findAppBrowser(lookPath func(string) (string, error), override string) (string, error) {
	if override != "" {
		return override, nil
	}
	for _, b := range appBrowsers {
		if p, err := lookPath(b); err == nil {
			return p, nil
		}
	}
	return "", errors.New("nenhum navegador da família Chromium (Chrome, Chromium, Brave, Edge, Vivaldi) para abrir como app; use --browser, ou instale o webkitgtk-6.0 para a janela nativa (make build-gui)")
}

// appArgs opens url as an app window with nekuroddo's own profile, so it
// is a separate process from the user's browser (and its own taskbar entry).
func appArgs(url, profile string) []string {
	return []string{
		"--app=" + url, "--user-data-dir=" + profile, "--class=nekuroddo",
		"--no-first-run", "--no-default-browser-check", "--window-size=1280,800",
	}
}

// appProfile is $XDG_DATA_HOME/nekuroddo/app (~/.local/share/nekuroddo/app).
func appProfile() string {
	base := os.Getenv("XDG_DATA_HOME")
	if base == "" {
		home, _ := os.UserHomeDir()
		base = filepath.Join(home, ".local", "share")
	}
	return filepath.Join(base, "nekuroddo", "app")
}

// runApp is --app: the GUI in its own browser window; nkg ends when the
// window closes (or ctx ends).
func runApp(ctx context.Context, gh *guihost.Server, stdout, stderr io.Writer) int {
	bin, err := findAppBrowser(exec.LookPath, os.Getenv("NKG_BROWSER"))
	if err != nil {
		fmt.Fprintln(stderr, "nkg:", err)
		return 1
	}
	profile := appProfile()
	_ = os.MkdirAll(profile, 0o700)
	cmd := exec.CommandContext(ctx, bin, appArgs(gh.LaunchURL(), profile)...)
	cmd.Stdout, cmd.Stderr = io.Discard, io.Discard // browsers are chatty
	start := time.Now()
	if err := cmd.Start(); err != nil {
		fmt.Fprintln(stderr, "nkg: abrir a janela:", err)
		return 1
	}
	exited := make(chan error, 1)
	go func() { exited <- cmd.Wait() }()
	select {
	case <-ctx.Done():
		return 0
	case <-gh.Done():
		if err := gh.Err(); err != nil {
			fmt.Fprintln(stderr, "nkg:", err)
			return 1
		}
		return 0
	case err := <-exited:
		if time.Since(start) >= appHandoff {
			return 0 // the user closed the window
		}
		if err != nil { // died at once: a broken browser, not a handoff
			fmt.Fprintln(stderr, "nkg: a janela não abriu:", err)
			return 1
		}
	}
	// handed to a running instance: that window lives on, keep serving
	fmt.Fprintf(stdout, "nkg: a janela abriu numa instância que já rodava; servindo até Ctrl+C\n")
	select {
	case <-ctx.Done():
	case <-gh.Done():
	}
	return 0
}
