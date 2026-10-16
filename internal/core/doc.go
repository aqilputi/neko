// Package core is nekuroddo's pure domain: the session tree with its frozen
// order, the inbox, small formatting rules, and commands/keymap/events as
// plain data. It does no I/O and imports no UI framework; every client (the
// TUI, the RPC server, Lua, the GUI) computes the same answers from it.
//
// Allowed imports: the standard library, github.com/sahilm/fuzzy, and the
// data types of internal/{store,claude,groups,events,jira,todo}. Only their
// types and pure methods (such as store.Session.Label) may be used, never
// functions that touch disk, processes or the network. internal/archtest
// enforces the import rule; the "types only" part is on review.
package core
