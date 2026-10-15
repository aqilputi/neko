package term

import (
	"os"
	"testing"
	"time"

	tea "charm.land/bubbletea/v2"
)

// shift+tab reaches the program as CSI Z even when the terminal marks it as
// a repeat or attaches text (kitty keyboard protocol).
func TestShiftTabReachesProgram(t *testing.T) {
	out := t.TempDir() + "/got"
	p, err := Start("t", "t", "/", 20, 5, "sh", "-c", "stty raw -echo; head -c 3 > "+out)
	if err != nil {
		t.Fatal(err)
	}
	defer p.Close()
	time.Sleep(200 * time.Millisecond)
	p.SendKey(tea.KeyPressMsg{Code: tea.KeyTab, Mod: tea.ModShift, IsRepeat: true, Text: "\t"})
	deadline := time.Now().Add(2 * time.Second)
	for time.Now().Before(deadline) {
		if b, _ := os.ReadFile(out); len(b) == 3 {
			if string(b) != "\x1b[Z" {
				t.Fatalf("got %q", b)
			}
			return
		}
		time.Sleep(20 * time.Millisecond)
	}
	b, _ := os.ReadFile(out)
	t.Fatalf("shift+tab not delivered: %q", b)
}
