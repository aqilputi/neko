package core

import (
	"sort"

	"github.com/aqilputi/nekuroddo/internal/store"
)

// SortInbox orders sessions waiting on you: most urgent attention first
// (blocked, returned, idle), then the tree's frozen order. Stable.
func SortInbox(xs []*store.Session, order map[string]int) {
	sort.SliceStable(xs, func(i, j int) bool {
		a, b := xs[i], xs[j]
		if a.Attention != b.Attention {
			return a.Attention < b.Attention
		}
		return order["sess:"+a.SessionID] < order["sess:"+b.SessionID]
	})
}
