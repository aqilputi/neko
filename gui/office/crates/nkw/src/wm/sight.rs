//! What the World tells the overlay about walls and the runtime `[walk]`
//! knobs (S22): `room_at`, `line_of_sight` (the tags stop showing through
//! walls) and `walk_set` (the phone's Ajustes). A child module of `world`.

use std::hash::{Hash, Hasher};

use glam::Vec3;
use nkw_core::RoomId;
use nkw_core::collide::{self, WallPlan};
use nkw_core::layout;

use super::World;

/// The cached wall plan and the layout it was built for.
pub(super) type LosCache = std::cell::RefCell<Option<(u64, WallPlan)>>;

impl World {
    /// The room `p` is in; `None` in the hall.
    pub fn room_at(&self, p: Vec3) -> Option<RoomId> {
        self.room_index_at(p).map(|i| self.rooms[i].id)
    }

    /// The rooms' walls as `collide::line_of_sight` takes them: every
    /// room's slot, its hall door (`DOOR_W` wide), the shell's wall height.
    pub fn wall_plan(&self) -> WallPlan {
        let slots = self.slots();
        WallPlan {
            rooms: self
                .rooms
                .iter()
                .zip(&slots)
                .map(|(r, s)| (r.id, *s))
                .collect(),
            doors: self
                .rooms
                .iter()
                .zip(&slots)
                .map(|(r, s)| (r.id, layout::door_world(s).0, collide::DOOR_W))
                .collect(),
            wall_h: nkw_gl::shell::WALL_H,
        }
    }

    /// What the wall plan depends on: rooms, desk widths, the output.
    fn plan_key(&self) -> u64 {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        self.rooms.len().hash(&mut h);
        for r in &self.rooms {
            r.id.0.hash(&mut h);
            r.desks.len().hash(&mut h);
            for d in &r.desks {
                (d.width as u8).hash(&mut h);
            }
        }
        (
            self.out.width_px,
            self.out.height_px,
            self.out.scale.to_bits(),
        )
            .hash(&mut h);
        h.finish()
    }

    /// True when no wall of the rooms cuts the segment `from`–`to`; a
    /// doorway counts as open when `open` lists the room beyond (the
    /// culling set the hub passes: `ctx.visible_rooms`). The wall plan is
    /// cached per layout.
    pub fn line_of_sight(&self, from: Vec3, to: Vec3, open: &[RoomId]) -> bool {
        let key = self.plan_key();
        let mut c = self.los.borrow_mut();
        if c.as_ref().is_none_or(|(k, _)| *k != key) {
            *c = Some((key, self.wall_plan()));
        }
        let plan = &c.as_ref().expect("just built").1;
        collide::line_of_sight(plan, from, to, open)
    }

    /// A runtime `[walk]` change (not persisted): `turn` smooth|snap,
    /// `mouse_look`, `comfort_vignette`, `speed`, `run_mul`, `snap_deg`,
    /// `eye_height` (clamped like `world.toml`); `Err` (pt-BR) on an
    /// unknown key or value, nothing changed then.
    pub fn walk_set(&mut self, key: &str, value: &str) -> Result<(), String> {
        let mut c = *self.walk_config();
        c.set(key, value)?;
        self.set_walk_config(c);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::{Cmd, Opts, WALK_SET};
    use nkw_core::{Output, TurnMode};
    use std::collections::BTreeMap;

    fn demo() -> World {
        let out = Output {
            width_px: 1280,
            height_px: 720,
            scale: 1.0,
        };
        crate::demo::world(
            out,
            Opts {
                reduced_motion: false,
            },
        )
    }

    #[test]
    fn room_at_and_the_hall() {
        let w = demo();
        let slots = w.slots();
        for (r, s) in w.rooms.iter().zip(&slots) {
            let (door, yaw) = layout::door_world(s);
            let inside = Vec3::new(door.x, 1.5, door.y) + layout::facing(yaw) * 2.0;
            assert_eq!(w.room_at(inside), Some(r.id), "{}", r.name);
            let hall = Vec3::new(door.x, 1.5, 0.0);
            assert_eq!(w.room_at(hall), None, "the hall");
        }
    }

    /// Each seated cat's head, with its room.
    fn heads(w: &World) -> Vec<(RoomId, Vec3)> {
        let mut v = Vec::new();
        for r in &w.rooms {
            for (d, desk) in r.desks.iter().enumerate() {
                if desk.cat.is_some() {
                    let seat = w.cat_seat(r.id, d).unwrap();
                    v.push((r.id, seat.pos + Vec3::Y * 0.8));
                }
            }
        }
        v
    }

    #[test]
    fn tags_of_the_next_rooms_cats_are_behind_walls() {
        let w = demo();
        let cats = heads(&w);
        assert!(cats.len() >= 2, "the fixture has cats");
        for (i, r) in w.rooms.iter().enumerate() {
            let Some(cam) = w.station_pose(&nkw_core::Station::Door(r.id)) else {
                continue;
            };
            let open = [r.id];
            for (room, head) in &cats {
                let seen = w.line_of_sight(cam.pos, *head, &open);
                assert_eq!(seen, *room == r.id, "from room {i} to a cat of {room:?}");
                // through the open doors of both rooms from the hall: a cat
                // seen from its own doorway
            }
        }
        // the plan follows the layout: a new room moves the walls
        let before = w.wall_plan();
        let mut w2 = w.clone();
        w2.command(
            Cmd::RoomNew {
                name: crate::world::ShortName::new("bravo"),
                context: false,
                go: false,
            },
            0.0,
        );
        assert_ne!(w2.wall_plan(), before);
        assert_eq!(w2.wall_plan().rooms.len(), before.rooms.len() + 1);
        assert_eq!(before.wall_h, nkw_gl::shell::WALL_H);
    }

    #[test]
    fn line_of_sight_is_cheap() {
        let w = demo();
        let cats = heads(&w);
        let cam = w.camera.pos;
        let open = w.visible_rooms(&[]);
        let n = 2000;
        let t = std::time::Instant::now();
        let mut seen = 0;
        for k in 0..n {
            let (_, p) = cats[k % cats.len()];
            seen += usize::from(w.line_of_sight(cam, p, &open));
        }
        let per = t.elapsed() / n as u32;
        assert!(seen > 0);
        // ≤ 2 µs per tag (release); 10× slack for the dev profile
        assert!(
            per < std::time::Duration::from_micros(20),
            "{per:?} per tag"
        );
    }

    #[test]
    fn walk_set_clamps_and_says_no() {
        let mut w = demo();
        assert_eq!(w.walk_set("turn", "snap"), Ok(()));
        assert_eq!(w.walk_config().turn, TurnMode::Snap);
        assert_eq!(w.walk_params.turn, TurnMode::Snap, "the walk params follow");
        assert_eq!(w.walk_set("speed", "9"), Ok(()));
        assert_eq!(w.walk_config().speed, 5.0, "clamped");
        assert_eq!(w.walk_set("mouse_look", "false"), Ok(()));
        assert!(!w.walk_config().mouse_look);
        assert_eq!(w.walk_set("eye_height", "1.7"), Ok(()));
        assert!((w.walk_params.eye_height - 1.7).abs() < 1e-6);
        let before = *w.walk_config();
        assert!(w.walk_set("colour", "red").is_err());
        assert!(w.walk_set("speed", "rápido").is_err());
        assert_eq!(*w.walk_config(), before, "unchanged");
        // through the command, with the error as the layout note
        let ok = BTreeMap::from([
            ("key".to_string(), "comfort_vignette".to_string()),
            ("value".to_string(), "true".to_string()),
        ]);
        let c = Cmd::from_name(WALK_SET, &ok).unwrap();
        assert_eq!(Cmd::from_name(c.name().0, &c.name().1), Some(c));
        let g0 = w.generation();
        assert!(w.command(c, 1.0).is_empty());
        assert!(w.walk_config().comfort_vignette);
        assert_eq!(w.generation(), g0, "not a layout change");
        let bad = BTreeMap::from([
            ("key".to_string(), "turn".to_string()),
            ("value".to_string(), "jump".to_string()),
        ]);
        w.command(Cmd::from_name(WALK_SET, &bad).unwrap(), 2.0);
        assert!(w.layout_note(2.0).is_some_and(|(n, _)| n.contains("turn")));
        assert_eq!(
            Cmd::from_name(WALK_SET, &BTreeMap::new()),
            None,
            "needs both"
        );
        assert!(
            crate::world::command_names()
                .iter()
                .any(|(n, _)| *n == WALK_SET)
        );
    }
}
