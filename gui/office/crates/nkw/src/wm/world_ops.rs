//! The window manager's operations on the [`World`] (§4.4–§4.6, phase 1):
//! the strip ops and presets, stacks taller than three, air screens,
//! dialogs on the mother's desk, fullscreen as a TV, maximize, activation,
//! placement of new windows and the snapshot for `world.json`.
//!
//! A child module of `world` (so it reaches the World's private state);
//! pure like it. Screens that are not part of a stack (dialogs, screens in
//! the air) stay in their desk's `screens`, *after* the stack ("loose"), so
//! the scene draws every screen of a desk through `World::screen_place`.

use std::path::{Path, PathBuf};

use glam::{Vec2, Vec3};
use nkw_core::arrange::{self, AirPose, Grab, Grip};
use nkw_core::carry;
use nkw_core::collide;
use nkw_core::dock::{self, DockSlot};
use nkw_core::history;
use nkw_core::layout::{self, FOV_Y};
use nkw_core::layouts;
use nkw_core::magnet::{self, Strength};
use nkw_core::persist::{
    DeskAt, DeskRec, Place, RoomKind, RoomRec, ScreenRec, StationRec, VERSION, WorldFile,
};
use nkw_core::pointer;
use nkw_core::strip::{self, Desk, Dir, Station, Width};
use nkw_core::{
    Action, ClusterId, FrameId, Kind, Pose, RoomId, ScreenId, ScreenPlace, Target, Time, Want,
    Wish, onetoone,
};

use super::{
    ArrangeSnap, ArrangeTarget, Arranging, Cluster, Cmd, FramePose, FrameRec, MY_ROOM, Out,
    RECEPTION, World,
};

/// `d_px.x` → `yaw` radians while `alt`+dragging moves a screen by mouse
/// (plan §2.3): a middling, not-configurable-yet sensitivity (no `[mouse]`
/// key exists for it, unlike `drag_threshold_px`).
const ROTATE_RAD_PER_PX: f32 = 0.01;

/// A live mouse drag (Move/Resize/Lasso), the mouse-only counterpart of
/// ARRUMAR's keyboard nudges (plan §2.3/D15): it rides on `World::arrange`
/// (started by `World::arrange_start_at`, below) for the undo snapshot and
/// the origin slot, so `DragCancel`/`DragEnd` just call the already-existing
/// `arrange_revert`/`arrange_commit`; only the extra geometry a mouse drag
/// needs — the grab offset/depth (`nkw_core::arrange::Grab`), the resize
/// anchor — lives here.
#[derive(Clone, Copy, Debug)]
pub(super) enum PointerDrag {
    Move {
        grab: Grab,
        /// the pointer's last position (for `alt`-drag's incremental `yaw`)
        last_at: Vec2,
    },
    Resize {
        grip: Grip,
        start_logical: Vec2,
        start_at: Vec2,
    },
    Lasso {
        start_at: Vec2,
    },
}

/// A `ScreenId` out of a pointer `Hit`, when there is one.
fn screen_of_hit(hit: pointer::Hit) -> Option<ScreenId> {
    match hit {
        pointer::Hit::Screen(id) | pointer::Hit::Dock { screen: id, .. } => Some(id),
        _ => None,
    }
}

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
    Run,
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
    /// in the air against a wall (dropped from the hand)
    pub wall: bool,
    /// visual scale in the air, plan §2.1 (`0.0` is the sentinel for "unset,
    /// 1×": `ScreenState::default()` must stay `Default`-derivable)
    pub scale: f32,
    /// ancorada na vista (plan §2.7); `None` = not docked. `render::layers`
    /// feeds this slot through `dock::layout` into a `Layer::Dock`, and
    /// `scene::draw_list_into` skips the screen's 3D quad while it is set.
    pub dock: Option<nkw_core::dock::DockSlot>,
}

impl ScreenState {
    /// The visual scale, with the `0.0` sentinel resolved to 1×.
    pub fn air_scale(&self) -> f32 {
        if self.scale > 0.0 { self.scale } else { 1.0 }
    }
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
    m("world.run", "correr", &["shift"]),
    m("world.recenter", "horizonte", &["c"]),
    m("world.grab", "pegar/soltar a tela", &["g"]),
    m("world.nearer", "tela na mão: mais perto", &["["]),
    m("world.farther", "tela na mão: mais longe", &["]"]),
    c("world.library", "biblioteca", &["b"]),
    CmdInfo {
        name: "world.walk_set",
        title: "ajuste do ANDAR (não fica salvo)",
        keys: &[],
        args: &[("key", "chave"), ("value", "valor")],
        palette: true,
    },
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
        self.carrying() == Some(id)
            || self
                .wm
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
        let st = self.screen_state(id);
        Some(Place {
            room: Some(self.rooms[r].name.clone()),
            desk: Some(DeskAt::Index(d)),
            width: Some(self.rooms[r].desks[d].width),
            air: st
                .air
                .map(|p| nkw_core::persist::Air::new(p.center.to_array(), p.yaw, st.wall)),
            ..Place::default()
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
                    tabbed: false,
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
            // the WM's flag (`world.tour_done`), set on its copy
            tour_done: false,
            ..WorldFile::default()
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
        if let Some(p) = self.rooms[room].desks[desk]
            .screens
            .get(k)
            .and_then(|id| self.hand_place(*id))
        {
            return p;
        }
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
        if let Some(v) = self.hand_logical(id) {
            return Some(v);
        }
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
                    self.hold(Hold::Back, true);
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
            Cmd::Talk => match self.station_cat().or_else(|| self.cat_in_view()) {
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
            // the ARRUMAR context reuses these two names for the arranged
            // screen's logical size instead of the desk's width preset
            // (table.rs/world.go bind them in both `normal.world` and
            // `world.arrange`)
            Cmd::Narrower if self.arrange.is_some() => {
                self.arrange_resize(|cur, u| arrange::step_width(cur, u, false), now)
            }
            Cmd::Narrower => self.op_width(Width::narrower, now),
            Cmd::Wider if self.arrange.is_some() => {
                self.arrange_resize(|cur, u| arrange::step_width(cur, u, true), now)
            }
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
            // standing at an empty frame, `Q` removes it (plan §2.2); else
            // it is the screen's close, as before
            Cmd::Close if matches!(self.station, Station::Frame(_)) => {
                let Station::Frame(id) = self.station else {
                    unreachable!()
                };
                self.frames.remove(&id);
                self.station = Station::Free(self.camera);
                self.changed(now, Some("moldura removida".into()));
                Vec::new()
            }
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
            Cmd::Hold(h, down) => {
                self.hold(h, down);
                Vec::new()
            }
            Cmd::Grab => self.grab(now),
            // `[` `]`: ANDAR's hand, or ARRUMAR's depth when it is on
            Cmd::Nearer if self.arrange.is_some() => {
                let cam = self.camera;
                self.arrange_touch(
                    "mais perto",
                    move |p| arrange::nearer(&cam, p, arrange::DEPTH_STEP),
                    now,
                )
            }
            Cmd::Nearer => self.hand_step(-1.0, now),
            Cmd::Farther if self.arrange.is_some() => {
                let cam = self.camera;
                self.arrange_touch(
                    "mais longe",
                    move |p| arrange::nearer(&cam, p, -arrange::DEPTH_STEP),
                    now,
                )
            }
            Cmd::Farther => self.hand_step(1.0, now),
            Cmd::Library { go } => self.library(go, now),
            Cmd::Recenter => {
                if let Some(w) = self.walking.as_mut() {
                    w.walker.pitch = 0.0;
                }
                Vec::new()
            }

            // --- wave 5: screens in space ---
            Cmd::NewScreen { at } => {
                let mut out = self.make_frame(at, now);
                out.extend(self.action(Action::Palette, now));
                out
            }
            Cmd::Frame { at } => self.make_frame(at, now),
            Cmd::TerminalAir => self.terminal_air(now),
            Cmd::Arrange => self.action(Action::Arrange, now),
            Cmd::ArrangeDone => self.action(Action::ArrangeDone, now),
            Cmd::ArrangeCancel => self.action(Action::Back, now),
            Cmd::ArrangeNext => self.arrange_step(true, now),
            Cmd::ArrangePrev => self.arrange_step(false, now),
            Cmd::Nudge(dir, step) => {
                let cam = self.camera;
                self.arrange_touch(
                    "mover",
                    move |p| arrange::nudge(&cam, p, dir, step.metres()),
                    now,
                )
            }
            Cmd::NearerBig => {
                let cam = self.camera;
                self.arrange_touch(
                    "mais perto",
                    move |p| arrange::nearer(&cam, p, arrange::DEPTH_STEP_BIG),
                    now,
                )
            }
            Cmd::FartherBig => {
                let cam = self.camera;
                self.arrange_touch(
                    "mais longe",
                    move |p| arrange::nearer(&cam, p, -arrange::DEPTH_STEP_BIG),
                    now,
                )
            }
            Cmd::YawLeft { fine } => {
                let step = if fine {
                    arrange::YAW_STEP_FINE
                } else {
                    arrange::YAW_STEP
                };
                self.arrange_touch("girar", move |p| arrange::yaw_by(p, -step), now)
            }
            Cmd::YawRight { fine } => {
                let step = if fine {
                    arrange::YAW_STEP_FINE
                } else {
                    arrange::YAW_STEP
                };
                self.arrange_touch("girar", move |p| arrange::yaw_by(p, step), now)
            }
            Cmd::PitchUp => self.arrange_touch(
                "inclinar",
                |p| arrange::pitch_by(p, arrange::PITCH_STEP),
                now,
            ),
            Cmd::PitchDown => self.arrange_touch(
                "inclinar",
                |p| arrange::pitch_by(p, -arrange::PITCH_STEP),
                now,
            ),
            Cmd::Shorter => self.arrange_resize(|cur, u| arrange::step_height(cur, u, false), now),
            Cmd::Taller => self.arrange_resize(|cur, u| arrange::step_height(cur, u, true), now),
            Cmd::SizePreset(n) => {
                self.arrange_resize(move |cur, u| arrange::size_preset(n, cur, u), now)
            }
            Cmd::ReadDistance => {
                let (cam, out) = (self.camera, self.out);
                self.arrange_touch(
                    "distância de leitura",
                    move |p| arrange::read_distance(&cam, &out, FOV_Y, p),
                    now,
                )
            }
            Cmd::ScaleDown => self.arrange_touch("escala", |p| arrange::rescale(p, -1), now),
            Cmd::ScaleUp => self.arrange_touch("escala", |p| arrange::rescale(p, 1), now),
            Cmd::ScaleReset => self.arrange_touch("escala", arrange::scale_reset, now),
            Cmd::SnapToggle => {
                if let Some(a) = self.arrange.as_mut() {
                    a.strong = !a.strong;
                }
                Vec::new()
            }
            Cmd::MagnetsToggle => {
                if let Some(a) = self.arrange.as_mut() {
                    a.magnets_off = !a.magnets_off;
                }
                Vec::new()
            }
            Cmd::ToTarget(target) => {
                if let Some(a) = self.arrange.as_mut() {
                    a.target = target;
                    self.pending.push(Wish::world(Want::Once));
                }
                Vec::new()
            }
            Cmd::SelectToggle => {
                let screen = self
                    .arrange
                    .as_ref()
                    .map(|a| a.screen)
                    .or_else(|| self.selected());
                if let Some(id) = screen
                    && !self.arrange_selection.remove(&id)
                {
                    self.arrange_selection.insert(id);
                }
                self.pending.push(Wish::world(Want::Once));
                Vec::new()
            }
            Cmd::SelectClear => {
                self.arrange_selection.clear();
                self.pending.push(Wish::world(Want::Once));
                Vec::new()
            }
            Cmd::MakeCluster(name) => self.make_cluster(name, now),
            Cmd::Uncluster => self.uncluster(now),
            Cmd::Dock => self.op_dock(now),
            Cmd::LayoutSave(name) => self.layout_save(name, now),
            Cmd::LayoutLoad(name) => self.layout_load(name, now),
            Cmd::LayoutDelete(name) => self.layout_delete(name, now),
            Cmd::LayoutRename { name, to } => self.layout_rename(name, to, now),
            Cmd::Undo => self.history_undo(now),
            Cmd::Redo => self.history_redo(now),
            Cmd::MoveFirst => self.op_move_edge(true, now),
            Cmd::MoveLast => self.op_move_edge(false, now),

            // phase 0's commands are the World's own (`World::command`)
            _ => Vec::new(),
        }
    }

    fn hold(&mut self, h: Hold, down: bool) {
        if self.walking.is_none() {
            return;
        }
        if matches!(h, Hold::TurnLeft | Hold::TurnRight)
            && self.walk_params.turn == nkw_core::TurnMode::Snap
        {
            if down {
                self.turn_press(if h == Hold::TurnLeft { -1.0 } else { 1.0 });
            }
            return;
        }
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
            Hold::Run => &mut k.run,
        } = down;
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
    pub(super) fn detach(&mut self, r: usize, d: usize, id: ScreenId) -> Vec<ScreenId> {
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
    pub(super) fn attach(&mut self, r: usize, d: usize, moved: &[ScreenId]) -> usize {
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
    pub(super) fn drop_if_empty(&mut self, r: usize, d: usize) -> bool {
        let desk = &self.rooms[r].desks[d];
        if desk.screens.is_empty() && desk.cat.is_none() {
            self.rooms[r].desks.remove(d);
            true
        } else {
            false
        }
    }

    /// Configure every stack screen of a desk to its preset size.
    pub(super) fn configure_desk(&self, r: usize, d: usize) -> Vec<Out> {
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

    pub(super) fn set_station_desk(&mut self, r: usize, d: usize, slot: usize) {
        self.station = Station::Desk {
            room: self.rooms[r].id,
            desk: d,
            slot,
        };
    }

    /// Like [`Self::changed`], but never glides the camera: for a drag or
    /// ARRUMAR's own commit/cancel (plan §2.1, "a câmera nunca se move por
    /// causa de um arrasto" — going to the screen afterwards is `h`/`l`,
    /// SALTO or ANDAR, not an automatic glide).
    pub(super) fn changed_still(&mut self, now: Time, note: Option<String>) {
        self.layout_gen += 1;
        if let Some(n) = note {
            self.note = Some((now, n));
        }
        self.pending.push(Wish::world(Want::Once));
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
        if let Some(out) = self.hand_width(f, now) {
            return out;
        }
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
        self.pin_loose(r, d, id, place);
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
        self.unpin_at(r, d, id, n);
        let out = self.configure_desk(r, d);
        self.set_station_desk(r, d, n);
        self.changed(now, Some(width_label(self.rooms[r].desks[d].width).into()));
        out
    }

    /// Move `id` (already at `(r, d, k)`'s desk) into the air at `place`,
    /// loose at the end of its desk's screen list (loose screens are always
    /// last: `place_depth`'s stack-index math assumes it).
    pub(super) fn pin_loose(&mut self, r: usize, d: usize, id: ScreenId, place: ScreenPlace) {
        self.wm.entry(id).or_default().air = Some(place);
        let desk = &mut self.rooms[r].desks[d];
        desk.screens.retain(|s| *s != id);
        desk.screens.push(id);
    }

    /// The inverse of [`Self::pin_loose`]: `id` back into desk `(r, d)` at
    /// `slot` (clamped), no longer air. `unpin` uses the end of the stack;
    /// ARRUMAR's `esc` uses the screen's exact original slot.
    pub(super) fn unpin_at(&mut self, r: usize, d: usize, id: ScreenId, slot: usize) {
        self.wm.entry(id).or_default().air = None;
        self.wm.entry(id).or_default().wall = false;
        for desk in &mut self.rooms[r].desks {
            desk.screens.retain(|s| *s != id);
        }
        let slot = slot.min(self.rooms[r].desks[d].screens.len());
        self.rooms[r].desks[d].screens.insert(slot, id);
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
        // back in the air (or on its wall) where it was left
        if let Some(a) = place.air {
            let [x, y, z] = a.pos();
            let st = self.wm.entry(id).or_default();
            st.air = Some(ScreenPlace {
                center: Vec3::new(x, y, z),
                yaw: a.yaw(),
                pitch: 0.0,
            });
            st.wall = a.wall;
        }
        let st = if place.air.is_some() {
            Station::Air(id)
        } else {
            Station::Desk {
                room: self.rooms[t].id,
                desk: at,
                slot: 0,
            }
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

    // ---------------------------------------------------------------
    // wave 5: screens in space (plan docs/plano-mundo-wm.md §2)

    /// `N` / `ctrl+n`: an armed, empty frame where the camera points (plan
    /// §2.2): a ray from the mira finds the current room's wall first, then
    /// a desk's top, else 1.2 m ahead of the camera, in that order
    /// (`nkw_core::spawn::frame_point`). The frame itself is real
    /// (selectable, removable with `Q`, persists in `self.frames`); a
    /// spawned window landing in it automatically is the placement
    /// pipeline's job (the `wm` plugin crate), not wired here.
    fn make_frame(&mut self, at: Option<FramePose>, now: Time) -> Vec<Out> {
        let pose = at.unwrap_or_else(|| {
            let room = self.current_room();
            let room_yaw = layout::room_frame(&self.slots()[room]).yaw;
            let walls: Vec<nkw_core::spawn::WallQuad> = self
                .room_walls(room)
                .into_iter()
                .map(|w| nkw_core::spawn::WallQuad {
                    a: w.a,
                    b: w.b,
                    normal: w.normal,
                    height: nkw_gl::shell::WALL_H,
                })
                .collect();
            // no window yet, so no real footprint: the plan's generic desk
            // size (elbow room for a 0-wide screen)
            let footprint = Vec2::new(layout::desk_width(0.0), layout::DESK_DEPTH);
            let desks: Vec<nkw_core::spawn::DeskQuad> = self
                .room_desk_slots(room)
                .into_iter()
                .map(|d| nkw_core::spawn::DeskQuad {
                    center: d.pos,
                    yaw: room_yaw,
                    size: footprint,
                })
                .collect();
            let p = nkw_core::spawn::frame_point(&self.camera, &walls, &desks);
            FramePose::from_pose(p.pos, p.yaw)
        });
        let id = FrameId(self.next_frame);
        self.next_frame += 1;
        let room = self.rooms[self.current_room()].id;
        self.frames.insert(
            id,
            FrameRec {
                room,
                pose: ScreenPlace {
                    center: pose.pos_m(),
                    yaw: pose.yaw_rad(),
                    pitch: 0.0,
                },
            },
        );
        self.go_to(Station::Frame(id), now, false);
        self.changed(now, Some("moldura".into()));
        Vec::new()
    }

    /// `v` / `Effect::ArrangeStart`: the selected screen becomes the subject
    /// of ARRUMAR. A desk screen is pinned loose right away (like
    /// `world.pin`), so its working pose is live from the first key; `esc`
    /// (`arrange_revert`) puts it back exactly, including its original slot.
    pub(super) fn arrange_start(&mut self, _now: Time) -> Vec<Out> {
        let Some(screen) = self.selected() else {
            return Vec::new();
        };
        if self.arrange.as_ref().is_some_and(|a| a.screen == screen) {
            return Vec::new();
        }
        let st = self.screen_state(screen);
        let origin_slot = self
            .locate(screen)
            .filter(|_| !self.is_loose(screen))
            .map(|(r, d, k)| (self.rooms[r].id, d, k));
        let idx = origin_slot.and_then(|(room, d, k)| self.room_index(room).map(|r| (r, d, k)));
        let place = st.air.unwrap_or_else(|| {
            idx.map(|(r, d, k)| self.screen_place(r, d, k))
                .unwrap_or_else(|| {
                    // no desk slot either (should not happen: `selected()`
                    // only ever returns a desk or air station) — fall back
                    // to right in front of the camera, same as a new frame
                    let p = carry::hand_pose(&self.camera, carry::HAND_DEFAULT);
                    ScreenPlace {
                        center: p.pos,
                        yaw: p.yaw,
                        pitch: 0.0,
                    }
                })
        });
        let mut out = Vec::new();
        if let Some((r, d, k)) = idx {
            let size = self.screen_logical(r, d, k);
            if let Some(s) = self.screens.get_mut(&screen)
                && (s.size.x <= 0.0 || s.size.y <= 0.0)
            {
                s.size = size;
            }
            self.pin_loose(r, d, screen, place);
            out = self.configure_desk(r, d);
        }
        let pose = AirPose {
            pos: place.center,
            yaw: place.yaw,
            pitch: place.pitch,
            scale: st.air_scale(),
        };
        let target = match (st.air.is_some(), st.wall) {
            (true, true) => ArrangeTarget::Wall,
            (true, false) => ArrangeTarget::Air,
            (false, _) => ArrangeTarget::Desk,
        };
        self.arrange = Some(Arranging {
            screen,
            origin_slot,
            origin_state: st,
            pose,
            target,
            strong: false,
            magnets_off: false,
            mark: self.history.mark(),
        });
        self.station = Station::Air(screen);
        self.layout_gen += 1;
        self.pending.push(Wish::world(Want::Once));
        out
    }

    /// `enter` / `Effect::ArrangeCommit`: settle the arranged screen at
    /// `target`, with the magnets if any are armed.
    pub(super) fn arrange_commit(&mut self, now: Time) -> Vec<Out> {
        let Some(a) = self.arrange.take() else {
            return Vec::new();
        };
        let out = match a.target {
            ArrangeTarget::Air | ArrangeTarget::Wall => {
                let room = self.current_room();
                let targets = magnet::Targets {
                    walls: self.room_walls(room),
                    desks: self.room_desk_slots(room),
                    screens: Vec::new(),
                    eye: self.camera,
                    one_to_one_m: onetoone::distance(self.out.height_px, 1.0, FOV_Y),
                    dock: None,
                    grid_origin: Vec3::ZERO,
                    eye_height: self.camera.pos.y,
                };
                let strength = if a.magnets_off {
                    Strength::Off
                } else if a.strong {
                    Strength::Strong
                } else {
                    Strength::Soft
                };
                let logical = self
                    .loose_logical(a.screen)
                    .unwrap_or_else(|| self.usable());
                let size_m = logical * onetoone::M_PER_PX * a.pose.scale;
                let (pose, _snap, _guides) = magnet::snap(&a.pose, size_m, &targets, strength);
                let st = self.wm.entry(a.screen).or_default();
                st.air = Some(pose.place());
                st.wall = a.target == ArrangeTarget::Wall;
                st.scale = pose.scale;
                // station bookkeeping only: the camera never moves because
                // of an arrange/drag (plan §2.1) — going to the screen is
                // h/l, SALTO or ANDAR, like any other station
                self.station = Station::Air(a.screen);
                Vec::new()
            }
            ArrangeTarget::Desk => self.send_to_nearest_desk(a.screen, a.pose.pos),
        };
        self.changed_still(now, Some("arrumado".into()));
        out
    }

    /// `esc` / `Effect::ArrangeRevert`: undo back to the mark. This always
    /// restores `a.origin_state` (captured at `arrange_start`, before
    /// `pin_loose` touched the screen) rather than an undone entry's
    /// `before`: `arrange_touch`'s first call snapshots the *already
    /// pinned-loose* state as its `before` (that pin happens so the working
    /// pose is live from the first key/drag event), so that snapshot is not
    /// the true pre-arrange state for a screen that started on a desk. We
    /// still call `undo_to` for its side effect — dropping this session's
    /// entries so a later `Cmd::Undo` does not replay these already-reverted
    /// micro-steps — we just do not use what it returns.
    pub(super) fn arrange_revert(&mut self, now: Time) -> Vec<Out> {
        let Some(a) = self.arrange.take() else {
            return Vec::new();
        };
        self.history.undo_to(a.mark);
        let state = a.origin_state;
        let mut out = Vec::new();
        if let Some((room, d, slot)) = a.origin_slot
            && let Some(r) = self.room_index(room)
        {
            self.unpin_at(r, d, a.screen, slot);
            self.wm.insert(a.screen, state);
            out = self.configure_desk(r, d);
            self.set_station_desk(r, d, slot);
        } else {
            self.wm.insert(a.screen, state);
            if let Some(st) = self.station_of(a.screen) {
                self.station = st;
            }
        }
        self.changed_still(now, Some("desfeito".into()));
        out
    }

    /// `tab` / `shift+tab`: commit the current screen and start arranging
    /// the next/previous one of the room, without leaving ARRUMAR.
    fn arrange_step(&mut self, next: bool, now: Time) -> Vec<Out> {
        let Some(a) = self.arrange.as_ref() else {
            return Vec::new();
        };
        let room = self.current_room();
        let mut ids: Vec<ScreenId> = self.rooms[room]
            .desks
            .iter()
            .flat_map(|d| d.screens.iter().copied())
            .collect();
        ids.sort();
        ids.dedup();
        if ids.len() < 2 {
            return Vec::new();
        }
        let Some(i) = ids.iter().position(|id| *id == a.screen) else {
            return Vec::new();
        };
        let j = if next {
            (i + 1) % ids.len()
        } else {
            (i + ids.len() - 1) % ids.len()
        };
        let target = ids[j];
        let mut out = self.arrange_commit(now);
        if let Some(st) = self.station_of(target) {
            self.station = st;
        }
        out.extend(self.arrange_start(now));
        out
    }

    /// Apply `f` to ARRUMAR's working pose (move/rotate/scale/depth), clamp
    /// it, keep it at a comfortable distance, and push an undo entry when it
    /// actually changed. A no-op outside ARRUMAR.
    fn arrange_touch(
        &mut self,
        label: &str,
        f: impl FnOnce(&AirPose) -> AirPose,
        now: Time,
    ) -> Vec<Out> {
        let Some(screen) = self.arrange.as_ref().map(|a| a.screen) else {
            return Vec::new();
        };
        let cam = self.camera;
        let before = self.wm.get(&screen).copied().unwrap_or_default();
        let Some(a) = self.arrange.as_mut() else {
            return Vec::new();
        };
        let mut pose = f(&a.pose).clamped();
        pose = arrange::comfort(&cam, pose);
        a.pose = pose;
        if a.target == ArrangeTarget::Desk {
            a.target = ArrangeTarget::Air;
        }
        let st = self.wm.entry(screen).or_default();
        st.air = Some(pose.place());
        st.scale = pose.scale;
        let after = *self.wm.get(&screen).expect("just inserted");
        if before != after {
            self.history.push(history::Entry {
                kind: history::OpKind::Move,
                label: label.to_string(),
                subjects: vec![screen.0 as u64],
                before: ArrangeSnap::from([(screen, before)]),
                after: ArrangeSnap::from([(screen, after)]),
                at: now,
            });
        }
        self.layout_gen += 1;
        self.pending.push(Wish::world(Want::Once));
        Vec::new()
    }

    /// `-` `=` `_` `+` `1`..`4` in ARRUMAR: the arranged screen's logical
    /// size (a `configure`, not a pose). Not undoable yet: the history
    /// snapshot used by `arrange_touch` is `ScreenState`, which does not
    /// carry the client's committed size.
    fn arrange_resize(&mut self, f: impl FnOnce(Vec2, Vec2) -> Vec2, _now: Time) -> Vec<Out> {
        let usable = self.usable();
        let Some(screen) = self.arrange.as_ref().map(|a| a.screen) else {
            return Vec::new();
        };
        let cur = self.loose_logical(screen).unwrap_or(usable);
        let new = f(cur, usable);
        if new == cur {
            return Vec::new();
        }
        if let Some(a) = self.arrange.as_mut()
            && a.target == ArrangeTarget::Desk
        {
            a.target = ArrangeTarget::Air;
        }
        self.layout_gen += 1;
        self.pending.push(Wish::world(Want::Once));
        vec![Out::Configure {
            screen,
            size: Some(new),
            fullscreen: None,
            maximized: None,
        }]
    }

    /// The room's back and side walls, for `to_wall` and the magnet target
    /// (plan §2.8). The hall-facing wall has a door and is left out.
    fn room_walls(&self, room: usize) -> Vec<carry::WallSeg> {
        let s = self.slots()[room];
        let (z0, z1) = match s.side {
            strip::Side::North => (
                -(super::HALL_W / 2.0 + super::ROOM_DEPTH),
                -super::HALL_W / 2.0,
            ),
            strip::Side::South => (super::HALL_W / 2.0, super::HALL_W / 2.0 + super::ROOM_DEPTH),
        };
        let (back_z, back_normal) = match s.side {
            strip::Side::North => (z0, Vec2::new(0.0, 1.0)),
            strip::Side::South => (z1, Vec2::new(0.0, -1.0)),
        };
        vec![
            carry::WallSeg {
                a: Vec2::new(s.x0, back_z),
                b: Vec2::new(s.x0 + s.len, back_z),
                normal: back_normal,
            },
            carry::WallSeg {
                a: Vec2::new(s.x0, z0),
                b: Vec2::new(s.x0, z1),
                normal: Vec2::new(1.0, 0.0),
            },
            carry::WallSeg {
                a: Vec2::new(s.x0 + s.len, z0),
                b: Vec2::new(s.x0 + s.len, z1),
                normal: Vec2::new(-1.0, 0.0),
            },
        ]
    }

    /// The room's desks as drop targets (plan §2.8, `to_desk`'s nearest
    /// search).
    fn room_desk_slots(&self, room: usize) -> Vec<carry::DeskSlot> {
        let s = self.slots()[room];
        let room_id = self.rooms[room].id;
        self.desk_local_xs(room)
            .into_iter()
            .enumerate()
            .map(|(d, x)| {
                let (pos, _) = layout::desk_root(&s, x);
                carry::DeskSlot {
                    room: room_id,
                    desk: d,
                    pos,
                }
            })
            .collect()
    }

    /// `d` (`to_desk`) on commit: the nearest desk of the current room: no
    /// desk there (an empty room) leaves it in the air, honestly, rather
    /// than pretending.
    fn send_to_nearest_desk(&mut self, screen: ScreenId, pos: Vec3) -> Vec<Out> {
        let room = self.current_room();
        let slots = self.room_desk_slots(room);
        let Some(best) = slots
            .iter()
            .min_by(|a, b| a.pos.distance(pos).total_cmp(&b.pos.distance(pos)))
            .copied()
        else {
            let st = self.wm.entry(screen).or_default();
            let yaw = st.air.map_or(0.0, |p| p.yaw);
            st.air = Some(ScreenPlace {
                center: pos,
                yaw,
                pitch: 0.0,
            });
            st.wall = false;
            // station bookkeeping only: the camera never moves because of
            // an arrange/drag (plan §2.1), same as arrange_commit's Air/Wall
            // branch
            self.station = Station::Air(screen);
            return Vec::new();
        };
        let Some(r) = self.room_index(best.room) else {
            return Vec::new();
        };
        let d = best.desk;
        let n = self.stack_len(r, d);
        self.unpin_at(r, d, screen, n);
        let out = self.configure_desk(r, d);
        self.set_station_desk(r, d, n);
        out
    }

    /// `c`: group the selection (plus the screen in hand, if any) into a
    /// named cluster. `h`/`l`/`j`/`k` do not stop at it yet (plan §2.11):
    /// this only models the membership, real enough for the phone/dump to
    /// list and for `C` to undo.
    fn make_cluster(&mut self, name: Option<ShortName>, now: Time) -> Vec<Out> {
        let current = self
            .arrange
            .as_ref()
            .map(|a| a.screen)
            .or_else(|| self.selected());
        let mut screens: Vec<ScreenId> = self.arrange_selection.iter().copied().collect();
        if let Some(id) = current
            && !screens.contains(&id)
        {
            screens.push(id);
        }
        screens.sort();
        if screens.len() < 2 {
            self.note = Some((now, "seleciona pelo menos duas telas".into()));
            self.pending.push(Wish::compose(Want::Once));
            return Vec::new();
        }
        let label = name.map(|n| n.as_str().to_string()).unwrap_or_else(|| {
            screens
                .iter()
                .filter_map(|id| self.screens.get(id))
                .map(|s| s.app_id.clone())
                .collect::<Vec<_>>()
                .join(" + ")
        });
        let id = ClusterId(self.next_cluster);
        self.next_cluster += 1;
        self.clusters.insert(
            id,
            Cluster {
                name: label.clone(),
                screens,
            },
        );
        self.arrange_selection.clear();
        self.station = Station::Cluster(id);
        self.changed(now, Some(format!("grupo: {label}")));
        Vec::new()
    }

    /// `C`: dissolve the cluster the station is at.
    fn uncluster(&mut self, now: Time) -> Vec<Out> {
        let Station::Cluster(id) = self.station else {
            return Vec::new();
        };
        let Some(c) = self.clusters.remove(&id) else {
            return Vec::new();
        };
        self.station = c
            .screens
            .first()
            .and_then(|s| self.station_of(*s))
            .unwrap_or(Station::Free(self.camera));
        self.changed(now, Some("grupo desfeito".into()));
        Vec::new()
    }

    /// `P` (NAV) / `p` (ARRUMAR): toggles docking (plan §2.7). This only
    /// records the intent and frees/claims a slot; `render::layers` is what
    /// turns `dock` into an actual `Layer::Dock` composed over the view.
    fn op_dock(&mut self, now: Time) -> Vec<Out> {
        let Some(screen) = self
            .arrange
            .as_ref()
            .map(|a| a.screen)
            .or_else(|| self.selected())
        else {
            return Vec::new();
        };
        if self.screen_state(screen).dock.is_some() {
            self.wm.entry(screen).or_default().dock = None;
            self.changed(now, Some("desancorada".into()));
            return Vec::new();
        }
        let used: std::collections::BTreeSet<DockSlot> =
            self.wm.values().filter_map(|s| s.dock).collect();
        if used.len() >= dock::DOCK_MAX {
            self.note = Some((now, "máximo de telas ancoradas".into()));
            self.pending.push(Wish::compose(Want::Once));
            return Vec::new();
        }
        const ORDER: [DockSlot; 8] = [
            DockSlot::TopLeft,
            DockSlot::Top,
            DockSlot::TopRight,
            DockSlot::Right,
            DockSlot::BottomRight,
            DockSlot::Bottom,
            DockSlot::BottomLeft,
            DockSlot::Left,
        ];
        let Some(slot) = ORDER.into_iter().find(|s| !used.contains(s)) else {
            return Vec::new();
        };
        self.wm.entry(screen).or_default().dock = Some(slot);
        self.changed(now, Some("ancorada".into()));
        Vec::new()
    }

    /// `u`: also outside ARRUMAR (plan §2.9).
    fn history_undo(&mut self, now: Time) -> Vec<Out> {
        let Some(e) = self.history.undo() else {
            return Vec::new();
        };
        let (snap, label) = (e.before.clone(), e.label.clone());
        for (id, st) in &snap {
            self.wm.insert(*id, *st);
        }
        self.changed(now, Some(format!("desfeito: {label}")));
        Vec::new()
    }

    /// `ctrl+r`.
    fn history_redo(&mut self, now: Time) -> Vec<Out> {
        let Some(e) = self.history.redo() else {
            return Vec::new();
        };
        let (snap, label) = (e.after.clone(), e.label.clone());
        for (id, st) in &snap {
            self.wm.insert(*id, *st);
        }
        self.changed(now, Some(format!("refeito: {label}")));
        Vec::new()
    }

    /// niri's `move-column-to-first/last`: the whole desk at the station
    /// moves to the front/back of its room's queue (`world.move_first` /
    /// `world.move_last`: `internal/app/world.go` and the Rust fallback
    /// keymap, `router/table.rs`, agree on `world+ctrl+home`/`world+ctrl+end`).
    fn op_move_edge(&mut self, first: bool, now: Time) -> Vec<Out> {
        let Some((r, d)) = self.station_desk() else {
            return Vec::new();
        };
        let n = self.rooms[r].desks.len();
        let target = if first { 0 } else { n - 1 };
        if d == target {
            return Vec::new();
        }
        let slot = match self.station {
            Station::Desk { slot, .. } => slot,
            _ => 0,
        };
        let desk = self.rooms[r].desks.remove(d);
        self.rooms[r].desks.insert(target, desk);
        self.set_station_desk(r, target, slot);
        self.changed(now, None);
        Vec::new()
    }

    // ---------------------------------------------------------------
    // wave 5: arrangements on disk (plan §2.6/§2.9, S32) and the aimed
    // terminal (plan §2.2/§2.12)

    /// `world.terminal_air`: a frame where the camera points (`make_frame`,
    /// untouched — another part of this wave owns its body), then a real
    /// terminal spawned into the world (`Out::Spawn`, the same primitive
    /// every `World`-originated spawn already uses; `Nkw::spawn` gives it
    /// the world's `WAYLAND_DISPLAY` and a fresh activation token). Two
    /// honest, documented gaps, not silent ones: (1) the spawned window is
    /// not wired to land inside this exact frame — `make_frame`'s own doc
    /// already says that placement is the `wm` plugin's job, not here; (2)
    /// unlike `world.terminal` this does not read `[launch] terminal`
    /// (`Config` lives on the `wm` plugin's `Wm`, out of `World`'s pure
    /// reach) — it always runs `nkw_core::rules::DEFAULT_TERMINAL` ("foot").
    fn terminal_air(&mut self, now: Time) -> Vec<Out> {
        let mut out = self.make_frame(None, now);
        out.push(Out::Spawn(vec![
            nkw_core::rules::DEFAULT_TERMINAL.to_string(),
        ]));
        out
    }

    /// `<state-dir>/layouts/<room-slug>/`, or `None` with no `--state-dir`
    /// (`--demo`, or a test that never sets `self.state_dir`).
    fn layouts_dir(&self) -> Option<PathBuf> {
        let base = self.state_dir.as_ref()?;
        let room = self.rooms[self.current_room()].name.clone();
        Some(base.join("layouts").join(layout_room_slug(&room)))
    }

    /// The open screens of `room` an arrangement can name: top-level
    /// windows only (same filter `snapshot` uses: a dialog is its mother's,
    /// never an entry of its own; an empty `app_id` has not announced
    /// itself yet).
    fn room_open(&self, room: usize) -> Vec<layouts::Open> {
        self.rooms[room]
            .desks
            .iter()
            .flat_map(|d| d.screens.iter())
            .filter_map(|id| {
                let sc = self.screens.get(id)?;
                if sc.app_id.is_empty() || self.screen_state(*id).parent.is_some() {
                    return None;
                }
                Some(layouts::Open {
                    id: *id,
                    app_id: sc.app_id.clone(),
                    title: sc.title.clone(),
                })
            })
            .collect()
    }

    /// Where `id` sits today, as a [`layouts::LayoutAt`]: docked wins (it is
    /// bookkeeping over whatever is under it); then air/wall (full fidelity:
    /// pitch and the visual scale too, which `place_of`'s own `Air` leaves
    /// at the defaults since `world.json` does not round-trip them yet);
    /// else its desk, slot and width. A screen with none of these (should
    /// not happen: every open screen is somewhere) saves nothing for it.
    fn layout_at_for(&self, id: ScreenId) -> Option<layouts::LayoutAt> {
        let st = self.screen_state(id);
        if let Some(slot) = st.dock {
            let scale_pct = pct(st.air_scale());
            return Some(layouts::LayoutAt::Dock { slot, scale_pct });
        }
        if let Some(place) = st.air {
            let mut air = nkw_core::persist::Air::new(place.center.to_array(), place.yaw, st.wall);
            air.pitch_mdeg = (place.pitch.to_degrees() * 1000.0).round() as i32;
            air.scale_pct = pct(st.air_scale());
            return Some(if st.wall {
                layouts::LayoutAt::Wall(air)
            } else {
                layouts::LayoutAt::Air(air)
            });
        }
        let (r, d, slot) = self.locate(id)?;
        Some(layouts::LayoutAt::Desk {
            desk: d,
            slot,
            width: self.rooms[r].desks[d].width,
        })
    }

    /// The current room's open screens as [`layouts::LayoutEntry`]s: a
    /// title is only carried when the room has more than one open window of
    /// that `app_id` (else a plain, title-less entry is more flexible to
    /// reload with — `layouts::match_open`'s own doc).
    fn layout_entries(&self, open: &[layouts::Open]) -> Vec<layouts::LayoutEntry> {
        let mut counts: std::collections::BTreeMap<&str, u32> = std::collections::BTreeMap::new();
        for o in open {
            *counts.entry(o.app_id.as_str()).or_default() += 1;
        }
        open.iter()
            .filter_map(|o| {
                let at = self.layout_at_for(o.id)?;
                let title = (counts[o.app_id.as_str()] > 1).then(|| o.title.clone());
                Some(layouts::LayoutEntry {
                    app_id: o.app_id.clone(),
                    title,
                    at,
                })
            })
            .collect()
    }

    /// Moves `id` to `at` (a loaded arrangement's entry, matched to it by
    /// `layouts::match_open`). `Cluster`'s group itself is not reconstructed
    /// (that is ARRUMAR's own, cross-screen geometry, elsewhere in this
    /// file): the screen just goes back to its saved pose, same as `Air`.
    fn apply_layout_at(&mut self, id: ScreenId, at: &layouts::LayoutAt, now: Time) -> Vec<Out> {
        match at {
            layouts::LayoutAt::Dock { slot, scale_pct } => {
                let st = self.wm.entry(id).or_default();
                st.dock = Some(*slot);
                st.scale = *scale_pct as f32 / 100.0;
                self.changed(now, None);
                Vec::new()
            }
            layouts::LayoutAt::Air(air) => self.apply_air(id, air, false),
            layouts::LayoutAt::Wall(air) => self.apply_air(id, air, true),
            layouts::LayoutAt::Cluster { air, .. } => self.apply_air(id, air, air.wall),
            layouts::LayoutAt::Desk { desk, slot, width } => {
                let Some((r, _, _)) = self.locate(id) else {
                    return Vec::new();
                };
                if self.rooms[r].desks.is_empty() {
                    return Vec::new();
                }
                let d = (*desk).min(self.rooms[r].desks.len() - 1);
                self.unpin_at(r, d, id, *slot);
                self.rooms[r].desks[d].width = *width;
                self.configure_desk(r, d)
            }
        }
    }

    fn apply_air(&mut self, id: ScreenId, air: &nkw_core::persist::Air, wall: bool) -> Vec<Out> {
        let Some((r, d, _)) = self.locate(id) else {
            return Vec::new();
        };
        let place = ScreenPlace {
            center: Vec3::from(air.pos()),
            yaw: air.yaw(),
            pitch: air.pitch(),
        };
        self.pin_loose(r, d, id, place);
        let out = self.configure_desk(r, d);
        let st = self.wm.entry(id).or_default();
        st.wall = wall;
        st.scale = air.scale();
        out
    }

    /// A short pt-BR note (the layout badge doubles as this file's only
    /// error channel: `World` is pure, with no `Req::Toast` of its own —
    /// `apps.rs`'s `toast()` is the `wm` plugin's equivalent).
    fn layout_toast(&mut self, now: Time, msg: String) -> Vec<Out> {
        self.note = Some((now, msg));
        self.pending.push(Wish::compose(Want::Once));
        Vec::new()
    }

    /// `s` in ARRUMAR / the palette's Arranjos: the current room's open
    /// screens, saved as `<state-dir>/layouts/<room>/<name>.json`.
    fn layout_save(&mut self, name: ShortName, now: Time) -> Vec<Out> {
        let valid = match layouts::valid_name(name.as_str()) {
            Ok(v) => v,
            Err(e) => return self.layout_toast(now, e),
        };
        let Some(dir) = self.layouts_dir() else {
            return self.layout_toast(now, "sem diretório de estado: arranjo não foi salvo".into());
        };
        let room = self.current_room();
        let room_name = self.rooms[room].name.clone();
        let open = self.room_open(room);
        let file = layouts::LayoutFile {
            version: layouts::LAYOUT_VERSION,
            name: valid.clone(),
            room: room_name,
            entries: self.layout_entries(&open),
        };
        match save_layout_file(&dir, &valid, &layouts::to_string(&file)) {
            Ok(()) => self.layout_toast(now, format!("arranjo salvo: {valid}")),
            Err(e) => self.layout_toast(
                now,
                format!("arranjo \"{valid}\": não deu para salvar ({e})"),
            ),
        }
    }

    /// `world.layout_load` / the palette's Arranjos: matches the file's
    /// entries against the current room's open screens (`match_open`) and
    /// moves what matched; what did not is reported, not forced into
    /// existence (the plan's "só arruma o que está aberto").
    fn layout_load(&mut self, name: ShortName, now: Time) -> Vec<Out> {
        let valid = match layouts::valid_name(name.as_str()) {
            Ok(v) => v,
            Err(e) => return self.layout_toast(now, e),
        };
        let Some(dir) = self.layouts_dir() else {
            return self.layout_toast(now, "sem diretório de estado: nada para carregar".into());
        };
        let text = match std::fs::read_to_string(dir.join(format!("{valid}.json"))) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return self.layout_toast(now, format!("arranjo \"{valid}\" não existe"));
            }
            Err(e) => return self.layout_toast(now, format!("arranjo \"{valid}\": {e}")),
        };
        let file = match layouts::parse(&text) {
            Ok(f) => f,
            Err(e) => return self.layout_toast(now, format!("arranjo \"{valid}\": {e}")),
        };
        let room = self.current_room();
        let open = self.room_open(room);
        let matched = layouts::match_open(&file, &open);
        let mut out = Vec::new();
        for (id, at) in &matched.moves {
            out.extend(self.apply_layout_at(*id, at, now));
        }
        let note = if matched.missing.is_empty() {
            format!("arranjo: {valid}")
        } else {
            format!(
                "arranjo: {valid} ({} de fora: {})",
                matched.missing.len(),
                matched.missing.join(", ")
            )
        };
        self.changed(now, Some(note));
        out
    }

    /// `world.layout_delete` / the palette's Arranjos.
    fn layout_delete(&mut self, name: ShortName, now: Time) -> Vec<Out> {
        let valid = match layouts::valid_name(name.as_str()) {
            Ok(v) => v,
            Err(e) => return self.layout_toast(now, e),
        };
        let Some(dir) = self.layouts_dir() else {
            return self.layout_toast(now, "sem diretório de estado: nada para apagar".into());
        };
        match std::fs::remove_file(dir.join(format!("{valid}.json"))) {
            Ok(()) => self.layout_toast(now, format!("arranjo apagado: {valid}")),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                self.layout_toast(now, format!("arranjo \"{valid}\" não existe"))
            }
            Err(e) => self.layout_toast(now, format!("arranjo \"{valid}\": {e}")),
        }
    }

    /// `world.layout_rename` / the palette's Arranjos: both names are
    /// validated; an existing `to` is never overwritten silently.
    fn layout_rename(&mut self, name: ShortName, to: ShortName, now: Time) -> Vec<Out> {
        let from = match layouts::valid_name(name.as_str()) {
            Ok(v) => v,
            Err(e) => return self.layout_toast(now, e),
        };
        let to = match layouts::valid_name(to.as_str()) {
            Ok(v) => v,
            Err(e) => return self.layout_toast(now, e),
        };
        let Some(dir) = self.layouts_dir() else {
            return self.layout_toast(now, "sem diretório de estado: nada para renomear".into());
        };
        if from == to {
            return self.layout_toast(now, format!("arranjo \"{from}\": mesmo nome"));
        }
        let to_path = dir.join(format!("{to}.json"));
        if to_path.exists() {
            return self.layout_toast(now, format!("já existe um arranjo \"{to}\""));
        }
        match std::fs::rename(dir.join(format!("{from}.json")), &to_path) {
            Ok(()) => self.layout_toast(now, format!("arranjo renomeado: {from} → {to}")),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                self.layout_toast(now, format!("arranjo \"{from}\" não existe"))
            }
            Err(e) => self.layout_toast(now, format!("arranjo \"{from}\": {e}")),
        }
    }

    // ---------------------------------------------------------------
    // wave 5: the pointer, for real mouse events (plan §2.3/§2.4, D15)

    /// The `PtrMode` of `nkw_core::pointer` that matches the world's own
    /// mode right now: FOCO, ARRUMAR and CONVERSA/SALTO/COMANDOS (`Ui`) come
    /// straight from the mode machine; plain ANDAR splits into `WalkAim`
    /// (mouse-look, the pointer locked to the crosshair) and `WalkFree`
    /// (`[walk] mouse_look = false`) by `walk_config`; anything else is NAV.
    pub fn pointer_mode(&self) -> pointer::PtrMode {
        if self.mode().focus().is_some() {
            return pointer::PtrMode::Focus;
        }
        if self.mode().arranging() {
            return pointer::PtrMode::Arrange;
        }
        if matches!(
            self.mode().kind(),
            Some(Kind::Talk { .. } | Kind::Hint { .. } | Kind::Palette)
        ) {
            return pointer::PtrMode::Ui;
        }
        if self.mode().walking() {
            return if self.walk_config().mouse_look {
                pointer::PtrMode::WalkAim
            } else {
                pointer::PtrMode::WalkFree
            };
        }
        pointer::PtrMode::Nav
    }

    /// Whether a mouse drag (Move/Resize/Lasso) is live right now (`nkw`'s
    /// `input.rs` uses this to let a wheel step depth/scale instead of
    /// paging rooms, and to let `esc` cancel the drag instead of whatever it
    /// would otherwise do).
    pub fn pointer_dragging(&self) -> bool {
        matches!(self.pointer.state(), pointer::State::Drag { .. })
    }

    fn pointer_view(&self) -> arrange::View {
        arrange::View {
            cam: self.camera,
            fov_y: FOV_Y,
            size_px: Vec2::new(self.out.width_px as f32, self.out.height_px as f32),
        }
    }

    /// The plan's "pick 3D" (D15 #6): a resize handle (NAV/ARRUMAR/ANDAR
    /// livre only — "alça é alça"), else the nearest screen the ray through
    /// `at` (physical px) crosses, else empty. Frames, pickables, cats,
    /// doors and books are not ray-cast here yet (the final report says so);
    /// FOCO/CONVERSA/SALTO/COMANDOS do not call this at all — `nkw`'s
    /// `input.rs` resolves `Hit::Client`/`Outside`/`Ui` itself, since only it
    /// knows about Wayland surfaces and the 2D overlay.
    pub fn pointer_hit(&self, at: Vec2) -> pointer::Hit {
        let view = self.pointer_view();
        let room = self.current_room();
        let mut screens: Vec<(ScreenId, AirPose, Vec2)> = Vec::new();
        for d in 0..self.rooms[room].desks.len() {
            for k in 0..self.rooms[room].desks[d].screens.len() {
                let id = self.rooms[room].desks[d].screens[k];
                let place = self.screen_place(room, d, k);
                let logical = self.screen_logical(room, d, k);
                let pose = AirPose {
                    pos: place.center,
                    yaw: place.yaw,
                    pitch: place.pitch,
                    scale: self.screen_state(id).air_scale(),
                };
                screens.push((id, pose, logical));
            }
        }
        if matches!(
            self.pointer_mode(),
            pointer::PtrMode::Nav | pointer::PtrMode::Arrange | pointer::PtrMode::WalkFree
        ) {
            let mut best: Option<(f32, pointer::Hit)> = None;
            for (id, pose, logical) in &screens {
                if let Some(grip) = arrange::grip_at(&view, pose, *logical, at, self.out.scale) {
                    let dist = (pose.pos - view.cam.pos).length();
                    if best.is_none_or(|(d, _)| dist < d) {
                        best = Some((dist, pointer::Hit::Handle { screen: *id, grip }));
                    }
                }
            }
            if let Some((_, hit)) = best {
                return hit;
            }
        }
        let (origin, dir) = view.ray(at);
        let mut nearest: Option<(f32, ScreenId)> = None;
        for (id, pose, logical) in &screens {
            let model = pose.model(*logical);
            if let Some(t) = collide::ray_hit_quad(origin, dir, model)
                && nearest.is_none_or(|(bt, _)| t < bt)
            {
                nearest = Some((t, *id));
            }
        }
        match nearest {
            Some((_, id)) => pointer::Hit::Screen(id),
            None => pointer::Hit::Empty,
        }
    }

    /// Feed one event to the pointer machine (`nkw_core::pointer::Machine`):
    /// a thin wrapper that supplies the mode. `nkw`'s `input.rs` is the only
    /// caller; it then passes the `PtrOut`s to `pointer_apply` (and, for
    /// `PassPress`/`PassRelease`, forwards the button to the seat itself —
    /// `World` knows nothing of Wayland clients).
    pub fn pointer_step(
        &mut self,
        ev: pointer::PtrEv,
        cfg: &pointer::Config,
        now: Time,
    ) -> Vec<pointer::PtrOut> {
        let mode = self.pointer_mode();
        self.pointer.step(ev, mode, cfg, now)
    }

    /// Turn what the pointer machine returned into world changes: every
    /// `PtrOut` that is not `nkw`'s own business (`Hover`/`Cursor` are
    /// cosmetic cursor-shape hints, not wired to a real cursor theme yet;
    /// `PassPress`/`PassRelease` are `input.rs`'s to forward).
    pub fn pointer_apply(&mut self, outs: &[pointer::PtrOut], now: Time) -> Vec<Out> {
        let mut out = Vec::new();
        for o in outs {
            match *o {
                pointer::PtrOut::Hover(_)
                | pointer::PtrOut::Cursor(_)
                | pointer::PtrOut::PassPress(_)
                | pointer::PtrOut::PassRelease(_) => {}
                pointer::PtrOut::Click {
                    button,
                    hit,
                    double,
                    ..
                } => out.extend(self.pointer_click(hit, button, double, now)),
                pointer::PtrOut::DragStart { gesture, hit, at } => {
                    out.extend(self.pointer_drag_start(gesture, hit, at, now));
                }
                pointer::PtrOut::DragMove { at, eff } => {
                    out.extend(self.pointer_drag_move(at, eff, now));
                }
                pointer::PtrOut::DragWheel { steps, scale } => {
                    out.extend(self.pointer_drag_wheel(steps, scale, now));
                }
                pointer::PtrOut::DragEnd { at, eff } => {
                    out.extend(self.pointer_drag_end(at, eff, now));
                }
                pointer::PtrOut::DragCancel => {
                    self.pointer_drag = None;
                    out.extend(self.arrange_revert(now));
                }
                pointer::PtrOut::Scale { hit, steps } => {
                    out.extend(self.pointer_scale(hit, steps, now))
                }
            }
        }
        out
    }

    /// A single click (not a drag): on a screen (or an anchored one), select
    /// it (`world.goto_screen`, like a SALTO pick); a double click also
    /// opens FOCO (plan §2.3, "clique simples continua selecionando... duplo
    /// clique = FOCO"). Lasso/handles/objects are not clickable targets.
    fn pointer_click(
        &mut self,
        hit: pointer::Hit,
        button: pointer::Button,
        double: bool,
        now: Time,
    ) -> Vec<Out> {
        if button != pointer::Button::Left {
            return Vec::new();
        }
        let Some(id) = screen_of_hit(hit) else {
            return Vec::new();
        };
        let mut out = self.command(Cmd::GotoScreen(id), now);
        if double {
            out.extend(self.command(Cmd::Focus, now));
        }
        out
    }

    /// Start (or switch) ARRUMAR's live state on `screen` without moving the
    /// station's own camera glide (plan §2.1: "a câmera nunca se move por
    /// causa de um arrasto") — it only points the station there directly, as
    /// `arrange_start` itself already does when it pins a screen into the
    /// air. A screen already being arranged (by keyboard or an earlier
    /// mouse-down) is left alone; a *different* one being arranged is
    /// committed first, exactly like `tab`'s `arrange_step`.
    fn arrange_start_at(&mut self, screen: ScreenId, now: Time) -> Vec<Out> {
        let mut out = Vec::new();
        if let Some(a) = self.arrange.as_ref() {
            if a.screen == screen {
                return out;
            }
            out.extend(self.arrange_commit(now));
        }
        if let Some(st) = self.station_of(screen) {
            self.station = st;
        }
        out.extend(self.arrange_start(now));
        out
    }

    fn pointer_move_start(&mut self, screen: ScreenId, at: Vec2, now: Time) -> Vec<Out> {
        let out = self.arrange_start_at(screen, now);
        if let Some(a) = self.arrange.as_ref() {
            let view = self.pointer_view();
            // billboard while dragging (plan §2.3: facing the eye until
            // `alt` rotates it, then it keeps whatever it had on drop)
            let grab = arrange::grab(&view, &a.pose, at, true);
            self.pointer_drag = Some(PointerDrag::Move { grab, last_at: at });
        }
        out
    }

    fn pointer_resize_start(
        &mut self,
        screen: ScreenId,
        grip: Grip,
        at: Vec2,
        now: Time,
    ) -> Vec<Out> {
        let out = self.arrange_start_at(screen, now);
        if self.arrange.is_some() {
            let logical = self.loose_logical(screen).unwrap_or_else(|| self.usable());
            self.pointer_drag = Some(PointerDrag::Resize {
                grip,
                start_logical: logical,
                start_at: at,
            });
        }
        out
    }

    fn pointer_drag_start(
        &mut self,
        gesture: pointer::Gesture,
        hit: pointer::Hit,
        at: Vec2,
        now: Time,
    ) -> Vec<Out> {
        match gesture {
            pointer::Gesture::Move => match screen_of_hit(hit) {
                Some(screen) => self.pointer_move_start(screen, at, now),
                None => Vec::new(),
            },
            pointer::Gesture::Resize(grip) => match hit {
                pointer::Hit::Handle { screen, .. } => {
                    self.pointer_resize_start(screen, grip, at, now)
                }
                _ => Vec::new(),
            },
            pointer::Gesture::ResizeNearest => match screen_of_hit(hit) {
                Some(screen) => {
                    let out = self.arrange_start_at(screen, now);
                    if let Some(a) = self.arrange.as_ref() {
                        let logical = self.loose_logical(screen).unwrap_or_else(|| self.usable());
                        let view = self.pointer_view();
                        let grip = arrange::nearest_corner(&view, &a.pose, logical, at);
                        self.pointer_drag = Some(PointerDrag::Resize {
                            grip,
                            start_logical: logical,
                            start_at: at,
                        });
                    }
                    out
                }
                None => Vec::new(),
            },
            pointer::Gesture::Lasso { .. } => {
                self.pointer_drag = Some(PointerDrag::Lasso { start_at: at });
                Vec::new()
            }
            // ANDAR com mira: `g` / `world.grab` (`World::grab`) already
            // does this by keyboard; the mouse hold-to-carry gesture
            // (ctrl+esquerdo segurado) is a follow-up — see the report.
            pointer::Gesture::Carry => Vec::new(),
        }
    }

    fn pointer_drag_move(&mut self, at: Vec2, eff: pointer::Effective, now: Time) -> Vec<Out> {
        let Some(drag) = self.pointer_drag else {
            return Vec::new();
        };
        match drag {
            PointerDrag::Move { grab, last_at } => {
                if let Some(a) = self.arrange.as_mut() {
                    a.strong = eff.strong;
                    a.magnets_off = eff.no_magnets;
                }
                self.pointer_drag = Some(PointerDrag::Move { grab, last_at: at });
                if eff.rotate {
                    let d = at - last_at;
                    self.arrange_touch(
                        "girar",
                        move |p| arrange::rotate(p, d, ROTATE_RAD_PER_PX),
                        now,
                    )
                } else {
                    let view = self.pointer_view();
                    let new_pose = arrange::drag_to(&view, &grab, at);
                    self.arrange_touch("mover", move |_| new_pose, now)
                }
            }
            PointerDrag::Resize {
                grip,
                start_logical,
                start_at,
            } => self.pointer_resize(grip, start_logical, start_at, at, eff.strong, now),
            PointerDrag::Lasso { .. } => Vec::new(),
        }
    }

    /// Resizing from a handle (plan §2.4): `nkw_core::arrange::resize` does
    /// the anchor-preserving geometry; `xdg_toplevel` min/max hints are not
    /// threaded through to `World` yet, so only `arrange::MIN_LOGICAL`/the
    /// usable size bound it (see the report).
    fn pointer_resize(
        &mut self,
        grip: Grip,
        start_logical: Vec2,
        start_at: Vec2,
        at: Vec2,
        keep_aspect: bool,
        now: Time,
    ) -> Vec<Out> {
        let Some(a) = self.arrange.as_ref() else {
            return Vec::new();
        };
        let (screen, pose) = (a.screen, a.pose);
        let view = self.pointer_view();
        let usable = self.usable();
        let hints = arrange::Hints::default();
        let (logical, new_pose) = arrange::resize(
            &view,
            &pose,
            start_logical,
            grip,
            start_at,
            at,
            keep_aspect,
            &hints,
            usable,
        );
        let mut out = self.arrange_touch("redimensionar", move |_| new_pose, now);
        out.push(Out::Configure {
            screen,
            size: Some(logical),
            fullscreen: None,
            maximized: None,
        });
        out
    }

    /// The wheel during a Move drag (plan §2.3): depth by default, visual
    /// scale with `alt` effective.
    fn pointer_drag_wheel(&mut self, steps: i32, scale: bool, now: Time) -> Vec<Out> {
        let Some(PointerDrag::Move { grab, last_at }) = self.pointer_drag else {
            return Vec::new();
        };
        if scale {
            return self.arrange_touch("escala", move |p| arrange::rescale(p, steps), now);
        }
        let mut grab = grab;
        arrange::step_depth(&mut grab, steps, false, now);
        self.pointer_drag = Some(PointerDrag::Move { grab, last_at });
        let view = self.pointer_view();
        let new_pose = arrange::drag_to(&view, &grab, last_at);
        self.arrange_touch("profundidade", move |_| new_pose, now)
    }

    fn pointer_drag_end(&mut self, at: Vec2, eff: pointer::Effective, now: Time) -> Vec<Out> {
        match self.pointer_drag {
            Some(PointerDrag::Lasso { start_at }) => {
                self.pointer_drag = None;
                return self.pointer_lasso_end(start_at, at, eff.strong, now);
            }
            Some(_) => {}
            None => return Vec::new(),
        }
        let mut out = self.pointer_drag_move(at, eff, now);
        self.pointer_drag = None;
        out.extend(self.arrange_commit(now));
        out
    }

    /// Box-select on release (plan §2.3's "em espaço vazio, laçada"):
    /// `nkw_core::arrange::lasso` by projected centre, over the current
    /// room's screens; `additive` (`shift`) adds to `arrange_selection`
    /// instead of replacing it.
    fn pointer_lasso_end(&mut self, a: Vec2, b: Vec2, additive: bool, now: Time) -> Vec<Out> {
        let _ = now;
        let view = self.pointer_view();
        let room = self.current_room();
        let mut centres = Vec::new();
        for d in 0..self.rooms[room].desks.len() {
            for k in 0..self.rooms[room].desks[d].screens.len() {
                let id = self.rooms[room].desks[d].screens[k];
                centres.push((id, self.screen_place(room, d, k).center));
            }
        }
        let picked = arrange::lasso(&view, a, b, &centres);
        if !additive {
            self.arrange_selection.clear();
        }
        self.arrange_selection.extend(picked);
        self.layout_gen += 1;
        self.pending.push(Wish::world(Want::Once));
        Vec::new()
    }

    /// `alt`+wheel with nothing pressed (plan §2.4): visual scale, and only
    /// on a screen already in the air — a desk-stack screen's `scale` has no
    /// visible effect (`place_depth` only reads it for `st.air`).
    fn pointer_scale(&mut self, hit: pointer::Hit, steps: i32, now: Time) -> Vec<Out> {
        let Some(id) = screen_of_hit(hit) else {
            return Vec::new();
        };
        let before = self.screen_state(id);
        if before.air.is_none() {
            return Vec::new();
        }
        let new_scale = (before.air_scale() + steps as f32 * arrange::SCALE_STEP)
            .clamp(arrange::SCALE_MIN, arrange::SCALE_MAX);
        self.wm.entry(id).or_default().scale = new_scale;
        let after = self.screen_state(id);
        if after != before {
            self.history.push(history::Entry {
                kind: history::OpKind::Move,
                label: "escala".to_string(),
                subjects: vec![id.0 as u64],
                before: ArrangeSnap::from([(id, before)]),
                after: ArrangeSnap::from([(id, after)]),
                at: now,
            });
        }
        self.layout_gen += 1;
        self.pending.push(Wish::world(Want::Once));
        Vec::new()
    }
}

/// 0..=100-clamped percentage (`Air::scale_pct` / `LayoutAt::Dock`'s), from
/// a `1.0`-centred visual scale.
fn pct(scale: f32) -> u16 {
    (scale * 100.0).round().clamp(0.0, u16::MAX as f32) as u16
}

/// A room's display name as a path component: `/` and NUL cannot survive
/// one, so each becomes `_`; an empty name (or `.`/`..`, once trimmed)
/// does too, so a stray room name never escapes `layouts/`. Anything else
/// (spaces, accents: both of nkw's own room names have one) is kept as-is.
fn layout_room_slug(room: &str) -> String {
    let s: String = room
        .trim()
        .chars()
        .map(|c| if c == '/' || c == '\0' { '_' } else { c })
        .collect();
    if s.is_empty() || s == "." || s == ".." {
        "_".to_string()
    } else {
        s
    }
}

/// `dir` (made if missing) then `<name>.json`, written atomically: a temp
/// file in the same dir, `fsync`ed, then `rename`d over the real path —
/// the same durability `persist_io::Store::save` gives `world.json`.
/// Unlike it, a corrupt *load* here is never moved aside to `.bak`:
/// loading a named arrangement is a one-off, visible, user-chosen action
/// (not boot-time recovery, which must never refuse to start), so an error
/// toast is the honest response; the user can just save over it again.
fn save_layout_file(dir: &Path, name: &str, text: &str) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let tmp = dir.join(format!(".{name}.json.tmp"));
    {
        use std::io::Write;
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(text.as_bytes())?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, dir.join(format!("{name}.json")))
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
            air: None,
            ..Place::default()
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
                width: Some(Width::TwoThirds),
                air: None,
                ..Place::default()
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
            ("key".to_string(), "turn".to_string()),
            ("value".to_string(), "snap".to_string()),
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

    // ---------------------------------------------------------------
    // wave 5: arrangements on disk, and the aimed terminal

    fn note(w: &World) -> String {
        w.note.as_ref().map(|(_, m)| m.clone()).unwrap_or_default()
    }

    #[test]
    fn layout_save_and_load_round_trip() {
        let tmp = tempfile::tempdir().unwrap();
        let mut w = fixture();
        w.state_dir = Some(tmp.path().to_path_buf());
        w.command(Cmd::LayoutSave(ShortName::new("padrao")), 0.0);
        assert_eq!(note(&w), "arranjo salvo: padrao");
        assert!(tmp.path().join("layouts/acme/padrao.json").is_file());

        // screen 1 (acme desk 0, alone there) goes to the air
        at(&mut w, 0, 0);
        w.command(Cmd::Pin, 0.0);
        assert!(w.screen_state(ScreenId(1)).air.is_some());

        // loading the saved arrangement puts it back exactly where it was
        w.command(Cmd::LayoutLoad(ShortName::new("padrao")), 1.0);
        assert_eq!(note(&w), "arranjo: padrao");
        assert!(w.screen_state(ScreenId(1)).air.is_none());
        assert_eq!(w.locate(ScreenId(1)), Some((1, 0, 0)));
    }

    #[test]
    fn layout_save_rejects_an_invalid_name() {
        let tmp = tempfile::tempdir().unwrap();
        let mut w = fixture();
        w.state_dir = Some(tmp.path().to_path_buf());
        w.command(Cmd::LayoutSave(ShortName::new("Maiuscula")), 0.0);
        assert!(note(&w).contains("nome do arranjo"), "{}", note(&w));
        assert!(!tmp.path().join("layouts").exists(), "nothing was written");
    }

    #[test]
    fn layout_load_of_a_missing_file_is_a_friendly_error() {
        let tmp = tempfile::tempdir().unwrap();
        let mut w = fixture();
        w.state_dir = Some(tmp.path().to_path_buf());
        let before = acme(&w);
        w.command(Cmd::LayoutLoad(ShortName::new("fantasma")), 0.0);
        assert!(note(&w).contains("não existe"), "{}", note(&w));
        assert_eq!(acme(&w), before, "nothing moved");
    }

    #[test]
    fn layout_load_with_a_screen_closed_reports_it_missing_without_crashing() {
        let tmp = tempfile::tempdir().unwrap();
        let mut w = fixture();
        w.state_dir = Some(tmp.path().to_path_buf());
        w.command(Cmd::LayoutSave(ShortName::new("trio")), 0.0);
        // acme.3 closes: it was alone on its desk, so the desk goes too
        w.rooms[1].desks.pop();
        w.screens.remove(&ScreenId(3));

        at(&mut w, 0, 0);
        w.command(Cmd::Pin, 0.0); // give the load something to actually move
        w.command(Cmd::LayoutLoad(ShortName::new("trio")), 1.0);

        assert!(note(&w).contains("1 de fora"), "{}", note(&w));
        assert!(note(&w).contains("acme.3"), "{}", note(&w));
        assert!(
            w.screen_state(ScreenId(1)).air.is_none(),
            "screen 1 still moved"
        );
    }

    #[test]
    fn layout_rename_then_delete() {
        let tmp = tempfile::tempdir().unwrap();
        let mut w = fixture();
        w.state_dir = Some(tmp.path().to_path_buf());
        w.command(Cmd::LayoutSave(ShortName::new("um")), 0.0);

        w.command(
            Cmd::LayoutRename {
                name: ShortName::new("um"),
                to: ShortName::new("dois"),
            },
            0.0,
        );
        assert!(!tmp.path().join("layouts/acme/um.json").exists());
        assert!(tmp.path().join("layouts/acme/dois.json").is_file());

        // renaming onto an existing name is refused, not overwritten
        w.command(Cmd::LayoutSave(ShortName::new("tres")), 0.0);
        w.command(
            Cmd::LayoutRename {
                name: ShortName::new("tres"),
                to: ShortName::new("dois"),
            },
            0.0,
        );
        assert!(note(&w).contains("já existe"), "{}", note(&w));
        assert!(tmp.path().join("layouts/acme/tres.json").is_file());

        w.command(Cmd::LayoutDelete(ShortName::new("dois")), 0.0);
        assert!(!tmp.path().join("layouts/acme/dois.json").exists());
        assert!(note(&w).contains("apagado"), "{}", note(&w));

        w.command(Cmd::LayoutDelete(ShortName::new("dois")), 0.0);
        assert!(note(&w).contains("não existe"), "{}", note(&w));
    }

    #[test]
    fn layout_commands_without_a_state_dir_toast_instead_of_touching_disk() {
        let mut w = fixture();
        assert!(w.state_dir.is_none());
        w.command(Cmd::LayoutSave(ShortName::new("sem-dir")), 0.0);
        assert!(note(&w).contains("sem diretório de estado"), "{}", note(&w));
    }

    #[test]
    fn terminal_air_makes_a_frame_and_spawns_a_real_terminal() {
        let mut w = fixture();
        let frames_before = w.frames.len();
        let out = w.command(Cmd::TerminalAir, 0.0);
        assert_eq!(w.frames.len(), frames_before + 1);
        assert!(matches!(w.station, Station::Frame(_)));
        assert!(
            out.contains(&Out::Spawn(vec![
                nkw_core::rules::DEFAULT_TERMINAL.to_string()
            ])),
            "{out:?}"
        );
    }

    // -----------------------------------------------------------------
    // the pointer, for real mouse events (plan §2.3/§2.4, D15)

    mod pointer_tests {
        use super::*;
        use nkw_core::pointer;

        fn ptr_cfg() -> pointer::Config {
            pointer::Config {
                drag: true,
                modifier: pointer::DragMod::Ctrl,
                world: pointer::WorldKey::CtrlAlt,
                threshold_px: 6.0,
                double_s: 0.4,
            }
        }

        const CTRL: pointer::Mods = pointer::Mods {
            ctrl: true,
            alt: false,
            shift: false,
            logo: false,
        };
        const CTRL_ALT: pointer::Mods = pointer::Mods {
            ctrl: true,
            alt: true,
            shift: false,
            logo: false,
        };
        const NONE: pointer::Mods = pointer::Mods {
            ctrl: false,
            alt: false,
            shift: false,
            logo: false,
        };

        /// The output's centre, physical px: `fixture()`'s station looks
        /// dead-on at screen 1, so this is exactly its own ray.
        fn centre() -> Vec2 {
            Vec2::new(OUT.width_px as f32 / 2.0, OUT.height_px as f32 / 2.0)
        }

        #[test]
        fn pointer_hit_finds_the_screen_straight_ahead() {
            let w = fixture();
            assert_eq!(w.pointer_hit(centre()), pointer::Hit::Screen(ScreenId(1)));
        }

        #[test]
        fn pointer_mode_matches_the_world_mode() {
            let mut w = fixture();
            assert_eq!(w.pointer_mode(), pointer::PtrMode::Nav);
            w.command(Cmd::Focus, 0.0);
            assert_eq!(w.pointer_mode(), pointer::PtrMode::Focus);
            w.command(Cmd::Back, 0.0);
            w.command(Cmd::Walk, 0.0);
            assert_eq!(
                w.pointer_mode(),
                pointer::PtrMode::WalkAim,
                "mouse_look is on by default"
            );
        }

        /// Plan §2.3: `ctrl+arrastar em qualquer ponto da tela` moves it in
        /// NAV — the required end-to-end case (a real mouse gesture, not a
        /// keyboard one, changes a screen's pose).
        #[test]
        fn ctrl_drag_in_nav_moves_the_screen() {
            let mut w = fixture();
            let cfg = ptr_cfg();
            let mut now = 0.0;
            let id = ScreenId(1);
            assert!(!w.is_loose(id), "starts on its desk, not in the air");
            let start_place = w.screen_place(1, 0, 0);

            w.pointer_step(pointer::PtrEv::Mods(CTRL), &cfg, now);
            let hit = pointer::Hit::Screen(id);
            let press_at = centre();
            let outs = w.pointer_step(
                pointer::PtrEv::Press {
                    button: pointer::Button::Left,
                    at: press_at,
                    hit,
                },
                &cfg,
                now,
            );
            assert!(outs.is_empty(), "a press alone is not a drag yet");

            now += 0.05;
            let move_at = press_at + Vec2::new(120.0, 0.0);
            let outs = w.pointer_step(pointer::PtrEv::Motion { at: move_at, hit }, &cfg, now);
            assert!(!outs.is_empty(), "past the threshold: a drag starts");
            w.pointer_apply(&outs, now);
            assert!(
                w.is_loose(id),
                "pinned loose while being dragged (like ARRUMAR)"
            );
            let mid_drag = w
                .screen_state(id)
                .air
                .expect("a live drag pose is in the air");
            assert!(
                mid_drag.center.distance(start_place.center) > 0.1,
                "the centre followed the pointer: {:?} vs {:?}",
                mid_drag.center,
                start_place.center
            );

            let outs = w.pointer_step(
                pointer::PtrEv::Release {
                    button: pointer::Button::Left,
                    at: move_at,
                },
                &cfg,
                now,
            );
            w.pointer_apply(&outs, now);
            assert!(w.is_loose(id), "committed to the air, not back on the desk");
            // a loose/air screen stays *listed* in its desk's `screens` (the
            // module doc: "after the stack"); `is_loose` is what marks it air
            assert_eq!(
                acme(&w)[0],
                vec![1],
                "screen 1 still listed on desk 0, now loose: {:?}",
                acme(&w)
            );
            let committed = w.screen_state(id).air.expect("still an air screen");
            assert!(
                committed.center.distance(start_place.center) > 0.1,
                "it actually moved: {:?} vs {:?}",
                committed.center,
                start_place.center
            );
        }

        /// Regression: `arrange_commit`'s Air/Wall branch used to call
        /// `go_to(Station::Air(..))`, which glides the camera right up to
        /// the dropped screen (close enough to fill the view) — a direct
        /// violation of plan §2.1, "a câmera nunca se move por causa de um
        /// arrasto". Dragging a screen onto a desk (`ArrangeTarget::Desk`,
        /// via `send_to_nearest_desk`'s "no slots" fallback) had the same
        /// bug. Both now only touch `self.station`.
        #[test]
        fn ctrl_drag_never_moves_the_camera() {
            let mut w = fixture();
            let cfg = ptr_cfg();
            let mut now = 0.0;
            let id = ScreenId(1);
            let camera_before = w.camera;

            w.pointer_step(pointer::PtrEv::Mods(CTRL), &cfg, now);
            let hit = pointer::Hit::Screen(id);
            let press_at = centre();
            w.pointer_step(
                pointer::PtrEv::Press {
                    button: pointer::Button::Left,
                    at: press_at,
                    hit,
                },
                &cfg,
                now,
            );
            now += 0.05;
            let move_at = press_at + Vec2::new(260.0, 0.0);
            let outs = w.pointer_step(pointer::PtrEv::Motion { at: move_at, hit }, &cfg, now);
            w.pointer_apply(&outs, now);
            assert_eq!(
                w.camera, camera_before,
                "the camera must not move mid-drag either"
            );

            let outs = w.pointer_step(
                pointer::PtrEv::Release {
                    button: pointer::Button::Left,
                    at: move_at,
                },
                &cfg,
                now,
            );
            w.pointer_apply(&outs, now);
            assert!(w.is_loose(id), "committed to the air");
            assert_eq!(
                w.camera, camera_before,
                "committing the drag must not move the camera, however far the screen landed"
            );
        }

        /// Plan §2.3: a plain drag with no modifier never moves a screen in
        /// NAV — the machine produces nothing at all, so nothing in `World`
        /// changes either.
        #[test]
        fn release_without_the_modifier_does_nothing() {
            let mut w = fixture();
            let cfg = ptr_cfg();
            let now = 0.0;
            let id = ScreenId(1);
            let before = w.screen_state(id);
            let hit = pointer::Hit::Screen(id);
            let press_at = centre();

            let outs = w.pointer_step(
                pointer::PtrEv::Press {
                    button: pointer::Button::Left,
                    at: press_at,
                    hit,
                },
                &cfg,
                now,
            );
            assert!(outs.is_empty());
            let move_at = press_at + Vec2::new(120.0, 0.0);
            let outs = w.pointer_step(pointer::PtrEv::Motion { at: move_at, hit }, &cfg, now);
            assert!(outs.is_empty(), "no modifier: the machine does nothing");
            let outs = w.pointer_step(
                pointer::PtrEv::Release {
                    button: pointer::Button::Left,
                    at: move_at,
                },
                &cfg,
                now,
            );
            assert!(
                outs.is_empty(),
                "no click either: it moved without a modifier"
            );
            let world_outs = w.pointer_apply(&outs, now);
            assert!(world_outs.is_empty());
            assert_eq!(w.screen_state(id), before, "nothing changed on the screen");
            assert!(!w.is_loose(id), "still on its desk");
        }

        /// Plan §2.3: `alt`+wheel during a Move drag scales instead of
        /// stepping depth.
        #[test]
        fn alt_wheel_during_a_drag_scales() {
            let mut w = fixture();
            let cfg = ptr_cfg();
            let mut now = 0.0;
            let id = ScreenId(1);
            let hit = pointer::Hit::Screen(id);
            let press_at = centre();

            w.pointer_step(pointer::PtrEv::Mods(CTRL), &cfg, now);
            w.pointer_step(
                pointer::PtrEv::Press {
                    button: pointer::Button::Left,
                    at: press_at,
                    hit,
                },
                &cfg,
                now,
            );
            now += 0.05;
            let move_at = press_at + Vec2::new(120.0, 0.0);
            let outs = w.pointer_step(pointer::PtrEv::Motion { at: move_at, hit }, &cfg, now);
            w.pointer_apply(&outs, now);
            assert!(w.is_loose(id));
            let scale_before = w.screen_state(id).air_scale();

            w.pointer_step(pointer::PtrEv::Mods(CTRL_ALT), &cfg, now);
            let outs = w.pointer_step(pointer::PtrEv::Wheel { steps: 1, hit }, &cfg, now);
            assert_eq!(
                outs,
                vec![pointer::PtrOut::DragWheel {
                    steps: 1,
                    scale: true,
                }]
            );
            w.pointer_apply(&outs, now);
            let scale_after = w.screen_state(id).air_scale();
            assert!(
                scale_after > scale_before,
                "{scale_before} -> {scale_after}"
            );
        }

        /// Plan §2.3: `esc` during a drag cancels it; `PtrOut::DragCancel`
        /// reaches `World::arrange_revert`, which undoes the pose and
        /// restores the screen's slot in its desk.
        #[test]
        fn escape_cancels_the_drag_and_reverts_via_history() {
            let mut w = fixture();
            let cfg = ptr_cfg();
            let mut now = 0.0;
            let id = ScreenId(1);
            let start_place = w.screen_place(1, 0, 0);
            let hit = pointer::Hit::Screen(id);
            let press_at = centre();

            w.pointer_step(pointer::PtrEv::Mods(CTRL), &cfg, now);
            w.pointer_step(
                pointer::PtrEv::Press {
                    button: pointer::Button::Left,
                    at: press_at,
                    hit,
                },
                &cfg,
                now,
            );
            now += 0.05;
            let move_at = press_at + Vec2::new(120.0, 0.0);
            let outs = w.pointer_step(pointer::PtrEv::Motion { at: move_at, hit }, &cfg, now);
            w.pointer_apply(&outs, now);
            let mid = w
                .screen_state(id)
                .air
                .expect("pinned loose, a live drag pose");
            assert!(
                mid.center.distance(start_place.center) > 0.1,
                "moved while dragging"
            );

            let outs = w.pointer_step(pointer::PtrEv::Cancel, &cfg, now);
            assert_eq!(outs, vec![pointer::PtrOut::DragCancel]);
            w.pointer_apply(&outs, now);

            // the screen is a plain desk screen again, back at its desk-0
            // slot and its original place — not left `is_loose` with a
            // leftover air pose (`arrange_revert` restores `origin_state`,
            // captured before `arrange_start`'s `pin_loose` ever touched it):
            assert!(
                w.screen_state(id).air.is_none(),
                "a plain desk screen again, not left pinned loose"
            );
            let back = w.screen_place(1, 0, 0);
            assert!(
                back.center.distance(start_place.center) < 0.01,
                "the pose is back where it started: {:?} vs {:?}",
                back.center,
                start_place.center
            );
            assert_eq!(acme(&w)[0], vec![1], "back at desk 0's slot 0 in the list");
            w.pointer_step(pointer::PtrEv::Mods(NONE), &cfg, now);
        }
    }
}
