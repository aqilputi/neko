package ui

// Slice "dispatch/input": the bottom input line, the permission mode and
// launching agents (WP-2.3 moves dispatch to app/dispatch.go).

import (
	"fmt"
	"strings"

	tea "charm.land/bubbletea/v2"
	"charm.land/lipgloss/v2"

	"github.com/aqilputi/nekuroddo/internal/claude"
	"github.com/aqilputi/nekuroddo/internal/config"
	"github.com/aqilputi/nekuroddo/internal/core"
	"github.com/aqilputi/nekuroddo/internal/jira"
	"github.com/aqilputi/nekuroddo/internal/todo"
)

// permModes are the --permission-mode values tab cycles through when
// dispatching an agent ("" = whatever Claude Code's settings say).
var permModes = []string{"", "acceptEdits", "auto", "plan", "bypassPermissions"}

func permIndex(mode string) int {
	for i, p := range permModes {
		if p == mode {
			return i
		}
	}
	return 0
}

func (m *Model) ask(p inputPurpose, prompt, value string) tea.Cmd {
	m.mode = modeInput
	m.purpose = p
	m.basePrompt = prompt
	m.refreshPrompt()
	m.input.SetValue(value)
	m.input.CursorEnd()
	return m.input.Focus()
}

// dispatching reports whether the input line ends in launching an agent,
// so it shows (and tab cycles) the permission mode.
func (m *Model) dispatching() bool {
	switch m.purpose {
	case inNewDir, inNewPrompt, inDispatchDir, inJiraAgentDir:
		return true
	}
	return false
}

func (m *Model) refreshPrompt() {
	prompt := m.basePrompt
	if m.dispatching() {
		t := m.t()
		badge := lipgloss.NewStyle().Foreground(t.Muted).Render("[padrão ⇥] ")
		switch mode := permModes[m.perm]; mode {
		case "":
		case "bypassPermissions":
			badge = lipgloss.NewStyle().Foreground(t.Bg).Background(t.Error).Bold(true).Render(" ⚠ BYPASS ") + " "
		default:
			badge = lipgloss.NewStyle().Foreground(t.Accent2).Bold(true).Render("[" + mode + "] ")
		}
		prompt = badge + prompt
	}
	m.input.Prompt = prompt
	m.input.SetWidth(max(m.w-lipglossWidth(prompt)-4, 10))
}

// dispatch launches a background agent with the chosen permission mode.
// Bypass asks first unless it is the configured default (permission_mode).
// The mode then goes back to the default, so a bypass picked with tab never
// sticks around unnoticed.
func (m *Model) dispatch(dir, name, prompt string, task *todo.Task) tea.Cmd {
	mode := permModes[m.perm]
	m.perm = permIndex(m.cfg.PermissionMode)
	var extra []string
	if mode != "" {
		extra = []string{"--permission-mode", mode}
	}
	run := func() tea.Msg {
		id, err := claude.Dispatch(dir, name, prompt, extra...)
		return dispatchedMsg{id: id, task: task, err: err}
	}
	if mode == "bypassPermissions" && m.cfg.PermissionMode != "bypassPermissions" {
		m.confirmThen(fmt.Sprintf("disparar em BYPASS (sem nenhuma checagem de permissão) em %s?\nos hooks, como o guarded-access, continuam valendo", config.Tilde(dir)),
			func(*Model) tea.Cmd { return run })
		return nil
	}
	return run
}

func (m *Model) submitInput() tea.Cmd {
	val := strings.TrimSpace(m.input.Value())
	m.input.Blur()
	m.mode = modeNormal
	switch m.purpose {
	case inFilter:
		m.view.Filter = val
		m.rebuild()
	case inReply:
		if val == "" {
			return nil
		}
		for i := range m.snap.Sessions {
			if s := m.snap.Sessions[i]; s.SessionID == m.draftDir {
				return m.openPane(s, false, core.ResolveOption(s.Questions, val))
			}
		}
	case inNewDir:
		if val == "" {
			return nil
		}
		m.draftDir = config.Expand(val)
		m.draftTask = nil
		return m.ask(inNewPrompt, "prompt › ", "")
	case inJiraProject:
		return m.submitJiraProject(val)
	case inJiraAgentDir:
		if val == "" || m.draftIssue == nil {
			return nil
		}
		is := *m.draftIssue
		dir := config.Expand(val)
		return m.dispatch(dir, is.Key+" "+core.Truncate(is.Summary, 40), jira.AgentPrompt(is), nil)
	case inDispatchDir:
		if val == "" || m.draftTask == nil {
			return nil
		}
		task := *m.draftTask
		dir := config.Expand(val)
		return m.dispatch(dir, fmt.Sprintf("todo #%d %s", task.ID, core.Truncate(task.Title, 40)), todo.Prompt(task), &task)
	case inNewPrompt:
		if val == "" {
			return nil
		}
		return m.dispatch(m.draftDir, "", val, nil)
	case inRename:
		return m.submitRename(val)
	case inMove:
		return m.submitMove(val)
	case inRenameCtx:
		return m.submitRenameCtx(val)
	}
	return nil
}

func (m *Model) onDispatched(msg dispatchedMsg) tea.Cmd {
	if msg.err != nil {
		return toastCmd(msg.err.Error(), true)
	}
	cmds := []tea.Cmd{m.refresh(), toastCmd("agent "+msg.id+" disparado", false)}
	if msg.task != nil {
		t := *msg.task
		id := msg.id
		cmds = append(cmds, func() tea.Msg {
			_ = todo.Start(t.ID)
			_ = todo.AddNote(t.ID, "nekuroddo: agent "+id+" disparado (claude attach "+id+")")
			g, err := todo.List()
			return todosMsg{g, err}
		})
	}
	return tea.Batch(cmds...)
}
