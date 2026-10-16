package theme

import (
	"image/color"
	"regexp"
	"slices"
	"testing"
)

var hexRe = regexp.MustCompile(`^#[0-9a-f]{6}$`)

func TestHex(t *testing.T) {
	for _, tc := range []struct {
		in   color.Color
		want string
	}{
		{c("#1e1e2e"), "#1e1e2e"},
		{c("#FFFFFF"), "#ffffff"},
		{color.RGBA{R: 1, G: 2, B: 3, A: 255}, "#010203"},
		{color.Black, "#000000"},
		{nil, ""},
	} {
		if got := Hex(tc.in); got != tc.want {
			t.Errorf("Hex(%v) = %q, want %q", tc.in, got, tc.want)
		}
	}
}

func TestTokensCoverEveryColor(t *testing.T) {
	keys := []string{"bg", "surface", "overlay", "border", "text", "subtle", "muted", "accent", "accent2",
		"busy", "blocked", "done", "idle", "error", "cost_low", "cost_high", "cat_body", "cat_eyes"}
	for _, th := range All {
		tok := th.Tokens()
		if len(tok) != len(keys) {
			t.Errorf("%s: %d tokens, want %d: %v", th.Name, len(tok), len(keys), tok)
		}
		for _, k := range keys {
			if !hexRe.MatchString(tok[k]) {
				t.Errorf("%s: token %q = %q, want #rrggbb", th.Name, k, tok[k])
			}
		}
		ctx := th.ContextTokens()
		if len(ctx) != len(th.Contexts) {
			t.Errorf("%s: %d context tokens, want %d", th.Name, len(ctx), len(th.Contexts))
		}
		for i, h := range ctx {
			if h != Hex(th.Contexts[i]) || !hexRe.MatchString(h) {
				t.Errorf("%s: contexts[%d] = %q", th.Name, i, h)
			}
		}
	}
	mocha := All[0].Tokens()
	if mocha["bg"] != "#1e1e2e" || mocha["blocked"] != "#f38ba8" || mocha["cat_eyes"] != "#f9e2af" {
		t.Errorf("catppuccin-mocha tokens = %v", mocha)
	}
}

func TestTokensOfEmptyTheme(t *testing.T) {
	var th Theme
	for k, v := range th.Tokens() {
		if v != "" {
			t.Errorf("empty theme: %s = %q, want \"\"", k, v)
		}
	}
	if got := th.ContextTokens(); got == nil || len(got) != 0 {
		t.Errorf("empty theme: ContextTokens() = %#v, want empty non-nil", got)
	}
}

func TestNames(t *testing.T) {
	got := Names()
	if len(got) != len(All) {
		t.Fatalf("Names() = %v", got)
	}
	for i, th := range All {
		if got[i] != th.Name {
			t.Errorf("Names()[%d] = %q, want %q", i, got[i], th.Name)
		}
	}
	if !slices.Contains(got, "dracula") {
		t.Errorf("Names() = %v, want dracula in it", got)
	}
}

func TestLookup(t *testing.T) {
	if th, ok := Lookup("gruvbox"); !ok || th.Name != "gruvbox" {
		t.Errorf("Lookup(gruvbox) = %q, %v", th.Name, ok)
	}
	if th, ok := Lookup("nope"); ok || th.Name != All[0].Name {
		t.Errorf("Lookup(nope) = %q, %v; want the first theme and false", th.Name, ok)
	}
}
