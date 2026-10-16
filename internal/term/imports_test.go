package term

import (
	"go/parser"
	"go/token"
	"path/filepath"
	"strconv"
	"strings"
	"testing"
)

// The Process half must stay framework-free so the core can drive it without
// a TUI: only screen.go and keys.go may import Bubble Tea, and the PTY side
// (process.go, osc.go) imports no rendering package at all.
func TestImportBoundaries(t *testing.T) {
	files, err := filepath.Glob("*.go")
	if err != nil {
		t.Fatal(err)
	}
	rendering := []string{"charm.land/bubbletea", "github.com/charmbracelet/ultraviolet", "github.com/charmbracelet/x/vt"}
	seen := map[string]bool{}
	for _, f := range files {
		if strings.HasSuffix(f, "_test.go") {
			continue
		}
		seen[f] = true
		af, err := parser.ParseFile(token.NewFileSet(), f, nil, parser.ImportsOnly)
		if err != nil {
			t.Fatal(err)
		}
		for _, imp := range af.Imports {
			path, _ := strconv.Unquote(imp.Path.Value)
			if strings.HasPrefix(path, "charm.land/bubbletea") && f != "screen.go" && f != "keys.go" {
				t.Errorf("%s imports %s: Bubble Tea belongs in screen.go/keys.go only", f, path)
			}
			if f == "process.go" || f == "osc.go" {
				for _, r := range rendering {
					if strings.HasPrefix(path, r) {
						t.Errorf("%s imports %s: the Process half must not depend on rendering", f, path)
					}
				}
			}
		}
	}
	for _, f := range []string{"process.go", "screen.go", "osc.go"} {
		if !seen[f] {
			t.Errorf("%s missing: the boundary check would pass vacuously", f)
		}
	}
}
