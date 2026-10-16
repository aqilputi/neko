package lua

import (
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"time"

	"github.com/aqilputi/nekuroddo/internal/app"
	"github.com/aqilputi/nekuroddo/internal/core"
)

// The commands that drive the runtime itself (RegisterCommands). They are
// builtins, not Lua: they stay when the runtime closes or a file is dropped.
const (
	CmdReload   = "lua.reload"
	CmdMessages = "lua.messages"
	CmdEnable   = "lua.enable"
)

// ErrDisabled is a reload of a runtime the config turned off.
var ErrDisabled = errors.New("lua: desativado no config.toml ([lua] enabled = false)")

// AutoReloadEvery is how often AutoReload looks at the files by default.
const AutoReloadEvery = time.Second

// RegisterCommands adds lua.reload, lua.messages and lua.enable to the
// app's registry: they run from every client (palette, keys, RPC, nk run).
func (r *Runtime) RegisterCommands() error {
	reg := r.app.Registry()
	return errors.Join(
		reg.Register(core.Command{Name: CmdReload, Title: "recarrega o init.lua e os plugins", Hint: "lua"},
			func(app.Context, app.Args) (app.Result, error) {
				if err := r.Reload(); err != nil {
					return app.Fail(err), err
				}
				return app.Info(fmt.Sprintf("lua recarregado (%d arquivos)", len(r.Files()))), nil
			}),
		reg.Register(core.Command{Name: CmdMessages, Title: "últimos erros e mensagens lua", Hint: "lua"},
			func(app.Context, app.Args) (app.Result, error) {
				msgs := r.Messages()
				return app.Result{Toast: messagesSummary(msgs), Data: msgs}, nil
			}),
		reg.Register(core.Command{
			Name: CmdEnable, Title: "religa um arquivo lua desativado", Hint: "lua",
			Args: []core.Arg{{Name: "file", Type: core.ArgString, Prompt: "arquivo (ex.: plugins/x.lua)", Required: true}},
		}, func(_ app.Context, args app.Args) (app.Result, error) {
			file := args.String("file")
			if err := r.Enable(file); err != nil {
				return app.Result{Toast: err.Error() + " · " + enableHint(r.Disabled()), Err: true}, err
			}
			return app.Info("lua: " + file + " religado"), nil
		}),
	)
}

// messagesSummary is the toast of lua.messages: how many and the latest.
func messagesSummary(msgs []Message) string {
	if len(msgs) == 0 {
		return "lua: nenhuma mensagem"
	}
	last := msgs[len(msgs)-1]
	text := last.Text
	if last.File != "" && !strings.Contains(text, last.File) {
		text = last.File + ": " + text
	}
	noun := "mensagens"
	if len(msgs) == 1 {
		noun = "mensagem"
	}
	return fmt.Sprintf("lua: %d %s · última: %s", len(msgs), noun, text)
}

// enableHint says which files lua.enable can turn back on.
func enableHint(disabled []string) string {
	if len(disabled) == 0 {
		return "nenhum arquivo desativado"
	}
	return "desativados: " + strings.Join(disabled, ", ")
}

// AutoReload watches init.lua and plugins/*.lua (by polling: no fsnotify)
// every period and reloads when a file appears, goes away or changes (mtime
// or size). It runs until Close. A second call, a disabled or closed
// runtime, or a period ≤ 0 do nothing.
func (r *Runtime) AutoReload(every time.Duration) {
	if every <= 0 || r.opts.Disabled {
		return
	}
	r.mu.Lock()
	if r.closed || r.watching {
		r.mu.Unlock()
		return
	}
	r.watching = true
	r.bg.Add(1) // under mu: Close cannot be waiting on bg yet
	r.mu.Unlock()
	last, _ := r.fingerprint()
	go func() {
		defer r.bg.Done()
		t := time.NewTicker(every)
		defer t.Stop()
		for {
			select {
			case <-r.quit:
				return
			case <-t.C:
			}
			now, err := r.fingerprint()
			if err != nil || now == last {
				continue
			}
			last = now
			_ = r.Reload() // each file's error is already reported (toast, ring, lua.error)
		}
	}()
}

// fingerprint is the files Reload would read, with their mtime and size.
func (r *Runtime) fingerprint() (string, error) {
	names, err := r.discover()
	if err != nil {
		return "", err
	}
	var b strings.Builder
	for _, n := range names {
		if st, err := os.Stat(filepath.Join(r.opts.ConfigDir, n)); err == nil {
			fmt.Fprintf(&b, "%s\x00%d\x00%d\n", n, st.ModTime().UnixNano(), st.Size())
		}
	}
	return b.String(), nil
}
