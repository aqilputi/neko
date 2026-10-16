package term

import (
	"io"
	"sync"

	tea "charm.land/bubbletea/v2"
	uv "github.com/charmbracelet/ultraviolet"
	"github.com/charmbracelet/x/ansi"
	"github.com/charmbracelet/x/vt"
)

// Screen is the rendering half of a pane: a virtual terminal (x/vt) fed with
// the program's output, drawn into the TUI, and turning keys, mouse and
// paste into the bytes the program expects.
type Screen struct {
	emu *vt.SafeEmulator

	mu     sync.Mutex // guards clips, mouse, scroll
	drawMu sync.Mutex // SafeEmulator does not lock Draw: guard Write vs Draw

	osc   oscFilter // only touched by Feed (one writer)
	clips []string  // clipboard writes (OSC 52) waiting to reach the real terminal
	mouse map[ansi.Mode]bool
	// scroll is how many lines the view is scrolled back into the emulator's
	// scrollback (classic renderer only; fullscreen apps scroll themselves).
	scroll int
}

// NewScreen makes a cols×rows screen (clamped to 10×3). Call Pipe to route
// its input (keys, emulator replies) to the program.
func NewScreen(cols, rows int) *Screen {
	cols, rows = clampSize(cols, rows)
	s := &Screen{emu: vt.NewSafeEmulator(cols, rows), mouse: map[ansi.Mode]bool{}}
	s.osc.onClipboard = func(t string) { s.mu.Lock(); s.clips = append(s.clips, t); s.mu.Unlock() }
	s.emu.SetCallbacks(vt.Callbacks{
		EnableMode:  func(m ansi.Mode) { s.setMouseMode(m, true) },
		DisableMode: func(m ansi.Mode) { s.setMouseMode(m, false) },
	})
	return s
}

// Pipe copies what the screen sends (keys, paste, cursor position reports,
// DA replies…) to w, the program's input.
func (s *Screen) Pipe(w io.Writer) { go func() { _, _ = io.Copy(w, s.emu) }() }

// Feed writes program output into the emulator. OSC sequences are pulled
// out first: x/vt v0.1.0 would print a non-ASCII title as text.
func (s *Screen) Feed(b []byte) {
	clean := s.osc.filter(b)
	s.drawMu.Lock()
	_, _ = s.emu.Write(clean)
	s.drawMu.Unlock()
}

// Resize resizes the emulator (clamped to 10×3) and reports whether the size
// changed.
func (s *Screen) Resize(cols, rows int) bool {
	cols, rows = clampSize(cols, rows)
	if s.emu.Width() == cols && s.emu.Height() == rows {
		return false
	}
	s.drawMu.Lock()
	s.emu.Resize(cols, rows)
	s.drawMu.Unlock()
	return true
}

// SendKey forwards a key press to the program (and jumps back to the live
// view if it was scrolled back).
func (s *Screen) SendKey(k tea.KeyPressMsg) {
	s.mu.Lock()
	s.scroll = 0
	s.mu.Unlock()
	k, raw := encodeKey(k) // fills the gaps of x/vt's key encoder
	if raw != "" {
		s.emu.SendText(raw)
		return
	}
	s.emu.SendKey(uv.KeyPressEvent(k))
}

func (s *Screen) setMouseMode(m ansi.Mode, on bool) {
	switch m {
	case ansi.ModeMouseX10, ansi.ModeMouseNormal, ansi.ModeMouseHighlight, ansi.ModeMouseButtonEvent, ansi.ModeMouseAnyEvent:
		s.mu.Lock()
		s.mouse[m] = on
		s.mu.Unlock()
	}
}

// WantsMouse reports whether the program turned on mouse reporting (Claude
// Code's fullscreen renderer does: it scrolls and selects on its own).
func (s *Screen) WantsMouse() bool {
	s.mu.Lock()
	defer s.mu.Unlock()
	for _, on := range s.mouse {
		if on {
			return true
		}
	}
	return false
}

// TakeClipboard returns and clears the clipboard writes the program made.
func (s *Screen) TakeClipboard() []string {
	s.mu.Lock()
	defer s.mu.Unlock()
	c := s.clips
	s.clips = nil
	return c
}

// Cursor is where the program's cursor is, relative to the pane, and whether
// it should be shown (hidden while scrolled back).
func (s *Screen) Cursor() (x, y int, visible bool) {
	s.drawMu.Lock()
	pos, hidden := s.emu.CursorPosition(), s.emu.CursorHidden()
	s.drawMu.Unlock()
	return pos.X, pos.Y, !hidden && s.Scrolled() == 0
}

// Scroll moves the view n lines back (positive) or forward into the
// scrollback; 0 is the live screen.
func (s *Screen) Scroll(n int) {
	s.drawMu.Lock()
	limit := s.emu.ScrollbackLen()
	s.drawMu.Unlock()
	s.mu.Lock()
	s.scroll = min(max(s.scroll+n, 0), limit)
	s.mu.Unlock()
}

// Height is the screen's height in rows.
func (s *Screen) Height() int { return s.emu.Height() }

// Scrolled is how many lines the view is scrolled back.
func (s *Screen) Scrolled() int {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.scroll
}

// Paste forwards pasted text (bracketed when the program asked for it).
func (s *Screen) Paste(text string) { s.emu.Paste(text) }

// Type sends raw text, as if typed.
func (s *Screen) Type(text string) { s.emu.SendText(text) }

// Render is the live screen as ANSI text.
func (s *Screen) Render() string { return s.emu.Render() }

// Draw copies the terminal's cells into scr at area — no ANSI re-parsing,
// and each cell keeps the width the emulator measured.
func (s *Screen) Draw(scr uv.Screen, area uv.Rectangle) {
	off := s.Scrolled()
	s.drawMu.Lock()
	defer s.drawMu.Unlock()
	if off == 0 {
		s.emu.Draw(scr, area)
		return
	}
	// Scrolled back: the top `off` rows come from the scrollback, the rest
	// from the top of the live screen.
	sb := s.emu.ScrollbackLen()
	off = min(off, sb)
	for y := 0; y < area.Dy(); y++ {
		for x := 0; x < area.Dx(); {
			var c *uv.Cell
			if y < off {
				c = s.emu.ScrollbackCellAt(x, sb-off+y)
			} else {
				c = s.emu.CellAt(x, y-off)
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
func (s *Screen) SendMouse(ev uv.MouseEvent) { s.emu.SendMouse(ev) }
