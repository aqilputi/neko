// Package term embeds a real program (the `claude` binary) inside the TUI:
// it runs in a PTY, its output is fed to a virtual terminal emulator, and the
// emulator's screen is rendered as a pane. Keys typed while the pane is
// focused are forwarded to the program unchanged, so Claude Code behaves
// exactly as it does in a normal terminal — and stays whatever version is
// installed.
//
// A pane has two halves: Process (process.go) owns the PTY, fans raw output
// out to subscribers and signals new output — no Bubble Tea, so the core can
// drive it for any client; Screen (screen.go) is the x/vt emulator the TUI
// draws and types into. Pane joins them for the TUI.
package term

// Pane is a program in a PTY (Process) shown on an emulator (Screen).
// ID and Title (the label the TUI shows) shadow Process.ID/Process.Title;
// WinTitle is the title the program set.
type Pane struct {
	ID    string
	Title string
	*Process
	*Screen
}

// Start runs argv in a PTY of size w×h inside dir, shown on a Screen.
func Start(id, title, dir string, w, h int, argv ...string) (*Pane, error) {
	scr := NewScreen(w, h)
	proc, err := startProcess(id, dir, w, h, scr.Feed, argv...)
	if err != nil {
		return nil, err
	}
	scr.Pipe(proc)
	return &Pane{ID: id, Title: title, Process: proc, Screen: scr}, nil
}

// Resize resizes the screen and, when that changed its size, the PTY.
func (p *Pane) Resize(w, h int) {
	if p.Screen.Resize(w, h) {
		p.Process.Resize(w, h)
	}
}

// WinTitle is the window title the program last set.
func (p *Pane) WinTitle() string { return p.Process.Title() }
