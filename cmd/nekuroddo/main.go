// nekuroddo is a TUI that orchestrates Claude Code sessions. It wraps the
// installed `claude` binary rather than reimplementing it, so it keeps up
// with Claude Code updates on its own. It is usually invoked as `nk` (a
// symlink); it behaves the same whatever name it runs under.
//
//	nk               the TUI
//	nk hook          Claude Code hook command: records the event for nekuroddo
//	nk hook-context  synchronous hook: tells a grouped session about its peers
package main

import (
	"fmt"
	"io"
	"os"

	tea "charm.land/bubbletea/v2"

	"github.com/aqilputi/nekuroddo/internal/config"
	"github.com/aqilputi/nekuroddo/internal/events"
	"github.com/aqilputi/nekuroddo/internal/groups"
	"github.com/aqilputi/nekuroddo/internal/ui"
)

func main() {
	if code := run(os.Args[1:], os.Stdin, os.Stdout, os.Stderr); code != 0 {
		os.Exit(code)
	}
}

// run is the whole program minus os.Exit, so the subcommands can be tested.
func run(args []string, stdin io.Reader, stdout, stderr io.Writer) int {
	// Move ~/.config/neko and friends to their nekuroddo names before
	// anything reads them; cheap (a few stats) since hooks run it too.
	config.MigrateLegacy()
	if len(args) > 0 {
		switch args[0] {
		case "hook":
			events.Record(stdin)
			return 0 // always exit 0, print nothing: never affect the Claude session
		case "hook-context":
			groups.Hook(stdin, stdout)
			return 0 // always exit 0; prints only the context to add
		default:
			// never start the TUI from a hook with a typo
			fmt.Fprintf(stderr, "nekuroddo: comando desconhecido %q (use: nk, nk hook, nk hook-context)\n", args[0])
			return 1 // not 2: a hook exiting 2 blocks the session
		}
	}
	cfg, err := config.Load()
	if err != nil {
		fmt.Fprintf(stderr, "nekuroddo: %s: %v (usando padrões)\n", config.Path(), err)
	}
	final, err := tea.NewProgram(ui.New(cfg)).Run()
	if err != nil {
		fmt.Fprintln(stderr, "nekuroddo:", err)
		return 1
	}
	if m, ok := final.(ui.Model); ok {
		fmt.Fprint(stdout, m.ExitBanner())
	}
	return 0
}
