//! Where a new, empty frame lands when `N`/`ctrl+n` fires a ray from the
//! mira (plan §2.2, "uma moldura vazia aparece onde a mira cruza o mundo
//! (parede, mesa ou ar a 1,2 m, o primeiro que servir), virada para você"):
//! a wall first, then a desk's top, then [`FALLBACK_M`] straight ahead as
//! the last resort. Pure; `nkw`'s `make_frame` feeds it the current room's
//! geometry.
//!
//! Walls and desk tops are modelled the same way screens are elsewhere in
//! this crate: a unit quad (x, y in -0.5..0.5, z = 0) placed by a model
//! matrix, hit-tested by [`crate::collide::ray_hit_quad`] — the same
//! primitive `arrange::AirPose::model`'s screens and `nkw`'s `wm::walk`
//! already use for their own ray casts.

use glam::{Mat4, Vec2, Vec3};

use crate::carry::{self, WALL_GAP};
use crate::collide::ray_hit_quad;
use crate::layout::DESK_TOP;
use crate::pose::Pose;

/// How far ahead of the camera an unaimed frame floats, metres (plan §2.2):
/// the same distance `carry::HAND_DEFAULT` uses for the screen in hand.
pub const FALLBACK_M: f32 = carry::HAND_DEFAULT;

/// A wall, as a vertical quad: its floor segment `a`-`b` (the
/// `carry::WallSeg` convention: `normal` points into the room) and its
/// height.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WallQuad {
    pub a: Vec2,
    pub b: Vec2,
    pub normal: Vec2,
    pub height: f32,
}

impl WallQuad {
    /// The screen-convention yaw of a frame hung flush on this wall: facing
    /// into the room (`carry::drop_target`'s own formula).
    fn yaw(&self) -> f32 {
        (-self.normal.x).atan2(self.normal.y)
    }

    fn model(&self) -> Mat4 {
        let mid = (self.a + self.b) / 2.0;
        Mat4::from_translation(Vec3::new(mid.x, self.height / 2.0, mid.y))
            * Mat4::from_rotation_y(-self.yaw())
            * Mat4::from_scale(Vec3::new(self.a.distance(self.b), self.height, 1.0))
    }

    /// Where a frame lands at ray parameter `t`: the hit point nudged
    /// `WALL_GAP` off the wall (the same clearance a dropped screen keeps),
    /// facing into the room.
    fn landing(&self, origin: Vec3, dir: Vec3, t: f32) -> Pose {
        let hit = origin + dir * t;
        let off = Vec3::new(self.normal.x, 0.0, self.normal.y) * WALL_GAP;
        Pose::new(hit + off, self.yaw(), 0.0)
    }
}

/// A desk's top, as a horizontal quad: its floor centre (the
/// `carry::DeskSlot` convention, y = 0), the screen-convention yaw a screen
/// sitting on it would face, and its footprint (along the wall, then out
/// from it).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DeskQuad {
    pub center: Vec3,
    pub yaw: f32,
    pub size: Vec2,
}

impl DeskQuad {
    fn model(&self) -> Mat4 {
        let top = self.center + Vec3::Y * DESK_TOP;
        // a horizontal quad: tip the unit quad's face (+z) up to +y first,
        // then turn it to the desk's yaw
        Mat4::from_translation(top)
            * Mat4::from_rotation_y(-self.yaw)
            * Mat4::from_rotation_x(-std::f32::consts::FRAC_PI_2)
            * Mat4::from_scale(self.size.extend(1.0))
    }

    fn landing(&self, origin: Vec3, dir: Vec3, t: f32) -> Pose {
        Pose::new(origin + dir * t, self.yaw, 0.0)
    }
}

/// The nearest of `quads`' hits along `(origin, dir)`, by whatever ray-cast
/// `hit` returns (`Some(t)`/`None`), and its index.
fn nearest<Q>(
    origin: Vec3,
    dir: Vec3,
    quads: &[Q],
    model: impl Fn(&Q) -> Mat4,
) -> Option<(f32, usize)> {
    quads
        .iter()
        .enumerate()
        .filter_map(|(i, q)| ray_hit_quad(origin, dir, model(q)).map(|t| (t, i)))
        .min_by(|a, b| a.0.total_cmp(&b.0))
}

/// Where a new frame spawns, in the plan's order (§2.2): the nearest wall
/// the mira (`cam`'s eye and forward) crosses, else the nearest desk top,
/// else [`FALLBACK_M`] straight ahead (`carry::hand_pose`, so it matches the
/// screen-in-hand convention exactly: level, facing the camera).
pub fn frame_point(cam: &Pose, walls: &[WallQuad], desks: &[DeskQuad]) -> Pose {
    let (origin, dir) = (cam.pos, cam.forward());
    if let Some((t, i)) = nearest(origin, dir, walls, WallQuad::model) {
        return walls[i].landing(origin, dir, t);
    }
    if let Some((t, i)) = nearest(origin, dir, desks, DeskQuad::model) {
        return desks[i].landing(origin, dir, t);
    }
    carry::hand_pose(cam, FALLBACK_M)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wall(a: Vec2, b: Vec2, normal: Vec2) -> WallQuad {
        WallQuad {
            a,
            b,
            normal,
            height: 2.8,
        }
    }

    fn desk(center: Vec3, yaw: f32) -> DeskQuad {
        DeskQuad {
            center,
            yaw,
            size: Vec2::new(0.9, 0.8),
        }
    }

    fn close(a: Vec3, b: Vec3, eps: f32) -> bool {
        (a - b).length() <= eps
    }

    #[test]
    fn frame_point_table() {
        // looking dead ahead (yaw 0, no pitch) at a back wall 5 m away
        let level = Pose::new(Vec3::new(0.0, 1.5, 0.0), 0.0, 0.0);
        let back_wall = wall(
            Vec2::new(-3.0, -5.0),
            Vec2::new(3.0, -5.0),
            Vec2::new(0.0, 1.0),
        );

        // hits the wall: lands just off it, facing back into the room
        let p = frame_point(&level, &[back_wall], &[]);
        assert!(
            close(p.pos, Vec3::new(0.0, 1.5, -5.0 + WALL_GAP), 1e-4),
            "{p:?}"
        );
        assert!((p.yaw - 0.0).abs() < 1e-5);
        assert_eq!(p.pitch, 0.0);

        // looking down at a desk top, no wall in reach (it's behind): lands
        // exactly where the ray crosses the desk's top
        let pitch = (-0.2f32).atan();
        let tilted = Pose::new(Vec3::new(0.0, 1.5, 0.0), 0.0, pitch);
        let dir = tilted.forward();
        let t_desk = (tilted.pos.y - DESK_TOP) / -dir.y;
        let desk_hit = tilted.pos + dir * t_desk;
        let d = desk(Vec3::new(desk_hit.x, 0.0, desk_hit.z), 0.0);
        let behind_wall = wall(
            Vec2::new(-3.0, 5.0),
            Vec2::new(3.0, 5.0),
            Vec2::new(0.0, -1.0),
        );
        let p = frame_point(&tilted, &[behind_wall], &[d]);
        assert!(close(p.pos, desk_hit, 1e-4), "{p:?} vs {desk_hit:?}");
        assert_eq!(p.yaw, 0.0);

        // nothing in the way at all: the 1.2 m fallback, level, facing back
        let p = frame_point(&level, &[], &[]);
        let want = carry::hand_pose(&level, FALLBACK_M);
        assert_eq!(p, want);
        assert!(
            close(p.pos, Vec3::new(0.0, 1.5, -FALLBACK_M), 1e-4),
            "{p:?}"
        );

        // the same downward ray also crosses the back wall, farther along —
        // put a desk right in its path, nearer than the wall, and check the
        // wall still wins: the plan's order is a category, not "whichever
        // is closer"
        let t_wall = 5.0 / pitch.cos();
        assert!(t_desk < t_wall, "the desk must be the nearer hit");
        let near_desk = desk(Vec3::new(desk_hit.x, 0.0, desk_hit.z), 0.0);
        let p = frame_point(&tilted, &[back_wall], &[near_desk]);
        let want = back_wall.landing(tilted.pos, dir, t_wall);
        assert!(close(p.pos, want.pos, 1e-3), "{p:?} vs {want:?}");
        assert!(
            (p.yaw - 0.0).abs() < 1e-5,
            "landed on the wall, not the desk"
        );

        // a ray that crosses neither (looking straight up) falls back
        let up = Pose::new(Vec3::new(10.0, 1.5, 10.0), 0.0, 1.5);
        let p = frame_point(&up, &[back_wall], &[near_desk]);
        assert_eq!(p, carry::hand_pose(&up, FALLBACK_M));
    }
}
