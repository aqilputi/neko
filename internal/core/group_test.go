package core

import "testing"

func TestCleanTag(t *testing.T) {
	for in, want := range map[string]string{
		"#Deploy RC":      "deploy-rc",
		"  # foo   bar  ": "foo-bar",
		"deploy-rc":       "deploy-rc",
		"":                "",
		"#":               "",
		"Ünïcode  Tag":    "ünïcode-tag",
	} {
		if got := CleanTag(in); got != want {
			t.Errorf("CleanTag(%q) = %q, want %q", in, got, want)
		}
	}
}
