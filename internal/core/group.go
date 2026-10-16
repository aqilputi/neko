package core

import "hash/fnv"

// GroupColorIndex is a stable index into the theme's context palette for a
// group tag (fnv-32a, offset so it rarely matches the section color next to
// it). It must stay identical to what ui.groupColor computed, or every
// user's group colors change.
func GroupColorIndex(tag string) int {
	h := fnv.New32a()
	h.Write([]byte(tag))
	return int(h.Sum32()%97) + 3
}
