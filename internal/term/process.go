package term

import (
	"os"
	"os/exec"
	"sync"
	"syscall"
	"time"

	"github.com/creack/pty"
)

// frame is how often, at most, Output signals: bursts of output coalesce
// into one redraw per ~30fps frame.
const frame = 33 * time.Millisecond

// subBuffer is how many chunks a subscriber may lag behind before it is cut.
const subBuffer = 256

// Process is the framework-free half of a pane: a program running in a PTY.
// It fans the raw output bytes out to subscribers (remote clients), signals
// new output (coalesced) for a local emulator, and tracks the window title
// the program sets. It knows nothing about rendering or Bubble Tea.
type Process struct {
	id   string
	cmd  *exec.Cmd
	ptmx *os.File

	sink func([]byte) // local emulator, fed synchronously in the read loop

	dirty    chan struct{} // raw "something changed", cap 1
	out      chan struct{} // coalesced signal, cap 1
	exited   chan error    // exit error once, then closed
	readDone chan struct{} // closed when the read loop ends

	osc oscFilter // title tracking only; its output is discarded

	mu     sync.Mutex // guards done, err, title, subs, closed
	done   bool
	err    error
	title  string
	subs   map[*subscriber]struct{}
	closed bool // read loop ended: no more subscribers
}

type subscriber struct{ ch chan []byte }

// clampSize keeps a PTY usable: at least 10 columns and 3 rows.
func clampSize(cols, rows int) (int, int) { return max(cols, 10), max(rows, 3) }

// StartProcess runs argv in a PTY of size cols×rows inside dir.
func StartProcess(id, dir string, cols, rows int, argv ...string) (*Process, error) {
	return startProcess(id, dir, cols, rows, nil, argv...)
}

// newProcess builds a Process with no program attached and starts its
// output coalescer, which runs until readDone is closed.
func newProcess(id string) *Process {
	p := &Process{
		id:       id,
		dirty:    make(chan struct{}, 1),
		out:      make(chan struct{}, 1),
		exited:   make(chan error, 1),
		readDone: make(chan struct{}),
		subs:     map[*subscriber]struct{}{},
	}
	p.osc.onTitle = func(t string) { p.mu.Lock(); p.title = t; p.mu.Unlock() }
	go p.pump()
	return p
}

// startProcess is StartProcess with a sink that sees every chunk of output
// before Output signals it (the TUI's emulator).
func startProcess(id, dir string, cols, rows int, sink func([]byte), argv ...string) (*Process, error) {
	cols, rows = clampSize(cols, rows)
	cmd := exec.Command(argv[0], argv[1:]...)
	cmd.Dir = dir
	cmd.Env = append(os.Environ(), "TERM=xterm-256color", "COLORTERM=truecolor")
	ptmx, err := pty.StartWithSize(cmd, &pty.Winsize{Cols: uint16(cols), Rows: uint16(rows)})
	if err != nil {
		return nil, err
	}
	p := newProcess(id)
	p.cmd, p.ptmx, p.sink = cmd, ptmx, sink
	go p.read()
	go p.wait()
	return p, nil
}

// read is the program → subscribers/emulator loop.
func (p *Process) read() {
	defer p.endOutput()
	buf := make([]byte, 32*1024)
	for {
		n, err := p.ptmx.Read(buf)
		if n > 0 {
			p.fanout(buf[:n])
			p.osc.filter(buf[:n])
			if p.sink != nil {
				p.sink(buf[:n])
			}
			p.markDirty()
		}
		if err != nil {
			return
		}
	}
}

func (p *Process) wait() {
	err := p.cmd.Wait()
	p.mu.Lock()
	p.done, p.err = true, err
	p.mu.Unlock()
	p.exited <- err
	close(p.exited)
	p.markDirty()
}

// endOutput closes every subscription and stops the coalescer.
func (p *Process) endOutput() {
	p.mu.Lock()
	p.closed = true
	for s := range p.subs {
		close(s.ch)
		delete(p.subs, s)
	}
	p.mu.Unlock()
	close(p.readDone)
}

// fanout hands a copy of b to every subscriber; one that lags subBuffer
// chunks behind is cut (its channel closed) instead of stalling the PTY.
func (p *Process) fanout(b []byte) {
	p.mu.Lock()
	defer p.mu.Unlock()
	if len(p.subs) == 0 {
		return
	}
	c := append([]byte(nil), b...)
	for s := range p.subs {
		select {
		case s.ch <- c:
		default:
			close(s.ch)
			delete(p.subs, s)
		}
	}
}

func (p *Process) markDirty() {
	select {
	case p.dirty <- struct{}{}:
	default:
	}
}

func (p *Process) signal() {
	select {
	case p.out <- struct{}{}:
	default:
	}
}

// pump turns raw changes into at most one Output signal per frame.
func (p *Process) pump() {
	for {
		select {
		case <-p.dirty:
		case <-p.readDone:
			select {
			case <-p.dirty:
				p.signal()
			default:
			}
			return
		}
		time.Sleep(frame)
		p.signal()
	}
}

// ID is the pane id the process was started with.
func (p *Process) ID() string { return p.id }

// Write sends already-encoded input (keys, paste) to the program.
func (p *Process) Write(b []byte) (int, error) { return p.ptmx.Write(b) }

// Resize changes the PTY size (clamped to 10×3); the program gets SIGWINCH.
func (p *Process) Resize(cols, rows int) {
	cols, rows = clampSize(cols, rows)
	_ = pty.Setsize(p.ptmx, &pty.Winsize{Cols: uint16(cols), Rows: uint16(rows)})
}

// Output signals new output, coalesced to one signal per ~33ms frame.
func (p *Process) Output() <-chan struct{} { return p.out }

// Subscribe returns the raw PTY bytes from now on, and a cancel func
// (idempotent). The channel closes on cancel, when the output ends, or when
// the subscriber lags too far behind.
func (p *Process) Subscribe() (<-chan []byte, func()) {
	s := &subscriber{ch: make(chan []byte, subBuffer)}
	p.mu.Lock()
	defer p.mu.Unlock()
	if p.closed {
		close(s.ch)
		return s.ch, func() {}
	}
	p.subs[s] = struct{}{}
	return s.ch, func() {
		p.mu.Lock()
		defer p.mu.Unlock()
		if _, ok := p.subs[s]; ok {
			close(s.ch)
			delete(p.subs, s)
		}
	}
}

// Exited delivers the program's exit error (nil for a clean exit) once and
// is then closed; later receives get nil at once. Use Err for the error
// after the fact.
func (p *Process) Exited() <-chan error { return p.exited }

// Done reports whether the program exited.
func (p *Process) Done() bool {
	p.mu.Lock()
	defer p.mu.Unlock()
	return p.done
}

// Err is the program's exit error, once Done.
func (p *Process) Err() error {
	p.mu.Lock()
	defer p.mu.Unlock()
	return p.err
}

// Title is the window title the program last set (OSC 0/2; Claude Code:
// "✳ <task>").
func (p *Process) Title() string {
	p.mu.Lock()
	defer p.mu.Unlock()
	return p.title
}

// Close detaches: kills the program (our `claude attach` client; a
// background session keeps running in the Claude Code daemon).
//
// Never SIGINT: an attach client may relay it to the session as Ctrl+C and
// interrupt the agent. SIGHUP is what closing a terminal window sends.
func (p *Process) Close() {
	if p.cmd.Process != nil && !p.Done() {
		_ = p.cmd.Process.Signal(syscall.SIGHUP)
		_ = p.cmd.Process.Kill()
	}
	_ = p.ptmx.Close()
}
