package archtest

import (
	"fmt"
	"go/ast"
	"go/parser"
	"go/token"
	"os"
	"path"
	"path/filepath"
	"sort"
	"strconv"
	"strings"
	"testing"
)

// The TUI is a translator (plan §2.2, WP-2.7): keys and messages in, calls
// to the core out, the core's state drawn. It reaches the world only through
// internal/app. This rule is per file and per call, on the source, because
// "may use store's types but not call store" is finer than an import graph.

// uiRule says what one shell package's non-test files may import and call.
type uiRule struct {
	Dir string // package directory, relative to the module root
	// Internal lists the module packages it may import (paths relative to
	// the module). For TypesOnly ones, calling a package-level function is
	// a violation unless it is in AllowFuncs ("pkg.Func").
	Internal   []string
	TypesOnly  []string
	AllowFuncs []string
	// OnlyIn limits an import to the named files.
	OnlyIn map[string][]string
	// Forbidden stdlib/third-party imports (an OnlyIn entry overrides).
	Forbidden []string
}

func uiRules() []uiRule {
	return []uiRule{{
		Dir: "internal/tui",
		// no internal/wire: the composition root is cmd/nekuroddo, and the
		// TUI gets the App it built (tui.New(a, cfg))
		Internal: []string{
			"internal/app", "internal/core", "internal/theme", "internal/cat", "internal/term",
			"internal/config", "internal/store", "internal/claude", "internal/events",
			"internal/groups", "internal/jira", "internal/todo",
		},
		TypesOnly: []string{
			"internal/store", "internal/claude", "internal/events", "internal/groups",
			"internal/jira", "internal/todo",
		},
		// pure helpers that build text from a value
		AllowFuncs: []string{"jira.AgentPrompt", "todo.Prompt"},
		OnlyIn: map[string][]string{
			// tea.ExecProcess takes an *exec.Cmd: fullscreen builds the one
			// app.Fullscreen describes (Result.Exec); nothing else runs here
			"os/exec": {"pane.go"},
		},
		Forbidden: []string{"os/exec", "net", "net/http/...", "github.com/creack/pty/...", module + "/internal/wire/..."},
	}}
}

// packageFuncs lists the exported package-level functions declared in dir
// (non-test files): what "calling the package" means.
func packageFuncs(dir string) (map[string]bool, error) {
	fset := token.NewFileSet()
	entries, err := os.ReadDir(dir)
	if err != nil {
		return nil, err
	}
	out := map[string]bool{}
	for _, e := range entries {
		n := e.Name()
		if e.IsDir() || !strings.HasSuffix(n, ".go") || strings.HasSuffix(n, "_test.go") {
			continue
		}
		f, err := parser.ParseFile(fset, filepath.Join(dir, n), nil, parser.SkipObjectResolution)
		if err != nil {
			return nil, err
		}
		for _, d := range f.Decls {
			if fd, ok := d.(*ast.FuncDecl); ok && fd.Recv == nil && fd.Name.IsExported() {
				out[fd.Name.Name] = true
			}
		}
	}
	return out, nil
}

// srcFile is one parsed file of the package under the rule.
type srcFile struct {
	name string
	file *ast.File
}

// uiViolations checks files against r. funcs maps an import path to its
// package-level functions (for the TypesOnly packages).
func uiViolations(fset *token.FileSet, files []srcFile, r uiRule, funcs map[string]map[string]bool) []string {
	internal := map[string]bool{}
	for _, p := range r.Internal {
		internal[module+"/"+p] = true
	}
	typesOnly := map[string]bool{}
	for _, p := range r.TypesOnly {
		typesOnly[module+"/"+p] = true
	}
	allowed := map[string]bool{}
	for _, f := range r.AllowFuncs {
		allowed[f] = true
	}
	var out []string
	bad := func(pos token.Pos, format string, args ...any) {
		p := fset.Position(pos)
		out = append(out, fmt.Sprintf("%s:%d: %s", filepath.Join(r.Dir, filepath.Base(p.Filename)), p.Line, fmt.Sprintf(format, args...)))
	}
	for _, sf := range files {
		names := map[string]string{} // local name → import path
		for _, im := range sf.file.Imports {
			p, _ := strconv.Unquote(im.Path.Value)
			local := path.Base(p)
			if im.Name != nil {
				local = im.Name.Name
			}
			names[local] = p
			if files, ok := r.OnlyIn[p]; ok {
				if !contains(files, sf.name) {
					bad(im.Pos(), "%s may only be imported in %s", p, strings.Join(files, ", "))
				}
				continue
			}
			switch {
			case matchesAny(p, r.Forbidden):
				bad(im.Pos(), "imports %s", p)
			case under(p, module) && !internal[p]:
				bad(im.Pos(), "imports %s (not an allowed internal package)", p)
			}
		}
		// what each selector on an imported package reaches, and where
		for _, d := range sf.file.Decls {
			ast.Inspect(d, func(n ast.Node) bool {
				sel, ok := n.(*ast.SelectorExpr)
				if !ok {
					return true
				}
				id, ok := sel.X.(*ast.Ident)
				if !ok {
					return true
				}
				p, ok := names[id.Name]
				if !ok {
					return true
				}
				if typesOnly[p] && funcs[p][sel.Sel.Name] && !allowed[id.Name+"."+sel.Sel.Name] {
					bad(sel.Pos(), "calls %s.%s: %s is for its types only (go through internal/app)", id.Name, sel.Sel.Name, p)
				}
				return true
			})
		}
	}
	sort.Strings(out)
	return out
}

func contains(xs []string, x string) bool {
	for _, s := range xs {
		if s == x {
			return true
		}
	}
	return false
}

func parseDir(t *testing.T, fset *token.FileSet, dir string) []srcFile {
	t.Helper()
	entries, err := os.ReadDir(dir)
	if err != nil {
		t.Fatal(err)
	}
	var files []srcFile
	for _, e := range entries {
		n := e.Name()
		if e.IsDir() || !strings.HasSuffix(n, ".go") || strings.HasSuffix(n, "_test.go") {
			continue
		}
		f, err := parser.ParseFile(fset, filepath.Join(dir, n), nil, parser.SkipObjectResolution)
		if err != nil {
			t.Fatal(err)
		}
		files = append(files, srcFile{name: n, file: f})
	}
	return files
}

// TestTUIRule is the guard for the TUI: it imports only the core, its own
// presentation packages and the data packages' types.
func TestTUIRule(t *testing.T) {
	root := repoRoot(t)
	for _, r := range uiRules() {
		fset := token.NewFileSet()
		files := parseDir(t, fset, filepath.Join(root, r.Dir))
		if len(files) == 0 {
			t.Fatalf("%s: no files", r.Dir)
		}
		funcs := map[string]map[string]bool{}
		for _, p := range r.TypesOnly {
			fs, err := packageFuncs(filepath.Join(root, p))
			if err != nil {
				t.Fatal(err)
			}
			funcs[module+"/"+p] = fs
		}
		for _, v := range uiViolations(fset, files, r, funcs) {
			t.Errorf("TUI rule: %s", v)
		}
	}
}

// The checker itself, on made-up files.
func TestUIViolations(t *testing.T) {
	r := uiRules()[0]
	funcs := map[string]map[string]bool{
		module + "/internal/store":  {"New": true},
		module + "/internal/groups": {"Load": true, "Queue": true},
		module + "/internal/jira":   {"AgentPrompt": true, "LoadCache": true},
		module + "/internal/todo":   {"Prompt": true, "List": true},
	}
	check := func(name, src string) []string {
		t.Helper()
		fset := token.NewFileSet()
		f, err := parser.ParseFile(fset, filepath.Join(r.Dir, name), src, parser.SkipObjectResolution)
		if err != nil {
			t.Fatal(err)
		}
		return uiViolations(fset, []srcFile{{name: name, file: f}}, r, funcs)
	}
	cases := []struct {
		name, file, src string
		want            []string
	}{
		{"types and allowed helpers are fine", "x.go", `package tui
import (
	"fmt"
	"github.com/aqilputi/nekuroddo/internal/store"
	"github.com/aqilputi/nekuroddo/internal/jira"
	"github.com/aqilputi/nekuroddo/internal/todo"
)
var _ store.Session
func f(is jira.Issue, t todo.Task) string { return fmt.Sprint(store.AttnNone, jira.AgentPrompt(is), todo.Prompt(t)) }`, nil},
		{"calling a data package", "x.go", `package tui
import (
	"github.com/aqilputi/nekuroddo/internal/groups"
	j "github.com/aqilputi/nekuroddo/internal/jira"
)
func f() { _ = groups.Load(); _ = j.LoadCache() }`, []string{
			"internal/tui/x.go:6: calls groups.Load: " + module + "/internal/groups is for its types only (go through internal/app)",
			"internal/tui/x.go:6: calls j.LoadCache: " + module + "/internal/jira is for its types only (go through internal/app)",
		}},
		{"os/exec outside pane.go, net, other internal", "x.go", `package tui
import (
	"os/exec"
	"net/http"
	"github.com/aqilputi/nekuroddo/internal/paths"
)
var _ = exec.Command
var _ = http.Get
var _ = paths.StateDir`, []string{
			"internal/tui/x.go:3: os/exec may only be imported in pane.go",
			"internal/tui/x.go:4: imports net/http",
			"internal/tui/x.go:5: imports " + module + "/internal/paths (not an allowed internal package)",
		}},
		{"os/exec in pane.go", "pane.go", `package tui
import "os/exec"
var _ = exec.Command`, nil},
		{"wire nowhere, not even in New", "model.go", `package tui
import "github.com/aqilputi/nekuroddo/internal/wire"
func New() { _ = wire.Default }`, []string{"internal/tui/model.go:2: imports " + module + "/internal/wire"}},
		{"nor lua or rpc", "x.go", `package tui
import (
	"github.com/aqilputi/nekuroddo/internal/lua"
	"github.com/aqilputi/nekuroddo/internal/rpc"
)
var _ = lua.New
var _ = rpc.New`, []string{
			"internal/tui/x.go:3: imports " + module + "/internal/lua (not an allowed internal package)",
			"internal/tui/x.go:4: imports " + module + "/internal/rpc (not an allowed internal package)",
		}},
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			got := check(c.file, c.src)
			if strings.Join(got, "\n") != strings.Join(c.want, "\n") {
				t.Fatalf("got\n%s\nwant\n%s", strings.Join(got, "\n"), strings.Join(c.want, "\n"))
			}
		})
	}
	if fs, err := packageFuncs(filepath.Join(repoRoot(t), "internal/groups")); err != nil || !fs["Load"] || fs["Save"] {
		t.Fatalf("groups' functions (Load yes, the method Save no): %v %v", fs, err)
	}
	if _, err := packageFuncs(filepath.Join(t.TempDir(), "nope")); err == nil {
		t.Fatal("a missing dir errs")
	}
}
