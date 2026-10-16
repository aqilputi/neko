package tui

import (
	"testing"
	"time"

	tea "charm.land/bubbletea/v2"

	"github.com/aqilputi/nekuroddo/internal/term"
)

// runCmd resolves a command with a deadline.
func runCmd(t *testing.T, c tea.Cmd) tea.Msg {
	t.Helper()
	ch := make(chan tea.Msg, 1)
	go func() { ch <- c() }()
	select {
	case m := <-ch:
		return m
	case <-time.After(5 * time.Second):
		t.Fatal("command did not resolve")
		return nil
	}
}

// listenPane reports output, then the exit with its error.
func TestListenPaneOutputThenExit(t *testing.T) {
	p, err := term.Start("pm1", "pm", "/", 20, 5, "/bin/sh", "-c", "printf oi; read x; exit 3")
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(p.Close)
	if m, ok := runCmd(t, listenPane(p)).(paneOutputMsg); !ok || m.ID != "pm1" {
		t.Fatalf("first message %#v, want paneOutputMsg{pm1}", m)
	}
	p.Type("\r")
	for i := 0; i < 100; i++ {
		switch m := runCmd(t, listenPane(p)).(type) {
		case paneOutputMsg:
		case paneExitMsg:
			if m.ID != "pm1" || m.Err == nil {
				t.Fatalf("exit %#v, want pm1 with an error", m)
			}
			return
		default:
			t.Fatalf("unexpected %T", m)
		}
	}
	t.Fatal("no paneExitMsg")
}
