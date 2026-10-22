//! The world as `nkw` sees it: `nkw-core`'s mode machine, strip, 1:1 pose,
//! tweens and scheduler glued to rooms, desks and screens. Pure (no
//! Wayland, no GL): the compositor maps `ScreenId`s to its windows and
//! turns the returned [`Out`]s into keyboard focus changes.

use std::collections::BTreeMap;

use glam::{Vec2, Vec3};
use nkw_core::collide::{Aabb, Door as Gap, Grid, WalkInput, WalkParams, Walker, Wall};
use nkw_core::layout;
use nkw_core::strip::{self, Desk, Dir, RoomSlot, Side, Spot, Station, Width};
use nkw_core::walk_exit::{self, ExitRule};
use nkw_core::{
    Action, CatId, Effect, Machine, Mode, ObjectAction, ObjectId, OneToOne, Output, Pose, RoomId,
    ScreenId, ScreenPlace, TalkPurpose, Target, Time, Tween, TweenOpts, Want, Wish, onetoone,
};
use serde::Serialize;

use crate::router::sym;

#[path = "wm/world_ops.rs"]
mod ops;
use nkw_core::persist::RoomKind;
pub use ops::{
    ATTENTION_S, COMMANDS, CmdInfo, DIALOG_DEFAULT, DIALOG_FRONT, Hold, LAYOUT_NOTE_S, RoomRef,
    ScreenState, ShortName, info as command_info, width_label,
};

/// The geometry lives in `nkw_core::layout`; its constants are re-exported
/// here for the hub and the plugins.
pub use nkw_core::layout::{
    EYE, FOV_Y, HALL_W, NAV_DIST, NAV_PITCH, ROOM_DEPTH, ROOM_END, ROOM_MIN, SCREEN_FROM_WALL,
    facing,
};

/// The reception's name: always the first room (§6).
pub const RECEPTION: &str = "recepção";
/// The user's own room: always the last one.
pub const MY_ROOM: &str = "minha sala";
/// The time-of-day band before the clock says otherwise (§5.4).
pub const DEFAULT_BAND: u8 = 3;

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

/// World commands, already resolved from keys or names (S9).
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
    /// `world.band {band}`: the time-of-day band (0..8)
    Band(u8),
    // --- the window manager (P2) ---
    /// `world.back`: leaves FOCO (and every overlay); a step back in ANDAR
    BackOrStep,
    /// `world.room_N` (1-based)
    Room(u8),
    /// `gg` / `G`
    First,
    Last,
    /// `t`: the station's cat
    Talk,
    TalkNext,
    TalkReply,
    /// `f` in CONVERSA
    TalkSit,
    /// `f` / `F`
    Hint {
        focus: bool,
    },
    /// `:`
    Palette,
    /// `H` / `L` (Left/Right)
    Shift(Dir),
    /// `,`
    Consume,
    /// `.`
    Expel,
    /// `-` / `=`
    Narrower,
    Wider,
    Maximize,
    Fullscreen,
    /// `p`
    Pin,
    /// `M {room}`
    SendRoom(RoomRef),
    /// `Q` / `world+q`
    Close,
    /// `world.room_new {name}` (`go`: and go there; `context`: a context's)
    RoomNew {
        name: ShortName,
        context: bool,
        go: bool,
    },
    /// `world.goto_room {room}`, `world.goto_room:<id>`
    GotoRoom(RoomRef),
    /// `world.goto_screen:<id>` (the palette's "ir para" items)
    GotoScreen(ScreenId),
    /// `world.forward` … (held until the key's release)
    Hold(Hold),
    /// `c` in ANDAR
    Recenter,
}

impl Cmd {
    /// The command of an S9 name (`world.left` …); `None`: not a command
    /// the World runs (the plugins may take it).
    pub fn from_name(name: &str, args: &BTreeMap<String, String>) -> Option<Cmd> {
        Some(match name {
            "world.left" => Cmd::Move(Dir::Left),
            "world.right" => Cmd::Move(Dir::Right),
            "world.up" => Cmd::Move(Dir::Up),
            "world.down" => Cmd::Move(Dir::Down),
            "world.room_next" => Cmd::RoomNext,
            "world.room_prev" => Cmd::RoomPrev,
            "world.focus" => Cmd::Focus,
            "world.walk_exit" | "world.talk_end" | "world.hint_cancel" | "world.palette_cancel" => {
                Cmd::Back
            }
            "world.back" => Cmd::BackOrStep,
            "world.walk" => Cmd::Walk,
            "world.focus_left" => Cmd::Neighbour(Dir::Left),
            "world.focus_right" => Cmd::Neighbour(Dir::Right),
            "world.focus_up" => Cmd::Neighbour(Dir::Up),
            "world.focus_down" => Cmd::Neighbour(Dir::Down),
            "world.band" => Cmd::Band(args.get("band")?.parse().ok().filter(|b| *b <= 8)?),
            "world.first" => Cmd::First,
            "world.last" => Cmd::Last,
            "world.talk" => Cmd::Talk,
            "world.talk_next" => Cmd::TalkNext,
            "world.talk_reply" => Cmd::TalkReply,
            "world.talk_sit" => Cmd::TalkSit,
            "world.hint" => Cmd::Hint { focus: false },
            "world.hint_focus" => Cmd::Hint { focus: true },
            "world.palette" => Cmd::Palette,
            "world.move_left" => Cmd::Shift(Dir::Left),
            "world.move_right" => Cmd::Shift(Dir::Right),
            "world.consume" => Cmd::Consume,
            "world.expel" => Cmd::Expel,
            "world.narrower" => Cmd::Narrower,
            "world.wider" => Cmd::Wider,
            "world.maximize" => Cmd::Maximize,
            "world.fullscreen" => Cmd::Fullscreen,
            "world.pin" => Cmd::Pin,
            "world.send_room" => Cmd::SendRoom(RoomRef::parse(args.get("room")?)?),
            "world.close" => Cmd::Close,
            "world.room_new" => {
                let name = args.get("name")?.trim();
                if name.is_empty() {
                    return None;
                }
                Cmd::RoomNew {
                    name: ShortName::new(name),
                    context: false,
                    go: true,
                }
            }
            "world.goto_room" => Cmd::GotoRoom(RoomRef::parse(args.get("room")?)?),
            "world.forward" => Cmd::Hold(Hold::Forward),
            "world.strafe_left" => Cmd::Hold(Hold::StrafeLeft),
            "world.strafe_right" => Cmd::Hold(Hold::StrafeRight),
            "world.turn_left" => Cmd::Hold(Hold::TurnLeft),
            "world.turn_right" => Cmd::Hold(Hold::TurnRight),
            "world.look_up" => Cmd::Hold(Hold::LookUp),
            "world.look_down" => Cmd::Hold(Hold::LookDown),
            "world.recenter" => Cmd::Recenter,
            GOTO_SCREEN => Cmd::GotoScreen(ScreenId(args.get("screen")?.parse().ok()?)),
            n => {
                if let Some(k) = n.strip_prefix("world.room_") {
                    let k: u8 = k.parse().ok().filter(|k| (1..=9).contains(k))?;
                    return Some(Cmd::Room(k));
                }
                if let Some(id) = n.strip_prefix("world.goto_room:") {
                    return Some(Cmd::GotoRoom(RoomRef::Id(RoomId(id.parse().ok()?))));
                }
                if let Some(id) = n.strip_prefix("world.goto_screen:") {
                    return Some(Cmd::GotoScreen(ScreenId(id.parse().ok()?)));
                }
                return None;
            }
        })
    }

    /// The S9 name and args of the command (the inverse of `from_name`;
    /// `Back` is `world.back`).
    pub fn name(&self) -> (&'static str, BTreeMap<String, String>) {
        let n = match self {
            Cmd::Move(Dir::Left) => "world.left",
            Cmd::Move(Dir::Right) => "world.right",
            Cmd::Move(Dir::Up) => "world.up",
            Cmd::Move(Dir::Down) => "world.down",
            Cmd::RoomNext => "world.room_next",
            Cmd::RoomPrev => "world.room_prev",
            Cmd::Focus => "world.focus",
            // (esc and ctrl+] alike: leave whatever mode is on)
            Cmd::Back => "world.walk_exit",
            Cmd::BackOrStep => "world.back",
            Cmd::Walk => "world.walk",
            Cmd::Neighbour(Dir::Left) => "world.focus_left",
            Cmd::Neighbour(Dir::Right) => "world.focus_right",
            Cmd::Neighbour(Dir::Up) => "world.focus_up",
            Cmd::Neighbour(Dir::Down) => "world.focus_down",
            Cmd::Band(b) => {
                return (
                    "world.band",
                    BTreeMap::from([("band".to_string(), b.to_string())]),
                );
            }
            Cmd::Room(k) => {
                return (ROOM_N[((*k).clamp(1, 9) - 1) as usize], BTreeMap::new());
            }
            Cmd::First => "world.first",
            Cmd::Last => "world.last",
            Cmd::Talk => "world.talk",
            Cmd::TalkNext => "world.talk_next",
            Cmd::TalkReply => "world.talk_reply",
            Cmd::TalkSit => "world.talk_sit",
            Cmd::Hint { focus: false } => "world.hint",
            Cmd::Hint { focus: true } => "world.hint_focus",
            Cmd::Palette => "world.palette",
            Cmd::Shift(Dir::Left | Dir::Up) => "world.move_left",
            Cmd::Shift(Dir::Right | Dir::Down) => "world.move_right",
            Cmd::Consume => "world.consume",
            Cmd::Expel => "world.expel",
            Cmd::Narrower => "world.narrower",
            Cmd::Wider => "world.wider",
            Cmd::Maximize => "world.maximize",
            Cmd::Fullscreen => "world.fullscreen",
            Cmd::Pin => "world.pin",
            Cmd::Close => "world.close",
            Cmd::SendRoom(r) => {
                return (
                    "world.send_room",
                    BTreeMap::from([("room".to_string(), r.arg())]),
                );
            }
            Cmd::GotoRoom(r) => {
                return (
                    "world.goto_room",
                    BTreeMap::from([("room".to_string(), r.arg())]),
                );
            }
            Cmd::RoomNew { name, .. } => {
                return (
                    "world.room_new",
                    BTreeMap::from([("name".to_string(), name.as_str().to_string())]),
                );
            }
            Cmd::GotoScreen(s) => {
                return (
                    GOTO_SCREEN,
                    BTreeMap::from([("screen".to_string(), s.0.to_string())]),
                );
            }
            Cmd::Hold(h) => match h {
                Hold::Forward => "world.forward",
                Hold::Back => "world.back",
                Hold::StrafeLeft => "world.strafe_left",
                Hold::StrafeRight => "world.strafe_right",
                Hold::TurnLeft => "world.turn_left",
                Hold::TurnRight => "world.turn_right",
                Hold::LookUp => "world.look_up",
                Hold::LookDown => "world.look_down",
            },
            Cmd::Recenter => "world.recenter",
        };
        (n, BTreeMap::new())
    }
}

/// The `(name, title)` of every `world.*` command of S9 that the World
/// runs (`Cmd::from_name` takes each one, with its args).
pub fn command_names() -> &'static [(&'static str, &'static str)] {
    &ops::NAMES
}

/// `world.goto_screen {screen}` (the palette's "ir para" items use
/// `world.goto_screen:<id>` and `world.goto_room:<id>`).
pub const GOTO_SCREEN: &str = "world.goto_screen";

const ROOM_N: [&str; 9] = [
    "world.room_1",
    "world.room_2",
    "world.room_3",
    "world.room_4",
    "world.room_5",
    "world.room_6",
    "world.room_7",
    "world.room_8",
    "world.room_9",
];

/// What the compositor (and the plugins) must do after a command, an
/// action or a tick.
#[derive(Clone, Debug, PartialEq)]
pub enum Out {
    KeyboardEnter(ScreenId),
    KeyboardLeave,
    PopupsDone,
    /// a machine effect the World does not handle itself (routed by the hub
    /// to the plugins, S7)
    Effect(Effect),
    /// do this to an object (→ `Objects::on_object`)
    Object(ObjectId, ObjectAction),
    /// configure a toplevel
    Configure {
        screen: ScreenId,
        size: Option<Vec2>,
        fullscreen: Option<bool>,
        maximized: Option<bool>,
    },
    /// `xdg_toplevel.close` (never kills)
    Close(ScreenId),
    /// a process with the world's `WAYLAND_DISPLAY`
    Spawn(Vec<String>),
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
    /// the day band (0..8) the scene lights the world with; set by
    /// `world.band {band}` (the clock)
    pub band: u8,
    tween: Option<Tween>,
    /// the FOCO target to swap in when the tween ends
    arrive: Option<Target>,
    /// the target composed 2D, 1:1 (FOCO, arrived)
    focus_2d: Option<Target>,
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
    next_room: u32,
    /// extra floor boxes per room, from the plugins (closed doors, furniture)
    obstacles: BTreeMap<RoomId, Vec<Aabb>>,
    /// the purpose of the CONVERSA the last action was about (routes its
    /// effects: cats for `Session`, objects for `VaultUnlock`)
    purpose: TalkPurpose,
    /// the WM's per-screen state (dialogs, air, fullscreen …)
    wm: BTreeMap<ScreenId, ScreenState>,
    /// room kinds that do not follow from the name (free, library …)
    kinds: BTreeMap<RoomId, RoomKind>,
    /// the layout badge: when and what
    note: Option<(Time, String)>,
    /// counts layout changes (persistence)
    layout_gen: u64,
}

impl World {
    /// An empty world: one room ("minha sala"), no desks.
    pub fn new(out: Output, opts: Opts) -> World {
        let rooms = vec![Room {
            id: RoomId(0),
            name: MY_ROOM.into(),
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
            band: DEFAULT_BAND,
            tween: None,
            arrive: None,
            focus_2d: None,
            frozen: false,
            pending: Vec::new(),
            arrival_pass: false,
            walking: None,
            walk_params: WalkParams::default(),
            next_id: 1,
            next_room: 0,
            obstacles: BTreeMap::new(),
            purpose: TalkPurpose::Session,
            wm: BTreeMap::new(),
            kinds: BTreeMap::new(),
            note: None,
            layout_gen: 0,
        };
        w.next_room = w.rooms.iter().map(|r| r.id.0 + 1).max().unwrap_or(0);
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
            Station::Air(s) => match self.locate(s) {
                Some((r, _, _)) => r,
                None => self.room_at(self.camera.pos).unwrap_or(0),
            },
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
        layout::room_entry(&self.slots()[room])
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

    /// Where screen `slot` of a desk hangs (`layout::screen_place`: the
    /// stack centred at the eye, on the desk's monitor; stacks over three
    /// pile behind; dialogs in front of their mother; pinned screens in the
    /// air).
    pub fn screen_place(&self, room: usize, desk: usize, slot: usize) -> ScreenPlace {
        self.place_at(room, desk, slot)
    }

    /// The local x (metres) of a desk's centre in its room.
    pub fn desk_x(&self, room: usize, desk: usize) -> f32 {
        self.desk_local_xs(room)[desk]
    }

    /// The logical size a desk's screen shows at: its committed size, or
    /// the configured one before the first buffer.
    pub fn screen_logical(&self, room: usize, desk: usize, slot: usize) -> Vec2 {
        let d = &self.rooms[room].desks[desk];
        if let Some(v) = d.screens.get(slot).and_then(|id| self.loose_logical(*id)) {
            return v;
        }
        let stack = self.stack_len(room, desk);
        d.screens
            .get(slot)
            .and_then(|id| self.screens.get(id))
            .map(|s| s.size)
            .filter(|s| s.x > 0.0 && s.y > 0.0)
            .unwrap_or_else(|| self.configure_size(d.width, stack))
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
                let s = self.slots()[r];
                Some(layout::desk_station(&s, self.desk_local_xs(r)[desk]))
            }
            Station::Free(p) => Some(p),
            Station::Door(room) => Some(self.room_entry_pose(self.room_index(room)?)),
            // a screen in the air: its 1:1 pose, aligned to its normal
            Station::Air(s) => self.one_to_one(s).map(|o| o.pose),
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
                    stack: self.stack_len(room, d).max(1),
                })
            })
            .collect()
    }

    /// The screen the station has selected.
    pub fn selected(&self) -> Option<ScreenId> {
        match self.station {
            Station::Desk { room, desk, slot } => {
                let r = self.room_index(room)?;
                self.rooms[r].desks.get(desk)?;
                (slot < self.stack_len(r, desk)).then(|| self.rooms[r].desks[desk].screens[slot])
            }
            Station::Air(s) => self.screens.contains_key(&s).then_some(s),
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

    /// FOCO on a window, arrived: the window composed in 2D and its rectangle.
    pub fn focus_2d(&self) -> Option<(ScreenId, OneToOne)> {
        match self.focus_2d? {
            Target::Screen(id) => Some((id, self.one_to_one(id)?)),
            Target::CatTerm(_) => None,
        }
    }

    /// FOCO, arrived, on whatever target (a window or a cat's terminal).
    pub fn focus_2d_target(&self) -> Option<(Target, OneToOne)> {
        let t = self.focus_2d?;
        Some((t, self.one_to_one_target(t)?))
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

    /// Glide (or cut) to `to`; the station becomes `Free(to)`.
    pub fn glide_free(&mut self, to: Pose, now: Time) {
        self.station = Station::Free(to);
        self.glide(to, now, false);
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
            Station::Air(s) => match self.locate(s) {
                Some((r, d, _)) if r == room => d + 1,
                _ => self.rooms[room].desks.len(),
            },
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
        } else if let Station::Desk { desk, room: r, .. } = &mut self.station
            && *r == rid
            && *desk >= at
        {
            *desk += 1;
        }
        self.layout_gen += 1;
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
            if self.focus_2d == Some(Target::Screen(id))
                && let Some(o) = self.one_to_one(id)
            {
                self.camera = o.pose;
            }
        }
    }

    /// A window went away.
    pub fn remove_screen(&mut self, id: ScreenId, now: Time) -> Vec<Out> {
        let mut out = Vec::new();
        let st = self.wm.remove(&id).unwrap_or_default();
        // a dialog in FOCO: back to its mother, still in FOCO
        let mother = st
            .parent
            .filter(|m| self.screens.contains_key(m) && self.focus_target() == Some(id));
        if self.focus_target() == Some(id) && mother.is_none() {
            out = self.action(Action::WindowClosed, now);
        }
        if let Station::Air(s) = self.station
            && s == id
            && let Some((r, d, _)) = self.locate(id)
        {
            self.station = Station::Desk {
                room: self.rooms[r].id,
                desk: d,
                slot: 0,
            };
        }
        self.focus_2d = self.focus_2d.filter(|s| *s != Target::Screen(id));
        self.arrive = self.arrive.filter(|s| *s != Target::Screen(id));
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
                let stack = self.stack_len(r, desk).max(1);
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
        if let Some(m) = mother {
            let t = self.modal_child(m).unwrap_or(m);
            out.extend(self.action(Action::Neighbour(Target::Screen(t)), now));
        }
        self.layout_gen += 1;
        self.pending.push(Wish::world(Want::Once));
        out
    }

    /// Run a machine action at `now`: the mode changes and the effects the
    /// World handles itself (camera, focus swap, walking …); the others come
    /// back as `Out::Effect` / `Out::Object` for the hub to route.
    pub fn action(&mut self, a: Action, now: Time) -> Vec<Out> {
        self.layout_gen += 1;
        let before = self.mode().clone();
        let fx = self.machine.apply(a, self.camera);
        self.purpose = self
            .mode()
            .purpose()
            .or(before.purpose())
            .unwrap_or(TalkPurpose::Session);
        self.effects(&fx, now, before.focus())
    }

    /// The CONVERSA purpose the last action was about (`Session` when none).
    pub fn purpose(&self) -> TalkPurpose {
        self.purpose
    }

    /// Run a world command at `now`.
    pub fn command(&mut self, cmd: Cmd, now: Time) -> Vec<Out> {
        self.layout_gen += 1;
        match cmd {
            Cmd::Move(dir) => {
                let room = self.current_room();
                // from the air: like a free spot at its pose
                let cur = match self.station {
                    Station::Air(_) => {
                        Station::Free(self.station_pose(&self.station).unwrap_or(self.camera))
                    }
                    st => st,
                };
                if let Some(st) = strip::neighbour(&self.spots(room), &cur, dir) {
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
                    self.go_room(r as usize, now);
                }
                Vec::new()
            }
            // a modal dialog takes the keyboard from its mother; a cat's desk
            // without a screen: its terminal
            Cmd::Focus => match self.selected() {
                Some(s) => {
                    let t = self.modal_child(s).unwrap_or(s);
                    self.action(Action::Focus(Target::Screen(t)), now)
                }
                None => match self.station_cat() {
                    Some(c) => self.action(Action::Focus(Target::CatTerm(c)), now),
                    None => Vec::new(),
                },
            },
            Cmd::Back => self.action(Action::Back, now),
            Cmd::Walk => self.action(Action::Walk, now),
            Cmd::Band(b) => {
                if self.band != b {
                    self.band = b;
                    self.pending.push(Wish::world(Want::Once));
                }
                Vec::new()
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
                        let t = self.modal_child(s).unwrap_or(s);
                        self.action(Action::Neighbour(Target::Screen(t)), now)
                    }
                    None => Vec::new(),
                }
            }
            other => self.wm_command(other, now),
        }
    }

    /// Turn the machine's effects into camera moves and compositor outs;
    /// `was`: the FOCO target before the action (a cat's terminal keeps its
    /// keyboard leave for the cats).
    fn effects(&mut self, fx: &[Effect], now: Time, was: Option<Target>) -> Vec<Out> {
        let mut out = Vec::new();
        for e in fx {
            match *e {
                Effect::GlideOneToOne(t) | Effect::TravelOneToOne(t) => {
                    let lateral = matches!(e, Effect::TravelOneToOne(_));
                    self.focus_2d = None;
                    if let Some(o) = self.one_to_one_target(t) {
                        self.arrive = Some(t);
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
                Effect::KeyboardLeave if matches!(was, Some(Target::CatTerm(_))) => {
                    out.push(Out::Effect(*e));
                }
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
                Effect::GoTo(st) => self.go_to(st, now, false),
                Effect::GoToRoom(r) => {
                    let st = match self.room_index(r) {
                        Some(i) if !self.rooms[i].desks.is_empty() => Station::Desk {
                            room: r,
                            desk: 0,
                            slot: 0,
                        },
                        _ => Station::Door(r),
                    };
                    self.go_to(st, now, false);
                }
                Effect::GoToStand(p) => self.glide_free(p, now),
                Effect::Select(s) => {
                    if let Some(st) = self.station_of(s) {
                        self.go_to(st, now, false);
                    }
                }
                Effect::Object(id, a) => out.push(Out::Object(id, a)),
                other => out.push(Out::Effect(other)),
            }
        }
        self.pending.push(Wish::compose(Want::Once));
        out
    }

    /// The furniture of a room's desks as floor boxes: each desk and its
    /// chair (with the cat on it, if any).
    pub fn desk_obstacles(&self, room: usize) -> Vec<Aabb> {
        let s = self.slots()[room];
        let mut v = Vec::new();
        for d in 0..self.rooms[room].desks.len() {
            let x = self.desk_local_xs(room)[d];
            let w = self.screen_logical(room, d, 0).x * onetoone::M_PER_PX;
            let (desk, _) = layout::desk_root(&s, x);
            let half = Vec2::new(layout::desk_width(w) / 2.0, layout::DESK_DEPTH / 2.0);
            v.push(floor_box(desk, half));
            let (chair, _) = layout::chair_root(&s, x);
            v.push(floor_box(chair, Vec2::splat(0.25)));
        }
        v
    }

    /// The occupancy grid of a room: its floor, the door to the hall, the
    /// desks and chairs as boxes, and the plugins' extra obstacles.
    pub fn room_grid(&self, room: usize) -> Grid {
        let s = self.slots()[room];
        let (z0, z1, wall) = match s.side {
            Side::North => (-(HALL_W / 2.0 + ROOM_DEPTH), -HALL_W / 2.0, Wall::South),
            Side::South => (HALL_W / 2.0, HALL_W / 2.0 + ROOM_DEPTH, Wall::North),
        };
        let floor = Aabb::new(Vec2::new(s.x0, z0), Vec2::new(s.x0 + s.len, z1));
        let mut obstacles = self.desk_obstacles(room);
        if let Some(extra) = self.obstacles.get(&self.rooms[room].id) {
            obstacles.extend_from_slice(extra);
        }
        let r = self.walk_params.radius;
        Grid::room(floor, &[Gap::new(wall, s.door_x)], &obstacles, r)
    }

    /// The plugins' extra floor boxes for a room (`room_grid` adds them);
    /// a walk in that room gets the new grid at once.
    pub fn set_obstacles(&mut self, room: RoomId, extra: Vec<Aabb>) {
        if self
            .obstacles
            .get(&room)
            .map_or(extra.is_empty(), |o| *o == extra)
        {
            return;
        }
        if extra.is_empty() {
            self.obstacles.remove(&room);
        } else {
            self.obstacles.insert(room, extra);
        }
        let here = self.room_at(self.camera.pos).unwrap_or(self.current_room());
        if self.walking.is_some() && self.rooms.get(here).is_some_and(|r| r.id == room) {
            let grid = self.room_grid(here);
            if let Some(w) = self.walking.as_mut() {
                w.grid = grid;
            }
        }
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
                // one last world pass: the dimmed surroundings under the 2D
                // window (none when it covers the whole output)
                self.arrival_pass = !self.covers_output(s);
                return true;
            }
            self.pending.push(Wish::world(Want::Once));
        }
        false
    }

    /// A fullscreen window in FOCO covers the output: no world to see.
    pub fn covers_output(&self, t: Target) -> bool {
        match t {
            Target::Screen(s) => self.screen_state(s).fullscreen,
            Target::CatTerm(_) => false,
        }
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
            // (a window or a cat's terminal)
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

    /// Bring the rooms in line with the core's contexts (§6.9): the
    /// reception first, one room per context in order (kept by name, created
    /// when new), "minha sala" last; each context's desks follow its cats
    /// (`strip::reconcile`). A room whose context went away stays while it
    /// holds screens of yours (its cats leave), and goes otherwise. The
    /// station follows its desk; FOCO on a cat that left goes back to NAV.
    pub fn sync_contexts(&mut self, contexts: &[(String, Vec<CatId>)], now: Time) -> Vec<Out> {
        let anchor = self.anchor();
        let mut old = std::mem::take(&mut self.rooms);
        let mut take = |name: &str| {
            old.iter()
                .position(|r| r.name == name)
                .map(|i| old.remove(i))
        };
        let reception = take(RECEPTION);
        let mut rooms = vec![reception.unwrap_or_else(|| self.new_room(RECEPTION))];
        let mut seen: Vec<&str> = vec![RECEPTION, MY_ROOM];
        for (name, cats) in contexts {
            if seen.contains(&name.as_str()) {
                continue;
            }
            seen.push(name);
            let mut r = take(name).unwrap_or_else(|| self.new_room(name));
            r.desks = strip::reconcile(&r.desks, cats);
            self.kinds.insert(r.id, RoomKind::Context(name.clone()));
            rooms.push(r);
        }
        let mine = take(MY_ROOM);
        for mut r in old {
            r.desks = strip::reconcile(&r.desks, &[]);
            // the user's own rooms stay; a context's only for its screens
            let own = matches!(
                self.kinds.get(&r.id),
                Some(RoomKind::Free | RoomKind::Library)
            );
            if !r.desks.is_empty() || own {
                rooms.push(r);
            }
        }
        rooms.push(mine.unwrap_or_else(|| self.new_room(MY_ROOM)));
        self.rooms = rooms;

        let mut out = Vec::new();
        if let Some(Target::CatTerm(c)) = self.mode().focus()
            && self.cat_desk(c).is_none()
        {
            out = self.action(Action::WindowClosed, now);
        }
        self.station = self.resolve(anchor);
        // the strip may have moved under the camera: cut to where it belongs
        if !self.gliding() {
            let to = match self.focus_2d_target() {
                Some((_, o)) => Some(o.pose),
                None if self.mode().overlay.is_none() && !self.mode().walking() => {
                    self.station_pose(&self.station)
                }
                None => None,
            };
            if let Some(p) = to
                && p != self.camera
            {
                self.camera = p;
            }
        }
        if self.walking.is_some() {
            let here = self.room_at(self.camera.pos).unwrap_or(self.current_room());
            let grid = self.room_grid(here);
            if let Some(w) = self.walking.as_mut() {
                w.grid = grid;
            }
        }
        self.layout_gen += 1;
        self.pending.push(Wish::world(Want::Once));
        out
    }

    fn new_room(&mut self, name: &str) -> Room {
        let id = RoomId(self.next_room);
        self.next_room += 1;
        Room {
            id,
            name: name.into(),
            desks: Vec::new(),
        }
    }

    /// What the station is at, independent of desk indices.
    fn anchor(&self) -> Anchor {
        match self.station {
            Station::Desk { room, desk, slot } => {
                let d = self
                    .room_index(room)
                    .and_then(|r| self.rooms[r].desks.get(desk));
                let key = match d {
                    Some(Desk { cat: Some(c), .. }) => DeskKey::Cat(*c),
                    Some(d) if !d.screens.is_empty() => DeskKey::Screen(d.screens[0]),
                    _ => DeskKey::Index(desk),
                };
                Anchor::Desk { room, key, slot }
            }
            other => Anchor::Other(other),
        }
    }

    /// The station an anchor points at after the strip changed.
    fn resolve(&self, a: Anchor) -> Station {
        let first = |w: &World, r: usize| match w.rooms[r].desks.is_empty() {
            true => Station::Free(w.room_entry_pose(r)),
            false => Station::Desk {
                room: w.rooms[r].id,
                desk: 0,
                slot: 0,
            },
        };
        match a {
            Anchor::Desk { room, key, slot } => {
                let Some(r) = self.room_index(room) else {
                    return first(self, 0);
                };
                let desks = &self.rooms[r].desks;
                let at = match key {
                    DeskKey::Cat(c) => desks.iter().position(|d| d.cat == Some(c)),
                    DeskKey::Screen(s) => desks.iter().position(|d| d.screens.contains(&s)),
                    DeskKey::Index(i) => (!desks.is_empty()).then(|| i.min(desks.len() - 1)),
                };
                match at {
                    Some(desk) => Station::Desk {
                        room,
                        desk,
                        slot: slot.min(desks[desk].screens.len().max(1) - 1),
                    },
                    None => first(self, r),
                }
            }
            Anchor::Other(Station::Door(r)) if self.room_index(r).is_none() => first(self, 0),
            Anchor::Other(st) => st,
        }
    }

    /// The room and desk index of a cat's desk.
    pub fn cat_desk(&self, cat: CatId) -> Option<(RoomId, usize)> {
        self.rooms.iter().find_map(|r| {
            r.desks
                .iter()
                .position(|d| d.cat == Some(cat))
                .map(|i| (r.id, i))
        })
    }

    /// The NAV pose of a desk.
    pub fn desk_pose(&self, room: RoomId, desk: usize) -> Option<Pose> {
        self.station_pose(&Station::Desk {
            room,
            desk,
            slot: 0,
        })
    }

    /// Where the cat of a desk sits (on its chair, back to the camera).
    pub fn cat_seat(&self, room: RoomId, desk: usize) -> Option<Pose> {
        let r = self.room_index(room)?;
        self.rooms[r].desks.get(desk)?;
        Some(layout::cat_seat(
            &self.slots()[r],
            self.desk_local_xs(r)[desk],
        ))
    }

    /// A desk's own monitor (a cat's terminal): its place and the logical
    /// size it shows (the desk's one-screen size).
    pub fn monitor_place(&self, room: RoomId, desk: usize) -> Option<(ScreenPlace, Vec2)> {
        let r = self.room_index(room)?;
        let d = self.rooms[r].desks.get(desk)?;
        let logical = self.configure_size(d.width, 1);
        let place = layout::monitor_place(
            &self.slots()[r],
            self.desk_local_xs(r)[desk],
            logical * onetoone::M_PER_PX,
        );
        Some((place, logical))
    }

    pub fn room_slot(&self, room: RoomId) -> Option<RoomSlot> {
        self.room_index(room).map(|r| self.slots()[r])
    }

    /// The room's hall door: floor (x, z) and the yaw into the room.
    pub fn room_door(&self, room: RoomId) -> Option<(Vec2, f32)> {
        self.room_slot(room).map(|s| layout::door_world(&s))
    }

    pub fn current_room_id(&self) -> RoomId {
        self.rooms[self.current_room()].id
    }

    /// The rooms this frame draws: the current one and `extra` (what the
    /// plugins opened), once each, in strip order.
    pub fn visible_rooms(&self, extra: &[RoomId]) -> Vec<RoomId> {
        let cur = self.current_room_id();
        self.rooms
            .iter()
            .map(|r| r.id)
            .filter(|id| *id == cur || extra.contains(id))
            .collect()
    }

    /// The 1:1 pose of a FOCO target: a window's, or a cat terminal's on
    /// its desk's monitor.
    pub fn one_to_one_target(&self, t: Target) -> Option<OneToOne> {
        match t {
            Target::Screen(s) => self.one_to_one(s),
            Target::CatTerm(c) => {
                let (room, desk) = self.cat_desk(c)?;
                let (place, logical) = self.monitor_place(room, desk)?;
                Some(onetoone::pose(&self.out, FOV_Y, logical, &place))
            }
        }
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
                kind: self.room_kind(r.id).label(),
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
                    let st = self.screen_state(*sid);
                    screens.push(DumpScreen {
                        id: s.id.0,
                        app_id: s.app_id.clone(),
                        title: s.title.clone(),
                        size: [s.size.x, s.size.y],
                        parent: st.parent.map(|p| p.0),
                        modal: st.modal,
                        fullscreen: st.fullscreen,
                        maximized: st.maximized,
                        air: st.air.is_some(),
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
            station: self.station,
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

/// A desk, whatever its index.
#[derive(Clone, Copy, Debug, PartialEq)]
enum DeskKey {
    Cat(CatId),
    Screen(ScreenId),
    Index(usize),
}

/// What the station is at, across a change of the strip.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Anchor {
    Desk {
        room: RoomId,
        key: DeskKey,
        slot: usize,
    },
    Other(Station),
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
    station: Station,
    camera: DumpCamera,
    rooms: Vec<DumpRoom>,
    desks: Vec<DumpDesk>,
    screens: Vec<DumpScreen>,
    focus: Option<DumpFocus>,
    wants: Vec<String>,
}

/// A floor box (x, z) of half size `half` around `c`.
fn floor_box(c: Vec3, half: Vec2) -> Aabb {
    let c = Vec2::new(c.x, c.z);
    Aabb::new(c - half, c + half)
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
    kind: &'static str,
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
    parent: Option<u32>,
    modal: bool,
    fullscreen: bool,
    maximized: bool,
    air: bool,
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
    fn focus_on_a_desk_without_screens_is_its_cats_terminal_or_nothing() {
        let rooms = vec![Room {
            id: RoomId(0),
            name: "acme".into(),
            desks: vec![desk(&[], None), desk(&[], Some(1))],
        }];
        let mut w = World::with_rooms(OUT, OPTS, rooms);
        assert!(w.command(Cmd::Focus, 0.0).is_empty(), "no screen, no cat");
        assert_eq!(w.mode().label(), "NAVEGAR");
        w.command(Cmd::Move(Dir::Right), 0.0);
        let out = w.command(Cmd::Focus, 0.0);
        assert!(
            out.contains(&Out::Effect(Effect::OpenPane(CatId(1)))),
            "{out:?}"
        );
        assert_eq!(w.mode().label(), "FOCO");
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

    #[test]
    fn walking_mode_takes_esc_and_world_w() {
        let mut w = three();
        w.command(Cmd::Walk, 0.0);
        assert!(w.mode().walking());
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
    fn walking_forward_stops_at_the_chair() {
        let mut w = three();
        let mut now = 0.0;
        w.command(Cmd::Walk, now);
        assert!(w.walking.is_some());
        let start = w.camera;
        walk(&mut w, &mut now, &[sym::W], 3.0);
        // the chair's box in front of the desk (north room: +z is the room's front)
        let (chair, _) = layout::chair_root(&w.slots()[0], w.desk_x(0, 0));
        let chair_front = chair.z + 0.25;
        assert!(w.camera.pos.z < start.pos.z - 0.8, "moved forward");
        assert!(
            w.camera.pos.z >= chair_front + w.walk_params.radius - 0.26,
            "{} vs chair front {chair_front}",
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
        w.station = Station::Free(Pose::new(Vec3::new(1.0, 1.5, 2.0), 0.5, 0.0));
        let d = w.dump(&[]);
        assert_eq!(
            d["station"]["free"]["pos"],
            serde_json::json!([1.0, 1.5, 2.0])
        );
        assert_eq!(d["station"]["free"]["yaw"], 0.5);
    }

    const TALK: Action = Action::Talk {
        cat: CatId(9),
        purpose: TalkPurpose::Session,
    };

    #[test]
    fn dialogue_effects_pass_through() {
        let mut w = three();
        let out = w.action(TALK, 0.0);
        assert!(
            out.contains(&Out::Effect(Effect::FrameFace(CatId(9)))),
            "{out:?}"
        );
        assert!(
            out.contains(&Out::Effect(Effect::CatTurns(CatId(9)))),
            "{out:?}"
        );
        assert_eq!(w.purpose(), TalkPurpose::Session);
        let out = w.action(Action::Reply, 0.0);
        assert_eq!(out, [Out::Effect(Effect::OpenLine)]);
        let out = w.action(Action::Send, 0.0);
        assert_eq!(
            out,
            [
                Out::Effect(Effect::CloseLine),
                Out::Effect(Effect::SendLine)
            ]
        );
        let out = w.action(Action::Back, 0.0);
        assert!(out.contains(&Out::Effect(Effect::CatBackToWork(CatId(9)))));
        assert_eq!(
            w.purpose(),
            TalkPurpose::Session,
            "still about the talk it left"
        );
        // the vault's dialogue keeps its purpose for the routing
        w.action(
            Action::Talk {
                cat: CatId(9),
                purpose: TalkPurpose::VaultUnlock,
            },
            0.0,
        );
        assert_eq!(w.purpose(), TalkPurpose::VaultUnlock);
        w.action(Action::Back, 0.0);
        assert_eq!(w.purpose(), TalkPurpose::VaultUnlock);
        w.action(Action::Walk, 0.0);
        assert_eq!(
            w.purpose(),
            TalkPurpose::Session,
            "no talk: back to the default"
        );
    }

    #[test]
    fn objects_come_out_as_outs() {
        let mut w = three();
        let id = ObjectId(4);
        assert_eq!(
            w.action(Action::Interact(id), 0.0),
            [Out::Object(id, ObjectAction::Interact)]
        );
        w.command(Cmd::Walk, 0.0);
        assert_eq!(
            w.action(Action::Adjust(id, -1), 0.0),
            [Out::Object(id, ObjectAction::Adjust(-1))]
        );
        w.command(Cmd::Focus, 0.0);
        assert!(
            w.action(Action::Interact(id), 0.0).is_empty(),
            "not under FOCO"
        );
    }

    #[test]
    fn picking_an_object_in_nav_goes_to_its_stand() {
        let mut w = three();
        let mut now = 0.0;
        let stand = layout::stand_before(Vec3::new(2.0, 1.0, -4.0), 0.0, 1.2);
        w.action(Action::Hint { focus: false }, now);
        let out = w.action(
            Action::Pick(nkw_core::Pick::Object {
                id: ObjectId(1),
                stand,
            }),
            now,
        );
        assert_eq!(out, [Out::Effect(Effect::HideHints)]);
        assert_eq!(w.station, Station::Free(stand));
        settle(&mut w, &mut now);
        assert_eq!(w.camera, stand);
        w.glide_free(w.spots(0)[1].pose, now);
        settle(&mut w, &mut now);
        assert_eq!(w.camera, w.spots(0)[1].pose);
    }

    #[test]
    fn go_to_room_lands_on_its_first_desk() {
        let rooms = (0..2)
            .map(|i| Room {
                id: RoomId(i),
                name: format!("r{i}"),
                desks: if i == 1 {
                    vec![desk(&[], None), desk(&[], None)]
                } else {
                    vec![]
                },
            })
            .collect();
        let mut w = World::with_rooms(OUT, OPTS, rooms);
        w.action(Action::Hint { focus: false }, 0.0);
        w.action(Action::Pick(nkw_core::Pick::Door(RoomId(1))), 0.0);
        assert_eq!(
            w.station,
            Station::Desk {
                room: RoomId(1),
                desk: 0,
                slot: 0
            }
        );
    }

    #[test]
    fn a_cats_terminal_in_focus() {
        let mut w = three();
        let mut now = 0.0;
        let t = Target::CatTerm(CatId(9));
        let out = w.action(Action::Focus(t), now);
        assert!(
            out.contains(&Out::Effect(Effect::OpenPane(CatId(9)))),
            "{out:?}"
        );
        assert!(
            out.contains(&Out::Effect(Effect::KeyboardEnter(t))),
            "{out:?}"
        );
        assert!(!out.iter().any(|o| matches!(o, Out::KeyboardEnter(_))));
        settle(&mut w, &mut now);
        let (got, o) = w.focus_2d_target().unwrap();
        assert_eq!(got, t);
        assert_eq!(w.camera, o.pose);
        assert!(w.focus_2d().is_none(), "not a window");
        let out = w.action(Action::Back, now);
        assert!(out.contains(&Out::Effect(Effect::KeyboardLeave)), "{out:?}");
        assert!(
            !out.contains(&Out::KeyboardLeave),
            "the cats' leave, not a client's"
        );
        assert!(w.focus_2d_target().is_none());
    }

    /// Project a world point to physical px (y down) like the 1:1 tests.
    fn project(p: &Pose, out: &Output, world: Vec3) -> Vec2 {
        use glam::{DVec3, DVec4};
        let aspect = out.width_px as f64 / out.height_px as f64;
        let proj = glam::dcamera::rh::proj::opengl::perspective(FOV_Y as f64, aspect, 0.05, 100.0);
        let view = glam::dcamera::rh::view::look_to_mat4(
            p.pos.as_dvec3(),
            p.forward().as_dvec3(),
            DVec3::Y,
        );
        let c = proj * view * DVec4::from((world.as_dvec3(), 1.0));
        let n = c.truncate() / c.w;
        Vec2::new(
            ((n.x + 1.0) / 2.0 * out.width_px as f64) as f32,
            ((1.0 - n.y) / 2.0 * out.height_px as f64) as f32,
        )
    }

    #[test]
    fn a_cats_terminal_lines_up_1_to_1_like_a_screen() {
        for out in [
            OUT,
            Output {
                width_px: 1920,
                height_px: 1080,
                scale: 1.25,
            },
        ] {
            let mut w = three();
            w.out = out;
            let (room, d) = w.cat_desk(CatId(9)).unwrap();
            assert_eq!((room, d), (RoomId(0), 1));
            let (place, logical) = w.monitor_place(room, d).unwrap();
            assert_eq!(logical, w.configure_size(Width::Half, 1));
            let o = w.one_to_one_target(Target::CatTerm(CatId(9))).unwrap();
            let pose = Pose::new(place.center, place.yaw, place.pitch);
            let (r, u) = (pose.right(), pose.up());
            let half = logical * onetoone::M_PER_PX / 2.0;
            let tl = place.center - r * half.x + u * half.y;
            let br = place.center + r * half.x - u * half.y;
            assert!((project(&o.pose, &out, tl) - o.origin).abs().max_element() < 0.01);
            assert!(
                (project(&o.pose, &out, br) - (o.origin + o.size))
                    .abs()
                    .max_element()
                    < 0.01
            );
            assert!(w.one_to_one_target(Target::CatTerm(CatId(77))).is_none());
        }
    }

    #[test]
    fn desk_poses_seats_doors_and_slots() {
        let w = three();
        let r = RoomId(0);
        assert_eq!(w.desk_pose(r, 2), Some(w.spots(0)[2].pose));
        assert_eq!(w.desk_pose(r, 3), None);
        assert_eq!(w.desk_pose(RoomId(5), 0), None);
        let seat = w.cat_seat(r, 1).unwrap();
        assert!((seat.pos.x - w.spots(0)[1].pose.pos.x).abs() < 1e-5);
        assert!((seat.pos.y - layout::SEAT_H).abs() < 1e-6);
        let slot = w.room_slot(r).unwrap();
        assert_eq!(slot, w.slots()[0]);
        assert_eq!(w.room_door(r), Some(layout::door_world(&slot)));
        assert_eq!(w.current_room_id(), r);
        assert_eq!(w.cat_desk(CatId(1)), None);
    }

    #[test]
    fn visible_rooms_in_strip_order() {
        let rooms = (0..3)
            .map(|i| Room {
                id: RoomId(10 - i),
                name: format!("r{i}"),
                desks: vec![],
            })
            .collect();
        let w = World::with_rooms(OUT, OPTS, rooms);
        assert_eq!(w.current_room_id(), RoomId(10));
        assert_eq!(w.visible_rooms(&[]), [RoomId(10)]);
        assert_eq!(
            w.visible_rooms(&[RoomId(8), RoomId(10), RoomId(8), RoomId(99)]),
            [RoomId(10), RoomId(8)]
        );
    }

    #[test]
    fn extra_obstacles_block_the_walk() {
        let mut w = three();
        let mut now = 0.0;
        w.command(Cmd::Walk, now);
        let start = w.camera.pos;
        // a wall of a box right in front
        let wall = Aabb::new(
            Vec2::new(start.x - 1.0, start.z - 0.6),
            Vec2::new(start.x + 1.0, start.z - 0.5),
        );
        w.set_obstacles(RoomId(0), vec![wall]);
        let g = w.room_grid(0);
        assert!(g != three().room_grid(0), "the grid has the box");
        walk(&mut w, &mut now, &[sym::W], 1.5);
        assert!(
            w.camera.pos.z > start.z - 0.5,
            "{} stopped by the box",
            w.camera.pos.z
        );
        w.set_obstacles(RoomId(0), vec![]);
        walk(&mut w, &mut now, &[sym::W], 1.5);
        assert!(w.camera.pos.z < start.z - 0.6, "free again");
    }

    #[test]
    fn band_is_a_command() {
        let mut w = three();
        assert_eq!(w.band, DEFAULT_BAND);
        w.frame_done(0.0);
        let args = BTreeMap::from([("band".to_string(), "6".to_string())]);
        let c = Cmd::from_name("world.band", &args).unwrap();
        assert_eq!(c, Cmd::Band(6));
        w.command(c, 0.0);
        assert_eq!(w.band, 6);
        assert!(!w.wishes().is_empty(), "a new light wants a pass");
        let bad = BTreeMap::from([("band".to_string(), "9".to_string())]);
        assert_eq!(Cmd::from_name("world.band", &bad), None);
        assert_eq!(Cmd::from_name("world.band", &BTreeMap::new()), None);
    }

    #[test]
    fn command_names_round_trip() {
        let all = [
            Cmd::Move(Dir::Left),
            Cmd::Move(Dir::Right),
            Cmd::Move(Dir::Up),
            Cmd::Move(Dir::Down),
            Cmd::RoomNext,
            Cmd::RoomPrev,
            Cmd::Focus,
            Cmd::Back,
            Cmd::Walk,
            Cmd::Neighbour(Dir::Left),
            Cmd::Neighbour(Dir::Right),
            Cmd::Neighbour(Dir::Up),
            Cmd::Neighbour(Dir::Down),
            Cmd::Band(2),
        ];
        for c in all {
            let (n, a) = c.name();
            assert_eq!(Cmd::from_name(n, &a), Some(c), "{n}");
        }
        assert_eq!(
            Cmd::from_name("world.walk_exit", &BTreeMap::new()),
            Some(Cmd::Back)
        );
        assert_eq!(
            Cmd::from_name("world.summon", &BTreeMap::new()),
            None,
            "the cats'"
        );
    }

    fn names(w: &World) -> Vec<String> {
        w.rooms.iter().map(|r| r.name.clone()).collect()
    }

    #[test]
    fn sync_contexts_builds_the_rooms() {
        let mut w = World::new(OUT, OPTS);
        let (mine, _) = w.add_screen("acme.term", "", 0.0);
        let ctx = |v: &[(&str, &[u32])]| -> Vec<(String, Vec<CatId>)> {
            v.iter()
                .map(|(n, c)| (n.to_string(), c.iter().map(|&i| CatId(i)).collect()))
                .collect()
        };
        w.sync_contexts(&ctx(&[("acme", &[1, 2]), ("demo", &[3])]), 0.0);
        assert_eq!(names(&w), [RECEPTION, "acme", "demo", MY_ROOM]);
        assert_eq!(w.cat_desk(CatId(2)).map(|d| d.1), Some(1));
        assert_eq!(
            w.locate(mine).map(|l| w.rooms[l.0].name.clone()),
            Some(MY_ROOM.into())
        );
        let ids: Vec<RoomId> = w.rooms.iter().map(|r| r.id).collect();
        assert_eq!(ids.len(), 4);
        assert_eq!(ids[3], RoomId(0), "minha sala kept its id");
        // the station followed its desk into the last room
        assert_eq!(w.selected(), Some(mine));
        assert_eq!(
            w.camera,
            w.station_pose(&w.station).unwrap(),
            "cut to the new place"
        );

        // kept by name, in the new order; a cat leaves, a context goes
        let acme = w.rooms[1].id;
        w.rooms[1].desks[0].screens.push(ScreenId(50));
        w.screens.insert(
            ScreenId(50),
            Screen {
                id: ScreenId(50),
                app_id: "acme.log".into(),
                title: String::new(),
                size: Vec2::ZERO,
            },
        );
        w.sync_contexts(&ctx(&[("demo", &[3, 4]), ("bravo", &[])]), 0.0);
        assert_eq!(
            names(&w),
            [RECEPTION, "demo", "bravo", "acme", MY_ROOM],
            "acme stays for the screen left there"
        );
        assert_eq!(w.rooms[3].id, acme);
        assert_eq!(w.rooms[3].desks.len(), 1, "only the desk with a screen");
        assert_eq!(w.rooms[3].desks[0].cat, None);
        assert_eq!(w.cat_desk(CatId(4)).map(|d| d.1), Some(1));
        assert!(w.rooms[2].desks.is_empty());

        // idempotent
        let before = w.rooms.clone();
        w.sync_contexts(&ctx(&[("demo", &[3, 4]), ("bravo", &[])]), 0.0);
        assert_eq!(w.rooms, before);

        // an empty vanished context just goes
        w.sync_contexts(&ctx(&[("demo", &[3, 4])]), 0.0);
        assert_eq!(names(&w), [RECEPTION, "demo", "acme", MY_ROOM]);
    }

    #[test]
    fn sync_contexts_ends_focus_on_a_cat_that_left() {
        let mut w = World::new(OUT, OPTS);
        let one = vec![("acme".to_string(), vec![CatId(1)])];
        w.sync_contexts(&one, 0.0);
        let mut now = 0.0;
        w.action(Action::Focus(Target::CatTerm(CatId(1))), now);
        settle(&mut w, &mut now);
        assert_eq!(w.mode().label(), "FOCO");
        let out = w.sync_contexts(&[("acme".to_string(), vec![])], now);
        assert_eq!(w.mode().label(), "NAVEGAR");
        assert!(out.contains(&Out::Effect(Effect::KeyboardLeave)), "{out:?}");
    }
}
