package term

import (
	"encoding/base64"
	"fmt"
	"strings"
	"testing"

	tea "charm.land/bubbletea/v2"
)

func TestOSCFilterCases(t *testing.T) {
	b64 := func(s string) string { return base64.StdEncoding.EncodeToString([]byte(s)) }
	cases := []struct {
		name, in, out string
		titles, clips []string
	}{
		{"plain text", "hello", "hello", nil, nil},
		{"CSI passes", "a\x1b[1mb", "a\x1b[1mb", nil, nil},
		{"lone ESC + char passes", "\x1b7x", "\x1b7x", nil, nil},
		{"title BEL", "\x1b]0;oi\x07z", "z", []string{"oi"}, nil},
		{"title ST", "\x1b]2;ação ✳\x1b\\z", "z", []string{"ação ✳"}, nil},
		{"empty title", "\x1b]0;\x07", "", []string{""}, nil},
		{"icon name dropped", "\x1b]1;ícone\x07z", "z", nil, nil},
		{"clipboard", "\x1b]52;c;" + b64("olá") + "\x07", "", nil, []string{"olá"}},
		{"clipboard no targets", "\x1b]52;;" + b64("x") + "\x07", "", nil, []string{"x"}},
		{"clipboard bad base64", "\x1b]52;c;!!!\x07z", "z", nil, nil},
		{"ascii OSC re-emitted with ST", "\x1b]8;;http://x\x07L", "\x1b]8;;http://x\x1b\\L", nil, nil},
		{"non-ascii unknown OSC dropped", "\x1b]8;;http://ç\x07L", "L", nil, nil},
		{"ESC not ST aborts OSC", "\x1b]0;abc\x1bxyz", "\x1bxyz", nil, nil},
		{"ESC ESC restarts sequence", "\x1b]0;abc\x1b\x1b[1m", "\x1b[1m", nil, nil},
		{"ESC ESC restarts OSC", "\x1b]0;a\x1b\x1b]0;b\x07", "", []string{"b"}, nil},
	}
	for _, c := range cases {
		for _, chunk := range []int{1, 2, 5, 1 << 10} {
			t.Run(fmt.Sprintf("%s/chunk%d", c.name, chunk), func(t *testing.T) {
				var titles, clips []string
				f := oscFilter{
					onTitle:     func(s string) { titles = append(titles, s) },
					onClipboard: func(s string) { clips = append(clips, s) },
				}
				var out []byte
				for i := 0; i < len(c.in); i += chunk {
					out = append(out, f.filter([]byte(c.in[i:min(i+chunk, len(c.in))]))...)
				}
				if string(out) != c.out {
					t.Errorf("out %q, want %q", out, c.out)
				}
				if strings.Join(titles, "|") != strings.Join(c.titles, "|") || len(titles) != len(c.titles) {
					t.Errorf("titles %q, want %q", titles, c.titles)
				}
				if strings.Join(clips, "|") != strings.Join(c.clips, "|") || len(clips) != len(c.clips) {
					t.Errorf("clips %q, want %q", clips, c.clips)
				}
			})
		}
	}
}

func TestOSCFilterNilCallbacks(t *testing.T) {
	var f oscFilter
	in := "\x1b]0;t\x07\x1b]52;c;" + base64.StdEncoding.EncodeToString([]byte("x")) + "\x07ok"
	if got := string(f.filter([]byte(in))); got != "ok" {
		t.Fatalf("got %q", got)
	}
}

// A runaway OSC is capped instead of growing without bound.
func TestOSCFilterCapsBuffer(t *testing.T) {
	var title string
	f := oscFilter{onTitle: func(s string) { title = s }}
	big := strings.Repeat("a", maxOSC+100)
	f.filter([]byte("\x1b]0;" + big))
	if len(f.buf) != maxOSC {
		t.Fatalf("buffer %d, want cap %d", len(f.buf), maxOSC)
	}
	if out := f.filter([]byte("\x07after")); string(out) != "after" {
		t.Fatalf("out %q", out)
	}
	if len(title) != maxOSC-2 { // "0;" is part of the capped buffer
		t.Fatalf("title len %d", len(title))
	}
}

// encodeKey: keys x/vt mishandles become raw sequences; the rest is cleaned
// and left to x/vt.
func TestEncodeKey(t *testing.T) {
	cases := []struct {
		name string
		k    tea.KeyPressMsg
		raw  string
	}{
		{"plain a", tea.KeyPressMsg{Code: 'a', Text: "a"}, ""},
		{"up", tea.KeyPressMsg{Code: tea.KeyUp}, ""},
		{"up numlock", tea.KeyPressMsg{Code: tea.KeyUp, Mod: tea.ModNumLock | tea.ModCapsLock | tea.ModScrollLock}, ""},
		{"shift+up", tea.KeyPressMsg{Code: tea.KeyUp, Mod: tea.ModShift}, "\x1b[1;2A"},
		{"alt+down", tea.KeyPressMsg{Code: tea.KeyDown, Mod: tea.ModAlt}, "\x1b[1;3B"},
		{"shift+alt+right", tea.KeyPressMsg{Code: tea.KeyRight, Mod: tea.ModShift | tea.ModAlt}, "\x1b[1;4C"},
		{"ctrl+left", tea.KeyPressMsg{Code: tea.KeyLeft, Mod: tea.ModCtrl}, "\x1b[1;5D"},
		{"ctrl+shift+home", tea.KeyPressMsg{Code: tea.KeyHome, Mod: tea.ModCtrl | tea.ModShift}, "\x1b[1;6H"},
		{"ctrl+alt+end", tea.KeyPressMsg{Code: tea.KeyEnd, Mod: tea.ModCtrl | tea.ModAlt}, "\x1b[1;7F"},
		{"meta+up", tea.KeyPressMsg{Code: tea.KeyUp, Mod: tea.ModMeta}, "\x1b[1;9A"},
		{"super+up", tea.KeyPressMsg{Code: tea.KeyUp, Mod: tea.ModSuper}, "\x1b[1;9A"},
		{"all mods", tea.KeyPressMsg{Code: tea.KeyUp, Mod: tea.ModShift | tea.ModAlt | tea.ModCtrl | tea.ModMeta}, "\x1b[1;16A"},
		{"shift+insert", tea.KeyPressMsg{Code: tea.KeyInsert, Mod: tea.ModShift}, "\x1b[2;2~"},
		{"ctrl+delete", tea.KeyPressMsg{Code: tea.KeyDelete, Mod: tea.ModCtrl}, "\x1b[3;5~"},
		{"alt+pgup", tea.KeyPressMsg{Code: tea.KeyPgUp, Mod: tea.ModAlt}, "\x1b[5;3~"},
		{"ctrl+pgdown", tea.KeyPressMsg{Code: tea.KeyPgDown, Mod: tea.ModCtrl}, "\x1b[6;5~"},
		{"shift+enter", tea.KeyPressMsg{Code: tea.KeyEnter, Mod: tea.ModShift}, "\x1b\r"},
		{"alt+enter", tea.KeyPressMsg{Code: tea.KeyEnter, Mod: tea.ModAlt}, "\x1b\r"},
		{"ctrl+enter", tea.KeyPressMsg{Code: tea.KeyEnter, Mod: tea.ModCtrl}, ""},
		{"ctrl+backspace", tea.KeyPressMsg{Code: tea.KeyBackspace, Mod: tea.ModCtrl | tea.ModNumLock}, "\x17"},
		{"ctrl+a", tea.KeyPressMsg{Code: 'a', Mod: tea.ModCtrl}, ""},
		{"shift+tab", tea.KeyPressMsg{Code: tea.KeyTab, Mod: tea.ModShift}, ""},
		{"ctrl+f5", tea.KeyPressMsg{Code: tea.KeyF5, Mod: tea.ModCtrl}, ""},
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			k, raw := encodeKey(c.k)
			if raw != c.raw {
				t.Fatalf("raw %q, want %q", raw, c.raw)
			}
			if k.Mod&(tea.ModNumLock|tea.ModCapsLock|tea.ModScrollLock) != 0 {
				t.Errorf("lock modifiers not stripped: %v", k.Mod)
			}
			if k.IsRepeat || k.BaseCode != 0 || k.ShiftedCode != 0 {
				t.Errorf("key not cleaned: %+v", k)
			}
		})
	}
}

// Text is dropped from control keys (x/vt would type it) but kept for space
// and printable keys.
func TestEncodeKeyText(t *testing.T) {
	cases := []struct {
		k    tea.KeyPressMsg
		text string
	}{
		{tea.KeyPressMsg{Code: tea.KeyTab, Text: "\t"}, ""},
		{tea.KeyPressMsg{Code: tea.KeyEnter, Text: "\r"}, ""},
		{tea.KeyPressMsg{Code: tea.KeyEscape, Text: "\x1b"}, ""},
		{tea.KeyPressMsg{Code: tea.KeyBackspace, Text: "\x7f"}, ""},
		{tea.KeyPressMsg{Code: tea.KeySpace, Text: " "}, " "},
		{tea.KeyPressMsg{Code: 'é', Text: "é", BaseCode: 'e', ShiftedCode: 'E', IsRepeat: true}, "é"},
	}
	for _, c := range cases {
		if k, _ := encodeKey(c.k); k.Text != c.text {
			t.Errorf("%v: text %q, want %q", c.k.Code, k.Text, c.text)
		}
	}
}
