package demo

import (
	_ "embed"
	"errors"
	"fmt"
	"io"
	"os"
	"os/exec"
	"strings"
	"sync"
	"unicode/utf8"

	"github.com/aqilputi/nekuroddo/internal/app"
	"github.com/aqilputi/nekuroddo/internal/store"
)

//go:embed screen.sh
var screenScript string

// ErrPaneDone is what Write answers once the script ended.
var ErrPaneDone = errors.New("demo: o painel terminou")

const (
	historyMax = 64 << 10 // bytes replayed to a viewer that subscribes late
	subBuffer  = 256      // chunks a subscriber may lag behind (then chunks drop)
)

// Panes is the demo's PTYSpawner: whatever argv the core asks for (always a
// `claude attach`), it runs screen.sh under sh with pipes — no PTY, no
// claude — drawing the session's scripted screen. A small line discipline
// in Write echoes what is typed and hands whole lines to the script, as a
// terminal in cooked mode would.
type Panes struct {
	s     *Scenario
	shell string // "sh" unless a test says otherwise
}

// Start runs the screen of the session whose short (or full) id is id.
func (ps Panes) Start(id, title, _ string, cols, rows int, _ ...string) (app.Pane, error) {
	ps.s.mu.Lock()
	x, ok := ps.s.byShortLocked(id)
	if !ok {
		x = ps.s.lookupLocked(id)
		ok = x.SessionID != ""
	}
	ps.s.mu.Unlock()
	shell := ps.shell
	if shell == "" {
		shell = "sh"
	}
	cmd := exec.Command(shell, "-c", screenScript, "nk-demo")
	cmd.Dir = os.TempDir()
	cmd.Env = []string{"PATH=/usr/bin:/bin", "LANG=C.UTF-8", "NK_DEMO_TITLE=claude · " + x.Label(),
		"NK_DEMO_BODY=" + ScreenBody(x, ok)}
	stdin, err := cmd.StdinPipe()
	if err != nil {
		return nil, err
	}
	r, w, err := os.Pipe()
	if err != nil {
		return nil, err
	}
	cmd.Stdout, cmd.Stderr = w, w
	if err := cmd.Start(); err != nil {
		_ = r.Close()
		_ = w.Close()
		return nil, fmt.Errorf("demo: %w", err)
	}
	_ = w.Close()
	p := &shellPane{id: id, title: title, cols: cols, rows: rows, cmd: cmd, stdin: stdin,
		subs: map[int]chan []byte{}, out: make(chan struct{}, 1), exited: make(chan error, 1)}
	go p.read(r)
	return p, nil
}

// ScreenBody is what the fake Claude screen shows for a session (found:
// whether the id named a session of the scenario).
func ScreenBody(x store.Session, found bool) string {
	const (
		or   = "\033[38;5;216m"
		bold = "\033[1m"
		dim  = "\033[2m"
		off  = "\033[0m"
	)
	var b strings.Builder
	line := func(f string, a ...any) { fmt.Fprintf(&b, f+"\n", a...) }
	line("%s✻%s %sClaude Code%s %s· demo do nekuroddo (nada aqui é real)%s", or, off, bold, off, dim, off)
	if !found {
		line("")
		line("⏺ sessão de demonstração — digite algo e veja o eco.")
	} else {
		line("%s  %s · %s · %s%s", dim, x.Label(), x.Context, x.State, off)
		line("")
		switch {
		case len(x.Questions) > 0:
			if x.LastText != "" {
				line("⏺ %s", x.LastText)
			}
			n := 0
			for _, q := range x.Questions {
				line("  %s%s%s", bold, q.Question, off)
				for _, o := range q.Options {
					n++
					mark := "  "
					if n == 1 {
						mark = or + "❯ " + off
					}
					line("  %s%d. %s %s— %s%s", mark, n, o.Label, dim, o.Description, off)
				}
			}
		case x.State == "busy":
			line("⏺ %s", x.Detail)
			for _, s := range x.Subs {
				if !s.Done {
					line("  ⎿  %s(%s)", s.Type, s.Description)
				}
			}
			line("%s✻ Pensando… (esc para interromper)%s", or, off)
		case x.LastText != "":
			line("⏺ %s", x.LastText)
		default:
			line("⏺ Em que posso ajudar?")
		}
	}
	line("")
	b.WriteString(dim + "  (demo) digite e tecle Enter; /exit encerra" + off)
	return b.String()
}

// shellPane is one running screen.sh.
type shellPane struct {
	id, title string
	cmd       *exec.Cmd
	stdin     io.WriteCloser

	mu         sync.Mutex
	cols, rows int
	line       []byte // the line being typed (cooked mode)
	esc        int    // 0 text, 1 after ESC, 2 inside a CSI/SS3 sequence
	hist       []byte
	subs       map[int]chan []byte
	next       int
	done       bool

	out    chan struct{}
	exited chan error
}

func (p *shellPane) ID() string    { return p.id }
func (p *shellPane) Title() string { return p.title }

func (p *shellPane) Resize(cols, rows int) {
	p.mu.Lock()
	p.cols, p.rows = cols, rows
	p.mu.Unlock()
}

func (p *shellPane) Output() <-chan struct{} { return p.out }
func (p *shellPane) Exited() <-chan error    { return p.exited }

func (p *shellPane) Done() bool {
	p.mu.Lock()
	defer p.mu.Unlock()
	return p.done
}

// Subscribe replays the screen so far, then follows the output.
func (p *shellPane) Subscribe() (<-chan []byte, func()) {
	p.mu.Lock()
	defer p.mu.Unlock()
	ch := make(chan []byte, subBuffer)
	if p.done {
		close(ch)
		return ch, func() {}
	}
	if len(p.hist) > 0 {
		ch <- append([]byte(nil), p.hist...)
	}
	id := p.next
	p.next++
	p.subs[id] = ch
	return ch, func() {
		p.mu.Lock()
		defer p.mu.Unlock()
		if c, ok := p.subs[id]; ok {
			delete(p.subs, id)
			close(c)
		}
	}
}

// emit fans b out to the subscribers and signals Output.
func (p *shellPane) emit(b []byte) {
	if len(b) == 0 {
		return
	}
	p.mu.Lock()
	p.hist = append(p.hist, b...)
	if over := len(p.hist) - historyMax; over > 0 {
		p.hist = append([]byte(nil), p.hist[over:]...)
	}
	for _, ch := range p.subs {
		select {
		case ch <- append([]byte(nil), b...):
		default:
		}
	}
	p.mu.Unlock()
	select {
	case p.out <- struct{}{}:
	default:
	}
}

func (p *shellPane) read(r *os.File) {
	defer r.Close()
	buf := make([]byte, 4096)
	for {
		n, err := r.Read(buf)
		p.emit(buf[:n])
		if err != nil {
			break
		}
	}
	err := p.cmd.Wait()
	p.mu.Lock()
	p.done = true
	for id, ch := range p.subs {
		delete(p.subs, id)
		close(ch)
	}
	p.mu.Unlock()
	p.exited <- err
	close(p.exited)
}

// Write is the keyboard: printable bytes echo and build the line, backspace
// erases a rune, Enter sends the line, ctrl+c drops it, escape sequences
// (arrows, alt+key) are ignored.
func (p *shellPane) Write(b []byte) (int, error) {
	p.mu.Lock()
	if p.done {
		p.mu.Unlock()
		return 0, ErrPaneDone
	}
	var echo, send []byte
	for _, c := range b {
		switch {
		case p.esc == 1:
			p.esc = 0
			if c == '[' || c == 'O' {
				p.esc = 2
			}
		case p.esc == 2:
			if c >= 0x40 && c <= 0x7e {
				p.esc = 0
			}
		case c == 0x1b:
			p.esc = 1
		case c == '\r' || c == '\n':
			echo = append(echo, "\r\n"...)
			send = append(append(send, p.line...), '\n')
			p.line = p.line[:0]
		case c == 0x7f || c == 0x08:
			if len(p.line) > 0 {
				_, size := utf8.DecodeLastRune(p.line)
				p.line = p.line[:len(p.line)-size]
				echo = append(echo, "\b \b"...)
			}
		case c == 0x03:
			echo = append(echo, "^C\r\n"...)
			send = append(send, '\n')
			p.line = p.line[:0]
		case c < 0x20:
		default:
			p.line = append(p.line, c)
			echo = append(echo, c)
		}
	}
	p.mu.Unlock()
	p.emit(echo)
	if len(send) > 0 {
		if _, err := p.stdin.Write(send); err != nil {
			return 0, err
		}
	}
	return len(b), nil
}

// Close ends the script (its subscribers close once it exits).
func (p *shellPane) Close() {
	_ = p.stdin.Close()
	_ = p.cmd.Process.Kill()
}
