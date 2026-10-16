package core

import (
	"hash/fnv"
	"strings"
)

// GroupColorIndex is a stable index into the theme's context palette for a
// group tag (fnv-32a, offset so it rarely matches the section color next to
// it). It must stay identical to what ui.groupColor computed, or every
// user's group colors change.
func GroupColorIndex(tag string) int {
	h := fnv.New32a()
	h.Write([]byte(tag))
	return int(h.Sum32()%97) + 3
}

// CleanTag normalizes a group tag: "#Deploy RC" → "deploy-rc". It must stay
// identical to groups.CleanTag (the hook side keeps its own copy).
func CleanTag(tag string) string {
	tag = strings.ToLower(strings.TrimSpace(strings.TrimPrefix(strings.TrimSpace(tag), "#")))
	return strings.Join(strings.Fields(tag), "-")
}
