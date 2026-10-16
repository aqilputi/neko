package theme

import (
	"fmt"
	"image/color"
)

// Hex is c as "#rrggbb" (alpha dropped); "" for nil.
func Hex(c color.Color) string {
	if c == nil {
		return ""
	}
	r, g, b, _ := c.RGBA()
	return fmt.Sprintf("#%02x%02x%02x", r>>8, g>>8, b>>8)
}

// Tokens is every single color of the theme as "#rrggbb", keyed in
// snake_case (bg, surface, …, cat_eyes): what GET /v1/theme hands a GUI so it
// paints like the TUI. The context palette is ContextTokens.
func (t Theme) Tokens() map[string]string {
	return map[string]string{
		"bg": Hex(t.Bg), "surface": Hex(t.Surface), "overlay": Hex(t.Overlay), "border": Hex(t.Border),
		"text": Hex(t.Text), "subtle": Hex(t.Subtle), "muted": Hex(t.Muted),
		"accent": Hex(t.Accent), "accent2": Hex(t.Accent2),
		"busy": Hex(t.Busy), "blocked": Hex(t.Blocked), "done": Hex(t.Done), "idle": Hex(t.Idle), "error": Hex(t.Error),
		"cost_low": Hex(t.CostLow), "cost_high": Hex(t.CostHigh),
		"cat_body": Hex(t.CatBody), "cat_eyes": Hex(t.CatEyes),
	}
}

// ContextTokens is the context palette as "#rrggbb", in order: a client picks
// entry i % len for the context (or group collar) with palette index i, like
// ContextColor.
func (t Theme) ContextTokens() []string {
	out := make([]string, 0, len(t.Contexts))
	for _, c := range t.Contexts {
		out = append(out, Hex(c))
	}
	return out
}

// Names lists the built-in themes in cycling order.
func Names() []string {
	out := make([]string, 0, len(All))
	for _, t := range All {
		out = append(out, t.Name)
	}
	return out
}

// Lookup finds a theme by exact name; when there is none it returns the
// first theme (ByName's fallback) and false.
func Lookup(name string) (Theme, bool) {
	i := ByName(name)
	return All[i], All[i].Name == name
}
