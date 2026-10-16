package claude

import (
	"errors"
	"os/exec"
	"reflect"
	"testing"
	"time"
)

func TestParseUsageLines(t *testing.T) {
	cases := []struct {
		name string
		in   string
		want []Limit
	}{
		{"empty", "", nil},
		{"no reset", "Current session: 0% used", []Limit{{Name: "Current session", Pct: 0}}},
		{"no zone", "  Current week (all models): 100% used · resets Oct 21, 10pm  ",
			[]Limit{{Name: "Current week (all models)", Pct: 100, Resets: "Oct 21, 10pm"}}},
		{"noise ignored", "Last 24h · 2876 requests\nsession: lots used\n97% of your usage", nil},
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			u := ParseUsage(c.in)
			if !reflect.DeepEqual(u.Limits, c.want) {
				t.Fatalf("limits = %+v, want %+v", u.Limits, c.want)
			}
			if time.Since(u.At) > time.Minute {
				t.Fatalf("At = %v", u.At)
			}
		})
	}
	if u := ParseUsage("  x\n"); u.Raw != "x" {
		t.Fatalf("Raw = %q", u.Raw)
	}
}

func TestFind(t *testing.T) {
	u := UsageReport{Limits: []Limit{{Name: "Current session", Pct: 1}, {Name: "Current week (Fable)", Pct: 2}}}
	if l, ok := u.Find("week"); !ok || l.Pct != 2 {
		t.Fatalf("Find(week) = %+v %v", l, ok)
	}
	if _, ok := u.Find("opus"); ok {
		t.Fatal("Find(opus) should miss")
	}
}

func TestShortReset(t *testing.T) {
	today := time.Now().Format("Jan 2")
	cases := []struct{ in, want string }{
		{today + ", 11:30am", "11:30am"},
		{"Feb 30, 1pm", "Feb 30, 1pm"}, // never today
		{"", ""},
		{"11:30am", "11:30am"},
	}
	for _, c := range cases {
		if got := (Limit{Resets: c.in}).ShortReset(); got != c.want {
			t.Errorf("ShortReset(%q) = %q, want %q", c.in, got, c.want)
		}
	}
}

func TestResetTimeYearAndLayouts(t *testing.T) {
	now := time.Now()
	soon := now.Add(48 * time.Hour)
	r, ok := Limit{Resets: soon.Format("Jan 2, 3:04pm")}.ResetTime()
	if !ok || r.Sub(soon).Abs() > time.Minute {
		t.Fatalf("near reset → %v %v, want ≈ %v", r, ok, soon)
	}
	// Without the comma too.
	if r, ok := (Limit{Resets: soon.Format("Jan 2 3pm")}).ResetTime(); !ok || r.Hour() != soon.Hour() || r.Minute() != 0 {
		t.Fatalf("no-comma layout → %v %v", r, ok)
	}
	// The year is the one that puts the date closest to now.
	for _, d := range []time.Duration{-100 * 24 * time.Hour, 100 * 24 * time.Hour} {
		want := now.Add(d)
		r, ok := Limit{Resets: want.Format("Jan 2, 3pm")}.ResetTime()
		if !ok || r.Year() != want.Year() || r.Month() != want.Month() || r.Day() != want.Day() {
			t.Errorf("%v → %v", want, r)
		}
	}
	for _, bad := range []string{"", "tomorrow", "11:30am"} {
		if _, ok := (Limit{Resets: bad}).ResetTime(); ok {
			t.Errorf("ResetTime(%q) should fail", bad)
		}
	}
}

func TestWindowStart(t *testing.T) {
	reset := time.Now().Add(2 * time.Hour).Truncate(time.Minute)
	u := UsageReport{Limits: []Limit{{Name: "Current session", Resets: reset.Format("Jan 2, 3:04pm")}}}
	if got := u.WindowStart(); !got.Equal(reset.Add(-5 * time.Hour)) {
		t.Fatalf("WindowStart = %v, want %v", got, reset.Add(-5*time.Hour))
	}
	for _, u := range []UsageReport{{}, {Limits: []Limit{{Name: "Current session", Resets: "soon"}}}} {
		if d := time.Since(u.WindowStart()) - 5*time.Hour; d < 0 || d > time.Minute {
			t.Errorf("fallback WindowStart off by %v", d)
		}
	}
}

func TestFetchUsage(t *testing.T) {
	argv := fakeClaude(t, "Current session: 33% used · resets Oct 15, 11:30am (UTC)\n", 0)
	u, err := FetchUsage()
	if err != nil {
		t.Fatal(err)
	}
	if got := readArgv(t, argv); !reflect.DeepEqual(got, []string{"-p", "/usage"}) {
		t.Fatalf("argv = %q", got)
	}
	if l, ok := u.Find("session"); !ok || l.Pct != 33 || l.Resets != "Oct 15, 11:30am" {
		t.Fatalf("limits = %+v", u.Limits)
	}

	fakeClaude(t, "", 1)
	var ee *exec.ExitError
	if _, err := FetchUsage(); !errors.As(err, &ee) {
		t.Fatalf("err = %v", err)
	}
}
