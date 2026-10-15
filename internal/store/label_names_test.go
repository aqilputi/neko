package store

import (
	"strings"
	"testing"

	"github.com/fakio/neko/internal/claude"
)

// A session never shows as a hash: title, then its own name, then its first
// request, then a description of where it is.
func TestLabelNeverAnID(t *testing.T) {
	const sid = "5236ec5f-1111-2222-3333-444455556666"
	ask := "investiga por que o people flow de BR parou de mandar eventos desde ontem à noite"
	cases := []struct {
		name string
		s    Session
		want string
	}{
		{"history, no title", Session{SessionID: sid, Cwd: "/home/u", FirstUser: ask, Kind: "history"}, claude.PromptName(ask)},
		{"bg named by its id", Session{SessionID: sid, ShortID: "5236ec5f", Name: "5236ec5f", Cwd: "/x", FirstUser: ask}, claude.PromptName(ask)},
		{"bg with title wins over prompt name", Session{SessionID: sid, ShortID: "5236ec5f", Name: claude.PromptName(ask), FirstUser: ask, Title: "People flow BR"}, "People flow BR"},
		{"neko's prompt name before the title", Session{SessionID: sid, ShortID: "5236ec5f", Name: claude.PromptName(ask), FirstUser: ask}, claude.PromptName(ask)},
		{"name you chose stays", Session{SessionID: sid, Name: "todo #12 deploy", Title: "Deploy", FirstUser: ask}, "todo #12 deploy"},
		{"custom name first", Session{SessionID: sid, CustomName: "meu", Title: "x"}, "meu"},
		{"new bg, no transcript yet", Session{SessionID: sid, ShortID: "5236ec5f", Name: "5236ec5f", Cwd: "/home/u/dev/neko", Kind: "background"}, "sessão nova em neko"},
		{"empty", Session{SessionID: sid, Cwd: "/home/u", Kind: "history"}, "conversa vazia em u"},
	}
	for _, c := range cases {
		if got := c.s.Label(); got != c.want {
			t.Errorf("%s: got %q, want %q", c.name, got, c.want)
		}
		if strings.Contains(c.s.Label(), "5236ec5f") {
			t.Errorf("%s: label has the id", c.name)
		}
	}
}

func TestPromptName(t *testing.T) {
	got := claude.PromptName("investiga por que o people flow de BR parou de mandar eventos desde ontem à noite\nmais")
	if got != "investiga por que o people flow de BR parou de…" {
		t.Fatalf("%q", got)
	}
	if got := claude.PromptName("  curto\n segunda linha"); got != "curto" {
		t.Fatalf("%q", got)
	}
	if !claude.NekoInternal(`Chame searchJiraIssuesUsingJql exatamente uma vez com cloudId="x"`) || claude.NekoInternal("Chame o deploy") {
		t.Fatal("NekoInternal")
	}
}
