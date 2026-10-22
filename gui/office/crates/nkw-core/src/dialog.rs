//! The dialogue of a CONVERSA (§3.7): the text appears letter by letter at
//! [`CPS`] characters a second (all at once under reduced motion, or after
//! `space`/`enter`), then the choices can be picked. Pure: the owner (cats,
//! objects) keeps one per open dialogue and asks it what to show at `now`;
//! the overlay draws the result.

use crate::Time;

/// Characters revealed per second.
pub const CPS: f32 = 45.0;
/// Frames a second while text is being revealed.
pub const REVEAL_HZ: f64 = 30.0;

#[derive(Clone, Debug, PartialEq)]
pub struct Dialog {
    text: String,
    /// chars in `text`
    total: usize,
    start: Time,
    revealed_all: bool,
    choices: Vec<String>,
    selected: usize,
    more: bool,
}

impl Dialog {
    /// A dialogue opened at `now`; under `reduced_motion` the whole text is
    /// shown from the start.
    pub fn new(text: &str, choices: Vec<String>, now: Time, reduced_motion: bool) -> Dialog {
        Dialog {
            text: text.to_string(),
            total: text.chars().count(),
            start: now,
            revealed_all: reduced_motion,
            choices,
            selected: 0,
            more: false,
        }
    }

    /// The same, with the "▼ there is more" mark on (another page follows).
    pub fn with_more(mut self, more: bool) -> Dialog {
        self.more = more;
        self
    }

    pub fn more(&self) -> bool {
        self.more
    }

    /// The whole text.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// How many chars are visible at `now`.
    pub fn shown_chars(&self, now: Time) -> usize {
        if self.revealed_all {
            return self.total;
        }
        let t = (now - self.start).max(0.0);
        ((t * CPS as f64 + 1e-9).floor() as usize).min(self.total)
    }

    /// The text revealed so far.
    pub fn shown(&self, now: Time) -> &str {
        let n = self.shown_chars(now);
        match self.text.char_indices().nth(n) {
            Some((i, _)) => &self.text[..i],
            None => &self.text,
        }
    }

    /// The whole text is visible (the choices can be picked).
    pub fn done(&self, now: Time) -> bool {
        self.shown_chars(now) >= self.total
    }

    /// `space`/`enter` while revealing: show everything now.
    pub fn reveal_all(&mut self) {
        self.revealed_all = true;
    }

    /// When the next frame is due while revealing (on a 30 Hz grid from the
    /// start); `None` once everything is shown.
    pub fn next_wake(&self, now: Time) -> Option<Time> {
        if self.done(now) {
            return None;
        }
        let k = ((now - self.start).max(0.0) * REVEAL_HZ).floor() + 1.0;
        Some(self.start + k / REVEAL_HZ)
    }

    /// Pick choice `i` (clamped to the last one).
    pub fn select(&mut self, i: usize) {
        self.selected = i.min(self.choices.len().saturating_sub(1));
    }

    /// `j`: the next choice (stays on the last).
    pub fn down(&mut self) {
        self.select(self.selected + 1);
    }

    /// `k`: the previous choice (stays on the first).
    pub fn up(&mut self) {
        self.select(self.selected.saturating_sub(1));
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    pub fn choices(&self) -> &[String] {
        &self.choices
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(text: &str) -> Dialog {
        Dialog::new(
            text,
            vec!["sim".into(), "não".into(), "talvez".into()],
            10.0,
            false,
        )
    }

    #[test]
    fn reveals_at_45_chars_a_second() {
        let x = d(&"a".repeat(90));
        let rows = [
            (10.0, 0),
            (9.0, 0),
            (10.0 + 1.0 / 45.0, 1),
            (10.5, 22),
            (11.0, 45),
            (12.0, 90),
            (99.0, 90),
        ];
        for (now, n) in rows {
            assert_eq!(x.shown_chars(now), n, "at {now}");
            assert_eq!(x.shown(now).len(), n);
        }
        assert!(!x.done(11.9));
        assert!(x.done(12.0));
    }

    #[test]
    fn shown_cuts_on_char_boundaries() {
        let x = d("ação é já");
        assert_eq!(x.shown(10.0 + 2.0 / 45.0), "aç");
        assert_eq!(x.shown(10.0 + 4.0 / 45.0), "ação");
        assert_eq!(x.shown(20.0), "ação é já");
        assert_eq!(x.text(), "ação é já");
    }

    #[test]
    fn reveal_all_and_reduced_motion() {
        let mut x = d("Aplicar a migração agora?");
        assert_eq!(x.shown(10.1), "Apli");
        x.reveal_all();
        assert_eq!(x.shown(10.1), "Aplicar a migração agora?");
        assert!(x.done(10.1));
        let r = Dialog::new("tudo de uma vez", vec![], 10.0, true);
        assert_eq!(r.shown(10.0), "tudo de uma vez");
        assert_eq!(r.next_wake(10.0), None);
    }

    #[test]
    fn next_wake_is_30_hz_then_none() {
        let x = d(&"a".repeat(45)); // one second of text
        let w = x.next_wake(10.0).unwrap();
        assert!((w - (10.0 + 1.0 / 30.0)).abs() < 1e-9);
        let w = x.next_wake(10.04).unwrap();
        assert!((w - (10.0 + 2.0 / 30.0)).abs() < 1e-9);
        assert!(x.next_wake(10.99).is_some());
        assert_eq!(x.next_wake(11.0), None);
        let empty = d("");
        assert_eq!(empty.next_wake(10.0), None);
        assert!(empty.done(10.0));
    }

    #[test]
    fn choices_select_and_clamp() {
        let mut x = d("?");
        assert_eq!(x.selected(), 0);
        x.up();
        assert_eq!(x.selected(), 0);
        x.down();
        x.down();
        x.down();
        assert_eq!(x.selected(), 2);
        x.select(1);
        assert_eq!(x.selected(), 1);
        x.select(8);
        assert_eq!(x.selected(), 2);
        assert_eq!(x.choices().len(), 3);
        let mut none = Dialog::new("oi", vec![], 0.0, false);
        none.down();
        assert_eq!(none.selected(), 0);
    }

    #[test]
    fn more_mark() {
        assert!(!d("a").more());
        assert!(d("a").with_more(true).more());
    }
}
