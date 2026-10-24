//! Colours and text scale of the toolkit.

use super::Rgb;
use crate::overlay::draw;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Theme {
    pub cream: Rgb,
    pub ink: Rgb,
    pub soft: Rgb,
    pub select: Rgb,
    pub ring: Rgb,
    pub accent: Rgb,
    pub error_bg: Rgb,
    pub error_ink: Rgb,
    pub high_contrast: bool,
    /// 80..=160
    pub font_scale_pct: u16,
}

pub fn text_pairs(t: &Theme) -> Vec<(&'static str, Rgb, Rgb)> {
    vec![
        ("ink/cream", t.ink, t.cream),
        ("ink/select", t.ink, t.select),
        ("soft/cream", t.soft, t.cream),
        ("soft/select", t.soft, t.select),
        ("ink/accent", t.ink, t.accent),
        ("error_ink/error_bg", t.error_ink, t.error_bg),
    ]
}

impl Default for Theme {
    fn default() -> Theme {
        Theme::cozy()
    }
}

impl Theme {
    pub fn cozy() -> Theme {
        Theme {
            cream: draw::CREAM,
            ink: draw::INK,
            soft: draw::SOFT,
            select: draw::SELECT,
            ring: draw::RING,
            accent: draw::HINT_BG,
            error_bg: draw::ERROR_BG,
            error_ink: draw::ERROR_INK,
            high_contrast: false,
            font_scale_pct: 100,
        }
    }

    pub fn high_contrast() -> Theme {
        Theme {
            cream: [0xff, 0xfa, 0xf0],
            ink: [0x1c, 0x12, 0x0b],
            soft: [0x3a, 0x2a, 0x1e],
            error_ink: [0x5c, 0x1a, 0x10],
            high_contrast: true,
            ..Theme::cozy()
        }
    }

    /// The text/background pairs the toolkit and the overlay draw (name,
    /// ink, background): the contrast test walks them.
    pub fn text_pairs(&self) -> Vec<(&'static str, Rgb, Rgb)> {
        text_pairs(self)
    }

    /// A text size scaled by `font_scale_pct`, never below 10 px.
    pub fn px(&self, base: u32) -> u32 {
        let pct = self.font_scale_pct.clamp(80, 160) as u32;
        ((base * pct + 50) / 100).max(10)
    }
}
