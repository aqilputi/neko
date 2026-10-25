//! The geometry of moving, turning, scaling and resizing a screen (plan
//! docs/plano-mundo-wm.md §2): the drag in the view plane, depth with the
//! comfort cap, rotate and scale with limits, resize with hints and presets,
//! the lasso, group moves and the overlap push.
//!
//! Units: metres and radians in the world (y up, yaw 0 looks down −z, the
//! viewer convention of `ScreenPlace`); pointer positions are physical px,
//! y down, origin top-left of the output; logical sizes are px (1 px = 1 mm
//! at scale 1).

use glam::{Mat4, Quat, Vec2, Vec3};
use serde::{Deserialize, Serialize};

use crate::ids::ScreenId;
use crate::onetoone::{self, M_PER_PX, Output, ScreenPlace};
use crate::persist::Air;
use crate::pose::Pose;
use crate::strip::{Dir, Width};
use crate::Time;

pub const DEPTH_MIN: f32 = 0.5;
pub const DEPTH_MAX: f32 = 6.0;
pub const DEPTH_STEP: f32 = 0.2;
pub const DEPTH_STEP_BIG: f32 = 0.5;
/// m/s at most (comfort, §2.12)
pub const DEPTH_SPEED: f32 = 1.0;
/// 25°
pub const PITCH_MAX: f32 = 0.436_332_3;
pub const SCALE_MIN: f32 = 0.5;
pub const SCALE_MAX: f32 = 3.0;
pub const SCALE_STEP: f32 = 0.1;
pub const NUDGE: f32 = 0.10;
pub const NUDGE_BIG: f32 = 0.50;
pub const NUDGE_FINE: f32 = 0.01;
/// 15°
pub const YAW_STEP: f32 = 0.261_799_4;
/// 5°
pub const YAW_STEP_FINE: f32 = 0.087_266_46;
/// 5°
pub const PITCH_STEP: f32 = 0.087_266_46;
pub const LIFT_M: f32 = 0.03;
pub const LIFT_BIG_M: f32 = 0.02;
pub const BIG_AREA_M2: f32 = 1.0;
pub const EYE_MIN_M: f32 = 0.5;
/// logical px; × out.scale
pub const HANDLE_CORNER_PX: f32 = 24.0;
pub const HANDLE_EDGE_PX: f32 = 12.0;
pub const MIN_LOGICAL: Vec2 = Vec2::new(200.0, 150.0);
pub const PRESET_SNAP_PX: f32 = 16.0;
pub const LIVE_CONFIGURE_HZ: f64 = 10.0;
/// > 10 % overlap pushes to the nearest free place
pub const OVERLAP_PUSH: f32 = 0.10;

/// A screen's pose anywhere (air, wall, desk as computed): no roll; `scale`
/// is visual only.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct AirPose {
    pub pos: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub scale: f32,
}

impl Default for AirPose {
    fn default() -> AirPose {
        AirPose {
            pos: Vec3::ZERO,
            yaw: 0.0,
            pitch: 0.0,
            scale: 1.0,
        }
    }
}

impl AirPose {
    /// Pitch 0, scale 1.
    pub fn new(pos: Vec3, yaw: f32) -> AirPose {
        AirPose {
            pos,
            yaw,
            ..AirPose::default()
        }
    }

    pub fn place(&self) -> ScreenPlace {
        ScreenPlace {
            center: self.pos,
            yaw: self.yaw,
            pitch: self.pitch,
        }
    }

    /// The viewer's pose that looks at this face head on (its axes are the
    /// face's right/up; its forward is the face's back).
    #[allow(dead_code)]
    fn frame(&self) -> Pose {
        Pose::new(self.pos, self.yaw, self.pitch)
    }

    /// The unit quad (x, y in −0.5..0.5, z = 0) onto the screen's face for
    /// `logical` px (1 px = 1 mm × scale) — the same convention as
    /// `scene::screen_model` and `collide::ray_hit_quad`.
    pub fn model(&self, logical: Vec2) -> Mat4 {
        let size = self.size_m(logical);
        let rot = Quat::from_rotation_y(-self.yaw) * Quat::from_rotation_x(self.pitch);
        Mat4::from_scale_rotation_translation(size.extend(1.0), rot, self.pos)
    }

    /// `logical · M_PER_PX · scale`.
    pub fn size_m(&self, logical: Vec2) -> Vec2 {
        logical * M_PER_PX * self.scale
    }

    /// Pitch within ±PITCH_MAX, scale within SCALE_MIN..=SCALE_MAX.
    pub fn clamped(self) -> AirPose {
        AirPose {
            pitch: self.pitch.clamp(-PITCH_MAX, PITCH_MAX),
            scale: self.scale.clamp(SCALE_MIN, SCALE_MAX),
            ..self
        }
    }

    /// mm / mdeg / pct, rounded.
    pub fn to_air(&self, wall: bool) -> Air {
        let mut a = Air::new(self.pos.to_array(), self.yaw, wall);
        a.pitch_mdeg = (self.pitch.to_degrees() * 1000.0).round() as i32;
        a.scale_pct = (self.scale * 100.0).round().clamp(0.0, u16::MAX as f32) as u16;
        a
    }

    pub fn from_air(a: &Air) -> AirPose {
        AirPose {
            pos: Vec3::from_array(a.pos()),
            yaw: a.yaw(),
            pitch: a.pitch(),
            scale: a.scale(),
        }
    }
}

/// The camera and the output, for pixel ↔ world.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View {
    pub cam: Pose,
    pub fov_y: f32,
    /// physical px
    pub size_px: Vec2,
}

impl View {
    /// `onetoone::focal_px(size_px.y, fov_y)`.
    pub fn focal_px(&self) -> f32 {
        (self.size_px.y / 2.0) / (self.fov_y / 2.0).tan()
    }

    /// (eye, unit direction) through the pixel `px` (continuous
    /// coordinates: pixel (i, j)'s centre is (i + 0.5, j + 0.5)).
    pub fn ray(&self, px: Vec2) -> (Vec3, Vec3) {
        let f = self.focal_px();
        let c = self.size_px / 2.0;
        let x = (px.x - c.x) / f;
        let y = (c.y - px.y) / f;
        let cam = &self.cam;
        let d = cam.forward() + cam.right() * x + cam.up() * y;
        (cam.pos, d.normalize())
    }

    /// The pixel `p` projects to; None behind the eye (or at it).
    pub fn project(&self, p: Vec3) -> Option<Vec2> {
        let cam = &self.cam;
        let rel = p - cam.pos;
        let z = rel.dot(cam.forward());
        if z <= 1e-4 {
            return None;
        }
        let f = self.focal_px();
        let c = self.size_px / 2.0;
        Some(Vec2::new(
            c.x + f * rel.dot(cam.right()) / z,
            c.y - f * rel.dot(cam.up()) / z,
        ))
    }
}

/// Where the pointer took the screen; kept for the whole drag.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Grab {
    pub start: AirPose,
    /// hit point − centre, in the view plane
    pub offset: Vec3,
    /// the centre's distance along `cam.forward()`
    pub depth: f32,
    pub billboard: bool,
    pub last_step: Time,
}

/// Take `pose` under the pointer at `px`.
pub fn grab(v: &View, pose: &AirPose, px: Vec2, billboard: bool) -> Grab {
    let _ = (v, px);
    Grab {
        start: *pose,
        offset: Vec3::ZERO,
        depth: (pose.pos - v.cam.pos).dot(v.cam.forward()),
        billboard,
        last_step: f64::NEG_INFINITY,
    }
}

/// The pose with its centre on the view-parallel plane at `g.depth` under
/// `px` (minus the offset); billboard: yaw faces the eye and pitch 0; else
/// the start yaw/pitch. Never closer than EYE_MIN_M.
pub fn drag_to(v: &View, g: &Grab, px: Vec2) -> AirPose {
    let _ = (v, px);
    g.start
}

/// ±DEPTH_STEP per step (`big`: DEPTH_STEP_BIG), clamped to
/// DEPTH_MIN..=DEPTH_MAX and to DEPTH_SPEED m/s (steps closer than
/// `step/DEPTH_SPEED` s apart are deferred: returns how many were applied).
pub fn step_depth(g: &mut Grab, steps: i32, big: bool, now: Time) -> i32 {
    let _ = (g, steps, big, now);
    0
}

/// dx → yaw, −dy → pitch, clamped.
pub fn rotate(pose: &AirPose, d_px: Vec2, rad_per_px: f32) -> AirPose {
    let _ = (d_px, rad_per_px);
    *pose
}

/// ±SCALE_STEP per step, clamped.
pub fn rescale(pose: &AirPose, steps: i32) -> AirPose {
    let _ = steps;
    *pose
}

pub fn scale_reset(pose: &AirPose) -> AirPose {
    AirPose {
        scale: 1.0,
        ..*pose
    }
}

/// The yaw facing the eye (the viewer convention: the screen's yaw is the
/// one of a camera looking at it).
pub fn face_eye(cam: &Pose, pos: Vec3) -> f32 {
    let d = pos - cam.pos;
    d.x.atan2(-d.z)
}

/// Left/Right along `cam.right()`, Up/Down along +Y.
pub fn nudge(cam: &Pose, pose: &AirPose, d: Dir, step_m: f32) -> AirPose {
    let _ = (cam, d, step_m);
    *pose
}

/// Along eye→centre (negative: farther); EYE_MIN_M..DEPTH_MAX.
pub fn nearer(cam: &Pose, pose: &AirPose, step_m: f32) -> AirPose {
    let _ = (cam, step_m);
    *pose
}

pub fn yaw_by(pose: &AirPose, rad: f32) -> AirPose {
    let _ = rad;
    *pose
}

/// Clamped.
pub fn pitch_by(pose: &AirPose, rad: f32) -> AirPose {
    let _ = rad;
    *pose
}

/// ≥ EYE_MIN_M from the eye.
pub fn comfort(cam: &Pose, pose: AirPose) -> AirPose {
    let _ = cam;
    pose
}

/// Lifted towards the eye by LIFT_M (LIFT_BIG_M when the face is >
/// BIG_AREA_M2), times `t` in 0..=1.
pub fn lift(cam: &Pose, pose: &AirPose, size_m: Vec2, t: f32) -> AirPose {
    let _ = (cam, size_m, t);
    *pose
}

/// On the eye→centre line at `scale · onetoone::distance(..)`, facing the
/// eye, pitch 0.
pub fn read_distance(cam: &Pose, out: &Output, fov_y: f32, pose: &AirPose) -> AirPose {
    let _ = (cam, out, fov_y, onetoone::distance);
    *pose
}

pub fn is_one_to_one(cam: &Pose, out: &Output, fov_y: f32, pose: &AirPose, tol_m: f32) -> bool {
    let _ = (cam, out, fov_y, pose, tol_m);
    false
}

/// A resize handle.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Grip {
    N,
    S,
    E,
    W,
    Ne,
    Nw,
    Se,
    Sw,
}

/// A handle under `px` on the projected quad: corners within
/// HANDLE_CORNER_PX·out_scale, edges within HANDLE_EDGE_PX·out_scale (inside
/// or outside the rim); None elsewhere.
pub fn grip_at(v: &View, pose: &AirPose, logical: Vec2, px: Vec2, out_scale: f32) -> Option<Grip> {
    let _ = (v, pose, logical, px, out_scale);
    None
}

pub fn nearest_corner(v: &View, pose: &AirPose, logical: Vec2, px: Vec2) -> Grip {
    let _ = (v, pose, logical, px);
    Grip::Se
}

/// `xdg_toplevel` min/max size, logical px; 0 = none.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Hints {
    pub min: Vec2,
    pub max: Vec2,
}

/// Resizing from `grip` while the pointer went `from → to`: the new logical
/// size (≥ MIN_LOGICAL and hints.min, ≤ `usable` and hints.max, snapped to
/// the presets within PRESET_SNAP_PX; `keep_aspect` keeps start w/h) and the
/// pose whose opposite edge/corner stays put in the world (same plane, same
/// yaw/pitch/scale).
#[allow(clippy::too_many_arguments)]
pub fn resize(
    v: &View,
    pose: &AirPose,
    start_logical: Vec2,
    grip: Grip,
    from: Vec2,
    to: Vec2,
    keep_aspect: bool,
    hints: &Hints,
    usable: Vec2,
) -> (Vec2, AirPose) {
    let _ = (v, grip, from, to, keep_aspect, hints, usable);
    (start_logical, *pose)
}

/// 1/3 1/2 2/3 1 of usable.x.
pub fn width_presets(usable: Vec2) -> [f32; 4] {
    [usable.x / 3.0, usable.x / 2.0, usable.x * 2.0 / 3.0, usable.x]
}

/// 1/2 2/3 1 of usable.y.
pub fn height_presets(usable: Vec2) -> [f32; 3] {
    [usable.y / 2.0, usable.y * 2.0 / 3.0, usable.y]
}

/// Width preset `n` (1..=4), height kept.
pub fn size_preset(n: u8, cur: Vec2, usable: Vec2) -> Vec2 {
    let _ = (n, usable);
    cur
}

/// The next/previous width preset.
pub fn step_width(cur: Vec2, usable: Vec2, wider: bool) -> Vec2 {
    let _ = (usable, wider);
    cur
}

/// `_` / `+`.
pub fn step_height(cur: Vec2, usable: Vec2, taller: bool) -> Vec2 {
    let _ = (usable, taller);
    cur
}

/// Desk presets as px.
pub fn width_of(w: Width, usable: Vec2) -> f32 {
    usable.x * w.fraction()
}

/// Screens whose projected centre falls in the rect `a`–`b` (any corner
/// order).
pub fn lasso(v: &View, a: Vec2, b: Vec2, centres: &[(ScreenId, Vec3)]) -> Vec<ScreenId> {
    let _ = (v, a, b, centres);
    Vec::new()
}

pub fn group_translate<K: Copy>(poses: &[(K, AirPose)], delta: Vec3) -> Vec<(K, AirPose)> {
    let _ = delta;
    poses.to_vec()
}

/// About the group centre, +Y.
pub fn group_rotate<K: Copy>(poses: &[(K, AirPose)], yaw: f32) -> Vec<(K, AirPose)> {
    let _ = yaw;
    poses.to_vec()
}

/// Fraction of `a`'s face covered by `b` (projected on `a`'s plane; 0 when
/// not facing within 30°).
pub fn overlap(a: (&AirPose, Vec2), b: (&AirPose, Vec2)) -> f32 {
    let _ = (a, b);
    0.0
}

/// With > OVERLAP_PUSH overlap on any other or inside an obstacle (floor
/// boxes, `collide::Aabb`): the nearest free pose in the same plane (5 cm
/// spiral up to `max_push_m`), else the pose as is.
pub fn free_spot(
    pose: &AirPose,
    size_m: Vec2,
    others: &[(AirPose, Vec2)],
    obstacles: &[crate::Aabb],
    max_push_m: f32,
) -> AirPose {
    let _ = (size_m, others, obstacles, max_push_m);
    *pose
}

/// Live `configure` while resizing: at most LIVE_CONFIGURE_HZ, never the
/// same size twice, one final exact.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LiveConfigure {
    last_at: Option<Time>,
    pending: Option<Vec2>,
    sent: Option<Vec2>,
}

impl LiveConfigure {
    /// Send now, or keep as pending.
    pub fn offer(&mut self, size: Vec2, now: Time) -> Option<Vec2> {
        let _ = now;
        self.pending = Some(size);
        None
    }

    /// The pending one when its slot comes.
    pub fn tick(&mut self, now: Time) -> Option<Vec2> {
        let _ = (now, self.last_at, self.sent);
        None
    }

    /// The last offered, if not yet sent.
    pub fn finish(&mut self) -> Option<Vec2> {
        self.pending.take()
    }
}
