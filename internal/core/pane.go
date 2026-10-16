package core

// ActivePane picks the pane the right side shows (it follows the selection):
// the selected session's pane when one is alive, otherwise none (details).
// cur is the pane shown now: INSERT keeps it whatever the selection, and so
// does zoom while a pane is shown. paneOf maps a session to its pane id
// (history sessions get a new short id); without an entry the pane id is the
// session's own short id. alive holds the pane ids still running. selID and
// selShortID are "" when no session is selected.
func ActivePane(cur, selID, selShortID string, paneOf map[string]string, alive map[string]bool, insert, zoom bool) string {
	if insert || (zoom && cur != "") {
		return cur
	}
	if selID == "" {
		return ""
	}
	id := paneOf[selID]
	if id == "" {
		id = selShortID
	}
	if id != "" && alive[id] {
		return id
	}
	return ""
}
