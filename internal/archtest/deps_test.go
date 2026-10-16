package archtest

import (
	"bytes"
	"encoding/json"
	"errors"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"sort"
	"strings"
	"testing"
)

const module = "github.com/aqilputi/nekuroddo"

// pkg is the part of `go list -json` the rule needs.
type pkg struct {
	ImportPath string
	Imports    []string
	Standard   bool
	DepOnly    bool
}

// rule says what the packages under Root may (not) reach.
type rule struct {
	Root string // import path; the rule covers Root and Root/...
	// Forbidden paths; "x/..." also covers x itself.
	Forbidden []string
	// Boundary packages are allowed, and the walk stops there: their own
	// imports are theirs (store reaches os/exec through claude, but core may
	// still use store's types). The plan allows types only; that half is on
	// review, not here.
	Boundary []string
	// InternalOnly, when set, limits which module packages Root may import
	// at all (besides itself and Boundary).
	InternalOnly bool
	// ThirdParty lists the non-module, non-stdlib packages Root may import
	// directly (nil = any not forbidden).
	ThirdParty []string
}

// forbiddenForDomain is plan §2.2's list for core and app.
var forbiddenForDomain = []string{
	"charm.land/bubbletea/...", "charm.land/bubbles/...", "charm.land/lipgloss/...",
	"github.com/charmbracelet/bubbletea/...", "github.com/charmbracelet/bubbles/...",
	"github.com/charmbracelet/lipgloss/...", "github.com/charmbracelet/ultraviolet/...",
	"github.com/charmbracelet/x/vt/...", "github.com/creack/pty/...",
	"os/exec", "net", "net/http/...",
	module + "/internal/term/...", module + "/internal/tui/...",
	module + "/internal/rpc/...", module + "/internal/lua/...", module + "/internal/wire/...",
}

var dataPackages = []string{
	module + "/internal/store", module + "/internal/claude", module + "/internal/groups",
	module + "/internal/events", module + "/internal/jira", module + "/internal/todo",
}

func rules() []rule {
	return []rule{
		{
			Root:         module + "/internal/core",
			Forbidden:    append(append([]string{}, forbiddenForDomain...), module+"/internal/app/..."),
			Boundary:     dataPackages,
			InternalOnly: true,
			ThirdParty:   []string{"github.com/sahilm/fuzzy"},
		},
		{
			Root:      module + "/internal/app",
			Forbidden: forbiddenForDomain,
			// app talks to the world through ports; it may name the adapters'
			// types and read config, but not reach their effects itself.
			Boundary: append(append([]string{}, dataPackages...),
				module+"/internal/config", module+"/internal/paths"),
		},
	}
}

func matches(path, pattern string) bool {
	if base, ok := strings.CutSuffix(pattern, "/..."); ok {
		return path == base || strings.HasPrefix(path, base+"/")
	}
	return path == pattern
}

func matchesAny(path string, patterns []string) bool {
	for _, p := range patterns {
		if matches(path, p) {
			return true
		}
	}
	return false
}

func under(path, root string) bool { return matches(path, root+"/...") }

// violations walks every package under r.Root through its imports and
// reports each forbidden package reached, with the chain that reaches it.
func violations(pkgs map[string]pkg, r rule) []string {
	var out []string
	seen := map[string]bool{}
	type step struct {
		path  string
		chain []string
	}
	var queue []step
	var roots []string
	for p, info := range pkgs {
		if under(p, r.Root) && !info.DepOnly {
			roots = append(roots, p)
		}
	}
	sort.Strings(roots)
	for _, p := range roots {
		queue = append(queue, step{p, []string{p}})
		seen[p] = true
	}
	for len(queue) > 0 {
		s := queue[0]
		queue = queue[1:]
		info := pkgs[s.path]
		inRoot := under(s.path, r.Root)
		for _, imp := range info.Imports {
			chain := append(append([]string{}, s.chain...), imp)
			switch {
			case matchesAny(imp, r.Forbidden):
				out = append(out, strings.Join(chain, " → "))
				continue
			case matchesAny(imp, r.Boundary):
				continue
			}
			if inRoot && !under(imp, r.Root) {
				isModule := under(imp, module)
				std := pkgs[imp].Standard || (!isModule && !strings.Contains(strings.SplitN(imp, "/", 2)[0], "."))
				switch {
				case isModule && r.InternalOnly:
					out = append(out, strings.Join(chain, " → ")+" (not an allowed internal package)")
					continue
				case !isModule && !std && r.ThirdParty != nil && !matchesAny(imp, r.ThirdParty):
					out = append(out, strings.Join(chain, " → ")+" (third-party not allowed)")
					continue
				}
			}
			if !seen[imp] {
				seen[imp] = true
				queue = append(queue, step{imp, chain})
			}
		}
	}
	sort.Strings(out)
	return out
}

func repoRoot(t *testing.T) string {
	t.Helper()
	root, err := filepath.Abs(filepath.Join("..", ".."))
	if err != nil {
		t.Fatal(err)
	}
	if _, err := os.Stat(filepath.Join(root, "go.mod")); err != nil {
		t.Fatalf("no go.mod at %s: %v", root, err)
	}
	return root
}

// hasGoFiles reports whether dir (recursively) has a non-test Go file.
func hasGoFiles(dir string) bool {
	found := false
	_ = filepath.WalkDir(dir, func(p string, d os.DirEntry, err error) error {
		if err == nil && !d.IsDir() && strings.HasSuffix(p, ".go") && !strings.HasSuffix(p, "_test.go") {
			found = true
			return filepath.SkipAll
		}
		return nil
	})
	return found
}

func goList(t *testing.T, root string, patterns []string) map[string]pkg {
	t.Helper()
	goBin := filepath.Join(os.Getenv("GOROOT"), "bin", "go")
	if _, err := os.Stat(goBin); os.Getenv("GOROOT") == "" || err != nil {
		goBin = "go"
	}
	args := append([]string{"list", "-e", "-deps", "-json"}, patterns...)
	cmd := exec.Command(goBin, args...)
	cmd.Dir = root
	var stderr bytes.Buffer
	cmd.Stderr = &stderr
	raw, err := cmd.Output()
	if err != nil {
		t.Fatalf("go list: %v\n%s", err, stderr.String())
	}
	pkgs := map[string]pkg{}
	dec := json.NewDecoder(bytes.NewReader(raw))
	for {
		var p pkg
		if err := dec.Decode(&p); errors.Is(err, io.EOF) {
			break
		} else if err != nil {
			t.Fatalf("decoding go list: %v", err)
		}
		pkgs[p.ImportPath] = p
	}
	return pkgs
}

// TestDependencyRule is the guard: core and app reach nothing forbidden.
func TestDependencyRule(t *testing.T) {
	root := repoRoot(t)
	var patterns []string
	var active []rule
	for _, r := range rules() {
		rel := strings.TrimPrefix(r.Root, module+"/")
		if !hasGoFiles(filepath.Join(root, rel)) {
			t.Logf("%s does not exist yet: skipped", rel)
			continue
		}
		patterns = append(patterns, "./"+rel+"/...")
		active = append(active, r)
	}
	if len(active) == 0 {
		t.Skip("nothing to check")
	}
	pkgs := goList(t, root, patterns)
	for _, r := range active {
		n := 0
		for p := range pkgs {
			if under(p, r.Root) {
				n++
			}
		}
		if n == 0 {
			t.Errorf("%s: go list found no packages", r.Root)
		}
		for _, v := range violations(pkgs, r) {
			t.Errorf("forbidden dependency: %s", v)
		}
	}
}

// The walker itself, on a made-up graph.
func TestViolations(t *testing.T) {
	core := module + "/internal/core"
	g := func(ps ...pkg) map[string]pkg {
		m := map[string]pkg{}
		for _, p := range ps {
			m[p.ImportPath] = p
		}
		return m
	}
	store := module + "/internal/store"
	cases := []struct {
		name string
		g    map[string]pkg
		want []string
	}{
		{"clean: stdlib, fuzzy, data types", g(
			pkg{ImportPath: core, Imports: []string{"fmt", "github.com/sahilm/fuzzy", store}},
			pkg{ImportPath: "fmt", Standard: true, DepOnly: true, Imports: []string{"os"}},
			pkg{ImportPath: "os", Standard: true, DepOnly: true},
			pkg{ImportPath: "github.com/sahilm/fuzzy", DepOnly: true},
			pkg{ImportPath: store, DepOnly: true, Imports: []string{module + "/internal/claude"}},
			pkg{ImportPath: module + "/internal/claude", DepOnly: true, Imports: []string{"os/exec"}},
		), nil},
		{"direct bubbletea", g(
			pkg{ImportPath: core + "/sub", Imports: []string{"charm.land/bubbletea/v2"}},
		), []string{core + "/sub → charm.land/bubbletea/v2"}},
		{"transitive os/exec through stdlib-looking helper", g(
			pkg{ImportPath: core, Imports: []string{"example.com/helper"}},
			pkg{ImportPath: "example.com/helper", DepOnly: true, Imports: []string{"os/exec"}},
		), []string{core + " → example.com/helper (third-party not allowed)"}},
		{"net exact, net/url fine, net/http subtree", g(
			pkg{ImportPath: core, Imports: []string{"net", "net/url", "net/http/httptest"}},
			pkg{ImportPath: "net/url", Standard: true, DepOnly: true},
		), []string{core + " → net", core + " → net/http/httptest"}},
		{"other internal package", g(
			pkg{ImportPath: core, Imports: []string{module + "/internal/theme", module + "/internal/tui"}},
		), []string{core + " → " + module + "/internal/theme (not an allowed internal package)", core + " → " + module + "/internal/tui"}},
		{"forbidden reached through stdlib dep", g(
			pkg{ImportPath: core, Imports: []string{"log"}},
			pkg{ImportPath: "log", Standard: true, DepOnly: true, Imports: []string{"os/exec"}},
		), []string{core + " → log → os/exec"}},
	}
	r := rules()[0]
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			got := violations(c.g, r)
			if strings.Join(got, "\n") != strings.Join(c.want, "\n") {
				t.Fatalf("got\n%s\nwant\n%s", strings.Join(got, "\n"), strings.Join(c.want, "\n"))
			}
		})
	}
	// app may import config; core may not
	app := rules()[1]
	cfg := g(pkg{ImportPath: module + "/internal/app", Imports: []string{module + "/internal/config", module + "/internal/core"}},
		pkg{ImportPath: module + "/internal/config", DepOnly: true, Imports: []string{"os/exec"}},
		pkg{ImportPath: module + "/internal/core", DepOnly: true})
	if v := violations(cfg, app); len(v) != 0 {
		t.Fatalf("app → config is fine: %v", v)
	}
}

func TestMatches(t *testing.T) {
	for _, c := range []struct {
		path, pattern string
		want          bool
	}{
		{"net", "net", true}, {"net/url", "net", false},
		{"net/http", "net/http/...", true}, {"net/http/httptest", "net/http/...", true}, {"net/httpx", "net/http/...", false},
		{"os/exec", "os/exec", true}, {"os", "os/exec", false},
	} {
		if got := matches(c.path, c.pattern); got != c.want {
			t.Errorf("matches(%q,%q)=%v", c.path, c.pattern, got)
		}
	}
}
