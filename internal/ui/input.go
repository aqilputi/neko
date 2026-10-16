package ui

// Slice "dispatch/input": the bottom input line, the permission mode and
// launching agents (WP-2.3 moves dispatch to app/dispatch.go).

import (
	"fmt"
	"strings"

	"charm.land/bubbles/v2/textinput"
	tea "charm.land/bubbletea/v2"
	"charm.land/lipgloss/v2"

	"github.com/aqilputi/nekuroddo/internal/app"
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

// dispatch launches a background agent (app.Dispatch) with the chosen
// permission mode. Bypass asks first unless it is the configured default:
// the core parks it and hands back a token the dialog answers. The mode then
// goes back to the default, so a bypass picked with tab never sticks around
// unnoticed.
func (m *Model) dispatch(dir, name, prompt string, task *todo.Task) tea.Cmd {
	req := app.DispatchReq{Dir: dir, Name: name, Prompt: prompt, PermMode: permModes[m.perm]}
	if task != nil {
		req.TaskID = task.ID
	}
	m.perm = permIndex(m.cfg.PermissionMode)
	a := m.app
	if a.DispatchNeedsConfirm(req.PermMode) {
		// only parks the confirmation: returns at once
		res, err := a.Dispatch(req)
		if err == nil && res.Pending != nil {
			m.confirmToken(res.Pending.Question, res.Pending.Token)
			return nil
		}
		return func() tea.Msg { return resultMsg(res, err) }
	}
	return func() tea.Msg {
		res, err := a.Dispatch(req)
		d, _ := res.Data.(app.Dispatched)
		return dispatchedMsg{id: d.ID, task: task, err: err}
	}
}

// resultMsg turns what a core command returned in the background into the
// message the update loop reacts to: a dispatch, an error or a toast.
func resultMsg(res app.Result, err error) tea.Msg {
	if err != nil {
		return toastMsg{text: err.Error(), err: true}
	}
	if d, ok := res.Data.(app.Dispatched); ok {
		var task *todo.Task
		if d.TaskID > 0 {
			task = &todo.Task{ID: d.TaskID}
		}
		return dispatchedMsg{id: d.ID, task: task}
	}
	if res.Toast != "" {
		return toastMsg{text: res.Toast, err: res.Err}
	}
	return nil
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

// onDispatched announces the agent; the core already started and noted the
// todo it works on, so the list is reloaded to show it.
func (m *Model) onDispatched(msg dispatchedMsg) tea.Cmd {
	if msg.err != nil {
		return toastCmd(msg.err.Error(), true)
	}
	cmds := []tea.Cmd{m.refresh(), toastCmd("agent "+msg.id+" disparado", false)}
	if msg.task != nil {
		cmds = append(cmds, todosCmd(m.app.LoadTodos)) // publishes todos.changed
	}
	return tea.Batch(cmds...)
}

// inputSlice is the bottom input line and the next dispatch's settings.
type inputSlice struct {
	input      textinput.Model
	purpose    inputPurpose
	basePrompt string // the input prompt without the permission badge
	perm       int    // index into permModes for the next dispatch
	draftDir   string
	draftTask  *todo.Task
}

func newInputSlice(cfg config.Config) inputSlice {
	ti := textinput.New()
	ti.Prompt = "› "
	ti.CharLimit = 4000
	return inputSlice{input: ti, perm: permIndex(cfg.PermissionMode)}
}
