//! Where the objects stand: the reception's furniture in its local frame
//! (x along the room, z from the back wall into the room, `layout`'s
//! convention), one wall clock per other room, one door object per room,
//! all turned into world `Object`s through `layout::to_world`.
//!
//! Positions follow the rooms: when the strip changes (a desk wider, a room
//! more), [`Placement::sync`] moves the objects and keeps their data.

use std::f32::consts::{FRAC_PI_2, PI};

use glam::{Vec2, Vec3};
use nkw_core::layout::{self, ROOM_DEPTH};
use nkw_core::strip::RoomSlot;
use nkw_core::{Aabb, DoorId, ObjectId, RoomId};
use nkw_objects::object::{Kind, Object};

/// Where you stand to use an object, metres before it.
pub const STAND_DIST: f32 = 1.2;
/// Where you stand for `kind`: a door is seen whole from further back.
pub fn stand_dist(kind: Kind) -> f32 {
    match kind {
        Kind::Door(_) => 2.2,
        _ => STAND_DIST,
    }
}

/// The first id of the per-room wall clocks (`CLOCK_BASE + room`).
pub const CLOCK_BASE: u32 = 100;
/// The first id of the door objects (`DOOR_BASE + room`).
pub const DOOR_BASE: u32 = 1000;
/// Height of a door object's centre (its label and aim point).
pub const DOOR_Y: f32 = 1.1;

/// The ids `demo_world()` gives each kind; a real world uses the same ones.
pub fn kind_id(kind: Kind) -> ObjectId {
    ObjectId(match kind {
        Kind::Turntable => 1,
        Kind::WallClock => 2,
        Kind::Battery => 3,
        Kind::NetPlant => 4,
        Kind::Fireplace => 5,
        Kind::Aquarium => 6,
        Kind::PackageBox => 7,
        Kind::TrayShelf => 8,
        Kind::OnAir => 9,
        Kind::KeyboardFlag => 10,
        Kind::Calendar => 11,
        Kind::BtRadio => 12,
        Kind::Lamp => 13,
        Kind::Vault => 14,
        Kind::Door(d) => DOOR_BASE + d.0,
    })
}

/// The reception's furniture by default (`world.toml` `[objects]` may
/// change it later): turntable, clock, battery jar, plant, vault.
pub const DEFAULT_RECEPTION: [Kind; 5] = [
    Kind::Turntable,
    Kind::WallClock,
    Kind::Battery,
    Kind::NetPlant,
    Kind::Vault,
];

/// Wall things hang this far off the back wall's line (its inner face is
/// at 0.05; their backs touch the wallpaper).
pub const ON_WALL: f32 = 0.075;
/// The window sill's top and depth centre (`nkw_gl::shell`'s windows).
pub const SILL_TOP: f32 = nkw_gl::shell::SILL_Y;
pub const SILL_Z: f32 = 0.155;
/// How far above its pot's base the plant's point (label, aim) is: the
/// objects' draw puts the pot at `pos.y - PLANT_UP` (on the sill).
pub const PLANT_UP: f32 = 0.35;

/// The back-wall windows of a room `len` long (local x of their centres).
pub fn windows(len: f32) -> Vec<f32> {
    nkw_gl::shell::window_xs(len)
}

/// An object's place in its room's local frame and the yaw you look at it
/// with (local: 0 faces the back wall, π/2 the right wall).
///
/// The reception (wave 3): the receptionist's desk and its notice board
/// (her monitor) in the middle of the back wall, between the two windows;
/// ON AIR above it to the right, the calendar to the left; the turntable
/// with its speakers on its low cabinet against the left wall by the far
/// corner (out of the desks' view), the floor lamp in that corner; the plant on the right window's sill;
/// the fireplace and the aquarium on the right wall; by the door the
/// battery's cushion and oil jar, the package; the clock above the door.
pub fn local_place(kind: Kind, len: f32) -> (Vec3, f32) {
    let mid = len / 2.0;
    let left = -FRAC_PI_2;
    let right = FRAC_PI_2;
    let win = windows(len);
    let last = *win.last().unwrap_or(&(len * 0.75));
    match kind {
        // on its cabinet against the left wall near the far corner, the
        // speakers beside it: out of the desks' view, so a playing record
        // asks for no frames while you work (S3r)
        Kind::Turntable => (Vec3::new(0.42, 0.85, 1.75), left),
        // above the door, hanging from the ceiling's fascia, facing the room
        Kind::WallClock => (Vec3::new(mid, 2.42, ROOM_DEPTH - 0.04), PI),
        // by the door, on its cushion
        Kind::Battery => (Vec3::new(mid - 1.3, 0.35, 4.25), PI),
        // on the sill of the right-hand window
        Kind::NetPlant => (Vec3::new(last + 0.32, SILL_TOP + PLANT_UP, SILL_Z), 0.0),
        Kind::Fireplace => (Vec3::new(len - 0.25, 0.5, 2.4), right),
        Kind::Aquarium => (Vec3::new(len - 0.3, 0.95, 3.75), right),
        Kind::PackageBox => (Vec3::new(mid + 1.2, 0.15, 4.5), PI),
        Kind::TrayShelf => (Vec3::new(ON_WALL, 1.3, 2.3), left),
        // over the board, to the right; the calendar to its left
        Kind::OnAir => (Vec3::new(mid + 0.52, 2.45, ON_WALL), 0.0),
        Kind::Calendar => (Vec3::new(mid - 0.52, 2.4, ON_WALL), 0.0),
        Kind::KeyboardFlag => (Vec3::new(0.5, 0.75, 3.2), left),
        Kind::BtRadio => (Vec3::new(0.4, 0.45, 3.85), left),
        // the floor lamp in the far corner, beside the turntable
        Kind::Lamp => (Vec3::new(0.32, 1.2, 0.68), left),
        // the reception keeps it only while there is no library
        Kind::Vault => (Vec3::new(len - 0.5, 0.5, 0.45), 0.0),
        // on the hall wall; looked at from inside the room
        Kind::Door(_) => (Vec3::new(mid, DOOR_Y, ROOM_DEPTH), PI),
    }
}

/// A room's own clock (every room but the reception): high on the back
/// wall between its first two windows, over the desks' monitors.
pub fn room_clock_place(len: f32) -> (Vec3, f32) {
    let win = windows(len);
    let x = match win.as_slice() {
        [a, b, ..] => (a + b) / 2.0,
        _ => len / 2.0 + 1.3,
    };
    (Vec3::new(x, 2.36, ON_WALL), 0.0)
}

/// The vault in the library: at the back wall beside the librarian's
/// counter (desk 0, in the middle).
pub fn library_vault_place(len: f32) -> (Vec3, f32) {
    (Vec3::new(len / 2.0 + 1.05, 0.5, 0.45), 0.0)
}

/// Half its floor footprint, metres (x across, z deep, before the yaw).
pub fn half_size(kind: Kind) -> Vec2 {
    match kind {
        // with its two speakers
        Kind::Turntable => Vec2::new(0.64, 0.25),
        Kind::Battery => Vec2::new(0.25, 0.25),
        Kind::NetPlant => Vec2::new(0.22, 0.22),
        Kind::Fireplace => Vec2::new(0.6, 0.25),
        Kind::Aquarium => Vec2::new(0.45, 0.2),
        Kind::PackageBox => Vec2::new(0.25, 0.2),
        Kind::KeyboardFlag => Vec2::new(0.2, 0.2),
        Kind::BtRadio => Vec2::new(0.2, 0.15),
        Kind::Vault => Vec2::new(0.35, 0.3),
        Kind::Lamp => Vec2::new(0.22, 0.22),
        _ => Vec2::new(0.15, 0.05),
    }
}

/// Whether it stands on the floor in the way (wall and ceiling things do
/// not; doors collide through `Door::blocks`).
pub fn blocks_floor(kind: Kind) -> bool {
    matches!(
        kind,
        Kind::Turntable
            | Kind::Battery
            | Kind::NetPlant
            | Kind::Fireplace
            | Kind::Aquarium
            | Kind::PackageBox
            | Kind::KeyboardFlag
            | Kind::BtRadio
            | Kind::Vault
            | Kind::Lamp
    )
}

/// One object to place.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placed {
    pub id: ObjectId,
    pub kind: Kind,
    pub room: RoomId,
}

/// The world object of `p` in a room at `slot`.
pub fn to_object(p: &Placed, slot: &RoomSlot) -> Object {
    to_object_in(p, slot, false)
}

/// Where `p` goes in its room (`library`: that room is the library).
fn local_of(p: &Placed, len: f32, library: bool) -> (Vec3, f32) {
    match p.kind {
        Kind::WallClock if p.id.0 >= CLOCK_BASE => room_clock_place(len),
        Kind::Vault if library => library_vault_place(len),
        k => local_place(k, len),
    }
}

fn to_object_in(p: &Placed, slot: &RoomSlot, library: bool) -> Object {
    let (local, local_yaw) = local_of(p, slot.len, library);
    let pos = layout::to_world(slot, local);
    let yaw = layout::room_frame(slot).yaw + local_yaw;
    // looking at it, so the crosshair is on it when you arrive
    let dist = stand_dist(p.kind);
    let mut stand = layout::stand_before(pos, yaw, dist);
    stand.pitch = (pos.y - stand.pos.y).atan2(dist).clamp(-0.7, 0.7);
    // the footprint, turned with the room (yaws are multiples of π/2)
    let h = half_size(p.kind);
    let quarter = ((yaw / FRAC_PI_2).round() as i32).rem_euclid(2) == 1;
    let h = if quarter { Vec2::new(h.y, h.x) } else { h };
    let c = Vec2::new(pos.x, pos.z);
    Object {
        id: p.id,
        kind: p.kind,
        room: p.room,
        pos,
        yaw,
        stand,
        aabb: Aabb::new(c - h, c + h),
    }
}

/// What to place for rooms `rooms` (the reception first): the reception's
/// furniture, a wall clock in every other room, a door object per room.
pub fn plan(reception: &[Kind], rooms: &[RoomId]) -> Vec<Placed> {
    let mut out = Vec::new();
    let Some(&first) = rooms.first() else {
        return out;
    };
    for &kind in reception {
        out.push(Placed {
            id: kind_id(kind),
            kind,
            room: first,
        });
    }
    for &r in &rooms[1..] {
        out.push(Placed {
            id: ObjectId(CLOCK_BASE + r.0),
            kind: Kind::WallClock,
            room: r,
        });
    }
    for &r in rooms {
        let kind = Kind::Door(DoorId(r.0));
        out.push(Placed {
            id: kind_id(kind),
            kind,
            room: r,
        });
    }
    out
}

/// The objects' placement, kept in step with the strip.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Placement {
    pub reception: Vec<Kind>,
    rooms: Vec<(RoomId, RoomSlot)>,
    /// the library room, where the vault goes (none: it stays in the reception)
    library: Option<RoomId>,
}

impl Placement {
    pub fn new(reception: Vec<Kind>) -> Placement {
        Placement {
            reception,
            rooms: Vec::new(),
            library: None,
        }
    }

    /// The library room (`World::room_kind(r) == Library`), if there is one:
    /// the vault moves there, beside the librarian's counter. A change
    /// re-places everything on the next [`Placement::sync`].
    pub fn set_library(&mut self, library: Option<RoomId>) {
        if self.library != library {
            self.library = library;
            self.rooms.clear();
        }
    }

    /// Re-places `objects` when the rooms moved; `true` when it did.
    /// Objects of rooms that went away go, new rooms get theirs; the data
    /// of the objects that stay is the caller's and is kept.
    pub fn sync(&mut self, rooms: &[(RoomId, RoomSlot)], objects: &mut Vec<Object>) -> bool {
        if self.rooms == rooms {
            return false;
        }
        self.rooms = rooms.to_vec();
        let ids: Vec<RoomId> = rooms.iter().map(|(r, _)| *r).collect();
        let library = self.library.filter(|l| ids.contains(l));
        *objects = plan(&self.reception, &ids)
            .into_iter()
            .map(|mut p| {
                if let (Kind::Vault, Some(l)) = (p.kind, library) {
                    p.room = l;
                }
                p
            })
            .filter_map(|p| {
                let slot = rooms.iter().find(|(r, _)| *r == p.room)?.1;
                Some(to_object_in(&p, &slot, Some(p.room) == library))
            })
            .collect();
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nkw_core::strip::rooms_along_hall;

    fn slots() -> Vec<(RoomId, RoomSlot)> {
        rooms_along_hall(&[6.0, 7.5, 6.0])
            .into_iter()
            .enumerate()
            .map(|(i, s)| (RoomId(i as u32), s))
            .collect()
    }

    fn inside(slot: &RoomSlot, p: Vec3) -> bool {
        let f = layout::room_frame(slot);
        let a = layout::to_world(slot, Vec3::ZERO);
        let b = layout::to_world(slot, Vec3::new(slot.len, 0.0, ROOM_DEPTH));
        let _ = f;
        let (x0, x1) = (a.x.min(b.x), a.x.max(b.x));
        let (z0, z1) = (a.z.min(b.z), a.z.max(b.z));
        p.x >= x0 - 1e-3 && p.x <= x1 + 1e-3 && p.z >= z0 - 1e-3 && p.z <= z1 + 1e-3
    }

    #[test]
    fn demo_ids_match_nkw_objects() {
        let w = nkw_objects::object::demo_world();
        for o in &w.objects {
            assert_eq!(kind_id(o.kind), o.id, "{:?}", o.kind);
        }
    }

    #[test]
    fn plan_has_the_reception_a_clock_per_other_room_and_a_door_per_room() {
        let rooms = [RoomId(0), RoomId(4), RoomId(7)];
        let p = plan(&DEFAULT_RECEPTION, &rooms);
        assert_eq!(p.len(), 5 + 2 + 3);
        assert!(p[..5].iter().all(|x| x.room == RoomId(0)));
        let clocks: Vec<_> = p.iter().filter(|x| x.kind == Kind::WallClock).collect();
        assert_eq!(clocks.len(), 3, "one clock per room");
        assert!(clocks.iter().any(|c| c.id == ObjectId(CLOCK_BASE + 7)));
        let doors: Vec<_> = p
            .iter()
            .filter(|x| matches!(x.kind, Kind::Door(_)))
            .collect();
        assert_eq!(doors.len(), 3);
        assert_eq!(doors[1].kind, Kind::Door(DoorId(4)));
        assert_eq!(doors[1].id, ObjectId(DOOR_BASE + 4));
        let ids: std::collections::BTreeSet<_> = p.iter().map(|x| x.id).collect();
        assert_eq!(ids.len(), p.len(), "unique ids");
        assert!(plan(&DEFAULT_RECEPTION, &[]).is_empty());
    }

    #[test]
    fn every_object_is_inside_its_room_and_its_stand_faces_it() {
        let s = slots();
        let rooms: Vec<RoomId> = s.iter().map(|x| x.0).collect();
        let all: Vec<Kind> = nkw_objects::object::demo_world()
            .objects
            .iter()
            .map(|o| o.kind)
            .collect();
        for p in plan(&all, &rooms) {
            let slot = s.iter().find(|x| x.0 == p.room).unwrap().1;
            let o = to_object(&p, &slot);
            assert!(inside(&slot, o.pos), "{:?} at {:?}", o.kind, o.pos);
            assert!(
                inside(&slot, o.stand.pos),
                "{:?} stand {:?}",
                o.kind,
                o.stand.pos
            );
            let to = Vec2::new(o.pos.x - o.stand.pos.x, o.pos.z - o.stand.pos.z);
            assert!(
                (to.length() - stand_dist(o.kind)).abs() < 1e-3,
                "{:?}",
                o.kind
            );
            assert!(
                to.normalize().dot(o.stand.heading()) > 0.999,
                "{:?}",
                o.kind
            );
            assert!(o.aabb.contains(Vec2::new(o.pos.x, o.pos.z)));
        }
    }

    /// Every kind the demo world has, once.
    fn all_kinds() -> Vec<Kind> {
        nkw_objects::object::demo_world()
            .objects
            .iter()
            .map(|o| o.kind)
            .collect()
    }

    /// `p` in the room's local frame (x along, z from the back wall).
    fn local(slot: &RoomSlot, p: Vec3) -> Vec3 {
        let o = layout::to_world(slot, Vec3::ZERO);
        let ex = layout::to_world(slot, Vec3::X) - o;
        let ez = layout::to_world(slot, Vec3::Z) - o;
        let d = p - o;
        Vec3::new(d.dot(ex), p.y, d.dot(ez))
    }

    #[test]
    fn every_stand_is_reachable_on_the_floor_away_from_the_walls() {
        for lens in [[6.0, 7.5, 6.0], [9.4, 6.0, 12.0], [14.0, 6.0, 6.0]] {
            let s: Vec<(RoomId, RoomSlot)> = rooms_along_hall(&lens)
                .into_iter()
                .enumerate()
                .map(|(i, s)| (RoomId(i as u32), s))
                .collect();
            for library in [None, Some(RoomId(2))] {
                let mut pl = Placement::new(all_kinds());
                pl.set_library(library);
                let mut objs = Vec::new();
                pl.sync(&s, &mut objs);
                for o in &objs {
                    let slot = s.iter().find(|x| x.0 == o.room).unwrap().1;
                    let st = local(&slot, o.stand.pos);
                    assert!(
                        st.x >= 0.4
                            && st.x <= slot.len - 0.4
                            && st.z >= 0.4
                            && st.z <= ROOM_DEPTH - 0.4,
                        "{:?} in {lens:?}: stand at local {st}",
                        o.kind
                    );
                    // and no floor thing stands on it
                    for b in objs
                        .iter()
                        .filter(|b| b.room == o.room && blocks_floor(b.kind))
                    {
                        assert!(
                            !b.aabb.contains(Vec2::new(o.stand.pos.x, o.stand.pos.z)),
                            "{:?}'s stand is inside {:?}",
                            o.kind,
                            b.kind
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn back_wall_things_keep_clear_of_the_windows() {
        for len in [6.0, 7.5, 9.4, 12.0] {
            let win = windows(len);
            // a window with its curtains spans ±0.87 m, up to its rod at 2.16 m
            let clear = |x: f32, half_w: f32, bottom: f32| {
                bottom > 2.17 || win.iter().all(|w| (x - w).abs() > 0.87 + half_w)
            };
            for (kind, half_w, half_h) in [(Kind::OnAir, 0.24, 0.09), (Kind::Calendar, 0.15, 0.21)]
            {
                let (p, _) = local_place(kind, len);
                assert!(
                    clear(p.x, half_w, p.y - half_h),
                    "{kind:?} at {p} (windows {win:?})"
                );
                assert!(
                    p.y + half_h < nkw_gl::shell::CEILING_Y - 0.1,
                    "{kind:?} under the cornice"
                );
            }
            let (c, _) = room_clock_place(len);
            assert!(
                clear(c.x, 0.17, c.y - 0.17),
                "clock at {c} (windows {win:?})"
            );
            // the plant sits on the right-hand window's sill
            let (p, _) = local_place(Kind::NetPlant, len);
            let w = *win.last().unwrap();
            assert!((p.x - w).abs() < 0.6 && (p.y - PLANT_UP - SILL_TOP).abs() < 1e-5);
        }
    }

    #[test]
    fn the_reception_is_composed_around_its_board() {
        let len = 6.0;
        let mid = len / 2.0;
        let at = |k| local_place(k, len).0;
        // ON AIR above the board to the right, the calendar to the left
        assert!(at(Kind::OnAir).x > mid && at(Kind::Calendar).x < mid);
        assert!(at(Kind::OnAir).y > 2.2 && at(Kind::Calendar).y > 2.2);
        // the clock above the door, facing into the room
        let (clock, yaw) = local_place(Kind::WallClock, len);
        assert!((clock.x - mid).abs() < 1e-5 && clock.z > ROOM_DEPTH - 0.1 && clock.y > 2.2);
        assert!((yaw - PI).abs() < 1e-5);
        // the turntable in the far left corner with the lamp beside it
        let tt = at(Kind::Turntable);
        assert!(tt.x < 0.6 && tt.z < 2.0);
        assert!(at(Kind::Lamp).distance(tt) < 1.2);
        // cushion and package by the door
        for k in [Kind::Battery, Kind::PackageBox] {
            assert!(at(k).z > 4.0 && (at(k).x - mid).abs() < 1.6, "{k:?}");
        }
        // other rooms' clocks hang on their back wall, not above the door
        let p = Placed {
            id: ObjectId(CLOCK_BASE + 3),
            kind: Kind::WallClock,
            room: RoomId(3),
        };
        let slot = rooms_along_hall(&[6.0])[0];
        let o = to_object(&p, &slot);
        assert!(local(&slot, o.pos).z < 0.2);
    }

    #[test]
    fn the_vault_moves_to_the_library_beside_the_counter() {
        let s = slots();
        let mut pl = Placement::new(DEFAULT_RECEPTION.to_vec());
        let mut objs = Vec::new();
        pl.sync(&s, &mut objs);
        let vault = |v: &[Object]| v.iter().find(|o| o.kind == Kind::Vault).unwrap().clone();
        assert_eq!(
            vault(&objs).room,
            RoomId(0),
            "no library: the reception keeps it"
        );
        pl.set_library(Some(RoomId(2)));
        assert!(pl.sync(&s, &mut objs), "a new library re-places");
        let v = vault(&objs);
        assert_eq!(v.room, RoomId(2));
        assert_eq!(v.id, kind_id(Kind::Vault));
        let slot = s[2].1;
        let l = local(&slot, v.pos);
        assert!(
            l.z < 0.6 && (l.x - slot.len / 2.0 - 1.05).abs() < 1e-3,
            "{l}"
        );
        assert_eq!(objs.len(), 5 + 2 + 3, "nothing else moved or went");
        assert!(!pl.sync(&s, &mut objs));
        // a library that is not (yet) one of the rooms changes nothing
        pl.set_library(Some(RoomId(9)));
        pl.sync(&s, &mut objs);
        assert_eq!(vault(&objs).room, RoomId(0));
    }

    #[test]
    fn the_door_object_sits_on_the_rooms_hall_door() {
        for (r, slot) in slots() {
            let p = Placed {
                id: ObjectId(DOOR_BASE + r.0),
                kind: Kind::Door(DoorId(r.0)),
                room: r,
            };
            let o = to_object(&p, &slot);
            let (door, _) = layout::door_world(&slot);
            assert!((Vec2::new(o.pos.x, o.pos.z) - door).length() < 1e-4);
        }
    }

    #[test]
    fn floor_things_stay_off_the_walk_to_the_door() {
        // the line from the desk's station straight back to the door is clear
        let (_, slot) = slots()[0];
        let station = layout::room_entry(&slot);
        let (door, _) = layout::door_world(&slot);
        let all: Vec<Kind> = nkw_objects::object::demo_world()
            .objects
            .iter()
            .map(|o| o.kind)
            .collect();
        for p in plan(&all, &[RoomId(0)]) {
            if !blocks_floor(p.kind) {
                continue;
            }
            let o = to_object(&p, &slot);
            for i in 0..=20 {
                let t = i as f32 / 20.0;
                let q = Vec2::new(station.pos.x, station.pos.z).lerp(door, t);
                assert!(!o.aabb.inflate(0.3).contains(q), "{:?} blocks", o.kind);
            }
        }
    }

    #[test]
    fn sync_moves_objects_with_the_rooms_and_only_when_they_change() {
        let mut pl = Placement::new(DEFAULT_RECEPTION.to_vec());
        let mut objs = Vec::new();
        let s = slots();
        assert!(pl.sync(&s, &mut objs));
        assert_eq!(objs.len(), 5 + 2 + 3);
        assert!(!pl.sync(&s, &mut objs), "same rooms: nothing to do");
        let wider = rooms_along_hall(&[8.0, 7.5, 6.0]);
        let s2: Vec<_> = s.iter().zip(wider).map(|((r, _), w)| (*r, w)).collect();
        let before = objs.iter().find(|o| o.kind == Kind::NetPlant).unwrap().pos;
        assert!(pl.sync(&s2, &mut objs));
        let after = objs.iter().find(|o| o.kind == Kind::NetPlant).unwrap().pos;
        // the plant rides its window, which moves with the wall (3/4 of the way along)
        assert!(
            (after.x - before.x - 1.5).abs() < 1e-4,
            "the plant follows the right-hand window: {before} → {after}"
        );
        assert!(pl.sync(&s2[..1], &mut objs));
        assert_eq!(objs.len(), 5 + 1, "the other rooms' clocks and doors went");
    }
}
