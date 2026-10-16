package rpc

import (
	"errors"
	"net/http"
	"slices"
	"strings"
	"sync"
	"testing"

	"github.com/aqilputi/nekuroddo/internal/rpc/proto"
	"github.com/aqilputi/nekuroddo/internal/theme"
)

func TestThemeActiveAndNamed(t *testing.T) {
	e := newEnv(t) // config.Default(): theme "" (follow the terminal)
	th, err := e.c.Theme(ctxT(t), "")
	if err != nil {
		t.Fatal(err)
	}
	first := theme.All[0]
	if th.Name != first.Name || th.Active != first.Name || th.Tokens["bg"] != theme.Hex(first.Bg) ||
		len(th.Contexts) != len(first.Contexts) || !slices.Equal(th.Themes, theme.Names()) {
		t.Errorf("active theme = %+v", th)
	}
	th, err = e.c.Theme(ctxT(t), "dracula")
	if err != nil {
		t.Fatal(err)
	}
	d, _ := theme.Lookup("dracula")
	if th.Name != "dracula" || th.Tokens["blocked"] != theme.Hex(d.Blocked) || th.Active != first.Name {
		t.Errorf("dracula = %+v", th)
	}
	if _, err := e.c.Theme(ctxT(t), "nope"); !IsCode(err, proto.CodeNotFound) {
		t.Errorf("unknown theme: err = %v", err)
	}
	// ?name= present but empty is the unknown name "", not the active one
	resp, err := e.rawUnix().Get("http://nk/v1/theme?name=")
	if err != nil {
		t.Fatal(err)
	}
	resp.Body.Close()
	if resp.StatusCode != http.StatusNotFound {
		t.Errorf("?name= = %d", resp.StatusCode)
	}
}

func TestThemeFromConfig(t *testing.T) {
	e := newEnvCfg(t, func(c *configT) { c.Theme = "gruvbox" })
	th, err := e.c.Theme(ctxT(t), "")
	if err != nil {
		t.Fatal(err)
	}
	if th.Name != "gruvbox" || th.Active != "gruvbox" {
		t.Errorf("config theme = %+v", th)
	}
	e = newEnvCfg(t, func(c *configT) { c.Theme = "não-existe" })
	if th, _ := e.c.Theme(ctxT(t), ""); th.Name != theme.All[0].Name || th.Active != theme.All[0].Name {
		t.Errorf("unknown config theme = %+v", th)
	}
}

// fakeLua answers Eval from a table and records the code.
type fakeLua struct {
	mu   sync.Mutex
	code []string
}

func (f *fakeLua) Eval(code string) (string, error) {
	f.mu.Lock()
	f.code = append(f.code, code)
	f.mu.Unlock()
	if strings.Contains(code, "error") {
		return "", errors.New("[string \"eval\"]:1: boom")
	}
	return "42", nil
}

func TestLuaEval(t *testing.T) {
	rt := &fakeLua{}
	e := newEnv(t, func(o *Options, _ *fakesT) { o.Lua = rt })
	got, err := e.c.LuaEval(ctxT(t), "return 6*7")
	if err != nil || got.Result != "42" || got.Error != "" {
		t.Errorf("eval = %+v, %v", got, err)
	}
	got, err = e.c.LuaEval(ctxT(t), "error('x')")
	if err != nil || got.Error == "" || got.Result != "" {
		t.Errorf("lua error = %+v, %v", got, err)
	}
	if _, err := e.c.LuaEval(ctxT(t), ""); !IsCode(err, proto.CodeBadRequest) {
		t.Errorf("empty code: %v", err)
	}
	resp, err := e.rawUnix().Post("http://nk/v1/lua/eval", "application/json", strings.NewReader("{"))
	if err != nil {
		t.Fatal(err)
	}
	resp.Body.Close()
	if resp.StatusCode != http.StatusBadRequest {
		t.Errorf("bad json = %d", resp.StatusCode)
	}
	rt.mu.Lock()
	n := len(rt.code)
	rt.mu.Unlock()
	if n != 2 {
		t.Errorf("runtime saw %d chunks, want 2", n)
	}
	h, err := e.c.Hello(ctxT(t))
	if err != nil || !slices.Contains(h.Capabilities, proto.CapLua) {
		t.Errorf("hello = %+v, %v", h, err)
	}
}

func TestLuaCapabilityNotDuplicated(t *testing.T) {
	e := newEnv(t, func(o *Options, _ *fakesT) {
		o.Lua = &fakeLua{}
		o.Capabilities = []string{proto.CapLua, proto.CapDemo}
	})
	h, err := e.c.Hello(ctxT(t))
	if err != nil {
		t.Fatal(err)
	}
	n := 0
	for _, c := range h.Capabilities {
		if c == proto.CapLua {
			n++
		}
	}
	if n != 1 || !slices.Contains(h.Capabilities, proto.CapDemo) {
		t.Errorf("capabilities = %v", h.Capabilities)
	}
}

func TestLuaEvalWithoutRuntime(t *testing.T) {
	e := newEnv(t)
	if _, err := e.c.LuaEval(ctxT(t), "return 1"); !IsCode(err, proto.CodeNotImplemented) {
		t.Errorf("no runtime: %v", err)
	}
	h, _ := e.c.Hello(ctxT(t))
	if slices.Contains(h.Capabilities, proto.CapLua) {
		t.Errorf("hello announces lua without a runtime: %v", h.Capabilities)
	}
}
