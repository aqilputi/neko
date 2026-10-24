//! Measuring a [`Node`] tree. Sizes are in the raster's own px: logical px
//! at scale 1 ([`super::measure`]), physical px at `s` ([`measure_at`]);
//! every node's logical sizes go through `s.p(..)` and its text through
//! `s.font(..)`, so a tree measured at 1.25 is what its raster will be.

use nk_cats::raster::Canvas;

use super::{Node, Size, Theme};
use crate::overlay::draw::{Fonts, PX_SMALL, Px};

/// The padding a [`Node::Selected`] adds around its child (logical px):
/// the unselected twin of a row is [`super::plain`], which pads the same.
pub const SEL_PAD: (i32, i32) = (8, 4);
/// The radius of the selection's background.
pub const SEL_R: i32 = 10;
/// A chip's padding and its text size (before the theme's scale).
pub const CHIP_PAD: (i32, i32) = (8, 3);
pub const CHIP_PX: u32 = PX_SMALL - 1;
/// A big panel's soft shadow (radius ≥ [`SHADOW_MIN_R`], opaque).
pub const SHADOW: i32 = 4;
pub const SHADOW_MIN_R: i32 = 16;

/// What measuring and painting need: the faces, the theme, the scale.
pub struct Cx<'a> {
    pub fonts: &'a mut Fonts,
    pub t: &'a Theme,
    pub s: Px,
}

impl Cx<'_> {
    /// The text width of `s` at `px` (raster px).
    pub fn text_w(&mut self, s: &str, px: u32) -> i32 {
        let f = self.fonts.face(self.s.font(px));
        Canvas::text_width(f, s)
    }

    /// A face's cell (raster px).
    pub fn cell(&mut self, px: u32) -> (i32, i32) {
        let f = self.fonts.face(self.s.font(px));
        (f.cw as i32, f.ch as i32)
    }
}

/// Whether a panel of this look casts the soft shadow.
pub fn casts_shadow(radius: i32, alpha: u8) -> bool {
    radius >= SHADOW_MIN_R && alpha == 255
}

fn shrink(a: Size, dw: i32, dh: i32) -> Size {
    Size {
        w: (a.w - dw).max(0),
        h: (a.h - dh).max(0),
    }
}

fn cap(s: Size, avail: Size) -> Size {
    Size {
        w: s.w.clamp(0, avail.w.max(0)),
        h: s.h.clamp(0, avail.h.max(0)),
    }
}

/// Whether a child stretches to its container's width (a Row with a
/// `Spacer(0)`, a Rule, or something that wraps one).
pub fn stretches_w(n: &Node) -> bool {
    match n {
        Node::Row { items, .. } | Node::Column { items, .. } => items
            .iter()
            .any(|i| matches!(i, Node::Spacer(0)) || stretches_w(i)),
        Node::Rule { .. } => true,
        Node::Panel { child, .. } | Node::Selected { child, .. } | Node::Faded { child, .. } => {
            stretches_w(child)
        }
        Node::Fixed { w, child, .. } => *w <= 0 && stretches_w(child),
        _ => false,
    }
}

/// The size `n` takes inside `avail` at scale `cx.s` (raster px).
pub fn size(n: &Node, avail: Size, cx: &mut Cx) -> Size {
    let s = cx.s;
    let got = match n {
        Node::Panel {
            radius,
            alpha,
            pad,
            child,
            ..
        } => {
            let p = s.p(*pad);
            let sh = if casts_shadow(*radius, *alpha) {
                s.p(SHADOW)
            } else {
                0
            };
            let c = size(child, shrink(avail, 2 * p, 2 * p + sh), cx);
            Size {
                w: c.w + 2 * p,
                h: c.h + 2 * p + sh,
            }
        }
        Node::Column { gap, items } => {
            let g = s.p(*gap);
            let mut h = 0;
            let mut w = 0;
            let mut grow = false;
            for (i, it) in items.iter().enumerate() {
                if i > 0 {
                    h += g;
                }
                if matches!(it, Node::Spacer(0)) {
                    grow = true;
                    continue;
                }
                let c = size(it, shrink(avail, 0, h), cx);
                h += c.h;
                w = w.max(c.w);
            }
            if items.iter().any(stretches_w) {
                w = avail.w;
            }
            if grow {
                h = h.max(avail.h);
            }
            Size { w, h }
        }
        Node::Row { gap, items } => {
            let sizes = row_sizes(items, *gap, avail, cx);
            let g = s.p(*gap) * (items.len() as i32 - 1).max(0);
            let w: i32 = sizes.iter().flatten().map(|z| z.w).sum::<i32>() + g;
            let h = sizes.iter().flatten().map(|z| z.h).max().unwrap_or(0);
            let grow = sizes.iter().any(Option::is_none);
            Size {
                w: if grow { w.max(avail.w) } else { w },
                h,
            }
        }
        Node::Text { s: text, px, .. } => {
            let w = cx.text_w(text, *px);
            let (_, ch) = cx.cell(*px);
            Size { w, h: ch }
        }
        Node::Icon { px, .. } => Size {
            w: s.p(*px),
            h: s.p(*px),
        },
        Node::Gauge { w, h, .. } | Node::Bars { w, h, .. } => Size {
            w: s.p(*w),
            h: s.p(*h),
        },
        Node::Chip { text, .. } => {
            let px = cx.t.px(CHIP_PX);
            let w = cx.text_w(text, px);
            let (_, ch) = cx.cell(px);
            Size {
                w: w + 2 * s.p(CHIP_PAD.0),
                h: ch + 2 * s.p(CHIP_PAD.1),
            }
        }
        Node::Spacer(n) => Size {
            w: s.p(*n),
            h: s.p(*n),
        },
        Node::Rule { .. } => Size {
            w: 0,
            h: s.p(1).max(1),
        },
        Node::Fixed { w, h, child } => {
            let c = if *w > 0 && *h > 0 {
                Size::default()
            } else {
                size(child, avail, cx)
            };
            Size {
                w: if *w > 0 { s.p(*w) } else { c.w },
                h: if *h > 0 { s.p(*h) } else { c.h },
            }
        }
        Node::Selected { child, .. } => {
            let (px, py) = (s.p(SEL_PAD.0), s.p(SEL_PAD.1));
            let c = size(child, shrink(avail, 2 * px, 2 * py), cx);
            Size {
                w: c.w + 2 * px,
                h: c.h + 2 * py,
            }
        }
        Node::Faded { child, .. } => size(child, avail, cx),
    };
    cap(got, avail)
}

/// The sizes of a Row's items (`None` for a `Spacer(0)`): the items that do
/// not stretch first, then the stretching ones share what is left, so a
/// stretching item never squeezes a fixed one after it.
pub fn row_sizes(items: &[Node], gap: i32, avail: Size, cx: &mut Cx) -> Vec<Option<Size>> {
    let g = cx.s.p(gap) * (items.len() as i32 - 1).max(0);
    let mut out: Vec<Option<Size>> = vec![None; items.len()];
    let mut used = g;
    for (i, it) in items.iter().enumerate() {
        if matches!(it, Node::Spacer(0)) || stretches_w(it) {
            continue;
        }
        let z = size(it, shrink(avail, used, 0), cx);
        used += z.w;
        out[i] = Some(z);
    }
    let n = items
        .iter()
        .filter(|it| !matches!(it, Node::Spacer(0)) && stretches_w(it))
        .count() as i32;
    if n > 0 {
        let share = (avail.w - used).max(0) / n;
        for (i, it) in items.iter().enumerate() {
            if !matches!(it, Node::Spacer(0)) && stretches_w(it) {
                let z = size(
                    it,
                    Size {
                        w: share,
                        h: avail.h,
                    },
                    cx,
                );
                out[i] = Some(z);
            }
        }
    }
    out
}

/// [`super::measure`] at scale `s` (raster px).
pub fn measure_at(n: &Node, avail: Size, fonts: &mut Fonts, t: &Theme, s: Px) -> Size {
    size(n, avail, &mut Cx { fonts, t, s })
}

pub fn measure(n: &Node, avail: Size, fonts: &mut Fonts, t: &Theme) -> Size {
    measure_at(n, avail, fonts, t, Px(1.0))
}
