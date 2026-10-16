package term

import (
	"bytes"
	"errors"
	"os/exec"
	"strings"
	"testing"
	"time"
)

// startProc runs a shell script in a bare Process (no screen) and closes it
// at the end of the test.
func startProc(t *testing.T, script string) *Process {
	t.Helper()
	p, err := StartProcess("p1", t.TempDir(), 20, 5, "/bin/sh", "-c", script)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(p.Close)
	return p
}

// collect drains a subscription in the background; seen reports whether the
// bytes received so far contain s.
type collector struct {
	ch     chan []byte
	got    bytes.Buffer
	closed bool
}

func newCollector(ch <-chan []byte) *collector {
	c := &collector{ch: make(chan []byte, 1024)}
	go func() {
		for b := range ch {
			c.ch <- b
		}
		close(c.ch)
	}()
	return c
}

// drain moves whatever arrived into got without blocking.
func (c *collector) drain() {
	for {
		select {
		case b, ok := <-c.ch:
			if !ok {
				c.closed = true
				return
			}
			c.got.Write(b)
		default:
			return
		}
	}
}

func (c *collector) waitFor(t *testing.T, s string) {
	t.Helper()
	eventually(t, "subscription bytes "+strummary(s), func() bool {
		c.drain()
		return strings.Contains(c.got.String(), s)
	})
}

func (c *collector) waitClosed(t *testing.T) {
	t.Helper()
	eventually(t, "subscription closed", func() bool { c.drain(); return c.closed })
}

func strummary(s string) string { return strings.ReplaceAll(s, "\x1b", "ESC") }

func waitExited(t *testing.T, p *Process) error {
	t.Helper()
	select {
	case err := <-p.Exited():
		return err
	case <-time.After(5 * time.Second):
		t.Fatal("process did not exit")
		return nil
	}
}

func TestStartProcessFailsForMissingProgram(t *testing.T) {
	if _, err := StartProcess("x", t.TempDir(), 20, 5, "/nonexistent/neko-test-binary"); err == nil {
		t.Fatal("expected an error starting a missing program")
	}
}

func TestProcessID(t *testing.T) {
	if p := startProc(t, "exit 0"); p.ID() != "p1" {
		t.Fatalf("ID %q", p.ID())
	}
}

// Exited delivers the exit error once, then is closed; Done follows it.
func TestProcessExited(t *testing.T) {
	cases := []struct {
		name, script string
		code         int // -1: clean exit
	}{
		{"clean", "exit 0", -1},
		{"code 3", "exit 3", 3},
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			p := startProc(t, c.script)
			err := waitExited(t, p)
			var ee *exec.ExitError
			switch {
			case c.code < 0 && err != nil:
				t.Fatalf("clean exit reported %v", err)
			case c.code >= 0 && (!errors.As(err, &ee) || ee.ExitCode() != c.code):
				t.Fatalf("exit error %v, want code %d", err, c.code)
			}
			if !p.Done() {
				t.Fatal("Done false after Exited")
			}
			if !errors.Is(p.Err(), err) {
				t.Fatalf("Err %v, want %v", p.Err(), err)
			}
			select {
			case e, ok := <-p.Exited():
				if ok {
					t.Fatalf("Exited should be closed after delivering, got %v", e)
				}
			case <-time.After(time.Second):
				t.Fatal("Exited not closed")
			}
		})
	}
}

func TestProcessCloseKills(t *testing.T) {
	p := startProc(t, "sleep 30")
	if p.Done() {
		t.Fatal("Done while running")
	}
	p.Close()
	if err := waitExited(t, p); err == nil {
		t.Fatal("killed program should report an error")
	}
	p.Close() // after exit: no signal, no panic
}

// Subscribers get the raw PTY bytes: OSC sequences included, unfiltered.
func TestProcessSubscribeRawBytes(t *testing.T) {
	p := startProc(t, `stty -echo; printf READY; read x; printf 'abc\033]0;título\007def'; sleep 30`)
	ch, cancel := p.Subscribe()
	defer cancel()
	c := newCollector(ch)
	c.waitFor(t, "READY")
	if _, err := p.Write([]byte("\n")); err != nil {
		t.Fatal(err)
	}
	c.waitFor(t, "abc\x1b]0;título\x07def")
	eventually(t, "title from OSC", func() bool { return p.Title() == "título" })
}

func TestProcessTwoSubscribersAndCancel(t *testing.T) {
	p := startProc(t, `stty -echo; printf READY; read x; printf ONE; read x; printf TWO; sleep 30`)
	ch1, cancel1 := p.Subscribe()
	ch2, cancel2 := p.Subscribe()
	defer cancel2()
	a, b := newCollector(ch1), newCollector(ch2)
	a.waitFor(t, "READY")
	b.waitFor(t, "READY")
	_, _ = p.Write([]byte("\n"))
	a.waitFor(t, "ONE")
	b.waitFor(t, "ONE")

	cancel1()
	cancel1() // idempotent
	a.waitClosed(t)
	_, _ = p.Write([]byte("\n"))
	b.waitFor(t, "TWO")
	if strings.Contains(a.got.String(), "TWO") {
		t.Fatal("cancelled subscriber still received bytes")
	}
}

// When the program's output ends, subscriptions close; late subscribers get
// a closed channel.
func TestProcessSubscriptionsCloseAtEnd(t *testing.T) {
	p := startProc(t, `stty -echo; printf READY; read x`)
	ch, cancel := p.Subscribe()
	c := newCollector(ch)
	c.waitFor(t, "READY")
	_, _ = p.Write([]byte("\n"))
	c.waitClosed(t)
	cancel() // after close: no panic
	late, cancelLate := p.Subscribe()
	defer cancelLate()
	select {
	case _, ok := <-late:
		if ok {
			t.Fatal("late subscription should be closed")
		}
	case <-time.After(time.Second):
		t.Fatal("late subscription not closed")
	}
}

// A subscriber that stops reading is cut instead of stalling the PTY.
func TestProcessSlowSubscriberCut(t *testing.T) {
	p := newProcess("x")
	defer close(p.readDone)
	slow, cancelSlow := p.Subscribe()
	defer cancelSlow()
	fast, cancelFast := p.Subscribe()
	defer cancelFast()
	for i := 0; i < subBuffer+1; i++ {
		p.fanout([]byte{'x'})
		<-fast
	}
	n := 0
	for range slow { // closed after the buffer filled
		n++
	}
	if n != subBuffer {
		t.Fatalf("slow subscriber got %d chunks before the cut, want %d", n, subBuffer)
	}
	p.fanout([]byte{'y'})
	if b := <-fast; string(b) != "y" {
		t.Fatalf("fast subscriber got %q", b)
	}
}

// The fan-out copies: the read loop reuses its buffer.
func TestProcessFanoutCopies(t *testing.T) {
	p := newProcess("x")
	defer close(p.readDone)
	ch, cancel := p.Subscribe()
	defer cancel()
	buf := []byte("abc")
	p.fanout(buf)
	copy(buf, "zzz")
	if got := string(<-ch); got != "abc" {
		t.Fatalf("got %q", got)
	}
}

// Write reaches the program: cat echoes it back.
func TestProcessWrite(t *testing.T) {
	p := startProc(t, `cat`)
	ch, cancel := p.Subscribe()
	defer cancel()
	c := newCollector(ch)
	if _, err := p.Write([]byte("miau\n")); err != nil {
		t.Fatal(err)
	}
	c.waitFor(t, "miau\r\nmiau")
}

func TestProcessResize(t *testing.T) {
	p := startProc(t, `stty -echo; printf READY; read x; stty size; read x; stty size; sleep 30`)
	ch, cancel := p.Subscribe()
	defer cancel()
	c := newCollector(ch)
	c.waitFor(t, "READY")
	p.Resize(30, 8)
	_, _ = p.Write([]byte("\n"))
	c.waitFor(t, "8 30")
	p.Resize(1, 1) // clamped to the minimum
	_, _ = p.Write([]byte("\n"))
	c.waitFor(t, "3 10")
}

func TestProcessMinimumSize(t *testing.T) {
	p, err := StartProcess("x", t.TempDir(), 0, 0, "/bin/sh", "-c", "stty size; sleep 30")
	if err != nil {
		t.Fatal(err)
	}
	defer p.Close()
	ch, cancel := p.Subscribe()
	defer cancel()
	newCollector(ch).waitFor(t, "3 10")
}

// Output fires for a real program's output.
func TestProcessOutputSignal(t *testing.T) {
	p := startProc(t, `printf oi; sleep 30`)
	select {
	case <-p.Output():
	case <-time.After(5 * time.Second):
		t.Fatal("no Output signal")
	}
}

// A burst of output becomes at most two signals (the frame that caught the
// first byte and the one that caught the rest), each a frame apart.
func TestProcessOutputCoalesces(t *testing.T) {
	p := newProcess("x")
	defer close(p.readDone)
	start := time.Now()
	for i := 0; i < 1000; i++ {
		p.markDirty()
	}
	n := 0
	select {
	case <-p.Output():
		n++
	case <-time.After(5 * time.Second):
		t.Fatal("no Output signal after a burst")
	}
	if d := time.Since(start); d < frame*3/4 {
		t.Fatalf("signal after %v, want it coalesced to ~%v", d, frame)
	}
	for quiet := time.After(4 * frame); ; {
		select {
		case <-p.Output():
			n++
			continue
		case <-quiet:
		}
		break
	}
	if n > 2 {
		t.Fatalf("%d signals for one burst, want at most 2", n)
	}
}

// When the output ends, a pending change is still signalled.
func TestProcessOutputFlushOnEnd(t *testing.T) {
	p := newProcess("x")
	p.markDirty()
	close(p.readDone)
	select {
	case <-p.Output():
	case <-time.After(5 * time.Second):
		t.Fatal("pending output not flushed at the end")
	}
}

func TestClampSize(t *testing.T) {
	cases := []struct{ w, h, cw, ch int }{
		{0, 0, 10, 3},
		{9, 2, 10, 3},
		{80, 24, 80, 24},
	}
	for _, c := range cases {
		if w, h := clampSize(c.w, c.h); w != c.cw || h != c.ch {
			t.Errorf("clampSize(%d,%d) = %d,%d", c.w, c.h, w, h)
		}
	}
}
