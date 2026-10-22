//! The proximity rule (addendum §2.3, §3.5): reach to interact, a smaller
//! radius to read, the ambient rest; and how close a cat notices you.
//!
//! Distances are on the floor plane (the eye's height does not count);
//! the aim cone is the 3D angle off the crosshair, the read cone the
//! horizontal angle off your heading.

use glam::{Vec2, Vec3};
use serde::{Deserialize, Serialize};

use crate::object::Object;
use nkw_core::Pose;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Radii {
    pub interact: f32,
    pub aim_cone_deg: f32,
    pub read: f32,
    pub read_cone_deg: f32,
    pub cat_notice: f32,
    pub cat_close: f32,
}

impl Default for Radii {
    fn default() -> Radii {
        Radii {
            interact: 4.0,
            aim_cone_deg: 8.0,
            read: 1.8,
            read_cone_deg: 50.0,
            cat_notice: 2.5,
            cat_close: 1.3,
        }
    }
}

/// Past this, metres, nothing animates (§3.8: "≤ 12 m"); whether it is in
/// view is the renderer's culling, not this.
pub const AMBIENT: f32 = 12.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Tier {
    Far,
    Ambient,
    Readable,
    Interactable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CatReaction {
    None,
    /// ears and head turn to you
    Notice,
    /// the tail sways while you stay
    Close,
}

fn floor(v: Vec3) -> Vec2 {
    Vec2::new(v.x, v.z)
}

fn dist(cam: &Pose, at: Vec3) -> f32 {
    floor(at).distance(floor(cam.pos))
}

/// Degrees between the crosshair and `at` (3D).
fn aim_deg(cam: &Pose, at: Vec3) -> f32 {
    let v = at - cam.pos;
    if v.length_squared() < 1e-8 {
        return 0.0;
    }
    cam.forward().angle_between(v).to_degrees()
}

/// Degrees between your heading and `at` on the floor.
fn heading_deg(cam: &Pose, at: Vec3) -> f32 {
    let v = floor(at) - floor(cam.pos);
    if v.length_squared() < 1e-8 {
        return 0.0;
    }
    cam.heading().angle_to(v).abs().to_degrees()
}

fn aimed(r: &Radii, cam: &Pose, at: Vec3) -> bool {
    dist(cam, at) <= r.interact && aim_deg(cam, at) <= r.aim_cone_deg
}

fn reads(r: &Radii, cam: &Pose, at: Vec3) -> bool {
    dist(cam, at) <= r.read && heading_deg(cam, at) <= r.read_cone_deg
}

pub fn tier(r: &Radii, cam: &Pose, obj: &Object) -> Tier {
    if aimed(r, cam, obj.pos) {
        Tier::Interactable
    } else if reads(r, cam, obj.pos) {
        Tier::Readable
    } else if dist(cam, obj.pos) <= AMBIENT {
        Tier::Ambient
    } else {
        Tier::Far
    }
}

/// The object closest to the crosshair within reach (ties: the nearer).
pub fn aim<'a>(r: &Radii, cam: &Pose, objs: &'a [Object]) -> Option<&'a Object> {
    objs.iter().filter(|o| aimed(r, cam, o.pos)).min_by(|a, b| {
        let ka = (aim_deg(cam, a.pos), dist(cam, a.pos));
        let kb = (aim_deg(cam, b.pos), dist(cam, b.pos));
        ka.partial_cmp(&kb).unwrap_or(std::cmp::Ordering::Equal)
    })
}

/// Everything close and in front enough to read, nearest first.
pub fn readable<'a>(r: &Radii, cam: &Pose, objs: &'a [Object]) -> Vec<&'a Object> {
    let mut out: Vec<&Object> = objs.iter().filter(|o| reads(r, cam, o.pos)).collect();
    out.sort_by(|a, b| {
        dist(cam, a.pos)
            .partial_cmp(&dist(cam, b.pos))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    out
}

/// How a cat reacts to you: `Close` within `cat_close` (wherever you
/// look), `Notice` within `cat_notice` while it is in your read cone. The
/// cat's own facing does not matter: cats notice regardless.
pub fn cat_reaction(r: &Radii, cam: &Pose, cat: Vec3, _cat_yaw: f32) -> CatReaction {
    let d = dist(cam, cat);
    if d <= r.cat_close {
        CatReaction::Close
    } else if d <= r.cat_notice && heading_deg(cam, cat) <= r.read_cone_deg {
        CatReaction::Notice
    } else {
        CatReaction::None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::ObjectId;
    use crate::object::Kind;
    use nkw_core::{Aabb, RoomId};
    use std::f32::consts::PI;

    /// Camera at the origin, eye 1.6 m, looking down -z.
    fn cam() -> Pose {
        Pose::new(Vec3::new(0.0, 1.6, 0.0), 0.0, 0.0)
    }

    fn obj(id: u32, x: f32, y: f32, z: f32) -> Object {
        Object {
            id: ObjectId(id),
            kind: Kind::WallClock,
            room: RoomId(0),
            pos: Vec3::new(x, y, z),
            yaw: 0.0,
            stand: Pose::default(),
            aabb: Aabb::new(Vec2::new(x - 0.2, z - 0.2), Vec2::new(x + 0.2, z + 0.2)),
        }
    }

    #[test]
    fn tier_table() {
        let r = Radii::default();
        let c = cam();
        for (o, t) in [
            (obj(1, 0.0, 1.6, -3.0), Tier::Interactable), // in the crosshair, 3 m
            (obj(2, 0.0, 1.6, -1.5), Tier::Interactable), // crosshair beats reading
            (obj(3, 0.8, 1.0, -1.0), Tier::Readable),     // off-axis, close
            (obj(4, 1.5, 1.6, -0.5), Tier::Ambient),      // 1.58 m but at 71°
            (obj(5, 0.0, 1.6, -4.5), Tier::Ambient),      // beyond reach
            (obj(6, 0.0, 1.0, 1.0), Tier::Ambient),       // behind, close
            (obj(7, 0.0, 1.6, -12.5), Tier::Far),
            (obj(8, 20.0, 0.0, 0.0), Tier::Far),
        ] {
            assert_eq!(tier(&r, &c, &o), t, "object {}", o.id.0);
        }
    }

    #[test]
    fn tier_follows_the_camera_turning() {
        let r = Radii::default();
        let o = obj(1, 3.0, 1.6, 0.0); // to the right
        assert_eq!(tier(&r, &cam(), &o), Tier::Ambient);
        let turned = Pose::new(Vec3::new(0.0, 1.6, 0.0), PI / 2.0, 0.0);
        assert_eq!(tier(&r, &turned, &o), Tier::Interactable);
        // looking down at the floor misses something at eye height
        let down = Pose::new(Vec3::new(0.0, 1.6, 0.0), PI / 2.0, -0.6);
        assert_ne!(tier(&r, &down, &o), Tier::Interactable);
    }

    #[test]
    fn aim_picks_the_smallest_angle_within_reach_and_ignores_behind() {
        let r = Radii::default();
        let c = cam();
        let objs = [
            obj(1, 0.3, 1.6, -3.0), // ~5.7°
            obj(2, 0.1, 1.6, -3.5), // ~1.6°
            obj(3, 0.0, 1.6, -6.0), // dead centre, out of reach
            obj(4, 0.0, 1.6, 2.0),  // behind
        ];
        assert_eq!(aim(&r, &c, &objs).unwrap().id, ObjectId(2));
        assert!(aim(&r, &c, &[obj(4, 0.0, 1.6, 2.0)]).is_none());
        assert!(
            aim(&r, &c, &[obj(5, 1.0, 1.6, -2.0)]).is_none(),
            "26° is outside 8°"
        );
        assert!(aim(&r, &c, &[]).is_none());
        // a tie on angle goes to the nearer one
        let tie = [obj(6, 0.0, 1.6, -3.5), obj(7, 0.0, 1.6, -2.0)];
        assert_eq!(aim(&r, &c, &tie).unwrap().id, ObjectId(7));
    }

    #[test]
    fn readable_excludes_far_and_out_of_cone_sorted_by_distance() {
        let r = Radii::default();
        let c = cam();
        let objs = [
            obj(1, 0.0, 1.0, -1.7),  // in
            obj(2, 0.0, 1.0, -1.9),  // beyond 1.8 m
            obj(3, -0.5, 1.0, -0.6), // ~40°, in
            obj(4, -1.0, 1.0, -0.5), // ~63°, out
            obj(5, 0.0, 1.0, 1.0),   // behind
        ];
        let ids: Vec<u32> = readable(&r, &c, &objs).iter().map(|o| o.id.0).collect();
        assert_eq!(ids, [3, 1]);
    }

    #[test]
    fn cat_reaction_by_distance_and_your_cone_not_the_cats_yaw() {
        let r = Radii::default();
        let c = cam();
        for (pos, want) in [
            (Vec3::new(0.0, 0.3, -2.4), CatReaction::Notice),
            (Vec3::new(0.0, 0.3, -2.6), CatReaction::None),
            (Vec3::new(0.0, 0.3, -1.2), CatReaction::Close),
            (Vec3::new(0.0, 0.3, 1.2), CatReaction::Close), // behind you, but close
            (Vec3::new(0.0, 0.3, 2.0), CatReaction::None),  // behind you: not noticed
        ] {
            for cat_yaw in [0.0, PI / 2.0, PI] {
                assert_eq!(
                    cat_reaction(&r, &c, pos, cat_yaw),
                    want,
                    "{pos} yaw {cat_yaw}"
                );
            }
        }
    }

    #[test]
    fn tiers_are_ordered() {
        assert!(Tier::Far < Tier::Ambient && Tier::Ambient < Tier::Readable);
        assert!(Tier::Readable < Tier::Interactable);
    }
}
