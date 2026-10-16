package core

// Commands are the one vocabulary shared by the keymap, the palette, Lua,
// RPC and the GUI (plan §2.4). They are plain data here; internal/app keeps
// the registry that runs them.

// Arg types a command may ask for.
const (
	ArgString  = "string"
	ArgSession = "session"
	ArgDir     = "dir"
	ArgTag     = "tag"
	ArgInt     = "int"
)

// Modes a binding is active in ("*" = any).
const (
	ModeNormal = "normal"
	ModeInsert = "insert"
	ModeInput  = "input"
	ModeAny    = "*"
)

// Scopes: "core" runs in app; "client" (navigation, folding, zoom) is
// handled by each client but still listed so palettes and keymaps show it.
const (
	ScopeCore   = "core"
	ScopeClient = "client"
)

// SourceBuiltin marks commands compiled in; Lua ones use "lua:<file>".
const SourceBuiltin = "builtin"

// Arg is one argument a command takes.
type Arg struct {
	Name     string `json:"name"`
	Type     string `json:"type"` // ArgString | ArgSession | ArgDir | ArgTag | ArgInt
	Prompt   string `json:"prompt"`
	Required bool   `json:"required"`
}

// Command describes a named action ("session.archive").
type Command struct {
	Name    string   `json:"name"`
	Title   string   `json:"title"` // palette/help text, pt-BR
	Hint    string   `json:"hint"`
	Keys    []string `json:"keys"` // default binding(s); empty = palette only
	Mode    string   `json:"mode"`
	Args    []Arg    `json:"args"`
	Scope   string   `json:"scope"`
	Confirm bool     `json:"confirm"` // may return a PendingConfirmation
	Source  string   `json:"source"`
}

// Help is how a binding shows in the help bar.
type Help struct {
	Key  string `json:"key"`
	Desc string `json:"desc"`
}

// Binding maps keys in a mode to a command (plan §2.7).
type Binding struct {
	Command string   `json:"command"`
	Keys    []string `json:"keys"`
	Mode    string   `json:"mode"`
	Help    Help     `json:"help"`
}

// Keymap is the ordered list of bindings every client resolves keys with
// (GET /v1/keymap).
type Keymap []Binding
