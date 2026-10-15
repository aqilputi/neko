package term

import (
	"encoding/base64"
	"io"
	"strings"
	"testing"
	"time"

	uv "github.com/charmbracelet/ultraviolet"
	"github.com/charmbracelet/x/vt"
)

// x/vt prints the rest of an OSC after a non-ASCII byte; the filter keeps
// Claude's "✳ title" out of the screen, even split across reads.
func TestOSCFilter(t *testing.T) {
	var title, clip string
	f := oscFilter{onTitle: func(s string) { title = s }, onClipboard: func(s string) { clip = s }}
	in := "abc\x1b]0;✳ resumo da tarefa\x07def\x1b]52;c;" + base64.StdEncoding.EncodeToString([]byte("copiado!")) + "\x1b\\gh\x1b]8;;http://x\x1b\\link\x1b[31mred"
	var out []byte
	for i := 0; i < len(in); i += 3 { // feed in tiny chunks
		out = append(out, f.filter([]byte(in[i:min(i+3, len(in))]))...)
	}
	e := vt.NewEmulator(60, 2)
	e.Write(out)
	if got := strings.TrimRight(strings.Split(e.String(), "\n")[0], " "); got != "abcdefghlinkred" {
		t.Fatalf("screen %q", got)
	}
	if title != "✳ resumo da tarefa" || clip != "copiado!" {
		t.Fatalf("title %q clip %q", title, clip)
	}
	if !strings.Contains(string(out), "\x1b]8;;http://x\x1b\\") || !strings.Contains(string(out), "\x1b[31m") {
		t.Fatalf("ascii OSC / CSI not passed through: %q", out)
	}
}

// With SGR mouse on (as Claude fullscreen sets it), a wheel event reaches
// the program as CSI < 64;x;y M.
func TestWheelEncoding(t *testing.T) {
	e := vt.NewSafeEmulator(20, 5)
	e.Write([]byte("\x1b[?1000h\x1b[?1006h"))
	got := make(chan string, 1)
	go func() {
		b := make([]byte, 64)
		n, _ := e.Read(b)
		got <- string(b[:n])
	}()
	e.SendMouse(uv.MouseWheelEvent{X: 2, Y: 3, Button: uv.MouseWheelUp})
	select {
	case s := <-got:
		if s != "\x1b[<64;3;4M" {
			t.Fatalf("wheel encoded as %q", s)
		}
	case <-time.After(time.Second):
		t.Fatal("nothing sent")
	}
	_ = io.EOF
}
