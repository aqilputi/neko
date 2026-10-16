package core

import (
	"fmt"

	"github.com/aqilputi/nekuroddo/internal/claude"
)

// Truncate cuts s to n runes, ending in "…" when it had to cut.
func Truncate(s string, n int) string {
	r := []rune(s)
	if len(r) <= n {
		return s
	}
	if n <= 1 {
		return "…"
	}
	return string(r[:n-1]) + "…"
}

// Clamp keeps v within [lo, hi]; an empty range (hi < lo) gives lo.
func Clamp(v, lo, hi int) int {
	if hi < lo {
		return lo
	}
	return max(lo, min(v, hi))
}

// PctText formats a share of the window: "", "<1%", "3%", "12%".
func PctText(p float64) string {
	switch {
	case p <= 0:
		return ""
	case p < 1:
		return "<1%"
	}
	return fmt.Sprintf("%.0f%%", p)
}

// OptionCount is how many numbered options the pending questions offer.
func OptionCount(qs []claude.Question) int {
	n := 0
	for _, q := range qs {
		n += len(q.Options)
	}
	return n
}

// ResolveOption turns a bare option number ("2") into that option's label,
// numbering options across all pending questions in order. Anything else
// (or a number out of range) comes back as typed.
func ResolveOption(qs []claude.Question, val string) string {
	var n int
	if _, err := fmt.Sscanf(val, "%d", &n); err != nil || fmt.Sprint(n) != val {
		return val
	}
	i := 0
	for _, q := range qs {
		for _, o := range q.Options {
			i++
			if i == n {
				return o.Label
			}
		}
	}
	return val
}

// WindowPct estimates how much of the current 5-hour window a session used:
// the window's % (limitPct, from /usage; 0 when unknown) split across
// sessions by what each consumed since the window began (cost, out of
// windowCosts, which includes this session). 0 when unknown or idle.
func WindowPct(limitPct int, cost float64, windowCosts []float64) float64 {
	if limitPct <= 0 || cost <= 0 {
		return 0
	}
	var total float64
	for _, c := range windowCosts {
		total += c
	}
	if total <= 0 {
		return 0
	}
	return float64(limitPct) * cost / total
}
