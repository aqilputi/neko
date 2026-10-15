package term

import (
	"os"
	"strings"
	"testing"
	"time"

	tea "charm.land/bubbletea/v2"
	"github.com/charmbracelet/x/ansi"
)

// TestLiveShiftTab: does shift+tab cycle Claude Code's permission mode in
// the pane? (NEKO_LIVE=1; no prompt is sent, no tokens spent)
func TestLiveShiftTab(t *testing.T) {
	if os.Getenv("NEKO_LIVE") == "" {
		t.Skip()
	}
	home, _ := os.UserHomeDir()
	argv := []string{"claude", "--model", "haiku"}
	if a := os.Getenv("NEKO_LIVE_ARGV"); a != "" {
		argv = strings.Fields(a)
		if strings.HasPrefix(a, "fish -c ") {
			argv = []string{"fish", "-c", strings.TrimPrefix(a, "fish -c ")}
		}
	}
	p, err := Start("x", "x", home, 110, 30, argv...)
	if err != nil {
		t.Fatal(err)
	}
	defer p.Close()
	bottom := func() string {
		var out []string
		for _, l := range strings.Split(ansi.Strip(p.Render()), "\n") {
			if l = strings.TrimSpace(l); l != "" && !strings.HasPrefix(l, "───") {
				out = append(out, l)
			}
		}
		if len(out) > 3 {
			out = out[len(out)-3:]
		}
		return strings.Join(out, " | ")
	}
	time.Sleep(6 * time.Second)
	if strings.Contains(bottom(), "trust this folder") {
		p.SendKey(tea.KeyPressMsg{Code: tea.KeyDown})
		time.Sleep(300 * time.Millisecond)
		p.SendKey(tea.KeyPressMsg{Code: tea.KeyEnter})
		time.Sleep(4 * time.Second)
	}
	t.Logf("start: %s", bottom())
	for i := 0; i < 4; i++ {
		p.SendKey(tea.KeyPressMsg{Code: tea.KeyTab, Mod: tea.ModShift})
		time.Sleep(800 * time.Millisecond)
		t.Logf("shift+tab %d: %s", i+1, bottom())
	}
}
