//! ANDAR's additions to the [`World`] (§3.5, wave 3, contract S15): the
//! `[walk]`/`[camera]` config, mouse-look, the aim, carrying a screen, the
//! comfort vignette and the library room.
//!
//! A child module of `world` (like `world_ops.rs`), so it reaches the
//! World's private state; pure like it.

use glam::{Mat4, Vec2, Vec3};
use nkw_core::aim::{self, Candidate};
use nkw_core::carry::{self, DeskSlot, Drop, WallSeg};
use nkw_core::collide::{AIM_CONE_RAD, Aabb, WalkParams, ray_hit_quad};
use nkw_core::persist::RoomKind;
use nkw_core::strip::{Side, Width};
use nkw_core::{CatId, Pose, RoomId, ScreenId, ScreenPlace, Time, TurnMode, Want, Wish, onetoone};
use serde::Serialize;

use super::{HALL_W, Out, ROOM_DEPTH, WalkConfig, World};

/// What the World itself can aim at (the objects plugin aims at its own).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Aim {
    Screen(ScreenId),
    Cat(CatId),
    Door(RoomId),
}

/// The screen in hand.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Carry {
    pub screen: ScreenId,
    /// metres ahead of the eye (`carry::HAND_MIN..HAND_MAX`)
    pub dist: f32,
    /// a new width preset chosen in hand (`-`/`=`), configured on drop
    pub width: Option<Width>,
}

/// The comfort vignette ramps this long (s).
pub const VIGNETTE_RAMP: f64 = 0.12;
/// A seated cat's face, above its seat (m), and its aim sphere.
pub const CAT_FACE_UP: f32 = 0.35;
pub const CAT_FACE_R: f32 = 0.3;
/// A door leaf's aim sphere: centre height and radius (m).
pub const DOOR_AIM_Y: f32 = 1.0;
pub const DOOR_AIM_R: f32 = 0.6;
/// `t` from a `Free` station reaches a seated cat this far (m) ...
pub const TALK_VIEW_M: f32 = 4.0;
/// ... this far off the heading (degrees).
pub const TALK_VIEW_DEG: f32 = 60.0;
/// The librarian (R4's fixture villager): she sits at the library's desk 0.
pub const LIBRARIAN: CatId = CatId(999_000);
/// The name of the library room `world.library` makes.
pub const LIBRARY: &str = "biblioteca";
/// Mouse motion within this long counts as turning (the vignette).
const LOOK_HOLD: f64 = 0.1;
/// An air screen blocks walking within this depth (m) of its plane.
const AIR_SCREEN_DEPTH: f32 = 0.1;

impl World {
    /// `[walk]` + `[camera]` (the hub calls it once after `Ext::new`): the
    /// walking params, the exit rule and the eye height (NAV poses unchanged).
    pub fn set_walk_config(&mut self, c: WalkConfig) {
        self.walk_cfg = c;
        let mut p = WalkParams::from_config(
            c.speed,
            c.run_mul,
            c.turn,
            c.turn_rate_dps,
            c.snap_deg,
            c.pitch_max_deg,
            c.eye_height,
        );
        if c.mouse_deg_per_count.is_finite() && c.mouse_deg_per_count > 0.0 {
            p.mouse = c.mouse_deg_per_count;
        }
        self.walk_params = p;
        if let Some(w) = self.walking.as_mut() {
            w.walker.pitch = w
                .walker
                .pitch
                .clamp(-p.pitch_max.to_radians(), p.pitch_max.to_radians());
            self.camera = w.walker.pose(&p);
        }
    }

    pub fn walk_config(&self) -> &WalkConfig {
        &self.walk_cfg
    }

    /// A relative pointer motion in ANDAR (counts, x right, y down): yaw
    /// and pitch at `mouse_deg_per_count`, the pitch clamped, never a roll.
    /// NAV: nothing. Reduced motion changes nothing (the mouse is you).
    pub fn look_delta(&mut self, dx: f32, dy: f32) {
        if !dx.is_finite() || !dy.is_finite() || (dx == 0.0 && dy == 0.0) {
            return;
        }
        let p = self.walk_params;
        let Some(w) = self.walking.as_mut() else {
            return;
        };
        let k = p.mouse.to_radians();
        let max = p.pitch_max.to_radians();
        w.walker.yaw = nkw_core::angle_diff(w.walker.yaw + dx * k, 0.0);
        w.walker.pitch = (w.walker.pitch - dy * k).clamp(-max, max);
        w.looked = w.last;
        let t = w.last;
        self.camera = w.walker.pose(&p);
        self.vignette_step(t);
        self.pending_once();
    }

    /// A turn key went down in ANDAR (`sign` −1 left, +1 right): in snap
    /// mode one step, at once; else the smooth turn is held. Returns
    /// whether it held (smooth).
    pub(super) fn turn_press(&mut self, sign: f32) -> bool {
        let p = self.walk_params;
        if p.turn != TurnMode::Snap {
            return true;
        }
        if let Some(w) = self.walking.as_mut() {
            w.walker.yaw =
                nkw_core::angle_diff(w.walker.yaw + sign * p.snap_step.to_radians(), 0.0);
            w.looked = w.last;
            let t = w.last;
            self.camera = w.walker.pose(&p);
            self.vignette_step(t);
            self.pending_once();
        }
        false
    }

    fn pending_once(&mut self) {
        let w = Wish::world(Want::Once);
        if !self.pending.contains(&w) {
            self.pending.push(w);
        }
    }

    /// From a `Free` station (or walking): the nearest seated cat of the
    /// room in view, within [`TALK_VIEW_M`] and [`TALK_VIEW_DEG`] of the
    /// heading (plan 3.5: from `Free`, `t` talks to the nearest cat in view).
    pub(super) fn cat_in_view(&self) -> Option<CatId> {
        if !matches!(self.station, nkw_core::strip::Station::Free(_)) && self.walking.is_none() {
            return None;
        }
        let ri = self.room_index_at(self.camera.pos)?;
        let room = &self.rooms[ri];
        let eye = Vec2::new(self.camera.pos.x, self.camera.pos.z);
        let heading = self.camera.heading();
        let max = TALK_VIEW_DEG.to_radians().cos();
        room.desks
            .iter()
            .enumerate()
            .filter_map(|(d, desk)| {
                let cat = desk.cat?;
                let seat = self.cat_seat(room.id, d)?;
                let to = Vec2::new(seat.pos.x, seat.pos.z) - eye;
                let dist = to.length();
                (dist <= TALK_VIEW_M && (dist < 1e-3 || to.dot(heading) / dist >= max))
                    .then_some((dist, cat))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|x| x.1)
    }

    /// The camera position and its unit facing (pitch included).
    pub fn aim_ray(&self) -> (Vec3, Vec3) {
        (self.camera.pos, self.camera.forward())
    }

    /// The World's own targets in the aim (ANDAR only): the nearest screen
    /// quad the ray hits within reach, a seated cat (a sphere at its face)
    /// or a doorway inside the 8° cone; the screen in hand never.
    pub fn aim(&self) -> Option<Aim> {
        self.walking.as_ref()?;
        let ray = self.aim_ray();
        let reach = self.walk_params.reach;
        let mut best: Option<(f32, Aim)> = None;
        let mut take = |t: f32, a: Aim| {
            if t <= reach && best.is_none_or(|(b, _)| t < b) {
                best = Some((t, a));
            }
        };
        let here = self.room_index_at(self.camera.pos);
        let mut spheres = Vec::new();
        for (ri, room) in self.rooms.iter().enumerate() {
            if let Some((xz, _)) = self.room_door(room.id) {
                spheres.push(Candidate {
                    what: Aim::Door(room.id),
                    centre: Vec3::new(xz.x, DOOR_AIM_Y, xz.y),
                    radius: DOOR_AIM_R,
                });
            }
            if here != Some(ri) {
                continue;
            }
            for (di, desk) in room.desks.iter().enumerate() {
                for (k, sid) in desk.screens.iter().enumerate() {
                    if self.carrying() == Some(*sid) {
                        continue;
                    }
                    let model = self.screen_model(ri, di, k);
                    if let Some(t) = ray_hit_quad(ray.0, ray.1, model) {
                        take(t, Aim::Screen(*sid));
                    }
                }
                if let Some(cat) = desk.cat
                    && let Some(seat) = self.cat_seat(room.id, di)
                {
                    spheres.push(Candidate {
                        what: Aim::Cat(cat),
                        centre: seat.pos + Vec3::Y * CAT_FACE_UP,
                        radius: CAT_FACE_R,
                    });
                }
            }
        }
        if let Some(a) = aim::pick(ray, &spheres, reach, AIM_CONE_RAD)
            && let Some(c) = spheres.iter().find(|c| c.what == a)
        {
            take((c.centre - ray.0).length() - c.radius, a);
        }
        best.map(|b| b.1)
    }

    /// The unit quad of screen `k` of a desk, placed (centre, yaw, pitch,
    /// size in metres).
    fn screen_model(&self, room: usize, desk: usize, k: usize) -> Mat4 {
        let p = self.screen_place(room, desk, k);
        let size = self.screen_logical(room, desk, k) * onetoone::M_PER_PX;
        Mat4::from_translation(p.center)
            * Mat4::from_rotation_y(-p.yaw)
            * Mat4::from_rotation_x(p.pitch)
            * Mat4::from_scale(Vec3::new(size.x, size.y, 1.0))
    }

    /// The screen in hand.
    pub fn carrying(&self) -> Option<ScreenId> {
        self.carry.map(|c| c.screen)
    }

    /// Where the screen in hand hangs (`None`: `id` is not in hand).
    pub(super) fn hand_place(&self, id: ScreenId) -> Option<ScreenPlace> {
        let c = self.carry.filter(|c| c.screen == id)?;
        let p = carry::hand_pose(&self.camera, c.dist);
        Some(ScreenPlace {
            center: p.pos,
            yaw: p.yaw,
            pitch: 0.0,
        })
    }

    /// The logical size of the screen in hand when `-`/`=` changed it.
    pub(super) fn hand_logical(&self, id: ScreenId) -> Option<Vec2> {
        let c = self.carry.filter(|c| c.screen == id)?;
        c.width.map(|w| self.configure_size(w, 1))
    }

    /// The comfort vignette, 0..1: up over 120 ms while walking or turning,
    /// down over 120 ms when still; 0 when `comfort_vignette` is off.
    pub fn comfort_vignette(&self, now: Time) -> f32 {
        if !self.walk_cfg.comfort_vignette {
            return 0.0;
        }
        let (t, rising, from) = self.vignette;
        let k = ((now - t) / VIGNETTE_RAMP).clamp(0.0, 1.0) as f32;
        if rising {
            from + (1.0 - from) * k
        } else {
            from * (1.0 - k)
        }
    }

    /// The vignette is still ramping (or the mouse just turned you): the
    /// World keeps frames coming (the overlay has no wish of its own).
    pub(super) fn vignette_busy(&self) -> bool {
        if !self.walk_cfg.comfort_vignette {
            return false;
        }
        let (t, rising, from) = self.vignette;
        let ramping = self.last_frame < t + VIGNETTE_RAMP && (rising || from > 0.0);
        let looking = self
            .walking
            .as_ref()
            .is_some_and(|w| self.last_frame < w.looked + LOOK_HOLD);
        ramping || looking
    }

    /// Follow the vignette's ramp after a walking step (no allocation).
    pub(super) fn vignette_step(&mut self, now: Time) {
        let moving = self
            .walking
            .as_ref()
            .is_some_and(|w| w.keys.any() || now - w.looked < LOOK_HOLD);
        if moving != self.vignette.1 {
            let v = self.comfort_vignette(now);
            self.vignette = (now, moving, v);
        }
    }

    /// The first room of `kind`, or a new one named `name` (the library
    /// right after the reception; others before "minha sala"); a room
    /// by that name of another kind takes the kind. Persisted like any.
    pub fn ensure_room(&mut self, kind: RoomKind, name: &str, now: Time) -> (RoomId, Vec<Out>) {
        let id = self.ensure_room_bare(kind.clone(), name, now);
        if kind == RoomKind::Library {
            self.library_counter(id);
        }
        (id, Vec::new())
    }

    /// The library's desk 0 is always the librarian's counter (her
    /// [`LIBRARIAN`] cat, no screens): made when missing, kept first.
    pub(super) fn library_counter(&mut self, id: RoomId) {
        let Some(i) = self.room_index(id) else {
            return;
        };
        let desks = &self.rooms[i].desks;
        if desks.first().is_some_and(|d| d.cat == Some(LIBRARIAN)) {
            return;
        }
        let anchor = self.anchor();
        let desks = &mut self.rooms[i].desks;
        let counter = match desks.iter().position(|d| d.cat == Some(LIBRARIAN)) {
            Some(k) => desks.remove(k),
            None => nkw_core::strip::Desk {
                width: Width::Half,
                screens: Vec::new(),
                cat: Some(LIBRARIAN),
            },
        };
        desks.insert(0, counter);
        self.station = self.resolve(anchor);
        self.layout_gen += 1;
        self.pending_once();
    }

    fn ensure_room_bare(&mut self, kind: RoomKind, name: &str, now: Time) -> RoomId {
        let _ = now;
        if let Some(r) = self.rooms.iter().find(|r| self.room_kind(r.id) == kind) {
            return r.id;
        }
        if let Some(i) = self.rooms.iter().position(|r| r.name == name) {
            let id = self.rooms[i].id;
            self.kinds.insert(id, kind);
            self.layout_gen += 1;
            return id;
        }
        let anchor = self.anchor();
        let room = self.new_room(name);
        let id = room.id;
        self.kinds.insert(id, kind.clone());
        let at = match kind {
            RoomKind::Library => self
                .rooms
                .iter()
                .position(|r| self.room_kind(r.id) == RoomKind::Reception)
                .map_or(0, |i| i + 1),
            _ => self
                .rooms
                .iter()
                .rposition(|r| self.room_kind(r.id) == RoomKind::User)
                .unwrap_or(self.rooms.len()),
        };
        self.rooms.insert(at, room);
        // the strip moved under the camera: the station keeps its place
        self.station = self.resolve(anchor);
        if !self.gliding()
            && self.mode().overlay.is_none()
            && !self.mode().walking()
            && self.focus_2d.is_none()
            && let Some(p) = self.station_pose(&self.station)
        {
            self.camera = p;
        }
        self.layout_gen += 1;
        self.pending_once();
        id
    }

    /// `world.library`: the library room (made when missing), then there
    /// (`go`; a restore only makes it).
    pub(super) fn library(&mut self, go: bool, now: Time) -> Vec<Out> {
        let (id, out) = self.ensure_room(RoomKind::Library, LIBRARY, now);
        if go && let Some(i) = self.room_index(id) {
            self.go_room(i, now);
        }
        out
    }

    /// `world.grab`: in ANDAR pick up the aimed screen, or drop the one in
    /// hand; elsewhere a note.
    pub(super) fn grab(&mut self, now: Time) -> Vec<Out> {
        if self.carry.is_some() {
            return self.drop_hand(now, false);
        }
        if self.walking.is_none() {
            self.note = Some((now, "só andando (w)".into()));
            self.pending.push(Wish::compose(Want::Once));
            return Vec::new();
        }
        let Some(Aim::Screen(s)) = self.aim() else {
            self.note = Some((now, "nada na mira".into()));
            self.pending.push(Wish::compose(Want::Once));
            return Vec::new();
        };
        // a dialog goes with its mother: take the mother
        let mut s = s;
        for _ in 0..8 {
            match self.screen_state(s).parent {
                Some(p) if self.screens.contains_key(&p) => s = p,
                _ => break,
            }
        }
        let Some((r, d, _)) = self.locate(s) else {
            return Vec::new();
        };
        let was_stacked = !self.is_loose(s);
        if let Some(st) = self.wm.get_mut(&s) {
            st.air = None;
            st.wall = false;
        }
        let desk = &mut self.rooms[r].desks[d];
        desk.screens.retain(|x| *x != s);
        desk.screens.push(s);
        self.carry = Some(Carry {
            screen: s,
            dist: carry::HAND_DEFAULT,
            width: None,
        });
        let out = if was_stacked {
            self.configure_desk(r, d)
        } else {
            Vec::new()
        };
        self.changed(now, Some("na mão".into()));
        out
    }

    /// `[` (−1) / `]` (+1): the screen in hand nearer / farther.
    pub(super) fn hand_step(&mut self, sign: f32, now: Time) -> Vec<Out> {
        match self.carry.as_mut() {
            Some(c) => {
                c.dist = (c.dist + sign * carry::HAND_STEP).clamp(carry::HAND_MIN, carry::HAND_MAX);
                self.pending_once();
            }
            None => {
                self.note = Some((now, "nada na mão".into()));
                self.pending.push(Wish::compose(Want::Once));
            }
        }
        Vec::new()
    }

    /// `-` / `=` with a screen in hand: its width preset (configured on
    /// drop). Returns `None` with nothing in hand.
    pub(super) fn hand_width(&mut self, f: fn(Width) -> Width, now: Time) -> Option<Vec<Out>> {
        let c = self.carry?;
        let cur = c.width.unwrap_or_else(|| {
            self.locate(c.screen)
                .map(|(r, d, _)| self.rooms[r].desks[d].width)
                .unwrap_or_default()
        });
        let new = f(cur);
        if let Some(c) = self.carry.as_mut() {
            c.width = Some(new);
        }
        self.changed(now, Some(super::width_label(new).into()));
        Some(Vec::new())
    }

    /// Drop the screen in hand (`air_only`: right where it is, as leaving
    /// ANDAR does) per `carry::drop_target`.
    pub(super) fn drop_hand(&mut self, now: Time, air_only: bool) -> Vec<Out> {
        let Some(c) = self.carry else {
            return Vec::new();
        };
        let s = c.screen;
        let hand = carry::hand_pose(&self.camera, c.dist);
        let here = self
            .room_index_at(self.camera.pos)
            .unwrap_or(self.current_room());
        let target = if air_only {
            Drop::Air(hand)
        } else {
            carry::drop_target(&hand, &self.desk_slots(here), &self.wall_segs(here))
        };
        self.carry = None;
        let Some((sr, sd, _)) = self.locate(s) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        let note = match target {
            Drop::Desk { room, desk } => {
                let t = self.room_index(room).unwrap_or(here);
                let moved = self.detach(sr, sd, s);
                self.attach(t, desk, &moved);
                if let Some(w) = c.width {
                    self.rooms[t].desks[desk].width = w;
                }
                out.extend(self.configure_desk(t, desk));
                let slot = self.stack_len(t, desk) - 1;
                let gone = (sr, sd) != (t, desk) && self.drop_if_empty(sr, sd);
                let desk = if gone && sr == t && sd < desk {
                    desk - 1
                } else {
                    desk
                };
                self.set_station_desk(t, desk, slot);
                "na mesa"
            }
            Drop::Wall(p) | Drop::Air(p) => {
                let wall = matches!(target, Drop::Wall(_));
                let st = self.wm.entry(s).or_default();
                st.air = Some(ScreenPlace {
                    center: p.pos,
                    yaw: p.yaw,
                    pitch: 0.0,
                });
                st.wall = wall;
                // into the room it was dropped in
                if sr != here && !self.rooms[here].desks.is_empty() {
                    let moved = self.detach(sr, sd, s);
                    let last = self.rooms[here].desks.len() - 1;
                    self.attach(here, last, &moved);
                    self.drop_if_empty(sr, sd);
                }
                if let Some(w) = c.width {
                    out.push(Out::Configure {
                        screen: s,
                        size: Some(self.configure_size(w, 1)),
                        fullscreen: None,
                        maximized: None,
                    });
                }
                if wall { "na parede" } else { "no ar" }
            }
        };
        self.refresh_walk_grid();
        self.changed(now, Some(note.into()));
        out
    }

    /// The walking grid again (an air screen came or went).
    pub(super) fn refresh_walk_grid(&mut self) {
        if self.walking.is_none() {
            return;
        }
        let here = self
            .room_index_at(self.camera.pos)
            .unwrap_or(self.current_room());
        let g = self.room_grid(here);
        if let Some(w) = self.walking.as_mut() {
            w.grid = g;
        }
    }

    /// The desks of a room as drop slots (their stack centre).
    fn desk_slots(&self, room: usize) -> Vec<DeskSlot> {
        let id = self.rooms[room].id;
        let slot = self.slots()[room];
        (0..self.rooms[room].desks.len())
            .map(|d| {
                // the desk's monitor, whatever hangs there now (the screen in
                // hand may be one of its screens)
                let size = self.configure_size(self.rooms[room].desks[d].width, 1);
                let p = nkw_core::layout::monitor_place(
                    &slot,
                    self.desk_x(room, d),
                    size * onetoone::M_PER_PX,
                );
                DeskSlot {
                    room: id,
                    desk: d,
                    pos: p.center,
                }
            })
            .collect()
    }

    /// The floor box of a room (x, z).
    pub(super) fn room_floor(&self, room: usize) -> Aabb {
        let s = self.slots()[room];
        let (z0, z1) = match s.side {
            Side::North => (-(HALL_W / 2.0 + ROOM_DEPTH), -HALL_W / 2.0),
            Side::South => (HALL_W / 2.0, HALL_W / 2.0 + ROOM_DEPTH),
        };
        Aabb::new(Vec2::new(s.x0, z0), Vec2::new(s.x0 + s.len, z1))
    }

    /// A room's four walls, normals into the room.
    fn wall_segs(&self, room: usize) -> Vec<WallSeg> {
        let f = self.room_floor(room);
        let (a, b) = (f.min, f.max);
        vec![
            WallSeg {
                a,
                b: Vec2::new(b.x, a.y),
                normal: Vec2::Y,
            },
            WallSeg {
                a: Vec2::new(a.x, b.y),
                b,
                normal: Vec2::NEG_Y,
            },
            WallSeg {
                a,
                b: Vec2::new(a.x, b.y),
                normal: Vec2::X,
            },
            WallSeg {
                a: Vec2::new(b.x, a.y),
                b,
                normal: Vec2::NEG_X,
            },
        ]
    }

    /// The floor boxes of a room's air screens that hang low enough to
    /// block walking (`collide::air_screen_blocks`).
    pub(super) fn air_obstacles(&self, room: usize) -> Vec<Aabb> {
        let mut v = Vec::new();
        for (di, desk) in self.rooms[room].desks.iter().enumerate() {
            for (k, sid) in desk.screens.iter().enumerate() {
                if self.screen_state(*sid).air.is_none() || self.carrying() == Some(*sid) {
                    continue;
                }
                let p = self.screen_place(room, di, k);
                let size = self.screen_logical(room, di, k) * onetoone::M_PER_PX;
                if !nkw_core::collide::air_screen_blocks(p.center.y - size.y / 2.0) {
                    continue;
                }
                let pose = Pose::new(p.center, p.yaw, 0.0);
                let r = pose.right() * (size.x / 2.0);
                let n = pose.forward() * (AIR_SCREEN_DEPTH / 2.0);
                let pts = [
                    p.center + r + n,
                    p.center + r - n,
                    p.center - r + n,
                    p.center - r - n,
                ];
                let min = pts
                    .iter()
                    .fold(Vec2::splat(f32::MAX), |m, q| m.min(Vec2::new(q.x, q.z)));
                let max = pts
                    .iter()
                    .fold(Vec2::splat(f32::MIN), |m, q| m.max(Vec2::new(q.x, q.z)));
                v.push(Aabb::new(min, max));
            }
        }
        v
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::router::sym;
    use crate::world::{Cmd, MY_ROOM, Opts, RECEPTION, Room, Screen, command_names};
    use nkw_core::strip::{Desk, Station};
    use nkw_core::{Output, angle_diff};

    const OUT: Output = Output {
        width_px: 1280,
        height_px: 720,
        scale: 1.0,
    };
    const OPTS: Opts = Opts {
        reduced_motion: false,
    };

    fn desk(screens: &[u32], cat: Option<u32>) -> Desk {
        Desk {
            width: Width::Half,
            screens: screens.iter().map(|&s| ScreenId(s)).collect(),
            cat: cat.map(CatId),
        }
    }

    /// acme: screen 1 | screen 2 on cat 9's desk | cat 7's desk, no screen.
    fn acme() -> World {
        let rooms = vec![Room {
            id: RoomId(0),
            name: "acme".into(),
            desks: vec![desk(&[1], None), desk(&[2], Some(9)), desk(&[], Some(7))],
        }];
        let mut w = World::with_rooms(OUT, OPTS, rooms);
        for i in 1..=2 {
            w.screens.insert(
                ScreenId(i),
                Screen {
                    id: ScreenId(i),
                    app_id: format!("acme.{i}"),
                    title: String::new(),
                    size: Vec2::new(624.0, 720.0),
                },
            );
        }
        w.next_id = 3;
        w
    }

    fn walking(w: &mut World) -> Time {
        w.command(Cmd::Walk, 0.0);
        assert!(w.walking.is_some());
        0.0
    }

    fn step(w: &mut World, now: &mut Time, secs: f64) {
        for _ in 0..(secs * 60.0).round() as usize {
            *now += 1.0 / 60.0;
            w.tick(*now);
        }
    }

    fn run(w: &mut World, name: &str, now: Time) -> Vec<Out> {
        let c = Cmd::from_name(name, &BTreeMap::new()).unwrap_or_else(|| panic!("{name}"));
        w.command(c, now)
    }

    #[test]
    fn look_delta_clamps_the_pitch_wraps_the_yaw_and_never_rolls() {
        let mut w = acme();
        let nav = w.camera;
        w.look_delta(100.0, 0.0);
        assert_eq!(w.camera, nav, "NAV: no-op");
        walking(&mut w);
        let k = 0.12f32.to_radians();
        // (dx, dy) cumulative → (yaw, pitch)
        let max = 55f32.to_radians();
        let rows: [(f32, f32, f32, f32); 6] = [
            (100.0, 0.0, 100.0 * k, 0.0),
            (0.0, -100.0, 100.0 * k, 100.0 * k),
            (0.0, -1000.0, 100.0 * k, max),
            (0.0, 3000.0, 100.0 * k, -max),
            (-100.0, 0.0, 0.0, -max),
            (1500.0, 0.0, angle_diff(1500.0 * k, 0.0), -max),
        ];
        for (i, (dx, dy, yaw, pitch)) in rows.into_iter().enumerate() {
            w.look_delta(dx, dy);
            assert!(
                angle_diff(w.camera.yaw, yaw).abs() < 1e-4,
                "row {i}: {}",
                w.camera.yaw
            );
            assert!(
                (w.camera.pitch - pitch).abs() < 1e-4,
                "row {i}: {}",
                w.camera.pitch
            );
            assert!(w.camera.yaw.abs() <= std::f32::consts::PI + 1e-6, "wrapped");
            assert!(w.camera.right().y.abs() < 1e-6, "no roll");
        }
        w.look_delta(f32::NAN, 1.0);
        assert!(w.camera.pitch.is_finite());
        // the reduced motion does not change the mouse
        let mut r = acme();
        r.opts.reduced_motion = true;
        walking(&mut r);
        r.look_delta(100.0, 0.0);
        assert!((r.camera.yaw - 100.0 * k).abs() < 1e-4);
    }

    #[test]
    fn the_config_sets_speed_eye_and_snap_turns() {
        let mut w = acme();
        let c = WalkConfig {
            speed: 4.0,
            eye_height: 1.7,
            turn: TurnMode::Snap,
            snap_deg: 45.0,
            pitch_max_deg: 30.0,
            mouse_deg_per_count: 0.2,
            ..WalkConfig::default()
        };
        w.set_walk_config(c);
        assert_eq!(*w.walk_config(), c);
        assert_eq!(w.walk_params.speed, 4.0);
        assert_eq!(w.walk_params.mouse, 0.2);
        let nav = w.camera;
        assert_eq!(nav.pos.y, crate::world::EYE, "NAV poses unchanged");
        let mut now = walking(&mut w);
        assert!((w.camera.pos.y - 1.7).abs() < 1e-6, "eye height in ANDAR");
        let yaw = w.camera.yaw;
        assert!(w.walk_key(0x65, true), "e");
        assert!(
            (angle_diff(w.camera.yaw, yaw) - 45f32.to_radians()).abs() < 1e-5,
            "one step, at once"
        );
        step(&mut w, &mut now, 0.5);
        assert!(
            (angle_diff(w.camera.yaw, yaw) - 45f32.to_radians()).abs() < 1e-5,
            "holding adds nothing"
        );
        assert!(w.walk_key(0x65, false));
        // by name too: world.turn_left {pressed}
        let press = BTreeMap::from([("pressed".to_string(), "true".to_string())]);
        w.command(Cmd::from_name("world.turn_left", &press).unwrap(), now);
        assert!(angle_diff(w.camera.yaw, yaw).abs() < 1e-5, "back");
        w.look_delta(0.0, -1000.0);
        assert!(
            (w.camera.pitch - 30f32.to_radians()).abs() < 1e-5,
            "pitch_max"
        );
        // walking speed: 4 m/s
        let z = w.camera.pos.z;
        w.walk_key(sym::DOWN, false);
        w.command(Cmd::Recenter, now);
        w.walk_key(0x73, true);
        step(&mut w, &mut now, 0.6);
        let v = w.walking.as_ref().unwrap().walker.vel.length();
        assert!((v - 4.0).abs() < 1e-3, "{v}");
        assert!(w.camera.pos.z > z);
    }

    #[test]
    fn held_keys_by_name_press_and_release() {
        let mut w = acme();
        let mut now = walking(&mut w);
        let arg = |p: &str| BTreeMap::from([("pressed".to_string(), p.to_string())]);
        let z = w.camera.pos.z;
        w.command(Cmd::from_name("world.back", &arg("true")).unwrap(), now);
        w.command(Cmd::from_name("world.run", &arg("true")).unwrap(), now);
        step(&mut w, &mut now, 0.3);
        assert!(w.walking.as_ref().unwrap().keys.run);
        w.command(Cmd::from_name("world.back", &arg("false")).unwrap(), now);
        w.command(Cmd::from_name("world.run", &arg("false")).unwrap(), now);
        step(&mut w, &mut now, 0.5);
        assert!(w.camera.pos.z > z + 0.3, "walked back");
        let ws = w.walking.as_ref().unwrap();
        assert!(!ws.keys.any() && !ws.walker.moving(), "released: stopped");
        assert_eq!(w.mode().label(), "ANDAR", "world.back released is no exit");
    }

    #[test]
    fn pressed_round_trips_over_every_held_name() {
        let held = [
            "world.forward",
            "world.back",
            "world.strafe_left",
            "world.strafe_right",
            "world.turn_left",
            "world.turn_right",
            "world.look_up",
            "world.look_down",
            "world.run",
        ];
        let names: Vec<&str> = command_names().iter().map(|n| n.0).collect();
        for n in held {
            assert!(names.contains(&n), "{n} in command_names");
            for p in ["true", "false"] {
                let args = BTreeMap::from([("pressed".to_string(), p.to_string())]);
                let c = Cmd::from_name(n, &args).unwrap_or_else(|| panic!("{n}"));
                if n == "world.back" && p == "true" {
                    assert_eq!(c, Cmd::BackOrStep);
                    continue;
                }
                assert!(
                    matches!(c, Cmd::Hold(_, d) if d == (p == "true")),
                    "{n} {p}"
                );
                let (name, a) = c.name();
                assert_eq!(name, n);
                assert_eq!(Cmd::from_name(name, &a), Some(c), "{n} {p}");
            }
        }
        // no arg: a press
        let c = Cmd::from_name("world.forward", &BTreeMap::new()).unwrap();
        assert!(matches!(c, Cmd::Hold(_, true)));
        for n in [
            "world.grab",
            "world.nearer",
            "world.farther",
            "world.library",
        ] {
            assert!(names.contains(&n), "{n}");
            let c = Cmd::from_name(n, &BTreeMap::new()).unwrap();
            assert_eq!(c.name().0, n);
        }
    }

    #[test]
    fn the_aim_sees_screens_cats_and_doors_in_andar_only() {
        let mut w = acme();
        assert_eq!(w.aim(), None, "NAV: no aim");
        let mut now = walking(&mut w);
        assert_eq!(
            w.aim(),
            Some(Aim::Screen(ScreenId(1))),
            "the station's screen"
        );
        let (o, d) = w.aim_ray();
        assert_eq!(o, w.camera.pos);
        assert!((d.length() - 1.0).abs() < 1e-5);
        // turned to the side wall: nothing
        w.look_delta(-90.0 / 0.12, 0.0);
        assert_eq!(w.aim(), None);
        // cat 7's desk has no screen: look down at its face from its station
        w.look_delta(90.0 / 0.12, 0.0);
        w.station = Station::Desk {
            room: RoomId(0),
            desk: 2,
            slot: 0,
        };
        w.command(Cmd::Back, now);
        w.command(Cmd::Move(nkw_core::Dir::Right), now);
        w.command(Cmd::Move(nkw_core::Dir::Right), now);
        step(&mut w, &mut now, 1.0);
        walking(&mut w);
        let seat = w.cat_seat(RoomId(0), 2).unwrap().pos + Vec3::Y * CAT_FACE_UP;
        let to = seat - w.camera.pos;
        let pitch = (to.y / to.length()).asin();
        w.look_delta(0.0, -(pitch.to_degrees() / 0.12));
        assert_eq!(w.aim(), Some(Aim::Cat(CatId(7))));
        // back to the doorway: the room's own door
        w.look_delta(180.0 / 0.12, 0.0);
        w.command(Cmd::Recenter, now);
        let (door, _) = w.room_door(RoomId(0)).unwrap();
        let ws = w.walking.as_mut().unwrap();
        ws.walker.pos = door + Vec2::new(0.0, -1.0);
        ws.walker.yaw = std::f32::consts::PI;
        step(&mut w, &mut now, 0.05);
        assert_eq!(w.aim(), Some(Aim::Door(RoomId(0))));
    }

    #[test]
    fn grab_carry_and_drop_in_the_air() {
        let mut w = acme();
        // NAV: a note, nothing in hand
        run(&mut w, "world.grab", 0.0);
        assert_eq!(w.layout_note(0.0).map(|n| n.0), Some("só andando (w)"));
        assert_eq!(w.carrying(), None);
        let mut now = walking(&mut w);
        let out = run(&mut w, "world.grab", now);
        assert_eq!(w.carrying(), Some(ScreenId(1)));
        assert!(out.is_empty(), "alone on its desk: nothing to configure");
        assert_eq!(w.stack_len(0, 0), 0, "out of the stack");
        // it hangs in hand: 1.2 m ahead, facing you
        let (r, d, k) = w.locate(ScreenId(1)).unwrap();
        let p = w.screen_place(r, d, k);
        let hand = carry::hand_pose(&w.camera, carry::HAND_DEFAULT);
        assert!((p.center - hand.pos).length() < 1e-5);
        assert!(angle_diff(p.yaw, w.camera.yaw).abs() < 1e-5);
        assert_eq!(w.aim(), None, "the hand is not a target");
        // [ ] within 0.8..3.0
        for _ in 0..5 {
            run(&mut w, "world.nearer", now);
        }
        assert_eq!(w.carry.unwrap().dist, carry::HAND_MIN);
        for _ in 0..20 {
            run(&mut w, "world.farther", now);
        }
        assert_eq!(w.carry.unwrap().dist, carry::HAND_MAX);
        for _ in 0..9 {
            run(&mut w, "world.nearer", now);
        }
        assert!((w.carry.unwrap().dist - 1.2).abs() < 1e-5);
        // - in hand: its own width, configured on drop
        let out = run(&mut w, "world.narrower", now);
        assert!(out.is_empty());
        assert_eq!(
            w.rooms[0].desks[0].width,
            Width::Half,
            "the desk keeps its width"
        );
        // walk back a second, drop: in the air where it was
        w.walk_key(0x73, true);
        step(&mut w, &mut now, 1.0);
        w.walk_key(0x73, false);
        step(&mut w, &mut now, 0.5);
        let hand = carry::hand_pose(&w.camera, 1.2);
        let out = run(&mut w, "world.grab", now);
        assert_eq!(w.carrying(), None);
        let st = w.screen_state(ScreenId(1));
        let air = st.air.unwrap_or_else(|| {
            panic!(
                "in the air: {:?} {:?} {:?}",
                w.rooms,
                w.camera,
                w.layout_note(now)
            )
        });
        assert!(!st.wall);
        assert!((air.center - hand.pos).length() < 1e-5);
        assert_eq!(
            out,
            [Out::Configure {
                screen: ScreenId(1),
                size: Some(w.configure_size(Width::Third, 1)),
                fullscreen: None,
                maximized: None,
            }]
        );
        assert_eq!(w.layout_note(now).map(|n| n.0), Some("no ar"));
        let d = w.dump(&[]);
        assert_eq!(d["screens"][0]["air"], true);
        assert_eq!(d["screens"][0]["hand"], false);
        // low enough to block the walk: you cannot walk through it
        let g = w.room_grid(0);
        assert!(g.blocked(Vec2::new(air.center.x, air.center.z)));
    }

    #[test]
    fn drop_on_a_desk_joins_its_pile() {
        let mut w = acme();
        let mut now = walking(&mut w);
        run(&mut w, "world.grab", now);
        // carry it over to the cat's desk (screen 2), right in front of it
        let target = w.screen_place(0, 1, 0).center;
        let ws = w.walking.as_mut().unwrap();
        ws.walker.pos = Vec2::new(target.x, target.z + carry::HAND_DEFAULT);
        ws.walker.yaw = 0.0;
        step(&mut w, &mut now, 0.05);
        let out = run(&mut w, "world.grab", now);
        assert_eq!(w.carrying(), None);
        assert_eq!(w.rooms[0].desks.len(), 2, "its old desk went");
        assert_eq!(w.rooms[0].desks[0].screens, [ScreenId(2), ScreenId(1)]);
        assert_eq!(w.stack_len(0, 0), 2);
        let half = w.configure_size(Width::Half, 2);
        assert!(out.iter().any(|o| matches!(o, Out::Configure { screen: ScreenId(1), size: Some(s), .. } if *s == half)), "{out:?}");
        assert_eq!(w.layout_note(now).map(|n| n.0), Some("na mesa"));
    }

    #[test]
    fn drop_on_a_wall_snaps_and_persists() {
        let mut w = acme();
        let mut now = walking(&mut w);
        run(&mut w, "world.grab", now);
        let f = w.room_floor(0);
        // facing the east wall, 1.4 m from it: the hand 0.2 m off it
        let ws = w.walking.as_mut().unwrap();
        ws.walker.pos = Vec2::new(f.max.x - 1.4, f.min.y + 2.6);
        ws.walker.yaw = std::f32::consts::FRAC_PI_2;
        step(&mut w, &mut now, 0.05);
        run(&mut w, "world.grab", now);
        let st = w.screen_state(ScreenId(1));
        assert!(st.wall);
        let p = st.air.unwrap();
        assert!(
            (p.center.x - (f.max.x - carry::WALL_GAP)).abs() < 1e-4,
            "{p:?}"
        );
        assert!(angle_diff(p.yaw, std::f32::consts::FRAC_PI_2).abs() < 1e-5);
        assert_eq!(w.layout_note(now).map(|n| n.0), Some("na parede"));
        let snap = w.snapshot();
        let rec = snap.screens.iter().find(|s| s.app_id == "acme.1").unwrap();
        let air = rec.place.air.expect("persisted");
        assert!(air.wall);
        let text = nkw_core::persist::to_string(&snap);
        assert_eq!(nkw_core::persist::parse(&text).unwrap(), snap);
    }

    #[test]
    fn leaving_andar_drops_the_hand_in_the_air_but_an_overlay_does_not() {
        let mut w = acme();
        let now = walking(&mut w);
        run(&mut w, "world.grab", now);
        w.command(Cmd::Hint { focus: false }, now);
        assert_eq!(w.carrying(), Some(ScreenId(1)), "SALTO over ANDAR keeps it");
        w.command(Cmd::Back, now);
        assert!(w.mode().walking());
        let hand = carry::hand_pose(&w.camera, 1.2);
        let out = w.command(Cmd::Back, now);
        assert_eq!(w.mode().label(), "NAVEGAR");
        assert_eq!(w.carrying(), None);
        let air = w.screen_state(ScreenId(1)).air.unwrap();
        assert!((air.center - hand.pos).length() < 1e-5);
        assert!(
            out.contains(&Out::Effect(nkw_core::Effect::WalkStop)),
            "{out:?}"
        );
    }

    #[test]
    fn walk_start_and_stop_pass_through_for_the_hub() {
        let mut w = acme();
        let out = w.command(Cmd::Walk, 0.0);
        assert!(
            out.contains(&Out::Effect(nkw_core::Effect::WalkStart)),
            "{out:?}"
        );
        let out = w.command(Cmd::Back, 0.0);
        assert!(
            out.contains(&Out::Effect(nkw_core::Effect::WalkStop)),
            "{out:?}"
        );
        assert!(w.walking.is_none());
    }

    #[test]
    fn the_comfort_vignette_ramps() {
        let mut w = acme();
        let mut now = walking(&mut w);
        w.walk_key(sym::W, true);
        step(&mut w, &mut now, 0.06);
        assert_eq!(w.comfort_vignette(now), 0.0, "off by default");
        w.set_walk_config(WalkConfig {
            comfort_vignette: true,
            ..WalkConfig::default()
        });
        w.walk_key(sym::W, false);
        step(&mut w, &mut now, 0.5);
        assert_eq!(w.comfort_vignette(now), 0.0, "still");
        w.walk_key(sym::W, true);
        step(&mut w, &mut now, 0.05);
        let mid = w.comfort_vignette(now);
        assert!(mid > 0.2 && mid < 0.8, "{mid}");
        step(&mut w, &mut now, 0.1);
        assert_eq!(w.comfort_vignette(now), 1.0);
        w.walk_key(sym::W, false);
        step(&mut w, &mut now, 0.05);
        let down = w.comfort_vignette(now);
        assert!(down > 0.2 && down < 0.8, "{down}");
        step(&mut w, &mut now, 0.1);
        assert_eq!(w.comfort_vignette(now), 0.0);
        // turning with the mouse counts
        w.look_delta(10.0, 0.0);
        step(&mut w, &mut now, 1.0 / 60.0);
        assert!(w.comfort_vignette(now) > 0.0);
    }

    #[test]
    fn the_vignette_keeps_frames_coming_until_it_has_faded() {
        let continuous = |w: &World| w.wishes().iter().any(|x| x.want == Want::Continuous);
        let mut w = acme();
        w.set_walk_config(WalkConfig {
            comfort_vignette: true,
            ..WalkConfig::default()
        });
        let mut now = walking(&mut w);
        w.frame_done(now);
        assert!(!continuous(&w), "still: no frames");
        // a mouse turn alone: frames while it ramps and fades
        w.look_delta(20.0, 0.0);
        let mut frames = 0;
        while continuous(&w) && frames < 120 {
            now += 1.0 / 60.0;
            w.tick(now);
            w.frame_done(now);
            frames += 1;
        }
        assert!(frames > 10 && frames < 60, "{frames} frames");
        assert_eq!(w.comfort_vignette(now), 0.0, "faded out");
        // leaving ANDAR while it shows: it fades out too
        w.walk_key(sym::W, true);
        step(&mut w, &mut now, 0.3);
        w.walk_key(sym::W, false);
        w.command(Cmd::Back, now);
        w.frame_done(now);
        assert!(continuous(&w), "fading");
        for _ in 0..20 {
            now += 1.0 / 60.0;
            w.tick(now);
            w.frame_done(now);
        }
        assert!(!continuous(&w), "{:?}", w.wishes());
        assert_eq!(w.comfort_vignette(now), 0.0);
    }

    #[test]
    fn exit_rules_from_the_config() {
        for (rule, free) in [
            (nkw_core::ExitRule::Stay, true),
            (nkw_core::ExitRule::Snap, false),
        ] {
            let mut w = acme();
            w.set_walk_config(WalkConfig {
                on_exit: rule,
                ..WalkConfig::default()
            });
            let mut now = walking(&mut w);
            // turned away: auto would stay free; snap still takes a station
            w.look_delta(120.0 / 0.12, 0.0);
            step(&mut w, &mut now, 0.05);
            w.command(Cmd::Back, now);
            assert_eq!(matches!(w.station, Station::Free(_)), free, "{rule:?}");
        }
    }

    #[test]
    fn ensure_room_is_idempotent_and_the_library_follows_the_reception() {
        let mut w = World::new(OUT, OPTS);
        let ctx = vec![("acme".to_string(), vec![CatId(2)])];
        w.sync_contexts(&ctx, 0.0);
        let names = |w: &World| w.rooms.iter().map(|r| r.name.clone()).collect::<Vec<_>>();
        assert_eq!(names(&w), [RECEPTION, "acme", MY_ROOM]);
        let (lib, out) = w.ensure_room(RoomKind::Library, LIBRARY, 0.0);
        assert!(out.is_empty());
        assert_eq!(names(&w), [RECEPTION, LIBRARY, "acme", MY_ROOM]);
        assert_eq!(w.room_kind(lib), RoomKind::Library);
        let (again, _) = w.ensure_room(RoomKind::Library, "outra", 0.0);
        assert_eq!(again, lib);
        assert_eq!(w.rooms.len(), 4);
        // the contexts change: the library stays right after the reception
        let ctx = vec![
            ("bravo".to_string(), vec![CatId(3)]),
            ("acme".to_string(), vec![CatId(2)]),
        ];
        w.sync_contexts(&ctx, 0.0);
        assert_eq!(names(&w), [RECEPTION, LIBRARY, "bravo", "acme", MY_ROOM]);
        w.sync_contexts(&[], 0.0);
        assert_eq!(names(&w), [RECEPTION, LIBRARY, MY_ROOM]);
        assert_eq!(w.snapshot().rooms[1].kind, RoomKind::Library, "persisted");
        // the cat's station followed its desk across the insert
        let mut w = World::new(OUT, OPTS);
        w.sync_contexts(&[("acme".to_string(), vec![CatId(2)])], 0.0);
        w.command(Cmd::RoomNext, 0.0);
        w.command(Cmd::RoomNext, 0.0);
        let before = w.station_cat();
        w.ensure_room(RoomKind::Library, LIBRARY, 0.0);
        assert_eq!(w.station_cat(), before);
    }

    #[test]
    fn world_library_makes_it_and_goes_there() {
        let mut w = World::new(OUT, OPTS);
        w.sync_contexts(&[], 0.0);
        run(&mut w, "world.library", 0.0);
        assert_eq!(w.rooms[1].name, LIBRARY);
        assert_eq!(
            w.station,
            Station::Desk {
                room: w.rooms[1].id,
                desk: 0,
                slot: 0
            },
            "at the counter"
        );
        assert_eq!(w.dump(&[])["rooms"][1]["kind"], "library");
        run(&mut w, "world.library", 0.0);
        assert_eq!(w.rooms.len(), 3, "once");
        assert_eq!(w.rooms[1].desks.len(), 1, "one counter");
    }

    #[test]
    fn the_library_always_has_the_librarians_counter_first() {
        let counter = |w: &World| {
            let i = w.rooms.iter().position(|r| r.name == LIBRARY).unwrap();
            let d = &w.rooms[i].desks;
            assert_eq!(d[0].cat, Some(LIBRARIAN), "desk 0 is hers");
            assert!(d[0].screens.is_empty());
            assert_eq!(d.iter().filter(|d| d.cat == Some(LIBRARIAN)).count(), 1);
            (w.rooms[i].id, d.len())
        };
        let mut w = World::new(OUT, OPTS);
        w.sync_contexts(&[("acme".to_string(), vec![CatId(2)])], 0.0);
        let (lib, n) = {
            w.ensure_room(RoomKind::Library, LIBRARY, 0.0);
            counter(&w)
        };
        assert_eq!(n, 1);
        assert_eq!(w.cat_desk(LIBRARIAN), Some((lib, 0)));
        assert!(w.cat_seat(lib, 0).is_some(), "a seat for her");
        // a screen dropped there sits on a desk after the counter; syncs keep both
        w.station = Station::Desk {
            room: lib,
            desk: 0,
            slot: 0,
        };
        let (s, _) = w.add_screen("demo.term", "", 0.0);
        assert_eq!(w.locate(s).map(|l| l.1), Some(1));
        for ctx in [
            vec![("bravo".to_string(), vec![CatId(3)])],
            vec![],
            vec![("acme".to_string(), vec![CatId(2)])],
        ] {
            w.sync_contexts(&ctx, 0.0);
            assert_eq!(counter(&w), (lib, 2));
            assert_eq!(w.rooms[1].id, lib);
        }
        // a library whose counter went missing (an old file, a stray edit) gets it back first
        let i = w.room_index(lib).unwrap();
        w.rooms[i].desks.remove(0);
        w.sync_contexts(&[], 0.0);
        assert_eq!(counter(&w), (lib, 2));
        assert_eq!(
            w.locate(s).map(|l| l.1),
            Some(1),
            "the screen's desk after it"
        );
        // the counter never goes away with its last screen
        w.remove_screen(s, 0.0);
        assert_eq!(counter(&w), (lib, 1));
    }

    #[test]
    fn talk_from_a_free_station_reaches_the_cat_in_view() {
        let mut w = acme();
        let seat = w.cat_seat(RoomId(0), 2).unwrap();
        // 2 m in front of cat 7's chair, looking at it
        let facing = seat.yaw + std::f32::consts::PI;
        let back = Pose::new(seat.pos, facing, 0.0).heading() * -2.0;
        let at = |yaw: f32| {
            Pose::new(
                Vec3::new(seat.pos.x + back.x, 1.5, seat.pos.z + back.y),
                yaw,
                0.0,
            )
        };
        w.glide_free(at(facing), 0.0);
        w.tick(10.0);
        let out = w.command(Cmd::Talk, 10.0);
        assert!(
            out.contains(&Out::Effect(nkw_core::Effect::FrameFace(CatId(7)))),
            "{out:?}"
        );
        w.command(Cmd::Back, 10.0);
        // turned away: nobody
        w.glide_free(at(facing + std::f32::consts::PI), 10.0);
        w.tick(20.0);
        assert!(w.command(Cmd::Talk, 20.0).is_empty());
    }

    #[test]
    fn a_restored_library_comes_back_with_its_counter() {
        let mut w = World::new(OUT, OPTS);
        w.sync_contexts(&[], 0.0);
        w.ensure_room(RoomKind::Library, LIBRARY, 0.0);
        let file = w.snapshot();
        let plan = nkw_core::persist::apply(&file, &[]);
        assert_eq!(plan.rooms, [(LIBRARY.to_string(), RoomKind::Library)]);
        // what the WM asks on restore: `Cmd::Library { go: false }`
        let mut fresh = World::new(OUT, OPTS);
        fresh.sync_contexts(&[], 0.0);
        let before = fresh.station;
        fresh.command(Cmd::Library { go: false }, 0.0);
        assert_eq!(fresh.station, before, "a restore does not go there");
        assert_eq!(fresh.rooms[1].name, LIBRARY);
        assert_eq!(fresh.room_kind(fresh.rooms[1].id), RoomKind::Library);
        assert_eq!(fresh.rooms[1].desks[0].cat, Some(LIBRARIAN));
    }

    #[test]
    fn walking_ticks_do_not_allocate() {
        let mut w = acme();
        let mut now = walking(&mut w);
        w.set_walk_config(WalkConfig {
            comfort_vignette: true,
            ..WalkConfig::default()
        });
        w.walk_key(sym::W, true);
        w.walk_key(0x65, true);
        step(&mut w, &mut now, 0.2);
        w.frame_done(now);
        let cap = w.pending.capacity();
        let len = w.pending.len();
        for _ in 0..240 {
            now += 1.0 / 120.0;
            w.tick(now);
            w.look_delta(3.0, 1.0);
            w.frame_done(now);
        }
        assert!(
            w.pending.capacity() <= cap.max(4),
            "{} > {cap}",
            w.pending.capacity()
        );
        assert!(w.pending.len() <= len.max(1));
    }
}
