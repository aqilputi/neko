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

use crate::Time;
use crate::ids::ScreenId;
use crate::onetoone::{self, M_PER_PX, Output, ScreenPlace};
use crate::persist::Air;
use crate::pose::Pose;
use crate::strip::{Dir, Width};

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

/// Where the ray through `px` crosses the plane through `pose.pos`
/// perpendicular to `normal`, `None` when the ray runs parallel to it.
fn plane_point(v: &View, pos: Vec3, normal: Vec3, px: Vec2) -> Vec3 {
    let (origin, dir) = v.ray(px);
    let denom = dir.dot(normal);
    if denom.abs() < 1e-6 {
        return pos;
    }
    let t = (pos - origin).dot(normal) / denom;
    origin + dir * t
}

/// Take `pose` under the pointer at `px`.
pub fn grab(v: &View, pose: &AirPose, px: Vec2, billboard: bool) -> Grab {
    let depth = (pose.pos - v.cam.pos).dot(v.cam.forward());
    let hit = plane_point(v, pose.pos, v.cam.forward(), px);
    Grab {
        start: *pose,
        offset: hit - pose.pos,
        depth,
        billboard,
        last_step: f64::NEG_INFINITY,
    }
}

/// The pose with its centre on the view-parallel plane at `g.depth` under
/// `px` (minus the offset); billboard: yaw faces the eye and pitch 0; else
/// the start yaw/pitch. Never closer than EYE_MIN_M.
pub fn drag_to(v: &View, g: &Grab, px: Vec2) -> AirPose {
    // the same view-parallel plane `grab` used, but at the (possibly
    // stepped) current depth
    let plane_pos = v.cam.pos + v.cam.forward() * g.depth;
    let hit = plane_point(v, plane_pos, v.cam.forward(), px);
    let centre = hit - g.offset;
    let (yaw, pitch) = if g.billboard {
        (face_eye(&v.cam, centre), 0.0)
    } else {
        (g.start.yaw, g.start.pitch)
    };
    comfort(
        &v.cam,
        AirPose {
            pos: centre,
            yaw,
            pitch,
            scale: g.start.scale,
        },
    )
}

/// ±DEPTH_STEP per step (`big`: DEPTH_STEP_BIG), clamped to
/// DEPTH_MIN..=DEPTH_MAX and to DEPTH_SPEED m/s (steps closer than
/// `step/DEPTH_SPEED` s apart are deferred: returns how many were applied).
pub fn step_depth(g: &mut Grab, steps: i32, big: bool, now: Time) -> i32 {
    if steps == 0 {
        return 0;
    }
    let unit = if big { DEPTH_STEP_BIG } else { DEPTH_STEP };
    let min_dt = (unit / DEPTH_SPEED) as Time;
    let elapsed = (now - g.last_step).max(0.0);
    let max_allowed = (elapsed / min_dt).floor() as i32;
    let applied = steps.abs().min(max_allowed.max(0));
    if applied <= 0 {
        return 0;
    }
    let sign = steps.signum() as f32;
    g.depth = (g.depth + sign * unit * applied as f32).clamp(DEPTH_MIN, DEPTH_MAX);
    g.last_step = now;
    applied
}

/// dx → yaw, −dy → pitch, clamped.
pub fn rotate(pose: &AirPose, d_px: Vec2, rad_per_px: f32) -> AirPose {
    AirPose {
        yaw: pose.yaw + d_px.x * rad_per_px,
        pitch: pose.pitch - d_px.y * rad_per_px,
        ..*pose
    }
    .clamped()
}

/// ±SCALE_STEP per step, clamped.
pub fn rescale(pose: &AirPose, steps: i32) -> AirPose {
    AirPose {
        scale: pose.scale + steps as f32 * SCALE_STEP,
        ..*pose
    }
    .clamped()
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
    let delta = match d {
        Dir::Left => -cam.right() * step_m,
        Dir::Right => cam.right() * step_m,
        Dir::Up => Vec3::Y * step_m,
        Dir::Down => -Vec3::Y * step_m,
    };
    comfort(
        cam,
        AirPose {
            pos: pose.pos + delta,
            ..*pose
        },
    )
}

/// Along eye→centre (negative: farther); EYE_MIN_M..DEPTH_MAX.
pub fn nearer(cam: &Pose, pose: &AirPose, step_m: f32) -> AirPose {
    let diff = pose.pos - cam.pos;
    let dist = diff.length();
    let dir = if dist > 1e-6 {
        diff / dist
    } else {
        cam.forward()
    };
    let new_dist = (dist - step_m).clamp(EYE_MIN_M, DEPTH_MAX);
    AirPose {
        pos: cam.pos + dir * new_dist,
        ..*pose
    }
}

pub fn yaw_by(pose: &AirPose, rad: f32) -> AirPose {
    AirPose {
        yaw: pose.yaw + rad,
        ..*pose
    }
}

/// Clamped.
pub fn pitch_by(pose: &AirPose, rad: f32) -> AirPose {
    AirPose {
        pitch: pose.pitch + rad,
        ..*pose
    }
    .clamped()
}

/// ≥ EYE_MIN_M from the eye.
pub fn comfort(cam: &Pose, pose: AirPose) -> AirPose {
    let diff = pose.pos - cam.pos;
    let len = diff.length();
    if len >= EYE_MIN_M {
        return pose;
    }
    let dir = if len > 1e-6 {
        diff / len
    } else {
        cam.forward()
    };
    AirPose {
        pos: cam.pos + dir * EYE_MIN_M,
        ..pose
    }
}

/// Lifted towards the eye by LIFT_M (LIFT_BIG_M when the face is >
/// BIG_AREA_M2), times `t` in 0..=1.
pub fn lift(cam: &Pose, pose: &AirPose, size_m: Vec2, t: f32) -> AirPose {
    let amount = if size_m.x * size_m.y > BIG_AREA_M2 {
        LIFT_BIG_M
    } else {
        LIFT_M
    };
    let diff = cam.pos - pose.pos;
    let len = diff.length();
    let dir = if len > 1e-6 {
        diff / len
    } else {
        cam.forward()
    };
    AirPose {
        pos: pose.pos + dir * amount * t.clamp(0.0, 1.0),
        ..*pose
    }
}

/// On the eye→centre line at `scale · onetoone::distance(..)`, facing the
/// eye, pitch 0.
pub fn read_distance(cam: &Pose, out: &Output, fov_y: f32, pose: &AirPose) -> AirPose {
    let diff = pose.pos - cam.pos;
    let dir = if diff.length() > 1e-6 {
        diff.normalize()
    } else {
        cam.forward()
    };
    let dist = pose.scale * onetoone::distance(out.height_px, out.scale, fov_y);
    let centre = cam.pos + dir * dist;
    AirPose {
        pos: centre,
        yaw: face_eye(cam, centre),
        pitch: 0.0,
        scale: pose.scale,
    }
}

pub fn is_one_to_one(cam: &Pose, out: &Output, fov_y: f32, pose: &AirPose, tol_m: f32) -> bool {
    let want = pose.scale * onetoone::distance(out.height_px, out.scale, fov_y);
    let got = (pose.pos - cam.pos).length();
    (got - want).abs() <= tol_m
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

/// The four corners of `pose`'s face, world space, in `Grip::{Nw,Ne,Se,Sw}`
/// order.
fn corners_world(pose: &AirPose, logical: Vec2) -> [(Vec3, Grip); 4] {
    let model = pose.model(logical);
    let at = |x: f32, y: f32| model.transform_point3(Vec3::new(x, y, 0.0));
    [
        (at(-0.5, 0.5), Grip::Nw),
        (at(0.5, 0.5), Grip::Ne),
        (at(0.5, -0.5), Grip::Se),
        (at(-0.5, -0.5), Grip::Sw),
    ]
}

/// The shortest distance from `p` to the segment `a`–`b`.
fn point_seg_dist(p: Vec2, a: Vec2, b: Vec2) -> f32 {
    let ab = b - a;
    let len2 = ab.length_squared();
    let t = if len2 > 1e-9 {
        ((p - a).dot(ab) / len2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    p.distance(a + ab * t)
}

/// A handle under `px` on the projected quad: corners within
/// HANDLE_CORNER_PX·out_scale, edges within HANDLE_EDGE_PX·out_scale (inside
/// or outside the rim); None elsewhere.
pub fn grip_at(v: &View, pose: &AirPose, logical: Vec2, px: Vec2, out_scale: f32) -> Option<Grip> {
    let corners = corners_world(pose, logical);
    let projected: Vec<(Vec2, Grip)> = corners
        .iter()
        .filter_map(|(w, g)| v.project(*w).map(|p| (p, *g)))
        .collect();
    if projected.len() < 4 {
        return None;
    }
    let corner_r = HANDLE_CORNER_PX * out_scale;
    for (p, g) in &projected {
        if px.distance(*p) <= corner_r {
            return Some(*g);
        }
    }
    let (nw, ne, se, sw) = (
        projected[0].0,
        projected[1].0,
        projected[2].0,
        projected[3].0,
    );
    let edge_r = HANDLE_EDGE_PX * out_scale;
    [
        (nw, ne, Grip::N),
        (ne, se, Grip::E),
        (se, sw, Grip::S),
        (sw, nw, Grip::W),
    ]
    .into_iter()
    .map(|(a, b, g)| (point_seg_dist(px, a, b), g))
    .filter(|(d, _)| *d <= edge_r)
    .min_by(|a, b| a.0.total_cmp(&b.0))
    .map(|(_, g)| g)
}

pub fn nearest_corner(v: &View, pose: &AirPose, logical: Vec2, px: Vec2) -> Grip {
    corners_world(pose, logical)
        .into_iter()
        .filter_map(|(w, g)| v.project(w).map(|p| (px.distance(p), g)))
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, g)| g)
        .unwrap_or(Grip::Se)
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
    // the resize happens in the screen's own plane (its own yaw/pitch),
    // not the camera's: a tilted screen's corner follows the ray hitting
    // its actual face.
    let rot = Quat::from_rotation_y(-pose.yaw) * Quat::from_rotation_x(pose.pitch);
    let right_dir = rot * Vec3::X;
    let up_dir = rot * Vec3::Y;
    let normal = rot * Vec3::Z;
    let local = |px: Vec2| -> (f32, f32) {
        let p = plane_point(v, pose.pos, normal, px);
        let rel = p - pose.pos;
        (rel.dot(right_dir), rel.dot(up_dir))
    };
    let (fu, fv) = local(from);
    let (tu, tv) = local(to);
    let (du, dv) = (tu - fu, tv - fv);

    let (move_left, move_right) = match grip {
        Grip::W | Grip::Nw | Grip::Sw => (true, false),
        Grip::E | Grip::Ne | Grip::Se => (false, true),
        Grip::N | Grip::S => (false, false),
    };
    let (move_bottom, move_top) = match grip {
        Grip::S | Grip::Sw | Grip::Se => (true, false),
        Grip::N | Grip::Nw | Grip::Ne => (false, true),
        Grip::W | Grip::E => (false, false),
    };

    let m_per_px = M_PER_PX * pose.scale;
    let start_w = (start_logical.x * m_per_px).max(1e-4);
    let start_h = (start_logical.y * m_per_px).max(1e-4);
    let (left0, right0) = (-start_w / 2.0, start_w / 2.0);
    let (bottom0, top0) = (-start_h / 2.0, start_h / 2.0);

    let raw_w = if move_right {
        start_w + du
    } else if move_left {
        start_w - du
    } else {
        start_w
    }
    .max(1e-4);
    let raw_h = if move_top {
        start_h + dv
    } else if move_bottom {
        start_h - dv
    } else {
        start_h
    }
    .max(1e-4);

    let (mut w, mut h) = (raw_w, raw_h);
    if keep_aspect {
        let aspect = start_w / start_h;
        let drives_w = move_left || move_right;
        let drives_h = move_top || move_bottom;
        if drives_w && !drives_h {
            h = w / aspect;
        } else if drives_h && !drives_w {
            w = h * aspect;
        } else if drives_w && drives_h {
            if (w / start_w - 1.0).abs() >= (h / start_h - 1.0).abs() {
                h = w / aspect;
            } else {
                w = h * aspect;
            }
        }
    }

    let mut logical = Vec2::new(w, h) / m_per_px;
    let min = MIN_LOGICAL.max(Vec2::new(hints.min.x.max(0.0), hints.min.y.max(0.0)));
    let max = Vec2::new(
        if hints.max.x > 0.0 {
            usable.x.min(hints.max.x)
        } else {
            usable.x
        },
        if hints.max.y > 0.0 {
            usable.y.min(hints.max.y)
        } else {
            usable.y
        },
    )
    .max(min);
    logical = logical.clamp(min, max);
    let snap = |val: f32, presets: &[f32]| -> f32 {
        presets
            .iter()
            .copied()
            .find(|p| (p - val).abs() <= PRESET_SNAP_PX)
            .unwrap_or(val)
    };
    logical.x = snap(logical.x, &width_presets(usable));
    logical.y = snap(logical.y, &height_presets(usable));

    // redo the anchor-preserving edges with the final (clamped/snapped)
    // size, so the fixed edge/corner stays exactly put.
    let w_final = logical.x * m_per_px;
    let h_final = logical.y * m_per_px;
    let (left, right) = if move_right {
        (left0, left0 + w_final)
    } else if move_left {
        (right0 - w_final, right0)
    } else {
        (-w_final / 2.0, w_final / 2.0)
    };
    let (bottom, top) = if move_top {
        (bottom0, bottom0 + h_final)
    } else if move_bottom {
        (top0 - h_final, top0)
    } else {
        (-h_final / 2.0, h_final / 2.0)
    };
    let centre_u = (left + right) / 2.0;
    let centre_v = (bottom + top) / 2.0;
    let new_pos = pose.pos + right_dir * centre_u + up_dir * centre_v;
    (
        logical,
        AirPose {
            pos: new_pos,
            ..*pose
        },
    )
}

/// 1/3 1/2 2/3 1 of usable.x.
pub fn width_presets(usable: Vec2) -> [f32; 4] {
    [
        usable.x / 3.0,
        usable.x / 2.0,
        usable.x * 2.0 / 3.0,
        usable.x,
    ]
}

/// 1/2 2/3 1 of usable.y.
pub fn height_presets(usable: Vec2) -> [f32; 3] {
    [usable.y / 2.0, usable.y * 2.0 / 3.0, usable.y]
}

/// Width preset `n` (1..=4), height kept.
pub fn size_preset(n: u8, cur: Vec2, usable: Vec2) -> Vec2 {
    let presets = width_presets(usable);
    let idx = (n.clamp(1, 4) - 1) as usize;
    Vec2::new(presets[idx], cur.y)
}

/// The nearest `presets` member strictly greater than `v` (or the largest,
/// if already at or past it) when `up`, else strictly smaller (or the
/// smallest).
fn step_preset(v: f32, presets: &[f32], up: bool) -> f32 {
    const EPS: f32 = 0.5;
    if up {
        presets
            .iter()
            .copied()
            .filter(|p| *p > v + EPS)
            .min_by(f32::total_cmp)
            .unwrap_or_else(|| presets.iter().copied().fold(f32::MIN, f32::max))
    } else {
        presets
            .iter()
            .copied()
            .filter(|p| *p < v - EPS)
            .max_by(f32::total_cmp)
            .unwrap_or_else(|| presets.iter().copied().fold(f32::MAX, f32::min))
    }
}

/// The next/previous width preset.
pub fn step_width(cur: Vec2, usable: Vec2, wider: bool) -> Vec2 {
    Vec2::new(step_preset(cur.x, &width_presets(usable), wider), cur.y)
}

/// `_` / `+`.
pub fn step_height(cur: Vec2, usable: Vec2, taller: bool) -> Vec2 {
    Vec2::new(cur.x, step_preset(cur.y, &height_presets(usable), taller))
}

/// Desk presets as px.
pub fn width_of(w: Width, usable: Vec2) -> f32 {
    usable.x * w.fraction()
}

/// Screens whose projected centre falls in the rect `a`–`b` (any corner
/// order).
pub fn lasso(v: &View, a: Vec2, b: Vec2, centres: &[(ScreenId, Vec3)]) -> Vec<ScreenId> {
    let (min, max) = (a.min(b), a.max(b));
    centres
        .iter()
        .filter_map(|(id, c)| v.project(*c).map(|p| (*id, p)))
        .filter(|(_, p)| p.x >= min.x && p.x <= max.x && p.y >= min.y && p.y <= max.y)
        .map(|(id, _)| id)
        .collect()
}

pub fn group_translate<K: Copy>(poses: &[(K, AirPose)], delta: Vec3) -> Vec<(K, AirPose)> {
    poses
        .iter()
        .map(|(k, p)| {
            (
                *k,
                AirPose {
                    pos: p.pos + delta,
                    ..*p
                },
            )
        })
        .collect()
}

/// About the group centre, +Y.
pub fn group_rotate<K: Copy>(poses: &[(K, AirPose)], yaw: f32) -> Vec<(K, AirPose)> {
    if poses.is_empty() {
        return Vec::new();
    }
    let centre: Vec3 = poses.iter().map(|(_, p)| p.pos).sum::<Vec3>() / poses.len() as f32;
    // the same handedness `AirPose::model`/`face_eye` use, so a rotated
    // cluster's screens keep facing outward as a rigid body.
    let rot = Quat::from_rotation_y(-yaw);
    poses
        .iter()
        .map(|(k, p)| {
            let rel = p.pos - centre;
            (
                *k,
                AirPose {
                    pos: centre + rot * rel,
                    yaw: p.yaw + yaw,
                    ..*p
                },
            )
        })
        .collect()
}

/// `pose`'s own basis: right, up, face normal (world space).
fn basis(pose: &AirPose) -> (Vec3, Vec3, Vec3) {
    let rot = Quat::from_rotation_y(-pose.yaw) * Quat::from_rotation_x(pose.pitch);
    (rot * Vec3::X, rot * Vec3::Y, rot * Vec3::Z)
}

/// Fraction of `a`'s face covered by `b` (projected on `a`'s plane; 0 when
/// not facing within 30°). `size_m` is each screen's full size, in metres
/// (as `free_spot` takes it too).
pub fn overlap(a: (&AirPose, Vec2), b: (&AirPose, Vec2)) -> f32 {
    let (pa, size_a) = a;
    let (pb, size_b) = b;
    let (right_a, up_a, normal_a) = basis(pa);
    let (right_b, up_b, normal_b) = basis(pb);
    let cos30 = 30f32.to_radians().cos();
    if normal_a.dot(normal_b) < cos30 {
        return 0.0;
    }
    let half_a = size_a / 2.0;
    let half_b = size_b / 2.0;
    let corners_b_world = [
        Vec2::new(-half_b.x, -half_b.y),
        Vec2::new(half_b.x, -half_b.y),
        Vec2::new(half_b.x, half_b.y),
        Vec2::new(-half_b.x, half_b.y),
    ]
    .map(|local| pb.pos + right_b * local.x + up_b * local.y);
    let local_b: Vec<Vec2> = corners_b_world
        .iter()
        .map(|w| {
            let rel = *w - pa.pos;
            Vec2::new(rel.dot(right_a), rel.dot(up_a))
        })
        .collect();
    let bmin = local_b
        .iter()
        .copied()
        .reduce(Vec2::min)
        .unwrap_or(Vec2::ZERO);
    let bmax = local_b
        .iter()
        .copied()
        .reduce(Vec2::max)
        .unwrap_or(Vec2::ZERO);
    let (amin, amax) = (-half_a, half_a);
    let ix = (amax.x.min(bmax.x) - amin.x.max(bmin.x)).max(0.0);
    let iy = (amax.y.min(bmax.y) - amin.y.max(bmin.y)).max(0.0);
    let inter = ix * iy;
    let area_a = half_a.x * 2.0 * (half_a.y * 2.0);
    if area_a <= 1e-9 {
        0.0
    } else {
        (inter / area_a).clamp(0.0, 1.0)
    }
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
    let (right_dir, up_dir, _) = basis(pose);
    let blocked = |p: &AirPose| -> bool {
        let flat = Vec2::new(p.pos.x, p.pos.z);
        if obstacles.iter().any(|o| o.contains(flat)) {
            return true;
        }
        others
            .iter()
            .any(|(o, sz)| overlap((p, size_m), (o, *sz)) > OVERLAP_PUSH)
    };
    if !blocked(pose) {
        return *pose;
    }
    const STEP: f32 = 0.05;
    let mut r = STEP;
    while r <= max_push_m + 1e-6 {
        let rings = ((std::f32::consts::TAU * r / STEP).ceil() as usize).max(8);
        for i in 0..rings {
            let theta = (i as f32 / rings as f32) * std::f32::consts::TAU;
            let offset = right_dir * (r * theta.cos()) + up_dir * (r * theta.sin());
            let candidate = AirPose {
                pos: pose.pos + offset,
                ..*pose
            };
            if !blocked(&candidate) {
                return candidate;
            }
        }
        r += STEP;
    }
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
        if self.sent == Some(size) {
            self.pending = None;
            return None;
        }
        let min_dt = 1.0 / LIVE_CONFIGURE_HZ;
        let ready = self.last_at.is_none_or(|t| now - t >= min_dt);
        if ready {
            self.sent = Some(size);
            self.last_at = Some(now);
            self.pending = None;
            Some(size)
        } else {
            self.pending = Some(size);
            None
        }
    }

    /// The pending one when its slot comes.
    pub fn tick(&mut self, now: Time) -> Option<Vec2> {
        let size = self.pending?;
        let min_dt = 1.0 / LIVE_CONFIGURE_HZ;
        if !self.last_at.is_none_or(|t| now - t >= min_dt) {
            return None;
        }
        self.sent = Some(size);
        self.last_at = Some(now);
        self.pending = None;
        Some(size)
    }

    /// The last offered, if not yet sent.
    pub fn finish(&mut self) -> Option<Vec2> {
        let size = self.pending.take()?;
        self.sent = Some(size);
        Some(size)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::FRAC_PI_2;

    fn close(a: f32, b: f32, eps: f32) -> bool {
        (a - b).abs() <= eps
    }

    fn close3(a: Vec3, b: Vec3, eps: f32) -> bool {
        (a - b).length() <= eps
    }

    fn close2(a: Vec2, b: Vec2, eps: f32) -> bool {
        (a - b).length() <= eps
    }

    /// Cam at the origin, looking down -z; fov 90° and a 1000×1000 output
    /// give a focal length of exactly 500 px.
    fn view() -> View {
        View {
            cam: Pose::default(),
            fov_y: FRAC_PI_2,
            size_px: Vec2::new(1000.0, 1000.0),
        }
    }

    fn pose_at(z: f32) -> AirPose {
        AirPose::new(Vec3::new(0.0, 0.0, z), 0.0)
    }

    #[test]
    fn clamped_limits_pitch_and_scale() {
        let rows: [(f32, f32, f32, f32); 5] = [
            (0.0, 1.0, 0.0, 1.0),
            (PITCH_MAX + 1.0, 1.0, PITCH_MAX, 1.0),
            (-PITCH_MAX - 1.0, 1.0, -PITCH_MAX, 1.0),
            (0.0, SCALE_MAX + 1.0, 0.0, SCALE_MAX),
            (0.0, SCALE_MIN - 1.0, 0.0, SCALE_MIN),
        ];
        for (pitch, scale, want_pitch, want_scale) in rows {
            let p = AirPose {
                pitch,
                scale,
                ..AirPose::default()
            }
            .clamped();
            assert_eq!(p.pitch, want_pitch);
            assert_eq!(p.scale, want_scale);
        }
    }

    #[test]
    fn air_round_trips_through_millimetres() {
        let p = AirPose {
            pos: Vec3::new(1.25, 1.5, -2.0),
            yaw: 0.3,
            pitch: 0.1,
            scale: 1.5,
        };
        let a = p.to_air(true);
        assert!(a.wall);
        let back = AirPose::from_air(&a);
        assert!(close3(back.pos, p.pos, 1e-3));
        assert!(close(back.yaw, p.yaw, 1e-4));
        assert!(close(back.pitch, p.pitch, 1e-4));
        assert!(close(back.scale, p.scale, 1e-3));
    }

    #[test]
    fn grabbing_the_centre_has_no_offset() {
        let v = view();
        let pose = pose_at(-3.0);
        let g = grab(&v, &pose, Vec2::new(500.0, 500.0), false);
        assert!(close3(g.offset, Vec3::ZERO, 1e-3));
        assert!(close(g.depth, 3.0, 1e-4));
    }

    #[test]
    fn dragging_moves_the_centre_in_the_view_plane() {
        let v = view();
        let pose = pose_at(-3.0);
        let g = grab(&v, &pose, Vec2::new(500.0, 500.0), false);
        // 100 px right at a 3 m plane and a 500 px focal is 0.6 m
        let moved = drag_to(&v, &g, Vec2::new(600.0, 500.0));
        assert!(close3(moved.pos, Vec3::new(0.6, 0.0, -3.0), 1e-3));
        assert_eq!(moved.yaw, pose.yaw, "not billboard: keeps the start yaw");
        assert_eq!(moved.pitch, pose.pitch);
        // dragging back to the grab point returns to the start
        let back = drag_to(&v, &g, Vec2::new(500.0, 500.0));
        assert!(close3(back.pos, pose.pos, 1e-3));
    }

    #[test]
    fn billboard_faces_the_eye_at_the_new_spot() {
        let v = view();
        let pose = pose_at(-3.0);
        let g = grab(&v, &pose, Vec2::new(500.0, 500.0), true);
        let moved = drag_to(&v, &g, Vec2::new(700.0, 500.0));
        assert_eq!(moved.pitch, 0.0);
        assert!((moved.yaw - face_eye(&v.cam, moved.pos)).abs() < 1e-5);
        assert_ne!(moved.yaw, 0.0, "off to the side: no longer facing -z");
    }

    #[test]
    fn dragging_never_gets_closer_than_the_comfort_floor() {
        let v = View {
            cam: Pose::default(),
            ..view()
        };
        // a plane right at the eye: the comfort floor kicks in
        let pose = AirPose::new(Vec3::new(0.0, 0.0, -0.01), 0.0);
        let g = grab(&v, &pose, Vec2::new(500.0, 500.0), false);
        let moved = drag_to(&v, &g, Vec2::new(500.0, 500.0));
        assert!((moved.pos - v.cam.pos).length() >= EYE_MIN_M - 1e-4);
    }

    #[test]
    fn step_depth_table() {
        let mut g = Grab {
            start: AirPose::default(),
            offset: Vec3::ZERO,
            depth: 1.0,
            billboard: false,
            last_step: f64::NEG_INFINITY,
        };
        // the very first step always applies, whatever the elapsed time
        assert_eq!(step_depth(&mut g, 3, false, 0.0), 3);
        assert!(close(g.depth, 1.0 + 3.0 * DEPTH_STEP, 1e-5));
        // immediately again: too soon, deferred
        assert_eq!(step_depth(&mut g, 1, false, 0.0), 0);
        // after the minimum interval, one more step is allowed
        let min_dt = (DEPTH_STEP / DEPTH_SPEED) as Time;
        let before = g.depth;
        assert_eq!(step_depth(&mut g, 2, false, min_dt), 1);
        assert!(close(g.depth, before + DEPTH_STEP, 1e-5));
        // negative steps move the other way and clamp at DEPTH_MIN
        let mut g = Grab {
            depth: DEPTH_MIN + 0.05,
            last_step: f64::NEG_INFINITY,
            ..g
        };
        assert_eq!(step_depth(&mut g, -5, false, 10.0), 5);
        assert_eq!(g.depth, DEPTH_MIN);
        // clamps at DEPTH_MAX too, with the big step
        let mut g = Grab {
            depth: DEPTH_MAX - 0.1,
            last_step: f64::NEG_INFINITY,
            ..g
        };
        assert_eq!(step_depth(&mut g, 5, true, 20.0), 5);
        assert_eq!(g.depth, DEPTH_MAX);
        // zero steps: a no-op that never updates last_step
        let mut g = Grab {
            last_step: f64::NEG_INFINITY,
            ..g
        };
        assert_eq!(step_depth(&mut g, 0, false, 5.0), 0);
        assert_eq!(g.last_step, f64::NEG_INFINITY);
    }

    #[test]
    fn rotate_turns_and_tilts_with_limits() {
        let p = AirPose::default();
        let r = rotate(&p, Vec2::new(100.0, -20.0), 0.01);
        assert!(close(r.yaw, 1.0, 1e-5));
        assert!(close(r.pitch, 0.2, 1e-5), "-dy -> pitch: {}", r.pitch);
        // pitch clamps; yaw keeps going
        let r = rotate(&p, Vec2::new(0.0, -10_000.0), 0.01);
        assert_eq!(r.pitch, PITCH_MAX);
    }

    #[test]
    fn rescale_steps_and_clamps() {
        let p = AirPose::default();
        assert!(close(rescale(&p, 2).scale, 1.2, 1e-5));
        assert!(close(rescale(&p, -2).scale, 0.8, 1e-5));
        assert_eq!(rescale(&p, 100).scale, SCALE_MAX);
        assert_eq!(rescale(&p, -100).scale, SCALE_MIN);
        let scaled = AirPose { scale: 2.0, ..p };
        assert_eq!(scale_reset(&scaled).scale, 1.0);
    }

    #[test]
    fn nudge_moves_along_camera_right_and_world_up() {
        let cam = Pose::new(Vec3::ZERO, FRAC_PI_2, 0.0); // right() = +z here
        let p = pose_at(-2.0);
        assert!(close3(
            nudge(&cam, &p, Dir::Right, 0.1).pos,
            p.pos + Vec3::new(0.0, 0.0, 0.1),
            1e-5
        ));
        assert!(close3(
            nudge(&cam, &p, Dir::Left, 0.1).pos,
            p.pos + Vec3::new(0.0, 0.0, -0.1),
            1e-5
        ));
        assert!(close3(
            nudge(&cam, &p, Dir::Up, 0.1).pos,
            p.pos + Vec3::Y * 0.1,
            1e-5
        ));
        assert!(close3(
            nudge(&cam, &p, Dir::Down, 0.1).pos,
            p.pos - Vec3::Y * 0.1,
            1e-5
        ));
    }

    #[test]
    fn nearer_and_farther_clamp_to_the_comfort_range() {
        let cam = Pose::default();
        let p = pose_at(-2.0);
        let closer = nearer(&cam, &p, 0.5);
        assert!(close(closer.pos.distance(cam.pos), 1.5, 1e-5));
        let farther = nearer(&cam, &p, -0.5);
        assert!(close(farther.pos.distance(cam.pos), 2.5, 1e-5));
        assert!(close(
            nearer(&cam, &p, 100.0).pos.distance(cam.pos),
            EYE_MIN_M,
            1e-5
        ));
        assert!(close(
            nearer(&cam, &p, -100.0).pos.distance(cam.pos),
            DEPTH_MAX,
            1e-5
        ));
    }

    #[test]
    fn yaw_and_pitch_by_add_and_pitch_clamps() {
        let p = AirPose::default();
        assert!(close(yaw_by(&p, 0.3).yaw, 0.3, 1e-6));
        assert!(close(pitch_by(&p, 0.1).pitch, 0.1, 1e-6));
        assert_eq!(pitch_by(&p, 10.0).pitch, PITCH_MAX);
        assert_eq!(pitch_by(&p, -10.0).pitch, -PITCH_MAX);
    }

    #[test]
    fn comfort_table() {
        let cam = Pose::default();
        let rows: [(Vec3, f32); 3] = [
            (Vec3::new(0.0, 0.0, -2.0), 2.0),
            (Vec3::new(0.0, 0.0, -0.1), EYE_MIN_M),
            (Vec3::ZERO, EYE_MIN_M),
        ];
        for (pos, want_dist) in rows {
            let p = comfort(&cam, AirPose::new(pos, 0.0));
            assert!(
                close(p.pos.distance(cam.pos), want_dist, 1e-4),
                "{pos:?} -> {p:?}"
            );
        }
    }

    #[test]
    fn lift_is_smaller_for_big_screens_and_scales_with_t() {
        let cam = Pose::default();
        let p = pose_at(-2.0);
        let small = lift(&cam, &p, Vec2::new(0.8, 0.6), 1.0);
        assert!(close((small.pos - p.pos).length(), LIFT_M, 1e-5));
        assert!((small.pos.z - p.pos.z) > 0.0, "towards the eye: +z here");
        let big = lift(&cam, &p, Vec2::new(1.2, 1.0), 1.0);
        assert!(close((big.pos - p.pos).length(), LIFT_BIG_M, 1e-5));
        let half = lift(&cam, &p, Vec2::new(0.8, 0.6), 0.5);
        assert!(close((half.pos - p.pos).length(), LIFT_M * 0.5, 1e-5));
    }

    #[test]
    fn read_distance_lands_at_the_one_to_one_point() {
        let cam = Pose::default();
        let out = Output {
            width_px: 1920,
            height_px: 1080,
            scale: 1.0,
        };
        let fov = std::f32::consts::FRAC_PI_3;
        let p = pose_at(-3.0);
        let want = onetoone::distance(out.height_px, out.scale, fov);
        let r = read_distance(&cam, &out, fov, &p);
        assert!(close(r.pos.distance(cam.pos), want, 1e-4));
        assert_eq!(r.pitch, 0.0);
        assert!(is_one_to_one(&cam, &out, fov, &r, 1e-3));
        assert!(!is_one_to_one(&cam, &out, fov, &p, 1e-3));
        // scale multiplies the reading distance
        let scaled = AirPose { scale: 2.0, ..p };
        let r2 = read_distance(&cam, &out, fov, &scaled);
        assert!(close(r2.pos.distance(cam.pos), 2.0 * want, 1e-4));
    }

    const LOGICAL: Vec2 = Vec2::new(800.0, 600.0);

    #[test]
    fn grip_at_finds_corners_then_edges_then_nothing() {
        let v = view();
        let p = pose_at(-2.0);
        // the geometry below: corners at (400,425) (600,425) (600,575) (400,575)
        assert_eq!(
            grip_at(&v, &p, LOGICAL, Vec2::new(400.0, 425.0), 1.0),
            Some(Grip::Nw)
        );
        assert_eq!(
            grip_at(&v, &p, LOGICAL, Vec2::new(410.0, 430.0), 1.0),
            Some(Grip::Nw),
            "within the corner radius"
        );
        assert_eq!(
            grip_at(&v, &p, LOGICAL, Vec2::new(500.0, 425.0), 1.0),
            Some(Grip::N),
            "the middle of the top edge"
        );
        assert_eq!(
            grip_at(&v, &p, LOGICAL, Vec2::new(600.0, 500.0), 1.0),
            Some(Grip::E)
        );
        assert_eq!(
            grip_at(&v, &p, LOGICAL, Vec2::new(500.0, 500.0), 1.0),
            None,
            "dead centre: content, not a handle"
        );
        // out_scale widens the hit zones
        assert_eq!(
            grip_at(&v, &p, LOGICAL, Vec2::new(500.0, 445.0), 2.0),
            Some(Grip::N)
        );
    }

    #[test]
    fn nearest_corner_picks_the_closest_one() {
        let v = view();
        let p = pose_at(-2.0);
        assert_eq!(
            nearest_corner(&v, &p, LOGICAL, Vec2::new(550.0, 450.0)),
            Grip::Ne
        );
        assert_eq!(
            nearest_corner(&v, &p, LOGICAL, Vec2::new(420.0, 560.0)),
            Grip::Sw
        );
    }

    fn big_usable() -> Vec2 {
        Vec2::new(2000.0, 2000.0)
    }

    #[test]
    fn resize_from_the_right_edge_grows_width_and_keeps_the_left_edge_put() {
        let v = view();
        let p = pose_at(-2.0);
        let (logical, moved) = resize(
            &v,
            &p,
            LOGICAL,
            Grip::E,
            Vec2::new(500.0, 500.0),
            Vec2::new(600.0, 500.0),
            false,
            &Hints::default(),
            big_usable(),
        );
        assert!(close2(logical, Vec2::new(1200.0, 600.0), 1.0));
        assert!(close3(moved.pos, Vec3::new(0.2, 0.0, -2.0), 1e-2));
    }

    #[test]
    fn resize_from_the_left_edge_keeps_the_right_edge_put() {
        let v = view();
        let p = pose_at(-2.0);
        let (logical, moved) = resize(
            &v,
            &p,
            LOGICAL,
            Grip::W,
            Vec2::new(500.0, 500.0),
            Vec2::new(600.0, 500.0),
            false,
            &Hints::default(),
            big_usable(),
        );
        assert!(close2(logical, Vec2::new(400.0, 600.0), 1.0));
        assert!(close3(moved.pos, Vec3::new(0.2, 0.0, -2.0), 1e-2));
    }

    #[test]
    fn resize_from_a_corner_grows_both_axes() {
        let v = view();
        let p = pose_at(-2.0);
        let (logical, moved) = resize(
            &v,
            &p,
            LOGICAL,
            Grip::Ne,
            Vec2::new(500.0, 500.0),
            Vec2::new(600.0, 400.0),
            false,
            &Hints::default(),
            big_usable(),
        );
        assert!(close2(logical, Vec2::new(1200.0, 1000.0), 1.0));
        assert!(close3(moved.pos, Vec3::new(0.2, 0.2, -2.0), 1e-2));
    }

    #[test]
    fn resize_keeps_aspect_when_asked() {
        let v = view();
        let p = pose_at(-2.0);
        let (logical, moved) = resize(
            &v,
            &p,
            LOGICAL,
            Grip::E,
            Vec2::new(500.0, 500.0),
            Vec2::new(600.0, 500.0),
            true,
            &Hints::default(),
            big_usable(),
        );
        assert!(close2(logical, Vec2::new(1200.0, 900.0), 1.0));
        // height grew symmetrically: no vertical anchor was dragged
        assert!(close(moved.pos.y, 0.0, 1e-3));
    }

    #[test]
    fn resize_never_shrinks_below_the_minimum() {
        let v = view();
        let p = pose_at(-2.0);
        let (logical, _) = resize(
            &v,
            &p,
            LOGICAL,
            Grip::E,
            Vec2::new(500.0, 500.0),
            Vec2::new(-5000.0, 500.0),
            false,
            &Hints::default(),
            big_usable(),
        );
        assert_eq!(logical.x, MIN_LOGICAL.x);
    }

    #[test]
    fn resize_respects_hints_and_the_usable_ceiling() {
        let v = view();
        let p = pose_at(-2.0);
        let hints = Hints {
            min: Vec2::new(300.0, 0.0),
            max: Vec2::ZERO,
        };
        let (logical, _) = resize(
            &v,
            &p,
            LOGICAL,
            Grip::E,
            Vec2::new(500.0, 500.0),
            Vec2::new(-5000.0, 500.0),
            false,
            &hints,
            big_usable(),
        );
        assert_eq!(logical.x, 300.0, "a stricter hint wins over MIN_LOGICAL");
        let (logical, _) = resize(
            &v,
            &p,
            LOGICAL,
            Grip::E,
            Vec2::new(500.0, 500.0),
            Vec2::new(50_000.0, 500.0),
            false,
            &Hints::default(),
            big_usable(),
        );
        assert_eq!(logical.x, big_usable().x, "never past the usable width");
    }

    #[test]
    fn resize_snaps_within_the_preset_tolerance() {
        let v = view();
        let p = pose_at(-2.0);
        // du of 0.197 m lands the raw width at 997 px, 3 px from the 1000 preset
        let (logical, _) = resize(
            &v,
            &p,
            LOGICAL,
            Grip::E,
            Vec2::new(500.0, 500.0),
            Vec2::new(549.25, 500.0),
            false,
            &Hints::default(),
            big_usable(),
        );
        assert_eq!(logical.x, 1000.0);
    }

    #[test]
    fn width_and_height_preset_helpers() {
        let usable = Vec2::new(1200.0, 900.0);
        assert_eq!(width_presets(usable), [400.0, 600.0, 800.0, 1200.0]);
        assert_eq!(height_presets(usable), [450.0, 600.0, 900.0]);
        assert_eq!(size_preset(1, Vec2::new(0.0, 500.0), usable).x, 400.0);
        assert_eq!(size_preset(4, Vec2::new(0.0, 500.0), usable).x, 1200.0);
        assert_eq!(size_preset(4, Vec2::new(0.0, 500.0), usable).y, 500.0);
    }

    #[test]
    fn step_width_and_height_move_to_the_next_preset() {
        let usable = Vec2::new(1200.0, 900.0);
        let cur = Vec2::new(600.0, 450.0);
        assert_eq!(step_width(cur, usable, true).x, 800.0);
        assert_eq!(step_width(cur, usable, false).x, 400.0);
        assert_eq!(
            step_width(Vec2::new(1200.0, 0.0), usable, true).x,
            1200.0,
            "stays at the top"
        );
        assert_eq!(
            step_width(Vec2::new(400.0, 0.0), usable, false).x,
            400.0,
            "stays at the bottom"
        );
        assert_eq!(step_height(cur, usable, true).y, 600.0);
        assert_eq!(
            step_height(cur, usable, false).y,
            450.0,
            "already at the bottom preset"
        );
        // an off-preset size rounds to the nearest side
        assert_eq!(step_width(Vec2::new(500.0, 0.0), usable, true).x, 600.0);
        assert_eq!(step_width(Vec2::new(500.0, 0.0), usable, false).x, 400.0);
    }

    #[test]
    fn lasso_picks_screens_whose_centre_is_inside_regardless_of_corner_order() {
        let v = view();
        let centres = [
            (ScreenId(1), Vec3::new(0.0, 0.0, -3.0)), // projects to (500, 500)
            (ScreenId(2), Vec3::new(1.0, 0.0, -3.0)), // well to the right
            (ScreenId(3), Vec3::new(5.0, 5.0, -3.0)), // off to the side
        ];
        let mut got = lasso(
            &v,
            Vec2::new(400.0, 400.0),
            Vec2::new(600.0, 600.0),
            &centres,
        );
        got.sort();
        assert_eq!(got, vec![ScreenId(1)]);
        // swapped corners: the same box
        let mut got2 = lasso(
            &v,
            Vec2::new(600.0, 600.0),
            Vec2::new(400.0, 400.0),
            &centres,
        );
        got2.sort();
        assert_eq!(got2, got);
        // a box wide enough also catches the second screen
        let mut got3 = lasso(
            &v,
            Vec2::new(300.0, 300.0),
            Vec2::new(900.0, 700.0),
            &centres,
        );
        got3.sort();
        assert_eq!(got3, vec![ScreenId(1), ScreenId(2)]);
    }

    #[test]
    fn group_translate_moves_every_pose_by_the_same_delta() {
        let poses = [(1u32, pose_at(-2.0)), (2u32, pose_at(-3.0))];
        let out = group_translate(&poses, Vec3::new(1.0, 2.0, 3.0));
        assert_eq!(out[0].1.pos, Vec3::new(1.0, 2.0, 1.0));
        assert_eq!(out[1].1.pos, Vec3::new(1.0, 2.0, 0.0));
        assert_eq!(out[0].1.yaw, poses[0].1.yaw);
    }

    #[test]
    fn group_rotate_spins_around_the_centre_as_a_rigid_body() {
        let poses = [
            (1u32, AirPose::new(Vec3::new(-1.0, 0.0, 0.0), 0.0)),
            (2u32, AirPose::new(Vec3::new(1.0, 0.0, 0.0), 0.0)),
        ];
        let out = group_rotate(&poses, FRAC_PI_2);
        // the centre (origin) does not move; distances from it are kept
        for ((_, before), (_, after)) in poses.iter().zip(out.iter()) {
            assert!(close(before.pos.length(), after.pos.length(), 1e-4));
            assert!(close(after.yaw, before.yaw + FRAC_PI_2, 1e-5));
        }
        // the two screens are still on opposite sides of the centre
        assert!(close3(out[0].1.pos, -out[1].1.pos, 1e-4));
        assert!(!close3(out[0].1.pos, poses[0].1.pos, 1e-3), "it did rotate");
    }

    #[test]
    fn group_rotate_of_nothing_is_nothing() {
        let out: Vec<(u32, AirPose)> = group_rotate(&[], 1.0);
        assert!(out.is_empty());
    }

    #[test]
    fn overlap_table() {
        let a = pose_at(-2.0);
        let size = Vec2::new(1.0, 1.0);
        // identical: full coverage
        assert!(close(overlap((&a, size), (&a, size)), 1.0, 1e-4));
        // shifted by half its width: a quarter of the area
        let shifted = AirPose {
            pos: a.pos + Vec3::new(0.5, 0.5, 0.0),
            ..a
        };
        assert!(close(overlap((&a, size), (&shifted, size)), 0.25, 1e-3));
        // far away: no overlap
        let far = AirPose {
            pos: a.pos + Vec3::new(10.0, 0.0, 0.0),
            ..a
        };
        assert_eq!(overlap((&a, size), (&far, size)), 0.0);
        // perpendicular (90° apart): not "facing" within 30°, so 0 regardless of position
        let perp = AirPose {
            yaw: FRAC_PI_2,
            ..a
        };
        assert_eq!(overlap((&a, size), (&perp, size)), 0.0);
    }

    #[test]
    fn free_spot_moves_away_from_a_full_overlap() {
        let pose = pose_at(-2.0);
        let size = Vec2::new(1.0, 1.0);
        let others = [(pose, size)];
        let moved = free_spot(&pose, size, &others, &[], 1.0);
        assert_ne!(moved.pos, pose.pos);
        assert!(overlap((&moved, size), (&pose, size)) <= OVERLAP_PUSH);
    }

    #[test]
    fn free_spot_avoids_an_obstacle_underneath() {
        let pose = pose_at(-2.0);
        let size = Vec2::new(1.0, 1.0);
        let obstacle = crate::Aabb::new(Vec2::new(-1.0, -3.0), Vec2::new(1.0, -1.0));
        assert!(obstacle.contains(Vec2::new(pose.pos.x, pose.pos.z)));
        let moved = free_spot(&pose, size, &[], &[obstacle], 1.0);
        assert!(!obstacle.contains(Vec2::new(moved.pos.x, moved.pos.z)));
    }

    #[test]
    fn free_spot_is_a_no_op_when_nothing_is_in_the_way() {
        let pose = pose_at(-2.0);
        let size = Vec2::new(1.0, 1.0);
        let far = AirPose {
            pos: pose.pos + Vec3::new(10.0, 0.0, 0.0),
            ..pose
        };
        let moved = free_spot(&pose, size, &[(far, size)], &[], 1.0);
        assert_eq!(moved.pos, pose.pos);
    }

    #[test]
    fn live_configure_rate_limits_and_never_repeats() {
        let mut lc = LiveConfigure::default();
        // the very first offer always goes out
        assert_eq!(
            lc.offer(Vec2::new(100.0, 100.0), 0.0),
            Some(Vec2::new(100.0, 100.0))
        );
        // the same size again: nothing to send
        assert_eq!(lc.offer(Vec2::new(100.0, 100.0), 0.01), None);
        // a different size too soon (< 1/LIVE_CONFIGURE_HZ s): deferred
        assert_eq!(lc.offer(Vec2::new(200.0, 100.0), 0.02), None);
        let min_dt = 1.0 / LIVE_CONFIGURE_HZ;
        assert_eq!(lc.tick(0.02), None, "still too soon");
        // once the slot opens, the pending size goes out
        assert_eq!(lc.tick(min_dt + 0.001), Some(Vec2::new(200.0, 100.0)));
        assert_eq!(lc.tick(min_dt + 0.002), None, "nothing left pending");
    }

    #[test]
    fn live_configure_finish_flushes_whatever_was_deferred() {
        let mut lc = LiveConfigure::default();
        lc.offer(Vec2::new(100.0, 100.0), 0.0);
        lc.offer(Vec2::new(150.0, 100.0), 0.0); // too soon: pending
        assert_eq!(lc.finish(), Some(Vec2::new(150.0, 100.0)));
        assert_eq!(lc.finish(), None, "nothing left to flush");
    }
}
