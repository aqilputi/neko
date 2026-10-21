//! The niri strip: where each column sits, the office geometry, and where
//! the camera goes (office view, zoomed onto a monitor) with its easing.
//! Pure math (glam types only), unit-tested.

use bevy::math::{Vec2, Vec3};

/// Width of one column (desk + elbow room).
pub const COL_W: f32 = 3.4;
/// Extra gap between two contexts.
pub const CTX_GAP: f32 = 1.3;

// ---- office geometry (world units; y up; cats face -z, the camera is on +z)
pub const DESK_TOP: f32 = 0.78;
pub const DESK_W: f32 = 2.8;
pub const DESK_D: f32 = 1.15;
pub const DESK_Z: f32 = -0.25;
/// The monitor's screen (its glowing face).
pub const SCREEN_W: f32 = 2.4;
pub const SCREEN_H: f32 = 1.5;
pub const SCREEN_Y: f32 = 1.95;
pub const SCREEN_Z: f32 = -0.62;
pub const CHAIR_Z: f32 = 0.95;
pub const SEAT_Y: f32 = 0.5;
/// Where the cat sits (its root, on the seat).
pub const CAT_Z: f32 = 0.82;
/// The aisle in front of the chairs, where the inbox cats walk.
pub const AISLE_Z: f32 = 2.35;
/// Size of an inbox cat, and of the one being pointed at.
pub const WALKER_SCALE: f32 = 0.5;
pub const WALKER_HL: f32 = 0.62;

/// The least width (world units) the office view shows around the focused
/// column: itself and a bit of each neighbour.
pub const OFFICE_MIN_W: f32 = COL_W * 1.7;

/// Vertical field of view of the camera (radians).
pub const FOV: f32 = 0.66;

/// x centre of every column: COL_W apart, CTX_GAP more where the context
/// changes; the strip starts at x = 0.
pub fn column_xs(ctxs: &[&str]) -> Vec<f32> {
    let mut out = Vec::with_capacity(ctxs.len());
    let mut x = 0.0;
    for (i, c) in ctxs.iter().enumerate() {
        if i > 0 {
            x += COL_W;
            if ctxs[i - 1] != *c {
                x += CTX_GAP;
            }
        }
        out.push(x);
    }
    out
}

/// A run of columns of one context (its rug and label).
#[derive(Clone, Debug, PartialEq)]
pub struct CtxSpan {
    pub ctx: String,
    pub first: usize,
    pub last: usize,
    /// left and right edge of the rug
    pub x0: f32,
    pub x1: f32,
}

pub fn ctx_spans(ctxs: &[&str], xs: &[f32]) -> Vec<CtxSpan> {
    let mut out: Vec<CtxSpan> = Vec::new();
    for (i, c) in ctxs.iter().enumerate() {
        match out.last_mut() {
            Some(s) if s.ctx == *c => {
                s.last = i;
                s.x1 = xs[i] + COL_W / 2.0 - 0.1;
            }
            _ => out.push(CtxSpan {
                ctx: c.to_string(),
                first: i,
                last: i,
                x0: xs[i] - COL_W / 2.0 + 0.1,
                x1: xs[i] + COL_W / 2.0 - 0.1,
            }),
        }
    }
    out
}

/// Index of the column next to `from` in the next (dir > 0) or previous
/// context: the first column of that context.
pub fn ctx_jump(ctxs: &[&str], from: usize, dir: i32) -> usize {
    if ctxs.is_empty() {
        return 0;
    }
    let from = from.min(ctxs.len() - 1);
    // start of the current run
    let mut start = from;
    while start > 0 && ctxs[start - 1] == ctxs[from] {
        start -= 1;
    }
    if dir > 0 {
        let mut i = from;
        while i < ctxs.len() && ctxs[i] == ctxs[from] {
            i += 1;
        }
        if i >= ctxs.len() { from } else { i }
    } else if from != start {
        start
    } else if start == 0 {
        0
    } else {
        let mut i = start - 1;
        while i > 0 && ctxs[i - 1] == ctxs[start - 1] {
            i -= 1;
        }
        i
    }
}

/// Nearest column to a world x.
#[cfg(test)]
pub fn nearest(xs: &[f32], x: f32) -> usize {
    let mut best = 0;
    for (i, cx) in xs.iter().enumerate() {
        if (cx - x).abs() < (xs[best] - x).abs() {
            best = i;
        }
    }
    best
}

/// A camera placement: eye and the point it looks at.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CamPose {
    pub eye: Vec3,
    pub look: Vec3,
}

impl CamPose {
    pub fn lerp(self, o: CamPose, u: f32) -> CamPose {
        CamPose {
            eye: self.eye.lerp(o.eye, u),
            look: self.look.lerp(o.look, u),
        }
    }
}

/// The office view: behind the cat, a bit above, looking at its monitor;
/// the focused column centred, neighbours partly visible.
/// A narrow window backs the camera off (same line of sight) until at least
/// OFFICE_MIN_W is visible at the look point.
pub fn office_pose(x: f32, aspect: f32) -> CamPose {
    let look = Vec3::new(x, 1.42, -0.5);
    let eye = Vec3::new(x, 2.75, 6.9);
    let d = (eye - look).length();
    let need = OFFICE_MIN_W / 2.0 / ((FOV / 2.0).tan() * aspect.max(0.1));
    if need <= d {
        return CamPose { eye, look };
    }
    CamPose {
        eye: look + (eye - look) * (need / d),
        look,
    }
}

/// Zoomed: straight in front of the screen, close enough that the screen
/// fills `fill` of the viewport (aspect = width / height of the viewport).
pub fn zoom_pose(x: f32, aspect: f32, fill: f32) -> CamPose {
    let t = (FOV / 2.0).tan();
    let d_h = SCREEN_H / fill / 2.0 / t;
    let d_w = SCREEN_W / fill / 2.0 / (t * aspect.max(0.1));
    let d = d_h.max(d_w);
    CamPose {
        eye: Vec3::new(x, SCREEN_Y, SCREEN_Z + d),
        look: Vec3::new(x, SCREEN_Y, SCREEN_Z),
    }
}

/// The four corners of a column's screen (world), for projecting the
/// zoomed terminal: top-left, top-right, bottom-right, bottom-left.
pub fn screen_corners(x: f32) -> [Vec3; 4] {
    let (hw, hh, z) = (SCREEN_W / 2.0, SCREEN_H / 2.0, SCREEN_Z + 0.001);
    [
        Vec3::new(x - hw, SCREEN_Y + hh, z),
        Vec3::new(x + hw, SCREEN_Y + hh, z),
        Vec3::new(x + hw, SCREEN_Y - hh, z),
        Vec3::new(x - hw, SCREEN_Y - hh, z),
    ]
}

/// Axis-aligned rect (min, max) of projected points.
pub fn bounds(pts: &[Vec2]) -> (Vec2, Vec2) {
    let mut lo = Vec2::splat(f32::MAX);
    let mut hi = Vec2::splat(f32::MIN);
    for p in pts {
        lo = lo.min(*p);
        hi = hi.max(*p);
    }
    (lo, hi)
}

/// Terminal size that fits a rect of `px` (physical pixels) with cells of
/// `cell` (physical pixels); at least 20×5.
pub fn fit_cells(px: Vec2, cell: Vec2) -> (u16, u16) {
    let cols = (px.x / cell.x).floor().clamp(20.0, 500.0) as u16;
    let rows = (px.y / cell.y).floor().clamp(5.0, 300.0) as u16;
    (cols, rows)
}

pub fn ease_in_out(u: f32) -> f32 {
    let k = u.clamp(0.0, 1.0);
    if k < 0.5 {
        4.0 * k * k * k
    } else {
        1.0 - (-2.0 * k + 2.0).powi(3) / 2.0
    }
}

/// A camera tween: from → to over `dur` seconds, starting at `t0`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tween {
    pub from: CamPose,
    pub to: CamPose,
    pub t0: f64,
    pub dur: f64,
}

impl Tween {
    pub fn still(p: CamPose) -> Tween {
        Tween {
            from: p,
            to: p,
            t0: 0.0,
            dur: 0.0,
        }
    }
    pub fn at(&self, t: f64) -> CamPose {
        if self.dur <= 0.0 || t >= self.t0 + self.dur {
            return self.to;
        }
        let u = ((t - self.t0) / self.dur).max(0.0) as f32;
        self.from.lerp(self.to, ease_in_out(u))
    }
    pub fn done(&self, t: f64) -> bool {
        self.dur <= 0.0 || t >= self.t0 + self.dur
    }
    /// Retarget from wherever the camera is now.
    pub fn retarget(&self, t: f64, to: CamPose, dur: f64) -> Tween {
        if to == self.to {
            return *self;
        }
        Tween {
            from: self.at(t),
            to,
            t0: t,
            dur,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xs_with_context_gaps() {
        let xs = column_xs(&["a", "a", "b", "c", "c"]);
        assert_eq!(xs[0], 0.0);
        assert_eq!(xs[1], COL_W);
        assert!((xs[2] - (2.0 * COL_W + CTX_GAP)).abs() < 1e-5);
        assert!((xs[3] - (3.0 * COL_W + 2.0 * CTX_GAP)).abs() < 1e-5);
        assert!(column_xs(&[]).is_empty());
    }

    #[test]
    fn spans() {
        let c = ["a", "a", "b"];
        let xs = column_xs(&c);
        let s = ctx_spans(&c, &xs);
        assert_eq!(s.len(), 2);
        assert_eq!((s[0].first, s[0].last), (0, 1));
        assert!(s[0].x1 > xs[1] && s[1].x0 > s[0].x1);
    }

    #[test]
    fn jumps_between_contexts() {
        let c = ["a", "a", "b", "b", "c"];
        assert_eq!(ctx_jump(&c, 0, 1), 2);
        assert_eq!(ctx_jump(&c, 3, 1), 4);
        assert_eq!(ctx_jump(&c, 4, 1), 4);
        assert_eq!(ctx_jump(&c, 3, -1), 2);
        assert_eq!(ctx_jump(&c, 2, -1), 0);
        assert_eq!(ctx_jump(&c, 4, -1), 2);
        assert_eq!(ctx_jump(&c, 0, -1), 0);
        assert_eq!(ctx_jump(&[], 0, 1), 0);
    }

    #[test]
    fn nearest_column() {
        let xs = [0.0, 3.0, 7.0];
        assert_eq!(nearest(&xs, 4.9), 1);
        assert_eq!(nearest(&xs, 5.1), 2);
        assert_eq!(nearest(&xs, -9.0), 0);
    }

    #[test]
    fn zoom_fills_the_view() {
        let p = zoom_pose(2.0, 16.0 / 9.0, 0.9);
        let d = p.eye.z - SCREEN_Z;
        let visible_h = 2.0 * d * (FOV / 2.0).tan();
        assert!(
            (SCREEN_H / visible_h - 0.9).abs() < 1e-4,
            "height-bound on a wide window"
        );
        // a narrow window is width-bound
        let p = zoom_pose(0.0, 1.0, 0.9);
        let d = p.eye.z - SCREEN_Z;
        let visible_w = 2.0 * d * (FOV / 2.0).tan();
        assert!((SCREEN_W / visible_w - 0.9).abs() < 1e-4);
        assert_eq!(p.look.y, SCREEN_Y);
    }

    #[test]
    fn office_pose_backs_off_in_a_narrow_window() {
        // wide: the classic pose
        assert_eq!(
            office_pose(2.0, 16.0 / 9.0),
            CamPose {
                eye: Vec3::new(2.0, 2.75, 6.9),
                look: Vec3::new(2.0, 1.42, -0.5)
            }
        );
        // narrow (portrait tile): the camera pulls back along the same line of
        // sight until OFFICE_MIN_W is visible at the look point
        let p = office_pose(0.0, 0.9);
        let d = (p.eye - p.look).length();
        let visible_w = 2.0 * d * (FOV / 2.0).tan() * 0.9;
        assert!(
            (visible_w - OFFICE_MIN_W).abs() < 1e-3,
            "visible_w={visible_w}"
        );
        let dir0 = (office_pose(0.0, 2.0).eye - office_pose(0.0, 2.0).look).normalize();
        assert!(((p.eye - p.look).normalize() - dir0).length() < 1e-5);
    }

    #[test]
    fn tween_eases_and_retargets() {
        let a = office_pose(0.0, 1.6);
        let b = office_pose(10.0, 1.6);
        let tw = Tween {
            from: a,
            to: b,
            t0: 1.0,
            dur: 0.5,
        };
        assert_eq!(tw.at(0.0), a);
        assert_eq!(tw.at(2.0), b);
        let mid = tw.at(1.25);
        assert!((mid.eye.x - 5.0).abs() < 1e-4);
        assert!(!tw.done(1.2) && tw.done(1.5));
        let r = tw.retarget(1.25, office_pose(-4.0, 1.6), 0.5);
        assert_eq!(r.from, mid);
        assert_eq!(tw.retarget(1.25, b, 0.5), tw);
        assert_eq!(ease_in_out(0.0), 0.0);
        assert_eq!(ease_in_out(1.0), 1.0);
    }

    #[test]
    fn cells_fit() {
        assert_eq!(
            fit_cells(Vec2::new(900.0, 400.0), Vec2::new(9.0, 20.0)),
            (100, 20)
        );
        assert_eq!(
            fit_cells(Vec2::new(10.0, 10.0), Vec2::new(9.0, 20.0)),
            (20, 5)
        );
        let (lo, hi) = bounds(&[Vec2::new(3.0, 1.0), Vec2::new(-1.0, 5.0)]);
        assert_eq!((lo, hi), (Vec2::new(-1.0, 1.0), Vec2::new(3.0, 5.0)));
    }
}
