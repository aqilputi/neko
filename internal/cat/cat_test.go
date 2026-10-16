package cat

import (
	"strings"
	"testing"
	"time"

	"charm.land/lipgloss/v2"
	"github.com/charmbracelet/x/ansi"

	"github.com/aqilputi/nekuroddo/internal/theme"
)

func TestNewIsAsleep(t *testing.T) {
	m := New()
	if m.Awake() {
		t.Fatal("a new cat should be asleep")
	}
}

func TestMeowWakesAndExpires(t *testing.T) {
	m := New()
	m.Meow("miau")
	if !m.Awake() {
		t.Fatal("Meow should wake the cat")
	}
	m.Advance()
	if !m.Awake() || m.meow != "miau" {
		t.Fatal("advancing within the window must keep the meow")
	}
	// pretend the 6s window passed
	m.meowUntil = time.Now().Add(-time.Millisecond)
	if m.Awake() {
		t.Fatal("cat still awake after the meow window")
	}
	m.Advance()
	if m.meow != "" {
		t.Fatalf("Advance should clear an expired meow, got %q", m.meow)
	}
	if m.frame != 2 {
		t.Fatalf("frame = %d, want 2", m.frame)
	}
}

func TestMeowEmptyTextIsNotAwake(t *testing.T) {
	m := New()
	m.Meow("")
	if m.Awake() {
		t.Fatal("an empty meow should not wake the cat")
	}
}

func TestMeowWindowIsAboutSixSeconds(t *testing.T) {
	m := New()
	before := time.Now()
	m.Meow("x")
	if d := m.meowUntil.Sub(before); d < 5*time.Second || d > 7*time.Second {
		t.Fatalf("meow window %v, want ~6s", d)
	}
}

func TestViewAsleepCyclesZz(t *testing.T) {
	th := theme.All[0]
	bg := lipgloss.NewStyle().Background(th.Surface)
	m := New()
	for i := 0; i < 2*len(zz); i++ {
		want := "ᓚᘏᗢ" + zz[i%len(zz)] + " "
		if got := ansi.Strip(m.View(th, bg)); got != want {
			t.Fatalf("frame %d: %q, want %q", i, got, want)
		}
		m.Advance()
	}
}

func TestViewAwakeShowsMeow(t *testing.T) {
	bg := lipgloss.NewStyle()
	for _, th := range theme.All {
		m := New()
		m.Meow("voltei!")
		for i := 0; i < 4; i++ {
			if got := ansi.Strip(m.View(th, bg)); got != "voltei! ᓚᘏᗢ! " {
				t.Fatalf("%s frame %d: %q", th.Name, i, got)
			}
			m.Advance()
		}
		// once the meow expires, back to snoring
		m.meowUntil = time.Now().Add(-time.Millisecond)
		m.Advance()
		if got := ansi.Strip(m.View(th, bg)); strings.Contains(got, "voltei") || !strings.HasPrefix(got, "ᓚᘏᗢ") {
			t.Fatalf("%s: after expiry %q", th.Name, got)
		}
	}
}

// The meow blinks between Accent and Blocked every two frames.
func TestViewAwakeBlinks(t *testing.T) {
	th := theme.Theme{
		Accent: lipgloss.Color("#ff0000"), Blocked: lipgloss.Color("#00ff00"),
		Accent2: lipgloss.Color("#0000ff"), CatBody: lipgloss.Color("#111111"), CatEyes: lipgloss.Color("#222222"),
	}
	m := New()
	m.Meow("oi")
	// the SGR before the meow text carries the blink color
	prefix := func(s string) string { return s[:strings.Index(s, "oi")] }
	var p []string
	for i := 0; i < 4; i++ {
		p = append(p, prefix(m.View(th, lipgloss.NewStyle())))
		m.Advance()
	}
	if p[0] != p[1] || p[2] != p[3] {
		t.Fatalf("color should hold for two frames: %q", p)
	}
	if p[0] == p[2] {
		t.Fatalf("color should blink every two frames: %q", p)
	}
}
