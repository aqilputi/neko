package lua

import (
	"errors"
	"os"
	"path/filepath"
	"testing"
	"time"

	"github.com/aqilputi/nekuroddo/internal/app"
	"github.com/aqilputi/nekuroddo/internal/app/apptest"
	"github.com/aqilputi/nekuroddo/internal/config"
)

// slack is how late an interrupted call may come back (the race detector
// and a loaded CI box slow everything down).
const slack = 250 * time.Millisecond

type env struct {
	t   *testing.T
	f   *apptest.Fakes
	app *app.App
	rt  *Runtime
	dir string // the nekuroddo config dir (init.lua, plugins/)
}

// newEnv builds an app over fakes and a runtime over a temp XDG config dir.
func newEnv(t *testing.T, opts Options) *env {
	t.Helper()
	base := t.TempDir()
	t.Setenv("XDG_CONFIG_HOME", base)
	t.Setenv("XDG_STATE_HOME", filepath.Join(base, "state"))
	f := apptest.New()
	a := app.New(config.Config{}, f.Ports())
	rt, err := New(a, opts)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { rt.Close(); a.Close() })
	return &env{t: t, f: f, app: a, rt: rt, dir: filepath.Join(base, "nekuroddo")}
}

// write puts a Lua file under the config dir.
func (e *env) write(rel, src string) {
	e.t.Helper()
	p := filepath.Join(e.dir, rel)
	if err := os.MkdirAll(filepath.Dir(p), 0o755); err != nil {
		e.t.Fatal(err)
	}
	if err := os.WriteFile(p, []byte(src), 0o644); err != nil {
		e.t.Fatal(err)
	}
}

func TestInfiniteLoopIsInterruptedWithinBudget(t *testing.T) {
	e := newEnv(t, Options{CommandTimeout: 50 * time.Millisecond})
	start := time.Now()
	_, err := e.rt.Eval("while true do end")
	took := time.Since(start)
	if !errors.Is(err, ErrBudget) {
		t.Fatalf("err = %v, want ErrBudget", err)
	}
	if took > 50*time.Millisecond+slack {
		t.Fatalf("took %v, want ≤ 50ms + %v", took, slack)
	}
	// the VM is still usable afterwards
	got, err := e.rt.Eval("return 1 + 1")
	if err != nil || got != "2" {
		t.Fatalf("Eval after timeout = %q, %v", got, err)
	}
}
