package term

import (
	"os"
	"strings"
	"testing"
	"time"

	tea "charm.land/bubbletea/v2"
	"github.com/charmbracelet/x/ansi"
)

// What the program receives for keys as kitty reports them (NumLock on,
// held keys repeating).
func TestKeysReachProgram(t *testing.T) {
	num := tea.ModNumLock
	cases := []struct {
		name string
		k    tea.KeyPressMsg
		want string
	}{
		{"backspace+numlock", tea.KeyPressMsg{Code: tea.KeyBackspace, Mod: num}, "\x7f"},
		{"held backspace", tea.KeyPressMsg{Code: tea.KeyBackspace, IsRepeat: true}, "\x7f"},
		{"left+numlock", tea.KeyPressMsg{Code: tea.KeyLeft, Mod: num}, "\x1b[D"},
		{"ctrl+left", tea.KeyPressMsg{Code: tea.KeyLeft, Mod: tea.ModCtrl | num}, "\x1b[1;5D"},
		{"alt+right", tea.KeyPressMsg{Code: tea.KeyRight, Mod: tea.ModAlt}, "\x1b[1;3C"},
		{"shift+end", tea.KeyPressMsg{Code: tea.KeyEnd, Mod: tea.ModShift}, "\x1b[1;2F"},
		{"ctrl+delete", tea.KeyPressMsg{Code: tea.KeyDelete, Mod: tea.ModCtrl}, "\x1b[3;5~"},
		{"delete", tea.KeyPressMsg{Code: tea.KeyDelete, Mod: num}, "\x1b[3~"},
		{"shift+enter", tea.KeyPressMsg{Code: tea.KeyEnter, Mod: tea.ModShift}, "\x1b\r"},
		{"ctrl+backspace", tea.KeyPressMsg{Code: tea.KeyBackspace, Mod: tea.ModCtrl}, "\x17"},
		{"alt+backspace", tea.KeyPressMsg{Code: tea.KeyBackspace, Mod: tea.ModAlt}, "\x1b\x7f"},
		{"text+capslock", tea.KeyPressMsg{Code: 'a', Text: "A", Mod: tea.ModCapsLock}, "A"},
	}
	for _, c := range cases {
		out := t.TempDir() + "/got"
		n := len(c.want)
		p, err := Start("t", "t", "/", 20, 5, "sh", "-c", "stty raw -echo; head -c "+itoa(n)+" > "+out)
		if err != nil {
			t.Fatal(err)
		}
		time.Sleep(150 * time.Millisecond)
		p.SendKey(c.k)
		got := ""
		for deadline := time.Now().Add(2 * time.Second); time.Now().Before(deadline); time.Sleep(20 * time.Millisecond) {
			if b, _ := os.ReadFile(out); len(b) == n {
				got = string(b)
				break
			}
		}
		p.Close()
		if got != c.want {
			t.Errorf("%s: got %q want %q", c.name, got, c.want)
		}
	}
}

func itoa(n int) string { return strings.TrimSpace(strings.Repeat(" ", 0) + string(rune('0'+n))) }

// TestLiveEditing types into the real Claude prompt and edits it
// (NEKO_LIVE=1; nothing is submitted, no tokens).
func TestLiveEditing(t *testing.T) {
	if os.Getenv("NEKO_LIVE") == "" {
		t.Skip()
	}
	home, _ := os.UserHomeDir()
	p, err := Start("x", "x", home, 100, 25, "claude", "--model", "haiku")
	if err != nil {
		t.Fatal(err)
	}
	defer p.Close()
	time.Sleep(7 * time.Second)
	if strings.Contains(ansi.Strip(p.Render()), "trust this folder") {
		p.SendKey(tea.KeyPressMsg{Code: tea.KeyDown})
		time.Sleep(300 * time.Millisecond)
		p.SendKey(tea.KeyPressMsg{Code: tea.KeyEnter})
		time.Sleep(5 * time.Second)
	}
	num := tea.ModNumLock
	typ := func(s string) {
		for _, r := range s {
			p.SendKey(tea.KeyPressMsg{Code: r, Text: string(r), Mod: num})
			time.Sleep(40 * time.Millisecond)
		}
	}
	key := func(k tea.KeyPressMsg, n int) {
		for i := 0; i < n; i++ {
			p.SendKey(k)
			time.Sleep(80 * time.Millisecond)
		}
	}
	prompt := func() string {
		for _, l := range strings.Split(ansi.Strip(p.Render()), "\n") {
			if l = strings.TrimSpace(l); strings.HasPrefix(l, "❯") {
				return l
			}
		}
		return ""
	}
	typ("abcdef")
	key(tea.KeyPressMsg{Code: tea.KeyBackspace, Mod: num}, 2)               // abcd
	key(tea.KeyPressMsg{Code: tea.KeyLeft, Mod: num}, 2)                    // ab|cd
	typ("X")                                                                // abX|cd
	key(tea.KeyPressMsg{Code: tea.KeyEnd, Mod: num}, 1)                     // abXcd|
	typ(" fim")                                                             // abXcd fim
	key(tea.KeyPressMsg{Code: tea.KeyBackspace, Mod: tea.ModCtrl | num}, 1) // abXcd
	time.Sleep(time.Second)
	got := prompt()
	t.Logf("prompt: %q", got)
	if !strings.Contains(got, "abXcd") || strings.Contains(got, "fim") {
		t.Fatalf("editing failed: %q", got)
	}
	key(tea.KeyPressMsg{Code: 'u', Mod: tea.ModCtrl}, 1) // clear the line, nothing sent
}
