//! What a monitor shows when its session has no pane open: the session's
//! activity tail, coloured by kind, with new lines scrolling in one by one
//! (the "code running" effect). Pure, unit-tested.

/// Colour role of a line (mapped to theme tokens by the renderer).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    /// `● Tool(arg)` — accent
    Tool,
    /// assistant text — normal text
    Text,
    /// `> what you typed` — subtle
    User,
    /// `  ⎿ result` — muted
    Result,
    /// `◆ subagent` — accent2
    Sub,
    /// diff `+` lines
    Add,
    /// diff `-` lines
    Del,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    pub at: String,
    pub role: Role,
    pub text: String,
}

pub fn role_of(kind: &str, text: &str) -> Role {
    match kind {
        "tool" => Role::Tool,
        "user" => Role::User,
        "sub" => Role::Sub,
        "result" => {
            let t = text.trim_start();
            if t.starts_with("+ ") || t.starts_with("+  ") {
                Role::Add
            } else if t.starts_with("- ") {
                Role::Del
            } else {
                Role::Result
            }
        }
        _ => Role::Text,
    }
}

/// Lines a monitor keeps (more than it shows, to match the next tail).
pub const KEEP: usize = 60;
/// Reveal one queued line every this many ms (a session's burst of new
/// lines scrolls in instead of jumping).
pub const REVEAL_MS: i64 = 110;

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Feed {
    /// every known line, oldest first
    pub lines: Vec<Line>,
    /// how many of them are on screen (the rest is queued to scroll in)
    pub shown: usize,
    /// when the last line was revealed
    pub last_reveal: i64,
}

impl Feed {
    /// A fresh tail from the core (oldest → newest). Lines already known are
    /// kept; the new ones queue up to scroll in. A tail that doesn't overlap
    /// what we had replaces it (only its last 3 lines animate).
    pub fn update(&mut self, tail: Vec<Line>, animate: bool) {
        if tail.is_empty() {
            return;
        }
        let overlap = self.overlap(&tail);
        match overlap {
            Some(k) => {
                let fresh: Vec<Line> = tail[k..].to_vec();
                if fresh.is_empty() {
                    return;
                }
                self.lines.extend(fresh);
            }
            None => {
                let n = tail.len();
                self.lines = tail;
                self.shown = if animate { n.saturating_sub(3) } else { n };
            }
        }
        if !animate {
            self.shown = self.lines.len();
        }
        if self.lines.len() > KEEP {
            let cut = self.lines.len() - KEEP;
            self.lines.drain(..cut);
            self.shown = self.shown.saturating_sub(cut);
        }
    }

    /// How many leading lines of `tail` we already have: the longest
    /// prefix of `tail` that is a suffix of ours.
    fn overlap(&self, tail: &[Line]) -> Option<usize> {
        if self.lines.is_empty() {
            return None;
        }
        let max = tail.len().min(self.lines.len());
        for k in (1..=max).rev() {
            if self.lines[self.lines.len() - k..] == tail[..k] {
                return Some(k);
            }
        }
        None
    }

    /// Reveals queued lines at REVEAL_MS each; true if something changed.
    pub fn tick(&mut self, now: i64) -> bool {
        if self.shown >= self.lines.len() {
            return false;
        }
        if now - self.last_reveal < REVEAL_MS {
            return false;
        }
        self.shown += 1;
        self.last_reveal = now;
        true
    }

    /// When (ms) the next queued line is due, for sleeping until then
    /// instead of redrawing every frame; None with nothing queued.
    pub fn next_reveal(&self, now: i64) -> Option<i64> {
        self.pending()
            .then(|| now.max(self.last_reveal + REVEAL_MS))
    }

    pub fn pending(&self) -> bool {
        self.shown < self.lines.len()
    }

    /// The `rows` lines on screen (bottom-aligned: newest last).
    pub fn visible(&self, rows: usize) -> &[Line] {
        let end = self.shown.min(self.lines.len());
        &self.lines[end.saturating_sub(rows)..end]
    }
}

/// Cuts a line to `cols` display columns (wide chars count 2), with "…".
pub fn clip(text: &str, cols: usize) -> String {
    let mut w = 0;
    let mut out = String::new();
    let chars: Vec<char> = text.chars().collect();
    for (i, c) in chars.iter().enumerate() {
        let cw = char_width(*c);
        if w + cw > cols || (w + cw == cols && i + 1 < chars.len()) {
            if w < cols {
                out.push('…');
            }
            return out;
        }
        out.push(*c);
        w += cw;
    }
    out
}

/// Display width of a char in a terminal grid (CJK/emoji wide = 2).
pub fn char_width(c: char) -> usize {
    let u = c as u32;
    if u < 0x1100 {
        return 1;
    }
    let wide = (0x1100..=0x115F).contains(&u)
        || (0x2E80..=0xA4CF).contains(&u)
        || (0xAC00..=0xD7A3).contains(&u)
        || (0xF900..=0xFAFF).contains(&u)
        || (0xFE30..=0xFE4F).contains(&u)
        || (0xFF00..=0xFF60).contains(&u)
        || (0xFFE0..=0xFFE6).contains(&u)
        || (0x1F300..=0x1FAFF).contains(&u)
        || (0x20000..=0x3FFFD).contains(&u);
    if wide { 2 } else { 1 }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn l(i: usize) -> Line {
        Line {
            at: i.to_string(),
            role: Role::Text,
            text: format!("line {i}"),
        }
    }

    #[test]
    fn roles() {
        assert_eq!(role_of("tool", "● Bash(x)"), Role::Tool);
        assert_eq!(role_of("result", "  ⎿ ok"), Role::Result);
        assert_eq!(role_of("result", "  +  fn x()"), Role::Add);
        assert_eq!(role_of("result", "  - old"), Role::Del);
        assert_eq!(role_of("user", "> oi"), Role::User);
        assert_eq!(role_of("sub", "◆ Explore"), Role::Sub);
        assert_eq!(role_of("text", "Vou olhar."), Role::Text);
        assert_eq!(role_of("???", ""), Role::Text);
    }

    #[test]
    fn first_tail_shows_mostly_at_once() {
        let mut f = Feed::default();
        f.update((0..10).map(l).collect(), true);
        assert_eq!(f.shown, 7);
        assert!(f.pending());
        let mut f = Feed::default();
        f.update((0..10).map(l).collect(), false);
        assert_eq!(f.shown, 10);
    }

    #[test]
    fn new_lines_scroll_in() {
        let mut f = Feed::default();
        f.update((0..10).map(l).collect(), false);
        // the core's next tail (last 10): two new lines
        f.update((2..12).map(l).collect(), true);
        assert_eq!(f.lines.len(), 12);
        assert_eq!(f.shown, 10);
        assert_eq!(f.next_reveal(900), Some(900), "first one: now");
        assert!(f.tick(1000));
        assert_eq!(f.next_reveal(1050), Some(1000 + REVEAL_MS));
        assert!(!f.tick(1050), "too soon");
        assert!(f.tick(1000 + REVEAL_MS));
        assert!(!f.tick(5000), "nothing left");
        assert_eq!(f.next_reveal(5000), None);
        assert_eq!(
            f.visible(3)
                .iter()
                .map(|x| x.at.as_str())
                .collect::<Vec<_>>(),
            ["9", "10", "11"]
        );
        // same tail again: nothing new
        f.update((2..12).map(l).collect(), true);
        assert_eq!(f.lines.len(), 12);
    }

    #[test]
    fn keeps_a_bounded_history() {
        let mut f = Feed::default();
        f.update((0..50).map(l).collect(), false);
        f.update((30..80).map(l).collect(), false);
        assert_eq!(f.lines.len(), KEEP);
        assert_eq!(f.lines.last().unwrap().at, "79");
        assert_eq!(f.shown, KEEP);
        assert_eq!(f.visible(100).len(), KEEP);
    }

    #[test]
    fn unrelated_tail_replaces() {
        let mut f = Feed::default();
        f.update((0..5).map(l).collect(), false);
        f.update((100..104).map(l).collect(), false);
        assert_eq!(f.lines[0].at, "100");
        assert_eq!(f.lines.len(), 4);
    }

    #[test]
    fn clipping() {
        assert_eq!(clip("abcdef", 10), "abcdef");
        assert_eq!(clip("abcdef", 4), "abc…");
        assert_eq!(clip("abcd", 4), "abcd");
        assert_eq!(clip("日本語", 4), "日…");
        assert_eq!(char_width('a'), 1);
        assert_eq!(char_width('日'), 2);
    }
}
