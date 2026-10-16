package demo

import (
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/aqilputi/nekuroddo/internal/app"
)

// collector drains a subscription into a string.
type collector struct {
	mu  sync.Mutex
	buf strings.Builder
	end chan struct{}
}

func collect(ch <-chan []byte) *collector {
	c := &collector{end: make(chan struct{})}
	go func() {
		defer close(c.end)
		for b := range ch {
			c.mu.Lock()
			c.buf.Write(b)
			c.mu.Unlock()
		}
	}()
	return c
}

func (c *collector) String() string {
	c.mu.Lock()
	defer c.mu.Unlock()
	return c.buf.String()
}

func (c *collector) waitFor(t *testing.T, what string) {
	t.Helper()
	deadline := time.Now().Add(5 * time.Second)
	for !strings.Contains(c.String(), what) {
		if time.Now().After(deadline) {
			t.Fatalf("timed out waiting for %q in %q", what, c.String())
		}
		time.Sleep(5 * time.Millisecond)
	}
}

func startPane(t *testing.T, s *Scenario, id string) app.Pane {
	t.Helper()
	p, err := s.Ports().Panes.Start(id, "título", "/demo/nao-existe", 100, 30, "claude", "attach", id)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(p.Close)
	return p
}

func TestPaneDrawsTheSessionByState(t *testing.T) {
	s, clk := newScenario(t)
	clk.Advance(StateEvery) // gateway blocked
	for _, tc := range []struct{ id, want string }{
		{"dmgatewy", "Aplico a migração 0042 no staging?"},
		{"dmmigrat", "remover agora"},
		{"dmreport", "Pronto: relatório"},
		{"dmindexr", "2 subagents"},
		{"unknown1", "sessão de demonstração"},
	} {
		p := startPane(t, s, tc.id)
		out, cancel := p.Subscribe()
		c := collect(out)
		c.waitFor(t, tc.want)
		c.waitFor(t, "> ")
		if !strings.Contains(c.String(), "demo") || strings.Contains(c.String(), "\n\n") {
			t.Errorf("%s: screen %q", tc.id, c.String())
		}
		cancel()
		<-c.end
		if p.ID() != tc.id || p.Title() != "título" {
			t.Errorf("id/title = %q/%q", p.ID(), p.Title())
		}
	}
}

func TestPaneLineDiscipline(t *testing.T) {
	s, _ := newScenario(t)
	p := startPane(t, s, "dmdocsit")
	out, cancel := p.Subscribe()
	defer cancel()
	c := collect(out)
	c.waitFor(t, "> ")
	select {
	case <-p.Output():
	case <-time.After(5 * time.Second):
		t.Fatal("no output signal")
	}
	// typing echoes; backspace erases; arrows and other escapes are dropped
	for _, chunk := range []string{"ol", "x\x7f", "\x1b[A", "\x1bOB", "a\x01", "\r"} {
		if _, err := p.Write([]byte(chunk)); err != nil {
			t.Fatal(err)
		}
	}
	c.waitFor(t, "recebi: ola")
	if !strings.Contains(c.String(), "olx\b \ba\r\n") {
		t.Errorf("echo = %q", c.String())
	}
	// ctrl+c drops the line; a backspace on an empty line does nothing
	_, _ = p.Write([]byte("zz\x03\x7f\x1bx" + "ç\x7f!\n"))
	c.waitFor(t, "recebi: !")
	if strings.Contains(c.String(), "recebi: zz") || !strings.Contains(c.String(), "^C\r\n") {
		t.Errorf("after ctrl+c = %q", c.String())
	}
	p.Resize(120, 40)
	if p.Done() {
		t.Error("done while running")
	}
}

func TestPaneReplayAndExit(t *testing.T) {
	s, _ := newScenario(t)
	p := startPane(t, s, "dmreport")
	first, cancel1 := p.Subscribe()
	c1 := collect(first)
	c1.waitFor(t, "> ")
	// a viewer that arrives late still sees the screen
	late, cancel2 := p.Subscribe()
	c2 := collect(late)
	c2.waitFor(t, "Pronto: relatório")
	cancel2()
	cancel2() // twice is fine
	<-c2.end
	p.Close()
	select {
	case err := <-p.Exited():
		_ = err // killed by a signal: any status is fine
	case <-time.After(5 * time.Second):
		t.Fatal("pane never exited")
	}
	if _, ok := <-p.Exited(); ok {
		t.Error("Exited not closed after the first value")
	}
	<-c1.end // the subscriber was closed
	cancel1()
	if !p.Done() {
		t.Error("not done after exit")
	}
	if _, err := p.Write([]byte("x")); err == nil {
		t.Error("write after exit")
	}
	ch, cancel := p.Subscribe()
	if _, ok := <-ch; ok {
		t.Error("subscribe after exit gave bytes")
	}
	cancel()
	p.Close() // twice is fine
}

func TestPaneScriptEnds(t *testing.T) {
	s, _ := newScenario(t)
	p := startPane(t, s, "dmdocsit")
	out, cancel := p.Subscribe()
	defer cancel()
	c := collect(out)
	c.waitFor(t, "> ")
	_, _ = p.Write([]byte("/exit\r"))
	select {
	case err := <-p.Exited():
		if err != nil {
			t.Errorf("exit = %v", err)
		}
	case <-time.After(5 * time.Second):
		t.Fatal("/exit did not end the script")
	}
	<-c.end
	if !strings.Contains(c.String(), "até mais") {
		t.Errorf("bye = %q", c.String())
	}
}

func TestPaneStartFails(t *testing.T) {
	s, _ := newScenario(t)
	sp := Panes{s: s, shell: "/nonexistent/sh"}
	if p, err := sp.Start("x", "t", "", 80, 24); err == nil || p != nil {
		t.Errorf("start with a missing shell = %v, %v", p, err)
	}
}
