package term

import (
	"strings"
	"testing"
	"time"

	tea "charm.land/bubbletea/v2"
)

func TestPaneRoundTrip(t *testing.T) {
	p, err := Start("t", "t", "/", 40, 6, "bash", "--norc", "-c", `printf '\e[31mpronto\e[0m\n'; read x; echo "recebi:$x"; sleep 0.3`)
	if err != nil {
		t.Fatal(err)
	}
	defer p.Close()
	waitFor(t, p, "pronto")
	p.Paste("miau")
	p.SendKey(tea.KeyPressMsg{Code: tea.KeyEnter})
	waitFor(t, p, "recebi:miau")
	if !strings.Contains(p.Render(), "\x1b[") {
		t.Error("render lost colors")
	}
	p.Resize(60, 10)
}

func waitFor(t *testing.T, p *Pane, s string) {
	t.Helper()
	deadline := time.Now().Add(5 * time.Second)
	for time.Now().Before(deadline) {
		if strings.Contains(p.Render(), s) {
			return
		}
		time.Sleep(50 * time.Millisecond)
	}
	t.Fatalf("timeout waiting for %q; screen:\n%s", s, p.Render())
}
