package core

import "testing"

func TestActivePane(t *testing.T) {
	of := map[string]string{"h": "new1"}
	alive := map[string]bool{"aaaa": true, "new1": true}
	for _, c := range []struct {
		name            string
		cur, sel, short string
		insert, zoom    bool
		want            string
	}{
		{"own short id", "", "a", "aaaa", false, false, "aaaa"},
		{"paneOf wins over the short id", "", "h", "", false, false, "new1"},
		{"no pane: details", "aaaa", "b", "bbbb", false, false, ""},
		{"dead pane: details", "", "d", "dead", false, false, ""},
		{"no selection", "aaaa", "", "", false, false, ""},
		{"insert pins", "aaaa", "b", "bbbb", true, false, "aaaa"},
		{"insert pins even none", "", "a", "aaaa", true, false, ""},
		{"zoom pins a shown pane", "aaaa", "b", "", false, true, "aaaa"},
		{"zoom without a pane follows", "", "a", "aaaa", false, true, "aaaa"},
	} {
		if got := ActivePane(c.cur, c.sel, c.short, of, alive, c.insert, c.zoom); got != c.want {
			t.Errorf("%s: got %q want %q", c.name, got, c.want)
		}
	}
}
