//! The world as `nkw` sees it: `nkw-core`'s mode machine, strip, 1:1 pose,
//! tweens and scheduler glued to rooms, desks and screens. Pure (no
//! Wayland, no GL): the compositor maps `ScreenId`s to its windows and
//! turns the returned [`Out`]s into keyboard focus changes.

use std::collections::BTreeMap;

use glam::{Vec2, Vec3};
use nkw_core::collide::{Aabb, Door as Gap, Grid, WalkInput, WalkParams, Walker, Wall};
use nkw_core::strip::{self, Desk, Dir, RoomSlot, Side, Spot, Station, Width};
use nkw_core::walk_exit::{self, ExitRule};
use nkw_core::{
    Action, Effect, Machine, Mode, OneToOne, Output, Pose, RoomId, ScreenId, ScreenPlace, Target,
    Time, Tween, TweenOpts, Want, Wish, onetoone,
};
use serde::Serialize;

/// Vertical field of view, radians (60°, fixed: §3.5).
pub const FOV_Y: f32 = std::f32::consts::FRAC_PI_3;
/// Eye height, metres.
pub const EYE: f32 = 1.5;
/// Width of the hall, metres (rooms open on both sides of it).
pub const HALL_W: f32 = 3.0;
/// Depth of a room, metres.
pub const ROOM_DEPTH: f32 = 5.0;
/// Screens hang this far in front of the back wall, metres.
pub const SCREEN_FROM_WALL: f32 = 0.6;
/// The NAV camera stands this far from a desk's screens, metres (§4.2).
pub const NAV_DIST: f32 = 2.4;
/// Free floor at both ends of a room's strip, metres.
pub const ROOM_END: f32 = 1.0;
/// Shortest room, metres.
pub const ROOM_MIN: f32 = 6.0;

/// A room and its strip of desks.
#[derive(Clone, Debug, PartialEq)]
pub struct Room {
    pub id: RoomId,
    pub name: String,
    pub desks: Vec<Desk>,
}

/// What the world knows of a window.
#[derive(Clone, Debug, PartialEq)]
pub struct Screen {
    pub id: ScreenId,
    pub app_id: String,
    pub title: String,
    /// logical px, as last committed (0 until the first buffer)
    pub size: Vec2,
}

/// World commands, already resolved from keys (see [`filter`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cmd {
    /// h / l / j / k in NAV
    Move(Dir),
    /// J / K
    RoomNext,
    RoomPrev,
    /// enter / i
    Focus,
    /// esc in NAV/ANDAR, ctrl+] in FOCO
    Back,
    /// w / world+w
    Walk,
    /// world+h/l/j/k in FOCO
    Neighbour(Dir),
}

/// What the compositor must do after a command or a tick.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Out {
    KeyboardEnter(ScreenId),
    KeyboardLeave,
    PopupsDone,
}

/// The modifiers the key filter looks at.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct KeyMods {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
}

/// What the key filter decided.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// to the focused client (FOCO only)
    Forward,
    /// the world's; maybe a command
    Intercept(Option<Cmd>),
}

/// keysyms the filter needs (xkb values; avoids depending on the keymap here)
pub mod sym {
    pub const H: u32 = 0x68;
    pub const J: u32 = 0x6a;
    pub const K: u32 = 0x6b;
    pub const L: u32 = 0x6c;
    pub const I: u32 = 0x69;
    pub const W: u32 = 0x77;
    pub const UPPER_J: u32 = 0x4a;
    pub const UPPER_K: u32 = 0x4b;
    pub const BRACKETRIGHT: u32 = 0x5d;
    pub const RETURN: u32 = 0xff0d;
    pub const KP_ENTER: u32 = 0xff8d;
    pub const ESCAPE: u32 = 0xff1b;
    pub const LEFT: u32 = 0xff51;
    pub const UP: u32 = 0xff52;
    pub const RIGHT: u32 = 0xff53;
    pub const DOWN: u32 = 0xff54;
}

fn dir_of(s: u32) -> Option<Dir> {
    match s {
        sym::H | sym::LEFT => Some(Dir::Left),
        sym::L | sym::RIGHT => Some(Dir::Right),
        sym::K | sym::UP => Some(Dir::Up),
        sym::J | sym::DOWN => Some(Dir::Down),
        _ => None,
    }
}

/// The key filter (§3.6): in FOCO only `ctrl+]` and the `world+` chords
/// (`ctrl+alt` nested) are the world's, everything else goes to the client;
/// in every other mode all keys are the world's. `s`: the modified keysym.
pub fn filter(mode: &Mode, m: KeyMods, s: u32) -> Verdict {
    let world = m.ctrl && m.alt;
    if mode.focus().is_some() {
        if world {
            let lower = if (0x41..=0x5a).contains(&s) {
                s | 0x20
            } else {
                s
            };
            return Verdict::Intercept(match lower {
                sym::W => Some(Cmd::Walk),
                _ => dir_of(lower).map(Cmd::Neighbour),
            });
        }
        if m.ctrl && s == sym::BRACKETRIGHT {
            return Verdict::Intercept(Some(Cmd::Back));
        }
        return Verdict::Forward;
    }
    if m.ctrl || m.alt {
        return Verdict::Intercept(if world && s == sym::W {
            Some(Cmd::Walk)
        } else {
            None
        });
    }
    let walking = mode.walking();
    Verdict::Intercept(match s {
        sym::ESCAPE => Some(Cmd::Back),
        _ if walking => None,
        sym::W => Some(Cmd::Walk),
        sym::UPPER_J => Some(Cmd::RoomNext),
        sym::UPPER_K => Some(Cmd::RoomPrev),
        sym::RETURN | sym::KP_ENTER | sym::I => Some(Cmd::Focus),
        s => dir_of(s).map(Cmd::Move),
    })
}

/// The keys held in ANDAR (§3.5).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WalkKeys {
    pub forward: bool,
    pub back: bool,
    pub left: bool,
    pub right: bool,
    pub turn_left: bool,
    pub turn_right: bool,
    pub look_up: bool,
    pub look_down: bool,
    pub run: bool,
}

impl WalkKeys {
    fn any(&self) -> bool {
        *self != WalkKeys::default()
    }

    fn input(&self) -> WalkInput {
        let axis = |p: bool, n: bool| f32::from(u8::from(p)) - f32::from(u8::from(n));
        WalkInput {
            forward: axis(self.forward, self.back),
            strafe: axis(self.right, self.left),
            run: self.run,
            turn: axis(self.turn_right, self.turn_left),
            look: axis(self.look_up, self.look_down),
            ..WalkInput::default()
        }
    }
}

/// The ANDAR state: the walker, its room's grid and its clock.
#[derive(Clone, Debug)]
pub struct Walking {
    pub walker: Walker,
    pub keys: WalkKeys,
    grid: Grid,
    last: Time,
}

/// Options of the world.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Opts {
    pub reduced_motion: bool,
}

/// The world.
#[derive(Clone, Debug)]
pub struct World {
    pub machine: Machine,
    pub rooms: Vec<Room>,
    pub screens: BTreeMap<ScreenId, Screen>,
    pub station: Station,
    pub camera: Pose,
    pub out: Output,
    pub opts: Opts,
    tween: Option<Tween>,
    /// the FOCO target to swap in when the tween ends
    arrive: Option<ScreenId>,
    /// the window composed 2D, 1:1 (FOCO, arrived)
    focus_2d: Option<ScreenId>,
    /// FOCO: the world pass does not run
    pub frozen: bool,
    /// wants that end after the next frame (`Once`, `At`)
    pending: Vec<Wish>,
    /// the FOCO arrival frame: one world pass even though frozen
    arrival_pass: bool,
    /// ANDAR
    pub walking: Option<Walking>,
    pub walk_params: WalkParams,
    next_id: u32,
}

/// Where a room sits in the world: the origin of its local frame (left end
/// of its back wall, on the floor) and the yaw a camera facing its back
/// wall has. Local x runs to that camera's right, local z towards it.
fn room_frame(slot: &RoomSlot) -> (Vec3, f32) {
    let back = HALL_W / 2.0 + ROOM_DEPTH;
    match slot.side {
        Side::North => (Vec3::new(slot.x0, 0.0, -back), 0.0),
        Side::South => (
            Vec3::new(slot.x0 + slot.len, 0.0, back),
            std::f32::consts::PI,
        ),
    }
}

/// The horizontal direction a camera with this yaw looks along.
pub fn facing(yaw: f32) -> Vec3 {
    Pose::new(Vec3::ZERO, yaw, 0.0).forward()
}

/// A point of a room's local frame in the world.
fn to_world(slot: &RoomSlot, local: Vec3) -> Vec3 {
    let (o, yaw) = room_frame(slot);
    let p = Pose::new(Vec3::ZERO, yaw, 0.0);
    o + p.right() * local.x + Vec3::Y * local.y - p.forward() * local.z
}

impl World {
    /// An empty world: one room ("minha sala"), no desks.
    pub fn new(out: Output, opts: Opts) -> World {
        let rooms = vec![Room {
            id: RoomId(0),
            name: "minha sala".into(),
            desks: Vec::new(),
        }];
        World::with_rooms(out, opts, rooms)
    }

    pub fn with_rooms(out: Output, opts: Opts, rooms: Vec<Room>) -> World {
        let mut w = World {
            machine: Machine::new(),
            rooms,
            screens: BTreeMap::new(),
            station: Station::Free(Pose::default()),
            camera: Pose::default(),
            out,
            opts,
            tween: None,
            arrive: None,
            focus_2d: None,
            frozen: false,
            pending: Vec::new(),
            arrival_pass: false,
            walking: None,
            walk_params: WalkParams::default(),
            next_id: 1,
        };
        w.station = match w.rooms.first().map(|r| r.desks.is_empty()) {
            Some(false) => Station::Desk {
                room: w.rooms[0].id,
                desk: 0,
                slot: 0,
            },
            _ => Station::Free(w.room_entry_pose(0)),
        };
        w.camera = w.station_pose(&w.station).unwrap_or_default();
        w.pending.push(Wish::world(Want::Once));
        w
    }

    /// The usable output size, logical px.
    pub fn usable(&self) -> Vec2 {
        Vec2::new(self.out.width_px as f32, self.out.height_px as f32) / self.out.scale
    }

    /// The size a new window of a desk of `width` is configured to.
    pub fn configure_size(&self, width: Width, stack: usize) -> Vec2 {
        strip::screen_size_px(width, self.usable(), stack).floor()
    }

    pub fn mode(&self) -> &Mode {
        self.machine.mode()
    }

    fn room_index(&self, id: RoomId) -> Option<usize> {
        self.rooms.iter().position(|r| r.id == id)
    }

    /// The room the camera is in (the station's).
    pub fn current_room(&self) -> usize {
        match self.station {
            Station::Desk { room, .. } | Station::Door(room) => self.room_index(room).unwrap_or(0),
            _ => self.room_at(self.camera.pos).unwrap_or(0),
        }
    }

    fn room_at(&self, p: Vec3) -> Option<usize> {
        let slots = self.slots();
        slots.iter().position(|s| {
            let inside_x = p.x >= s.x0 && p.x <= s.x0 + s.len;
            let inside_z = match s.side {
                Side::North => p.z <= -HALL_W / 2.0,
                Side::South => p.z >= HALL_W / 2.0,
            };
            inside_x && inside_z
        })
    }

    fn desk_widths_m(&self, room: usize) -> Vec<f32> {
        let usable = self.usable();
        self.rooms[room]
            .desks
            .iter()
            .map(|d| strip::desk_width_m(d.width, usable))
            .collect()
    }

    /// The rooms along the hall.
    pub fn slots(&self) -> Vec<RoomSlot> {
        let lens: Vec<f32> = (0..self.rooms.len())
            .map(|r| {
                let w = self.desk_widths_m(r);
                let n = w.len() as f32;
                let strip = w.iter().sum::<f32>() + (n - 1.0).max(0.0) * strip::DESK_GAP;
                (strip + 2.0 * ROOM_END).max(ROOM_MIN)
            })
            .collect();
        strip::rooms_along_hall(&lens)
    }

    /// Where a room is entered (no desks yet): its middle, facing the back.
    fn room_entry_pose(&self, room: usize) -> Pose {
        let slot = self.slots()[room];
        let (_, yaw) = room_frame(&slot);
        let local = Vec3::new(slot.len / 2.0, EYE, SCREEN_FROM_WALL + NAV_DIST);
        Pose::new(to_world(&slot, local), yaw, 0.0)
    }

    /// Centre (local x, metres) of each desk of a room.
    fn desk_local_xs(&self, room: usize) -> Vec<f32> {
        let w = self.desk_widths_m(room);
        let xs = strip::desk_xs(&w, strip::DESK_GAP);
        let len = self.slots()[room].len;
        let strip_len = w.iter().sum::<f32>() + (w.len() as f32 - 1.0).max(0.0) * strip::DESK_GAP;
        let left = (len - strip_len) / 2.0;
        xs.into_iter().map(|x| x + left).collect()
    }

    /// Where screen `slot` of a desk hangs (stacked screens split the
    /// height around the eye).
    pub fn screen_place(&self, room: usize, desk: usize, slot: usize) -> ScreenPlace {
        let slots = self.slots();
        let s = &slots[room];
        let x = self.desk_local_xs(room)[desk];
        let d = &self.rooms[room].desks[desk];
        let n = d.screens.len().clamp(1, strip::STACK_VISIBLE);
        let h = self.screen_logical(room, desk, slot).y * onetoone::M_PER_PX;
        let y = EYE + h * ((n as f32 - 1.0) / 2.0 - slot.min(n - 1) as f32);
        let (_, yaw) = room_frame(s);
        ScreenPlace {
            center: to_world(s, Vec3::new(x, y, SCREEN_FROM_WALL)),
            yaw,
            pitch: 0.0,
        }
    }

    /// The logical size a desk's screen shows at: its committed size, or
    /// the configured one before the first buffer.
    pub fn screen_logical(&self, room: usize, desk: usize, slot: usize) -> Vec2 {
        let d = &self.rooms[room].desks[desk];
        d.screens
            .get(slot)
            .and_then(|id| self.screens.get(id))
            .map(|s| s.size)
            .filter(|s| s.x > 0.0 && s.y > 0.0)
            .unwrap_or_else(|| self.configure_size(d.width, d.screens.len()))
    }

    /// Where a screen is: (room, desk, slot).
    pub fn locate(&self, id: ScreenId) -> Option<(usize, usize, usize)> {
        self.rooms.iter().enumerate().find_map(|(r, room)| {
            room.desks.iter().enumerate().find_map(|(d, desk)| {
                desk.screens
                    .iter()
                    .position(|s| *s == id)
                    .map(|k| (r, d, k))
            })
        })
    }

    /// The NAV camera pose of a station.
    pub fn station_pose(&self, st: &Station) -> Option<Pose> {
        match *st {
            Station::Desk { room, desk, .. } => {
                let r = self.room_index(room)?;
                self.rooms[r].desks.get(desk)?;
                let slots = self.slots();
                let s = &slots[r];
                let x = self.desk_local_xs(r)[desk];
                let (_, yaw) = room_frame(s);
                let local = Vec3::new(x, EYE, SCREEN_FROM_WALL + NAV_DIST);
                Some(Pose::new(to_world(s, local), yaw, 0.0))
            }
            Station::Free(p) => Some(p),
            Station::Door(room) => Some(self.room_entry_pose(self.room_index(room)?)),
            _ => None,
        }
    }

    /// The current room's stations, in `h`/`l` order.
    pub fn spots(&self, room: usize) -> Vec<Spot> {
        let id = self.rooms[room].id;
        (0..self.rooms[room].desks.len())
            .filter_map(|d| {
                let station = Station::Desk {
                    room: id,
                    desk: d,
                    slot: 0,
                };
                Some(Spot {
                    station,
                    pose: self.station_pose(&station)?,
                    stack: self.rooms[room].desks[d].screens.len().max(1),
                })
            })
            .collect()
    }

    /// The screen the station has selected.
    pub fn selected(&self) -> Option<ScreenId> {
        match self.station {
            Station::Desk { room, desk, slot } => {
                let r = self.room_index(room)?;
                self.rooms[r].desks.get(desk)?.screens.get(slot).copied()
            }
            Station::Air(s) => Some(s),
            _ => None,
        }
    }

    /// The 1:1 pose of a screen.
    pub fn one_to_one(&self, id: ScreenId) -> Option<OneToOne> {
        let (r, d, k) = self.locate(id)?;
        let place = self.screen_place(r, d, k);
        Some(onetoone::pose(
            &self.out,
            FOV_Y,
            self.screen_logical(r, d, k),
            &place,
        ))
    }

    /// FOCO, arrived: the window composed in 2D and its rectangle.
    pub fn focus_2d(&self) -> Option<(ScreenId, OneToOne)> {
        let id = self.focus_2d?;
        Some((id, self.one_to_one(id)?))
    }

    /// The FOCO target (also while gliding to it).
    pub fn focus_target(&self) -> Option<ScreenId> {
        match self.mode().focus() {
            Some(Target::Screen(s)) => Some(s),
            _ => None,
        }
    }

    pub fn gliding(&self) -> bool {
        self.tween.is_some()
    }

    fn glide(&mut self, to: Pose, now: Time, lateral: bool) {
        let opts = TweenOpts {
            lateral,
            reduced_motion: self.opts.reduced_motion,
            ..TweenOpts::default()
        };
        let t = Tween::plan(self.camera, to, now, opts);
        if t.dur == 0.0 {
            self.camera = to;
            self.tween = None;
            self.pending.push(Wish::world(Want::Once));
        } else {
            self.tween = Some(t);
        }
    }

    fn go_to(&mut self, st: Station, now: Time, lateral: bool) {
        self.station = st;
        if let Some(p) = self.station_pose(&st) {
            self.glide(p, now, lateral);
        }
    }

    /// A new window: a new desk right of the current station (§4.4), the
    /// station moves there (unless in FOCO). Returns its id and the size
    /// to configure it to.
    pub fn add_screen(&mut self, app_id: &str, title: &str, now: Time) -> (ScreenId, Vec2) {
        let id = ScreenId(self.next_id);
        self.next_id += 1;
        let room = self.current_room();
        let at = match self.station {
            Station::Desk { desk, .. } => (desk + 1).min(self.rooms[room].desks.len()),
            _ => self.rooms[room].desks.len(),
        };
        let desk = Desk {
            width: Width::Half,
            screens: vec![id],
            cat: None,
        };
        let size = self.configure_size(desk.width, 1);
        self.rooms[room].desks.insert(at, desk);
        self.screens.insert(
            id,
            Screen {
                id,
                app_id: app_id.into(),
                title: title.into(),
                size: Vec2::ZERO,
            },
        );
        let rid = self.rooms[room].id;
        if self.mode().focus().is_none() {
            self.go_to(
                Station::Desk {
                    room: rid,
                    desk: at,
                    slot: 0,
                },
                now,
                false,
            );
        } else if let Station::Desk { desk, .. } = &mut self.station
            && *desk >= at
        {
            *desk += 1;
        }
        self.pending.push(Wish::world(Want::Once));
        (id, size)
    }

    pub fn set_app_id(&mut self, id: ScreenId, app_id: &str, title: &str) {
        if let Some(s) = self.screens.get_mut(&id) {
            s.app_id = app_id.into();
            s.title = title.into();
        }
    }

    /// A commit changed the screen's size (logical px).
    pub fn set_size(&mut self, id: ScreenId, size: Vec2) {
        if let Some(s) = self.screens.get_mut(&id)
            && s.size != size
        {
            s.size = size;
            self.pending.push(Wish::world(Want::Once));
        }
    }

    /// A window went away.
    pub fn remove_screen(&mut self, id: ScreenId, now: Time) -> Vec<Out> {
        let mut out = Vec::new();
        if self.focus_target() == Some(id) {
            let fx = self.machine.apply(Action::WindowClosed, self.camera);
            out = self.effects(&fx, now);
        }
        self.focus_2d = self.focus_2d.filter(|s| *s != id);
        self.arrive = self.arrive.filter(|s| *s != id);
        let Some((r, d, k)) = self.locate(id) else {
            return out;
        };
        self.screens.remove(&id);
        let rid = self.rooms[r].id;
        let desk = &mut self.rooms[r].desks[d];
        desk.screens.remove(k);
        let gone = desk.screens.is_empty() && desk.cat.is_none();
        if gone {
            self.rooms[r].desks.remove(d);
        }
        if let Station::Desk { room, desk, slot } = self.station
            && room == rid
        {
            let n = self.rooms[r].desks.len();
            let st = if n == 0 {
                Station::Free(self.room_entry_pose(r))
            } else {
                let desk = if gone && desk >= d && desk > 0 {
                    desk - 1
                } else {
                    desk
                }
                .min(n - 1);
                let stack = self.rooms[r].desks[desk].screens.len().max(1);
                Station::Desk {
                    room,
                    desk,
                    slot: slot.min(stack - 1),
                }
            };
            if self.mode().focus().is_none() {
                self.go_to(st, now, false);
            } else {
                self.station = st;
            }
        }
        self.pending.push(Wish::world(Want::Once));
        out
    }

    /// Run a world command at `now`.
    pub fn command(&mut self, cmd: Cmd, now: Time) -> Vec<Out> {
        let cam = self.camera;
        match cmd {
            Cmd::Move(dir) => {
                let room = self.current_room();
                if let Some(st) = strip::neighbour(&self.spots(room), &self.station, dir) {
                    let lateral = matches!(dir, Dir::Left | Dir::Right);
                    self.go_to(st, now, lateral);
                    self.pending.push(Wish::world(Want::Once));
                }
                Vec::new()
            }
            Cmd::RoomNext | Cmd::RoomPrev => {
                let n = self.rooms.len() as isize;
                let step = if cmd == Cmd::RoomNext { 1 } else { -1 };
                let r = self.current_room() as isize + step;
                if (0..n).contains(&r) {
                    let r = r as usize;
                    let st = if self.rooms[r].desks.is_empty() {
                        Station::Door(self.rooms[r].id)
                    } else {
                        Station::Desk {
                            room: self.rooms[r].id,
                            desk: 0,
                            slot: 0,
                        }
                    };
                    self.go_to(st, now, false);
                }
                Vec::new()
            }
            Cmd::Focus => match self.selected() {
                Some(s) => {
                    let fx = self.machine.apply(Action::Focus(Target::Screen(s)), cam);
                    self.effects(&fx, now)
                }
                None => Vec::new(),
            },
            Cmd::Back => {
                let fx = self.machine.apply(Action::Back, cam);
                self.effects(&fx, now)
            }
            Cmd::Walk => {
                let fx = self.machine.apply(Action::Walk, cam);
                self.effects(&fx, now)
            }
            Cmd::Neighbour(dir) => {
                let Some(cur) = self.focus_target() else {
                    return Vec::new();
                };
                let Some((r, d, k)) = self.locate(cur) else {
                    return Vec::new();
                };
                let here = Station::Desk {
                    room: self.rooms[r].id,
                    desk: d,
                    slot: k,
                };
                let Some(st) = strip::neighbour(&self.spots(r), &here, dir) else {
                    return Vec::new();
                };
                self.station = st;
                match self.selected() {
                    Some(s) => {
                        let fx = self
                            .machine
                            .apply(Action::Neighbour(Target::Screen(s)), cam);
                        self.effects(&fx, now)
                    }
                    None => Vec::new(),
                }
            }
        }
    }

    /// Turn the machine's effects into camera moves and compositor outs.
    fn effects(&mut self, fx: &[Effect], now: Time) -> Vec<Out> {
        let mut out = Vec::new();
        for e in fx {
            match *e {
                Effect::GlideOneToOne(Target::Screen(s))
                | Effect::TravelOneToOne(Target::Screen(s)) => {
                    let lateral = matches!(e, Effect::TravelOneToOne(_));
                    self.focus_2d = None;
                    if let Some(o) = self.one_to_one(s) {
                        self.arrive = Some(s);
                        self.glide(o.pose, now, lateral);
                        if self.tween.is_none() {
                            self.focus_2d = self.arrive.take();
                        }
                    }
                }
                Effect::GlideTo(_) => {
                    // back to the station (the saved pose may be stale if
                    // the strip changed meanwhile)
                    self.focus_2d = None;
                    self.arrive = None;
                    let to = self.station_pose(&self.station).unwrap_or(self.camera);
                    self.glide(to, now, false);
                }
                Effect::KeyboardEnter(Target::Screen(s)) => out.push(Out::KeyboardEnter(s)),
                Effect::KeyboardLeave => out.push(Out::KeyboardLeave),
                Effect::PopupsDone => out.push(Out::PopupsDone),
                Effect::Freeze => self.frozen = true,
                Effect::Unfreeze => self.frozen = false,
                Effect::SelectNextOnDesk => {
                    self.focus_2d = None;
                    self.arrive = None;
                }
                Effect::WalkStart => self.walk_start(now),
                Effect::WalkStop => self.walking = None,
                Effect::WalkExit => self.walk_exit(now),
                _ => {}
            }
        }
        self.pending.push(Wish::compose(Want::Once));
        out
    }

    /// The occupancy grid of a room: its floor, the door to the hall, the
    /// desks and cats as boxes.
    pub fn room_grid(&self, room: usize) -> Grid {
        let s = self.slots()[room];
        let (z0, z1, wall) = match s.side {
            Side::North => (-(HALL_W / 2.0 + ROOM_DEPTH), -HALL_W / 2.0, Wall::South),
            Side::South => (HALL_W / 2.0, HALL_W / 2.0 + ROOM_DEPTH, Wall::North),
        };
        let floor = Aabb::new(Vec2::new(s.x0, z0), Vec2::new(s.x0 + s.len, z1));
        let mut obstacles = Vec::new();
        for (d, desk) in self.rooms[room].desks.iter().enumerate() {
            let p = self.screen_place(room, d, 0);
            let toward = -facing(p.yaw);
            let half = self.screen_logical(room, d, 0).x * onetoone::M_PER_PX / 2.0 + 0.2;
            let c = p.center + toward * 0.25;
            obstacles.push(Aabb::new(
                Vec2::new(c.x - half, c.z - 0.4),
                Vec2::new(c.x + half, c.z + 0.4),
            ));
            if desk.cat.is_some() {
                let c = c + toward * 0.45;
                obstacles.push(Aabb::new(
                    Vec2::new(c.x - 0.3, c.z - 0.3),
                    Vec2::new(c.x + 0.3, c.z + 0.3),
                ));
            }
        }
        let r = self.walk_params.radius;
        Grid::room(floor, &[Gap::new(wall, s.door_x)], &obstacles, r)
    }

    fn walk_start(&mut self, now: Time) {
        let room = self.current_room();
        let mut start = self.camera;
        start.pitch = 0.0;
        self.tween = None;
        self.walking = Some(Walking {
            walker: Walker::new(&start),
            keys: WalkKeys::default(),
            grid: self.room_grid(room),
            last: now,
        });
    }

    /// `walk.on_exit = auto` (§3.5): a station close by and ahead, else
    /// stay where you are.
    fn walk_exit(&mut self, now: Time) {
        let room = self.room_at(self.camera.pos).unwrap_or(self.current_room());
        let to = walk_exit::exit(ExitRule::Auto, &self.camera, &self.spots(room));
        self.station = to.station;
        let opts = TweenOpts {
            reduced_motion: self.opts.reduced_motion,
            max: walk_exit::EXIT_GLIDE,
            ..TweenOpts::default()
        };
        let t = Tween::plan(self.camera, to.pose, now, opts);
        self.tween = (t.dur > 0.0).then_some(t);
    }

    /// A walking key went down or up (ANDAR only); `s`: the keysym.
    /// Returns whether it is one.
    pub fn walk_key(&mut self, s: u32, down: bool) -> bool {
        let Some(w) = self.walking.as_mut() else {
            return false;
        };
        let lower = if (0x41..=0x5a).contains(&s) {
            s | 0x20
        } else {
            s
        };
        let k = &mut w.keys;
        let slot = match lower {
            0x77 => &mut k.forward,
            0x73 => &mut k.back,
            0x61 => &mut k.left,
            0x64 => &mut k.right,
            0x71 | sym::LEFT => &mut k.turn_left,
            0x65 | sym::RIGHT => &mut k.turn_right,
            sym::UP => &mut k.look_up,
            sym::DOWN => &mut k.look_down,
            0xffe1 | 0xffe2 => &mut k.run,
            0x63 if down => {
                w.walker.pitch = 0.0;
                return true;
            }
            _ => return false,
        };
        *slot = down;
        true
    }

    /// Advance the camera to `now`. Returns true on the frame a FOCO glide
    /// arrives (the quad is swapped for the 2D element in this same frame).
    pub fn tick(&mut self, now: Time) -> bool {
        if let Some(w) = self.walking.as_mut() {
            let dt = (now - w.last).max(0.0);
            w.last = now;
            let input = w.keys.input();
            w.walker.update(&w.grid, &input, &self.walk_params, dt);
            self.camera = w.walker.pose(&self.walk_params);
        }
        let Some(t) = self.tween else {
            return false;
        };
        let f = t.sample(now);
        self.camera = f.pose;
        if f.done {
            self.tween = None;
            if let Some(s) = self.arrive.take() {
                self.focus_2d = Some(s);
                // one last world pass: the dimmed surroundings under the 2D window
                self.arrival_pass = true;
                return true;
            }
            self.pending.push(Wish::world(Want::Once));
        }
        false
    }

    /// The fade of a cut at `now` (0: none).
    pub fn fade(&self, now: Time) -> f32 {
        self.tween.map_or(0.0, |t| t.sample(now).fade)
    }

    /// Ask for a frame (a commit, the HUD …).
    pub fn want(&mut self, w: Wish) {
        self.pending.push(w);
    }

    /// Everything that wants frames now.
    pub fn wishes(&self) -> Vec<Wish> {
        let mut w = self.pending.clone();
        let walking = self
            .walking
            .as_ref()
            .is_some_and(|w| w.keys.any() || w.walker.moving());
        if self.tween.is_some() || walking {
            w.push(Wish::world(Want::Continuous));
        }
        if self.frozen && self.focus_2d.is_some() {
            // FOCO: the world pass does not run; what wanted it only composes
            for x in &mut w {
                x.pass = nkw_core::Pass::Compose;
            }
        }
        if self.arrival_pass {
            w.push(Wish::world(Want::Once));
        }
        w
    }

    /// A frame was drawn at `now`: drop the wants it satisfied.
    pub fn frame_done(&mut self, now: Time) {
        self.arrival_pass = false;
        self.pending
            .retain(|w| matches!(w.want, Want::At(t) if t > now + 1e-6));
    }

    /// The JSON of `dump` (contract S2, `"version": 1`).
    pub fn dump(&self, wants: &[Wish]) -> serde_json::Value {
        let rooms: Vec<_> = self
            .rooms
            .iter()
            .zip(self.slots())
            .map(|(r, s)| DumpRoom {
                id: r.id.0,
                name: r.name.clone(),
                side: if s.side == Side::North {
                    "north"
                } else {
                    "south"
                },
                x0: s.x0,
                len: s.len,
            })
            .collect();
        let mut desks = Vec::new();
        let mut screens = Vec::new();
        for (ri, r) in self.rooms.iter().enumerate() {
            for (di, d) in r.desks.iter().enumerate() {
                desks.push(DumpDesk {
                    room: r.id.0,
                    width: d.width,
                    slots: d.screens.iter().map(|s| s.0).collect(),
                    cat: d.cat.map(|c| c.0),
                });
                for (k, sid) in d.screens.iter().enumerate() {
                    let Some(s) = self.screens.get(sid) else {
                        continue;
                    };
                    let place = self.screen_place(ri, di, k);
                    screens.push(DumpScreen {
                        id: s.id.0,
                        app_id: s.app_id.clone(),
                        title: s.title.clone(),
                        size: [s.size.x, s.size.y],
                        placement: DumpPlacement {
                            room: r.id.0,
                            desk: di,
                            slot: k,
                            center: place.center.to_array(),
                            yaw: place.yaw,
                        },
                    });
                }
            }
        }
        let focus = self.focus_2d().map(|(id, o)| DumpFocus {
            screen: id.0,
            rect: [o.origin.x, o.origin.y, o.size.x, o.size.y],
        });
        let d = Dump {
            version: 1,
            mode: crate::script::mode_name(self.mode().label()).to_string(),
            station: StationDump::from(&self.station),
            camera: DumpCamera {
                pos: self.camera.pos.to_array(),
                yaw: self.camera.yaw,
                pitch: self.camera.pitch,
            },
            rooms,
            desks,
            screens,
            focus,
            wants: wants.iter().map(want_name).collect(),
        };
        serde_json::to_value(d).expect("dump serialises")
    }
}

fn want_name(w: &Wish) -> String {
    let pass = match w.pass {
        nkw_core::Pass::World => "world",
        nkw_core::Pass::Compose => "compose",
    };
    match w.want {
        Want::Continuous => format!("{pass}:continuous"),
        Want::Rate(hz) => format!("{pass}:rate:{hz}"),
        Want::Once => format!("{pass}:once"),
        Want::At(t) => format!("{pass}:at:{t:.3}"),
    }
}

#[derive(Serialize)]
struct Dump {
    version: u32,
    mode: String,
    station: StationDump,
    camera: DumpCamera,
    rooms: Vec<DumpRoom>,
    desks: Vec<DumpDesk>,
    screens: Vec<DumpScreen>,
    focus: Option<DumpFocus>,
    wants: Vec<String>,
}

/// `nkw_core::Station` has no serde impl (frozen crate): a local mirror.
#[derive(Serialize, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum StationDump {
    Desk { room: u32, desk: usize, slot: usize },
    Air(u32),
    Shelf { dir: u32, row: u32 },
    Door(u32),
    Board,
    Free { pos: [f32; 3], yaw: f32, pitch: f32 },
}

impl From<&Station> for StationDump {
    fn from(s: &Station) -> StationDump {
        match *s {
            Station::Desk { room, desk, slot } => StationDump::Desk {
                room: room.0,
                desk,
                slot,
            },
            Station::Air(s) => StationDump::Air(s.0),
            Station::Shelf { dir, row } => StationDump::Shelf { dir: dir.0, row },
            Station::Door(r) => StationDump::Door(r.0),
            Station::Board => StationDump::Board,
            Station::Free(p) => StationDump::Free {
                pos: p.pos.to_array(),
                yaw: p.yaw,
                pitch: p.pitch,
            },
        }
    }
}

#[derive(Serialize)]
struct DumpCamera {
    pos: [f32; 3],
    yaw: f32,
    pitch: f32,
}

#[derive(Serialize)]
struct DumpRoom {
    id: u32,
    name: String,
    side: &'static str,
    x0: f32,
    len: f32,
}

#[derive(Serialize)]
struct DumpDesk {
    room: u32,
    width: Width,
    slots: Vec<u32>,
    cat: Option<u32>,
}

#[derive(Serialize)]
struct DumpScreen {
    id: u32,
    app_id: String,
    title: String,
    size: [f32; 2],
    placement: DumpPlacement,
}

#[derive(Serialize)]
struct DumpPlacement {
    room: u32,
    desk: usize,
    slot: usize,
    center: [f32; 3],
    yaw: f32,
}

#[derive(Serialize)]
struct DumpFocus {
    screen: u32,
    rect: [f32; 4],
}

#[cfg(test)]
mod tests {
    use super::*;
    use nkw_core::{Base, CatId};

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

    /// One room, three desks with one screen each (ids 1, 2, 3).
    fn three() -> World {
        let rooms = vec![Room {
            id: RoomId(0),
            name: "acme".into(),
            desks: vec![desk(&[1], None), desk(&[2], Some(9)), desk(&[3], None)],
        }];
        let mut w = World::with_rooms(OUT, OPTS, rooms);
        for i in 1..=3 {
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
        w.next_id = 4;
        w
    }

    /// Run until every glide is over.
    fn settle(w: &mut World, now: &mut Time) {
        for _ in 0..100 {
            if !w.gliding() {
                return;
            }
            *now += 1.0 / 60.0;
            w.tick(*now);
        }
        panic!("glide never ended");
    }

    fn desk_of(w: &World) -> usize {
        match w.station {
            Station::Desk { desk, .. } => desk,
            other => panic!("not at a desk: {other:?}"),
        }
    }

    #[test]
    fn stations_from_a_demo_strip() {
        let w = three();
        let spots = w.spots(0);
        assert_eq!(spots.len(), 3);
        // h/l order runs left to right along the room, 0.624 m desks 0.8 m apart
        let xs: Vec<f32> = spots.iter().map(|s| s.pose.pos.x).collect();
        assert!((xs[1] - xs[0] - (0.624 + 0.8)).abs() < 1e-4, "{xs:?}");
        assert!((xs[2] - xs[1] - (0.624 + 0.8)).abs() < 1e-4, "{xs:?}");
        for (i, s) in spots.iter().enumerate() {
            assert_eq!(s.pose.pos.y, EYE);
            assert_eq!(s.pose.yaw, 0.0);
            // NAV_DIST in front of the screen it faces
            let place = w.screen_place(0, i, 0);
            assert!((place.center.z + NAV_DIST - s.pose.pos.z).abs() < 1e-4);
            assert!((place.center.x - s.pose.pos.x).abs() < 1e-4);
        }
    }

    #[test]
    fn south_rooms_face_the_other_way() {
        let rooms = (0..2)
            .map(|i| Room {
                id: RoomId(i),
                name: format!("r{i}"),
                desks: vec![desk(&[], None), desk(&[], None)],
            })
            .collect();
        let w = World::with_rooms(OUT, OPTS, rooms);
        let s = w.spots(1);
        assert!((s[0].pose.yaw - std::f32::consts::PI).abs() < 1e-6);
        // h/l still go to the camera's right
        let step = s[1].pose.pos - s[0].pose.pos;
        assert!(step.dot(s[0].pose.right()) > 0.0);
        assert!(s[0].pose.pos.z > 0.0, "south of the hall");
        let place = w.screen_place(1, 0, 0);
        assert!(place.center.z > s[0].pose.pos.z, "the screens are ahead");
    }

    #[test]
    fn h_and_l_reach_the_right_station() {
        let mut w = three();
        let mut now = 0.0;
        assert_eq!(desk_of(&w), 0);
        w.command(Cmd::Move(Dir::Right), now);
        settle(&mut w, &mut now);
        assert_eq!(desk_of(&w), 1);
        assert_eq!(w.camera, w.spots(0)[1].pose);
        w.command(Cmd::Move(Dir::Right), now);
        w.command(Cmd::Move(Dir::Right), now); // past the end: stays
        settle(&mut w, &mut now);
        assert_eq!(desk_of(&w), 2);
        w.command(Cmd::Move(Dir::Left), now);
        settle(&mut w, &mut now);
        assert_eq!(desk_of(&w), 1);
        assert_eq!(w.selected(), Some(ScreenId(2)));
    }

    #[test]
    fn h_l_is_a_lateral_glide() {
        let mut w = three();
        let yaw = w.camera.yaw;
        w.command(Cmd::Move(Dir::Right), 0.0);
        assert!(w.gliding());
        w.tick(0.05);
        assert_eq!(w.camera.yaw, yaw);
        assert!(w.wishes().iter().any(|x| x.want == Want::Continuous));
    }

    #[test]
    fn focus_pose_equals_onetoone() {
        let mut w = three();
        let mut now = 0.0;
        let out = w.command(Cmd::Focus, now);
        assert_eq!(out, [Out::KeyboardEnter(ScreenId(1))]);
        assert_eq!(w.mode().label(), "FOCO");
        settle(&mut w, &mut now);
        let place = w.screen_place(0, 0, 0);
        let want = onetoone::pose(&OUT, FOV_Y, Vec2::new(624.0, 720.0), &place);
        assert_eq!(w.camera, want.pose);
        let (id, o) = w.focus_2d().unwrap();
        assert_eq!(id, ScreenId(1));
        assert_eq!(o, want);
        assert_eq!(o.origin, Vec2::new(328.0, 0.0));
    }

    #[test]
    fn focus_swap_happens_on_the_frame_the_tween_ends() {
        let mut w = three();
        w.command(Cmd::Focus, 0.0);
        let end = w.tween.unwrap().end();
        assert!(!w.tick(end - 0.001));
        assert!(w.focus_2d().is_none(), "still the quad while gliding");
        assert!(w.tick(end), "arrives on this frame");
        assert!(w.focus_2d().is_some(), "swapped in the same frame");
        assert_eq!(w.camera, w.one_to_one(ScreenId(1)).unwrap().pose);
        assert!(!w.tick(end + 0.016), "only once");
    }

    #[test]
    fn reduced_motion_cuts_and_still_swaps() {
        let mut w = three();
        w.opts.reduced_motion = true;
        w.command(Cmd::Focus, 0.0);
        let t = w.tween.unwrap();
        assert_eq!(t.path, nkw_core::Path::Cut);
        assert!(w.tick(t.end()));
    }

    #[test]
    fn back_from_focus_returns_to_the_station() {
        let mut w = three();
        let mut now = 0.0;
        w.command(Cmd::Move(Dir::Right), now);
        settle(&mut w, &mut now);
        let nav = w.camera;
        w.command(Cmd::Focus, now);
        settle(&mut w, &mut now);
        assert!(w.frozen);
        let out = w.command(Cmd::Back, now);
        assert_eq!(out, [Out::KeyboardLeave, Out::PopupsDone]);
        assert!(w.focus_2d().is_none(), "2D becomes the quad again at once");
        assert!(!w.frozen);
        settle(&mut w, &mut now);
        assert_eq!(w.camera, nav);
        assert_eq!(w.mode().base, Base::Nav);
        assert!(w.mode().overlay.is_none());
    }

    #[test]
    fn neighbour_in_focus_travels_and_moves_the_keyboard() {
        let mut w = three();
        let mut now = 0.0;
        w.command(Cmd::Focus, now);
        settle(&mut w, &mut now);
        let out = w.command(Cmd::Neighbour(Dir::Right), now);
        assert_eq!(
            out,
            [
                Out::KeyboardLeave,
                Out::PopupsDone,
                Out::KeyboardEnter(ScreenId(2))
            ]
        );
        assert!(w.focus_2d().is_none());
        assert_eq!(w.tween.unwrap().path, nkw_core::Path::Lateral);
        settle(&mut w, &mut now);
        assert_eq!(w.focus_2d().unwrap().0, ScreenId(2));
        assert_eq!(desk_of(&w), 1);
        // back lands on the neighbour's station
        w.command(Cmd::Back, now);
        settle(&mut w, &mut now);
        assert_eq!(w.camera, w.spots(0)[1].pose);
    }

    #[test]
    fn focus_on_an_empty_desk_does_nothing() {
        let rooms = vec![Room {
            id: RoomId(0),
            name: "acme".into(),
            desks: vec![desk(&[], Some(1))],
        }];
        let mut w = World::with_rooms(OUT, OPTS, rooms);
        assert!(w.command(Cmd::Focus, 0.0).is_empty());
        assert_eq!(w.mode().label(), "NAVEGAR");
    }

    #[test]
    fn new_window_lands_right_of_the_station_and_is_selected() {
        let mut w = three();
        let (id, size) = w.add_screen("acme.new", "new", 0.0);
        assert_eq!(id, ScreenId(4));
        assert_eq!(size, Vec2::new(624.0, 720.0));
        assert_eq!(w.rooms[0].desks[1].screens, [id]);
        assert_eq!(desk_of(&w), 1);
        assert_eq!(w.selected(), Some(id));
    }

    #[test]
    fn new_window_in_focus_keeps_the_focus() {
        let mut w = three();
        let mut now = 0.0;
        w.command(Cmd::Move(Dir::Right), now);
        settle(&mut w, &mut now);
        w.command(Cmd::Focus, now);
        settle(&mut w, &mut now);
        w.add_screen("acme.new", "", now);
        assert_eq!(w.focus_target(), Some(ScreenId(2)));
        assert_eq!(w.selected(), Some(ScreenId(2)), "station follows its desk");
        assert!(!w.gliding());
    }

    #[test]
    fn closing_the_focused_window_goes_back_to_nav() {
        let mut w = three();
        let mut now = 0.0;
        w.command(Cmd::Move(Dir::Right), now);
        settle(&mut w, &mut now);
        w.command(Cmd::Focus, now);
        settle(&mut w, &mut now);
        let out = w.remove_screen(ScreenId(2), now);
        assert!(out.contains(&Out::KeyboardLeave));
        assert_eq!(w.mode().label(), "NAVEGAR");
        // the cat's desk stays, empty
        assert_eq!(w.rooms[0].desks.len(), 3);
        assert!(w.rooms[0].desks[1].screens.is_empty());
        settle(&mut w, &mut now);
        assert_eq!(w.camera, w.spots(0)[1].pose);
    }

    #[test]
    fn closing_a_user_window_takes_its_desk() {
        let mut w = three();
        let mut now = 0.0;
        w.command(Cmd::Move(Dir::Right), now);
        w.command(Cmd::Move(Dir::Right), now);
        settle(&mut w, &mut now);
        w.remove_screen(ScreenId(3), now);
        assert_eq!(w.rooms[0].desks.len(), 2);
        assert_eq!(desk_of(&w), 1);
        assert!(w.locate(ScreenId(3)).is_none());
    }

    #[test]
    fn rooms_next_and_prev() {
        let rooms = (0..3)
            .map(|i| Room {
                id: RoomId(i),
                name: format!("r{i}"),
                desks: vec![desk(&[], None)],
            })
            .collect();
        let mut w = World::with_rooms(OUT, OPTS, rooms);
        let mut now = 0.0;
        w.command(Cmd::RoomNext, now);
        settle(&mut w, &mut now);
        assert_eq!(w.current_room(), 1);
        w.command(Cmd::RoomNext, now);
        w.command(Cmd::RoomNext, now);
        settle(&mut w, &mut now);
        assert_eq!(w.current_room(), 2);
        w.command(Cmd::RoomPrev, now);
        settle(&mut w, &mut now);
        assert_eq!(w.current_room(), 1);
    }

    #[test]
    fn idle_world_wants_nothing() {
        let mut w = three();
        w.frame_done(0.0);
        assert!(w.wishes().is_empty());
        let mut now = 0.0;
        w.command(Cmd::Move(Dir::Right), now);
        settle(&mut w, &mut now);
        w.frame_done(now);
        assert!(w.wishes().is_empty(), "{:?}", w.wishes());
    }

    #[test]
    fn frozen_world_only_composes() {
        let mut w = three();
        let mut now = 0.0;
        w.command(Cmd::Focus, now);
        settle(&mut w, &mut now);
        // the arrival frame takes one world pass (the dimmed surroundings)
        assert!(w.wishes().iter().any(|x| x.pass == nkw_core::Pass::World));
        w.frame_done(now);
        w.want(Wish::world(Want::Once));
        assert!(w.wishes().iter().all(|x| x.pass == nkw_core::Pass::Compose));
    }

    fn nav() -> Mode {
        Mode::base(Base::Nav)
    }

    fn focus() -> Mode {
        let mut w = three();
        w.command(Cmd::Focus, 0.0);
        w.mode().clone()
    }

    #[test]
    fn key_filter_table() {
        let none = KeyMods::default();
        let ctrl = KeyMods { ctrl: true, ..none };
        let world = KeyMods {
            ctrl: true,
            alt: true,
            shift: false,
        };
        let shift = KeyMods {
            shift: true,
            ..none
        };
        let alt = KeyMods { alt: true, ..none };
        use Verdict::{Forward as F, Intercept as I};
        let cases: &[(Mode, KeyMods, u32, Verdict)] = &[
            // NAV: everything is the world's
            (nav(), none, sym::H, I(Some(Cmd::Move(Dir::Left)))),
            (nav(), none, sym::L, I(Some(Cmd::Move(Dir::Right)))),
            (nav(), none, sym::LEFT, I(Some(Cmd::Move(Dir::Left)))),
            (nav(), none, sym::J, I(Some(Cmd::Move(Dir::Down)))),
            (nav(), none, sym::K, I(Some(Cmd::Move(Dir::Up)))),
            (nav(), shift, sym::UPPER_J, I(Some(Cmd::RoomNext))),
            (nav(), shift, sym::UPPER_K, I(Some(Cmd::RoomPrev))),
            (nav(), none, sym::RETURN, I(Some(Cmd::Focus))),
            (nav(), none, sym::I, I(Some(Cmd::Focus))),
            (nav(), none, sym::W, I(Some(Cmd::Walk))),
            (nav(), none, sym::ESCAPE, I(Some(Cmd::Back))),
            (nav(), none, 0x61, I(None)),
            (nav(), ctrl, sym::BRACKETRIGHT, I(None)),
            (nav(), alt, sym::H, I(None)),
            (nav(), world, sym::W, I(Some(Cmd::Walk))),
            // FOCO: only ctrl+] and world+ are intercepted
            (focus(), none, sym::H, F),
            (focus(), none, sym::RETURN, F),
            (focus(), none, sym::ESCAPE, F),
            (focus(), shift, sym::UPPER_J, F),
            (focus(), ctrl, 0x63, F),
            (focus(), alt, sym::H, F),
            (focus(), ctrl, sym::BRACKETRIGHT, I(Some(Cmd::Back))),
            (focus(), world, sym::H, I(Some(Cmd::Neighbour(Dir::Left)))),
            (focus(), world, sym::L, I(Some(Cmd::Neighbour(Dir::Right)))),
            (focus(), world, sym::J, I(Some(Cmd::Neighbour(Dir::Down)))),
            (focus(), world, sym::K, I(Some(Cmd::Neighbour(Dir::Up)))),
            (focus(), world, sym::W, I(Some(Cmd::Walk))),
            (focus(), world, 0x57, I(Some(Cmd::Walk))),
            (focus(), world, 0x61, I(None)),
            (focus(), world, sym::BRACKETRIGHT, I(None)),
        ];
        for (mode, m, s, want) in cases {
            assert_eq!(filter(mode, *m, *s), *want, "{} {m:?} {s:#x}", mode.label());
        }
    }

    #[test]
    fn walking_mode_keeps_only_esc_and_w() {
        let mut w = three();
        w.command(Cmd::Walk, 0.0);
        assert!(w.mode().walking());
        let m = w.mode().clone();
        assert_eq!(
            filter(&m, KeyMods::default(), sym::H),
            Verdict::Intercept(None)
        );
        assert_eq!(
            filter(&m, KeyMods::default(), sym::W),
            Verdict::Intercept(None),
            "w walks forward"
        );
        let world = KeyMods {
            ctrl: true,
            alt: true,
            shift: false,
        };
        assert_eq!(
            filter(&m, world, sym::W),
            Verdict::Intercept(Some(Cmd::Walk))
        );
        assert_eq!(
            filter(&m, KeyMods::default(), sym::ESCAPE),
            Verdict::Intercept(Some(Cmd::Back))
        );
        w.command(Cmd::Back, 0.0);
        assert_eq!(w.mode().label(), "NAVEGAR");
    }

    /// Walk with `keys` held for `secs` at 60 fps.
    fn walk(w: &mut World, now: &mut Time, keys: &[u32], secs: f64) {
        for &k in keys {
            assert!(w.walk_key(k, true));
        }
        for _ in 0..(secs * 60.0) as usize {
            *now += 1.0 / 60.0;
            w.tick(*now);
        }
        for &k in keys {
            w.walk_key(k, false);
        }
    }

    #[test]
    fn walking_forward_stops_at_the_desk() {
        let mut w = three();
        let mut now = 0.0;
        w.command(Cmd::Walk, now);
        assert!(w.walking.is_some());
        let start = w.camera;
        walk(&mut w, &mut now, &[sym::W], 3.0);
        let desk_front = w.screen_place(0, 0, 0).center.z + 0.25 + 0.4;
        assert!(w.camera.pos.z < start.pos.z - 1.0, "moved forward");
        assert!(
            w.camera.pos.z >= desk_front + w.walk_params.radius - 0.26,
            "{} vs desk front {desk_front}",
            w.camera.pos.z
        );
        for _ in 0..60 {
            now += 1.0 / 60.0;
            w.tick(now);
        }
        w.frame_done(now);
        assert!(w.wishes().is_empty(), "standing still wants no frames");
    }

    #[test]
    fn walking_turns_and_strafes() {
        let mut w = three();
        let mut now = 0.0;
        w.command(Cmd::Walk, now);
        walk(&mut w, &mut now, &[0x65], 0.5); // e: turn right
        assert!(w.camera.yaw > 0.5, "{}", w.camera.yaw);
        let x = w.camera.pos.x;
        walk(&mut w, &mut now, &[0x61], 0.5); // a: strafe left
        assert!(w.camera.pos.x != x);
        assert!(!w.walk_key(sym::H, true), "h is not a walking key");
    }

    #[test]
    fn leaving_andar_near_a_station_snaps_to_it() {
        let mut w = three();
        let mut now = 0.0;
        let nav = w.camera;
        w.command(Cmd::Walk, now);
        walk(&mut w, &mut now, &[0x73], 0.2); // a little back
        w.command(Cmd::Back, now);
        assert_eq!(w.mode().label(), "NAVEGAR");
        assert!(w.walking.is_none());
        settle(&mut w, &mut now);
        assert_eq!(w.camera, nav);
        assert_eq!(desk_of(&w), 0);
    }

    #[test]
    fn leaving_andar_far_from_stations_stays_free() {
        let mut w = three();
        let mut now = 0.0;
        w.command(Cmd::Walk, now);
        walk(&mut w, &mut now, &[0x65], 0.8); // turn right, well past 40°
        w.command(Cmd::Back, now);
        assert!(matches!(w.station, Station::Free(_)), "{:?}", w.station);
    }

    #[test]
    fn dump_has_the_contract_keys() {
        let mut w = three();
        let mut now = 0.0;
        w.command(Cmd::Focus, now);
        settle(&mut w, &mut now);
        let d = w.dump(&w.wishes());
        for k in [
            "version", "mode", "station", "camera", "rooms", "desks", "screens", "focus", "wants",
        ] {
            assert!(d.get(k).is_some(), "missing {k}: {d}");
        }
        assert_eq!(d["version"], 1);
        assert_eq!(d["mode"], "FOCO");
        assert_eq!(d["focus"]["screen"], 1);
        assert_eq!(
            d["focus"]["rect"],
            serde_json::json!([328.0, 0.0, 624.0, 720.0])
        );
        assert_eq!(d["station"]["desk"]["desk"], 0);
        assert_eq!(d["desks"][1]["cat"], 9);
        assert_eq!(d["desks"][1]["width"], "half");
        assert_eq!(d["screens"][2]["app_id"], "acme.3");
    }
}
