//! Pure HUD pieces: the badge chips, the toasts' life and slide, and the
//! dialogue's "animalese" blips (addendum §3.1, §3.4).

use nkw_core::Time;

use super::draw::{CREAM, ERROR_BG, ERROR_INK, INK, Rgb};
use crate::ext::{Badge, ToastKind};

/// A badge as a chip: text, background, ink.
pub fn chip(b: &Badge) -> (String, Rgb, Rgb) {
    const AMBER: Rgb = [0xff, 0xd8, 0x6a];
    const SKY: Rgb = [0xcf, 0xe6, 0xf5];
    const LEAF: Rgb = [0xd4, 0xec, 0xc4];
    const RED: Rgb = [0xe0, 0x4a, 0x3a];
    match b {
        Badge::Inbox(n) => (format!("✉ {n}"), AMBER, INK),
        Badge::Confirm => ("? confirmar".into(), AMBER, INK),
        Badge::BatteryLow(p) => (format!("bateria {p}%"), ERROR_BG, ERROR_INK),
        Badge::NetDown => ("sem rede".into(), ERROR_BG, ERROR_INK),
        Badge::VaultOpen => ("cofre aberto".into(), LEAF, INK),
        Badge::Updates(n) => (format!("{n} atualizações"), SKY, INK),
        Badge::OnAir => ("ON AIR".into(), RED, [0xff, 0xfb, 0xf2]),
        Badge::Layout(s) => (s.clone(), CREAM, INK),
    }
}

/// How long a toast slides in and out.
pub const TOAST_IN: Time = 0.14;
pub const TOAST_OUT: Time = 0.20;
/// The most toasts stacked at once.
pub const MAX_TOASTS: usize = 3;
/// How far a toast slides, logical px.
pub const SLIDE: f64 = 24.0;

#[derive(Clone, Debug, PartialEq)]
pub struct Toast {
    pub text: String,
    pub kind: ToastKind,
    pub born: Time,
    /// it starts leaving here
    pub until: Time,
}

/// `ms == 0`: the default (3 s info, 6 s error).
pub fn toast_ms(kind: ToastKind, ms: u32) -> u32 {
    match (ms, kind) {
        (0, ToastKind::Info) => 3000,
        (0, ToastKind::Error) => 6000,
        (ms, _) => ms,
    }
}

impl Toast {
    pub fn new(text: &str, kind: ToastKind, ms: u32, now: Time) -> Toast {
        Toast {
            text: text.to_string(),
            kind,
            born: now,
            until: now + toast_ms(kind, ms) as f64 / 1000.0,
        }
    }

    /// Gone (slid out) at `now`.
    pub fn gone(&self, now: Time, reduced: bool) -> bool {
        now >= self.until + if reduced { 0.0 } else { TOAST_OUT }
    }

    /// Sliding in or out at `now`.
    pub fn moving(&self, now: Time, reduced: bool) -> bool {
        !reduced && ((now - self.born) < TOAST_IN || (now >= self.until && !self.gone(now, false)))
    }

    /// Its x offset at `now` (negative: to the left of its place):
    /// ease-out in, ease-in out; none under reduced motion.
    pub fn offset(&self, now: Time, reduced: bool) -> i32 {
        if reduced {
            return 0;
        }
        let x = if now < self.born + TOAST_IN {
            let p = ((now - self.born) / TOAST_IN).clamp(0.0, 1.0);
            -(1.0 - (1.0 - (1.0 - p).powi(3))) * SLIDE
        } else if now >= self.until {
            let p = ((now - self.until) / TOAST_OUT).clamp(0.0, 1.0);
            -p.powi(3) * SLIDE * 3.0
        } else {
            0.0
        };
        x.round() as i32
    }
}

/// Push a toast; the oldest go beyond [`MAX_TOASTS`].
pub fn push(toasts: &mut Vec<Toast>, t: Toast) {
    toasts.push(t);
    let n = toasts.len();
    if n > MAX_TOASTS {
        toasts.drain(..n - MAX_TOASTS);
    }
}

/// The animalese voices (Hz): a cat's name picks one.
pub const VOICES: [f32; 7] = [392.0, 440.0, 494.0, 523.0, 587.0, 659.0, 698.0];
/// The most blips one frame emits (a whole text at once makes no burst).
pub const MAX_BLIPS: usize = 3;

/// A name's voice (FNV-1a over its bytes).
pub fn voice(name: &str) -> f32 {
    let mut h: u32 = 0x811c_9dc5;
    for b in name.bytes() {
        h ^= b as u32;
        h = h.wrapping_mul(0x0100_0193);
    }
    VOICES[(h % VOICES.len() as u32) as usize]
}

/// One blip's pitch: the voice ±4 %, varied by the character.
pub fn pitch(voice: f32, c: char) -> f32 {
    let v = (c as u32).wrapping_mul(2_654_435_761) % 81; // 0..=80
    voice * (1.0 + (v as f32 - 40.0) / 1000.0)
}

/// The blips for the text that appeared between `before` and `after` (only
/// when `after` continues `before`: a new dialogue starts from nothing).
pub fn blips(before: &str, after: &str, name: &str) -> Vec<f32> {
    let new = after.strip_prefix(before).unwrap_or(after);
    let v = voice(name);
    new.chars()
        .filter(|c| !c.is_whitespace())
        .take(MAX_BLIPS)
        .map(|c| pitch(v, c))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chips_for_every_badge() {
        let all = [
            Badge::Inbox(3),
            Badge::Confirm,
            Badge::BatteryLow(12),
            Badge::NetDown,
            Badge::VaultOpen,
            Badge::Updates(4),
            Badge::OnAir,
            Badge::Layout("us".into()),
        ];
        let texts: Vec<String> = all.iter().map(|b| chip(b).0).collect();
        assert_eq!(texts[0], "✉ 3");
        assert_eq!(texts[2], "bateria 12%");
        assert_eq!(texts[7], "us");
        assert!(texts.iter().all(|t| !t.is_empty()));
    }

    #[test]
    fn toasts_live_3s_or_6s_and_stack_at_most_3() {
        let t = Toast::new("ok", ToastKind::Info, 0, 10.0);
        assert_eq!(t.until, 13.0);
        assert!(!t.gone(13.1, false));
        assert!(t.gone(13.2, false));
        assert!(t.gone(13.0, true));
        let e = Toast::new("falhou", ToastKind::Error, 0, 10.0);
        assert_eq!(e.until, 16.0);
        assert_eq!(Toast::new("x", ToastKind::Info, 500, 0.0).until, 0.5);
        let mut v = Vec::new();
        for i in 0..5 {
            push(
                &mut v,
                Toast::new(&format!("{i}"), ToastKind::Info, 0, i as f64),
            );
        }
        assert_eq!(
            v.iter().map(|t| t.text.as_str()).collect::<Vec<_>>(),
            ["2", "3", "4"]
        );
    }

    #[test]
    fn toasts_slide_in_and_out() {
        let t = Toast::new("ok", ToastKind::Info, 1000, 0.0);
        assert_eq!(t.offset(0.0, false), -24);
        assert!(t.offset(0.07, false) > -24 && t.offset(0.07, false) < 0);
        assert_eq!(t.offset(0.14, false), 0);
        assert_eq!(t.offset(0.5, false), 0);
        assert!(t.offset(1.1, false) < 0);
        assert_eq!(t.offset(0.0, true), 0, "reduced motion: no slide");
        assert!(t.moving(0.05, false) && !t.moving(0.5, false) && t.moving(1.05, false));
        assert!(!t.moving(0.05, true));
    }

    #[test]
    fn blips_one_per_new_visible_char() {
        assert_eq!(
            blips("Ol", "Olá m", "acme").len(),
            2,
            "á and m, not the space"
        );
        assert_eq!(blips("", "Aplicar a migração", "acme").len(), MAX_BLIPS);
        assert!(blips("abc", "abc", "acme").is_empty());
        // a different text starts over
        assert_eq!(blips("xyz", "ab", "acme").len(), 2);
        let v = voice("api-gateway");
        assert!(VOICES.contains(&v));
        assert_eq!(v, voice("api-gateway"), "same cat, same voice");
        for c in "aeiouxyzç".chars() {
            let p = pitch(v, c);
            assert!((p / v - 1.0).abs() <= 0.0401, "{c}: {p}");
        }
    }
}
