//! The window manager's operations on the [`World`] (§4.4–§4.6, phase 1):
//! the strip ops and presets, stacks taller than three, air screens,
//! dialogs on the mother's desk, fullscreen as a TV, maximize, activation,
//! placement of new windows and the snapshot for `world.json`.
//!
//! A child module of `world` (so it reaches the World's private state);
//! pure like it. Screens that are not part of a stack (dialogs, screens in
//! the air) stay in their desk's `screens`, *after* the stack ("loose"), so
//! the scene draws every screen of a desk through `World::screen_place`.

use glam::{Vec2, Vec3};
use nkw_core::layout;
use nkw_core::persist::{
    DeskAt, DeskRec, Place, RoomKind, RoomRec, ScreenRec, StationRec, VERSION, WorldFile,
};
use nkw_core::strip::{self, Desk, Dir, Station, Width};
use nkw_core::{
    Action, Kind, Pose, RoomId, ScreenId, ScreenPlace, Target, Time, Want, Wish, onetoone,
};

use super::{Cmd, MY_ROOM, Out, RECEPTION, World};

/// How far a dialog floats in front of its mother (metres).
pub const DIALOG_FRONT: f32 = 0.05;
/// A dialog's size before its first buffer (logical px).
pub const DIALOG_DEFAULT: Vec2 = Vec2::new(480.0, 320.0);
/// Each screen of a pile (stacks over three) sits this much further back.
pub const PILE_STEP: f32 = 0.04;
/// ... and this much higher, so its top edge shows.
pub const PILE_RISE: f32 = 0.015;
/// The pile never goes deeper than this (the desk ends at the wall).
pub const PILE_MAX: f32 = 0.28;
/// "quer atenção" stays this long on a screen that asked without a token.
pub const ATTENTION_S: f64 = 6.0;
/// The layout badge stays this long after a change.
pub const LAYOUT_NOTE_S: f64 = 3.0;
/// The longest room name a command carries (bytes).
pub const ROOM_NAME_MAX: usize = 47;

/// A room name inside a `Copy` command: at most [`ROOM_NAME_MAX`] bytes,
/// cut at a char boundary.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct ShortName {
    len: u8,
    buf: [u8; ROOM_NAME_MAX],
}

impl ShortName {
    pub fn new(s: &str) -> ShortName {
        let mut end = s.len().min(ROOM_NAME_MAX);
        while !s.is_char_boundary(end) {
            end -= 1;
        }
        let mut buf = [0u8; ROOM_NAME_MAX];
        buf[..end].copy_from_slice(&s.as_bytes()[..end]);
        ShortName {
            len: end as u8,
            buf,
        }
    }

    pub fn as_str(&self) -> &str {
        std::str::from_utf8(&self.buf[..self.len as usize]).unwrap_or("")
    }
}

impl std::fmt::Debug for ShortName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.as_str())
    }
}

/// A room as a command argument: its id (a number, as in `dump`) or its name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoomRef {
    Id(RoomId),
    Name(ShortName),
}

impl RoomRef {
    pub fn parse(s: &str) -> Option<RoomRef> {
        let s = s.trim();
        if s.is_empty() {
            return None;
        }
        Some(match s.parse::<u32>() {
            Ok(n) => RoomRef::Id(RoomId(n)),
            Err(_) => RoomRef::Name(ShortName::new(s)),
        })
    }

    /// The argument's text (the inverse of `parse`).
    pub fn arg(&self) -> String {
        match self {
            RoomRef::Id(r) => r.0.to_string(),
            RoomRef::Name(n) => n.as_str().to_string(),
        }
    }
}

/// A walking key held through a command (`world.forward` …); released by
/// the key's release (`World::walk_key`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hold {
    Forward,
    Back,
    StrafeLeft,
    StrafeRight,
    TurnLeft,
    TurnRight,
    LookUp,
    LookDown,
}

/// What the WM knows of a screen beyond [`super::Screen`].
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ScreenState {
    /// `xdg_toplevel.set_parent`: a dialog of this screen
    pub parent: Option<ScreenId>,
    /// `xdg_dialog_v1.set_modal`: blocks the mother's input
    pub modal: bool,
    /// pinned in the air here (`world.pin`)
    pub air: Option<ScreenPlace>,
    pub fullscreen: bool,
    pub maximized: bool,
    /// the desk width before `world.maximize`
    pub restore: Option<Width>,
    /// "quer atenção" until then (activation without a valid token)
    pub attention: Option<Time>,
    /// its app_id arrived and the rules/memory placed it
    pub placed: bool,
}

/// One `world.*` command the World runs: name, title (pt-BR), the S9 keys,
/// its args (name, prompt), whether the palette offers it.
pub struct CmdInfo {
    pub name: &'static str,
    pub title: &'static str,
    pub keys: &'static [&'static str],
    pub args: &'static [(&'static str, &'static str)],
    pub palette: bool,
}

const fn c(name: &'static str, title: &'static str, keys: &'static [&'static str]) -> CmdInfo {
    CmdInfo {
        name,
        title,
        keys,
        args: &[],
        palette: true,
    }
}

/// Only in its own mode (walking, CONVERSA, SALTO …): not in the palette.
const fn m(name: &'static str, title: &'static str, keys: &'static [&'static str]) -> CmdInfo {
    CmdInfo {
        name,
        title,
        keys,
        args: &[],
        palette: false,
    }
}

const ROOM_ARG: &[(&str, &str)] = &[("room", "sala")];

/// Every S9 command the World runs (contract S9; titles verbatim).
pub const COMMANDS: &[CmdInfo] = &[
    c("world.left", "estação anterior", &["h", "left"]),
    c("world.right", "próxima estação", &["l", "right"]),
    c("world.down", "tela de baixo", &["j", "down"]),
    c("world.up", "tela de cima", &["k", "up"]),
    c("world.room_next", "próxima sala", &["J", "world+shift+j"]),
    c("world.room_prev", "sala anterior", &["K", "world+shift+k"]),
    c("world.room_1", "sala 1", &["1"]),
    c("world.room_2", "sala 2", &["2"]),
    c("world.room_3", "sala 3", &["3"]),
    c("world.room_4", "sala 4", &["4"]),
    c("world.room_5", "sala 5", &["5"]),
    c("world.room_6", "sala 6", &["6"]),
    c("world.room_7", "sala 7", &["7"]),
    c("world.room_8", "sala 8", &["8"]),
    c("world.room_9", "sala 9", &["9"]),
    c("world.first", "primeira mesa", &["gg"]),
    c("world.last", "última mesa", &["G"]),
    c(
        "world.focus",
        "FOCO na tela (ou no terminal do gato)",
        &["enter", "i"],
    ),
    c("world.talk", "conversa com o gato", &["t"]),
    c("world.walk", "ANDAR (e volta)", &["w", "world+w"]),
    c("world.hint", "SALTO", &["f", "world+f"]),
    c("world.hint_focus", "SALTO direto para o FOCO", &["F"]),
    c("world.move_left", "tela para a mesa à esquerda", &["H"]),
    c("world.move_right", "tela para a mesa à direita", &["L"]),
    c("world.consume", "junta na pilha à esquerda", &[","]),
    c("world.expel", "tira para uma mesa só dela", &["."]),
    c("world.narrower", "mesa mais estreita", &["-"]),
    c("world.wider", "mesa mais larga", &["="]),
    c("world.pin", "prende a tela no ar", &["p"]),
    CmdInfo {
        name: "world.send_room",
        title: "manda a tela para outra sala",
        keys: &["M"],
        args: ROOM_ARG,
        palette: true,
    },
    c("world.close", "fecha a janela", &["Q", "world+q"]),
    m("world.palette", "COMANDOS", &[":", "ctrl+p", "world+p"]),
    c("world.fullscreen", "tela cheia (TV)", &[]),
    c("world.maximize", "maximiza (largura 1)", &[]),
    CmdInfo {
        name: "world.room_new",
        title: "sala nova",
        keys: &[],
        args: &[("name", "nome da sala")],
        palette: true,
    },
    CmdInfo {
        name: "world.goto_room",
        title: "ir para a sala",
        keys: &[],
        args: ROOM_ARG,
        palette: true,
    },
    c("world.focus_left", "FOCO na vizinha", &["world+h"]),
    c("world.focus_right", "FOCO na vizinha", &["world+l"]),
    c("world.focus_up", "FOCO na vizinha", &["world+k"]),
    c("world.focus_down", "FOCO na vizinha", &["world+j"]),
    m("world.forward", "andar", &["w"]),
    m("world.back", "sai do FOCO", &["s", "ctrl+]"]),
    m("world.strafe_left", "andar", &["a"]),
    m("world.strafe_right", "andar", &["d"]),
    m("world.turn_left", "girar", &["q", "left"]),
    m("world.turn_right", "girar", &["e", "right"]),
    m("world.look_up", "olhar", &["up"]),
    m("world.look_down", "olhar", &["down"]),
    m("world.recenter", "horizonte", &["c"]),
    m("world.walk_exit", "volta ao NAV", &["esc"]),
    m("world.talk_next", "avança", &["space", "enter"]),
    m("world.talk_reply", "responder (linha)", &["i", "r"]),
    m("world.talk_sit", "FOCO no terminal do gato", &["f"]),
    m("world.talk_end", "encerra", &["esc", "q"]),
    m("world.hint_cancel", "cancela", &["esc"]),
    m("world.palette_cancel", "paleta", &["esc"]),
];

/// `(name, title)` of every [`COMMANDS`] row, in order.
pub const NAMES: [(&str, &str); COMMANDS.len()] = {
    let mut out = [("", ""); COMMANDS.len()];
    let mut i = 0;
    while i < COMMANDS.len() {
        out[i] = (COMMANDS[i].name, COMMANDS[i].title);
        i += 1;
    }
    out
};

/// The info row of a command name.
pub fn info(name: &str) -> Option<&'static CmdInfo> {
    COMMANDS.iter().find(|c| c.name == name)
}

/// The pt-BR name of a width preset (the layout badge).
pub fn width_label(w: Width) -> &'static str {
    match w {
        Width::Third => "1/3",
        Width::Half => "1/2",
        Width::TwoThirds => "2/3",
        Width::Full => "inteira",
    }
}

impl World {
    // ---------------------------------------------------------------
    // reading

    /// The WM state of a screen (default when it has none).
    pub fn screen_state(&self, id: ScreenId) -> ScreenState {
        self.wm.get(&id).copied().unwrap_or_default()
    }

    /// What a room is: its recorded kind, else from its name.
    pub fn room_kind(&self, id: RoomId) -> RoomKind {
        if let Some(k) = self.kinds.get(&id) {
            return k.clone();
        }
        let name = self
            .rooms
            .iter()
            .find(|r| r.id == id)
            .map(|r| r.name.as_str())
            .unwrap_or("");
        RoomKind::from_name(name, RECEPTION, MY_ROOM)
    }

    /// Counts every change of rooms, desks, screens and the station (for
    /// saving `world.json`).
    pub fn generation(&self) -> u64 {
        self.layout_gen
    }

    /// The layout badge's text while it shows, and until when.
    pub fn layout_note(&self, now: Time) -> Option<(&str, Time)> {
        let (t, s) = self.note.as_ref()?;
        let until = t + LAYOUT_NOTE_S;
        (now < until).then_some((s.as_str(), until))
    }

    /// Screens that asked for attention, and until when.
    pub fn attention(&self, now: Time) -> Vec<(ScreenId, Time)> {
        self.wm
            .iter()
            .filter_map(|(id, s)| s.attention.filter(|t| *t > now).map(|t| (*id, t)))
            .collect()
    }

    /// A dialog or a screen in the air: not part of its desk's stack.
    pub fn is_loose(&self, id: ScreenId) -> bool {
        self.wm
            .get(&id)
            .is_some_and(|s| s.parent.is_some() || s.air.is_some())
    }

    /// How many screens of a desk form its stack (loose ones excluded).
    pub fn stack_len(&self, room: usize, desk: usize) -> usize {
        self.rooms[room].desks[desk]
            .screens
            .iter()
            .filter(|s| !self.is_loose(**s))
            .count()
    }

    /// The deepest modal dialog over `id`, if any (it takes the keyboard).
    pub fn modal_child(&self, id: ScreenId) -> Option<ScreenId> {
        let mut cur = id;
        let mut found = None;
        for _ in 0..8 {
            let next = self
                .wm
                .iter()
                .filter(|(c, s)| s.parent == Some(cur) && s.modal && self.screens.contains_key(c))
                .map(|(c, _)| *c)
                .max();
            match next {
                Some(c) => {
                    found = Some(c);
                    cur = c;
                }
                None => break,
            }
        }
        found
    }

    /// The station a screen is seen from.
    pub fn station_of(&self, id: ScreenId) -> Option<Station> {
        let st = self.screen_state(id);
        if st.air.is_some() {
            return Some(Station::Air(id));
        }
        if let Some(p) = st.parent
            && p != id
            && !self.screen_state(p).parent.is_some_and(|g| g == id)
        {
            return self.station_of(p);
        }
        let (r, d, k) = self.locate(id)?;
        Some(Station::Desk {
            room: self.rooms[r].id,
            desk: d,
            slot: k,
        })
    }

    /// The room index of a `RoomRef`: by id, else by name (exact, then
    /// case-insensitive, then prefix).
    pub fn find_room(&self, r: &RoomRef) -> Option<usize> {
        match r {
            RoomRef::Id(id) => self.room_index(*id),
            RoomRef::Name(n) => {
                let n = n.as_str();
                let low = n.to_lowercase();
                self.rooms
                    .iter()
                    .position(|r| r.name == n)
                    .or_else(|| self.rooms.iter().position(|r| r.name.to_lowercase() == low))
                    .or_else(|| {
                        self.rooms
                            .iter()
                            .position(|r| r.name.to_lowercase().starts_with(&low))
                    })
            }
        }
    }

    /// Where a screen is now, as a place (the place memory).
    pub fn place_of(&self, id: ScreenId) -> Option<Place> {
        let (r, d, _) = self.locate(id)?;
        Some(Place {
            room: Some(self.rooms[r].name.clone()),
            desk: Some(DeskAt::Index(d)),
            width: Some(self.rooms[r].desks[d].width),
        })
    }

    /// The world as `world.json` keeps it (no place memory: the WM merges it).
    pub fn snapshot(&self) -> WorldFile {
        let rooms = self
            .rooms
            .iter()
            .map(|r| RoomRec {
                id: r.id.0,
                name: r.name.clone(),
                kind: self.room_kind(r.id),
            })
            .collect();
        let mut desks = Vec::new();
        let mut screens = Vec::new();
        for (ri, r) in self.rooms.iter().enumerate() {
            for (di, d) in r.desks.iter().enumerate() {
                let n = self.stack_len(ri, di);
                let apps = d.screens[..n]
                    .iter()
                    .filter_map(|s| self.screens.get(s))
                    .map(|s| s.app_id.clone())
                    .collect();
                desks.push(DeskRec {
                    room: r.name.clone(),
                    width: d.width,
                    screens: apps,
                    cat: d.cat.is_some(),
                });
                for s in &d.screens {
                    let Some(sc) = self.screens.get(s) else {
                        continue;
                    };
                    if sc.app_id.is_empty() || self.screen_state(*s).parent.is_some() {
                        continue;
                    }
                    if let Some(place) = self.place_of(*s) {
                        screens.push(ScreenRec {
                            app_id: sc.app_id.clone(),
                            place,
                        });
                    }
                }
            }
        }
        let name = |id: RoomId| self.room_index(id).map(|i| self.rooms[i].name.clone());
        let station = match self.station {
            Station::Desk { room, desk, slot } => {
                name(room).map(|room| StationRec::Desk { room, desk, slot })
            }
            Station::Door(r) => name(r).map(|room| StationRec::Door { room }),
            Station::Air(s) => self.locate(s).map(|(r, d, _)| StationRec::Desk {
                room: self.rooms[r].name.clone(),
                desk: d,
                slot: 0,
            }),
            _ => None,
        };
        WorldFile {
            version: VERSION,
            rooms,
            desks,
            screens,
            station,
            places: Default::default(),
        }
    }

    // ---------------------------------------------------------------
    // geometry

    /// Where screen `k` of a desk hangs: the stack (up to three in front,
    /// the rest piled behind), a dialog in front of its mother, or its
    /// place in the air.
    pub(super) fn place_at(&self, room: usize, desk: usize, k: usize) -> ScreenPlace {
        self.place_depth(room, desk, k, 0)
    }

    fn place_depth(&self, room: usize, desk: usize, k: usize, depth: u8) -> ScreenPlace {
        let s = self.slots()[room];
        let x = self.desk_local_xs(room)[desk];
        let d = &self.rooms[room].desks[desk];
        // (a slot past the screens: where the next one would hang)
        let st = d
            .screens
            .get(k)
            .map(|id| self.screen_state(*id))
            .unwrap_or_default();
        if let Some(p) = st.air {
            return p;
        }
        if st.parent.is_some() {
            let mother = st
                .parent
                .filter(|_| depth < 4)
                .and_then(|m| self.locate(m))
                .map(|(r, dd, kk)| self.place_depth(r, dd, kk, depth + 1));
            let size = self.screen_logical(room, desk, k) * onetoone::M_PER_PX;
            let base = mother.unwrap_or_else(|| layout::monitor_place(&s, x, size));
            return in_front(&base, DIALOG_FRONT);
        }
        let n = self.stack_len(room, desk);
        let size = self.screen_logical(room, desk, k) * onetoone::M_PER_PX;
        let start = self.view_start(room, desk, n);
        let shown = n.min(strip::STACK_VISIBLE);
        if k >= n {
            return layout::screen_place(&s, x, n, k, size);
        }
        if k >= start && k < start + shown {
            return layout::screen_place(&s, x, n, k - start, size);
        }
        // the pile: behind the nearest end of what shows
        let (edge, behind) = if k < start {
            (0, start - k)
        } else {
            (shown - 1, k - (start + shown - 1))
        };
        let mut p = layout::screen_place(&s, x, n, edge, size);
        let back = layout::facing(layout::room_frame(&s).yaw);
        let deep = (PILE_STEP * behind as f32).min(PILE_MAX);
        p.center += back * deep + Vec3::Y * (PILE_RISE * behind as f32);
        p
    }

    /// The first stack slot shown in front: the selected one stays in view.
    fn view_start(&self, room: usize, desk: usize, n: usize) -> usize {
        let shown = strip::STACK_VISIBLE;
        if n <= shown {
            return 0;
        }
        match self.station {
            Station::Desk {
                room: r,
                desk: d,
                slot,
            } if r == self.rooms[room].id && d == desk => {
                slot.saturating_sub(shown - 1).min(n - shown)
            }
            _ => 0,
        }
    }

    /// The logical size of a loose screen (dialog/air) or a fullscreen one,
    /// when it differs from the stack's.
    pub(super) fn loose_logical(&self, id: ScreenId) -> Option<Vec2> {
        let st = self.screen_state(id);
        let committed = self
            .screens
            .get(&id)
            .map(|s| s.size)
            .filter(|s| s.x > 0.0 && s.y > 0.0);
        if st.parent.is_some() {
            return Some(committed.unwrap_or(DIALOG_DEFAULT));
        }
        if st.air.is_some() || st.fullscreen {
            return Some(committed.unwrap_or_else(|| self.usable().floor()));
        }
        None
    }

    // ---------------------------------------------------------------
    // commands

    /// The WM's commands (`Cmd` variants beyond phase 0).
    pub(super) fn wm_command(&mut self, cmd: Cmd, now: Time) -> Vec<Out> {
        match cmd {
            Cmd::BackOrStep => {
                if self.mode().walking() {
                    self.hold(Hold::Back);
                    Vec::new()
                } else {
                    self.action(Action::Back, now)
                }
            }
            Cmd::Room(n) => {
                if let Some(i) = (n as usize)
                    .checked_sub(1)
                    .filter(|i| *i < self.rooms.len())
                {
                    self.go_room(i, now);
                }
                Vec::new()
            }
            Cmd::First | Cmd::Last => {
                let r = self.current_room();
                let n = self.rooms[r].desks.len();
                if n > 0 {
                    let desk = if cmd == Cmd::First { 0 } else { n - 1 };
                    let st = Station::Desk {
                        room: self.rooms[r].id,
                        desk,
                        slot: 0,
                    };
                    if !self.station.same_place(&st) {
                        self.go_to(st, now, true);
                        self.pending.push(Wish::world(Want::Once));
                    }
                }
                Vec::new()
            }
            Cmd::Talk => match self.station_cat() {
                Some(cat) => self.action(
                    Action::Talk {
                        cat,
                        purpose: nkw_core::TalkPurpose::Session,
                    },
                    now,
                ),
                None => Vec::new(),
            },
            Cmd::TalkNext => self.action(Action::TalkNext, now),
            Cmd::TalkReply => self.action(Action::Reply, now),
            Cmd::TalkSit => {
                let Some(Kind::Talk { cat, .. }) = self.mode().kind() else {
                    return Vec::new();
                };
                match self.cat_desk(cat).and_then(|(r, d)| self.desk_pose(r, d)) {
                    Some(desk) => self.action(Action::Sit { desk }, now),
                    None => Vec::new(),
                }
            }
            Cmd::Hint { focus } => self.action(Action::Hint { focus }, now),
            Cmd::Palette => self.action(Action::Palette, now),
            Cmd::Shift(dir) => self.op_shift(dir, now),
            Cmd::Consume => self.op_consume(now),
            Cmd::Expel => self.op_expel(now),
            Cmd::Narrower => self.op_width(Width::narrower, now),
            Cmd::Wider => self.op_width(Width::wider, now),
            Cmd::Maximize => self.op_maximize(now),
            Cmd::Fullscreen => match self.focus_target().or(self.selected()) {
                Some(s) => {
                    let on = !self.screen_state(s).fullscreen;
                    self.set_fullscreen(s, on, now)
                }
                None => Vec::new(),
            },
            Cmd::Pin => self.op_pin(now),
            Cmd::SendRoom(r) => self.op_send_room(r, now),
            Cmd::Close => match self.focus_target().or(self.selected()) {
                Some(s) => vec![Out::Close(s)],
                None => Vec::new(),
            },
            Cmd::RoomNew { name, context, go } => {
                let i = self.room_new(name.as_str(), context);
                if go {
                    self.go_room(i, now);
                }
                self.changed(now, None);
                Vec::new()
            }
            Cmd::GotoRoom(r) => {
                if let Some(i) = self.find_room(&r) {
                    self.go_room(i, now);
                }
                Vec::new()
            }
            Cmd::GotoScreen(s) => {
                if let Some(st) = self.station_of(s) {
                    self.go_to(st, now, false);
                    self.pending.push(Wish::world(Want::Once));
                }
                Vec::new()
            }
            Cmd::Hold(h) => {
                self.hold(h);
                Vec::new()
            }
            Cmd::Recenter => {
                if let Some(w) = self.walking.as_mut() {
                    w.walker.pitch = 0.0;
                }
                Vec::new()
            }
            // phase 0's commands are the World's own (`World::command`)
            _ => Vec::new(),
        }
    }

    fn hold(&mut self, h: Hold) {
        let Some(w) = self.walking.as_mut() else {
            return;
        };
        let k = &mut w.keys;
        *match h {
            Hold::Forward => &mut k.forward,
            Hold::Back => &mut k.back,
            Hold::StrafeLeft => &mut k.left,
            Hold::StrafeRight => &mut k.right,
            Hold::TurnLeft => &mut k.turn_left,
            Hold::TurnRight => &mut k.turn_right,
            Hold::LookUp => &mut k.look_up,
            Hold::LookDown => &mut k.look_down,
        } = true;
    }

    /// The cat of the station's desk.
    pub(super) fn station_cat(&self) -> Option<nkw_core::CatId> {
        let Station::Desk { room, desk, .. } = self.station else {
            return None;
        };
        let r = self.room_index(room)?;
        self.rooms[r].desks.get(desk)?.cat
    }

    /// Go to room `i`: its first desk, else its door.
    pub(super) fn go_room(&mut self, i: usize, now: Time) {
        let st = if self.rooms[i].desks.is_empty() {
            Station::Door(self.rooms[i].id)
        } else {
            Station::Desk {
                room: self.rooms[i].id,
                desk: 0,
                slot: 0,
            }
        };
        self.go_to(st, now, false);
        self.pending.push(Wish::world(Want::Once));
    }

    /// The selected screen when it is part of a stack: (room, desk, slot).
    fn sel_stack(&self) -> Option<(usize, usize, usize)> {
        let s = self.selected()?;
        if self.is_loose(s) {
            return None;
        }
        self.locate(s)
    }

    /// Take `id` and its dialogs out of desk (r, d); the desk stays.
    fn detach(&mut self, r: usize, d: usize, id: ScreenId) -> Vec<ScreenId> {
        let kids: Vec<ScreenId> = self.rooms[r].desks[d]
            .screens
            .iter()
            .copied()
            .filter(|s| *s != id && self.descends(*s, id))
            .collect();
        let desk = &mut self.rooms[r].desks[d];
        desk.screens.retain(|s| *s != id && !kids.contains(s));
        let mut v = vec![id];
        v.extend(kids);
        v
    }

    /// Whether `s` is a dialog of `of` (at any depth).
    fn descends(&self, s: ScreenId, of: ScreenId) -> bool {
        let mut cur = s;
        for _ in 0..8 {
            match self.screen_state(cur).parent {
                Some(p) if p == of => return true,
                Some(p) => cur = p,
                None => return false,
            }
        }
        false
    }

    /// Put `moved` (a screen, then its dialogs) into desk (r, d): the first
    /// at the end of the stack (or loose when it is), the rest loose.
    fn attach(&mut self, r: usize, d: usize, moved: &[ScreenId]) -> usize {
        let n = self.stack_len(r, d);
        let first_loose = self.is_loose(moved[0]);
        let desk = &mut self.rooms[r].desks[d];
        let slot = if first_loose {
            desk.screens.push(moved[0]);
            0
        } else {
            desk.screens.insert(n, moved[0]);
            n
        };
        desk.screens.extend_from_slice(&moved[1..]);
        slot
    }

    /// Remove desk (r, d) when nothing is left on it; returns whether it went.
    fn drop_if_empty(&mut self, r: usize, d: usize) -> bool {
        let desk = &self.rooms[r].desks[d];
        if desk.screens.is_empty() && desk.cat.is_none() {
            self.rooms[r].desks.remove(d);
            true
        } else {
            false
        }
    }

    /// Configure every stack screen of a desk to its preset size.
    fn configure_desk(&self, r: usize, d: usize) -> Vec<Out> {
        let n = self.stack_len(r, d);
        let desk = &self.rooms[r].desks[d];
        let size = self.configure_size(desk.width, n);
        desk.screens[..n]
            .iter()
            .filter(|s| !self.screen_state(**s).fullscreen)
            .map(|s| Out::Configure {
                screen: *s,
                size: Some(size),
                fullscreen: None,
                maximized: None,
            })
            .collect()
    }

    fn set_station_desk(&mut self, r: usize, d: usize, slot: usize) {
        self.station = Station::Desk {
            room: self.rooms[r].id,
            desk: d,
            slot,
        };
    }

    /// After a layout change: a new generation, the badge, a frame, and the
    /// camera where it belongs (a glide in NAV, a cut in FOCO).
    pub(super) fn changed(&mut self, now: Time, note: Option<String>) {
        self.layout_gen += 1;
        if let Some(n) = note {
            self.note = Some((now, n));
        }
        self.pending.push(Wish::world(Want::Once));
        if let Some((_, o)) = self.focus_2d_target() {
            self.camera = o.pose;
        } else if self.mode().overlay.is_none()
            && !self.mode().walking()
            && let Some(p) = self.station_pose(&self.station)
            && p != self.camera
            && self.tween.is_none_or(|t| t.to != p)
        {
            self.glide(p, now, false);
        }
    }

    /// `H` / `L`: a screen alone on its desk moves the desk past its
    /// neighbour; a screen of a stack (or of a cat's desk) leaves it for a
    /// desk of its own on that side.
    fn op_shift(&mut self, dir: Dir, now: Time) -> Vec<Out> {
        let right = match dir {
            Dir::Left => false,
            Dir::Right => true,
            Dir::Up | Dir::Down => return Vec::new(),
        };
        let Some((r, d, _)) = self.sel_stack() else {
            return Vec::new();
        };
        let id = self.selected().expect("selected");
        let n = self.rooms[r].desks.len();
        let desk = &self.rooms[r].desks[d];
        let alone = desk.cat.is_none() && self.stack_len(r, d) == 1;
        let mut out = Vec::new();
        if alone {
            let j = if right { d + 1 } else { d.wrapping_sub(1) };
            if j >= n {
                return out;
            }
            self.rooms[r].desks.swap(d, j);
            self.set_station_desk(r, j, 0);
        } else {
            let width = self.rooms[r].desks[d].width;
            let moved = self.detach(r, d, id);
            let at = if right { d + 1 } else { d };
            self.rooms[r].desks.insert(
                at,
                Desk {
                    width,
                    screens: moved,
                    cat: None,
                },
            );
            let old = if right { d } else { d + 1 };
            out.extend(self.configure_desk(r, old));
            out.extend(self.configure_desk(r, at));
            self.set_station_desk(r, at, 0);
        }
        self.changed(now, None);
        out
    }

    /// `,`: the screen joins the bottom of the stack on its left.
    fn op_consume(&mut self, now: Time) -> Vec<Out> {
        let Some((r, d, _)) = self.sel_stack() else {
            return Vec::new();
        };
        if d == 0 {
            return Vec::new();
        }
        let id = self.selected().expect("selected");
        let moved = self.detach(r, d, id);
        let left = d - 1;
        let slot = self.attach(r, left, &moved);
        let gone = self.drop_if_empty(r, d);
        let mut out = self.configure_desk(r, left);
        if !gone {
            out.extend(self.configure_desk(r, d));
        }
        self.set_station_desk(r, left, slot);
        let n = self.stack_len(r, left);
        self.changed(now, Some(format!("pilha {n}")));
        out
    }

    /// `.`: the screen leaves its stack (or the cat's desk) for a desk of
    /// its own, right after.
    fn op_expel(&mut self, now: Time) -> Vec<Out> {
        let Some((r, d, _)) = self.sel_stack() else {
            return Vec::new();
        };
        if self.stack_len(r, d) <= 1 && self.rooms[r].desks[d].cat.is_none() {
            return Vec::new();
        }
        let id = self.selected().expect("selected");
        let width = self.rooms[r].desks[d].width;
        let moved = self.detach(r, d, id);
        self.rooms[r].desks.insert(
            d + 1,
            Desk {
                width,
                screens: moved,
                cat: None,
            },
        );
        let mut out = self.configure_desk(r, d);
        out.extend(self.configure_desk(r, d + 1));
        self.set_station_desk(r, d + 1, 0);
        self.changed(now, Some(width_label(width).into()));
        out
    }

    /// The station's desk (room, desk), if the station is at one.
    fn station_desk(&self) -> Option<(usize, usize)> {
        match self.station {
            Station::Desk { room, desk, .. } => {
                let r = self.room_index(room)?;
                (desk < self.rooms[r].desks.len()).then_some((r, desk))
            }
            Station::Air(s) => self.locate(s).map(|(r, d, _)| (r, d)),
            _ => None,
        }
    }

    /// `-` / `=`: the desk one preset narrower/wider; its screens are
    /// configured to the new size (and are no longer maximized).
    fn op_width(&mut self, f: fn(Width) -> Width, now: Time) -> Vec<Out> {
        let Some((r, d)) = self.station_desk() else {
            return Vec::new();
        };
        let old = self.rooms[r].desks[d].width;
        let new = f(old);
        if new == old {
            self.changed(now, Some(width_label(new).into()));
            return Vec::new();
        }
        self.rooms[r].desks[d].width = new;
        let mut out = self.configure_desk(r, d);
        for o in &mut out {
            if let Out::Configure {
                screen, maximized, ..
            } = o
                && let Some(st) = self.wm.get_mut(screen)
                && st.maximized
            {
                st.maximized = false;
                st.restore = None;
                *maximized = Some(false);
            }
        }
        self.changed(now, Some(width_label(new).into()));
        out
    }

    /// Maximize (the desk at full width) and back.
    fn op_maximize(&mut self, now: Time) -> Vec<Out> {
        match self.sel_stack() {
            Some(_) => {
                let s = self.selected().expect("selected");
                let on = !self.screen_state(s).maximized;
                self.set_maximized(s, on, now)
            }
            None => Vec::new(),
        }
    }

    /// `xdg_toplevel.set_maximized` / `world.maximize`: the screen's desk
    /// goes full width (and back to its width before).
    pub fn set_maximized(&mut self, id: ScreenId, on: bool, now: Time) -> Vec<Out> {
        let Some((r, d, _)) = self.locate(id) else {
            return Vec::new();
        };
        if self.is_loose(id) {
            return Vec::new();
        }
        let st = self.wm.entry(id).or_default();
        let desk = &mut self.rooms[r].desks[d];
        if on {
            if !st.maximized {
                st.restore = Some(desk.width);
            }
            desk.width = Width::Full;
        } else if st.maximized {
            desk.width = st.restore.take().unwrap_or_default();
        }
        st.maximized = on;
        let mut out = self.configure_desk(r, d);
        for o in &mut out {
            if let Out::Configure {
                screen, maximized, ..
            } = o
                && *screen == id
            {
                *maximized = Some(on);
            }
        }
        if !out
            .iter()
            .any(|o| matches!(o, Out::Configure { screen, .. } if *screen == id))
        {
            out.push(Out::Configure {
                screen: id,
                size: None,
                fullscreen: None,
                maximized: Some(on),
            });
        }
        let w = self.rooms[r].desks[d].width;
        self.changed(now, Some(width_label(w).into()));
        out
    }

    /// `xdg_toplevel.set_fullscreen` / `world.fullscreen`: configured to the
    /// whole output. In FOCO it covers the output (the world pass does not
    /// run); outside FOCO its quad is a TV on the desk.
    pub fn set_fullscreen(&mut self, id: ScreenId, on: bool, now: Time) -> Vec<Out> {
        let Some((r, d, _)) = self.locate(id) else {
            return Vec::new();
        };
        let was = self.screen_state(id).fullscreen;
        self.wm.entry(id).or_default().fullscreen = on;
        let size = if on {
            self.usable().floor()
        } else if self.is_loose(id) {
            self.loose_logical(id).unwrap_or(DIALOG_DEFAULT)
        } else {
            self.configure_size(self.rooms[r].desks[d].width, self.stack_len(r, d))
        };
        if was != on {
            self.changed(now, Some(if on { "TV" } else { "tela" }.into()));
        }
        vec![Out::Configure {
            screen: id,
            size: Some(size),
            fullscreen: Some(on),
            maximized: None,
        }]
    }

    /// `p`: the selected screen stays in the air where it is (the station
    /// becomes `Air`, 1:1 aligned to its normal); again: back to its stack.
    fn op_pin(&mut self, now: Time) -> Vec<Out> {
        if let Station::Air(s) = self.station {
            return self.unpin(s, now);
        }
        let Some((r, d, k)) = self.sel_stack() else {
            return Vec::new();
        };
        let id = self.rooms[r].desks[d].screens[k];
        let place = self.screen_place(r, d, k);
        let size = self.screen_logical(r, d, k);
        if let Some(s) = self.screens.get_mut(&id)
            && (s.size.x <= 0.0 || s.size.y <= 0.0)
        {
            s.size = size;
        }
        self.wm.entry(id).or_default().air = Some(place);
        let desk = &mut self.rooms[r].desks[d];
        desk.screens.retain(|s| *s != id);
        desk.screens.push(id);
        let out = self.configure_desk(r, d);
        self.go_to(Station::Air(id), now, false);
        self.changed(now, Some("no ar".into()));
        out
    }

    fn unpin(&mut self, id: ScreenId, now: Time) -> Vec<Out> {
        let Some((r, d, _)) = self.locate(id) else {
            return Vec::new();
        };
        let n = self.stack_len(r, d);
        self.wm.entry(id).or_default().air = None;
        let desk = &mut self.rooms[r].desks[d];
        desk.screens.retain(|s| *s != id);
        desk.screens.insert(n.min(desk.screens.len()), id);
        let out = self.configure_desk(r, d);
        self.set_station_desk(r, d, n);
        self.changed(now, Some(width_label(self.rooms[r].desks[d].width).into()));
        out
    }

    /// `M`: the screen (and its dialogs) goes to a new desk at the end of
    /// another room; the station stays in this one.
    fn op_send_room(&mut self, to: RoomRef, now: Time) -> Vec<Out> {
        let Some((r, d, _)) = self.sel_stack() else {
            return Vec::new();
        };
        let Some(t) = self.find_room(&to).filter(|t| *t != r) else {
            return Vec::new();
        };
        let id = self.selected().expect("selected");
        let width = self.rooms[r].desks[d].width;
        let moved = self.detach(r, d, id);
        let gone = self.drop_if_empty(r, d);
        self.rooms[t].desks.push(Desk {
            width,
            screens: moved,
            cat: None,
        });
        let at = self.rooms[t].desks.len() - 1;
        let mut out = self.configure_desk(t, at);
        let n = self.rooms[r].desks.len();
        if n == 0 {
            self.station = Station::Door(self.rooms[r].id);
        } else {
            if !gone {
                out.extend(self.configure_desk(r, d));
            }
            let desk = if gone && d > 0 { d - 1 } else { d }.min(n - 1);
            self.set_station_desk(r, desk, 0);
        }
        let name = self.rooms[t].name.clone();
        self.changed(now, Some(format!("→ {name}")));
        out
    }

    /// A room made by name (`world.room_new`, a rule, a restore), before
    /// "minha sala"; an existing one by that name is kept. Returns its index.
    pub(super) fn room_new(&mut self, name: &str, context: bool) -> usize {
        if let Some(i) = self.rooms.iter().position(|r| r.name == name) {
            return i;
        }
        let room = self.new_room(name);
        let kind = if context {
            RoomKind::Context(name.to_string())
        } else {
            RoomKind::Free
        };
        self.kinds.insert(room.id, kind);
        let at = self
            .rooms
            .iter()
            .rposition(|r| self.room_kind(r.id) == RoomKind::User)
            .unwrap_or(self.rooms.len());
        self.rooms.insert(at, room);
        at
    }

    // ---------------------------------------------------------------
    // the xdg side

    /// A fresh window's app_id arrived: move it to `place` (a room by name,
    /// made when missing; a new desk at the end or at an index; a width).
    pub fn place_new(&mut self, id: ScreenId, place: &Place, now: Time) -> Vec<Out> {
        let Some((r, d, _)) = self.locate(id) else {
            return Vec::new();
        };
        let st = self.screen_state(id);
        let alone = self.rooms[r].desks[d].screens == [id] && self.rooms[r].desks[d].cat.is_none();
        if st.placed || st.parent.is_some() || !alone {
            return Vec::new();
        }
        self.wm.entry(id).or_default().placed = true;
        let anchor = self.anchor();
        let width = place.width.unwrap_or(self.rooms[r].desks[d].width);
        self.rooms[r].desks.remove(d);
        let room_id = self.rooms[r].id;
        let t = match &place.room {
            Some(name) => self.room_new(name, false),
            None => self.room_index(room_id).unwrap_or(0),
        };
        let len = self.rooms[t].desks.len();
        let at = match place.desk {
            Some(DeskAt::Last) => len,
            Some(DeskAt::Index(i)) => i.min(len),
            None if self.rooms[t].id == room_id => d.min(len),
            None => len,
        };
        self.rooms[t].desks.insert(
            at,
            Desk {
                width,
                screens: vec![id],
                cat: None,
            },
        );
        let st = Station::Desk {
            room: self.rooms[t].id,
            desk: at,
            slot: 0,
        };
        if self.mode().focus().is_none() {
            self.go_to(st, now, false);
        } else {
            self.station = self.resolve(anchor);
        }
        self.changed(now, None);
        vec![Out::Configure {
            screen: id,
            size: Some(self.configure_size(width, 1)),
            fullscreen: None,
            maximized: None,
        }]
    }

    /// Mark a window placed without moving it (no rule, no memory).
    pub fn mark_placed(&mut self, id: ScreenId) {
        if self.screens.contains_key(&id) {
            self.wm.entry(id).or_default().placed = true;
        }
    }

    /// `xdg_toplevel.set_parent`: a dialog leaves the desk it was given
    /// and floats 5 cm in front of its mother, centred; with the mother in
    /// FOCO it becomes the FOCO target (centred, 1:1). `None`: a window of
    /// its own again (a new desk right of the mother's).
    pub fn set_parent(&mut self, id: ScreenId, parent: Option<ScreenId>, now: Time) -> Vec<Out> {
        let parent =
            parent.filter(|p| *p != id && self.screens.contains_key(p) && !self.descends(*p, id));
        if self.screen_state(id).parent == parent {
            return Vec::new();
        }
        let Some((r, d, _)) = self.locate(id) else {
            return Vec::new();
        };
        let anchor = self.anchor();
        let moved = self.detach(r, d, id);
        let rid = self.rooms[r].id;
        let gone = self.drop_if_empty(r, d);
        self.wm.entry(id).or_default().parent = parent;
        let mut out = Vec::new();
        match parent {
            Some(p) => {
                let (mr, md, _) = self.locate(p).expect("mother on a desk");
                self.attach(mr, md, &moved);
                out.push(Out::Configure {
                    screen: id,
                    size: Some(Vec2::ZERO),
                    fullscreen: None,
                    maximized: None,
                });
                let was_here = matches!(anchor, super::Anchor::Desk { room, key: super::DeskKey::Screen(s), .. }
                    if room == rid && s == id);
                if self.focus_target() == Some(p) {
                    out.extend(self.action(Action::Neighbour(Target::Screen(id)), now));
                } else if was_here || gone {
                    if let Some(st) = self.station_of(p)
                        && self.mode().focus().is_none()
                    {
                        self.go_to(st, now, false);
                    } else {
                        self.station = self.resolve(anchor);
                    }
                } else {
                    self.station = self.resolve(anchor);
                }
            }
            None => {
                // back into a stack of its own, right of where it floated
                let at = (d + usize::from(!gone)).min(self.rooms[r].desks.len());
                self.rooms[r].desks.insert(
                    at,
                    Desk {
                        width: Width::Half,
                        screens: moved,
                        cat: None,
                    },
                );
                out.extend(self.configure_desk(r, at));
                self.station = self.resolve(anchor);
            }
        }
        self.changed(now, None);
        out
    }

    /// `xdg_dialog_v1.set_modal` / `unset_modal`.
    pub fn set_modal(&mut self, id: ScreenId, modal: bool, now: Time) -> Vec<Out> {
        if !self.screens.contains_key(&id) || self.screen_state(id).modal == modal {
            return Vec::new();
        }
        self.wm.entry(id).or_default().modal = modal;
        // a modal dialog over the FOCO window takes the keyboard
        let mut out = Vec::new();
        if modal
            && let Some(p) = self.screen_state(id).parent
            && self.focus_target() == Some(p)
        {
            out = self.action(Action::Neighbour(Target::Screen(id)), now);
        }
        self.pending.push(Wish::compose(Want::Once));
        out
    }

    /// xdg-activation: a valid recent token selects the station (NAV) or
    /// badges (elsewhere); without one, "quer atenção" on the screen for 6 s.
    pub fn activate(&mut self, id: ScreenId, valid: bool, now: Time) -> Vec<Out> {
        if !self.screens.contains_key(&id) {
            return Vec::new();
        }
        if valid {
            let target = self.modal_child(id).unwrap_or(id);
            return self.action(Action::Activation(target), now);
        }
        self.wm.entry(id).or_default().attention = Some(now + ATTENTION_S);
        self.pending.push(Wish::compose(Want::Once));
        Vec::new()
    }
}

/// `p`, `dist` metres toward whoever looks at it.
fn in_front(p: &ScreenPlace, dist: f32) -> ScreenPlace {
    let fwd = Pose::new(p.center, p.yaw, p.pitch).forward();
    ScreenPlace {
        center: p.center - fwd * dist,
        ..*p
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::world::{Opts, Room, Screen};
    use nkw_core::{Base, CatId, Output};

    const OUT: Output = Output {
        width_px: 1280,
        height_px: 720,
        scale: 1.0,
    };
    const OPTS: Opts = Opts {
        reduced_motion: false,
    };
    const NO: BTreeMap<String, String> = BTreeMap::new();

    fn desk(screens: &[u32], cat: Option<u32>) -> Desk {
        Desk {
            width: Width::Half,
            screens: screens.iter().map(|&s| ScreenId(s)).collect(),
            cat: cat.map(CatId),
        }
    }

    /// The 3-room fixture: reception (cat 1), acme (screens 1 | 2 on cat 9's
    /// desk | 3), minha sala (empty); the station at acme's first desk.
    fn fixture() -> World {
        let rooms = vec![
            Room {
                id: RoomId(0),
                name: RECEPTION.into(),
                desks: vec![desk(&[], Some(1))],
            },
            Room {
                id: RoomId(1),
                name: "acme".into(),
                desks: vec![desk(&[1], None), desk(&[2], Some(9)), desk(&[3], None)],
            },
            Room {
                id: RoomId(2),
                name: MY_ROOM.into(),
                desks: vec![],
            },
        ];
        let mut w = World::with_rooms(OUT, OPTS, rooms);
        for i in 1..=3 {
            w.screens.insert(
                ScreenId(i),
                Screen {
                    id: ScreenId(i),
                    app_id: format!("acme.{i}"),
                    title: format!("janela {i}"),
                    size: Vec2::new(624.0, 720.0),
                },
            );
        }
        w.next_id = 4;
        w.station = Station::Desk {
            room: RoomId(1),
            desk: 0,
            slot: 0,
        };
        w.camera = w.station_pose(&w.station).unwrap();
        w
    }

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

    /// The screens of each desk of room 1 (acme).
    fn acme(w: &World) -> Vec<Vec<u32>> {
        w.rooms[1]
            .desks
            .iter()
            .map(|d| d.screens.iter().map(|s| s.0).collect())
            .collect()
    }

    fn at(w: &mut World, desk: usize, slot: usize) {
        w.station = Station::Desk {
            room: RoomId(1),
            desk,
            slot,
        };
    }

    fn run(w: &mut World, name: &str) -> Vec<Out> {
        let c = Cmd::from_name(name, &NO).unwrap_or_else(|| panic!("{name}"));
        w.command(c, 0.0)
    }

    fn sizes(out: &[Out]) -> Vec<(u32, Vec2)> {
        out.iter()
            .filter_map(|o| match o {
                Out::Configure {
                    screen,
                    size: Some(s),
                    ..
                } => Some((screen.0, *s)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn short_names_and_room_refs() {
        let n = ShortName::new("acme");
        assert_eq!(n.as_str(), "acme");
        assert_eq!(format!("{n:?}"), "\"acme\"");
        let long = "ç".repeat(40); // 80 bytes
        let cut = ShortName::new(&long);
        assert!(cut.as_str().len() <= ROOM_NAME_MAX);
        assert_eq!(cut.as_str(), "ç".repeat(23), "cut at a char boundary");
        assert_eq!(RoomRef::parse(" 3 "), Some(RoomRef::Id(RoomId(3))));
        assert_eq!(
            RoomRef::parse("acme"),
            Some(RoomRef::Name(ShortName::new("acme")))
        );
        assert_eq!(RoomRef::parse("  "), None);
        for r in [
            RoomRef::Id(RoomId(7)),
            RoomRef::Name(ShortName::new("bravo")),
        ] {
            assert_eq!(RoomRef::parse(&r.arg()), Some(r));
        }
        let w = fixture();
        assert_eq!(w.find_room(&RoomRef::parse("ACME").unwrap()), Some(1));
        assert_eq!(w.find_room(&RoomRef::parse("minha").unwrap()), Some(2));
        assert_eq!(w.find_room(&RoomRef::parse("2").unwrap()), Some(2));
        assert_eq!(w.find_room(&RoomRef::parse("zzz").unwrap()), None);
    }

    #[test]
    fn the_ops_table() {
        // (station desk, slot), command, acme's desks after, station desk after
        type Row = (
            (usize, usize),
            &'static str,
            &'static [&'static [u32]],
            usize,
        );
        let rows: &[Row] = &[
            // H/L: alone on its desk → the desk moves past its neighbour
            ((0, 0), "world.move_right", &[&[2], &[1], &[3]], 1),
            ((0, 0), "world.move_left", &[&[1], &[2], &[3]], 0),
            ((2, 0), "world.move_left", &[&[1], &[3], &[2]], 1),
            ((2, 0), "world.move_right", &[&[1], &[2], &[3]], 2),
            // on a cat's desk → out to a desk of its own on that side
            ((1, 0), "world.move_right", &[&[1], &[], &[2], &[3]], 2),
            ((1, 0), "world.move_left", &[&[1], &[2], &[], &[3]], 1),
            // , joins the stack on the left; its empty desk goes
            ((2, 0), "world.consume", &[&[1], &[2, 3]], 1),
            ((0, 0), "world.consume", &[&[1], &[2], &[3]], 0),
            // . out of the cat's desk; alone: nothing
            ((1, 0), "world.expel", &[&[1], &[], &[2], &[3]], 2),
            ((0, 0), "world.expel", &[&[1], &[2], &[3]], 0),
            // first / last
            ((1, 0), "world.first", &[&[1], &[2], &[3]], 0),
            ((0, 0), "world.last", &[&[1], &[2], &[3]], 2),
        ];
        for &((d, k), name, want, st) in rows {
            let mut w = fixture();
            at(&mut w, d, k);
            run(&mut w, name);
            let want: Vec<Vec<u32>> = want.iter().map(|v| v.to_vec()).collect();
            assert_eq!(acme(&w), want, "{name} from desk {d}");
            match w.station {
                Station::Desk { desk, room, .. } => {
                    assert_eq!((room, desk), (RoomId(1), st), "{name} from desk {d}")
                }
                other => panic!("{name}: station {other:?}"),
            }
        }
    }

    #[test]
    fn the_cats_desk_keeps_its_cat() {
        let mut w = fixture();
        at(&mut w, 1, 0);
        run(&mut w, "world.move_right");
        assert_eq!(w.rooms[1].desks[1].cat, Some(CatId(9)));
        assert_eq!(w.rooms[1].desks[2].cat, None);
        assert_eq!(w.selected(), Some(ScreenId(2)));
    }

    #[test]
    fn consume_then_expel_round_trip_with_sizes() {
        let mut w = fixture();
        at(&mut w, 2, 0);
        let out = run(&mut w, "world.consume");
        // the cat's desk now stacks 2 and 3: both at half height
        let half = w.configure_size(Width::Half, 2);
        assert_eq!(half, Vec2::new(624.0, 360.0));
        assert_eq!(sizes(&out), [(2, half), (3, half)]);
        assert_eq!(w.selected(), Some(ScreenId(3)));
        assert_eq!(w.layout_note(0.0).map(|n| n.0), Some("pilha 2"));
        let out = run(&mut w, "world.expel");
        assert_eq!(acme(&w), [vec![1], vec![2], vec![3]]);
        let one = w.configure_size(Width::Half, 1);
        assert_eq!(sizes(&out), [(2, one), (3, one)]);
    }

    #[test]
    fn configure_sizes_per_preset() {
        let mut w = fixture();
        // 1280x720: the preset share less the 16 px margin
        let rows = [
            ("world.wider", Width::TwoThirds, 837.0),
            ("world.wider", Width::Full, 1264.0),
            ("world.wider", Width::Full, 1264.0),
            ("world.narrower", Width::TwoThirds, 837.0),
            ("world.narrower", Width::Half, 624.0),
            ("world.narrower", Width::Third, 410.0),
            ("world.narrower", Width::Third, 410.0),
        ];
        for (name, width, px) in rows {
            let before = w.rooms[1].desks[0].width;
            let out = run(&mut w, name);
            assert_eq!(w.rooms[1].desks[0].width, width, "{name}");
            if before == width {
                assert!(out.is_empty(), "{name}: no change, no configure");
            } else {
                assert_eq!(sizes(&out), [(1, Vec2::new(px, 720.0))], "{name}");
            }
            assert_eq!(w.layout_note(0.0).map(|n| n.0), Some(width_label(width)));
        }
        assert!(
            w.layout_note(LAYOUT_NOTE_S + 0.1).is_none(),
            "the badge goes after 3 s"
        );
    }

    #[test]
    fn maximize_and_back() {
        let mut w = fixture();
        let out = run(&mut w, "world.maximize");
        assert_eq!(w.rooms[1].desks[0].width, Width::Full);
        assert_eq!(
            out,
            [Out::Configure {
                screen: ScreenId(1),
                size: Some(Vec2::new(1264.0, 720.0)),
                fullscreen: None,
                maximized: Some(true)
            }]
        );
        assert!(w.screen_state(ScreenId(1)).maximized);
        let out = run(&mut w, "world.maximize");
        assert_eq!(w.rooms[1].desks[0].width, Width::Half);
        assert!(matches!(
            out[0],
            Out::Configure {
                maximized: Some(false),
                ..
            }
        ));
        // narrowing a maximized desk unmaximizes it
        run(&mut w, "world.maximize");
        let out = run(&mut w, "world.narrower");
        assert!(matches!(
            out[0],
            Out::Configure {
                maximized: Some(false),
                ..
            }
        ));
        assert!(!w.screen_state(ScreenId(1)).maximized);
    }

    #[test]
    fn stacks_over_three_pile_behind_and_jk_walk_them_all() {
        let mut w = fixture();
        // five screens on acme's first desk
        for i in 4..=7 {
            w.screens.insert(
                ScreenId(i),
                Screen {
                    id: ScreenId(i),
                    app_id: format!("acme.{i}"),
                    title: String::new(),
                    size: Vec2::ZERO,
                },
            );
            w.rooms[1].desks[0].screens.push(ScreenId(i));
        }
        assert_eq!(w.stack_len(1, 0), 5);
        assert_eq!(w.spots(1)[0].stack, 5);
        let mut now = 0.0;
        // depth: how far behind the screen plane, seen from the station
        let cam = w.camera;
        let z = |w: &World, k: usize| -> f32 {
            let c = w.screen_place(1, 0, k).center;
            let f = cam.forward();
            (c - cam.pos).dot(Vec3::new(f.x, 0.0, f.z).normalize())
        };
        // at slot 0: 0..3 in front (same plane), 3 and 4 piled behind
        let front = z(&w, 0);
        assert!((z(&w, 2) - front).abs() < 1e-3);
        assert!(z(&w, 3) > front + 0.03, "{} vs {front}", z(&w, 3));
        assert!(z(&w, 4) > z(&w, 3), "deeper in the pile");
        assert!(w.screen_place(1, 0, 4).center.y > w.screen_place(1, 0, 2).center.y);
        // j walks the whole stack; the selected one is always in front
        for slot in 1..5 {
            w.command(Cmd::Move(Dir::Down), now);
            settle(&mut w, &mut now);
            assert_eq!(w.selected(), Some(w.rooms[1].desks[0].screens[slot]));
            assert!((z(&w, slot) - front).abs() < 1e-3, "slot {slot} in front");
        }
        assert!(z(&w, 0) > front + 0.03, "the top went behind");
        w.command(Cmd::Move(Dir::Down), now);
        assert_eq!(w.selected(), Some(ScreenId(7)), "the bottom stays");
        // configure: three visible share the height
        assert_eq!(w.configure_size(Width::Half, 5).y, 240.0);
    }

    #[test]
    fn pin_floats_and_comes_back() {
        let mut w = fixture();
        let mut now = 0.0;
        at(&mut w, 2, 0);
        let before = w.screen_place(1, 2, 0);
        run(&mut w, "world.pin");
        assert_eq!(w.station, Station::Air(ScreenId(3)));
        assert_eq!(w.screen_state(ScreenId(3)).air, Some(before));
        settle(&mut w, &mut now);
        // the station is the 1:1 pose, aligned to its normal
        let o = w.one_to_one(ScreenId(3)).unwrap();
        assert_eq!(w.camera, o.pose);
        assert_eq!(w.selected(), Some(ScreenId(3)));
        // the strip changes under it: it stays where it was
        w.rooms[1].desks.insert(0, desk(&[], None));
        let (r, d, k) = w.locate(ScreenId(3)).unwrap();
        assert_eq!(w.screen_place(r, d, k), before);
        // h from the air goes to the nearest station on that side
        w.command(Cmd::Move(Dir::Left), now);
        assert!(matches!(w.station, Station::Desk { .. }), "{:?}", w.station);
        w.station = Station::Air(ScreenId(3));
        run(&mut w, "world.pin");
        assert!(w.screen_state(ScreenId(3)).air.is_none());
        assert_eq!(w.selected(), Some(ScreenId(3)));
        assert_eq!(w.layout_note(0.0).map(|n| n.0), Some("1/2"));
    }

    #[test]
    fn send_to_another_room() {
        let mut w = fixture();
        at(&mut w, 2, 0);
        let args = BTreeMap::from([("room".to_string(), MY_ROOM.to_string())]);
        let c = Cmd::from_name("world.send_room", &args).unwrap();
        let out = w.command(c, 0.0);
        assert_eq!(acme(&w), [vec![1], vec![2]]);
        assert_eq!(w.rooms[2].desks[0].screens, [ScreenId(3)]);
        assert_eq!(sizes(&out)[0].0, 3);
        assert_eq!(
            w.station,
            Station::Desk {
                room: RoomId(1),
                desk: 1,
                slot: 0
            },
            "the station stays in this room"
        );
        // to its own room: nothing
        let args = BTreeMap::from([("room".to_string(), "acme".to_string())]);
        assert!(
            w.command(Cmd::from_name("world.send_room", &args).unwrap(), 0.0)
                .is_empty()
        );
        assert_eq!(Cmd::from_name("world.send_room", &NO), None, "needs a room");
    }

    #[test]
    fn close_room_n_and_new_rooms() {
        let mut w = fixture();
        at(&mut w, 1, 0);
        assert_eq!(run(&mut w, "world.close"), [Out::Close(ScreenId(2))]);
        run(&mut w, "world.room_3");
        assert_eq!(w.current_room(), 2);
        run(&mut w, "world.room_9");
        assert_eq!(w.current_room(), 2, "no 9th room");
        run(&mut w, "world.room_1");
        assert_eq!(w.current_room(), 0);
        let args = BTreeMap::from([("name".to_string(), "bravo".to_string())]);
        w.command(Cmd::from_name("world.room_new", &args).unwrap(), 0.0);
        let names: Vec<_> = w.rooms.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, [RECEPTION, "acme", "bravo", MY_ROOM]);
        assert_eq!(w.room_kind(w.rooms[2].id), RoomKind::Free);
        assert_eq!(w.current_room(), 2, "goes to the new room");
        assert_eq!(w.station, Station::Door(w.rooms[2].id));
        // the same name again: no second room
        w.command(Cmd::from_name("world.room_new", &args).unwrap(), 0.0);
        assert_eq!(w.rooms.len(), 4);
        let args = BTreeMap::from([("room".to_string(), "acme".to_string())]);
        w.command(Cmd::from_name("world.goto_room", &args).unwrap(), 0.0);
        assert_eq!(w.current_room(), 1);
        // the palette's "ir para" items carry the target in the name
        w.command(Cmd::from_name("world.goto_room:2", &NO).unwrap(), 0.0);
        assert_eq!(w.current_room_id(), RoomId(2), "by id: minha sala");
        w.command(Cmd::from_name("world.goto_screen:3", &NO).unwrap(), 0.0);
        assert_eq!(w.selected(), Some(ScreenId(3)));
    }

    #[test]
    fn free_rooms_survive_the_contexts() {
        let mut w = fixture();
        w.room_new("bravo", false);
        w.sync_contexts(&[("acme".to_string(), vec![CatId(9)])], 0.0);
        let names: Vec<_> = w.rooms.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, [RECEPTION, "acme", "bravo", MY_ROOM]);
        assert_eq!(w.room_kind(w.rooms[1].id), RoomKind::Context("acme".into()));
        assert_eq!(w.room_kind(w.rooms[0].id), RoomKind::Reception);
        assert_eq!(w.room_kind(w.rooms[3].id), RoomKind::User);
    }

    #[test]
    fn fullscreen_is_a_tv_and_covers_foco() {
        let mut w = fixture();
        let mut now = 0.0;
        let out = run(&mut w, "world.fullscreen");
        assert_eq!(
            out,
            [Out::Configure {
                screen: ScreenId(1),
                size: Some(Vec2::new(1280.0, 720.0)),
                fullscreen: Some(true),
                maximized: None
            }]
        );
        // the client commits the new size: the quad on the desk is a TV
        w.set_size(ScreenId(1), Vec2::new(1280.0, 720.0));
        assert_eq!(w.screen_logical(1, 0, 0), Vec2::new(1280.0, 720.0));
        w.command(Cmd::Focus, now);
        settle(&mut w, &mut now);
        let (_, o) = w.focus_2d().unwrap();
        assert_eq!(o.origin, Vec2::ZERO);
        assert_eq!(o.size, Vec2::new(1280.0, 720.0));
        // covering the output: no world pass, not even the arrival one
        assert!(
            w.wishes().iter().all(|x| x.pass == nkw_core::Pass::Compose),
            "{:?}",
            w.wishes()
        );
        w.frame_done(now);
        w.want(Wish::world(Want::Once));
        assert!(w.wishes().iter().all(|x| x.pass == nkw_core::Pass::Compose));
        // back to a window: its desk size again
        let out = w.set_fullscreen(ScreenId(1), false, now);
        assert!(matches!(
            out[0],
            Out::Configure { size: Some(s), fullscreen: Some(false), .. } if s == Vec2::new(624.0, 720.0)
        ));
    }

    fn dialog(w: &mut World, now: Time) -> ScreenId {
        let (id, _) = w.add_screen("acme.dialog", "abrir", now);
        id
    }

    #[test]
    fn dialogs_float_in_front_of_the_mother() {
        let mut w = fixture();
        let mut now = 0.0;
        at(&mut w, 2, 0);
        w.camera = w.station_pose(&w.station).unwrap();
        let id = dialog(&mut w, now);
        assert_eq!(w.rooms[1].desks.len(), 4, "a desk of its own at first");
        let out = w.set_parent(id, Some(ScreenId(3)), now);
        assert_eq!(acme(&w), [vec![1], vec![2], vec![3, id.0]]);
        assert!(w.is_loose(id));
        assert_eq!(w.stack_len(1, 2), 1);
        assert!(matches!(out[0], Out::Configure { size: Some(s), .. } if s == Vec2::ZERO));
        settle(&mut w, &mut now);
        assert_eq!(
            w.selected(),
            Some(ScreenId(3)),
            "the station is the mother's"
        );
        let mother = w.screen_place(1, 2, 0);
        let (r, d, k) = w.locate(id).unwrap();
        let p = w.screen_place(r, d, k);
        // 5 cm toward the camera, centred
        assert!((mother.center.distance(p.center) - DIALOG_FRONT).abs() < 1e-5);
        assert!((p.center.x - mother.center.x).abs() < 1e-5);
        assert!((p.center.y - mother.center.y).abs() < 1e-5);
        assert!(p.center.distance(w.camera.pos) < mother.center.distance(w.camera.pos));
        assert_eq!(w.screen_logical(r, d, k), DIALOG_DEFAULT);
        // its mother's desk keeps the stack's size
        assert_eq!(w.configure_size(Width::Half, w.stack_len(1, 2)).y, 720.0);
    }

    #[test]
    fn a_dialog_over_foco_is_the_centred_target_and_goes_back() {
        let mut w = fixture();
        let mut now = 0.0;
        w.command(Cmd::Focus, now);
        settle(&mut w, &mut now);
        let id = dialog(&mut w, now);
        let out = w.set_parent(id, Some(ScreenId(1)), now);
        assert!(out.contains(&Out::KeyboardEnter(id)), "{out:?}");
        settle(&mut w, &mut now);
        w.set_size(id, Vec2::new(400.0, 300.0));
        let (f, o) = w.focus_2d().unwrap();
        assert_eq!(f, id);
        // centred on the output
        let c = o.origin + o.size / 2.0;
        assert!(
            (c - Vec2::new(640.0, 360.0)).abs().max_element() < 1.0,
            "{c}"
        );
        assert_eq!(o.size, Vec2::new(400.0, 300.0));
        // it closes: FOCO goes back to the mother
        let out = w.remove_screen(id, now);
        assert!(out.contains(&Out::KeyboardEnter(ScreenId(1))), "{out:?}");
        settle(&mut w, &mut now);
        assert_eq!(w.focus_2d().unwrap().0, ScreenId(1));
        assert_eq!(w.mode().label(), "FOCO");
    }

    #[test]
    fn a_modal_dialog_blocks_the_mother() {
        let mut w = fixture();
        let id = dialog(&mut w, 0.0);
        w.set_parent(id, Some(ScreenId(1)), 0.0);
        at(&mut w, 0, 0);
        // not modal: FOCO on the mother is allowed
        assert_eq!(
            w.command(Cmd::Focus, 0.0),
            [Out::KeyboardEnter(ScreenId(1))]
        );
        w.command(Cmd::Back, 0.0);
        w.set_modal(id, true, 0.0);
        assert_eq!(w.modal_child(ScreenId(1)), Some(id));
        assert_eq!(w.command(Cmd::Focus, 0.0), [Out::KeyboardEnter(id)]);
        // a modal hint while the mother is in FOCO moves the keyboard
        let mut w = fixture();
        w.command(Cmd::Focus, 0.0);
        let id = dialog(&mut w, 0.0);
        w.wm.entry(id).or_default().parent = None;
        w.set_parent(id, Some(ScreenId(1)), 0.0);
        assert_eq!(w.focus_target(), Some(id));
    }

    #[test]
    fn dialogs_travel_with_their_mother() {
        let mut w = fixture();
        let id = dialog(&mut w, 0.0);
        w.set_parent(id, Some(ScreenId(1)), 0.0);
        at(&mut w, 0, 0);
        run(&mut w, "world.move_right");
        assert_eq!(acme(&w), [vec![2], vec![1, id.0], vec![3]]);
        at(&mut w, 1, 0);
        run(&mut w, "world.consume");
        assert_eq!(acme(&w), [vec![2, 1, id.0], vec![3]]);
        assert_eq!(w.stack_len(1, 0), 2);
        // unparented: a desk of its own again
        w.set_parent(id, None, 0.0);
        assert!(!w.is_loose(id));
        assert_eq!(w.rooms[1].desks.len(), 3);
    }

    #[test]
    fn activation_with_and_without_a_token() {
        let mut w = fixture();
        let mut now = 0.0;
        w.activate(ScreenId(3), true, now);
        settle(&mut w, &mut now);
        assert_eq!(w.selected(), Some(ScreenId(3)));
        assert!(w.attention(now).is_empty());
        w.activate(ScreenId(1), false, now);
        assert_eq!(w.selected(), Some(ScreenId(3)), "no token: nothing moves");
        assert_eq!(w.attention(now), [(ScreenId(1), now + ATTENTION_S)]);
        assert!(w.attention(now + ATTENTION_S + 0.01).is_empty());
        assert!(w.activate(ScreenId(77), true, now).is_empty());
    }

    #[test]
    fn placing_a_new_window() {
        let mut w = fixture();
        let (id, _) = w.add_screen("acme.term", "", 0.0);
        assert_eq!(w.rooms[1].desks[1].screens, [id], "right of the station");
        let p = Place {
            room: Some("bravo".into()),
            desk: Some(DeskAt::Last),
            width: Some(Width::TwoThirds),
        };
        let out = w.place_new(id, &p, 0.0);
        assert_eq!(sizes(&out), [(id.0, Vec2::new(837.0, 720.0))]);
        let names: Vec<_> = w.rooms.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(
            names,
            [RECEPTION, "acme", "bravo", MY_ROOM],
            "made the room"
        );
        assert_eq!(w.rooms[2].desks[0].screens, [id]);
        assert_eq!(w.rooms[2].desks[0].width, Width::TwoThirds);
        assert_eq!(w.selected(), Some(id));
        assert_eq!(acme(&w).len(), 3, "its first desk went");
        assert!(w.place_new(id, &p, 0.0).is_empty(), "only once");
        // an index in the current room
        let (id2, _) = w.add_screen("demo", "", 0.0);
        w.place_new(
            id2,
            &Place {
                desk: Some(DeskAt::Index(0)),
                ..Place::default()
            },
            0.0,
        );
        assert_eq!(w.rooms[2].desks[0].screens, [id2]);
        assert_eq!(
            w.place_of(id),
            Some(Place {
                room: Some("bravo".into()),
                desk: Some(DeskAt::Index(1)),
                width: Some(Width::TwoThirds)
            })
        );
    }

    #[test]
    fn snapshot_has_rooms_desks_and_places() {
        let mut w = fixture();
        w.room_new("bravo", false);
        let id = dialog(&mut w, 0.0);
        w.set_parent(id, Some(ScreenId(1)), 0.0);
        at(&mut w, 2, 0);
        let f = w.snapshot();
        assert_eq!(f.version, VERSION);
        let names: Vec<_> = f.rooms.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, [RECEPTION, "acme", "bravo", MY_ROOM]);
        assert_eq!(f.rooms[2].kind, RoomKind::Free);
        assert_eq!(f.desks.len(), 4);
        assert_eq!(
            f.desks[1].screens,
            ["acme.1"],
            "the dialog is not a stack screen"
        );
        assert!(f.desks[2].cat);
        assert_eq!(f.screens.len(), 3, "dialogs are not kept");
        assert_eq!(
            f.station,
            Some(StationRec::Desk {
                room: "acme".into(),
                desk: 2,
                slot: 0
            })
        );
        let text = nkw_core::persist::to_string(&f);
        assert_eq!(nkw_core::persist::parse(&text).unwrap(), f);
    }

    #[test]
    fn generation_counts_changes() {
        let mut w = fixture();
        let g = w.generation();
        run(&mut w, "world.wider");
        assert!(w.generation() > g);
        let g = w.generation();
        w.add_screen("x", "", 0.0);
        assert!(w.generation() > g);
    }

    #[test]
    fn walking_commands_hold_keys_and_back_decides_by_mode() {
        let mut w = fixture();
        let mut now = 0.0;
        w.command(Cmd::Walk, now);
        let start = w.camera;
        run(&mut w, "world.back");
        assert!(w.mode().walking(), "world.back in ANDAR is a step back");
        for _ in 0..20 {
            now += 1.0 / 60.0;
            w.tick(now);
        }
        let moved = w.camera.pos - start.pos;
        assert!(
            moved.dot(start.forward()) < -0.01,
            "walked backward: {moved}"
        );
        assert!(w.walk_key(0x73, false), "s released");
        run(&mut w, "world.forward");
        assert!(w.walking.as_ref().unwrap().keys.forward);
        w.walking.as_mut().unwrap().walker.pitch = 0.3;
        run(&mut w, "world.recenter");
        assert_eq!(w.walking.as_ref().unwrap().walker.pitch, 0.0);
        run(&mut w, "world.walk_exit");
        assert_eq!(w.mode().base, Base::Nav);
        // in FOCO world.back leaves it
        w.command(Cmd::Focus, now);
        run(&mut w, "world.back");
        assert_eq!(w.mode().label(), "NAVEGAR");
    }

    #[test]
    fn talk_hint_palette_through_the_machine() {
        let mut w = fixture();
        at(&mut w, 1, 0);
        let out = run(&mut w, "world.talk");
        assert!(out.contains(&Out::Effect(nkw_core::Effect::FrameFace(CatId(9)))));
        assert_eq!(w.mode().label(), "CONVERSA");
        assert_eq!(
            run(&mut w, "world.talk_next"),
            [Out::Effect(nkw_core::Effect::TalkNext)]
        );
        let out = run(&mut w, "world.talk_sit");
        assert!(
            out.contains(&Out::Effect(nkw_core::Effect::OpenPane(CatId(9)))),
            "{out:?}"
        );
        assert_eq!(w.mode().label(), "FOCO");
        run(&mut w, "world.back");
        run(&mut w, "world.hint");
        assert_eq!(w.mode().label(), "SALTO");
        run(&mut w, "world.hint_cancel");
        run(&mut w, "world.palette");
        assert_eq!(w.mode().label(), "COMANDOS");
        run(&mut w, "world.palette_cancel");
        assert_eq!(w.mode().label(), "NAVEGAR");
        // no cat at the station: no talk
        at(&mut w, 0, 0);
        assert!(run(&mut w, "world.talk").is_empty());
        // focus on a cat's desk without a screen: its terminal
        let mut w = fixture();
        w.rooms[1].desks[1].screens.clear();
        at(&mut w, 1, 0);
        let out = run(&mut w, "world.focus");
        assert!(
            out.contains(&Out::Effect(nkw_core::Effect::OpenPane(CatId(9)))),
            "{out:?}"
        );
    }

    #[test]
    fn every_command_name_round_trips() {
        let args = BTreeMap::from([
            ("room".to_string(), "acme".to_string()),
            ("name".to_string(), "bravo".to_string()),
        ]);
        for (name, title) in NAMES {
            assert!(!title.is_empty(), "{name}");
            let c = Cmd::from_name(name, &args).unwrap_or_else(|| panic!("{name}"));
            let (n, a) = c.name();
            assert_eq!(Cmd::from_name(n, &a), Some(c), "{name}");
            assert!(info(name).is_some());
        }
        assert_eq!(NAMES.len(), COMMANDS.len());
        assert!(info("world.summon").is_none(), "the cats' command");
    }
}
