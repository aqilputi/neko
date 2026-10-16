// Package archtest only holds tests: they fail when a package breaks the
// dependency rule of docs/plano-arquitetura.md §2.2: core and app reach no
// framework or process (deps_test.go), and the TUI reaches the world only
// through app (tui_test.go).
package archtest
