// vtprobe drives a program inside neko's real embedded pane (term.Pane):
// it types prompts, scrolls with the wheel, drags to select, and dumps the
// screen, title, cursor and clipboard after each step. A debugging tool for
// the embedded claude.
//
//	vtprobe -out /tmp/probe -- claude --model haiku
package main

import (
	"flag"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"time"

	uv "github.com/charmbracelet/ultraviolet"
	"github.com/charmbracelet/x/ansi"

	"github.com/aqilputi/neko/internal/term"
)

func main() {
	out := flag.String("out", "/tmp/vtprobe", "output dir")
	w := flag.Int("w", 100, "cols")
	h := flag.Int("h", 30, "rows")
	prompts := flag.String("prompts", "responda apenas: oi1|liste 40 frutas, uma por linha|responda apenas: oi3", "prompts separated by |")
	flag.Parse()
	_ = os.MkdirAll(*out, 0o755)

	home, _ := os.UserHomeDir()
	p, err := term.Start("probe", "probe", home, *w, *h, flag.Args()...)
	if err != nil {
		panic(err)
	}
	defer p.Close()
	screen := func() string { return ansi.Strip(p.Render()) }
	idle := func(d, max time.Duration) {
		start, last, prev := time.Now(), time.Now(), ""
		for time.Since(start) < max {
			time.Sleep(250 * time.Millisecond)
			if s := screen(); s != prev {
				prev, last = s, time.Now()
			} else if time.Since(last) > d {
				return
			}
		}
	}
	report := func(name string) {
		x, y, vis := p.Cursor()
		info := fmt.Sprintf("title=%q cursor=(%d,%d) visible=%v wantsMouse=%v clip=%q\n",
			p.WinTitle(), x, y, vis, p.WantsMouse(), p.TakeClipboard())
		_ = os.WriteFile(filepath.Join(*out, name), []byte(info+screen()), 0o644)
		fmt.Print(name, ": ", info)
	}

	idle(3*time.Second, 30*time.Second)
	if strings.Contains(screen(), "trust this folder") {
		p.Type("\x1b[B")
		time.Sleep(300 * time.Millisecond)
		p.Type("\r")
		idle(3*time.Second, 30*time.Second)
	}
	for i, q := range strings.Split(*prompts, "|") {
		p.Type(q)
		time.Sleep(300 * time.Millisecond)
		p.Type("\r")
		idle(6*time.Second, 120*time.Second)
		report(fmt.Sprintf("%d-prompt.txt", i+1))
	}

	for i := 0; i < 5; i++ {
		p.SendMouse(uv.MouseWheelEvent{X: 10, Y: 10, Button: uv.MouseWheelUp})
		time.Sleep(100 * time.Millisecond)
	}
	idle(1*time.Second, 5*time.Second)
	report("wheel-up.txt")

	// drag across a line to select it
	y := 5
	p.SendMouse(uv.MouseClickEvent{X: 2, Y: y, Button: uv.MouseLeft})
	for x := 3; x < 40; x += 4 {
		p.SendMouse(uv.MouseMotionEvent{X: x, Y: y, Button: uv.MouseLeft})
		time.Sleep(30 * time.Millisecond)
	}
	p.SendMouse(uv.MouseReleaseEvent{X: 40, Y: y, Button: uv.MouseLeft})
	idle(1*time.Second, 5*time.Second)
	report("drag.txt")
}
