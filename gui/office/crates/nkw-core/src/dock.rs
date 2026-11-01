//! Anchored screens ("docks", plan §2.7): their rects on the output, in
//! logical px, off the HUD's reserved rects; and where FOCO centres a
//! window beside them.

use glam::Vec2;
use serde::{Deserialize, Serialize};

use crate::ids::ScreenId;

/// A dock's slot on the output.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DockSlot {
    TopLeft,
    Top,
    TopRight,
    Right,
    BottomRight,
    Bottom,
    BottomLeft,
    Left,
}

pub const DOCK_MAX: usize = 4;
pub const DOCK_SCALE: f32 = 0.33;
pub const DOCK_MARGIN: i32 = 16;
pub const DOCK_EDGE_PX: i32 = 48;
pub const DOCK_MIN_SCALE: f32 = 0.25;

/// A rectangle, logical px (y down).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub fn new(x: i32, y: i32, w: i32, h: i32) -> Rect {
        Rect { x, y, w, h }
    }

    pub fn contains(&self, x: f64, y: f64) -> bool {
        x >= self.x as f64
            && y >= self.y as f64
            && x < (self.x + self.w) as f64
            && y < (self.y + self.h) as f64
    }

    pub fn intersects(&self, o: &Rect) -> bool {
        self.x < o.x + o.w && o.x < self.x + self.w && self.y < o.y + o.h && o.y < self.y + self.h
    }

    pub fn area(&self) -> i64 {
        self.w.max(0) as i64 * self.h.max(0) as i64
    }

    pub fn centre(&self) -> (f32, f32) {
        (
            self.x as f32 + self.w as f32 / 2.0,
            self.y as f32 + self.h as f32 / 2.0,
        )
    }
}

/// One docked screen, for [`layout`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DockIn {
    pub id: ScreenId,
    pub slot: DockSlot,
    /// the window's logical size
    pub logical: Vec2,
    pub scale: f32,
}

/// Which output edges a slot hugs: (top, bottom, left, right).
fn slot_edges(slot: DockSlot) -> (bool, bool, bool, bool) {
    match slot {
        DockSlot::TopLeft => (true, false, true, false),
        DockSlot::Top => (true, false, false, false),
        DockSlot::TopRight => (true, false, false, true),
        DockSlot::Right => (false, false, false, true),
        DockSlot::BottomRight => (false, true, false, true),
        DockSlot::Bottom => (false, true, false, false),
        DockSlot::BottomLeft => (false, true, true, false),
        DockSlot::Left => (false, false, true, false),
    }
}

/// The half/quarter of `out` a 1× dock of this slot shares the output by
/// (plan §2.7): corners get a quarter, a plain edge gets its half.
fn slot_region(slot: DockSlot, out: [i32; 2]) -> Rect {
    let (w, h) = (out[0], out[1]);
    let (lw, th) = (w / 2, h / 2);
    let (rw, bh) = (w - lw, h - th);
    match slot {
        DockSlot::TopLeft => Rect::new(0, 0, lw, th),
        DockSlot::Top => Rect::new(0, 0, w, th),
        DockSlot::TopRight => Rect::new(lw, 0, rw, th),
        DockSlot::Right => Rect::new(lw, 0, rw, h),
        DockSlot::BottomRight => Rect::new(lw, th, rw, bh),
        DockSlot::Bottom => Rect::new(0, th, w, bh),
        DockSlot::BottomLeft => Rect::new(0, th, lw, bh),
        DockSlot::Left => Rect::new(0, 0, lw, h),
    }
}

/// `d`'s rect, or `None` when it does not fit even at DOCK_MIN_SCALE.
fn dock_rect(out: [i32; 2], d: DockIn, reserved: &[Rect]) -> Option<Rect> {
    // below 1× it floats anywhere in its corner/edge; at 1× it shares the
    // output with the other docks by slot
    let region = if d.scale >= 1.0 {
        slot_region(d.slot, out)
    } else {
        Rect::new(0, 0, out[0], out[1])
    };
    if region.w <= 0 || region.h <= 0 || d.logical.x <= 0.0 || d.logical.y <= 0.0 {
        return None;
    }
    let fit_w = (region.w as f32 - 2.0 * DOCK_MARGIN as f32) / d.logical.x;
    let fit_h = (region.h as f32 - 2.0 * DOCK_MARGIN as f32) / d.logical.y;
    let eff_scale = d.scale.min(fit_w).min(fit_h);
    if eff_scale < DOCK_MIN_SCALE {
        return None;
    }
    let size = d.logical * eff_scale;
    let (w, h) = (size.x.round() as i32, size.y.round() as i32);
    let (top, bottom, left, right) = slot_edges(d.slot);
    let mut x = if left {
        region.x + DOCK_MARGIN
    } else if right {
        region.x + region.w - w - DOCK_MARGIN
    } else {
        region.x + (region.w - w) / 2
    };
    let mut y = if top {
        region.y + DOCK_MARGIN
    } else if bottom {
        region.y + region.h - h - DOCK_MARGIN
    } else {
        region.y + (region.h - h) / 2
    };
    // move inward/down off whatever it would otherwise overlap: the HUD
    // (always in `reserved`) keeps its corner.
    for r in reserved {
        if !Rect::new(x, y, w, h).intersects(r) {
            continue;
        }
        if left {
            x = x.max(r.x + r.w + DOCK_MARGIN);
        } else if right {
            x = x.min(r.x - w - DOCK_MARGIN);
        }
        if top {
            y = y.max(r.y + r.h + DOCK_MARGIN);
        } else if bottom {
            y = y.min(r.y - h - DOCK_MARGIN);
        }
    }
    let rect = Rect::new(x, y, w, h);
    let out_rect = Rect::new(0, 0, out[0], out[1]);
    if rect.x < out_rect.x
        || rect.y < out_rect.y
        || rect.x + rect.w > out_rect.x + out_rect.w
        || rect.y + rect.h > out_rect.y + out_rect.h
    {
        return None;
    }
    Some(rect)
}

/// Every dock's rect on an output of `out` logical px: below 1× a
/// picture-in-picture in its slot; at 1× the window at 1:1, 1× docks
/// sharing the output by slot (left/right halves, top/bottom halves,
/// corners quarters); moved inward/down off every `reserved` rect (the HUD
/// wins its corner); shrunk down to DOCK_MIN_SCALE to fit; `None` = does
/// not fit (the caller keeps it docked, hidden, and toasts once).
pub fn layout(out: [i32; 2], docks: &[DockIn], reserved: &[Rect]) -> Vec<(ScreenId, Option<Rect>)> {
    docks
        .iter()
        .map(|d| (d.id, dock_rect(out, *d, reserved)))
        .collect()
}

/// The slot within DOCK_EDGE_PX of an edge/corner under (x, y).
pub fn slot_at(out: [i32; 2], x: f64, y: f64) -> Option<DockSlot> {
    let edge = DOCK_EDGE_PX as f64;
    let near_top = y <= edge;
    let near_bottom = y >= out[1] as f64 - edge;
    let near_left = x <= edge;
    let near_right = x >= out[0] as f64 - edge;
    match (near_top, near_bottom, near_left, near_right) {
        (true, _, true, _) => Some(DockSlot::TopLeft),
        (true, _, _, true) => Some(DockSlot::TopRight),
        (_, true, true, _) => Some(DockSlot::BottomLeft),
        (_, true, _, true) => Some(DockSlot::BottomRight),
        (true, _, _, _) => Some(DockSlot::Top),
        (_, true, _, _) => Some(DockSlot::Bottom),
        (_, _, true, _) => Some(DockSlot::Left),
        (_, _, _, true) => Some(DockSlot::Right),
        _ => None,
    }
}

/// Where FOCO goes with docks present.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DockFocus {
    Beside,
    Over,
    #[default]
    Auto,
}

/// Where FOCO centres a window of `logical` px given the dock rects: the
/// centre of the largest free area when the window fits there at 1:1
/// (`Beside`, or `Auto` when it fits), else the output centre (`Over`).
pub fn focus_centre(out: [i32; 2], docks: &[Rect], logical: Vec2, policy: DockFocus) -> (f32, f32) {
    let full_centre = (out[0] as f32 / 2.0, out[1] as f32 / 2.0);
    if matches!(policy, DockFocus::Over) {
        return full_centre;
    }
    // the inset left after every dock's own margin from its nearer edge: a
    // rect spanning (nearly) the full height is a vertical-edge dock
    // (left/right) and only eats into that margin, a full-width one only
    // the horizontal; a corner/PiP box eats into both.
    let (mut left, mut right, mut top, mut bottom) = (0, 0, 0, 0);
    for d in docks {
        // a vertical-edge dock (left/right) spans (nearly) the full
        // height and only eats into the horizontal margin; a
        // horizontal-edge one (top/bottom) the other way; a corner/PiP
        // box (neither) eats into both.
        let vertical_edge = d.h as f32 >= out[1] as f32 * 0.9 && (d.w as f32) < out[0] as f32 * 0.9;
        let horizontal_edge =
            d.w as f32 >= out[0] as f32 * 0.9 && (d.h as f32) < out[1] as f32 * 0.9;
        if !horizontal_edge {
            if d.x + d.w / 2 < out[0] / 2 {
                left = left.max(d.x + d.w);
            } else {
                right = right.max(out[0] - d.x);
            }
        }
        if !vertical_edge {
            if d.y + d.h / 2 < out[1] / 2 {
                top = top.max(d.y + d.h);
            } else {
                bottom = bottom.max(out[1] - d.y);
            }
        }
    }
    let free = Rect::new(
        left,
        top,
        (out[0] - left - right).max(0),
        (out[1] - top - bottom).max(0),
    );
    let fits = logical.x <= free.w as f32 && logical.y <= free.h as f32;
    match policy {
        DockFocus::Beside => free.centre(),
        DockFocus::Auto if fits => free.centre(),
        _ => full_centre,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_helpers() {
        let a = Rect::new(10, 20, 100, 50);
        assert!(a.contains(10.0, 20.0));
        assert!(a.contains(109.9, 69.9));
        assert!(!a.contains(110.0, 69.9), "exclusive on the far edge");
        assert!(!a.contains(9.9, 20.0));
        assert_eq!(a.area(), 5000);
        assert_eq!(a.centre(), (60.0, 45.0));
        assert!(a.intersects(&Rect::new(50, 20, 10, 10)));
        assert!(!a.intersects(&Rect::new(200, 20, 10, 10)));
        assert!(
            !a.intersects(&Rect::new(10, 100, 10, 10)),
            "touching, not overlapping"
        );
    }

    #[test]
    fn slot_at_table() {
        let out = [1000, 800];
        let rows: [(f64, f64, Option<DockSlot>); 9] = [
            (10.0, 10.0, Some(DockSlot::TopLeft)),
            (500.0, 10.0, Some(DockSlot::Top)),
            (990.0, 10.0, Some(DockSlot::TopRight)),
            (990.0, 790.0, Some(DockSlot::BottomRight)),
            (10.0, 790.0, Some(DockSlot::BottomLeft)),
            (500.0, 790.0, Some(DockSlot::Bottom)),
            (10.0, 400.0, Some(DockSlot::Left)),
            (990.0, 400.0, Some(DockSlot::Right)),
            (500.0, 400.0, None),
        ];
        for (x, y, want) in rows {
            assert_eq!(slot_at(out, x, y), want, "({x}, {y})");
        }
        // exactly at the edge tolerance: still counts
        assert_eq!(
            slot_at(out, DOCK_EDGE_PX as f64, 400.0),
            Some(DockSlot::Left)
        );
        assert_eq!(
            slot_at(out, DOCK_EDGE_PX as f64 + 1.0, 400.0),
            None,
            "just past it"
        );
    }

    fn dock(id: u32, slot: DockSlot, logical: Vec2, scale: f32) -> DockIn {
        DockIn {
            id: ScreenId(id),
            slot,
            logical,
            scale,
        }
    }

    #[test]
    fn a_picture_in_picture_sits_in_its_corner_with_the_margin() {
        let out = [1920, 1080];
        let d = dock(1, DockSlot::TopLeft, Vec2::new(640.0, 360.0), DOCK_SCALE);
        let got = layout(out, &[d], &[]);
        let want = Rect::new(
            DOCK_MARGIN,
            DOCK_MARGIN,
            (640.0 * DOCK_SCALE).round() as i32,
            (360.0 * DOCK_SCALE).round() as i32,
        );
        assert_eq!(got, vec![(ScreenId(1), Some(want))]);
    }

    #[test]
    fn a_1x_dock_takes_its_half_of_the_output() {
        let out = [1920, 1080];
        let d = dock(2, DockSlot::Left, Vec2::new(800.0, 1000.0), 1.0);
        let got = layout(out, &[d], &[]);
        // the left half is 0..960; the window (800×1000) fits at 1:1,
        // flush against the left edge, vertically centred
        let want = Rect::new(DOCK_MARGIN, (1080 - 1000) / 2, 800, 1000);
        assert_eq!(got, vec![(ScreenId(2), Some(want))]);
    }

    #[test]
    fn a_1x_corner_dock_takes_its_quarter() {
        let out = [1920, 1080];
        let d = dock(3, DockSlot::TopRight, Vec2::new(400.0, 300.0), 1.0);
        let got = layout(out, &[d], &[]);
        // top-right quarter is x 960..1920, y 0..540
        let want = Rect::new(1920 - 400 - DOCK_MARGIN, DOCK_MARGIN, 400, 300);
        assert_eq!(got, vec![(ScreenId(3), Some(want))]);
    }

    #[test]
    fn it_shrinks_towards_dock_min_scale_to_fit_a_small_output() {
        let out = [300, 200];
        let d = dock(4, DockSlot::TopLeft, Vec2::new(640.0, 360.0), 0.5);
        let got = layout(out, &[d], &[]);
        let (_, rect) = got[0];
        let rect = rect.expect("still fits, just shrunk");
        // the width is the tighter constraint: (300 - 32) / 640
        let eff = (300.0 - 2.0 * DOCK_MARGIN as f32) / 640.0;
        assert!(eff < 0.5 && eff > DOCK_MIN_SCALE, "{eff}");
        assert_eq!(rect.w, (640.0 * eff).round() as i32);
        assert_eq!(rect.h, (360.0 * eff).round() as i32);
    }

    #[test]
    fn it_gives_up_below_dock_min_scale() {
        let out = [50, 50];
        let d = dock(5, DockSlot::TopLeft, Vec2::new(640.0, 360.0), DOCK_SCALE);
        let got = layout(out, &[d], &[]);
        assert_eq!(got, vec![(ScreenId(5), None)]);
    }

    #[test]
    fn it_moves_off_a_reserved_rect_in_its_corner() {
        let out = [1920, 1080];
        let d = dock(6, DockSlot::TopLeft, Vec2::new(640.0, 360.0), DOCK_SCALE);
        let hud = Rect::new(0, 0, 300, 100);
        let got = layout(out, &[d], &[hud]);
        let (_, rect) = got[0];
        let rect = rect.unwrap();
        assert_eq!(rect.x, hud.x + hud.w + DOCK_MARGIN);
        assert_eq!(rect.y, hud.y + hud.h + DOCK_MARGIN);
        assert!(!rect.intersects(&hud));
    }

    #[test]
    fn focus_over_always_centres_on_the_whole_output() {
        let out = [1920, 1080];
        let docks = [Rect::new(0, 0, 960, 1080)];
        let c = focus_centre(out, &docks, Vec2::new(100.0, 100.0), DockFocus::Over);
        assert_eq!(c, (960.0, 540.0));
    }

    #[test]
    fn focus_beside_uses_the_free_area_left_by_a_side_dock() {
        let out = [1920, 1080];
        let docks = [Rect::new(0, 0, 960, 1080)]; // a left dock, full height
        let c = focus_centre(out, &docks, Vec2::new(800.0, 1000.0), DockFocus::Beside);
        assert_eq!(c, (1440.0, 540.0));
    }

    #[test]
    fn focus_auto_falls_back_to_over_when_it_does_not_fit() {
        let out = [1920, 1080];
        let docks = [Rect::new(0, 0, 960, 1080)];
        // 1000 px wide: does not fit the 960 px free area on the right
        let c = focus_centre(out, &docks, Vec2::new(1000.0, 500.0), DockFocus::Auto);
        assert_eq!(c, (960.0, 540.0), "over: the full output centre");
        let c = focus_centre(out, &docks, Vec2::new(800.0, 500.0), DockFocus::Auto);
        assert_eq!(c, (1440.0, 540.0), "beside: it fits");
    }

    #[test]
    fn focus_with_no_docks_is_just_the_output_centre() {
        let out = [1920, 1080];
        let c = focus_centre(out, &[], Vec2::new(800.0, 600.0), DockFocus::Beside);
        assert_eq!(c, (960.0, 540.0));
    }
}
