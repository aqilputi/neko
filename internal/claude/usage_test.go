package claude

import "testing"

func TestParseUsage(t *testing.T) {
	u := ParseUsage(`You are currently using your subscription to power your Claude Code usage

Current session: 12% used · resets Oct 15, 11:30am (America/Sao_Paulo)
Current week (all models): 12% used · resets Oct 21, 10pm (America/Sao_Paulo)
Current week (Fable): 8% used · resets Oct 21, 10pm (America/Sao_Paulo)

Last 24h · 2876 requests · 20 sessions
  97% of your usage came from subagent-heavy sessions`)
	if len(u.Limits) != 3 {
		t.Fatalf("limits: %+v", u.Limits)
	}
	s, ok := u.Find("session")
	if !ok || s.Pct != 12 || s.Resets != "Oct 15, 11:30am" {
		t.Fatalf("session: %+v", s)
	}
	if f, _ := u.Find("fable"); f.Pct != 8 {
		t.Fatalf("fable: %+v", f)
	}
}

func TestResetTime(t *testing.T) {
	for _, s := range []string{"Oct 15, 11:30am", "Oct 21, 10pm"} {
		r, ok := Limit{Resets: s}.ResetTime()
		if !ok || r.Year() < 2000 {
			t.Fatalf("%q → %v %v", s, r, ok)
		}
		if got := r.Format("Jan 2, 3:04pm"); got != map[string]string{"Oct 15, 11:30am": "Oct 15, 11:30am", "Oct 21, 10pm": "Oct 21, 10:00pm"}[s] {
			t.Fatalf("%q → %q", s, got)
		}
	}
}
