package jira

import (
	"encoding/json"
	"testing"
)

func TestCleanDesc(t *testing.T) {
	cases := map[string]string{
		`null`:                                  "",
		`{"type":"doc"}`:                        "",
		`"* \\[x\\] feito \\~16:30\n\n\n\nfim"`: "* [x] feito ~16:30\n\nfim",
		`"antes ![](blob:https://x/?a=b&url=null) depois"`: "antes [imagem] depois",
		`"\u200c\n## Título"`:                              "## Título",
	}
	for in, want := range cases {
		if got := cleanDesc(json.RawMessage(in)); got != want {
			t.Errorf("cleanDesc(%s) = %q, want %q", in, got, want)
		}
	}
}
