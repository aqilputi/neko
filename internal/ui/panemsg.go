package ui

import (
	tea "charm.land/bubbletea/v2"

	"github.com/aqilputi/nekuroddo/internal/term"
)

// paneOutputMsg says pane `ID` has new output to render.
type paneOutputMsg struct{ ID string }

// paneExitMsg says the program in pane `ID` exited.
type paneExitMsg struct {
	ID  string
	Err error
}

// listenPane resolves on the pane's next output signal (already coalesced to
// ~30fps by term.Process) or on its exit. Re-issue it after every
// paneOutputMsg.
func listenPane(p *term.Pane) tea.Cmd {
	return func() tea.Msg {
		select {
		case err := <-p.Exited():
			return paneExitMsg{ID: p.ID, Err: err}
		case <-p.Output():
			return paneOutputMsg{ID: p.ID}
		}
	}
}
