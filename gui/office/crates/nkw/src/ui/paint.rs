//! Painting a [`Node`] tree onto a canvas: every node draws at the size
//! [`layout::size`] gives it, inside the box it is handed (the public
//! [`paint_at`] paints into a box-sized canvas and blits it, so nothing
//! ever lands outside `avail`).

use nk_cats::raster::Canvas;

use super::layout::{self, CHIP_PAD, CHIP_PX, Cx, SEL_PAD, SEL_R, SHADOW, casts_shadow, size};
use super::{Node, Size, Theme, icon};
use crate::overlay::draw::{FADE_STEPS, Fonts, Px, ellipsize};

/// Paints `n` with its top-left at `at` inside the box `avail` (raster px).
pub fn node(n: &Node, c: &mut Canvas, at: (i32, i32), avail: Size, cx: &mut Cx) {
    let s = cx.s;
    let me = size(n, avail, cx);
    let (x, y) = at;
    match n {
        Node::Panel {
            radius,
            bg,
            alpha,
            pad,
            child,
        } => {
            let r = s.p(*radius);
            let sh = if casts_shadow(*radius, *alpha) {
                let k = s.p(SHADOW);
                c.rounded_rect_alpha(x, y + k, me.w, me.h - k, r, cx.t.ink, 40);
                k
            } else {
                0
            };
            c.rounded_rect_alpha(x, y, me.w, me.h - sh, r, *bg, *alpha);
            let p = s.p(*pad);
            let inner = Size {
                w: me.w - 2 * p,
                h: me.h - 2 * p - sh,
            };
            node(child, c, (x + p, y + p), inner, cx);
        }
        Node::Column { gap, items } => {
            let g = s.p(*gap);
            let sizes: Vec<Option<Size>> = {
                let mut used = 0;
                let mut v = Vec::new();
                for (i, it) in items.iter().enumerate() {
                    if i > 0 {
                        used += g;
                    }
                    if matches!(it, Node::Spacer(0)) {
                        v.push(None);
                        continue;
                    }
                    let sz = size(
                        it,
                        Size {
                            w: me.w,
                            h: (me.h - used).max(0),
                        },
                        cx,
                    );
                    used += sz.h;
                    v.push(Some(sz));
                }
                v
            };
            let fixed: i32 = sizes.iter().flatten().map(|z| z.h).sum::<i32>()
                + g * (items.len() as i32 - 1).max(0);
            let grow = sizes.iter().filter(|z| z.is_none()).count() as i32;
            let extra = if grow > 0 {
                ((me.h - fixed).max(0)) / grow
            } else {
                0
            };
            let mut yy = y;
            for (it, sz) in items.iter().zip(&sizes) {
                match sz {
                    None => yy += extra,
                    Some(z) => {
                        let bx = Size {
                            w: me.w,
                            h: z.h.min((y + me.h - yy).max(0)),
                        };
                        node(it, c, (x, yy), bx, cx);
                        yy += z.h;
                    }
                }
                yy += g;
            }
        }
        Node::Row { gap, items } => {
            let g = s.p(*gap);
            let sizes = layout::row_sizes(items, *gap, me, cx);
            let used: i32 = sizes.iter().flatten().map(|z| z.w).sum::<i32>()
                + g * (items.len() as i32 - 1).max(0);
            let grow = sizes.iter().filter(|z| z.is_none()).count() as i32;
            let extra = if grow > 0 {
                ((me.w - used).max(0)) / grow
            } else {
                0
            };
            let mut xx = x;
            for (it, sz) in items.iter().zip(&sizes) {
                match sz {
                    None => xx += extra,
                    Some(z) => {
                        let w = z.w.min((x + me.w - xx).max(0));
                        let yy = y + (me.h - z.h) / 2;
                        node(it, c, (xx, yy), Size { w, h: z.h }, cx);
                        xx += z.w;
                    }
                }
                xx += g;
            }
        }
        Node::Text {
            s: text,
            px,
            rgb,
            bold,
        } => {
            let face = cx.fonts.face(s.font(*px));
            let full = Canvas::text_width(face, text);
            let shown = if full > me.w {
                let cols = (me.w / (face.cw as i32).max(1)).max(1) as usize;
                ellipsize(text, cols)
            } else {
                text.clone()
            };
            c.text_px(face, x, y, &shown, *rgb, *bold);
        }
        Node::Icon { kind, rgb, .. } => {
            icon::draw(*kind, c, x, y, me.w.min(me.h), *rgb);
        }
        Node::Gauge {
            permille,
            w: _,
            h: _,
            rgb,
        } => gauge(c, x, y, me, *permille, *rgb, s),
        Node::Bars { n, of, rgb, .. } => bars(c, x, y, me, *n, *of, *rgb),
        Node::Chip { text, bg, fg } => {
            c.rounded_rect(x, y, me.w, me.h, me.h / 2, *bg);
            let face = cx.fonts.face(s.font(cx.t.px(CHIP_PX)));
            let cols = ((me.w - 2 * s.p(CHIP_PAD.0)) / (face.cw as i32).max(1)).max(1) as usize;
            c.text_px(
                face,
                x + s.p(CHIP_PAD.0),
                y + s.p(CHIP_PAD.1),
                &ellipsize(text, cols),
                *fg,
                false,
            );
        }
        Node::Spacer(_) => {}
        Node::Rule { rgb } => {
            c.rounded_rect_alpha(x, y, avail.w, me.h, 0, *rgb, 255);
        }
        Node::Fixed { child, .. } => node(child, c, (x, y), me, cx),
        Node::Selected { ring, child } => {
            let t = cx.t;
            c.rounded_rect(x, y, me.w, me.h, s.p(SEL_R), t.select);
            if *ring {
                c.rounded_rect_ring(x, y, me.w, me.h, s.p(SEL_R), s.p(2).max(2), t.ring);
            }
            let (px, py) = (s.p(SEL_PAD.0), s.p(SEL_PAD.1));
            let inner = Size {
                w: me.w - 2 * px,
                h: me.h - 2 * py,
            };
            node(child, c, (x + px, y + py), inner, cx);
        }
        Node::Faded { level, child } => {
            let lv = (*level).min(FADE_STEPS) as u32;
            if lv == 0 || me.w <= 0 || me.h <= 0 {
                return;
            }
            let mut tmp = Canvas::transparent(me.w as usize, me.h as usize);
            node(child, &mut tmp, (0, 0), me, cx);
            let a = (255 * lv / FADE_STEPS as u32) as u8;
            c.blit(&tmp, x, y, a);
        }
    }
}

/// A battery: outline, nub at the right, the charge inside.
#[allow(clippy::too_many_arguments)]
fn gauge(c: &mut Canvas, x: i32, y: i32, me: Size, permille: u16, rgb: [u8; 3], s: Px) {
    let nub = (me.w / 10).max(s.p(2));
    let bw = me.w - nub;
    let r = (me.h / 4).max(1);
    let lw = s.p(2).max(1);
    c.rounded_rect_ring(x, y, bw, me.h, r, lw, rgb);
    c.rounded_rect(x + bw, y + me.h / 3, nub, me.h - 2 * (me.h / 3), 1, rgb);
    let inset = lw + s.p(1).max(1);
    let full = bw - 2 * inset;
    let w = (full * permille.min(1000) as i32 + 500) / 1000;
    if w > 0 {
        c.rounded_rect(
            x + inset,
            y + inset,
            w,
            me.h - 2 * inset,
            (r - inset).max(1),
            rgb,
        );
    }
}

/// `of` rising bars, the first `n` solid, the rest faint.
fn bars(c: &mut Canvas, x: i32, y: i32, me: Size, n: u8, of: u8, rgb: [u8; 3]) {
    let of = of.max(1) as i32;
    let gap = (me.w / (of * 4)).max(1);
    let bw = ((me.w - gap * (of - 1)) / of).max(1);
    for i in 0..of {
        let h = (me.h * (i + 1) / of).max(1);
        let bx = x + i * (bw + gap);
        let a = if i < n as i32 { 255 } else { 70 };
        c.rounded_rect_alpha(bx, y + me.h - h, bw, h, (bw / 3).max(1), rgb, a);
    }
}

/// Paints `n` at `at` inside `avail` (raster px) at scale `s`; nothing lands
/// outside the box.
#[allow(clippy::too_many_arguments)]
pub fn paint_at(
    n: &Node,
    c: &mut Canvas,
    at: (i32, i32),
    avail: Size,
    fonts: &mut Fonts,
    t: &Theme,
    s: Px,
) {
    if avail.w <= 0 || avail.h <= 0 {
        return;
    }
    let mut cx = Cx { fonts, t, s };
    let mut tmp = Canvas::transparent(avail.w as usize, avail.h as usize);
    node(n, &mut tmp, (0, 0), avail, &mut cx);
    c.blit(&tmp, at.0, at.1, 255);
}

pub fn paint(n: &Node, c: &mut Canvas, at: (i32, i32), avail: Size, fonts: &mut Fonts, t: &Theme) {
    paint_at(n, c, at, avail, fonts, t, Px(1.0));
}

/// A tree as a canvas of exactly its measured size at scale `s` (what the
/// overlay's cache rasterises for `Spec::Ui`).
pub fn render(n: &Node, avail: Size, fonts: &mut Fonts, t: &Theme, s: Px) -> Canvas {
    let mut cx = Cx { fonts, t, s };
    let me = layout::size(n, avail, &mut cx);
    let mut c = Canvas::transparent(me.w.max(1) as usize, me.h.max(1) as usize);
    node(n, &mut c, (0, 0), me, &mut cx);
    c
}
