package term

import (
	"fmt"

	tea "charm.land/bubbletea/v2"
)

// encodeKey covers what x/vt v0.1.0's key encoder misses. That encoder
// matches special keys by exact value, so a key carrying a modifier it does
// not know is dropped silently — the kitty keyboard protocol reports
// NumLock/CapsLock as modifiers, which killed backspace and the arrows — and
// it has no encoding at all for modified navigation keys (ctrl+←, shift+end…)
// or shift+enter.
//
// It returns the key cleaned for x/vt, or a raw sequence to write instead.
func encodeKey(k tea.KeyPressMsg) (tea.KeyPressMsg, string) {
	k.Mod &^= tea.ModCapsLock | tea.ModNumLock | tea.ModScrollLock
	k.IsRepeat = false // a held key must keep repeating, not vanish
	k.BaseCode, k.ShiftedCode = 0, 0

	switch k.Code {
	case tea.KeyTab, tea.KeyEnter, tea.KeyEscape, tea.KeyBackspace, tea.KeySpace:
		if k.Code != tea.KeySpace {
			k.Text = ""
		}
	}

	// xterm modifier parameter: 1 + shift(1) + alt(2) + ctrl(4) + meta/super(8)
	mod := 1
	if k.Mod.Contains(tea.ModShift) {
		mod++
	}
	if k.Mod.Contains(tea.ModAlt) {
		mod += 2
	}
	if k.Mod.Contains(tea.ModCtrl) {
		mod += 4
	}
	if k.Mod.Contains(tea.ModMeta) || k.Mod.Contains(tea.ModSuper) {
		mod += 8
	}

	switch k.Code {
	case tea.KeyEnter:
		if k.Mod.Contains(tea.ModShift) || k.Mod.Contains(tea.ModAlt) {
			return k, "\x1b\r" // newline in Claude Code's prompt
		}
	case tea.KeyBackspace:
		if k.Mod.Contains(tea.ModCtrl) {
			return k, "\x17" // ctrl+w: delete the previous word
		}
	}
	if mod == 1 {
		return k, "" // unmodified: x/vt handles it (cursor-key modes included)
	}
	if final, ok := map[rune]byte{
		tea.KeyUp: 'A', tea.KeyDown: 'B', tea.KeyRight: 'C', tea.KeyLeft: 'D',
		tea.KeyHome: 'H', tea.KeyEnd: 'F',
	}[k.Code]; ok {
		return k, fmt.Sprintf("\x1b[1;%d%c", mod, final)
	}
	if n, ok := map[rune]int{
		tea.KeyInsert: 2, tea.KeyDelete: 3, tea.KeyPgUp: 5, tea.KeyPgDown: 6,
	}[k.Code]; ok {
		return k, fmt.Sprintf("\x1b[%d;%d~", n, mod)
	}
	return k, ""
}
