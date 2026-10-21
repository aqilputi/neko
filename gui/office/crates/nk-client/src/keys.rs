//! Keys → bytes for the PTY (xterm sequences with modifiers, the semantics of
//! the TUI's `internal/term/keys.go`) and → the core keymap's notation (the
//! Bubble Tea v2 keystrokes the TUI binds: "ctrl+k", "shift+space", "G"…).

/// A key press. Printable keys come as `Char` already shifted (`'G'`, `':'`);
/// space is `Char(' ')`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Key {
    Char(char),
    Enter,
    Tab,
    Backspace,
    Esc,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
    Insert,
    Delete,
    /// F1..F12
    F(u8),
}

/// Modifiers held with a key (lock keys never count).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Mods {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
}

impl Mods {
    pub const NONE: Mods = Mods { ctrl: false, alt: false, shift: false };
    pub const CTRL: Mods = Mods { ctrl: true, alt: false, shift: false };
    pub const ALT: Mods = Mods { ctrl: false, alt: true, shift: false };
    pub const SHIFT: Mods = Mods { ctrl: false, alt: false, shift: true };

    /// The xterm modifier parameter: 1 + shift(1) + alt(2) + ctrl(4).
    fn param(self) -> u8 {
        1 + self.shift as u8 + 2 * self.alt as u8 + 4 * self.ctrl as u8
    }
}

/// The bytes to write to the PTY for a key (normal cursor-key mode).
pub fn encode_key(key: Key, mods: Mods) -> Vec<u8> {
    encode_key_mode(key, mods, false)
}

/// Like [`encode_key`], honoring the terminal's application cursor-key mode
/// (DECCKM, `Term::app_cursor`): unmodified arrows/home/end become `ESC O x`.
pub fn encode_key_mode(key: Key, mods: Mods, app_cursor: bool) -> Vec<u8> {
    let m = mods.param();
    let esc = |mut v: Vec<u8>| {
        if mods.alt {
            v.insert(0, 0x1b);
        }
        v
    };
    match key {
        Key::Enter if mods.shift || mods.alt => b"\x1b\r".to_vec(), // newline in Claude Code's prompt
        Key::Enter => b"\r".to_vec(),
        Key::Backspace if mods.ctrl => vec![0x17], // ctrl+w: delete the previous word
        Key::Backspace => esc(vec![0x7f]),
        Key::Tab if mods.shift => b"\x1b[Z".to_vec(),
        Key::Tab => esc(b"\t".to_vec()),
        Key::Esc => esc(vec![0x1b]),
        Key::Up | Key::Down | Key::Right | Key::Left | Key::Home | Key::End => {
            let f = match key {
                Key::Up => 'A',
                Key::Down => 'B',
                Key::Right => 'C',
                Key::Left => 'D',
                Key::Home => 'H',
                _ => 'F',
            };
            if m > 1 {
                format!("\x1b[1;{m}{f}").into_bytes()
            } else if app_cursor {
                format!("\x1bO{f}").into_bytes()
            } else {
                format!("\x1b[{f}").into_bytes()
            }
        }
        Key::Insert | Key::Delete | Key::PageUp | Key::PageDown => {
            let n = match key {
                Key::Insert => 2,
                Key::Delete => 3,
                Key::PageUp => 5,
                _ => 6,
            };
            tilde(n, m)
        }
        Key::F(n @ 1..=4) => {
            let f = (b'P' + n - 1) as char;
            if m > 1 { format!("\x1b[1;{m}{f}").into_bytes() } else { format!("\x1bO{f}").into_bytes() }
        }
        Key::F(n @ 5..=12) => tilde([15, 17, 18, 19, 20, 21, 23, 24][(n - 5) as usize], m),
        Key::F(_) => Vec::new(),
        Key::Char(c) => {
            if mods.ctrl
                && let Some(b) = ctrl_byte(c)
            {
                return esc(vec![b]);
            }
            let mut buf = [0u8; 4];
            esc(c.encode_utf8(&mut buf).as_bytes().to_vec())
        }
    }
}

fn tilde(n: u8, m: u8) -> Vec<u8> {
    if m > 1 { format!("\x1b[{n};{m}~").into_bytes() } else { format!("\x1b[{n}~").into_bytes() }
}

/// The C0 byte of ctrl+c, as xterm sends it.
fn ctrl_byte(c: char) -> Option<u8> {
    Some(match c {
        'a'..='z' => c as u8 - b'a' + 1,
        'A'..='Z' => c as u8 - b'A' + 1,
        ' ' | '@' | '2' => 0,
        '[' | '3' => 0x1b,
        '\\' | '4' => 0x1c,
        ']' | '5' => 0x1d,
        '^' | '6' => 0x1e,
        '_' | '/' | '-' | '7' => 0x1f,
        '?' | '8' => 0x7f,
        _ => return None,
    })
}

/// The key in the core keymap's notation: named keys spell every modifier
/// ("shift+tab", "shift+space", "alt+shift+up", "f5"); printable characters
/// already carry shift ("G", ":", "_"), except a letter with ctrl/alt
/// ("ctrl+shift+k").
pub fn key_name(key: &Key, mods: Mods) -> String {
    let named = match key {
        Key::Char(' ') => Some("space".to_string()),
        Key::Char(_) => None,
        Key::Enter => Some("enter".into()),
        Key::Tab => Some("tab".into()),
        Key::Backspace => Some("backspace".into()),
        Key::Esc => Some("esc".into()),
        Key::Up => Some("up".into()),
        Key::Down => Some("down".into()),
        Key::Left => Some("left".into()),
        Key::Right => Some("right".into()),
        Key::Home => Some("home".into()),
        Key::End => Some("end".into()),
        Key::PageUp => Some("pgup".into()),
        Key::PageDown => Some("pgdown".into()),
        Key::Insert => Some("insert".into()),
        Key::Delete => Some("delete".into()),
        Key::F(n) => Some(format!("f{n}")),
    };
    if let Some(n) = named {
        return prefix(mods.ctrl, mods.alt, mods.shift) + &n;
    }
    let Key::Char(c) = *key else { unreachable!() };
    if mods.ctrl || mods.alt {
        let lower: String = c.to_lowercase().collect();
        let upper: String = c.to_uppercase().collect();
        if lower != upper {
            // a letter: ctrl+K is spelled ctrl+shift+k
            let shift = mods.shift || c.to_string() != lower;
            return prefix(mods.ctrl, mods.alt, shift) + &lower;
        }
        return prefix(mods.ctrl, mods.alt, false) + &c.to_string();
    }
    c.to_string()
}

fn prefix(ctrl: bool, alt: bool, shift: bool) -> String {
    let mut s = String::new();
    if ctrl {
        s.push_str("ctrl+");
    }
    if alt {
        s.push_str("alt+");
    }
    if shift {
        s.push_str("shift+");
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    const CA: Mods = Mods { ctrl: true, alt: true, shift: false };

    // the cases of internal/term/keys_test.go (TestKeysReachProgram), plus
    // the unmodified keys x/vt encodes there
    #[test]
    fn go_cases() {
        let cases: &[(&str, Key, Mods, &[u8])] = &[
            ("backspace", Key::Backspace, Mods::NONE, b"\x7f"),
            ("left", Key::Left, Mods::NONE, b"\x1b[D"),
            ("ctrl+left", Key::Left, Mods::CTRL, b"\x1b[1;5D"),
            ("alt+right", Key::Right, Mods::ALT, b"\x1b[1;3C"),
            ("shift+end", Key::End, Mods::SHIFT, b"\x1b[1;2F"),
            ("ctrl+delete", Key::Delete, Mods::CTRL, b"\x1b[3;5~"),
            ("delete", Key::Delete, Mods::NONE, b"\x1b[3~"),
            ("shift+enter", Key::Enter, Mods::SHIFT, b"\x1b\r"),
            ("alt+enter", Key::Enter, Mods::ALT, b"\x1b\r"),
            ("ctrl+backspace", Key::Backspace, Mods::CTRL, b"\x17"),
            ("alt+backspace", Key::Backspace, Mods::ALT, b"\x1b\x7f"),
            ("text", Key::Char('A'), Mods::NONE, b"A"),
        ];
        for (name, k, m, want) in cases {
            assert_eq!(encode_key(*k, *m), want.to_vec(), "{name}");
        }
    }

    #[test]
    fn plain_keys() {
        let cases: &[(Key, &[u8])] = &[
            (Key::Enter, b"\r"),
            (Key::Tab, b"\t"),
            (Key::Esc, b"\x1b"),
            (Key::Up, b"\x1b[A"),
            (Key::Down, b"\x1b[B"),
            (Key::Right, b"\x1b[C"),
            (Key::Home, b"\x1b[H"),
            (Key::End, b"\x1b[F"),
            (Key::Insert, b"\x1b[2~"),
            (Key::PageUp, b"\x1b[5~"),
            (Key::PageDown, b"\x1b[6~"),
            (Key::F(1), b"\x1bOP"),
            (Key::F(4), b"\x1bOS"),
            (Key::F(5), b"\x1b[15~"),
            (Key::F(12), b"\x1b[24~"),
            (Key::F(13), b""),
            (Key::Char(' '), b" "),
            (Key::Char('ç'), "ç".as_bytes()),
        ];
        for (k, want) in cases {
            assert_eq!(encode_key(*k, Mods::NONE), want.to_vec(), "{k:?}");
        }
    }

    #[test]
    fn modified_keys() {
        assert_eq!(encode_key(Key::Tab, Mods::SHIFT), b"\x1b[Z");
        assert_eq!(encode_key(Key::PageDown, Mods { shift: true, ctrl: true, alt: false }), b"\x1b[6;6~");
        assert_eq!(encode_key(Key::Up, Mods { shift: true, alt: true, ctrl: true }), b"\x1b[1;8A");
        assert_eq!(encode_key(Key::F(2), Mods::SHIFT), b"\x1b[1;2Q");
        assert_eq!(encode_key(Key::F(5), Mods::CTRL), b"\x1b[15;5~");
        assert_eq!(encode_key(Key::Char('c'), Mods::CTRL), b"\x03");
        assert_eq!(encode_key(Key::Char('C'), Mods::CTRL), b"\x03");
        assert_eq!(encode_key(Key::Char(' '), Mods::CTRL), b"\x00");
        assert_eq!(encode_key(Key::Char('['), Mods::CTRL), b"\x1b");
        assert_eq!(encode_key(Key::Char(']'), Mods::CTRL), b"\x1d");
        assert_eq!(encode_key(Key::Char('/'), Mods::CTRL), b"\x1f");
        assert_eq!(encode_key(Key::Char('x'), Mods::ALT), b"\x1bx");
        assert_eq!(encode_key(Key::Char('x'), CA), b"\x1b\x18");
        assert_eq!(encode_key(Key::Esc, Mods::ALT), b"\x1b\x1b");
        assert_eq!(encode_key(Key::Char('é'), Mods::CTRL), "é".as_bytes());
    }

    #[test]
    fn app_cursor_mode() {
        assert_eq!(encode_key_mode(Key::Up, Mods::NONE, true), b"\x1bOA");
        assert_eq!(encode_key_mode(Key::Home, Mods::NONE, true), b"\x1bOH");
        assert_eq!(encode_key_mode(Key::Up, Mods::CTRL, true), b"\x1b[1;5A");
        assert_eq!(encode_key_mode(Key::Delete, Mods::NONE, true), b"\x1b[3~");
    }

    // the cases of gui/web/src/keymap/keys.test.ts
    #[test]
    fn names() {
        let cs = Mods { ctrl: true, shift: true, alt: false };
        let asf = Mods { alt: true, shift: true, ctrl: false };
        let cases: &[(Key, Mods, &str)] = &[
            (Key::Char('j'), Mods::NONE, "j"),
            (Key::Char('G'), Mods::SHIFT, "G"),
            (Key::Char(':'), Mods::SHIFT, ":"),
            (Key::Char('?'), Mods::SHIFT, "?"),
            (Key::Char('/'), Mods::NONE, "/"),
            (Key::Char('1'), Mods::NONE, "1"),
            (Key::Enter, Mods::NONE, "enter"),
            (Key::Esc, Mods::NONE, "esc"),
            (Key::Tab, Mods::NONE, "tab"),
            (Key::Tab, Mods::SHIFT, "shift+tab"),
            (Key::Char(' '), Mods::NONE, "space"),
            (Key::Char(' '), Mods::SHIFT, "shift+space"),
            (Key::Char(' '), Mods::CTRL, "ctrl+space"),
            (Key::Down, Mods::NONE, "down"),
            (Key::Left, Mods::NONE, "left"),
            (Key::PageDown, Mods::NONE, "pgdown"),
            (Key::PageUp, Mods::NONE, "pgup"),
            (Key::Up, Mods::ALT, "alt+up"),
            (Key::Up, asf, "alt+shift+up"),
            (Key::Char('k'), Mods::CTRL, "ctrl+k"),
            (Key::Char('K'), cs, "ctrl+shift+k"),
            (Key::Char('K'), Mods::CTRL, "ctrl+shift+k"),
            (Key::Char('/'), Mods::CTRL, "ctrl+/"),
            (Key::Char('_'), cs, "ctrl+_"),
            (Key::Char(']'), Mods::CTRL, "ctrl+]"),
            (Key::Char('x'), Mods::ALT, "alt+x"),
            (Key::F(5), Mods::NONE, "f5"),
            (Key::Char('ç'), Mods::NONE, "ç"),
            (Key::Backspace, Mods::CTRL, "ctrl+backspace"),
        ];
        for (k, m, want) in cases {
            assert_eq!(key_name(k, *m), *want, "{k:?} {m:?}");
        }
    }
}
