// neko is a TUI that orchestrates Claude Code sessions. It wraps the
// installed `claude` binary rather than reimplementing it, so it keeps up
// with Claude Code updates on its own.
//
//	neko        the TUI
//	neko hook   Claude Code hook command: records the event for neko
package main

import (
	"fmt"
	"os"

	tea "charm.land/bubbletea/v2"

	"github.com/aqilputi/neko/internal/config"
	"github.com/aqilputi/neko/internal/events"
	"github.com/aqilputi/neko/internal/ui"
)

func main() {
	if len(os.Args) > 1 && os.Args[1] == "hook" {
		events.Record(os.Stdin)
		return // always exit 0, print nothing: never affect the Claude session
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
