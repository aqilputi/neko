// Package cat is the little black cat napping in the bottom-right corner of
// neko. It sleeps (zZ) and wakes up to meow when an agent comes back. It has
// no clock of its own: the UI's single animation clock advances it.
package cat

import (
	"time"

	"charm.land/lipgloss/v2"

	"github.com/fakio/neko/internal/theme"
)

type Model struct {
	frame     int
	meow      string
	meowUntil time.Time
}

func New() Model { return Model{} }

// Meow wakes the cat up to say something for a few seconds.
func (m *Model) Meow(text string) {
	m.meow = text
	m.meowUntil = time.Now().Add(6 * time.Second)
}

// Awake reports whether the cat is meowing (the UI animates faster then).
func (m Model) Awake() bool { return m.meow != "" && time.Now().Before(m.meowUntil) }

// Advance moves the animation one frame.
func (m *Model) Advance() {
	m.frame++
	if m.meow != "" && !time.Now().Before(m.meowUntil) {
		m.meow = ""
	}
}

var zz = []string{"   ", "z  ", "zZ ", "zZz", " Zz", "  z"}

// View renders the cat (and its speech bubble) for the right end of the
// status line, on bg.
func (m Model) View(t theme.Theme, bg lipgloss.Style) string {
	body := bg.Foreground(t.CatBody).Bold(true)
	if m.Awake() {
		blink := t.Blocked
		if m.frame%4 < 2 {
			blink = t.Accent
		}
		return bg.Foreground(blink).Bold(true).Render(m.meow+" ") +
			body.Render("ᓚᘏᗢ") + bg.Foreground(t.CatEyes).Bold(true).Render("!") + bg.Render(" ")
	}
	return body.Render("ᓚᘏᗢ") + bg.Foreground(t.Accent2).Render(zz[m.frame%len(zz)]) + bg.Render(" ")
}
