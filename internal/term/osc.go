package term

import (
	"bytes"
	"encoding/base64"
)

// oscFilter pulls OSC sequences (ESC ] … BEL or ESC ] … ESC \) out of the
// program's output before it reaches x/vt.
//
// x/vt v0.1.0 aborts an OSC at the first non-ASCII byte and prints the rest
// as text, so Claude Code's window title ("✳ <task summary>") showed up as
// ghost text in the prompt box. Titles and clipboard writes (OSC 52) are
// handled by neko; other OSCs pass through when they are plain ASCII.
// It keeps state between writes: a sequence may arrive split across reads.
type oscFilter struct {
	state int // 0 text, 1 after ESC, 2 inside OSC, 3 inside OSC after ESC
	buf   []byte

	onTitle     func(string)
	onClipboard func(string)
}

const maxOSC = 1 << 20

func (f *oscFilter) filter(in []byte) []byte {
	out := make([]byte, 0, len(in))
	for _, b := range in {
		switch f.state {
		case 0:
			if b == 0x1b {
				f.state = 1
				continue
			}
			out = append(out, b)
		case 1:
			if b == ']' {
				f.state, f.buf = 2, f.buf[:0]
				continue
			}
			f.state = 0
			out = append(out, 0x1b, b)
		case 2:
			switch b {
			case 0x07:
				out = f.finish(out)
			case 0x1b:
				f.state = 3
			default:
				if len(f.buf) < maxOSC {
					f.buf = append(f.buf, b)
				}
			}
		case 3:
			if b == '\\' {
				out = f.finish(out)
				continue
			}
			// ESC inside an OSC that is not ST: drop the OSC, restart on this ESC
			f.state = 1
			if b != 0x1b {
				f.state = 0
				out = append(out, 0x1b, b)
			}
		}
	}
	return out
}

func (f *oscFilter) finish(out []byte) []byte {
	f.state = 0
	cmd, data, _ := bytes.Cut(f.buf, []byte{';'})
	switch string(cmd) {
	case "0", "2":
		if f.onTitle != nil {
			f.onTitle(string(data))
		}
		return out
	case "1": // icon name
		return out
	case "52": // clipboard: 52;<targets>;<base64>
		_, payload, _ := bytes.Cut(data, []byte{';'})
		if txt, err := base64.StdEncoding.DecodeString(string(payload)); err == nil && f.onClipboard != nil {
			f.onClipboard(string(txt))
		}
		return out
	}
	for _, c := range f.buf {
		if c >= 0x80 {
			return out // x/vt would print it: drop
		}
	}
	out = append(out, 0x1b, ']')
	out = append(out, f.buf...)
	return append(out, 0x1b, '\\')
}
