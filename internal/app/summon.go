package app

import (
	"sort"

	"github.com/aqilputi/nekuroddo/internal/core"
	"github.com/aqilputi/nekuroddo/internal/store"
)

// EvInboxSummoned says which session inbox.summon picked, so every client
// opens it: the GUI's inbox cat, and the one that ran the command remotely
// (a compositor shortcut running `nk run inbox.summon`).
const EvInboxSummoned = "inbox.summoned"

// InboxSummoned is inbox.summon's Data and the event's payload.
type InboxSummoned struct {
	SessionID string `json:"session_id"`
	Label     string `json:"label"`
}

// InboxSummon picks the session that waits on the user the most: blocked
// before returned before idle (core.SortInbox), then the most recent
// activity. With nobody waiting it only says so.
func (a *App) InboxSummon() Result {
	snap := a.Snapshot()
	var waiting []*store.Session
	for i := range snap.Sessions {
		if s := &snap.Sessions[i]; s.Attention != store.AttnNone && !s.Archived {
			waiting = append(waiting, s)
		}
	}
	if len(waiting) == 0 {
		return Info("ninguém te esperando ✨")
	}
	// most recent first, then the attention order (stable): the activity
	// breaks ties within the same attention
	order := map[string]int{}
	byRecent := append([]*store.Session(nil), waiting...)
	sort.SliceStable(byRecent, func(i, j int) bool { return byRecent[i].LastAt.After(byRecent[j].LastAt) })
	for rank, s := range byRecent {
		order["sess:"+s.SessionID] = rank
	}
	core.SortInbox(waiting, order)
	s := waiting[0]
	pick := InboxSummoned{SessionID: s.SessionID, Label: s.Label()}
	a.Publish(EvInboxSummoned, pick)
	return Result{Toast: "→ " + core.Truncate(pick.Label, 50), Data: pick}
}
