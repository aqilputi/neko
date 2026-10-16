// nekuroddo is a TUI that orchestrates Claude Code sessions. It wraps the
// installed `claude` binary rather than reimplementing it, so it keeps up
// with Claude Code updates on its own. It is usually invoked as `nk` (a
// symlink); it behaves the same whatever name it runs under.
//
//	nk [--listen <addr>]       the TUI (with --listen, also the RPC server, same core)
//	nk serve [--listen <addr>] the core headless, served over RPC (no TUI)
//	nk run [<cmd> [k=v…]]      runs a command on a running nk serve / nk --listen
//	nk hook                    Claude Code hook command: records the event for nekuroddo
//	nk hook-context            synchronous hook: tells a grouped session about its peers
package main

import (
	"context"
	"errors"
	"flag"
	"fmt"
	"io"
	"os"
	"strings"

	tea "charm.land/bubbletea/v2"

	"github.com/aqilputi/nekuroddo/internal/app"
	"github.com/aqilputi/nekuroddo/internal/config"
	"github.com/aqilputi/nekuroddo/internal/events"
	"github.com/aqilputi/nekuroddo/internal/groups"
	"github.com/aqilputi/nekuroddo/internal/tui"
)

// version is what hello reports (set with -ldflags "-X main.version=…").
var version = "dev"

const usageText = `uso:
  nk [--listen <endereço>]        a TUI; com --listen, também o servidor RPC (mesmo core)
  nk serve [--listen <endereço>]  o core sem TUI, servido por RPC (padrão: unix://%s)
  nk run [<comando> [arg=valor …]] [--session <id>] [--yes] [--addr <endereço>]
                                  roda um comando num nk serve / nk --listen; sem comando, lista
  nk hook                         hook do Claude Code: registra o evento (sempre sai 0)
  nk hook-context                 hook síncrono: conta a uma sessão agrupada quem são os pares
  nk help                         esta ajuda

endereços: unix:///caminho ou tcp://127.0.0.1:<porta> (TCP exige o token gravado 0600 em %s)
`

func main() {
	if code := run(os.Args[1:], os.Stdin, os.Stdout, os.Stderr); code != 0 {
		os.Exit(code)
	}
}

// run is the whole program minus os.Exit, so the subcommands can be tested.
func run(args []string, stdin io.Reader, stdout, stderr io.Writer) int {
	return runContext(context.Background(), args, stdin, stdout, stderr)
}

// runContext is run under ctx: nk serve stops when it ends (tests cancel
// it; a real serve also stops on SIGINT/SIGTERM).
func runContext(ctx context.Context, args []string, stdin io.Reader, stdout, stderr io.Writer) int {
	// Move ~/.config/neko and friends to their nekuroddo names before
	// anything reads them; cheap (a few stats) since hooks run it too.
	config.MigrateLegacy()
	if len(args) > 0 && !strings.HasPrefix(args[0], "-") {
		// The hooks come first and touch nothing else: never the core, wire,
		// Lua or the server (TestHooksDoNotBuildTheApp).
		switch args[0] {
		case "hook":
			events.Record(stdin)
			return 0 // always exit 0, print nothing: never affect the Claude session
		case "hook-context":
			groups.Hook(stdin, stdout)
			return 0 // always exit 0; prints only the context to add
		case "serve":
			return serve(ctx, args[1:], stdout, stderr)
		case "run":
			return runRemote(ctx, args[1:], stdin, stdout, stderr)
		case "help":
			usage(stdout)
			return 0
		default:
			// never start the TUI from a hook with a typo
			fmt.Fprintf(stderr, "nekuroddo: comando desconhecido %q (use: nk, nk serve, nk run, nk hook, nk hook-context, nk help)\n", args[0])
			return 1 // not 2: a hook exiting 2 blocks the session
		}
	}
	return startTUI(ctx, args, stdout, stderr)
}

func usage(w io.Writer) {
	fmt.Fprintf(w, usageText, config.Tilde(defaultSocket()), config.Tilde(defaultToken()))
}

// loadConfig reads the config; a broken one is a warning (defaults apply).
func loadConfig(stderr io.Writer) config.Config {
	cfg, err := config.Load()
	if err != nil {
		fmt.Fprintf(stderr, "nekuroddo: %s: %v (usando padrões)\n", config.Path(), err)
	}
	return cfg
}

// runTUI runs the Bubble Tea program (tests swap it: there is no terminal).
var runTUI = func(m tea.Model) (tea.Model, error) { return tea.NewProgram(m).Run() }

// startTUI is plain nk: the TUI over a core built here, and with --listen the
// RPC server on that same core, in the same process.
func startTUI(ctx context.Context, args []string, stdout, stderr io.Writer) int {
	fs := newFlags("nk", stderr)
	listen := fs.String("listen", "", "também serve o core por RPC neste endereço (unix:///… ou tcp://127.0.0.1:0)")
	fs.Usage = func() {}
	if err := fs.Parse(args); err != nil {
		if errors.Is(err, flag.ErrHelp) {
			usage(stdout)
			return 0
		}
		usage(stderr)
		return 2
	}
	if fs.NArg() > 0 {
		fmt.Fprintf(stderr, "nekuroddo: argumento inesperado %q\n", fs.Arg(0))
		return 2
	}
	cfg := loadConfig(stderr)
	h := newHost(cfg, stderr, tuiCaps...)
	defer h.Close()
	m := tui.New(h.app, cfg).WithLua(h.lua)
	if *listen != "" {
		srv, addr, err := listenRPC(h, *listen)
		if err != nil {
			fmt.Fprintln(stderr, "nekuroddo:", err)
			return 1
		}
		defer srv.Close()
		fmt.Fprintf(stderr, "nekuroddo: servindo em %s\n", addr)
		m = m.WithPaneHook(paneHook(srv)) // a client attaching a session the TUI shows shares its PTY
	}
	start, stop := scheduleOnce(ctx, h.app, app.ScheduleFrom(cfg))
	defer stop()
	final, err := runTUI(m.WithScheduler(start))
	if err != nil {
		fmt.Fprintln(stderr, "nekuroddo:", err)
		return 1
	}
	if m, ok := final.(tui.Model); ok {
		fmt.Fprint(stdout, m.ExitBanner())
	}
	return 0
}
