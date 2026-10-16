// Package wire is the composition root: thin adapters that bind the real
// packages (claude, store, term, groups, events, jira, todo, notify-send,
// xdg-open) to the ports of internal/app. It holds no logic of its own.
package wire

import (
	"os/exec"
	"time"

	"github.com/aqilputi/nekuroddo/internal/app"
	"github.com/aqilputi/nekuroddo/internal/claude"
	"github.com/aqilputi/nekuroddo/internal/config"
	"github.com/aqilputi/nekuroddo/internal/events"
	"github.com/aqilputi/nekuroddo/internal/groups"
	"github.com/aqilputi/nekuroddo/internal/jira"
	"github.com/aqilputi/nekuroddo/internal/store"
	"github.com/aqilputi/nekuroddo/internal/term"
	"github.com/aqilputi/nekuroddo/internal/todo"
)

// Default binds every port to the real world.
func Default(cfg config.Config) app.Ports {
	return app.Ports{
		Clock:    Clock{},
		Claude:   Claude{},
		Sessions: store.New(cfg),
		Groups:   Groups{},
		Events:   &events.Tail{},
		Panes:    Panes{},
		Notify:   Notifier{},
		Open:     Opener{},
		Jira:     Jira{},
		Todo:     Todo{},
	}
}

// Clock is the wall clock.
type Clock struct{}

func (Clock) Now() time.Time { return time.Now() }

// Claude is the Claude Code CLI (package claude; NEKO_CLAUDE overrides the binary).
type Claude struct{}

func (Claude) ListAgents() ([]claude.Agent, error) { return claude.ListAgents() }
func (Claude) Dispatch(dir, name, prompt string, extra ...string) (string, error) {
	return claude.Dispatch(dir, name, prompt, extra...)
}
func (Claude) ResumeInBackground(dir, sessionID string) (string, error) {
	return claude.ResumeInBackground(dir, sessionID)
}
func (Claude) Stop(id string) error                    { return claude.Stop(id) }
func (Claude) Remove(id string) error                  { return claude.Remove(id) }
func (Claude) AttachArgv(id string) []string           { return claude.AttachArgv(id) }
func (Claude) FetchUsage() (claude.UsageReport, error) { return claude.FetchUsage() }

// Groups is groups.json plus the per-session mail queue (package groups).
type Groups struct{}

func (Groups) Load() groups.File                           { return groups.Load() }
func (Groups) Save(f groups.File) error                    { return f.Save() }
func (Groups) Queue(sessionID string, n groups.Note) error { return groups.Queue(sessionID, n) }

// Panes starts programs in a PTY (term.Process, the half of a pane with no
// Bubble Tea). The title is only the TUI's label, so the process ignores it.
type Panes struct{}

func (Panes) Start(id, _, dir string, cols, rows int, argv ...string) (app.Pane, error) {
	p, err := term.StartProcess(id, dir, cols, rows, argv...)
	if err != nil {
		return nil, err // never a typed nil inside the interface
	}
	return p, nil
}

// Notifier is notify-send. A missing notify-send is an error the caller may
// ignore (the desktop notification is best effort).
type Notifier struct{}

func (Notifier) Notify(title, body string) error {
	p, err := exec.LookPath("notify-send")
	if err != nil {
		return err
	}
	return exec.Command(p, "-a", "nekuroddo", title, body).Run()
}

// Opener is xdg-open. It returns once the program started; it is reaped in
// the background.
type Opener struct{}

func (Opener) OpenURL(u string) error {
	cmd := exec.Command("xdg-open", u)
	if err := cmd.Start(); err != nil {
		return err
	}
	go func() { _ = cmd.Wait() }()
	return nil
}

// Jira is package jira (a headless claude calling the Atlassian connector).
type Jira struct{}

func (Jira) Cached() jira.Cache                        { return jira.LoadCache() }
func (Jira) Fetch(cfg jira.Config) (jira.Cache, error) { return jira.Fetch(cfg) }
func (Jira) Create(cfg jira.Config, n jira.NewIssue) (string, float64, error) {
	return jira.Create(cfg, n)
}

// Todo is the todo skill's script (package todo; NEKO_TODO_SCRIPT overrides it).
type Todo struct{}

func (Todo) Available() bool                       { return todo.Available() }
func (Todo) List() ([]todo.Group, error)           { return todo.List() }
func (Todo) Start(id int) error                    { return todo.Start(id) }
func (Todo) Done(id int) error                     { return todo.Done(id) }
func (Todo) LinkJira(id int, key string) error     { return todo.LinkJira(id, key) }
func (Todo) AddFromJira(key, summary string) error { return todo.AddFromJira(key, summary) }
func (Todo) AddNote(id int, text string) error     { return todo.AddNote(id, text) }
