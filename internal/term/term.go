// Package term embeds a real program (the `claude` binary) inside the TUI:
// it runs in a PTY, its output is fed to a virtual terminal emulator, and the
// emulator's screen is rendered as a pane. Keys typed while the pane is
// focused are forwarded to the program unchanged, so Claude Code behaves
// exactly as it does in a normal terminal — and stays whatever version is
// installed.
package term

import (
	"io"
	"os"
	"os/exec"
	"sync"
	"syscall"
	"time"

	tea "charm.land/bubbletea/v2"
	uv "github.com/charmbracelet/ultraviolet"
	"github.com/charmbracelet/x/ansi"
	"github.com/charmbracelet/x/vt"
	"github.com/creack/pty"
)

// OutputMsg says pane `ID` has new output to render.
type OutputMsg struct{ ID string }

// ExitMsg says the program in pane `ID` exited.
type ExitMsg struct {
	ID  string
	Err error
}

type Pane struct {
	ID    string
	Title string

	cmd  *exec.Cmd
	ptmx *os.File
	emu  *vt.SafeEmulator

	mu     sync.Mutex // guards done, title, clips, scroll
	drawMu sync.Mutex // SafeEmulator does not lock Draw: guard Write vs Draw
	dirty  chan struct{}
	exited chan error
	done   bool

	osc   oscFilter
	title string   // last window title the program set (Claude: "✳ <task>")
	clips []string // clipboard writes (OSC 52) waiting to reach the real terminal
	mouse map[ansi.Mode]bool
	// scroll is how many lines the view is scrolled back into the emulator's
	// scrollback (classic renderer only; fullscreen apps scroll themselves).
	scroll int
}

// Start runs argv in a PTY of size w×h inside dir.
func Start(id, title, dir string, w, h int, argv ...string) (*Pane, error) {
	cmd := exec.Command(argv[0], argv[1:]...)
	cmd.Dir = dir
	cmd.Env = append(os.Environ(), "TERM=xterm-256color", "COLORTERM=truecolor")
	ptmx, err := pty.StartWithSize(cmd, &pty.Winsize{Cols: uint16(max(w, 10)), Rows: uint16(max(h, 3))})
	if err != nil {
		return nil, err
	}
	p := &Pane{
		ID: id, Title: title, cmd: cmd, ptmx: ptmx,
		emu:    vt.NewSafeEmulator(max(w, 10), max(h, 3)),
		dirty:  make(chan struct{}, 1),
		exited: make(chan error, 1),
		mouse:  map[ansi.Mode]bool{},
	}
	p.osc.onTitle = func(t string) { p.mu.Lock(); p.title = t; p.mu.Unlock() }
	p.osc.onClipboard = func(t string) { p.mu.Lock(); p.clips = append(p.clips, t); p.mu.Unlock() }
	p.emu.SetCallbacks(vt.Callbacks{
		EnableMode:  func(m ansi.Mode) { p.setMouseMode(m, true) },
		DisableMode: func(m ansi.Mode) { p.setMouseMode(m, false) },
	})
	// program → emulator
	go func() {
		buf := make([]byte, 32*1024)
		for {
			n, err := ptmx.Read(buf)
			if n > 0 {
				clean := p.osc.filter(buf[:n])
				p.drawMu.Lock()
				_, _ = p.emu.Write(clean)
				p.drawMu.Unlock()
				p.markDirty()
			}
			if err != nil {
				return
			}
		}
	}()
	// emulator replies (cursor position reports, DA, …) and keys → program
	go func() { _, _ = io.Copy(ptmx, p.emu) }()
	go func() {
		err := cmd.Wait()
		p.mu.Lock()
		p.done = true
		p.mu.Unlock()
		p.exited <- err
		p.markDirty()
	}()
	return p, nil
}

func (p *Pane) markDirty() {
	select {
	case p.dirty <- struct{}{}:
	default:
	}
}

// Listen returns a command that resolves on the next output burst (coalesced
// to ~30fps) or on exit. Re-issue it after every OutputMsg.
func (p *Pane) Listen() tea.Cmd {
	return func() tea.Msg {
		select {
		case err := <-p.exited:
			return ExitMsg{ID: p.ID, Err: err}
		case <-p.dirty:
			time.Sleep(33 * time.Millisecond)
			return OutputMsg{ID: p.ID}
		}
	}
}

func (p *Pane) Done() bool {
	p.mu.Lock()
	defer p.mu.Unlock()
	return p.done
}

func (p *Pane) Resize(w, h int) {
	w, h = max(w, 10), max(h, 3)
	if p.emu.Width() == w && p.emu.Height() == h {
		return
	}
	p.drawMu.Lock()
	p.emu.Resize(w, h)
	p.drawMu.Unlock()
	_ = pty.Setsize(p.ptmx, &pty.Winsize{Cols: uint16(w), Rows: uint16(h)})
}

// SendKey forwards a key press to the program (and jumps back to the live
// view if it was scrolled back).
func (p *Pane) SendKey(k tea.KeyPressMsg) {
	p.mu.Lock()
	p.scroll = 0
	p.mu.Unlock()
	k, raw := encodeKey(k) // fills the gaps of x/vt's key encoder
	if raw != "" {
		p.emu.SendText(raw)
		return
	}
	p.emu.SendKey(uv.KeyPressEvent(k))
}

func (p *Pane) setMouseMode(m ansi.Mode, on bool) {
	switch m {
	case ansi.ModeMouseX10, ansi.ModeMouseNormal, ansi.ModeMouseHighlight, ansi.ModeMouseButtonEvent, ansi.ModeMouseAnyEvent:
		p.mu.Lock()
		p.mouse[m] = on
		p.mu.Unlock()
	}
}

// WantsMouse reports whether the program turned on mouse reporting (Claude
// Code's fullscreen renderer does: it scrolls and selects on its own).
func (p *Pane) WantsMouse() bool {
	p.mu.Lock()
	defer p.mu.Unlock()
	for _, on := range p.mouse {
		if on {
			return true
		}
	}
	return false
}

// WinTitle is the window title the program last set.
func (p *Pane) WinTitle() string {
	p.mu.Lock()
	defer p.mu.Unlock()
	return p.title
}

// TakeClipboard returns and clears the clipboard writes the program made.
func (p *Pane) TakeClipboard() []string {
	p.mu.Lock()
	defer p.mu.Unlock()
	c := p.clips
	p.clips = nil
	return c
}

// Cursor is where the program's cursor is, relative to the pane, and whether
// it should be shown (hidden while scrolled back).
func (p *Pane) Cursor() (x, y int, visible bool) {
	p.drawMu.Lock()
	pos, hidden := p.emu.CursorPosition(), p.emu.CursorHidden()
	p.drawMu.Unlock()
	p.mu.Lock()
	scrolled := p.scroll > 0
	p.mu.Unlock()
	return pos.X, pos.Y, !hidden && !scrolled
}

// Scroll moves the view n lines back (positive) or forward into the
// scrollback; 0 is the live screen.
func (p *Pane) Scroll(n int) {
	p.drawMu.Lock()
	limit := p.emu.ScrollbackLen()
	p.drawMu.Unlock()
	p.mu.Lock()
	p.scroll = min(max(p.scroll+n, 0), limit)
	p.mu.Unlock()
}

// Height is the pane's height in rows.
func (p *Pane) Height() int { return p.emu.Height() }

func (p *Pane) Scrolled() int {
	p.mu.Lock()
	defer p.mu.Unlock()
	return p.scroll
}

// Paste forwards pasted text (bracketed when the program asked for it).
func (p *Pane) Paste(s string) { p.emu.Paste(s) }

// Type sends raw text, as if typed.
func (p *Pane) Type(s string) { p.emu.SendText(s) }

func (p *Pane) Render() string { return p.emu.Render() }

// Draw copies the terminal's cells into scr at area — no ANSI re-parsing,
// and each cell keeps the width the emulator measured.
func (p *Pane) Draw(scr uv.Screen, area uv.Rectangle) {
	off := p.Scrolled()
	p.drawMu.Lock()
	defer p.drawMu.Unlock()
	if off == 0 {
		p.emu.Draw(scr, area)
		return
	}
	// Scrolled back: the top `off` rows come from the scrollback, the rest
	// from the top of the live screen.
	sb := p.emu.ScrollbackLen()
	off = min(off, sb)
	for y := 0; y < area.Dy(); y++ {
		for x := 0; x < area.Dx(); {
			var c *uv.Cell
			if y < off {
				c = p.emu.ScrollbackCellAt(x, sb-off+y)
			} else {
				c = p.emu.CellAt(x, y-off)
			}
			w := 1
			if c != nil {
				c = c.Clone()
				if c.Width > 1 {
					w = c.Width
				}
			} else {
				e := uv.EmptyCell
				c = &e
			}
			scr.SetCell(area.Min.X+x, area.Min.Y+y, c)
			x += w
		}
	}
}

// SendMouse forwards a mouse event with coordinates relative to the pane.
// The emulator only encodes it if the program enabled mouse reporting.
func (p *Pane) SendMouse(ev uv.MouseEvent) { p.emu.SendMouse(ev) }

// Close detaches: kills our `claude attach` client. A background session
// keeps running in the Claude Code daemon.
//
// Never SIGINT: an attach client may relay it to the session as Ctrl+C and
// interrupt the agent. SIGHUP is what closing a terminal window sends.
func (p *Pane) Close() {
	if p.cmd.Process != nil && !p.Done() {
		_ = p.cmd.Process.Signal(syscall.SIGHUP)
		_ = p.cmd.Process.Kill()
	}
	_ = p.ptmx.Close()
}
