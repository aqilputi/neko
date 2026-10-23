//! Carrying a screen in ANDAR (§3.5 "Carregar uma tela"), pure: where the
//! screen in hand floats and where it lands when dropped.

use glam::{Vec2, Vec3};

use crate::ids::RoomId;
use crate::pose::Pose;

/// Hand distance range and default, metres.
pub const HAND_MIN: f32 = 0.8;
pub const HAND_MAX: f32 = 3.0;
pub const HAND_DEFAULT: f32 = 1.2;
/// `[` / `]` step, metres.
pub const HAND_STEP: f32 = 0.2;
/// A desk slot closer than this takes the screen into its pile.
pub const SNAP_DESK_M: f32 = 0.6;
/// A wall closer than this and aligned within [`SNAP_WALL_DEG`] takes it.
pub const SNAP_WALL_M: f32 = 0.4;
pub const SNAP_WALL_DEG: f32 = 15.0;
/// A screen on a wall hangs this far off it, metres.
pub const WALL_GAP: f32 = 0.05;
/// Wall positions snap to this grid, metres.
pub const WALL_GRID: f32 = 0.25;

/// A desk that can take a screen: its stack centre.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DeskSlot {
    pub room: RoomId,
    pub desk: usize,
    pub pos: Vec3,
}

/// A wall segment on the floor plane (x, z), its normal into the room.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WallSeg {
    pub a: Vec2,
    pub b: Vec2,
    pub normal: Vec2,
}

/// Where a dropped screen goes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Drop {
    Desk { room: RoomId, desk: usize },
    Wall(Pose),
    Air(Pose),
}

/// The screen in hand: `dist` ahead of the camera at eye height, facing the
/// camera (the screen pose's yaw is the camera's: the viewer convention of
/// `ScreenPlace`), pitch 0, no roll.
pub fn hand_pose(cam: &Pose, dist: f32) -> Pose {
    let h = cam.heading() * dist;
    Pose::new(cam.pos + Vec3::new(h.x, 0.0, h.y), cam.yaw, 0.0)
}

/// The first drop rule that fits, in the plan's order: a desk slot within
/// [`SNAP_DESK_M`] (the nearest; the first on a tie), a wall within
/// [`SNAP_WALL_M`] that the screen faces away from within [`SNAP_WALL_DEG`]
/// (5 cm off it, on the 0.25 m grid along it and in height), else the air.
pub fn drop_target(hand: &Pose, desks: &[DeskSlot], walls: &[WallSeg]) -> Drop {
    let near_desk = desks
        .iter()
        .map(|d| (d.pos.distance(hand.pos), d))
        .filter(|(dist, _)| *dist <= SNAP_DESK_M + 1e-5)
        .fold(None::<(f32, &DeskSlot)>, |best, (dist, d)| match best {
            Some((b, _)) if b <= dist + 1e-6 => best,
            _ => Some((dist, d)),
        });
    if let Some((_, d)) = near_desk {
        return Drop::Desk {
            room: d.room,
            desk: d.desk,
        };
    }
    let p = Vec2::new(hand.pos.x, hand.pos.z);
    // the screen's face looks back at whoever holds it
    let front = -hand.heading();
    let max_off = SNAP_WALL_DEG.to_radians().cos();
    let mut best: Option<(f32, Pose)> = None;
    for w in walls {
        let along = w.b - w.a;
        let len = along.length();
        if len < 1e-6 {
            continue;
        }
        let u = along / len;
        let s = (p - w.a).dot(u);
        let off = (p - w.a).dot(w.normal);
        if !(0.0..=len).contains(&s) || !(-WALL_GAP..=SNAP_WALL_M).contains(&off) {
            continue;
        }
        if front.dot(w.normal) < max_off - 1e-6 {
            continue;
        }
        let grid = |v: f32| (v / WALL_GRID).round() * WALL_GRID;
        let at = w.a + u * grid(s).clamp(0.0, len) + w.normal * WALL_GAP;
        let yaw = (-w.normal.x).atan2(w.normal.y);
        let pose = Pose::new(Vec3::new(at.x, grid(hand.pos.y), at.y), yaw, 0.0);
        if best.is_none_or(|(b, _)| off.abs() < b) {
            best = Some((off.abs(), pose));
        }
    }
    match best {
        Some((_, pose)) => Drop::Wall(pose),
        None => Drop::Air(*hand),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pose::angle_diff;

    fn cam(x: f32, z: f32, yaw_deg: f32, pitch_deg: f32) -> Pose {
        Pose::new(
            Vec3::new(x, 1.5, z),
            yaw_deg.to_radians(),
            pitch_deg.to_radians(),
        )
    }

    #[test]
    fn the_hand_is_ahead_level_and_facing_you() {
        for (yaw, pitch) in [(0.0, 0.0), (90.0, 20.0), (-135.0, -40.0), (179.0, 55.0)] {
            let c = cam(1.0, 2.0, yaw, pitch);
            let h = hand_pose(&c, HAND_DEFAULT);
            assert_eq!(h.pitch, 0.0, "no pitch");
            assert!(angle_diff(h.yaw, c.yaw).abs() < 1e-4, "faces the camera");
            assert!((h.pos.y - c.pos.y).abs() < 1e-6, "eye height");
            let d = Vec2::new(h.pos.x - c.pos.x, h.pos.z - c.pos.z);
            assert!((d.length() - HAND_DEFAULT).abs() < 1e-4);
            assert!(d.normalize().dot(c.heading()) > 0.9999, "ahead");
        }
    }

    /// A 6 × 6 m room from (0, -6) to (6, 0); the back wall at z = -6.
    fn walls() -> Vec<WallSeg> {
        vec![
            WallSeg {
                a: Vec2::new(0.0, -6.0),
                b: Vec2::new(6.0, -6.0),
                normal: Vec2::new(0.0, 1.0),
            },
            WallSeg {
                a: Vec2::new(0.0, -6.0),
                b: Vec2::new(0.0, 0.0),
                normal: Vec2::new(1.0, 0.0),
            },
        ]
    }

    fn desk(d: usize, x: f32, z: f32) -> DeskSlot {
        DeskSlot {
            room: RoomId(3),
            desk: d,
            pos: Vec3::new(x, 1.5, z),
        }
    }

    fn at(x: f32, z: f32, yaw: f32) -> Pose {
        Pose::new(Vec3::new(x, 1.5, z), yaw.to_radians(), 0.0)
    }

    #[test]
    fn drop_rules_in_the_plans_order() {
        let desks = [desk(0, 1.0, -5.0), desk(1, 3.0, -5.0)];
        let walls = walls();
        // 0.4 m from desk 1 and 0.6 m from the back wall: the desk wins
        assert_eq!(
            drop_target(&at(3.0, -5.4, 0.0), &desks, &walls),
            Drop::Desk {
                room: RoomId(3),
                desk: 1
            }
        );
        // a tie at 0.5 m: the first desk
        let tie = [desk(0, 1.5, -5.0), desk(1, 2.5, -5.0)];
        assert_eq!(
            drop_target(&at(2.0, -5.0, 0.0), &tie, &walls),
            Drop::Desk {
                room: RoomId(3),
                desk: 0
            }
        );
        // near the back wall, facing it within 15°: on it, 5 cm off, on the grid
        let Drop::Wall(p) = drop_target(&at(4.1, -5.7, 10.0), &desks, &walls) else {
            panic!("wall");
        };
        assert!((p.pos.z - (-6.0 + WALL_GAP)).abs() < 1e-5, "{p:?}");
        assert!((p.pos.x - 4.0).abs() < 1e-5, "snapped along: {p:?}");
        assert!((p.pos.y - 1.5).abs() < 1e-5, "snapped up: {p:?}");
        assert!(p.yaw.abs() < 1e-5, "flat on the wall, facing the room");
        assert_eq!(p.pitch, 0.0);
        // turned 20° away from it: in the air
        assert_eq!(
            drop_target(&at(4.1, -5.7, 20.0), &desks, &walls),
            Drop::Air(at(4.1, -5.7, 20.0))
        );
        // the side wall (x = 0), facing it: yaw −90°
        let Drop::Wall(p) = drop_target(&at(0.3, -2.6, -95.0), &desks, &walls) else {
            panic!("side wall");
        };
        assert!((p.pos.x - WALL_GAP).abs() < 1e-5);
        assert!((p.pos.z + 2.5).abs() < 1e-5, "{p:?}");
        assert!(angle_diff(p.yaw, (-90f32).to_radians()).abs() < 1e-5);
        // the middle of the room: in the air, as it is
        let mid = at(3.0, -3.0, 0.0);
        assert_eq!(drop_target(&mid, &desks, &walls), Drop::Air(mid));
        // past the end of a wall segment does not count
        let past = at(7.0, -5.8, 0.0);
        assert_eq!(drop_target(&past, &[], &walls), Drop::Air(past));
    }
}
