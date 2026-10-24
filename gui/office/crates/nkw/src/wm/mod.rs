//! The window manager's plugin (§4, phase 1): what the palette, SALTO and
//! the HUD get from the WM, `world.json` (rooms, desks, rules, the place
//! memory), the launcher, `world.terminal`, the clipboard bridge to the
//! host, and the Smithay states the xdg handlers need (activation, dialog).
//!
//! The layout operations themselves are the World's (`world_ops.rs`, a
//! child module of `world`); the xdg side is `handlers.rs`.
//!
//! `inject wm` shapes: `{"command": "world.wider", "args": {…}}` (dispatched
//! like a key), `{"save": true}` (write `world.json` now); for tests
//! `{"x11_dir": DIR}` (where the satellite's socket is looked for) and
//! `{"proc_root": DIR}` (a fake `/proc` for "aberto lá fora").
//!
//! `dump` = `{"rooms":[{id,name,kind}],"places":{app_id:[room,desk]},
//! "rules":n,"clipboard":{"host":bool,"mimes":n},"apps":n,"saves":n,
//! "persist":bool,"xwayland":{display,pid,state}|null,"tour_done":bool}`
//! — never any clipboard content. Launching (env, X11 on demand, isolated
//! profiles, the tour flag) and the phone's Apps/Janelas are `apps.rs`.

pub mod apps;
pub mod clipboard;
pub mod launch_env;
pub mod launcher;
pub mod persist_io;
pub mod xwayland;

use std::cell::{OnceCell, RefCell};
use std::collections::{BTreeMap, BTreeSet};
use std::os::fd::OwnedFd;
use std::path::PathBuf;

use glam::Vec3;
use nkw_core::persist::{DeskAt, Place, Plan, RoomKind, WorldFile};
use nkw_core::rules::{self, Config};
use nkw_core::{Action, Pick, TalkPurpose, Target, Time, Want, Wish, onetoone};
use serde_json::{Value, json};
use smithay::wayland::selection::data_device::{
    clear_data_device_selection, request_data_device_client_selection, set_data_device_selection,
};
use smithay::wayland::shell::xdg::dialog::XdgDialogState;
use smithay::wayland::xdg_activation::XdgActivationState;

use crate::ext::{
    ArgKind, ArgSpec, Badge, Ctx, PaletteItem, Pickable, Plugin, Req, Source, Tag, ToastKind,
};
use crate::state::Nkw;
use crate::world::{Aim, COMMANDS, Cmd, HALL_W, RoomRef, ShortName, World};
use clipboard::{Bridge, FromHost, Status, ToHost};
use launcher::App;
use persist_io::{Load, Store};

/// A window spawned by `world.terminal` within this long lands right of
/// the station whatever the rules say.
pub const INTENT_S: f64 = 5.0;
/// An activation token older than this is not "recent".
pub const TOKEN_MAX_AGE: std::time::Duration = std::time::Duration::from_secs(10);

const TAG_TITLE: u64 = 0x776d_0000_0000;
const TAG_ATTENTION: u64 = 0x776d_0001_0000;
const TITLE_RGB: [u8; 3] = [250, 240, 220];
const ATTENTION_RGB: [u8; 3] = [255, 170, 90];

/// The `world.*` commands the WM plugin runs itself (not the World).
pub const WM_COMMANDS: &[(&str, &str, &[&str])] = &[
    ("world.terminal", "terminal novo", &["T", "world+enter"]),
    ("world.launcher", "apps", &["world+d"]),
    ("world.quit", "sair do mundo (confirma)", &["world+shift+e"]),
];

/// `world.launch {exec}` / `world.launch:<desktop id>`: run an app.
pub const LAUNCH: &str = "world.launch";
/// `world.place_next`'s intent lasts this long without `ttl_s`.
pub const INTENT_TTL_S: f64 = 5.0;
/// How many parent links a window's pid may be from a process we spawned.
pub const INTENT_HOPS: usize = 6;
/// The mimes `world.copy` offers.
pub const COPY_MIMES: [&str; 3] = ["text/plain;charset=utf-8", "text/plain", "UTF8_STRING"];

/// The placement intent of `world.place_next`.
#[derive(Clone, Debug, PartialEq)]
struct Intent {
    place: Place,
    until: Time,
}

pub struct Wm {
    demo: bool,
    store: Option<Store>,
    config: Config,
    /// a message for a toast at the first tick (broken files)
    errors: Vec<String>,
    /// what `world.json` asked to restore; the queue serves new windows
    plan: Plan,
    restore: bool,
    /// the place memory: app_id → where its last window lived
    memory: BTreeMap<String, Place>,
    seen_gen: u64,
    last_saved: Option<WorldFile>,
    latest: Option<WorldFile>,
    due: Option<Time>,
    saves: u32,
    apps: OnceCell<Vec<App>>,
    intent_until: Option<Time>,
    /// xdg-activation (the handlers reach it through `Nkw.ext.wm`)
    pub activation: Option<XdgActivationState>,
    dialog: Option<XdgDialogState>,
    bridge: Option<Bridge>,
    pub clip: Status,
    /// the rooms as of the last change (`dump`)
    rooms: Vec<Value>,
    /// `world.place_next`: where the next window goes
    next: Option<Intent>,
    /// what `world.copy` put on the seat (never logged, never dumped)
    copied: Option<String>,
    /// the loop, for what must touch the seat (`world.copy`)
    lh: Option<calloop::LoopHandle<'static, Nkw>>,
    /// `xwayland-satellite`, on demand (S22)
    xwayland: xwayland::Xwayland,
    /// children waiting for the satellite's socket
    x_waiting: Vec<Vec<String>>,
    /// the world's `WAYLAND_DISPLAY` (the "aberto lá fora" check)
    socket: String,
    /// where processes are read for that check (`None` under `--demo`
    /// unless a test injects one)
    proc_root: Option<PathBuf>,
    /// `<state-dir>` (isolated profiles live under `profiles/`)
    state_root: Option<PathBuf>,
    /// `world.json`'s `tour_done`
    tour_done: bool,
    /// the first tick (the tour starts 1.5 s later), and whether it was asked
    tour_at: Option<Time>,
    tour_asked: bool,
    /// the Apps page's outside set, and when it was read
    outside_cache: RefCell<Option<(Time, BTreeSet<String>)>>,
}

impl Wm {
    pub fn new(cli: &crate::cli::Cli) -> Wm {
        let mut wm = Wm {
            demo: cli.demo,
            store: None,
            config: Config::default(),
            errors: Vec::new(),
            plan: Plan::default(),
            restore: false,
            memory: BTreeMap::new(),
            seen_gen: u64::MAX,
            last_saved: None,
            latest: None,
            due: None,
            saves: 0,
            apps: OnceCell::new(),
            intent_until: None,
            activation: None,
            dialog: None,
            bridge: None,
            clip: Status::default(),
            rooms: Vec::new(),
            next: None,
            copied: None,
            lh: None,
            xwayland: xwayland::Xwayland::default(),
            x_waiting: Vec::new(),
            socket: cli.socket.clone(),
            proc_root: (!cli.demo).then(|| PathBuf::from("/proc")),
            state_root: None,
            tour_done: false,
            tour_at: None,
            tour_asked: false,
            outside_cache: RefCell::new(None),
        };
        if cli.demo {
            if cli.reduced_motion {
                wm.config.reduced_motion();
            }
            wm.xwayland = xwayland_for(cli, &wm.config);
            return wm;
        }
        let env = |k: &str| std::env::var_os(k).map(PathBuf::from);
        if let Some(p) = persist_io::config_path(
            cli.state_dir.as_deref(),
            env("XDG_CONFIG_HOME").as_deref(),
            env("HOME").as_deref(),
        ) {
            let (c, err) = persist_io::read_config(&p);
            wm.config = c;
            wm.errors.extend(err);
            for n in &wm.config.notes {
                tracing::warn!("{n}");
            }
        }
        if cli.reduced_motion {
            wm.config.reduced_motion();
        }
        if let Some(dir) = cli.state_root() {
            let store = Store::new(dir.clone());
            match store.load() {
                Load::Missing => {}
                Load::Ok(f) => {
                    wm.tour_done = f.tour_done;
                    wm.plan = nkw_core::persist::apply(&f, &[]);
                    wm.memory = wm.plan.places.clone();
                    wm.last_saved = Some(f);
                    wm.restore = true;
                }
                Load::Corrupt(msg) => wm.errors.push(msg),
            }
            wm.store = Some(store);
            wm.state_root = Some(dir);
        }
        wm.xwayland = xwayland_for(cli, &wm.config);
        wm
    }

    /// The satellite's X display (`":N"`) while it is up.
    pub fn xwayland_display(&self) -> Option<String> {
        self.xwayland.display()
    }

    pub fn xwayland_state(&self) -> &xwayland::XState {
        &self.xwayland.state
    }

    /// The parsed `world.toml` (`keys.*` for the hub; the CLI flags win).
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Where a new window of `app_id` goes: a rule > its restored place >
    /// the place memory > the default (`None`). Right after `world.terminal`
    /// the next window lands right of the station.
    pub fn place_for(&mut self, app_id: &str, now: Time) -> Option<Place> {
        if self.intent_until.take().is_some_and(|t| now <= t) {
            return None;
        }
        let mem = self
            .plan
            .take(app_id)
            .or_else(|| self.memory.get(app_id).cloned());
        let mem: BTreeMap<String, Place> =
            mem.into_iter().map(|p| (app_id.to_string(), p)).collect();
        rules::place_for(&self.config.rules, &mem, app_id)
    }

    /// `world.terminal`'s argv: `[launch] terminal`, else `foot`; the
    /// testclient under `--demo`.
    pub fn terminal(&self) -> Vec<String> {
        if self.demo {
            return ["testclient", "--app-id", "demo.term"]
                .map(String::from)
                .to_vec();
        }
        self.config.terminal()
    }

    /// `world.terminal {cmd}`: the terminal running `sh -c cmd` (as is
    /// without one, or with a blank one).
    pub fn terminal_with(&self, cmd: Option<&str>) -> Vec<String> {
        let mut v = self.terminal();
        if let Some(c) = cmd.filter(|c| !c.trim().is_empty()) {
            v.extend(["-e", "sh", "-c", c].map(String::from));
        }
        v
    }

    /// `world.interact`, the fallback after objects/cats declined (S15):
    /// the World's aim — a screen goes FOCO, a cat talks, a doorway leads
    /// to its room.
    fn interact(&mut self, ctx: &Ctx) -> Option<Vec<Req>> {
        let r = match ctx.world.aim()? {
            Aim::Screen(s) => {
                let t = ctx.world.modal_child(s).unwrap_or(s);
                Req::Action(Action::Focus(Target::Screen(t)))
            }
            Aim::Cat(cat) => Req::Action(Action::Talk {
                cat,
                purpose: TalkPurpose::Session,
            }),
            Aim::Door(r) => Req::Cmd(Cmd::GotoRoom(RoomRef::Id(r))),
        };
        Some(vec![r])
    }

    /// `world.place_next {room, desk?, ttl_s?}`: the next new toplevel
    /// that descends from a process we spawned (any, under `--demo`) lands
    /// on that room/desk; one at a time, `ttl_s` (5) at most.
    fn place_next(&mut self, args: &BTreeMap<String, String>, ctx: &Ctx) -> Option<Vec<Req>> {
        let room = RoomRef::parse(args.get("room")?)?;
        let name = match room {
            RoomRef::Id(id) => ctx.world.rooms.iter().find(|r| r.id == id)?.name.clone(),
            RoomRef::Name(n) => match ctx.world.find_room(&room) {
                Some(i) => ctx.world.rooms[i].name.clone(),
                None => n.as_str().to_string(),
            },
        };
        let desk = match args.get("desk").map(|d| d.trim()) {
            None | Some("") => None,
            Some("last") => Some(DeskAt::Last),
            Some(d) => Some(DeskAt::Index(d.parse().ok()?)),
        };
        let ttl = args
            .get("ttl_s")
            .and_then(|t| t.trim().parse::<f64>().ok())
            .filter(|t| t.is_finite() && *t > 0.0)
            .unwrap_or(INTENT_TTL_S)
            .min(60.0);
        self.next = Some(Intent {
            place: Place {
                room: Some(name),
                desk,
                ..Place::default()
            },
            until: ctx.now + ttl,
        });
        Some(Vec::new())
    }

    /// The placement intent a new window of `pid` takes, if any (taken).
    /// `ours`: a pid we spawned. Under `--demo` any window takes it.
    pub fn take_intent(
        &mut self,
        pid: Option<u32>,
        ours: &dyn Fn(u32) -> bool,
        now: Time,
    ) -> Option<Place> {
        let i = self.next.as_ref()?;
        if now > i.until {
            self.next = None;
            return None;
        }
        let fits = self.demo
            || pid.is_some_and(|p| {
                nkw_sys::pidmap::anchor(p, ours, &nkw_sys::pidmap::ProcFs, INTENT_HOPS).is_some()
            });
        if !fits {
            return None;
        }
        self.next.take().map(|i| i.place)
    }

    /// `world.copy {text}`: the seat's selection becomes `text` (and the
    /// host's, through the bridge). Never logged, never in `dump`.
    fn copy(&mut self, args: &BTreeMap<String, String>) -> Option<Vec<Req>> {
        let text = args.get("text")?.clone();
        self.copied = Some(text);
        let Some(lh) = &self.lh else {
            return Some(Vec::new());
        };
        lh.insert_idle(|st: &mut Nkw| {
            let mimes: Vec<String> = COPY_MIMES.iter().map(|m| m.to_string()).collect();
            set_data_device_selection(&st.dh, &st.seat, mimes.clone(), ());
            let wm = &mut st.ext.wm;
            wm.clip.mirrored = false;
            wm.clip.mimes = mimes.len();
            if let Some(b) = &wm.bridge {
                b.send(ToHost::Offer(mimes));
            }
        });
        Some(Vec::new())
    }

    /// Write what `world.copy` set into `fd` (a paste of ours); the fd
    /// back when the selection is not ours (a client's or the host's).
    pub fn send_copied(&self, mime: &str, fd: OwnedFd) -> Result<(), OwnedFd> {
        let Some(text) = self.copied.clone() else {
            return Err(fd);
        };
        if !COPY_MIMES.contains(&mime) {
            return Ok(()); // dropping the fd ends the paste
        }
        // off the loop: a slow reader must not stall the compositor
        std::thread::spawn(move || {
            use std::io::Write;
            let mut f = std::fs::File::from(fd);
            let _ = f.write_all(text.as_bytes());
        });
        Ok(())
    }

    fn apps(&self) -> &[App] {
        self.apps.get_or_init(|| {
            if self.demo {
                launcher::demo()
            } else {
                launcher::system(&self.config.terminal())
            }
        })
    }

    /// Smithay globals that only the handlers use (made once the loop runs).
    fn globals(&mut self, dh: &smithay::reexports::wayland_server::DisplayHandle) {
        if self.activation.is_none() {
            self.activation = Some(XdgActivationState::new::<Nkw>(dh));
            self.dialog = Some(XdgDialogState::new::<Nkw>(dh));
        }
    }

    /// A client of ours set the clipboard (mimes only).
    pub fn our_selection(&mut self, mimes: Vec<String>) {
        self.copied = None;
        self.clip.mirrored = false;
        self.clip.mimes = mimes.len();
        if let Some(b) = &self.bridge {
            b.send(ToHost::Offer(mimes));
        }
    }

    /// A client of ours pastes the host's selection.
    pub fn host_receive(&mut self, mime: String, fd: OwnedFd) {
        if let Some(b) = &self.bridge {
            b.send(ToHost::Receive { mime, fd });
        }
    }

    fn save_now(&mut self) {
        let (Some(store), Some(f)) = (&self.store, &self.latest) else {
            return;
        };
        match store.save(f) {
            Ok(()) => {
                self.last_saved = self.latest.take();
                self.saves += 1;
            }
            Err(e) => tracing::warn!("world.json: {e}"),
        }
        self.due = None;
    }

    /// Follow the World's changes: the place memory, and a save 2 s after
    /// the last one.
    fn track(&mut self, w: &World, now: Time) {
        if w.generation() == self.seen_gen {
            return;
        }
        self.seen_gen = w.generation();
        let mut snap = w.snapshot();
        self.rooms = snap
            .rooms
            .iter()
            .map(|r| json!({"id": r.id, "name": r.name, "kind": r.kind.label()}))
            .collect();
        for s in &snap.screens {
            nkw_core::persist::remember(&mut self.memory, &s.app_id, s.place.clone());
        }
        snap.places = self.memory.clone();
        snap.tour_done = self.tour_done;
        if self.store.is_none() || self.last_saved.as_ref() == Some(&snap) {
            return;
        }
        if self.latest.as_ref() != Some(&snap) {
            self.latest = Some(snap);
            self.due = Some(now + persist_io::SAVE_DELAY_S);
        }
    }

    fn restore_reqs(&mut self) -> Vec<Req> {
        let mut v: Vec<Req> = self
            .plan
            .rooms
            .iter()
            .map(|(name, kind)| match kind {
                // the library comes back with its counter (the librarian's desk)
                RoomKind::Library => Req::Cmd(Cmd::Library { go: false }),
                _ => Req::Cmd(Cmd::RoomNew {
                    name: ShortName::new(name),
                    context: matches!(kind, RoomKind::Context(_)),
                    go: false,
                }),
            })
            .collect();
        if let Some(st) = &self.plan.station {
            v.push(Req::Cmd(Cmd::GotoRoom(RoomRef::Name(ShortName::new(
                st.room(),
            )))));
        }
        v
    }
}

/// The satellite of this run: `--xwayland`, else `[launch] xwayland`;
/// the binary looked up on `PATH` only when it may run.
fn xwayland_for(cli: &crate::cli::Cli, cfg: &Config) -> xwayland::Xwayland {
    let mode = xwayland::Xwayland::resolve(cli.xwayland, cfg.launch.xwayland, cli.demo);
    let bin = (mode != nkw_core::rules::XwaylandMode::Off)
        .then(|| xwayland::which(xwayland::BINARY, std::env::var_os("PATH").as_deref()))
        .flatten();
    xwayland::Xwayland::new(mode, bin, PathBuf::from(xwayland::SOCKET_DIR))
}

/// A message from the clipboard thread (on the loop, with the hub).
fn on_host(st: &mut Nkw, m: FromHost) {
    match m {
        FromHost::Up(b) => st.ext.wm.clip.host = b,
        FromHost::Selection(mimes) => {
            st.ext.wm.copied = None;
            st.ext.wm.clip.mimes = mimes.len();
            st.ext.wm.clip.mirrored = true;
            set_data_device_selection(&st.dh, &st.seat, mimes, ());
        }
        FromHost::Cleared => {
            if st.ext.wm.clip.mirrored {
                clear_data_device_selection(&st.dh, &st.seat);
                st.ext.wm.clip.mirrored = false;
                st.ext.wm.clip.mimes = 0;
            }
        }
        FromHost::Send { mime, fd } => {
            // `world.copy`'s text, else our client writes into the host
            // app's fd (dropped on error)
            if let Err(fd) = st.ext.wm.send_copied(&mime, fd) {
                let _ = request_data_device_client_selection(&st.seat, mime, fd);
            }
        }
    }
}

fn arg_specs(args: &[(&str, &str)]) -> Vec<ArgSpec> {
    args.iter()
        .map(|(name, prompt)| ArgSpec {
            name: name.to_string(),
            prompt: prompt.to_string(),
            kind: if *name == "room" {
                ArgKind::Room
            } else {
                ArgKind::Text
            },
            required: true,
        })
        .collect()
}

fn keys(k: &[&str]) -> Vec<String> {
    k.iter().map(|s| s.to_string()).collect()
}

impl Plugin for Wm {
    fn name(&self) -> &'static str {
        "wm"
    }

    fn start(
        &mut self,
        cli: &crate::cli::Cli,
        lh: &calloop::LoopHandle<'static, Nkw>,
        ping: calloop::ping::Ping,
    ) {
        let _ = ping;
        self.lh = Some(lh.clone());
        lh.insert_idle(|st: &mut Nkw| {
            let dh = st.dh.clone();
            st.ext.wm.globals(&dh);
        });
        if cli.demo || cli.headless {
            return;
        }
        let (tx, rx) = calloop::channel::channel::<FromHost>();
        let ok = lh
            .insert_source(rx, |e, _, st: &mut Nkw| {
                if let calloop::channel::Event::Msg(m) = e {
                    on_host(st, m);
                }
            })
            .is_ok();
        if ok {
            self.bridge = clipboard::start(tx);
        }
    }

    fn tick(&mut self, ctx: &Ctx) -> Vec<Req> {
        let mut reqs = Vec::new();
        for e in self.errors.drain(..) {
            reqs.push(Req::Toast {
                text: e,
                kind: ToastKind::Error,
                ms: 6000,
            });
        }
        reqs.extend(self.xwayland_tick(ctx.now));
        reqs.extend(self.tour_tick(ctx.now));
        if std::mem::take(&mut self.restore) {
            reqs.extend(self.restore_reqs());
            // the restore itself is not a change worth a save
            return reqs;
        }
        self.track(ctx.world, ctx.now);
        if self.due.is_some_and(|t| t <= ctx.now) {
            self.save_now();
        }
        reqs
    }

    fn wishes(&self, ctx: &Ctx) -> Vec<Wish> {
        let w = ctx.world;
        let next = [
            self.due,
            w.layout_note(ctx.now).map(|n| n.1),
            w.attention(ctx.now).iter().map(|a| a.1).reduce(f64::min),
            self.xwayland.next_poll(ctx.now),
            self.tour_wish(),
        ]
        .into_iter()
        .flatten()
        .reduce(f64::min);
        next.map(|t| Wish::compose(Want::At(t)))
            .into_iter()
            .collect()
    }

    fn command(
        &mut self,
        name: &str,
        args: &BTreeMap<String, String>,
        ctx: &Ctx,
    ) -> Option<Vec<Req>> {
        let r = match name {
            "world.terminal" => {
                self.intent_until = Some(ctx.now + INTENT_S);
                let argv = self.terminal_with(args.get("cmd").map(String::as_str));
                self.spawn_reqs(argv, ctx.now)
            }
            apps::LAUNCH_ISOLATED => self.launch_isolated(args, ctx.now)?,
            apps::XWAYLAND => self.xwayland_cmd(args, ctx.now)?,
            apps::TOUR_DONE => {
                self.set_tour_done();
                Vec::new()
            }
            apps::CLOSE_SCREEN | apps::SEND_SCREEN => apps::screen_cmd(name, args, ctx)?,
            "world.launcher" => vec![Req::Action(Action::Palette)],
            "world.quit" => vec![Req::Quit(0)],
            "world.interact" => self.interact(ctx)?,
            "world.place_next" => self.place_next(args, ctx)?,
            "world.copy" => self.copy(args)?,
            LAUNCH => {
                let argv = launcher::split_exec(args.get("exec")?);
                let name = launch_env::program(&argv)?
                    .rsplit('/')
                    .next()
                    .unwrap_or_default()
                    .to_string();
                self.launch("", &name, argv, ctx.now)
            }
            n => {
                let id = n.strip_prefix("world.launch:")?;
                let app = self.apps().iter().find(|a| a.id == id)?.clone();
                self.launch(&app.id, &app.name, app.argv, ctx.now)
            }
        };
        Some(r)
    }

    fn inject(&mut self, json: &Value, ctx: &Ctx) -> Vec<Req> {
        let mut out = Vec::new();
        if let Some(name) = json.get("command").and_then(Value::as_str) {
            let args = json
                .get("args")
                .and_then(Value::as_object)
                .map(|m| {
                    m.iter()
                        .map(|(k, v)| {
                            (
                                k.clone(),
                                v.as_str()
                                    .map(String::from)
                                    .unwrap_or_else(|| v.to_string()),
                            )
                        })
                        .collect()
                })
                .unwrap_or_default();
            out.push(Req::Command {
                name: name.into(),
                args,
            });
        }
        // tests: where the satellite's socket and the processes are read
        if let Some(d) = json.get("x11_dir").and_then(Value::as_str) {
            self.xwayland.socket_dir = PathBuf::from(d);
        }
        if let Some(d) = json.get("proc_root").and_then(Value::as_str) {
            self.proc_root = Some(PathBuf::from(d));
            *self.outside_cache.borrow_mut() = None;
        }
        if json.get("save").and_then(Value::as_bool) == Some(true) {
            self.track(ctx.world, ctx.now);
            self.save_now();
        }
        out
    }

    fn palette_items(&self, ctx: &Ctx) -> Vec<PaletteItem> {
        let mut v: Vec<PaletteItem> = COMMANDS
            .iter()
            .filter(|c| c.palette)
            .map(|c| PaletteItem {
                name: c.name.into(),
                title: c.title.into(),
                keys: keys(c.keys),
                source: Source::World,
                args: arg_specs(c.args),
            })
            .collect();
        v.extend(WM_COMMANDS.iter().map(|(n, t, k)| PaletteItem {
            name: n.to_string(),
            title: t.to_string(),
            keys: keys(k),
            source: Source::World,
            args: Vec::new(),
        }));
        v.extend(apps::palette_items());
        v.extend(self.apps().iter().map(|a| PaletteItem {
            name: format!("world.launch:{}", a.id),
            title: a.name.clone(),
            keys: Vec::new(),
            source: Source::App {
                exec: a.argv.join(" "),
            },
            args: Vec::new(),
        }));
        let w = ctx.world;
        v.extend(w.rooms.iter().map(|r| PaletteItem {
            name: format!("world.goto_room:{}", r.id.0),
            title: format!("ir para {}", r.name),
            keys: Vec::new(),
            source: Source::Goto,
            args: Vec::new(),
        }));
        v.extend(w.screens.values().map(|s| PaletteItem {
            name: format!("world.goto_screen:{}", s.id.0),
            title: format!(
                "ir para {}",
                if s.title.is_empty() {
                    &s.app_id
                } else {
                    &s.title
                }
            ),
            keys: Vec::new(),
            source: Source::Goto,
            args: Vec::new(),
        }));
        v
    }

    fn pickables(&self, ctx: &Ctx) -> Vec<Pickable> {
        let w = ctx.world;
        let mut v = Vec::new();
        for (ri, room) in w.rooms.iter().enumerate() {
            if !ctx.visible_rooms.contains(&room.id) {
                continue;
            }
            for (di, desk) in room.desks.iter().enumerate() {
                for (k, sid) in desk.screens.iter().enumerate() {
                    let Some(station) = w.station_of(*sid) else {
                        continue;
                    };
                    let p = w.screen_place(ri, di, k);
                    let half = w.screen_logical(ri, di, k) * onetoone::M_PER_PX / 2.0;
                    v.push(Pickable {
                        pick: Pick::Screen {
                            station,
                            screen: *sid,
                        },
                        pos: p.center,
                        radius: half.length(),
                    });
                }
            }
        }
        // doors: from the hall every room's, from a room the ones it opens
        let in_hall = w.camera.pos.z.abs() < HALL_W / 2.0;
        let cur = w.current_room_id();
        for room in &w.rooms {
            let seen = in_hall || (room.id != cur && ctx.visible_rooms.contains(&room.id));
            if !seen {
                continue;
            }
            if let Some((xz, _)) = w.room_door(room.id) {
                v.push(Pickable {
                    pick: Pick::Door(room.id),
                    pos: Vec3::new(xz.x, 1.0, xz.y),
                    radius: 0.5,
                });
            }
        }
        v
    }

    fn tags(&self, ctx: &Ctx) -> Vec<Tag> {
        let w = ctx.world;
        let mut v = Vec::new();
        let top = |id| {
            let (r, d, k) = w.locate(id)?;
            let p = w.screen_place(r, d, k);
            let h = w.screen_logical(r, d, k).y * onetoone::M_PER_PX;
            Some(p.center + Vec3::Y * (h / 2.0 + 0.06))
        };
        if w.mode().focus().is_none()
            && !w.frozen
            && let Some(s) = w.selected()
            && let Some(sc) = w.screens.get(&s)
            && let Some(pos) = top(s)
        {
            let text = if sc.title.is_empty() {
                &sc.app_id
            } else {
                &sc.title
            };
            if !text.is_empty() {
                v.push(Tag {
                    id: TAG_TITLE + u64::from(s.0),
                    pos,
                    text: text.clone(),
                    rgb: TITLE_RGB,
                    max_m: 8.0,
                });
            }
        }
        for (s, _) in w.attention(ctx.now) {
            if let Some(pos) = top(s) {
                v.push(Tag {
                    id: TAG_ATTENTION + u64::from(s.0),
                    pos: pos + Vec3::Y * 0.08,
                    text: "quer atenção".into(),
                    rgb: ATTENTION_RGB,
                    max_m: 20.0,
                });
            }
        }
        v
    }

    fn badges(&self, ctx: &Ctx) -> Vec<Badge> {
        ctx.world
            .layout_note(ctx.now)
            .map(|(n, _)| Badge::Layout(n.to_string()))
            .into_iter()
            .collect()
    }

    fn dump(&self) -> Value {
        let rooms = self.rooms.clone();
        let places: serde_json::Map<String, Value> = self
            .memory
            .iter()
            .map(|(app, p)| {
                let desk = match p.desk {
                    Some(DeskAt::Index(i)) => json!(i),
                    Some(DeskAt::Last) => json!("last"),
                    None => Value::Null,
                };
                (app.clone(), json!([p.room, desk]))
            })
            .collect();
        json!({
            "rooms": rooms,
            "places": places,
            "rules": self.config.rules.len(),
            "clipboard": {"host": self.clip.host, "mimes": self.clip.mimes},
            "apps": self.apps.get().map_or(0, Vec::len),
            "saves": self.saves,
            "persist": self.store.is_some(),
            "xwayland": self.xwayland.enabled().then(|| json!({
                "display": self.xwayland.display(),
                "pid": self.xwayland.pid,
                "state": self.xwayland.state.label(),
            })),
            "tour_done": self.tour_done,
        })
    }

    fn status(&self, ctx: &Ctx) -> crate::ext::Status {
        let _ = ctx;
        crate::ext::Status {
            xwayland: self
                .xwayland
                .enabled()
                .then(|| self.xwayland.state == xwayland::XState::Up),
            ..crate::ext::Status::default()
        }
    }

    fn phone_apps(&self, ctx: &Ctx) -> Vec<crate::ui::phone::PhoneApp> {
        let _ = ctx;
        self.phone_apps_of()
    }

    fn phone_page(
        &self,
        app: &str,
        st: &crate::ui::phone::PhoneState,
        ctx: &Ctx,
    ) -> Option<crate::ui::phone::PhonePage> {
        self.phone_page_of(app, st, ctx)
    }

    fn shutdown(&mut self) {
        if self.store.is_some() && self.latest.is_some() {
            self.save_now();
        }
        if let Some(b) = self.bridge.as_mut() {
            b.shutdown();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::Cli;
    use crate::world::{Opts, Room, Screen};
    use glam::Vec2;
    use nkw_core::strip::{Desk, Width};
    use nkw_core::{Output, RoomId, ScreenId};

    fn world() -> World {
        let out = Output {
            width_px: 1280,
            height_px: 720,
            scale: 1.0,
        };
        let rooms = vec![Room {
            id: RoomId(0),
            name: "acme".into(),
            desks: vec![Desk {
                width: Width::Half,
                screens: vec![ScreenId(1)],
                cat: None,
            }],
        }];
        let mut w = World::with_rooms(
            out,
            Opts {
                reduced_motion: false,
            },
            rooms,
        );
        w.screens.insert(
            ScreenId(1),
            Screen {
                id: ScreenId(1),
                app_id: "acme.term".into(),
                title: "shell".into(),
                size: Vec2::new(624.0, 720.0),
            },
        );
        w
    }

    fn with_ctx<R>(w: &World, now: Time, f: impl FnOnce(&Ctx) -> R) -> R {
        let pids = BTreeMap::new();
        let visible = w.visible_rooms(&[]);
        let ctx = Ctx {
            world: w,
            now,
            frame: 0,
            reduced_motion: false,
            demo: true,
            scale: 1.0,
            size_px: [1280, 720],
            pids: &pids,
            visible_rooms: &visible,
            line: None,
        };
        f(&ctx)
    }

    /// A spawn with the launcher's default env (no X11).
    fn sw(argv: Vec<String>) -> Req {
        Req::SpawnWith {
            argv,
            env: launch_env::default_env(None),
        }
    }

    fn cli_in(dir: &std::path::Path) -> Cli {
        Cli {
            state_dir: Some(dir.to_path_buf()),
            headless: true,
            // never the host's satellite in unit tests
            xwayland: Some(crate::cli::XwaylandMode::Off),
            ..Cli::default()
        }
    }

    #[test]
    fn demo_has_no_persistence_and_fixture_apps() {
        let wm = Wm::new(&Cli {
            demo: true,
            ..Cli::default()
        });
        assert!(wm.store.is_none());
        let w = world();
        let items = with_ctx(&w, 0.0, |c| wm.palette_items(c));
        let names: Vec<&str> = items.iter().map(|i| i.name.as_str()).collect();
        assert!(names.contains(&"world.wider"));
        assert!(names.contains(&"world.terminal"));
        assert!(names.contains(&"world.launch:demo.alpha"));
        assert!(names.contains(&"world.goto_room:0"));
        assert!(names.contains(&"world.goto_screen:1"));
        assert!(
            !names.contains(&"world.forward"),
            "walking keys are not palette items"
        );
        let send = items.iter().find(|i| i.name == "world.send_room").unwrap();
        assert_eq!(send.args[0].kind, ArgKind::Room);
        let goto = items
            .iter()
            .find(|i| i.name == "world.goto_screen:1")
            .unwrap();
        assert_eq!(goto.title, "ir para shell");
        assert_eq!(goto.source, Source::Goto);
        assert_eq!(wm.dump()["persist"], false);
        assert_eq!(wm.dump()["apps"], 2);
    }

    #[test]
    fn saves_two_seconds_after_the_last_change_and_restores() {
        let t = tempfile::tempdir().unwrap();
        std::fs::write(
            t.path().join("world.toml"),
            "[[rules]]\napp_id = \"demo.*\"\nroom = \"bravo\"\n",
        )
        .unwrap();
        let mut wm = Wm::new(&cli_in(t.path()));
        assert_eq!(wm.dump()["rules"], 1);
        let mut w = world();
        with_ctx(&w, 0.0, |c| wm.tick(c));
        assert_eq!(wm.due, Some(2.0));
        let wishes = with_ctx(&w, 0.0, |c| wm.wishes(c));
        assert_eq!(
            wishes,
            [Wish::compose(Want::At(apps::TOUR_AFTER_S))],
            "the tour comes first (a fresh state dir), then the save at 2"
        );
        // another change pushes the save back
        w.command(Cmd::Wider, 1.0);
        with_ctx(&w, 1.0, |c| wm.tick(c));
        assert_eq!(wm.due, Some(3.0));
        with_ctx(&w, 2.5, |c| wm.tick(c));
        assert_eq!(wm.saves, 0);
        with_ctx(&w, 3.0, |c| wm.tick(c));
        assert_eq!(wm.saves, 1);
        assert!(
            with_ctx(&w, 9.0, |c| wm.wishes(c)).is_empty(),
            "at rest: no timer"
        );
        let saved = nkw_core::persist::parse(
            &std::fs::read_to_string(t.path().join("world.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(saved.desks[0].width, Width::TwoThirds);
        assert_eq!(saved.places["acme.term"].room.as_deref(), Some("acme"));
        // no change: no new save
        with_ctx(&w, 9.0, |c| wm.tick(c));
        assert_eq!(wm.saves, 1);

        // a new run: the rooms come back, the window goes back to its place
        let mut wm2 = Wm::new(&cli_in(t.path()));
        let mut fresh = World::new(w.out, w.opts);
        let reqs = with_ctx(&fresh, 0.0, |c| wm2.tick(c));
        assert!(
            reqs.iter()
                .any(|r| matches!(r, Req::Cmd(Cmd::RoomNew { go: false, .. }))),
            "{reqs:?}"
        );
        for r in reqs {
            if let Req::Cmd(c) = r {
                fresh.command(c, 0.0);
            }
        }
        let names: Vec<_> = fresh.rooms.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, ["acme", "minha sala"]);
        let p = wm2.place_for("acme.term", 0.0).unwrap();
        assert_eq!(p.room.as_deref(), Some("acme"));
        assert_eq!(p.width, Some(Width::TwoThirds));
        // the rule beats memory for the room
        assert_eq!(
            wm2.place_for("demo.x", 0.0).unwrap().room.as_deref(),
            Some("bravo")
        );
    }

    #[test]
    fn a_corrupted_file_toasts_once() {
        let t = tempfile::tempdir().unwrap();
        std::fs::write(t.path().join("world.json"), "{{{").unwrap();
        let mut wm = Wm::new(&cli_in(t.path()));
        let w = world();
        let reqs = with_ctx(&w, 0.0, |c| wm.tick(c));
        assert!(matches!(
            &reqs[0],
            Req::Toast {
                kind: ToastKind::Error,
                ..
            }
        ));
        assert!(t.path().join("world.json.bak").exists());
        let again = with_ctx(&w, 0.1, |c| wm.tick(c));
        assert!(again.iter().all(|r| !matches!(r, Req::Toast { .. })));
    }

    #[test]
    fn terminal_intent_beats_the_rules() {
        let t = tempfile::tempdir().unwrap();
        std::fs::write(t.path().join("world.toml"), "[[rules]]\napp_id = \"foot\"\nroom = \"bravo\"\n\n[launch]\nterminal = \"foot -o x=1\"\n").unwrap();
        let mut wm = Wm::new(&cli_in(t.path()));
        let w = world();
        let r = with_ctx(&w, 10.0, |c| {
            wm.command("world.terminal", &BTreeMap::new(), c)
        })
        .unwrap();
        assert_eq!(r, [sw(vec!["foot".into(), "-o".into(), "x=1".into()])]);
        assert_eq!(wm.place_for("foot", 11.0), None, "right of the station");
        assert!(wm.place_for("foot", 12.0).is_some(), "only once");
        // stale intent
        with_ctx(&w, 10.0, |c| {
            wm.command("world.terminal", &BTreeMap::new(), c)
        });
        assert!(wm.place_for("foot", 10.0 + INTENT_S + 1.0).is_some());
    }

    #[test]
    fn commands_of_the_plugin() {
        let mut wm = Wm::new(&Cli {
            demo: true,
            ..Cli::default()
        });
        let w = world();
        let no = BTreeMap::new();
        with_ctx(&w, 0.0, |c| {
            assert_eq!(wm.command("world.quit", &no, c), Some(vec![Req::Quit(0)]));
            assert_eq!(
                wm.command("world.launcher", &no, c),
                Some(vec![Req::Action(Action::Palette)])
            );
            // the World's now (`Cmd::from_name` runs first)
            assert_eq!(wm.command("world.library", &no, c), None);
            assert_eq!(wm.command("world.grab", &no, c), None);
            assert_eq!(
                wm.command("world.terminal", &no, c),
                Some(vec![sw(vec![
                    "testclient".into(),
                    "--app-id".into(),
                    "demo.term".into()
                ])])
            );
            let exec = BTreeMap::from([("exec".to_string(), "mpv --loop \"a b.mkv\"".to_string())]);
            assert_eq!(
                wm.command(LAUNCH, &exec, c),
                Some(vec![sw(vec![
                    "mpv".into(),
                    "--loop".into(),
                    "a b.mkv".into()
                ])])
            );
            assert!(
                matches!(&wm.command("world.launch:demo.bravo", &no, c).unwrap()[0], Req::SpawnWith { argv: a, .. } if a[0] == "testclient")
            );
            assert_eq!(wm.command("world.launch:nope", &no, c), None);
            assert_eq!(wm.command("world.summon", &no, c), None, "the cats'");
            let inj = wm.inject(&json!({"command": "world.wider", "args": {"n": 2}}), c);
            assert_eq!(
                inj,
                [Req::Command {
                    name: "world.wider".into(),
                    args: BTreeMap::from([("n".to_string(), "2".to_string())])
                }]
            );
        });
    }

    #[test]
    fn pickables_tags_badges() {
        let wm = Wm::new(&Cli {
            demo: true,
            ..Cli::default()
        });
        let mut w = world();
        let picks = with_ctx(&w, 0.0, |c| wm.pickables(c));
        assert_eq!(picks.len(), 1, "one screen, no door from inside the room");
        assert!(matches!(
            picks[0].pick,
            Pick::Screen {
                screen: ScreenId(1),
                ..
            }
        ));
        let tags = with_ctx(&w, 0.0, |c| wm.tags(c));
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].text, "shell");
        assert!(tags[0].pos.y > w.screen_place(0, 0, 0).center.y);
        assert!(with_ctx(&w, 0.0, |c| wm.badges(c)).is_empty());
        w.command(Cmd::Narrower, 0.0);
        w.activate(ScreenId(1), false, 0.0);
        assert_eq!(
            with_ctx(&w, 1.0, |c| wm.badges(c)),
            [Badge::Layout("1/3".into())]
        );
        let tags = with_ctx(&w, 1.0, |c| wm.tags(c));
        assert!(tags.iter().any(|t| t.text == "quer atenção"));
        let wishes = with_ctx(&w, 1.0, |c| wm.wishes(c));
        assert_eq!(
            wishes,
            [Wish::compose(Want::At(crate::world::LAYOUT_NOTE_S))]
        );
        // from the hall the doors are targets
        w.camera.pos.z = 0.0;
        let picks = with_ctx(&w, 9.0, |c| wm.pickables(c));
        assert!(
            picks
                .iter()
                .any(|p| matches!(p.pick, Pick::Door(RoomId(0))))
        );
        assert!(with_ctx(&w, 9.0, |c| wm.wishes(c)).is_empty());
    }

    #[test]
    fn tick_is_cheap_even_when_the_world_changes_every_frame() {
        let mut wm = Wm::new(&Cli {
            demo: true,
            ..Cli::default()
        });
        let out = Output {
            width_px: 1280,
            height_px: 720,
            scale: 1.0,
        };
        let mut w = crate::demo::world(
            out,
            Opts {
                reduced_motion: false,
            },
        );
        let t = std::time::Instant::now();
        for i in 0..200 {
            w.command(Cmd::Move(nkw_core::Dir::Right), i as f64);
            with_ctx(&w, i as f64, |c| {
                wm.tick(c);
                wm.wishes(c);
            });
        }
        let per = t.elapsed() / 200;
        assert!(
            per < std::time::Duration::from_micros(500),
            "{per:?} per tick"
        );
    }

    #[test]
    fn a_saved_library_is_restored_as_the_library() {
        let t = tempfile::tempdir().unwrap();
        let mut w = World::new(world().out, world().opts);
        w.sync_contexts(&[], 0.0);
        w.ensure_room(RoomKind::Library, crate::world::LIBRARY, 0.0);
        std::fs::write(
            t.path().join("world.json"),
            nkw_core::persist::to_string(&w.snapshot()),
        )
        .unwrap();
        let mut wm = Wm::new(&cli_in(t.path()));
        let mut fresh = World::new(w.out, w.opts);
        fresh.sync_contexts(&[], 0.0);
        let reqs = with_ctx(&fresh, 0.0, |c| wm.tick(c));
        assert!(
            reqs.contains(&Req::Cmd(Cmd::Library { go: false })),
            "{reqs:?}"
        );
        for r in reqs {
            if let Req::Cmd(c) = r {
                fresh.command(c, 0.0);
            }
        }
        assert_eq!(fresh.room_kind(fresh.rooms[1].id), RoomKind::Library);
        assert_eq!(fresh.rooms[1].desks[0].cat, Some(crate::world::LIBRARIAN));
    }

    #[test]
    fn terminal_with_a_command() {
        let t = tempfile::tempdir().unwrap();
        std::fs::write(
            t.path().join("world.toml"),
            "[launch]\nterminal = \"foot -o x=1\"\n",
        )
        .unwrap();
        let mut wm = Wm::new(&cli_in(t.path()));
        let w = world();
        let args = BTreeMap::from([("cmd".to_string(), "$EDITOR notas.md".to_string())]);
        let r = with_ctx(&w, 0.0, |c| wm.command("world.terminal", &args, c)).unwrap();
        assert_eq!(
            r,
            [sw([
                "foot",
                "-o",
                "x=1",
                "-e",
                "sh",
                "-c",
                "$EDITOR notas.md"
            ]
            .map(String::from)
            .to_vec())]
        );
        let blank = BTreeMap::from([("cmd".to_string(), "  ".to_string())]);
        let r = with_ctx(&w, 0.0, |c| wm.command("world.terminal", &blank, c)).unwrap();
        assert_eq!(r, [sw(["foot", "-o", "x=1"].map(String::from).to_vec())]);
    }

    #[test]
    fn interact_falls_back_on_the_worlds_aim() {
        let mut wm = Wm::new(&Cli {
            demo: true,
            ..Cli::default()
        });
        let mut w = world();
        let no = BTreeMap::new();
        assert_eq!(
            with_ctx(&w, 0.0, |c| wm.command("world.interact", &no, c)),
            None,
            "NAV: nothing in the aim, nobody takes it"
        );
        w.command(Cmd::Walk, 0.0);
        assert_eq!(w.aim(), Some(Aim::Screen(ScreenId(1))));
        assert_eq!(
            with_ctx(&w, 0.0, |c| wm.command("world.interact", &no, c)),
            Some(vec![Req::Action(Action::Focus(Target::Screen(ScreenId(
                1
            ))))])
        );
    }

    #[test]
    fn the_placement_intent() {
        let mut wm = Wm::new(&Cli {
            demo: true,
            ..Cli::default()
        });
        let mut w = world();
        let (lib, _) = w.ensure_room(RoomKind::Library, "biblioteca", 0.0);
        let args = BTreeMap::from([
            ("room".to_string(), lib.0.to_string()),
            ("desk".to_string(), "0".to_string()),
        ]);
        let r = with_ctx(&w, 10.0, |c| wm.command("world.place_next", &args, c));
        assert_eq!(r, Some(vec![]));
        let no = |_: u32| false;
        let p = wm
            .take_intent(None, &no, 11.0)
            .expect("demo: the next window");
        assert_eq!(p.room.as_deref(), Some("biblioteca"));
        assert_eq!(p.desk, Some(DeskAt::Index(0)));
        assert_eq!(wm.take_intent(None, &no, 11.0), None, "once");
        // it expires; a newer one replaces the older
        let by_name = BTreeMap::from([
            ("room".to_string(), "acme".to_string()),
            ("ttl_s".to_string(), "1".to_string()),
        ]);
        with_ctx(&w, 20.0, |c| wm.command("world.place_next", &args, c));
        with_ctx(&w, 20.0, |c| wm.command("world.place_next", &by_name, c));
        assert_eq!(wm.take_intent(None, &no, 21.5), None, "expired");
        with_ctx(&w, 30.0, |c| wm.command("world.place_next", &by_name, c));
        let p = wm.take_intent(None, &no, 30.5).unwrap();
        assert_eq!((p.room.as_deref(), p.desk), (Some("acme"), None));
        assert_eq!(
            with_ctx(&w, 0.0, |c| wm.command(
                "world.place_next",
                &BTreeMap::new(),
                c
            )),
            None,
            "needs a room"
        );
        // not under --demo: only a window of a process we spawned (or its child)
        let t = tempfile::tempdir().unwrap();
        let mut real = Wm::new(&cli_in(t.path()));
        with_ctx(&w, 0.0, |c| real.command("world.place_next", &args, c));
        let me = std::process::id();
        assert_eq!(
            real.take_intent(Some(me), &|p| p == 1, 0.5),
            None,
            "not ours"
        );
        assert_eq!(real.take_intent(None, &|_| true, 0.5), None, "no pid");
        let mine = real.take_intent(Some(me), &|p| p == me, 0.5);
        assert!(mine.is_some(), "ours");
    }

    #[test]
    fn copy_keeps_the_text_out_of_the_dump() {
        let mut wm = Wm::new(&Cli {
            demo: true,
            ..Cli::default()
        });
        let w = world();
        let args = BTreeMap::from([("text".to_string(), "/srv/projeto/notas.md".to_string())]);
        let r = with_ctx(&w, 0.0, |c| wm.command("world.copy", &args, c));
        assert_eq!(r, Some(vec![]));
        assert!(!wm.dump().to_string().contains("notas"));
        assert_eq!(wm.copied.as_deref(), Some("/srv/projeto/notas.md"));
        // a paste of ours gets the bytes
        let (rd, wr) = std::io::pipe().unwrap();
        assert!(wm.send_copied("text/plain", wr.into()).is_ok());
        let mut got = String::new();
        std::io::Read::read_to_string(&mut std::fs::File::from(OwnedFd::from(rd)), &mut got)
            .unwrap();
        assert_eq!(got, "/srv/projeto/notas.md");
        // a client's selection takes over
        wm.our_selection(vec!["text/plain".into()]);
        let (_rd, wr) = std::io::pipe().unwrap();
        assert!(wm.send_copied("text/plain", wr.into()).is_err());
        assert_eq!(
            with_ctx(&w, 0.0, |c| wm.command("world.copy", &BTreeMap::new(), c)),
            None
        );
    }

    #[test]
    fn walk_config_from_the_file_and_reduced_motion() {
        let t = tempfile::tempdir().unwrap();
        std::fs::write(
            t.path().join("world.toml"),
            "[walk]\nspeed = 9\n[camera]\neye_height = 1.6\n",
        )
        .unwrap();
        let wm = Wm::new(&cli_in(t.path()));
        assert_eq!(wm.config().walk.speed, 5.0, "clamped");
        assert_eq!(wm.config().walk.eye_height, 1.6);
        assert_eq!(wm.config().walk.turn, nkw_core::TurnMode::Smooth);
        let wm = Wm::new(&Cli {
            reduced_motion: true,
            ..cli_in(t.path())
        });
        assert_eq!(
            wm.config().walk.turn,
            nkw_core::TurnMode::Snap,
            "reduced motion snaps"
        );
        std::fs::write(t.path().join("world.toml"), "[walk]\nturn = \"smooth\"\n").unwrap();
        let wm = Wm::new(&Cli {
            reduced_motion: true,
            ..cli_in(t.path())
        });
        assert_eq!(
            wm.config().walk.turn,
            nkw_core::TurnMode::Smooth,
            "unless the file says so"
        );
    }

    #[test]
    fn dump_never_has_clipboard_text() {
        let mut wm = Wm::new(&Cli {
            demo: true,
            ..Cli::default()
        });
        wm.our_selection(vec![
            "text/plain;charset=utf-8".into(),
            "UTF8_STRING".into(),
        ]);
        let d = wm.dump();
        assert_eq!(d["clipboard"], json!({"host": false, "mimes": 2}));
        let s = d.to_string();
        assert!(!s.contains("text/plain"), "{s}");
        for k in ["rooms", "places", "rules", "clipboard", "apps"] {
            assert!(d.get(k).is_some(), "{k}");
        }
    }
}
