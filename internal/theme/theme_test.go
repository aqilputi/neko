package theme

import (
	"image/color"
	"testing"
)

func TestAllThemesComplete(t *testing.T) {
	if len(All) == 0 {
		t.Fatal("no themes")
	}
	seen := map[string]bool{}
	for _, th := range All {
		if th.Name == "" {
			t.Fatal("theme without a name")
		}
		if seen[th.Name] {
			t.Errorf("duplicate theme name %q", th.Name)
		}
		seen[th.Name] = true
		colors := map[string]color.Color{
			"Bg": th.Bg, "Surface": th.Surface, "Overlay": th.Overlay, "Border": th.Border,
			"Text": th.Text, "Subtle": th.Subtle, "Muted": th.Muted,
			"Accent": th.Accent, "Accent2": th.Accent2,
			"Busy": th.Busy, "Blocked": th.Blocked, "Done": th.Done, "Idle": th.Idle, "Error": th.Error,
			"CostLow": th.CostLow, "CostHigh": th.CostHigh,
			"CatBody": th.CatBody, "CatEyes": th.CatEyes,
		}
		for name, col := range colors {
			if col == nil {
				t.Errorf("%s: %s is nil", th.Name, name)
			}
		}
		if len(th.Contexts) == 0 {
			t.Errorf("%s: no context colors", th.Name)
		}
		for i, col := range th.Contexts {
			if col == nil {
				t.Errorf("%s: Contexts[%d] is nil", th.Name, i)
			}
		}
		// the background and the text must differ, or nothing is readable
		if sameColor(th.Bg, th.Text) {
			t.Errorf("%s: Bg == Text", th.Name)
		}
	}
}

func sameColor(a, b color.Color) bool {
	r1, g1, b1, a1 := a.RGBA()
	r2, g2, b2, a2 := b.RGBA()
	return r1 == r2 && g1 == g2 && b1 == b2 && a1 == a2
}

func TestByName(t *testing.T) {
	for i, th := range All {
		if got := ByName(th.Name); got != i {
			t.Errorf("ByName(%q) = %d, want %d", th.Name, got, i)
		}
	}
	for _, name := range []string{"", "nope", "Dracula", " dracula"} {
		if got := ByName(name); got != 0 {
			t.Errorf("ByName(%q) = %d, want 0 (fallback)", name, got)
		}
	}
}

func TestContextColor(t *testing.T) {
	th := All[0]
	n := len(th.Contexts)
	for _, tc := range []struct{ i, want int }{{0, 0}, {1, 1}, {n - 1, n - 1}, {n, 0}, {n + 2, 2}, {3*n + 1, 1}} {
		if got := th.ContextColor(tc.i); !sameColor(got, th.Contexts[tc.want]) {
			t.Errorf("ContextColor(%d) = %v, want Contexts[%d]", tc.i, got, tc.want)
		}
	}
	empty := Theme{Accent: c("#123456")}
	for _, i := range []int{0, 5} {
		if got := empty.ContextColor(i); !sameColor(got, empty.Accent) {
			t.Errorf("empty Contexts: ContextColor(%d) = %v, want Accent", i, got)
		}
	}
}
