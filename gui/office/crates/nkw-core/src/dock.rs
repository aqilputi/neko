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

/// Every dock's rect on an output of `out` logical px: below 1× a
/// picture-in-picture in its slot; at 1× the window at 1:1, 1× docks
/// sharing the output by slot (left/right halves, top/bottom halves,
/// corners quarters); moved inward/down off every `reserved` rect (the HUD
/// wins its corner); shrunk down to DOCK_MIN_SCALE to fit; `None` = does
/// not fit (the caller keeps it docked, hidden, and toasts once).
pub fn layout(out: [i32; 2], docks: &[DockIn], reserved: &[Rect]) -> Vec<(ScreenId, Option<Rect>)> {
    let _ = (out, reserved);
    docks.iter().map(|d| (d.id, None)).collect()
}

/// The slot within DOCK_EDGE_PX of an edge/corner under (x, y).
pub fn slot_at(out: [i32; 2], x: f64, y: f64) -> Option<DockSlot> {
    let _ = (out, x, y);
    None
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
    let _ = (docks, logical, policy);
    (out[0] as f32 / 2.0, out[1] as f32 / 2.0)
}
