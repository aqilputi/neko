package todo

import (
	"encoding/json"
	"errors"
	"os"
	"path/filepath"
	"reflect"
	"strings"
	"testing"
)

// fakeScript is a stand-in for the todo skill's todo.py: it records its argv
// (as JSON) and answers with FAKE_OUT / FAKE_ERR / FAKE_CODE.
const fakeScript = `import json, os, sys
with open(os.environ["FAKE_ARGV"], "w") as f:
    json.dump(sys.argv[1:], f)
sys.stdout.write(os.environ.get("FAKE_OUT", ""))
sys.stderr.write(os.environ.get("FAKE_ERR", ""))
sys.exit(int(os.environ.get("FAKE_CODE", "0")))
`

type fake struct{ argv string }

func newFake(t *testing.T) fake {
	t.Helper()
	dir := t.TempDir()
	s := filepath.Join(dir, "todo.py")
	if err := os.WriteFile(s, []byte(fakeScript), 0o644); err != nil {
		t.Fatal(err)
	}
	f := fake{argv: filepath.Join(dir, "argv.json")}
	t.Setenv("NEKO_TODO_SCRIPT", s)
	t.Setenv("FAKE_ARGV", f.argv)
	t.Setenv("FAKE_OUT", "")
	t.Setenv("FAKE_ERR", "")
	t.Setenv("FAKE_CODE", "0")
	return f
}

func (f fake) args(t *testing.T) []string {
	t.Helper()
	b, err := os.ReadFile(f.argv)
	if err != nil {
		t.Fatalf("todo script was not run: %v", err)
	}
	var a []string
	if err := json.Unmarshal(b, &a); err != nil {
		t.Fatal(err)
	}
	return a
}

func TestScriptAndAvailable(t *testing.T) {
	home := t.TempDir()
	t.Setenv("HOME", home)
	t.Setenv("NEKO_TODO_SCRIPT", "")
	def := filepath.Join(home, ".claude", "skills", "todo", "scripts", "todo.py")
	if script() != def {
		t.Fatalf("script = %q", script())
	}
	if Available() {
		t.Fatal("Available with no script installed")
	}
	os.MkdirAll(filepath.Dir(def), 0o755)
	os.WriteFile(def, nil, 0o644)
	if !Available() {
		t.Fatal("not Available with the script installed")
	}
	t.Setenv("NEKO_TODO_SCRIPT", filepath.Join(home, "other.py"))
	if Available() {
		t.Fatal("override to a missing script should not be Available")
	}
}

func TestList(t *testing.T) {
	f := newFake(t)
	t.Setenv("FAKE_OUT", `[
	 {"group":"neko","tasks":[
	   {"id":12,"title":"Testes","desc":"cobrir","status":"doing","initiative":"neko",
	    "refs":[{"kind":"jira","value":"VIS-1","url":"https://x/VIS-1"}],
	    "contexts":["@repo"],"tags":["go"],"when":"2026-10-16","due":null,"priority":2,
	    "waiting_on":"Silmar","links":["https://l"],"notes":[{"at":"2026-10-15","text":"oi"}]}]},
	 {"group":"(sem iniciativa)","tasks":[]}
	]`)
	gs, err := List()
	if err != nil {
		t.Fatal(err)
	}
	if got := f.args(t); !reflect.DeepEqual(got, []string{"list", "--format", "json"}) {
		t.Fatalf("argv = %q", got)
	}
	if len(gs) != 2 || gs[0].Group != "neko" || len(gs[0].Tasks) != 1 || gs[1].Group != "(sem iniciativa)" {
		t.Fatalf("groups = %+v", gs)
	}
	tk := gs[0].Tasks[0]
	if tk.ID != 12 || tk.Title != "Testes" || tk.Status != "doing" || tk.JiraKey() != "VIS-1" ||
		tk.When == nil || *tk.When != "2026-10-16" || tk.Due != nil ||
		tk.Priority == nil || *tk.Priority != 2 || tk.WaitingOn == nil || *tk.WaitingOn != "Silmar" ||
		!reflect.DeepEqual(tk.Notes, []Note{{At: "2026-10-15", Text: "oi"}}) {
		t.Fatalf("task = %+v", tk)
	}
}

func TestListBadJSON(t *testing.T) {
	newFake(t)
	t.Setenv("FAKE_OUT", "Nada pendente.\n")
	_, err := List()
	var se *json.SyntaxError
	if !errors.As(err, &se) || !strings.HasPrefix(err.Error(), "todo list: ") {
		t.Fatalf("err = %v", err)
	}
}

func TestCommandsArgv(t *testing.T) {
	cases := []struct {
		name string
		call func() error
		want []string
	}{
		{"Start", func() error { return Start(7) }, []string{"start", "7"}},
		{"Done", func() error { return Done(7) }, []string{"done", "7"}},
		{"AddNote", func() error { return AddNote(7, "agent abcd1234: começou") }, []string{"edit", "7", "--note", "agent abcd1234: começou"}},
		{"LinkJira", func() error { return LinkJira(7, "VIS-42") }, []string{"edit", "7", "-r", "jira:VIS-42", "--add-tag", "jira"}},
		{"AddFromJira", func() error { return AddFromJira("VIS-42", "Corrigir login") },
			[]string{"add", "VIS-42 Corrigir login", "-r", "jira:VIS-42", "-t", "jira", "-c", "@jira"}},
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			f := newFake(t)
			if err := c.call(); err != nil {
				t.Fatal(err)
			}
			if got := f.args(t); !reflect.DeepEqual(got, c.want) {
				t.Fatalf("argv = %q, want %q", got, c.want)
			}
		})
	}
}

func TestCommandErrors(t *testing.T) {
	cases := []struct {
		name    string
		call    func() error
		wantErr string
	}{
		{"Start", func() error { return Start(99) }, "todo start 99: no task #99"},
		{"Done", func() error { return Done(99) }, "todo done 99: no task #99"},
		{"AddNote", func() error { return AddNote(99, "x") }, "todo edit 99 --note x: no task #99"},
		{"LinkJira", func() error { return LinkJira(99, "K-1") }, "todo edit 99 -r jira:K-1 --add-tag jira: no task #99"},
		{"AddFromJira", func() error { return AddFromJira("K-1", "s") }, "todo add K-1 s -r jira:K-1 -t jira -c @jira: no task #99"},
		{"List", func() error { _, err := List(); return err }, "todo list --format json: no task #99"},
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			newFake(t)
			t.Setenv("FAKE_ERR", "  no task #99\n")
			t.Setenv("FAKE_CODE", "2")
			if err := c.call(); err == nil || err.Error() != c.wantErr {
				t.Fatalf("err = %v, want %q", err, c.wantErr)
			}
		})
	}
}

func TestNoPython(t *testing.T) {
	newFake(t)
	t.Setenv("PATH", t.TempDir()) // python3 not found
	if err := Start(1); err == nil || strings.HasPrefix(err.Error(), "todo ") {
		t.Fatalf("err = %v, want the raw exec error", err)
	}
}

func TestJiraKey(t *testing.T) {
	cases := []struct {
		refs []Ref
		want string
	}{
		{nil, ""},
		{[]Ref{{Kind: "pr", Value: "#3"}}, ""},
		{[]Ref{{Kind: "pr", Value: "#3"}, {Kind: "jira", Value: "VIS-1"}, {Kind: "jira", Value: "VIS-2"}}, "VIS-1"},
	}
	for _, c := range cases {
		if got := (Task{Refs: c.refs}).JiraKey(); got != c.want {
			t.Errorf("JiraKey(%+v) = %q, want %q", c.refs, got, c.want)
		}
	}
}

func TestPrompt(t *testing.T) {
	const tail = "\nAntes de qualquer ação irreversível ou externa (merge, push, deploy, mensagem), pare e me pergunte. " +
		"Quando terminar, atualize a tarefa com a skill todo (nota com o resultado; marque done só se estiver de fato concluída)."
	cases := []struct {
		name string
		task Task
		want string
	}{
		{"minimal", Task{ID: 3, Title: "Testar"},
			"Trabalhe na tarefa #3 da minha lista de todos: Testar\n" + tail},
		{"full", Task{
			ID: 12, Title: "Login", Desc: "quebra no SSO",
			Refs:       []Ref{{Kind: "jira", Value: "VIS-1", URL: "https://j/VIS-1"}, {Kind: "path", Value: "/repo"}},
			Initiative: "neko", Contexts: []string{"@repo", "@silmar"},
			Links: []string{"https://a", "https://b"},
			Notes: []Note{{At: "2026-10-15", Text: "começou"}},
		}, "Trabalhe na tarefa #12 da minha lista de todos: Login\n" +
			"Descrição: quebra no SSO\n" +
			"Ref jira: VIS-1 https://j/VIS-1\n" +
			"Ref path: /repo \n" +
			"Iniciativa: neko\n" +
			"Contextos: @repo, @silmar\n" +
			"Link: https://a\nLink: https://b\n" +
			"Nota (2026-10-15): começou\n" + tail},
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			if got := Prompt(c.task); got != c.want {
				t.Fatalf("Prompt =\n%s\nwant\n%s", got, c.want)
			}
		})
	}
}
