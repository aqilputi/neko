// Package todo reads and updates the user's task list through the todo
// skill's CLI (~/.claude/skills/todo/scripts/todo.py). It never touches the
// JSON store directly: the CLI locks and writes atomically.
package todo

import (
	"encoding/json"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strconv"
	"strings"
)

type Note struct {
	At   string `json:"at"`
	Text string `json:"text"`
}

type Ref struct {
	Kind  string `json:"kind"` // jira | pr | path | doc | cmd | …
	Value string `json:"value"`
	URL   string `json:"url"`
}

type Task struct {
	ID         int      `json:"id"`
	Title      string   `json:"title"`
	Desc       string   `json:"desc"`
	Refs       []Ref    `json:"refs"`
	Status     string   `json:"status"`
	Initiative string   `json:"initiative"`
	Contexts   []string `json:"contexts"`
	Tags       []string `json:"tags"`
	When       *string  `json:"when"`
	Due        *string  `json:"due"`
	Priority   *int     `json:"priority"`
	WaitingOn  *string  `json:"waiting_on"`
	Links      []string `json:"links"`
	Notes      []Note   `json:"notes"`
}

type Group struct {
	Group string `json:"group"`
	Tasks []Task `json:"tasks"`
}

func script() string {
	if s := os.Getenv("NEKO_TODO_SCRIPT"); s != "" {
		return s
	}
	home, _ := os.UserHomeDir()
	return filepath.Join(home, ".claude", "skills", "todo", "scripts", "todo.py")
}

// Available reports whether the todo skill is installed.
func Available() bool {
	_, err := os.Stat(script())
	return err == nil
}

func run(args ...string) ([]byte, error) {
	out, err := exec.Command("python3", append([]string{script()}, args...)...).Output()
	if err != nil {
		if ee, ok := err.(*exec.ExitError); ok {
			return nil, fmt.Errorf("todo %s: %s", strings.Join(args, " "), strings.TrimSpace(string(ee.Stderr)))
		}
		return nil, err
	}
	return out, nil
}

// List returns open tasks grouped by initiative.
func List() ([]Group, error) {
	out, err := run("list", "--format", "json")
	if err != nil {
		return nil, err
	}
	var gs []Group
	if err := json.Unmarshal(out, &gs); err != nil {
		return nil, fmt.Errorf("todo list: %w", err)
	}
	return gs, nil
}

func Start(id int) error { _, err := run("start", strconv.Itoa(id)); return err }
func Done(id int) error  { _, err := run("done", strconv.Itoa(id)); return err }

// JiraKey returns the task's linked Jira issue, if any.
func (t Task) JiraKey() string {
	for _, r := range t.Refs {
		if r.Kind == "jira" {
			return r.Value
		}
	}
	return ""
}

// LinkJira records that the task now has a Jira issue.
func LinkJira(id int, key string) error {
	_, err := run("edit", strconv.Itoa(id), "-r", "jira:"+key, "--add-tag", "jira")
	return err
}

// AddFromJira creates a task that tracks a Jira issue.
func AddFromJira(key, summary string) error {
	_, err := run("add", key+" "+summary, "-r", "jira:"+key, "-t", "jira", "-c", "@jira")
	return err
}

// AddNote appends a note to a task (used to link it to the agent working on it).
func AddNote(id int, text string) error {
	_, err := run("edit", strconv.Itoa(id), "--note", text)
	return err
}

// Prompt builds the prompt for an agent dispatched to work on t.
func Prompt(t Task) string {
	var b strings.Builder
	fmt.Fprintf(&b, "Trabalhe na tarefa #%d da minha lista de todos: %s\n", t.ID, t.Title)
	if t.Desc != "" {
		fmt.Fprintf(&b, "Descrição: %s\n", t.Desc)
	}
	for _, r := range t.Refs {
		fmt.Fprintf(&b, "Ref %s: %s %s\n", r.Kind, r.Value, r.URL)
	}
	if t.Initiative != "" {
		fmt.Fprintf(&b, "Iniciativa: %s\n", t.Initiative)
	}
	if len(t.Contexts) > 0 {
		fmt.Fprintf(&b, "Contextos: %s\n", strings.Join(t.Contexts, ", "))
	}
	for _, l := range t.Links {
		fmt.Fprintf(&b, "Link: %s\n", l)
	}
	for _, n := range t.Notes {
		fmt.Fprintf(&b, "Nota (%s): %s\n", n.At, n.Text)
	}
	b.WriteString("\nAntes de qualquer ação irreversível ou externa (merge, push, deploy, mensagem), pare e me pergunte. ")
	b.WriteString("Quando terminar, atualize a tarefa com a skill todo (nota com o resultado; marque done só se estiver de fato concluída).")
	return b.String()
}
