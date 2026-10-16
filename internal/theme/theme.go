// Package theme holds the color palettes nekuroddo can switch between at runtime.
package theme

import (
	"image/color"

	"charm.land/lipgloss/v2"
)

// Theme is a named palette. Every UI color comes from here so switching themes
// (key `T`) recolors the whole screen.
type Theme struct {
	Name string

	Bg      color.Color // app background
	Surface color.Color // panels, the cat's floor
	Overlay color.Color // selection background
	Border  color.Color
	Text    color.Color
	Subtle  color.Color
	Muted   color.Color

	Accent  color.Color // focused borders, titles
	Accent2 color.Color

	Busy    color.Color
	Blocked color.Color // waiting on the user: the color that must pop
	Done    color.Color
	Idle    color.Color
	Error   color.Color

	// Context colors are assigned round-robin to contexts (visio, pessoal, …).
	Contexts []color.Color

	// CostLow → CostHigh is the gradient of the cost bars.
	CostLow  color.Color
	CostHigh color.Color

	CatBody color.Color
	CatEyes color.Color
}

func c(hex string) color.Color { return lipgloss.Color(hex) }

// All lists the built-in themes in the order `T` cycles through them.
var All = []Theme{
	{
		Name: "catppuccin-mocha",
		Bg:   c("#1e1e2e"), Surface: c("#313244"), Overlay: c("#45475a"), Border: c("#585b70"),
		Text: c("#cdd6f4"), Subtle: c("#a6adc8"), Muted: c("#7f849c"),
		Accent: c("#cba6f7"), Accent2: c("#89b4fa"),
		Busy: c("#89dceb"), Blocked: c("#f38ba8"), Done: c("#a6e3a1"), Idle: c("#9399b2"), Error: c("#eba0ac"),
		Contexts: []color.Color{c("#f5c2e7"), c("#89b4fa"), c("#a6e3a1"), c("#fab387"), c("#94e2d5"), c("#f9e2af"), c("#b4befe")},
		CostLow:  c("#a6e3a1"), CostHigh: c("#f38ba8"),
		CatBody: c("#11111b"), CatEyes: c("#f9e2af"),
	},
	{
		Name: "tokyo-night",
		Bg:   c("#1a1b26"), Surface: c("#24283b"), Overlay: c("#364a82"), Border: c("#414868"),
		Text: c("#c0caf5"), Subtle: c("#a9b1d6"), Muted: c("#565f89"),
		Accent: c("#7aa2f7"), Accent2: c("#bb9af7"),
		Busy: c("#7dcfff"), Blocked: c("#ff007c"), Done: c("#9ece6a"), Idle: c("#565f89"), Error: c("#f7768e"),
		Contexts: []color.Color{c("#bb9af7"), c("#7aa2f7"), c("#9ece6a"), c("#ff9e64"), c("#2ac3de"), c("#e0af68"), c("#73daca")},
		CostLow:  c("#9ece6a"), CostHigh: c("#ff007c"),
		CatBody: c("#0f0f17"), CatEyes: c("#e0af68"),
	},
	{
		Name: "gruvbox",
		Bg:   c("#282828"), Surface: c("#3c3836"), Overlay: c("#504945"), Border: c("#665c54"),
		Text: c("#ebdbb2"), Subtle: c("#d5c4a1"), Muted: c("#928374"),
		Accent: c("#fe8019"), Accent2: c("#fabd2f"),
		Busy: c("#83a598"), Blocked: c("#fb4934"), Done: c("#b8bb26"), Idle: c("#928374"), Error: c("#cc241d"),
		Contexts: []color.Color{c("#d3869b"), c("#83a598"), c("#b8bb26"), c("#fe8019"), c("#8ec07c"), c("#fabd2f")},
		CostLow:  c("#b8bb26"), CostHigh: c("#fb4934"),
		CatBody: c("#1d2021"), CatEyes: c("#fabd2f"),
	},
	{
		Name: "dracula",
		Bg:   c("#282a36"), Surface: c("#343746"), Overlay: c("#44475a"), Border: c("#6272a4"),
		Text: c("#f8f8f2"), Subtle: c("#e2e2dc"), Muted: c("#6272a4"),
		Accent: c("#ff79c6"), Accent2: c("#bd93f9"),
		Busy: c("#8be9fd"), Blocked: c("#ff5555"), Done: c("#50fa7b"), Idle: c("#6272a4"), Error: c("#ff5555"),
		Contexts: []color.Color{c("#ff79c6"), c("#bd93f9"), c("#50fa7b"), c("#ffb86c"), c("#8be9fd"), c("#f1fa8c")},
		CostLow:  c("#50fa7b"), CostHigh: c("#ff5555"),
		CatBody: c("#191a21"), CatEyes: c("#f1fa8c"),
	},
	{
		Name: "rose-pine-dawn",
		Bg:   c("#faf4ed"), Surface: c("#f2e9e1"), Overlay: c("#dfdad9"), Border: c("#cecacd"),
		Text: c("#575279"), Subtle: c("#797593"), Muted: c("#9893a5"),
		Accent: c("#907aa9"), Accent2: c("#286983"),
		Busy: c("#56949f"), Blocked: c("#b4637a"), Done: c("#286983"), Idle: c("#9893a5"), Error: c("#b4637a"),
		Contexts: []color.Color{c("#907aa9"), c("#286983"), c("#d7827e"), c("#56949f"), c("#ea9d34"), c("#b4637a")},
		CostLow:  c("#56949f"), CostHigh: c("#b4637a"),
		CatBody: c("#232136"), CatEyes: c("#ea9d34"),
	},
}

// ByName returns the theme with that name, or the first one.
func ByName(name string) int {
	for i, t := range All {
		if t.Name == name {
			return i
		}
	}
	return 0
}

// ContextColor picks a stable color for the i-th context.
func (t Theme) ContextColor(i int) color.Color {
	if len(t.Contexts) == 0 {
		return t.Accent
	}
	return t.Contexts[i%len(t.Contexts)]
}
