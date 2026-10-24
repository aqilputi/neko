//! Tiny vector icons: each is at most [`MAX_PRIMS`] primitives (disc, ring,
//! rect, line, tri and their punches) on a unit square, drawn in one colour
//! at any size (16/24/32 px are the grid sizes; anything works).

use nk_cats::raster::Canvas;

use super::{Icon, Rgb};

/// The most primitives an icon may use (a test counts them).
pub const MAX_PRIMS: usize = 12;

/// Every icon, for tests and the help page.
pub const ALL: [Icon; 27] = [
    Icon::Cat,
    Icon::Envelope,
    Icon::Map,
    Icon::Grid,
    Icon::Window,
    Icon::Note,
    Icon::Book,
    Icon::Lock,
    Icon::LockOpen,
    Icon::Gear,
    Icon::Question,
    Icon::Battery,
    Icon::Plug,
    Icon::Wifi,
    Icon::Clock,
    Icon::Play,
    Icon::Pause,
    Icon::Check,
    Icon::Dot,
    Icon::Arrow,
    Icon::Search,
    Icon::Home,
    Icon::Phone,
    Icon::Terminal,
    Icon::X11,
    Icon::Warning,
    Icon::Info,
];

/// One primitive on the unit square (y down).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Prim {
    Disc(f32, f32, f32),
    /// centre, radius to the middle of the stroke, stroke width
    Ring(f32, f32, f32, f32),
    /// x, y, w, h, corner radius
    Rect(f32, f32, f32, f32, f32),
    /// x, y, w, h, corner radius, stroke width
    RectRing(f32, f32, f32, f32, f32, f32),
    Line(f32, f32, f32, f32, f32),
    Tri((f32, f32), (f32, f32), (f32, f32)),
    PunchDisc(f32, f32, f32),
    PunchRect(f32, f32, f32, f32),
    PunchLine(f32, f32, f32, f32, f32),
    PunchTri((f32, f32), (f32, f32), (f32, f32)),
}

/// The stroke most icons use.
const W: f32 = 0.11;

/// The primitives of `kind`.
pub fn prims(kind: Icon) -> Vec<Prim> {
    use Prim::*;
    match kind {
        Icon::Cat => vec![
            Disc(0.5, 0.58, 0.33),
            Tri((0.19, 0.48), (0.22, 0.1), (0.46, 0.3)),
            Tri((0.81, 0.48), (0.78, 0.1), (0.54, 0.3)),
            PunchDisc(0.38, 0.56, 0.055),
            PunchDisc(0.62, 0.56, 0.055),
            PunchTri((0.46, 0.68), (0.54, 0.68), (0.5, 0.73)),
        ],
        Icon::Envelope => vec![
            RectRing(0.1, 0.22, 0.8, 0.56, 0.1, W),
            Line(0.16, 0.3, 0.5, 0.56, W),
            Line(0.84, 0.3, 0.5, 0.56, W),
        ],
        Icon::Map => vec![
            Tri((0.12, 0.2), (0.38, 0.12), (0.38, 0.8)),
            Tri((0.12, 0.2), (0.38, 0.8), (0.12, 0.88)),
            Tri((0.4, 0.12), (0.62, 0.2), (0.62, 0.88)),
            Tri((0.4, 0.12), (0.62, 0.88), (0.4, 0.8)),
            Tri((0.64, 0.2), (0.88, 0.12), (0.88, 0.8)),
            Tri((0.64, 0.2), (0.88, 0.8), (0.64, 0.88)),
            PunchDisc(0.51, 0.42, 0.07),
        ],
        Icon::Grid => vec![
            Rect(0.14, 0.14, 0.3, 0.3, 0.08),
            Rect(0.56, 0.14, 0.3, 0.3, 0.08),
            Rect(0.14, 0.56, 0.3, 0.3, 0.08),
            Rect(0.56, 0.56, 0.3, 0.3, 0.08),
        ],
        Icon::Window => vec![
            RectRing(0.1, 0.16, 0.8, 0.68, 0.1, W),
            Rect(0.1, 0.16, 0.8, 0.18, 0.08),
        ],
        Icon::Note => vec![
            Disc(0.32, 0.74, 0.14),
            Line(0.44, 0.74, 0.44, 0.16, W),
            Line(0.44, 0.16, 0.74, 0.28, W),
            Line(0.74, 0.28, 0.74, 0.4, W * 0.8),
        ],
        Icon::Book => vec![
            Rect(0.18, 0.12, 0.62, 0.76, 0.08),
            PunchRect(0.3, 0.12, 0.05, 0.76),
            PunchLine(0.44, 0.3, 0.68, 0.3, 0.07),
            PunchRect(0.3, 0.76, 0.5, 0.06),
        ],
        Icon::Lock => vec![
            Ring(0.5, 0.4, 0.18, W * 1.1),
            Rect(0.2, 0.42, 0.6, 0.46, 0.1),
            PunchDisc(0.5, 0.6, 0.07),
            PunchRect(0.47, 0.6, 0.06, 0.15),
        ],
        Icon::LockOpen => vec![
            Ring(0.62, 0.3, 0.17, W * 1.1),
            PunchRect(0.3, 0.3, 0.3, 0.14),
            Rect(0.16, 0.44, 0.6, 0.44, 0.1),
            PunchDisc(0.46, 0.62, 0.07),
        ],
        Icon::Gear => vec![
            Line(0.5, 0.1, 0.5, 0.9, 0.18),
            Line(0.1, 0.5, 0.9, 0.5, 0.18),
            Line(0.22, 0.22, 0.78, 0.78, 0.18),
            Line(0.78, 0.22, 0.22, 0.78, 0.18),
            Disc(0.5, 0.5, 0.3),
            PunchDisc(0.5, 0.5, 0.12),
        ],
        Icon::Question => vec![
            Ring(0.5, 0.34, 0.2, W * 1.2),
            PunchRect(0.2, 0.36, 0.3, 0.3),
            Line(0.6, 0.5, 0.5, 0.6, W * 1.2),
            Line(0.5, 0.6, 0.5, 0.66, W * 1.2),
            Disc(0.5, 0.84, 0.07),
        ],
        Icon::Battery => vec![
            RectRing(0.08, 0.28, 0.76, 0.44, 0.1, W),
            Rect(0.84, 0.4, 0.08, 0.2, 0.03),
            Rect(0.18, 0.38, 0.36, 0.24, 0.04),
        ],
        Icon::Plug => vec![
            Line(0.38, 0.1, 0.38, 0.3, W),
            Line(0.62, 0.1, 0.62, 0.3, W),
            Rect(0.24, 0.3, 0.52, 0.3, 0.12),
            Line(0.5, 0.6, 0.5, 0.9, W),
        ],
        Icon::Wifi => vec![
            Ring(0.5, 0.82, 0.62, W),
            Ring(0.5, 0.82, 0.42, W),
            Ring(0.5, 0.82, 0.22, W),
            PunchTri((0.5, 0.82), (-0.3, 0.0), (-0.3, 1.0)),
            PunchTri((0.5, 0.82), (1.3, 0.0), (1.3, 1.0)),
            PunchRect(0.0, 0.84, 1.0, 0.16),
            Disc(0.5, 0.8, 0.08),
        ],
        Icon::Clock => vec![
            Ring(0.5, 0.5, 0.36, W),
            Line(0.5, 0.5, 0.5, 0.28, W),
            Line(0.5, 0.5, 0.66, 0.58, W),
        ],
        Icon::Play => vec![Tri((0.28, 0.16), (0.82, 0.5), (0.28, 0.84))],
        Icon::Pause => vec![
            Rect(0.24, 0.18, 0.18, 0.64, 0.06),
            Rect(0.58, 0.18, 0.18, 0.64, 0.06),
        ],
        Icon::Check => vec![
            Line(0.18, 0.52, 0.42, 0.74, W * 1.3),
            Line(0.42, 0.74, 0.84, 0.28, W * 1.3),
        ],
        Icon::Dot => vec![Disc(0.5, 0.5, 0.2)],
        Icon::Arrow => vec![
            Line(0.2, 0.5, 0.82, 0.5, W * 1.2),
            Line(0.2, 0.5, 0.46, 0.24, W * 1.2),
            Line(0.2, 0.5, 0.46, 0.76, W * 1.2),
        ],
        Icon::Search => vec![
            Ring(0.42, 0.42, 0.24, W * 1.1),
            Line(0.6, 0.6, 0.84, 0.84, W * 1.4),
        ],
        Icon::Home => vec![
            Tri((0.08, 0.5), (0.5, 0.12), (0.92, 0.5)),
            Rect(0.2, 0.46, 0.6, 0.42, 0.06),
            PunchRect(0.42, 0.62, 0.16, 0.26),
        ],
        Icon::Phone => vec![
            RectRing(0.26, 0.08, 0.48, 0.84, 0.12, W),
            Disc(0.5, 0.78, 0.05),
        ],
        Icon::Terminal => vec![
            Rect(0.08, 0.16, 0.84, 0.68, 0.12),
            PunchLine(0.24, 0.36, 0.38, 0.5, W),
            PunchLine(0.38, 0.5, 0.24, 0.64, W),
            PunchLine(0.48, 0.64, 0.7, 0.64, W),
        ],
        Icon::X11 => vec![
            Line(0.2, 0.18, 0.8, 0.82, W * 1.6),
            Line(0.8, 0.18, 0.2, 0.82, W * 1.6),
        ],
        Icon::Warning => vec![
            Tri((0.5, 0.08), (0.94, 0.88), (0.06, 0.88)),
            PunchLine(0.5, 0.36, 0.5, 0.6, W * 1.1),
            PunchDisc(0.5, 0.74, 0.065),
        ],
        Icon::Info => vec![
            Disc(0.5, 0.5, 0.42),
            PunchDisc(0.5, 0.3, 0.07),
            PunchLine(0.5, 0.46, 0.5, 0.72, W * 1.1),
        ],
    }
}

/// Draws `kind` in the `sz`×`sz` box at (x, y), raster px.
pub fn draw(kind: Icon, c: &mut Canvas, x: i32, y: i32, sz: i32, rgb: Rgb) {
    if sz <= 0 {
        return;
    }
    // on its own canvas: punches cut only the icon
    let mut t = Canvas::transparent(sz as usize, sz as usize);
    let k = sz as f32;
    let u = |v: f32| v * k;
    let wpx = |w: f32| (w * k).max(1.4);
    for p in prims(kind) {
        match p {
            Prim::Disc(cx, cy, r) => t.disc(u(cx), u(cy), u(r), rgb),
            Prim::Ring(cx, cy, r, w) => t.ring(u(cx), u(cy), u(r), wpx(w), rgb),
            Prim::Rect(rx, ry, rw, rh, r) => {
                t.rounded_rect_f(u(rx), u(ry), u(rw), u(rh), u(r), 0.0, rgb)
            }
            Prim::RectRing(rx, ry, rw, rh, r, w) => {
                t.rounded_rect_f(u(rx), u(ry), u(rw), u(rh), u(r), wpx(w), rgb)
            }
            Prim::Line(x0, y0, x1, y1, w) => t.line(u(x0), u(y0), u(x1), u(y1), wpx(w), rgb),
            Prim::Tri(a, b, d) => t.tri((u(a.0), u(a.1)), (u(b.0), u(b.1)), (u(d.0), u(d.1)), rgb),
            Prim::PunchDisc(cx, cy, r) => t.punch_disc(u(cx), u(cy), u(r)),
            Prim::PunchRect(rx, ry, rw, rh) => t.punch_rect(u(rx), u(ry), u(rw), u(rh)),
            Prim::PunchLine(x0, y0, x1, y1, w) => t.punch_line(u(x0), u(y0), u(x1), u(y1), wpx(w)),
            Prim::PunchTri(a, b, d) => {
                t.punch_tri((u(a.0), u(a.1)), (u(b.0), u(b.1)), (u(d.0), u(d.1)))
            }
        }
    }
    c.blit(&t, x, y, 255);
}
