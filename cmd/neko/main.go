// neko is a TUI that orchestrates Claude Code sessions. It wraps the
// installed `claude` binary rather than reimplementing it, so it keeps up
// with Claude Code updates on its own.
//
//	neko               the TUI
//	neko hook          Claude Code hook command: records the event for neko
//	neko hook-context  synchronous hook: tells a grouped session about its peers
package main

import (
	"fmt"
	"os"

	tea "charm.land/bubbletea/v2"

	"github.com/aqilputi/neko/internal/config"
	"github.com/aqilputi/neko/internal/events"
	"github.com/aqilputi/neko/internal/groups"
	"github.com/aqilputi/neko/internal/ui"
)

func main() {
	if len(os.Args) > 1 {
		switch os.Args[1] {
		case "hook":
			events.Record(os.Stdin)
			return // always exit 0, print nothing: never affect the Claude session
		case "hook-context":
			groups.Hook(os.Stdin, os.Stdout)
			return // always exit 0; prints only the context to add
		default:
			// never start the TUI from a hook with a typo
			fmt.Fprintf(os.Stderr, "neko: comando desconhecido %q (use: neko, neko hook, neko hook-context)\n", os.Args[1])
			os.Exit(1) // not 2: a hook exiting 2 blocks the session
		}
	}
	cfg, err := config.Load()
	if err != nil {
		fmt.Fprintf(os.Stderr, "neko: %s: %v (usando padrões)\n", config.Path(), err)
	}
	final, err := tea.NewProgram(ui.New(cfg)).Run()
	if err != nil {
		fmt.Fprintln(os.Stderr, "neko:", err)
		os.Exit(1)
	}
	if m, ok := final.(ui.Model); ok {
		fmt.Print(m.ExitBanner())
	}
}
