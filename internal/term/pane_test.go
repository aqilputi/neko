package term

import (
	"encoding/base64"
	"os"
	"strings"
	"testing"
	"time"

	tea "charm.land/bubbletea/v2"
	uv "github.com/charmbracelet/ultraviolet"
)

// eventually polls cond until it holds or the deadline passes.
func eventually(t *testing.T, what string, cond func() bool) {
	t.Helper()
	for deadline := time.Now().Add(5 * time.Second); time.Now().Before(deadline); time.Sleep(10 * time.Millisecond) {
		if cond() {
			return
		}
	}
	t.Fatalf("timeout waiting for %s", what)
}

// startSh runs a shell script in a pane and closes it at the end of the test.
func startSh(t *testing.T, w, h int, script string) *Pane {
	t.Helper()
	p, err := Start("p1", "pane", t.TempDir(), w, h, "/bin/sh", "-c", script)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(p.Close)
	return p
}

// readFile waits until the file holds exactly n bytes and returns them.
func readFile(t *testing.T, path string, n int) string {
	t.Helper()
	var got []byte
	for deadline := time.Now().Add(5 * time.Second); time.Now().Before(deadline); time.Sleep(10 * time.Millisecond) {
		if got, _ = os.ReadFile(path); len(got) >= n {
			return string(got)
		}
	}
	t.Fatalf("program got %q, want %d bytes", got, n)
	return ""
}

// listen runs the pane's Listen command with a timeout.
func listen(t *testing.T, p *Pane) tea.Msg {
	t.Helper()
	ch := make(chan tea.Msg, 1)
	go func() { ch <- p.Listen()() }()
	select {
	case m := <-ch:
		return m
	case <-time.After(5 * time.Second):
		t.Fatal("Listen did not resolve")
		return nil
	}
}

// waitExit drains Listen until the ExitMsg.
func waitExit(t *testing.T, p *Pane) ExitMsg {
	t.Helper()
	for i := 0; i < 100; i++ {
		switch m := listen(t, p).(type) {
		case ExitMsg:
			return m
		case OutputMsg:
			if m.ID != p.ID {
				t.Fatalf("OutputMsg for %q", m.ID)
			}
		}
	}
	t.Fatal("no ExitMsg")
	return ExitMsg{}
}

// rowText reads row y of a drawn buffer as text (wide-char tails skipped).
func rowText(b uv.ScreenBuffer, y int) string {
	var s strings.Builder
	for x := 0; x < b.Width(); x++ {
		c := b.CellAt(x, y)
		switch {
		case c == nil:
			s.WriteByte(' ')
		case c.Content == "" && c.Width == 0:
		case c.Content == "":
			s.WriteByte(' ')
		default:
			s.WriteString(c.Content)
		}
	}
	return strings.TrimRight(s.String(), " ")
}

func TestStartFailsForMissingProgram(t *testing.T) {
	if _, err := Start("x", "x", t.TempDir(), 20, 5, "/nonexistent/neko-test-binary"); err == nil {
		t.Fatal("expected an error starting a missing program")
	}
}

func TestListenOutputThenExit(t *testing.T) {
	p := startSh(t, 20, 5, "printf oi; read x; exit 0")
	if m, ok := listen(t, p).(OutputMsg); !ok || m.ID != "p1" {
		t.Fatalf("first message %#v, want OutputMsg{p1}", m)
	}
	waitFor(t, p, "oi")
	if p.Done() {
		t.Fatal("Done while the program still runs")
	}
	p.Type("\r")
	m := waitExit(t, p)
	if m.ID != "p1" || m.Err != nil {
		t.Fatalf("exit %#v, want clean exit of p1", m)
	}
	if !p.Done() {
		t.Fatal("Done false after exit")
	}
	p.Close() // after exit: must not signal or panic; Cleanup closes again
}

func TestExitCodeReported(t *testing.T) {
	p := startSh(t, 20, 5, "exit 3")
	if m := waitExit(t, p); m.Err == nil {
		t.Fatal("non-zero exit should carry an error")
	}
}

func TestCloseKillsRunningProgram(t *testing.T) {
	p := startSh(t, 20, 5, "printf go; sleep 30")
	waitFor(t, p, "go")
	p.Close()
	if m := waitExit(t, p); m.Err == nil {
		t.Fatal("killed program should report an error")
	}
	if !p.Done() {
		t.Fatal("Done false after Close")
	}
}

func TestTitleAndClipboard(t *testing.T) {
	b64 := base64.StdEncoding.EncodeToString([]byte("copiado ✓"))
	p := startSh(t, 30, 5, `printf '\033]0;✳ título da tarefa\007\033]52;c;`+b64+`\033\\\033]2;segundo\007ok'; sleep 30`)
	waitFor(t, p, "ok")
	if got := p.WinTitle(); got != "segundo" {
		t.Fatalf("title %q, want the last one set", got)
	}
	if strings.Contains(p.Render(), "tarefa") || strings.Contains(p.Render(), "segundo") {
		t.Fatalf("title leaked onto the screen:\n%s", p.Render())
	}
	if got := p.TakeClipboard(); len(got) != 1 || got[0] != "copiado ✓" {
		t.Fatalf("clipboard %q", got)
	}
	if got := p.TakeClipboard(); got != nil {
		t.Fatalf("TakeClipboard should clear, got %q", got)
	}
}

func TestWantsMouse(t *testing.T) {
	cases := []struct {
		name, seq string
		want      bool
	}{
		{"none", ``, false},
		{"normal", `\033[?1000h`, true},
		{"any-event", `\033[?1003h`, true},
		{"x10", `\033[?9h`, true},
		{"on then off", `\033[?1002h\033[?1002l`, false},
		{"sgr encoding only", `\033[?1006h`, false},
		{"cursor mode is not mouse", `\033[?25l`, false},
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			p := startSh(t, 20, 5, `printf '`+c.seq+`READY'; sleep 30`)
			waitFor(t, p, "READY")
			if got := p.WantsMouse(); got != c.want {
				t.Fatalf("WantsMouse = %v, want %v", got, c.want)
			}
		})
	}
}

func TestCursor(t *testing.T) {
	p := startSh(t, 20, 5, `printf 'abc\r\nde'; read x; printf '\033[?25lH'; sleep 30`)
	waitFor(t, p, "de")
	if x, y, vis := p.Cursor(); x != 2 || y != 1 || !vis {
		t.Fatalf("cursor (%d,%d,%v), want (2,1,true)", x, y, vis)
	}
	p.Type("\r")
	waitFor(t, p, "H")
	eventually(t, "hidden cursor", func() bool { _, _, vis := p.Cursor(); return !vis })
}

func TestResize(t *testing.T) {
	p := startSh(t, 20, 5, `read x; stty size; sleep 30`)
	if p.Height() != 5 {
		t.Fatalf("height %d", p.Height())
	}
	p.Resize(20, 5) // same size: no-op
	p.Resize(30, 8)
	if p.Height() != 8 {
		t.Fatalf("height after resize %d, want 8", p.Height())
	}
	p.Type("\r")
	waitFor(t, p, "8 30") // the program sees the new size
	p.Resize(1, 1)        // clamped to the minimum
	if p.Height() != 3 {
		t.Fatalf("height after tiny resize %d, want 3", p.Height())
	}
}

func TestMinimumSizeOnStart(t *testing.T) {
	p := startSh(t, 1, 1, `stty size; sleep 30`)
	if p.Height() != 3 {
		t.Fatalf("height %d, want 3", p.Height())
	}
	waitFor(t, p, "3 10")
}

func TestBracketedPaste(t *testing.T) {
	out := t.TempDir() + "/got"
	cases := []struct {
		name, mode, want string
	}{
		{"plain", ``, "colado"},
		{"bracketed", `\033[?2004h`, "\x1b[200~colado\x1b[201~"},
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			f := out + c.name
			p := startSh(t, 20, 5, `stty raw -echo; printf '`+c.mode+`READY'; head -c `+itoa2(len(c.want))+` > `+f)
			waitFor(t, p, "READY")
			p.Paste("colado")
			if got := readFile(t, f, len(c.want)); got != c.want {
				t.Fatalf("got %q want %q", got, c.want)
			}
		})
	}
}

func TestTypeSendsRawText(t *testing.T) {
	f := t.TempDir() + "/got"
	want := "olá\x01!"
	p := startSh(t, 20, 5, `stty raw -echo; printf READY; head -c `+itoa2(len(want))+` > `+f)
	waitFor(t, p, "READY")
	p.Type(want)
	if got := readFile(t, f, len(want)); got != want {
		t.Fatalf("got %q", got)
	}
}

func TestSendMouse(t *testing.T) {
	f := t.TempDir() + "/got"
	want := "\x1b[<64;3;4M"
	p := startSh(t, 20, 5, `stty raw -echo; printf '\033[?1000h\033[?1006hREADY'; head -c `+itoa2(len(want))+` > `+f)
	waitFor(t, p, "READY")
	p.SendMouse(uv.MouseWheelEvent{X: 2, Y: 3, Button: uv.MouseWheelUp})
	if got := readFile(t, f, len(want)); got != want {
		t.Fatalf("got %q want %q", got, want)
	}
}

// scrollPane prints a wide-char line then L1..L12 on a 10×5 pane, leaving
// screen L9..L12 + cursor row and scrollback "W日本", L1..L8.
func scrollPane(t *testing.T) *Pane {
	p := startSh(t, 10, 5, `printf 'W日本\n'; i=1; while [ $i -le 12 ]; do echo L$i; i=$((i+1)); done; sleep 30`)
	waitFor(t, p, "L12")
	return p
}

func TestScrollClamps(t *testing.T) {
	p := scrollPane(t)
	p.Scroll(2)
	if p.Scrolled() != 2 {
		t.Fatalf("scrolled %d", p.Scrolled())
	}
	if _, _, vis := p.Cursor(); vis {
		t.Fatal("cursor should hide while scrolled back")
	}
	p.Scroll(-100)
	if p.Scrolled() != 0 {
		t.Fatalf("scrolled %d, want 0 (live)", p.Scrolled())
	}
	p.Scroll(1000)
	if got := p.Scrolled(); got != 9 {
		t.Fatalf("scrolled %d, want clamp at scrollback length 9", got)
	}
	p.SendKey(tea.KeyPressMsg{Code: 'x', Text: "x"})
	if p.Scrolled() != 0 {
		t.Fatal("a key press should jump back to the live view")
	}
}

func TestDraw(t *testing.T) {
	p := scrollPane(t)
	draw := func() uv.ScreenBuffer {
		b := uv.NewScreenBuffer(12, 5) // wider than the pane: extra cells are empty
		p.Draw(b, uv.Rect(0, 0, 12, 5))
		return b
	}
	rows := func(b uv.ScreenBuffer) []string {
		var r []string
		for y := 0; y < 5; y++ {
			r = append(r, rowText(b, y))
		}
		return r
	}
	cases := []struct {
		scroll int
		want   []string
	}{
		{0, []string{"L9", "L10", "L11", "L12", ""}},
		{2, []string{"L7", "L8", "L9", "L10", "L11"}},
		{9, []string{"W日本", "L1", "L2", "L3", "L4"}},
	}
	for _, c := range cases {
		p.Scroll(-1000)
		p.Scroll(c.scroll)
		if got := rows(draw()); strings.Join(got, "|") != strings.Join(c.want, "|") {
			t.Errorf("scroll %d: rows %q, want %q", c.scroll, got, c.want)
		}
	}
	// the wide cell keeps its width when copied from the scrollback
	p.Scroll(-1000)
	p.Scroll(9)
	b := draw()
	if c := b.CellAt(1, 0); c == nil || c.Content != "日" || c.Width != 2 {
		t.Fatalf("wide cell %+v", c)
	}
}

// Drawing into an offset area places the cells there.
func TestDrawOffsetArea(t *testing.T) {
	p := startSh(t, 10, 3, `printf miau; sleep 30`)
	waitFor(t, p, "miau")
	b := uv.NewScreenBuffer(20, 6)
	p.Draw(b, uv.Rect(5, 2, 10, 3))
	if got := rowText(b, 2); got != "     miau" {
		t.Fatalf("row 2 %q", got)
	}
	if got := rowText(b, 0); got != "" {
		t.Fatalf("row 0 should be untouched, got %q", got)
	}
}

func itoa2(n int) string {
	if n < 10 {
		return string(rune('0' + n))
	}
	return itoa2(n/10) + string(rune('0'+n%10))
}
