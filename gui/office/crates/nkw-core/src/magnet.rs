//! Snapping on release (and on ARRUMAR's `g`), with the guides to draw
//! (plan §2.8): dock > desk > wall > cluster > edge > 1:1 > eye > grid > air.

use glam::{Quat, Vec2, Vec3};

use crate::arrange::AirPose;
use crate::dock::DockSlot;
use crate::ids::{RoomId, ScreenId};
use crate::pose::Pose;

/// How hard the magnets pull: `Off` (`x`, `ctrl` pressed again), `Soft`
/// (the default), `Strong` (`shift`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Strength {
    Off,
    #[default]
    Soft,
    Strong,
}

pub const SOFT_WALL_M: f32 = 0.08;
pub const SOFT_WALL_DEG: f32 = 8.0;
pub const SOFT_EDGE_M: f32 = 0.02;
pub const SOFT_EDGE_DEG: f32 = 5.0;
pub const STRONG_EDGE_M: f32 = 0.04;
pub const GRID_M: f32 = 0.25;
pub const GRID_DEG: f32 = 15.0;
pub const EYE_M: f32 = 0.03;
/// desks: carry::SNAP_DESK_M; wall gap: carry::WALL_GAP
pub const ONE_TO_ONE_M: f32 = 0.02;

/// What the screen can snap to.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Targets {
    pub walls: Vec<crate::carry::WallSeg>,
    pub desks: Vec<crate::carry::DeskSlot>,
    /// the others in the air (edges; cluster on Strong): id, pose, size m
    pub screens: Vec<(ScreenId, AirPose, Vec2)>,
    pub eye: Pose,
    /// the scale-1 reading distance; the pose's scale multiplies it
    pub one_to_one_m: f32,
    /// the pointer is on a dock edge (the caller found it in 2D)
    pub dock: Option<DockSlot>,
    /// the room's origin (the grid's)
    pub grid_origin: Vec3,
    pub eye_height: f32,
}

/// Where a release lands.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Snap {
    Air,
    Dock(DockSlot),
    Desk { room: RoomId, desk: usize },
    Wall(Pose),
    Cluster(ScreenId),
    Edge(ScreenId),
    OneToOne,
    Eye,
    Grid,
}

/// What the overlay draws.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Guide {
    /// dashed line from the face's bottom centre down, "1,42 m"
    Floor {
        from: Vec3,
        height_m: f32,
    },
    /// "1,2 m"
    Depth {
        at: Vec3,
        m: f32,
    },
    /// where it will land
    Ghost {
        pose: AirPose,
        size_m: Vec2,
    },
    WallShadow {
        pose: Pose,
        size_m: Vec2,
    },
    DeskSlot {
        room: RoomId,
        desk: usize,
        at: Vec3,
    },
    /// "alinhado"
    EdgeLine {
        a: Vec3,
        b: Vec3,
    },
    GridDots {
        centre: Vec3,
        yaw: f32,
    },
    Horizon {
        y: f32,
    },
    /// the ring on the rim + "1:1"
    OneToOne,
    DockSlot(DockSlot),
    ClusterRing(ScreenId),
}

/// `pose`'s own basis: right, up, face normal (world space); same
/// convention as `arrange::AirPose::model`.
fn basis(pose: &AirPose) -> (Vec3, Vec3, Vec3) {
    let rot = Quat::from_rotation_y(-pose.yaw) * Quat::from_rotation_x(pose.pitch);
    (rot * Vec3::X, rot * Vec3::Y, rot * Vec3::Z)
}

/// Floor + Depth, always drawn (even `Off`): the dashed line to the floor
/// and the distance along the eye's own forward axis.
fn base_guides(pose: &AirPose, eye: &Pose) -> Vec<Guide> {
    vec![
        Guide::Floor {
            from: Vec3::new(pose.pos.x, 0.0, pose.pos.z),
            height_m: pose.pos.y,
        },
        Guide::Depth {
            at: pose.pos,
            m: (pose.pos - eye.pos).dot(eye.forward()),
        },
    ]
}

/// The wall `pose` fits against, within `tol_m`/`tol_deg`: the first wall
/// segment it is near and roughly facing away from (same rule as
/// `carry::drop_target`, with the magnet's own, tighter tolerance). The
/// along-wall position snaps to `carry::WALL_GRID`, the gap is
/// `carry::WALL_GAP`; the height is kept as is.
fn wall_fit(
    pose: &AirPose,
    walls: &[crate::carry::WallSeg],
    tol_m: f32,
    tol_deg: f32,
) -> Option<Pose> {
    use crate::carry::{WALL_GAP, WALL_GRID};
    let p = Vec2::new(pose.pos.x, pose.pos.z);
    // the screen's own face normal, on the floor plane (ScreenPlace's
    // convention: it faces whoever looks at it with this yaw)
    let facing = -Vec2::new(pose.yaw.sin(), -pose.yaw.cos());
    let max_off = tol_deg.to_radians().cos();
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
        if !(0.0..=len).contains(&s) || !(-WALL_GAP..=tol_m).contains(&off) {
            continue;
        }
        if facing.dot(w.normal) < max_off - 1e-6 {
            continue;
        }
        let grid = |v: f32| (v / WALL_GRID).round() * WALL_GRID;
        let at = w.a + u * grid(s).clamp(0.0, len) + w.normal * WALL_GAP;
        let yaw = (-w.normal.x).atan2(w.normal.y);
        let wall_pose = Pose::new(Vec3::new(at.x, pose.pos.y, at.y), yaw, 0.0);
        if best.is_none_or(|(b, _)| off.abs() < b) {
            best = Some((off.abs(), wall_pose));
        }
    }
    best.map(|(_, p)| p)
}

/// Whether `pose`'s centre lands on `other`'s face (co-planar within 30°,
/// the same tolerance `arrange::overlap` uses): "soltar em cima".
fn on_top_of(pose: &AirPose, other: &AirPose, other_size: Vec2) -> bool {
    let (_, _, normal) = basis(pose);
    let (right, up, other_normal) = basis(other);
    if normal.dot(other_normal) < 30f32.to_radians().cos() {
        return false;
    }
    let rel = pose.pos - other.pos;
    let half = other_size / 2.0;
    rel.dot(right).abs() <= half.x && rel.dot(up).abs() <= half.y
}

/// One axis's best edge/centre alignment against `other`: `None` outside
/// `tol`. Candidates: the two centres coincide on this axis, or `pose`'s
/// near edge touches `other`'s far one (either side).
fn axis_edge_fit(
    my_half: f32,
    my_val: f32,
    other_half: f32,
    other_val: f32,
    tol: f32,
) -> Option<f32> {
    [
        other_val,                        // same centre
        other_val - other_half - my_half, // touching, pose on the near side
        other_val + other_half + my_half, // touching, pose on the far side
    ]
    .into_iter()
    .map(|target| (target, (target - my_val).abs()))
    .filter(|(_, d)| *d <= tol)
    .min_by(|a, b| a.1.total_cmp(&b.1))
    .map(|(target, _)| target)
}

/// The edge/centre snap of `pose` against any of `screens`, within `tol`
/// (co-planar within `SOFT_EDGE_DEG`): the id, the new centre, and the two
/// points the "alinhado" guide connects.
///
/// Each of the two local axes (right, up) is matched independently, against
/// the best-fitting screen for *that* axis: an axis that already happens to
/// coincide (distance 0, matching trivially) must never hide a real
/// correction the other axis needs — so axes are never compared against
/// each other, only within themselves. Both axes apply if both match; the
/// reported id/guide favour whichever one actually moved the pose.
fn edge_fit(
    pose: &AirPose,
    size_m: Vec2,
    screens: &[(ScreenId, AirPose, Vec2)],
    tol: f32,
) -> Option<(ScreenId, Vec3, Vec3, Vec3)> {
    let (right, up, normal) = basis(pose);
    let half = size_m / 2.0;
    let max_off = SOFT_EDGE_DEG.to_radians().cos();
    let axes = [(right, half.x), (up, half.y)];
    // per axis: (|correction|, the new coordinate, the matched screen, its centre)
    let mut best_per_axis: [Option<(f32, f32, ScreenId, Vec3)>; 2] = [None, None];
    for (id, other, other_size) in screens {
        let (_, _, other_normal) = basis(other);
        if normal.dot(other_normal) < max_off {
            continue;
        }
        let rel = other.pos - pose.pos;
        let other_half = *other_size / 2.0;
        let vals = [rel.dot(right), rel.dot(up)];
        let other_halves = [other_half.x, other_half.y];
        for (axis, slot) in best_per_axis.iter_mut().enumerate() {
            let (_, my_half) = axes[axis];
            let Some(target) = axis_edge_fit(my_half, 0.0, other_halves[axis], vals[axis], tol)
            else {
                continue;
            };
            let d = target.abs();
            if slot.is_none_or(|(b, ..)| d < b) {
                *slot = Some((d, target, *id, other.pos));
            }
        }
    }
    if best_per_axis.iter().all(Option::is_none) {
        return None;
    }
    let mut new_pos = pose.pos;
    for (axis, (dir, _)) in axes.iter().enumerate() {
        if let Some((_, target, _, _)) = best_per_axis[axis] {
            new_pos += *dir * target;
        }
    }
    let (_, _, id, other_pos) = best_per_axis
        .into_iter()
        .flatten()
        .max_by(|a, b| a.0.total_cmp(&b.0))
        .expect("at least one axis matched");
    Some((id, new_pos, pose.pos, other_pos))
}

/// The first rule that fits, in this order: Dock > Desk > Wall > Cluster
/// (Strong, over another) > Edge > OneToOne > Eye > Grid (Strong only) >
/// Air; the snapped pose and the guides (always Floor + Depth + Ghost).
/// `Off`: the pose as is, `Snap::Air`, guides Floor + Depth only.
pub fn snap(pose: &AirPose, size_m: Vec2, t: &Targets, s: Strength) -> (AirPose, Snap, Vec<Guide>) {
    if s == Strength::Off {
        return (*pose, Snap::Air, base_guides(pose, &t.eye));
    }
    let mut guides = base_guides(pose, &t.eye);
    guides.push(Guide::Ghost {
        pose: *pose,
        size_m,
    });

    // 1. Dock: the caller already found the edge zone in 2D.
    if let Some(slot) = t.dock {
        guides.push(Guide::DockSlot(slot));
        return (*pose, Snap::Dock(slot), guides);
    }

    // 2. Desk: within carry::SNAP_DESK_M of a slot's centre.
    if let Some(d) = t
        .desks
        .iter()
        .map(|d| (pose.pos.distance(d.pos), d))
        .filter(|(dist, _)| *dist <= crate::carry::SNAP_DESK_M + 1e-5)
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, d)| d)
    {
        guides.push(Guide::DeskSlot {
            room: d.room,
            desk: d.desk,
            at: d.pos,
        });
        let snapped = AirPose {
            pos: d.pos,
            ..*pose
        };
        return (
            snapped,
            Snap::Desk {
                room: d.room,
                desk: d.desk,
            },
            guides,
        );
    }

    // 3. Wall: same tolerance regardless of strength (only Edge and Grid
    // widen under Strong).
    if let Some(wp) = wall_fit(pose, &t.walls, SOFT_WALL_M, SOFT_WALL_DEG) {
        guides.push(Guide::WallShadow { pose: wp, size_m });
        let snapped = AirPose {
            pos: wp.pos,
            yaw: wp.yaw,
            pitch: 0.0,
            scale: pose.scale,
        };
        return (snapped, Snap::Wall(wp), guides);
    }

    // 4. Cluster: only with Strong, dropped on top of another screen.
    if s == Strength::Strong
        && let Some((id, _)) = t
            .screens
            .iter()
            .filter(|(_, other, sz)| on_top_of(pose, other, *sz))
            .map(|(id, other, _)| (*id, pose.pos.distance(other.pos)))
            .min_by(|a, b| a.1.total_cmp(&b.1))
    {
        guides.push(Guide::ClusterRing(id));
        return (*pose, Snap::Cluster(id), guides);
    }

    // 5. Edge: another screen's edge or centre, co-planar.
    let edge_tol = if s == Strength::Strong {
        STRONG_EDGE_M
    } else {
        SOFT_EDGE_M
    };
    if let Some((id, new_pos, a, b)) = edge_fit(pose, size_m, &t.screens, edge_tol) {
        guides.push(Guide::EdgeLine { a, b });
        let snapped = AirPose {
            pos: new_pos,
            ..*pose
        };
        return (snapped, Snap::Edge(id), guides);
    }

    // 6. The 1:1 reading distance (scaled).
    let want_1to1 = t.one_to_one_m * pose.scale;
    let dist = (pose.pos - t.eye.pos).length();
    if (dist - want_1to1).abs() <= ONE_TO_ONE_M {
        let dir = if dist > 1e-6 {
            (pose.pos - t.eye.pos) / dist
        } else {
            t.eye.forward()
        };
        guides.push(Guide::OneToOne);
        let snapped = AirPose {
            pos: t.eye.pos + dir * want_1to1,
            ..*pose
        };
        return (snapped, Snap::OneToOne, guides);
    }

    // 7. Eye height.
    if (pose.pos.y - t.eye_height).abs() <= EYE_M {
        guides.push(Guide::Horizon { y: t.eye_height });
        let snapped = AirPose {
            pos: Vec3::new(pose.pos.x, t.eye_height, pose.pos.z),
            ..*pose
        };
        return (snapped, Snap::Eye, guides);
    }

    // 8. Grid: only Strong, and it always produces a value (no tolerance
    // gate): the room's own lattice, GRID_M apart, yaw to GRID_DEG.
    if s == Strength::Strong {
        let grid1 = |v: f32, origin: f32| origin + ((v - origin) / GRID_M).round() * GRID_M;
        let pos = Vec3::new(
            grid1(pose.pos.x, t.grid_origin.x),
            pose.pos.y,
            grid1(pose.pos.z, t.grid_origin.z),
        );
        let yaw = (pose.yaw.to_degrees() / GRID_DEG).round() * GRID_DEG;
        guides.push(Guide::GridDots {
            centre: pos,
            yaw: yaw.to_radians(),
        });
        let snapped = AirPose {
            pos,
            yaw: yaw.to_radians(),
            ..*pose
        };
        return (snapped, Snap::Grid, guides);
    }

    // 9. Nothing fit: the air.
    (*pose, Snap::Air, guides)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::carry::{DeskSlot, WallSeg};

    fn close(a: f32, b: f32, eps: f32) -> bool {
        (a - b).abs() <= eps
    }

    fn targets() -> Targets {
        Targets {
            eye: Pose::default(),
            // away from the usual 1.5 m test-screen height, so the eye
            // magnet (§2.8) does not interfere with unrelated tests
            eye_height: 1.37,
            one_to_one_m: 0.935,
            grid_origin: Vec3::ZERO,
            ..Targets::default()
        }
    }

    fn pose_at(pos: Vec3) -> AirPose {
        AirPose::new(pos, 0.0)
    }

    const SIZE: Vec2 = Vec2::new(1.0, 1.0);

    #[test]
    fn off_bypasses_every_rule_and_draws_only_the_base_guides() {
        let t = targets();
        let p = pose_at(Vec3::new(5.0, 5.0, 5.0));
        let (out, snap, guides) = snap(&p, SIZE, &t, Strength::Off);
        assert_eq!(out, p);
        assert_eq!(snap, Snap::Air);
        assert_eq!(guides.len(), 2, "Floor + Depth only");
    }

    #[test]
    fn dock_wins_over_everything_else() {
        let mut t = targets();
        t.dock = Some(DockSlot::TopLeft);
        t.desks = vec![DeskSlot {
            room: RoomId(1),
            desk: 0,
            pos: Vec3::ZERO,
        }];
        let p = pose_at(Vec3::new(0.01, 0.0, 0.0)); // also inside the desk's capture
        let (out, snap, guides) = snap(&p, SIZE, &t, Strength::Soft);
        assert_eq!(out, p, "a dock does not move the pose");
        assert_eq!(snap, Snap::Dock(DockSlot::TopLeft));
        assert!(guides.contains(&Guide::DockSlot(DockSlot::TopLeft)));
    }

    #[test]
    fn desk_snaps_to_the_slot_centre_within_its_radius() {
        let mut t = targets();
        let slot = DeskSlot {
            room: RoomId(2),
            desk: 1,
            pos: Vec3::new(2.0, 1.5, -3.0),
        };
        t.desks = vec![slot];
        let near = pose_at(slot.pos + Vec3::new(0.05, 0.0, 0.0));
        let (out, snap_kind, _) = snap(&near, SIZE, &t, Strength::Soft);
        assert_eq!(out.pos, slot.pos);
        assert_eq!(
            snap_kind,
            Snap::Desk {
                room: RoomId(2),
                desk: 1
            }
        );
        // just past the capture radius: falls through to the air
        let far = pose_at(slot.pos + Vec3::new(crate::carry::SNAP_DESK_M + 0.1, 0.0, 0.0));
        let (out, snap_kind, _) = snap(&far, SIZE, &t, Strength::Soft);
        assert_eq!(out, far);
        assert_eq!(snap_kind, Snap::Air);
    }

    fn north_wall() -> WallSeg {
        WallSeg {
            a: Vec2::new(0.0, -6.0),
            b: Vec2::new(6.0, -6.0),
            normal: Vec2::new(0.0, 1.0),
        }
    }

    #[test]
    fn wall_snaps_along_its_grid_and_sets_the_gap_and_yaw() {
        let mut t = targets();
        t.walls = vec![north_wall()];
        // yaw 0 faces +z here, i.e. away from the wall at z = -6: it fits
        let p = pose_at(Vec3::new(4.13, 1.5, -5.95));
        let (out, snap_kind, guides) = snap(&p, SIZE, &t, Strength::Soft);
        assert!(close(out.pos.x, 4.25, 1e-4), "snapped to the 0.25 m grid");
        assert!(close(out.pos.z, -6.0 + crate::carry::WALL_GAP, 1e-4));
        assert_eq!(out.pos.y, 1.5, "height is kept");
        assert_eq!(out.yaw, 0.0);
        assert!(matches!(snap_kind, Snap::Wall(_)));
        assert!(guides.iter().any(|g| matches!(g, Guide::WallShadow { .. })));
        // too far from the wall: no magnet
        let far = pose_at(Vec3::new(4.0, 1.5, -4.0));
        let (out, snap_kind, _) = snap(&far, SIZE, &t, Strength::Soft);
        assert_eq!(out, far);
        assert_eq!(snap_kind, Snap::Air);
    }

    #[test]
    fn cluster_only_fires_on_strong_when_dropped_on_top() {
        let mut t = targets();
        let other = pose_at(Vec3::new(3.0, 1.5, -3.0));
        t.screens = vec![(ScreenId(7), other, SIZE)];
        let p = pose_at(other.pos); // exactly on top of it
        let (out, snap_kind, guides) = snap(&p, SIZE, &t, Strength::Strong);
        assert_eq!(out, p, "clustering does not reposition the screen");
        assert_eq!(snap_kind, Snap::Cluster(ScreenId(7)));
        assert!(guides.contains(&Guide::ClusterRing(ScreenId(7))));
        // soft: clustering never triggers (it falls through to the edge
        // magnet instead, since the centres coincide exactly)
        let (_, snap_kind, _) = snap(&p, SIZE, &t, Strength::Soft);
        assert_eq!(snap_kind, Snap::Edge(ScreenId(7)));
    }

    #[test]
    fn edge_aligns_centres_within_tolerance() {
        let mut t = targets();
        let other = pose_at(Vec3::new(3.0, 1.5, -3.0));
        t.screens = vec![(ScreenId(5), other, SIZE)];
        // 1.5 cm off on x: within SOFT_EDGE_M (2 cm)
        let p = pose_at(other.pos + Vec3::new(0.015, 0.0, 0.0));
        let (out, snap_kind, guides) = snap(&p, SIZE, &t, Strength::Soft);
        assert!(close(out.pos.x, other.pos.x, 1e-5), "centres align");
        assert_eq!(out.pos.y, p.pos.y);
        assert_eq!(snap_kind, Snap::Edge(ScreenId(5)));
        assert!(guides.iter().any(|g| matches!(g, Guide::EdgeLine { .. })));
        // 3 cm off on x and 20 cm off on y: past tolerance on both axes
        // (the y offset also keeps it from trivially matching there)
        let too_far = pose_at(other.pos + Vec3::new(0.03, 0.2, 0.0));
        let (out, snap_kind, _) = snap(&too_far, SIZE, &t, Strength::Soft);
        assert_eq!(out, too_far);
        assert_eq!(snap_kind, Snap::Air);
    }

    #[test]
    fn edge_touches_a_neighbours_border_too() {
        let mut t = targets();
        let other = pose_at(Vec3::new(3.0, 1.5, -3.0));
        t.screens = vec![(ScreenId(5), other, SIZE)];
        // the screens (1 m wide) would touch exactly 1 m apart; this is
        // 1.5 cm past that, inside the soft tolerance
        let p = pose_at(other.pos - Vec3::new(1.015, 0.0, 0.0));
        let (out, snap_kind, _) = snap(&p, SIZE, &t, Strength::Soft);
        assert!(
            close(other.pos.x - out.pos.x, 1.0, 1e-4),
            "edges now touch exactly: {out:?}"
        );
        assert_eq!(snap_kind, Snap::Edge(ScreenId(5)));
    }

    #[test]
    fn edge_ignores_screens_that_are_not_co_planar() {
        let mut t = targets();
        let turned = AirPose {
            yaw: std::f32::consts::FRAC_PI_2,
            ..pose_at(Vec3::new(3.0, 1.5, -3.0))
        };
        t.screens = vec![(ScreenId(9), turned, SIZE)];
        let p = pose_at(turned.pos);
        let (_, snap_kind, _) = snap(&p, SIZE, &t, Strength::Soft);
        assert_eq!(snap_kind, Snap::Air, "90° apart: not co-planar");
    }

    #[test]
    fn one_to_one_snaps_the_reading_distance_scaled() {
        let t = targets(); // eye at the origin, forward -z, one_to_one_m = 0.935
        let p = pose_at(Vec3::new(0.0, 0.0, -0.945)); // 1 cm past 0.935
        let (out, snap_kind, guides) = snap(&p, SIZE, &t, Strength::Soft);
        assert!(close(out.pos.distance(t.eye.pos), 0.935, 1e-4));
        assert_eq!(snap_kind, Snap::OneToOne);
        assert!(guides.contains(&Guide::OneToOne));
        // scale doubles the target distance
        let scaled = AirPose {
            scale: 2.0,
            ..pose_at(Vec3::new(0.0, 0.0, -1.88))
        };
        let (out, snap_kind, _) = snap(&scaled, SIZE, &t, Strength::Soft);
        assert!(close(out.pos.distance(t.eye.pos), 1.87, 1e-3));
        assert_eq!(snap_kind, Snap::OneToOne);
    }

    #[test]
    fn eye_height_levels_the_screen() {
        let mut t = targets();
        t.eye_height = 1.5;
        let p = pose_at(Vec3::new(2.0, 1.52, -3.0));
        let (out, snap_kind, guides) = snap(&p, SIZE, &t, Strength::Soft);
        assert_eq!(out.pos.y, 1.5);
        assert_eq!(out.pos.x, 2.0);
        assert_eq!(snap_kind, Snap::Eye);
        assert!(guides.contains(&Guide::Horizon { y: 1.5 }));
    }

    #[test]
    fn grid_only_fires_on_strong_and_always_produces_a_value() {
        let t = targets();
        let p = AirPose::new(Vec3::new(1.1, 1.5, -2.05), 0.2);
        let (out, snap_kind, _) = snap(&p, SIZE, &t, Strength::Strong);
        assert!(close(out.pos.x, 1.0, 1e-4));
        assert!(close(out.pos.z, -2.0, 1e-4));
        assert!(close(out.yaw, 15f32.to_radians(), 1e-4));
        assert_eq!(snap_kind, Snap::Grid);
        // soft: the grid never engages, so this falls through to the air
        let (out2, snap_kind2, _) = snap(&p, SIZE, &t, Strength::Soft);
        assert_eq!(out2, p);
        assert_eq!(snap_kind2, Snap::Air);
    }

    #[test]
    fn priority_wall_beats_edge_and_one_to_one() {
        // a pose that would also match another screen's edge and the 1:1
        // distance, but is also against a wall: the wall wins
        let mut t = targets();
        t.one_to_one_m = 5.95; // matches this pose's distance from the eye too
        t.walls = vec![north_wall()];
        let other = pose_at(Vec3::new(4.0, 1.5, -5.94));
        t.screens = vec![(ScreenId(1), other, SIZE)];
        let p = pose_at(Vec3::new(4.0, 1.5, -5.95));
        let (_, snap_kind, _) = snap(&p, SIZE, &t, Strength::Soft);
        assert!(matches!(snap_kind, Snap::Wall(_)));
    }

    #[test]
    fn base_guides_measure_floor_height_and_eye_depth() {
        let t = targets();
        let p = pose_at(Vec3::new(1.0, 1.42, -3.0));
        let guides = base_guides(&p, &t.eye);
        assert_eq!(
            guides[0],
            Guide::Floor {
                from: Vec3::new(1.0, 0.0, -3.0),
                height_m: 1.42
            }
        );
        let Guide::Depth { m, .. } = guides[1] else {
            panic!()
        };
        assert!(close(m, 3.0, 1e-5));
    }
}
