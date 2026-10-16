package ui

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	tea "charm.land/bubbletea/v2"

	"github.com/aqilputi/nekuroddo/internal/app"
	"github.com/aqilputi/nekuroddo/internal/config"
)

// A fake claude that records its argv and prints a short id.
func fakeClaude(t *testing.T) string {
	dir := t.TempDir()
	log := filepath.Join(dir, "args")
	bin := filepath.Join(dir, "claude")
	os.WriteFile(bin, []byte("#!/bin/sh\necho \"$@\" > "+log+"\necho 'started abcd1234'\n"), 0o755)
	t.Setenv("NEKO_CLAUDE", bin)
	return log
}

func typeLine(m *Model, s string) {
	m.input.SetValue(s)
}

func TestDispatchPermissionModes(t *testing.T) {
	log := fakeClaude(t)
	m := New(config.Default())
	m.w, m.h = 120, 40

	// tab → acceptEdits: dispatches with the flag, no confirmation
	m.draftDir = t.TempDir()
	m.ask(inNewPrompt, "prompt › ", "")
	m.key(tea.KeyPressMsg{Code: tea.KeyTab})
	if !strings.Contains(m.input.Prompt, "acceptEdits") {
		t.Fatalf("prompt badge: %q", m.input.Prompt)
	}
	typeLine(&m, "oi")
	cmd := m.key(tea.KeyPressMsg{Code: tea.KeyEnter})
	if m.top() != nil || cmd == nil {
		t.Fatal("acceptEdits should dispatch without confirming")
	}
	if msg := cmd().(dispatchedMsg); msg.err != nil || msg.id != "abcd1234" {
		t.Fatalf("dispatch: %+v", msg)
	}
	if b, _ := os.ReadFile(log); !strings.Contains(string(b), "--bg --allow-dangerously-skip-permissions --name oi --permission-mode acceptEdits oi") {
		t.Fatalf("args: %s", b)
	}
	if m.perm != 0 {
		t.Fatal("mode must go back to the default after a dispatch")
	}

	// shift+tab from default → bypass: must ask, then pass the flag
	m.ask(inNewPrompt, "prompt › ", "")
	m.key(tea.KeyPressMsg{Code: tea.KeyTab, Mod: tea.ModShift})
	if !strings.Contains(m.input.Prompt, "BYPASS") {
		t.Fatalf("prompt badge: %q", m.input.Prompt)
	}
	typeLine(&m, "faz tudo")
	if cmd := m.key(tea.KeyPressMsg{Code: tea.KeyEnter}); cmd != nil || m.top() == nil {
		t.Fatal("bypass must open a confirmation first")
	}
	cmd = m.key(tea.KeyPressMsg{Code: 'y', Text: "y"})
	if cmd == nil {
		t.Fatal("confirming must dispatch")
	}
	cmd()
	if b, _ := os.ReadFile(log); !strings.Contains(string(b), "--permission-mode bypassPermissions faz tudo") {
		t.Fatalf("args: %s", b)
	}

	// default mode: no flag at all (Claude's own settings decide)
	m.ask(inNewPrompt, "prompt › ", "")
	typeLine(&m, "normal")
	m.key(tea.KeyPressMsg{Code: tea.KeyEnter})()
	if b, _ := os.ReadFile(log); strings.Contains(string(b), "permission-mode") {
		t.Fatalf("default should not pass a mode: %s", b)
	}
}

// With bypass as the configured default it dispatches straight away.
func TestBypassDefaultNoConfirm(t *testing.T) {
	log := fakeClaude(t)
	cfg := config.Default()
	cfg.PermissionMode = "bypassPermissions"
	m := New(cfg)
	m.w, m.h = 120, 40
	m.draftDir = t.TempDir()
	m.ask(inNewPrompt, "prompt › ", "")
	if !strings.Contains(m.input.Prompt, "BYPASS") {
		t.Fatalf("default badge: %q", m.input.Prompt)
	}
	typeLine(&m, "vai")
	cmd := m.key(tea.KeyPressMsg{Code: tea.KeyEnter})
	if cmd == nil || m.top() != nil {
		t.Fatal("default bypass should not ask")
	}
	cmd()
	if b, _ := os.ReadFile(log); !strings.Contains(string(b), "--permission-mode bypassPermissions vai") {
		t.Fatalf("args: %s", b)
	}
	if permModes[m.perm] != "bypassPermissions" {
		t.Fatal("should stay on the configured default")
	}
}

// Any key but y/enter cancels the parked bypass in the core: nothing runs.
func TestBypassCancelledByOtherKey(t *testing.T) {
	log := fakeClaude(t)
	m := New(config.Default())
	m.w, m.h = 120, 40
	m.draftDir = t.TempDir()
	m.perm = permIndex("bypassPermissions")
	m.ask(inNewPrompt, "prompt › ", "")
	typeLine(&m, "nada")
	if cmd := m.key(tea.KeyPressMsg{Code: tea.KeyEnter}); cmd != nil || m.top() == nil || len(m.app.Pending()) != 1 {
		t.Fatal("bypass parks a confirmation in the core")
	}
	if cmd := m.key(tea.KeyPressMsg{Code: 'n', Text: "n"}); cmd != nil || m.top() != nil {
		t.Fatal("n closes without dispatching")
	}
	if len(m.app.Pending()) != 0 {
		t.Fatal("the token is cancelled")
	}
	if _, err := os.Stat(log); err == nil {
		t.Fatal("claude must not run")
	}
}

// A token the core no longer knows (answered elsewhere) shows the error.
func TestConfirmTokenUnknown(t *testing.T) {
	m := New(config.Default())
	m.w, m.h = 120, 40
	m.confirmToken("q?", "gone")
	msg := m.key(tea.KeyPressMsg{Code: tea.KeyEnter})()
	if tm, ok := msg.(toastMsg); !ok || !tm.err || !strings.Contains(tm.text, "confirmação") {
		t.Fatalf("unknown token: %+v", msg)
	}
	m.confirmToken("q?", "gone")
	if m.key(tea.KeyPressMsg{Code: 'x', Text: "x"}) != nil || m.top() != nil {
		t.Fatal("cancelling an unknown token just closes")
	}
}

func TestResultMsg(t *testing.T) {
	if msg := resultMsg(app.Result{Data: app.Dispatched{ID: "a1", TaskID: 4}}, nil).(dispatchedMsg); msg.id != "a1" || msg.task == nil || msg.task.ID != 4 {
		t.Fatalf("dispatched with task: %+v", msg)
	}
	if msg := resultMsg(app.Result{Data: app.Dispatched{ID: "a2"}}, nil).(dispatchedMsg); msg.task != nil {
		t.Fatalf("dispatched without task: %+v", msg)
	}
	if msg := resultMsg(app.Result{Toast: "ok", Err: true}, nil).(toastMsg); msg.text != "ok" || !msg.err {
		t.Fatalf("toast: %+v", msg)
	}
	if resultMsg(app.Result{}, nil) != nil {
		t.Fatal("empty result: no message")
	}
}
