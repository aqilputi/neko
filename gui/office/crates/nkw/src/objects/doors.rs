//! The rooms' hall doors as `nkw_objects::door::Door`s (addendum §4.4):
//! one source of truth, `openness(now)`, feeds collision (`obstacles`),
//! culling (`opens_rooms`), audio (`portals`) and the leaf's swing.

use glam::{Mat4, Quat, Vec2, Vec3};
use nkw_core::collide::{Door as Gap, Wall};
use nkw_core::layout::{self, HALL_W};
use nkw_core::strip::{RoomSlot, Side};
use nkw_core::{Aabb, DoorId, RoomId, Time, Want};
use nkw_objects::acoustic::Portal;
use nkw_objects::door::{Door, Effect, Params, State};

/// The hall, as a room of the acoustic graph (it has no `RoomId` of its own).
pub const HALL: RoomId = RoomId(u32::MAX);
/// The leaf swings this far open, degrees.
pub const OPEN_DEG: f32 = 100.0;
/// The leaf mesh is this wide (`MeshId::DoorLeaf`: x 0..0.98 from the hinge).
pub const LEAF_W: f32 = 0.98;

/// The door id of a room's hall door.
pub fn door_of(room: RoomId) -> DoorId {
    DoorId(room.0)
}

/// Whether a floor point is in the hall (between the two rows of rooms).
pub fn in_hall(p: Vec2) -> bool {
    p.y.abs() < HALL_W / 2.0
}

#[derive(Clone, Debug, Default)]
pub struct Doors {
    pub doors: Vec<Door>,
    /// the room's frame yaw per door (the leaf's turn)
    yaws: Vec<f32>,
    pub p: Params,
}

impl Doors {
    /// One door per room, on its hall wall; a known door keeps its state,
    /// a new one starts `Open`. Returns whether the set changed.
    pub fn sync(&mut self, rooms: &[(RoomId, RoomSlot)]) -> bool {
        let same = self.doors.len() == rooms.len()
            && self
                .doors
                .iter()
                .zip(rooms)
                .all(|(d, (r, s))| d.id == door_of(*r) && d.pos == layout::door_world(s).0);
        if same {
            return false;
        }
        let mut old = std::mem::take(&mut self.doors);
        self.yaws.clear();
        for (r, s) in rooms {
            let (pos, yaw) = layout::door_world(s);
            let wall = match s.side {
                Side::North => Wall::South,
                Side::South => Wall::North,
            };
            let gap = Gap::new(wall, s.door_x);
            let d = match old.iter().position(|d| d.id == door_of(*r)) {
                Some(i) => {
                    let mut d = old.remove(i);
                    d.pos = pos;
                    d.geom = gap;
                    d
                }
                None => Door::new(door_of(*r), gap, (*r, HALL), pos, State::Open),
            };
            self.doors.push(d);
            self.yaws.push(yaw);
        }
        true
    }

    pub fn get(&self, id: DoorId) -> Option<&Door> {
        self.doors.iter().find(|d| d.id == id)
    }

    /// The swing parameters: under reduced motion the swing is cut (the
    /// openness jumps).
    fn params(&self, reduced: bool) -> Params {
        Params {
            swing_s: if reduced { 0.0 } else { self.p.swing_s },
            ..self.p
        }
    }

    /// Open a closed door, close an open one.
    pub fn toggle(&mut self, id: DoorId, now: Time, reduced: bool) -> Vec<Effect> {
        let p = self.params(reduced);
        match self.doors.iter_mut().find(|d| d.id == id) {
            Some(d) => d.toggle(now, &p),
            None => Vec::new(),
        }
    }

    /// Finish swings, run the auto-closes (held while `player` is near).
    pub fn tick(&mut self, now: Time, player: Vec2, reduced: bool) -> Vec<(DoorId, Effect)> {
        let p = self.params(reduced);
        let mut out = Vec::new();
        for d in &mut self.doors {
            for e in d.tick(now, player, &p) {
                out.push((d.id, e));
            }
        }
        out
    }

    /// The closed leaf of a room's door, for that room's occupancy grid (the
    /// grid inflates it by the player's radius).
    pub fn obstacles(&self, room: RoomId, now: Time) -> Vec<Aabb> {
        self.get(door_of(room))
            .and_then(|d| d.blocks(now, &self.p))
            .into_iter()
            .collect()
    }

    /// Rooms seen from `from` besides itself: through its own door into the
    /// hall, then through every other door with a gap. From the hall, every
    /// door with a gap.
    pub fn opens_rooms(&self, from: RoomId, hall: bool, now: Time) -> Vec<RoomId> {
        let see = |d: &Door| d.see_through(now);
        if !hall && !self.get(door_of(from)).is_some_and(see) {
            return Vec::new();
        }
        self.doors
            .iter()
            .filter(|d| see(d) && d.rooms.0 != from)
            .map(|d| d.rooms.0)
            .collect()
    }

    /// What the doors need from the scheduler: every frame while one
    /// swings, else one wakeup at the next auto-close deadline.
    pub fn wants(&self, now: Time, reduced: bool) -> Vec<Want> {
        let p = self.params(reduced);
        let mut out = Vec::new();
        let mut next: Option<Time> = None;
        for d in &self.doors {
            if let Some(w) = d.want(now) {
                out.push(w);
            }
            if let Some(t) = d.next_wake(now, &p) {
                next = Some(next.map_or(t, |n: Time| n.min(t)));
            }
        }
        if out.is_empty()
            && let Some(t) = next
        {
            out.push(Want::At(t.max(now)));
        }
        out
    }

    pub fn swinging(&self, now: Time) -> bool {
        self.doors.iter().any(|d| d.swinging(now))
    }

    /// The doors as the acoustics sees them.
    pub fn portals(&self, now: Time) -> Vec<Portal> {
        self.doors
            .iter()
            .map(|d| Portal {
                id: d.id,
                rooms: d.rooms,
                pos: d.pos,
                openness: d.openness(now),
            })
            .collect()
    }

    /// The frame's and the leaf's model matrices (`MeshId::DoorFrame`,
    /// `MeshId::DoorLeaf`): the leaf hangs from the gap's left post and
    /// swings into the room.
    pub fn models(&self, now: Time) -> Vec<(DoorId, Mat4, Mat4)> {
        self.doors
            .iter()
            .zip(&self.yaws)
            .map(|(d, &yaw)| {
                let pos = Vec3::new(d.pos.x, 0.0, d.pos.y);
                let turn = Quat::from_rotation_y(-yaw);
                let frame = Mat4::from_rotation_translation(turn, pos);
                let right = turn * Vec3::X;
                let hinge = pos - right * (LEAF_W / 2.0);
                let a = d.openness(now) * OPEN_DEG.to_radians();
                let leaf = Mat4::from_rotation_translation(turn * Quat::from_rotation_y(a), hinge);
                (d.id, frame, leaf)
            })
            .collect()
    }

    /// For `dump`: `[{id, rooms, openness, passable}]`.
    pub fn dump(&self, now: Time) -> serde_json::Value {
        let v: Vec<_> = self
            .doors
            .iter()
            .map(|d| {
                serde_json::json!({
                    "id": d.id.0,
                    "rooms": [d.rooms.0.0, if d.rooms.1 == HALL { -1 } else { d.rooms.1.0 as i64 }],
                    "openness": (d.openness(now) * 1000.0).round() / 1000.0,
                    "passable": d.passable(now, &self.p),
                })
            })
            .collect();
        serde_json::Value::Array(v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nkw_core::strip::rooms_along_hall;
    use nkw_objects::sound::Cue;

    fn rooms() -> Vec<(RoomId, RoomSlot)> {
        rooms_along_hall(&[6.0, 7.5, 6.0])
            .into_iter()
            .enumerate()
            .map(|(i, s)| (RoomId(i as u32), s))
            .collect()
    }

    fn doors() -> Doors {
        let mut d = Doors::default();
        assert!(d.sync(&rooms()));
        d
    }

    const FAR: Vec2 = Vec2::new(-50.0, 0.0);

    #[test]
    fn one_open_door_per_room_kept_across_syncs() {
        let mut d = doors();
        assert_eq!(d.doors.len(), 3);
        assert!(!d.sync(&rooms()), "same rooms");
        d.toggle(DoorId(1), 0.0, true);
        d.tick(0.0, FAR, true);
        assert_eq!(d.get(DoorId(1)).unwrap().state(), State::Closed);
        let mut fewer = rooms();
        fewer.remove(2);
        assert!(d.sync(&fewer));
        assert_eq!(d.doors.len(), 2);
        assert_eq!(
            d.get(DoorId(1)).unwrap().state(),
            State::Closed,
            "state kept"
        );
        assert_eq!(d.get(DoorId(0)).unwrap().state(), State::Open);
    }

    #[test]
    fn obstacles_appear_and_disappear_with_openness() {
        let mut d = doors();
        assert!(d.obstacles(RoomId(0), 0.0).is_empty(), "open: free gap");
        let fx = d.toggle(DoorId(0), 1.0, false);
        assert_eq!(fx, [Effect::Closing]);
        // half way the leaf is not yet across
        let mid = 1.0 + d.p.swing_s * 0.2;
        assert!(d.obstacles(RoomId(0), mid).is_empty());
        let end = 1.0 + d.p.swing_s;
        let fx = d.tick(end, FAR, false);
        assert!(fx.contains(&(DoorId(0), Effect::Cue(Cue::DoorClose))));
        let ob = d.obstacles(RoomId(0), end);
        assert_eq!(ob.len(), 1);
        let (pos, _) = layout::door_world(&rooms()[0].1);
        assert!(ob[0].contains(pos), "the leaf sits in the gap");
        assert!(d.obstacles(RoomId(1), end).is_empty(), "only its own room");
        d.toggle(DoorId(0), 5.0, true);
        assert!(
            d.obstacles(RoomId(0), 5.0).is_empty(),
            "reduced motion: open at once"
        );
    }

    #[test]
    fn opens_rooms_follows_see_through() {
        let mut d = doors();
        let mut v = d.opens_rooms(RoomId(0), false, 0.0);
        v.sort();
        assert_eq!(v, [RoomId(1), RoomId(2)]);
        d.toggle(DoorId(2), 0.0, true);
        d.tick(0.0, FAR, true);
        assert_eq!(d.opens_rooms(RoomId(0), false, 0.0), [RoomId(1)]);
        d.toggle(DoorId(0), 0.0, true);
        d.tick(0.0, FAR, true);
        assert!(
            d.opens_rooms(RoomId(0), false, 0.0).is_empty(),
            "own door shut"
        );
        assert_eq!(
            d.opens_rooms(RoomId(0), true, 0.0),
            [RoomId(1)],
            "from the hall, every open door"
        );
        // touching a shut door shows the room beyond the same instant
        d.toggle(DoorId(0), 3.0, false);
        assert_eq!(d.opens_rooms(RoomId(0), false, 3.0), [RoomId(1)]);
    }

    #[test]
    fn wants_continuous_while_swinging_else_the_auto_close_deadline() {
        let mut d = doors();
        // open and at rest: one wakeup at the auto-close (first tick starts it)
        d.tick(0.0, FAR, false);
        assert_eq!(d.wants(0.0, false), [Want::At(8.0)]);
        d.toggle(DoorId(0), 1.0, false);
        assert_eq!(d.wants(1.1, false), [Want::Continuous]);
        assert!(d.swinging(1.1));
        // after the deadline every door swings shut, then nothing is left
        d.tick(8.0, FAR, false);
        let w = d.wants(8.1, false);
        assert!(w.iter().all(|w| *w == Want::Continuous) && !w.is_empty());
        d.tick(9.0, FAR, false);
        assert!(d.wants(9.0, false).is_empty(), "all closed: no timer");
        // reduced motion: no continuous frames, the openness jumps
        d.toggle(DoorId(1), 10.0, true);
        assert!(!d.wants(10.0, true).contains(&Want::Continuous));
    }

    #[test]
    fn auto_close_waits_while_the_player_is_near() {
        let mut d = doors();
        let (pos, _) = layout::door_world(&rooms()[0].1);
        d.tick(0.0, pos, false);
        d.tick(20.0, pos + Vec2::new(0.5, 0.0), false);
        assert_eq!(d.get(DoorId(0)).unwrap().state(), State::Open);
        assert!(!d.get(DoorId(0)).unwrap().swinging(20.0));
        assert!(
            d.get(DoorId(1)).unwrap().swinging(20.0),
            "the far ones close"
        );
    }

    #[test]
    fn portals_carry_live_openness_to_the_hall() {
        let mut d = doors();
        d.toggle(DoorId(0), 0.0, true);
        d.tick(0.0, FAR, true);
        let p = d.portals(0.0);
        assert_eq!(p.len(), 3);
        assert_eq!(p[0].rooms, (RoomId(0), HALL));
        assert_eq!(p[0].openness, 0.0);
        assert_eq!(p[1].openness, 1.0);
    }

    #[test]
    fn the_leaf_swings_into_the_room() {
        let mut d = doors();
        let interior = |i: usize| match rooms()[i].1.side {
            Side::North => -1.0,
            Side::South => 1.0,
        };
        for i in 0..2 {
            let (_, frame, shut) = d.models(0.0)[i];
            let tip_shut = shut.transform_point3(Vec3::new(LEAF_W, 0.0, 0.0));
            let mid = frame.transform_point3(Vec3::ZERO);
            assert!(
                (tip_shut - mid).length() > 0.4,
                "open leaf is not across the gap"
            );
            assert!(
                (tip_shut.z - mid.z) * interior(i) > 0.5,
                "open into the room"
            );
            let _ = &mut d;
        }
        d.toggle(DoorId(0), 0.0, true);
        d.tick(0.0, FAR, true);
        let (_, frame, leaf) = d.models(0.0)[0];
        let tip = leaf.transform_point3(Vec3::new(LEAF_W, 0.0, 0.0));
        let mid = frame.transform_point3(Vec3::ZERO);
        assert!((tip.z - mid.z).abs() < 1e-4, "closed: across the gap");
        assert!(((tip.x - mid.x) - LEAF_W / 2.0).abs() < 1e-4);
    }

    #[test]
    fn hall_test() {
        assert!(in_hall(Vec2::new(3.0, 0.0)));
        assert!(!in_hall(Vec2::new(3.0, -2.0)));
    }

    #[test]
    fn dump_shape() {
        let d = doors();
        let v = d.dump(0.0);
        assert_eq!(v[0]["id"], 0);
        assert_eq!(v[0]["rooms"], serde_json::json!([0, -1]));
        assert_eq!(v[0]["passable"], true);
        assert_eq!(v[0]["openness"], 1.0);
    }
}
