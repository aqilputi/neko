package store

import "testing"

func TestLabelPrefersTitleOverDerivedNames(t *testing.T) {
	cases := []struct {
		s    Session
		want string
	}{
		{Session{Name: "2703ddb5", ShortID: "2703ddb5", SessionID: "2703ddb5-1afa", Title: "Migração zone presence"}, "Migração zone presence"},
		{Session{Name: "acme-research-context-3b", Cwd: "/x/acme-research-context", Title: "Migração"}, "Migração"},
		{Session{Name: "todo #3 limpar worktrees", ShortID: "abcd1234", SessionID: "abcd1234-x", Title: "outra"}, "todo #3 limpar worktrees"},
		{Session{Name: "2703ddb5", ShortID: "2703ddb5", SessionID: "2703ddb5-1afa", Kind: "background", Cwd: "/x/neko"}, "sessão nova em neko"},
	}
	for _, c := range cases {
		if got := c.s.Label(); got != c.want {
			t.Errorf("%+v → %q, want %q", c.s, got, c.want)
		}
	}
}
