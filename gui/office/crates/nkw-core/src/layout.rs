//! The geometry of rooms and stations (§4.2, §4.3): where a room sits, where
//! its desks, chairs, monitors and cats go, and where the NAV camera stands
//! to look at them. Pure; `nkw` draws from it and walks on it.
//!
//! The composition (§4.2, "a cat working at a desk with a monitor"): the
//! desk stands against the back wall, the monitor stands ON its top, the
//! chair sits at the desk's front edge with the cat on it, its back to you;
//! the station frames desk + cat + monitor and a part of each neighbour.
//!
//! A room's local frame: origin at the left end of its back wall, on the
//! floor; local x runs to the right of a camera facing the back wall, local
//! z grows from the wall into the room (towards that camera), y is up.

use glam::{Vec2, Vec3};

use crate::onetoone::ScreenPlace;
use crate::pose::Pose;
use crate::strip::{RoomSlot, STACK_VISIBLE, Side};

/// Vertical field of view, radians (60°, fixed: §3.5).
pub const FOV_Y: f32 = std::f32::consts::FRAC_PI_3;
/// Eye height, metres.
pub const EYE: f32 = 1.5;
/// Width of the hall, metres (rooms open on both sides of it).
pub const HALL_W: f32 = 3.0;
/// Depth of a room, metres.
pub const ROOM_DEPTH: f32 = 5.0;
/// Free floor at both ends of a room's strip, metres.
pub const ROOM_END: f32 = 1.0;
/// Shortest room, metres.
pub const ROOM_MIN: f32 = 6.0;
/// A desk's back edge, from the back wall, metres.
pub const DESK_FROM_WALL: f32 = 0.10;
/// Depth of a desk (its top), metres.
pub const DESK_DEPTH: f32 = 0.80;
/// Height of a desk's top, metres.
pub const DESK_TOP: f32 = 0.75;
/// The screen plane: on the back half of the desk top.
pub const SCREEN_FROM_WALL: f32 = 0.42;
/// A stack never dips below the monitor's stand; stacks of 2–3 grow upward
/// and tilt to the eye.
pub const SCREEN_BOTTOM_MIN: f32 = DESK_TOP + 0.12;
/// The chair (the cat's seat): at the desk's front edge, 0.6–0.75 m before
/// the screen.
pub const CHAIR_FROM_WALL: f32 = DESK_FROM_WALL + DESK_DEPTH + 0.25;
/// Camera → screen plane in NAV, metres (tuned by shot, see the wave plan:
/// far enough that the chair seat is in frame).
pub const NAV_DIST: f32 = 2.3;
/// NAV looks a little down (≈ −5°): the desk and the cat sit in frame while
/// the screen stays within ±8 % of the frame's centre.
pub const NAV_PITCH: f32 = -0.09;
/// Instances closer than this dither out (§3.11), metres.
pub const FADE_NEAR: f32 = 0.6;
/// Height of the chair's seat (the cat's hips), metres.
pub const SEAT_H: f32 = 0.45;
/// The monitor's stand: its foot sits this far behind the screen plane
/// (the middle of the bezel's depth), metres.
pub const STAND_BEHIND: f32 = 0.04;
/// How much a stacked screen may tilt towards the eye, radians (20°).
pub const STACK_TILT_MAX: f32 = 20.0 * std::f32::consts::PI / 180.0;

/// Where a room sits in the world: the origin of its local frame and the
/// yaw of a camera facing its back wall.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RoomFrame {
    pub origin: Vec3,
    pub yaw: f32,
}

pub fn room_frame(slot: &RoomSlot) -> RoomFrame {
    let back = HALL_W / 2.0 + ROOM_DEPTH;
    match slot.side {
        Side::North => RoomFrame {
            origin: Vec3::new(slot.x0, 0.0, -back),
            yaw: 0.0,
        },
        Side::South => RoomFrame {
            origin: Vec3::new(slot.x0 + slot.len, 0.0, back),
            yaw: std::f32::consts::PI,
        },
    }
}

/// The horizontal direction a camera with this yaw looks along.
pub fn facing(yaw: f32) -> Vec3 {
    Pose::new(Vec3::ZERO, yaw, 0.0).forward()
}

/// A point of a room's local frame in the world.
pub fn to_world(slot: &RoomSlot, local: Vec3) -> Vec3 {
    let f = room_frame(slot);
    let p = Pose::new(Vec3::ZERO, f.yaw, 0.0);
    f.origin + p.right() * local.x + Vec3::Y * local.y - p.forward() * local.z
}

/// The NAV pose of the desk whose centre is at local `desk_x`.
pub fn desk_station(slot: &RoomSlot, desk_x: f32) -> Pose {
    let f = room_frame(slot);
    let local = Vec3::new(desk_x, EYE, SCREEN_FROM_WALL + NAV_DIST);
    Pose::new(to_world(slot, local), f.yaw, NAV_PITCH)
}

/// The `Station::Door` pose: the room's middle, at the stations' depth.
pub fn room_entry(slot: &RoomSlot) -> Pose {
    desk_station(slot, slot.len / 2.0)
}

/// The floor point under a desk's centre and its yaw (for `MeshId::Desk`).
pub fn desk_root(slot: &RoomSlot, desk_x: f32) -> (Vec3, f32) {
    let local = Vec3::new(desk_x, 0.0, DESK_FROM_WALL + DESK_DEPTH / 2.0);
    (to_world(slot, local), room_frame(slot).yaw)
}

/// The floor point of a desk's chair, facing the screen.
pub fn chair_root(slot: &RoomSlot, desk_x: f32) -> (Vec3, f32) {
    let local = Vec3::new(desk_x, 0.0, CHAIR_FROM_WALL);
    (to_world(slot, local), room_frame(slot).yaw)
}

/// The width a desk is drawn (and walked around) at, for a screen
/// `screen_w` metres wide: elbow room on both sides.
pub fn desk_width(screen_w: f32) -> f32 {
    (screen_w + 0.36).max(0.9)
}

/// Height of a stack's centre: `EYE` when it fits above
/// `SCREEN_BOTTOM_MIN`, else lifted so its bottom sits there.
fn stack_centre_y(n: usize, h: f32) -> f32 {
    EYE.max(SCREEN_BOTTOM_MIN + n as f32 * h / 2.0)
}

/// Bottom of the lowest screen of a stack of `stack` screens `size_m` big
/// (the top of the monitor's stand).
pub fn stack_bottom(stack: usize, size_m: Vec2) -> f32 {
    let n = stack.clamp(1, STACK_VISIBLE);
    stack_centre_y(n, size_m.y) - n as f32 * size_m.y / 2.0
}

/// Where screen `index` of a desk's stack of `stack` hangs; every screen is
/// `size_m` big. The whole stack is centred at `EYE` when it fits above
/// `SCREEN_BOTTOM_MIN`, else lifted; screens of a stack of 2–3 tilt towards
/// the station's eye, at most 20°.
pub fn screen_place(
    slot: &RoomSlot,
    desk_x: f32,
    stack: usize,
    index: usize,
    size_m: Vec2,
) -> ScreenPlace {
    let n = stack.clamp(1, STACK_VISIBLE);
    let k = index.min(n - 1);
    let h = size_m.y;
    let y = stack_centre_y(n, h) + h * ((n as f32 - 1.0) / 2.0 - k as f32);
    let pitch = if n > 1 {
        (y - EYE)
            .atan2(NAV_DIST)
            .clamp(-STACK_TILT_MAX, STACK_TILT_MAX)
    } else {
        0.0
    };
    ScreenPlace {
        center: to_world(slot, Vec3::new(desk_x, y, SCREEN_FROM_WALL)),
        yaw: room_frame(slot).yaw,
        pitch,
    }
}

/// Where the monitor stand's foot meets the desk top, and its yaw.
/// `size_m`: the screen's size (the stand stays under its centre whatever
/// the size; kept for the stand's proportions).
pub fn monitor_root(slot: &RoomSlot, desk_x: f32, size_m: Vec2) -> (Vec3, f32) {
    let _ = size_m;
    let local = Vec3::new(desk_x, DESK_TOP, SCREEN_FROM_WALL - STAND_BEHIND);
    (to_world(slot, local), room_frame(slot).yaw)
}

/// A cat's monitor: a stack of 1.
pub fn monitor_place(slot: &RoomSlot, desk_x: f32, size_m: Vec2) -> ScreenPlace {
    screen_place(slot, desk_x, 1, 0, size_m)
}

/// The cat on its chair: hips at `SEAT_H`, facing the screen (its back to
/// the camera).
pub fn cat_seat(slot: &RoomSlot, desk_x: f32) -> Pose {
    let local = Vec3::new(desk_x, SEAT_H, CHAIR_FROM_WALL);
    Pose::new(to_world(slot, local), room_frame(slot).yaw, 0.0)
}

/// Where to stand to look at something at `pos` that is looked at with
/// `yaw` (the `ScreenPlace` convention): `dist` metres before it, at eye
/// height, facing it level.
pub fn stand_before(pos: Vec3, yaw: f32, dist: f32) -> Pose {
    let at = pos - facing(yaw) * dist;
    Pose::new(Vec3::new(at.x, EYE, at.z), yaw, 0.0)
}

/// The room's hall door: its floor point (x, z) on the hall wall and the
/// yaw that looks into the room.
pub fn door_world(slot: &RoomSlot) -> (Vec2, f32) {
    let z = match slot.side {
        Side::North => -HALL_W / 2.0,
        Side::South => HALL_W / 2.0,
    };
    (Vec2::new(slot.door_x, z), room_frame(slot).yaw)
}

/// A point on the hall's centre line, at eye height.
pub fn hall_point(x: f32) -> Vec3 {
    Vec3::new(x, EYE, 0.0)
}

/// The dither-out of an instance centred at `p` seen from `cam`: 1 (gone)
/// at `FADE_NEAR / 2` or closer, 0 at `FADE_NEAR` or farther, smoothstep
/// between.
pub fn fade_for(cam: Vec3, p: Vec3) -> f32 {
    let d = cam.distance(p);
    let t = ((FADE_NEAR - d) / (FADE_NEAR * 0.5)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::onetoone::M_PER_PX;
    use crate::strip::{DESK_GAP, Width, rooms_along_hall, screen_size_px};
    use glam::Vec4;
    use std::f32::consts::PI;

    fn slots() -> Vec<RoomSlot> {
        rooms_along_hall(&[6.0, 7.5, 6.0])
    }

    /// NDC of `p` seen from `pose` at 16:9, 60° (`nkw_gl`'s `Camera::proj`).
    fn ndc(pose: &Pose, p: Vec3) -> Option<Vec2> {
        let proj = glam::camera::rh::proj::opengl::perspective(FOV_Y, 16.0 / 9.0, 0.05, 100.0);
        let c = proj * pose.view() * Vec4::from((p, 1.0));
        (c.w > 0.0).then(|| Vec2::new(c.x / c.w, c.y / c.w))
    }

    fn inside(pose: &Pose, p: Vec3) -> bool {
        ndc(pose, p).is_some_and(|n| n.abs().max_element() <= 1.0)
    }

    /// Depth along the room's local z (from the back wall into the room).
    fn depth(slot: &RoomSlot, p: Vec3) -> f32 {
        let f = room_frame(slot);
        (p - f.origin).dot(-facing(f.yaw))
    }

    fn corners(pl: &ScreenPlace, size: Vec2) -> [Vec3; 4] {
        let p = Pose::new(pl.center, pl.yaw, pl.pitch);
        let (r, u) = (p.right() * size.x / 2.0, p.up() * size.y / 2.0);
        [
            pl.center - r + u,
            pl.center + r + u,
            pl.center - r - u,
            pl.center + r - u,
        ]
    }

    /// Every output, scale, width and stack the invariants hold for, with
    /// the screen size (metres) such a desk shows.
    fn cases() -> Vec<(String, Width, usize, Vec2)> {
        let mut v = Vec::new();
        for (w, h) in [(1280.0, 720.0), (1920.0, 1080.0)] {
            for scale in [1.0, 1.25] {
                let usable = Vec2::new(w, h) / scale;
                for width in [Width::Third, Width::Half, Width::TwoThirds, Width::Full] {
                    for stack in 1..=3 {
                        let size = screen_size_px(width, usable, stack) * M_PER_PX;
                        v.push((
                            format!("{w}x{h}@{scale} {width:?} ×{stack}"),
                            width,
                            stack,
                            size,
                        ));
                    }
                }
            }
        }
        v
    }

    #[test]
    fn the_screen_plane_is_over_the_desk_top() {
        assert!((DESK_FROM_WALL..=DESK_FROM_WALL + DESK_DEPTH).contains(&SCREEN_FROM_WALL));
        const { assert!(SCREEN_FROM_WALL - STAND_BEHIND > DESK_FROM_WALL) };
        assert!((CHAIR_FROM_WALL - 1.15).abs() < 1e-6);
        for slot in slots() {
            let (foot, yaw) = monitor_root(&slot, 2.0, Vec2::ONE);
            assert!((foot.y - DESK_TOP).abs() < 1e-6);
            assert_eq!(yaw, room_frame(&slot).yaw);
            let d = depth(&slot, foot);
            assert!(
                (DESK_FROM_WALL..=DESK_FROM_WALL + DESK_DEPTH).contains(&d),
                "{d}"
            );
        }
    }

    #[test]
    fn stacks_stay_above_the_stand_and_tilt_to_the_eye() {
        let slot = slots()[0];
        for (name, _, stack, size) in cases() {
            let places: Vec<ScreenPlace> = (0..stack)
                .map(|k| screen_place(&slot, 2.0, stack, k, size))
                .collect();
            let lowest = places.last().unwrap().center.y - size.y / 2.0;
            assert!(
                lowest >= SCREEN_BOTTOM_MIN - 1e-5,
                "{name}: bottom {lowest}"
            );
            assert!((stack_bottom(stack, size) - lowest).abs() < 1e-5, "{name}");
            let centre = places.iter().map(|p| p.center.y).sum::<f32>() / stack as f32;
            if stack as f32 * size.y / 2.0 <= EYE - SCREEN_BOTTOM_MIN {
                assert!((centre - EYE).abs() < 1e-5, "{name}: centred at the eye");
            }
            for (k, p) in places.iter().enumerate() {
                assert!(p.pitch.abs() <= STACK_TILT_MAX + 1e-6, "{name}");
                assert!((depth(&slot, p.center) - SCREEN_FROM_WALL).abs() < 1e-5);
                if stack == 1 {
                    assert_eq!(p.pitch, 0.0, "{name}");
                } else if k == 0 {
                    assert!(p.pitch > 0.0, "{name}: the top one faces down to the eye");
                } else if k == stack - 1 {
                    assert!(p.pitch < 0.0, "{name}: the bottom one faces up");
                }
            }
        }
        // too tall to fit: lifted, its bottom on the stand
        let tall = Vec2::new(1.0, 1.6);
        let p = screen_place(&slot, 2.0, 1, 0, tall);
        assert!((p.center.y - tall.y / 2.0 - SCREEN_BOTTOM_MIN).abs() < 1e-5);
        assert!(p.center.y > EYE);
        // the index past the visible stack piles behind the last one
        assert_eq!(
            screen_place(&slot, 2.0, 5, 4, Vec2::splat(0.3)),
            screen_place(&slot, 2.0, 3, 2, Vec2::splat(0.3))
        );
    }

    #[test]
    fn the_station_frames_desk_chair_and_stack() {
        for slot in slots() {
            for (name, _, stack, size) in cases() {
                let x = 2.5;
                let cam = desk_station(&slot, x);
                assert_eq!(cam.pitch, NAV_PITCH);
                assert_eq!(cam.yaw, room_frame(&slot).yaw);
                // the desk top, corners and all
                let hw = desk_width(size.x) / 2.0;
                for (dx, dz) in [(-hw, 0.0), (hw, 0.0), (-hw, DESK_DEPTH), (hw, DESK_DEPTH)] {
                    let p = to_world(&slot, Vec3::new(x + dx, DESK_TOP, DESK_FROM_WALL + dz));
                    assert!(
                        inside(&cam, p),
                        "{name}: desk corner {dx},{dz} {:?}",
                        ndc(&cam, p)
                    );
                }
                // the chair seat
                let (chair, _) = chair_root(&slot, x);
                let seat = chair + Vec3::Y * SEAT_H;
                assert!(inside(&cam, seat), "{name}: seat {:?}", ndc(&cam, seat));
                // the whole stack
                for k in 0..stack {
                    let pl = screen_place(&slot, x, stack, k, size);
                    for c in corners(&pl, size) {
                        assert!(inside(&cam, c), "{name}: screen {k} {:?}", ndc(&cam, c));
                    }
                }
            }
        }
    }

    #[test]
    fn a_single_screen_sits_near_the_frame_centre() {
        // the composition checklist: within ±8 % of the frame height
        for slot in slots() {
            let cam = desk_station(&slot, 1.0);
            let pl = screen_place(&slot, 1.0, 1, 0, Vec2::new(0.624, 0.72));
            let n = ndc(&cam, pl.center).unwrap();
            assert!(n.x.abs() < 1e-4);
            assert!(n.y.abs() / 2.0 <= 0.08, "{} of the frame", n.y / 2.0);
        }
    }

    #[test]
    fn half_desk_neighbours_are_at_least_half_in_view() {
        for (w, h) in [(1280.0, 720.0), (1920.0, 1080.0)] {
            for scale in [1.0, 1.25] {
                let usable = Vec2::new(w, h) / scale;
                let s = screen_size_px(Width::Half, usable, 1) * M_PER_PX;
                let slot = slots()[1];
                let x = 3.0;
                let cam = desk_station(&slot, x);
                let dw = desk_width(s.x);
                for sign in [-1.0, 1.0] {
                    let nx = x + sign * (s.x + DESK_GAP);
                    let n = 40;
                    let seen = (0..=n)
                        .filter(|i| {
                            let dx = (*i as f32 / n as f32 - 0.5) * dw;
                            let p = Vec3::new(nx + dx, DESK_TOP, DESK_FROM_WALL);
                            inside(&cam, to_world(&slot, p))
                        })
                        .count();
                    assert!(seen * 2 >= n, "{w}x{h}@{scale} side {sign}: {seen}/{n}");
                }
            }
        }
    }

    #[test]
    fn the_chair_is_between_the_station_and_the_screen() {
        for slot in slots() {
            let cam = desk_station(&slot, 2.0);
            let (chair, yaw) = chair_root(&slot, 2.0);
            assert_eq!(yaw, room_frame(&slot).yaw);
            let screen = screen_place(&slot, 2.0, 1, 0, Vec2::ONE).center;
            let (dc, dch, ds) = (
                depth(&slot, cam.pos),
                depth(&slot, chair),
                depth(&slot, screen),
            );
            assert!(dc > dch && dch > ds, "{dc} {dch} {ds}");
            assert!(dch > DESK_FROM_WALL + DESK_DEPTH, "in front of the desk");
            let (desk, _) = desk_root(&slot, 2.0);
            assert!((depth(&slot, desk) - (DESK_FROM_WALL + DESK_DEPTH / 2.0)).abs() < 1e-5);
            assert_eq!(desk.y, 0.0);
            assert_eq!(chair.y, 0.0);
        }
    }

    #[test]
    fn the_cat_faces_its_screen_with_its_back_to_you() {
        for slot in slots() {
            let seat = cat_seat(&slot, 1.7);
            assert!((seat.pos.y - SEAT_H).abs() < 1e-6);
            let screen = screen_place(&slot, 1.7, 1, 0, Vec2::new(0.9, 0.5)).center;
            let to = screen - seat.pos;
            let yaw = to.x.atan2(-to.z);
            assert!(
                angle(yaw, seat.yaw) < 1f32.to_radians(),
                "{yaw} vs {}",
                seat.yaw
            );
            let cam = desk_station(&slot, 1.7);
            assert!(angle(cam.yaw, seat.yaw) < 1e-6, "back to the camera");
            // the head stays below the screen's centre
            assert!(seat.pos.y + 0.6 <= EYE - 0.25 + 1e-6);
        }
    }

    fn angle(a: f32, b: f32) -> f32 {
        crate::pose::angle_diff(a, b).abs()
    }

    #[test]
    fn monitor_place_is_a_stack_of_one() {
        let slot = slots()[2];
        let s = Vec2::new(0.944, 1.04);
        assert_eq!(
            monitor_place(&slot, 1.2, s),
            screen_place(&slot, 1.2, 1, 0, s)
        );
    }

    #[test]
    fn room_entry_is_on_the_rooms_axis() {
        for slot in slots() {
            let e = room_entry(&slot);
            assert!((e.pos.x - (slot.x0 + slot.len / 2.0)).abs() < 1e-5);
            assert_eq!(e, desk_station(&slot, slot.len / 2.0));
            let (door, yaw) = door_world(&slot);
            assert!(
                (door.x - e.pos.x).abs() < 1e-5,
                "the door is on the axis too"
            );
            assert_eq!(yaw, e.yaw);
            assert!((door.y.abs() - HALL_W / 2.0).abs() < 1e-6);
            // looking into the room from the door goes away from the hall
            assert!(facing(yaw).z * door.y > 0.0);
        }
        assert_eq!(hall_point(4.0), Vec3::new(4.0, EYE, 0.0));
    }

    #[test]
    fn stand_before_faces_the_thing() {
        let p = Vec3::new(2.0, 0.9, -5.0);
        let s = stand_before(p, 0.0, 1.2);
        assert!((s.pos - Vec3::new(2.0, EYE, -3.8)).length() < 1e-5);
        assert_eq!((s.yaw, s.pitch), (0.0, 0.0));
        let s = stand_before(p, PI / 2.0, 1.0);
        assert!((s.pos - Vec3::new(1.0, EYE, -5.0)).length() < 1e-5);
    }

    #[test]
    fn fade_ramps_between_half_and_full_fade_near() {
        let at = |d: f32| fade_for(Vec3::ZERO, Vec3::new(0.0, 0.0, d));
        assert_eq!(at(0.0), 1.0);
        assert_eq!(at(0.2), 1.0);
        assert_eq!(at(FADE_NEAR * 0.5), 1.0);
        assert_eq!(at(0.6), 0.0);
        assert_eq!(at(3.0), 0.0);
        let mut last = 1.0;
        for i in 0..=30 {
            let f = at(0.3 + i as f32 * 0.01);
            assert!(f <= last + 1e-6 && (0.0..=1.0).contains(&f));
            last = f;
        }
        assert!((at(0.45) - 0.5).abs() < 1e-5);
    }

    /// The numbers `nkw/src/world.rs` used before the move (3-room demo):
    /// frames, local → world, the hall door; stations at the new depth.
    #[test]
    fn layout_equals_the_old_world_numbers() {
        for slot in slots() {
            let back = HALL_W / 2.0 + ROOM_DEPTH;
            let (o, yaw) = match slot.side {
                Side::North => (Vec3::new(slot.x0, 0.0, -back), 0.0),
                Side::South => (Vec3::new(slot.x0 + slot.len, 0.0, back), PI),
            };
            assert_eq!(room_frame(&slot), RoomFrame { origin: o, yaw });
            let p = Pose::new(Vec3::ZERO, yaw, 0.0);
            let local = Vec3::new(1.3, 0.7, 2.1);
            let old = o + p.right() * local.x + Vec3::Y * local.y - p.forward() * local.z;
            assert!((to_world(&slot, local) - old).length() < 1e-5);
            let st = desk_station(&slot, 1.3);
            let old = to_world(&slot, Vec3::new(1.3, EYE, SCREEN_FROM_WALL + NAV_DIST));
            assert!((st.pos - old).length() < 1e-6);
        }
        let s = slots();
        assert_eq!(room_frame(&s[1]).yaw, PI);
        assert!((room_frame(&s[2]).origin.x - 6.6).abs() < 1e-5);
        assert!((facing(0.0) - Vec3::NEG_Z).length() < 1e-6);
    }
}
