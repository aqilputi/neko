package store

import (
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
)

// overrides are the user's own corrections, kept in
// ~/.local/state/neko/overrides.json: a name per session, a context per
// session (the cwd rule gets sessions in ~ wrong), and a display name per
// context (renaming a whole section of the tree).
type overrides struct {
	Names       map[string]string `json:"names,omitempty"`         // sessionId → name
	Contexts    map[string]string `json:"contexts,omitempty"`      // sessionId → context
	ContextName map[string]string `json:"context_names,omitempty"` // rule's context → shown name
}

func (s *Store) loadOverrides() {
	if b, err := os.ReadFile(statePath("overrides.json")); err == nil {
		_ = json.Unmarshal(b, &s.ov)
	}
	if s.ov.Names == nil {
		s.ov.Names = map[string]string{}
	}
	if s.ov.Contexts == nil {
		s.ov.Contexts = map[string]string{}
	}
	if s.ov.ContextName == nil {
		s.ov.ContextName = map[string]string{}
	}
}

func (s *Store) saveOverrides() {
	_ = os.MkdirAll(filepath.Dir(statePath("overrides.json")), 0o755)
	if b, err := json.MarshalIndent(s.ov, "", "  "); err == nil {
		_ = os.WriteFile(statePath("overrides.json"), b, 0o644)
	}
}

func setOrDelete(m map[string]string, k, v string) {
	if v = strings.TrimSpace(v); v == "" {
		delete(m, k)
	} else {
		m[k] = v
	}
}

// Rename gives a session its own name; "" goes back to Claude's.
func (s *Store) Rename(sessionID, name string) {
	s.mu.Lock()
	defer s.mu.Unlock()
	setOrDelete(s.ov.Names, sessionID, name)
	s.saveOverrides()
}

// SetContext moves a session to another context; "" goes back to the cwd rule.
func (s *Store) SetContext(sessionID, ctx string) {
	s.mu.Lock()
	defer s.mu.Unlock()
	setOrDelete(s.ov.Contexts, sessionID, ctx)
	s.saveOverrides()
}

// RenameContext renames a context (a section of the tree) for every session
// in it, as shown now. name "" undoes a rename. Sessions moved into it by
// hand follow along.
func (s *Store) RenameContext(shown, name string) {
	s.mu.Lock()
	defer s.mu.Unlock()
	name = strings.TrimSpace(name)
	orig := shown
	for o, n := range s.ov.ContextName {
		if n == shown { // already renamed once
			orig = o
		}
	}
	if name == "" || name == orig {
		delete(s.ov.ContextName, orig)
		name = orig
	} else {
		s.ov.ContextName[orig] = name
	}
	for id, c := range s.ov.Contexts {
		if c == shown {
			s.ov.Contexts[id] = name
		}
	}
	s.saveOverrides()
}

// applyOverrides sets the session's own name and context, given the
// context the cwd rule picked.
func (s *Store) applyOverrides(ss *Session) {
	s.mu.Lock()
	defer s.mu.Unlock()
	ss.CustomName = s.ov.Names[ss.SessionID]
	if c, ok := s.ov.Contexts[ss.SessionID]; ok {
		ss.Context, ss.ContextSet = c, true
		return
	}
	if n, ok := s.ov.ContextName[ss.Context]; ok {
		ss.Context = n
	}
}

// Reapply recomputes names and contexts of sessions already in a snapshot,
// so a change shows before the next Refresh.
func (s *Store) Reapply(sessions []Session) {
	for i := range sessions {
		sessions[i].Context = s.cfg.ContextFor(sessions[i].Cwd)
		sessions[i].ContextSet = false
		s.applyOverrides(&sessions[i])
	}
}
