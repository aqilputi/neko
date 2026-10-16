package claude

import (
	"context"
	"os/exec"
	"regexp"
	"strconv"
	"strings"
	"time"
)

// Limit is one line of /usage: "Current session: 12% used · resets Oct 15, 11:30am (…)".
type Limit struct {
	Name   string // "Current session", "Current week (all models)", …
	Pct    int
	Resets string // "Oct 15, 11:30am"
}

// UsageReport is `claude -p /usage`: the subscription limits Claude Code
// itself reports (no tokens spent, no credentials read by nekuroddo).
type UsageReport struct {
	Limits []Limit
	Raw    string
	At     time.Time
}

var limitRe = regexp.MustCompile(`^(.+?):\s+(\d+)% used(?:\s+·\s+resets\s+([^(]+?))?\s*(?:\(|$)`)

func ParseUsage(raw string) UsageReport {
	u := UsageReport{Raw: strings.TrimSpace(raw), At: time.Now()}
	for _, line := range strings.Split(raw, "\n") {
		if m := limitRe.FindStringSubmatch(strings.TrimSpace(line)); m != nil {
			pct, _ := strconv.Atoi(m[2])
			u.Limits = append(u.Limits, Limit{Name: m[1], Pct: pct, Resets: strings.TrimSpace(m[3])})
		}
	}
	return u
}

// FetchUsage runs `claude -p /usage` without persisting a session or firing
// hooks: otherwise every poll leaves a transcript in the cwd's project and
// shows up (via our own `nk hook`) as a short-lived session that notifies.
func FetchUsage() (UsageReport, error) {
	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()
	out, err := exec.CommandContext(ctx, Bin(), "-p", "/usage",
		"--no-session-persistence", "--settings", `{"disableAllHooks":true}`).Output()
	if err != nil {
		return UsageReport{}, err
	}
	return ParseUsage(string(out)), nil
}

// Find returns the first limit whose name contains s.
func (u UsageReport) Find(s string) (Limit, bool) {
	for _, l := range u.Limits {
		if strings.Contains(strings.ToLower(l.Name), s) {
			return l, true
		}
	}
	return Limit{}, false
}

// ShortReset trims "Oct 15, 11:30am" to "11:30am" when it is today.
func (l Limit) ShortReset() string {
	r := l.Resets
	if d, t, ok := strings.Cut(r, ", "); ok && d == time.Now().Format("Jan 2") {
		return t
	}
	return r
}

// ResetTime parses Resets ("Oct 15, 11:30am" or "Oct 21, 10pm"), in local
// time, picking the year that puts it closest to now.
func (l Limit) ResetTime() (time.Time, bool) {
	now := time.Now()
	for _, layout := range []string{"Jan 2, 3:04pm", "Jan 2, 3pm", "Jan 2 3:04pm", "Jan 2 3pm"} {
		t, err := time.ParseInLocation(layout, l.Resets, time.Local)
		if err != nil {
			continue
		}
		best := time.Date(now.Year(), t.Month(), t.Day(), t.Hour(), t.Minute(), 0, 0, time.Local)
		for _, y := range []int{now.Year() - 1, now.Year() + 1} {
			c := time.Date(y, t.Month(), t.Day(), t.Hour(), t.Minute(), 0, 0, time.Local)
			if c.Sub(now).Abs() < best.Sub(now).Abs() {
				best = c
			}
		}
		return best, true
	}
	return time.Time{}, false
}

// WindowStart is when the current 5-hour session window began (reset − 5h),
// or now − 5h when /usage gave no reset time.
func (u UsageReport) WindowStart() time.Time {
	if l, ok := u.Find("session"); ok {
		if r, ok := l.ResetTime(); ok {
			return r.Add(-5 * time.Hour)
		}
	}
	return time.Now().Add(-5 * time.Hour)
}
