package ui

import (
	"testing"

	"charm.land/bubbles/v2/key"
)

// The palette runs actions by synthesizing the binding's first key; each
// synthesized key must match its own binding.
func TestKeyPressMatchesBindings(t *testing.T) {
	for _, group := range fullHelp() {
		for _, b := range group {
			k := b.Keys()[0]
			if !key.Matches(keyPress(k), b) {
				t.Errorf("keyPress(%q).String()=%q does not match %v", k, keyPress(k).String(), b.Keys())
			}
		}
	}
}
