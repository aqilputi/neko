package core

import "slices"

// PermBypass is the permission mode that skips Claude Code's prompts: a
// client always confirms it unless it is the configured default.
const PermBypass = "bypassPermissions"

// permModes are the --permission-mode values nekuroddo dispatches with, in
// the order a client cycles them; "" is Claude Code's own setting.
var permModes = []string{"", "acceptEdits", "auto", "plan", PermBypass}

// PermModes is a copy of the permission modes, "" (Claude's own) first.
func PermModes() []string { return slices.Clone(permModes) }

// ValidPermMode reports whether m is one of PermModes.
func ValidPermMode(m string) bool { return slices.Contains(permModes, m) }
