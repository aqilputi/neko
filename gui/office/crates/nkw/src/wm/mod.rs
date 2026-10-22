//! The window manager's plugin (§4, phase 1): what the palette, SALTO and
//! the HUD get from the WM, `world.json` (rooms, desks, rules, the place
//! memory), the launcher, `world.terminal`, the clipboard bridge to the
//! host, and the Smithay states the xdg handlers need (activation, dialog).
//!
//! The layout operations themselves are the World's (`world_ops.rs`, a
//! child module of `world`); the xdg side is `handlers.rs`.
//!
//! `inject wm` shapes: `{"command": "world.wider", "args": {…}}` (dispatched
//! like a key), `{"save": true}` (write `world.json` now).
//!
//! `dump` = `{"rooms":[{id,name,kind}],"places":{app_id:[room,desk]},
//! "rules":n,"clipboard":{"host":bool,"mimes":n},"apps":n,"saves":n,
//! "persist":bool}` — never any clipboard content.

pub mod clipboard;
pub mod launcher;
pub mod persist_io;

use std::cell::OnceCell;
use std::collections::BTreeMap;
use std::os::fd::OwnedFd;
use std::path::PathBuf;

use glam::Vec3;
use nkw_core::persist::{DeskAt, Place, Plan, RoomKind, WorldFile};
use nkw_core::rules::{self, Config};
use nkw_core::{Action, Pick, Time, Want, Wish, onetoone};
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
use crate::world::{COMMANDS, Cmd, HALL_W, RoomRef, ShortName, World};
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
    ("world.library", "biblioteca (fase 4: toast)", &["b"]),
    ("world.quit", "sair do mundo (confirma)", &["world+shift+e"]),
    ("world.grab", "pegar/soltar a tela (fase 3: toast)", &["g"]),
    ("world.nearer", "tela na mão (fase 3: toast)", &["["]),
    ("world.farther", "tela na mão (fase 3: toast)", &["]"]),
];

/// `world.launch {exec}` / `world.launch:<desktop id>`: run an app.
pub const LAUNCH: &str = "world.launch";

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
        };
        if cli.demo {
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
        }
        if let Some(dir) = cli.state_root() {
            let store = Store::new(dir);
            match store.load() {
                Load::Missing => {}
                Load::Ok(f) => {
                    wm.plan = nkw_core::persist::apply(&f, &[]);
                    wm.memory = wm.plan.places.clone();
                    wm.last_saved = Some(f);
                    wm.restore = true;
                }
                Load::Corrupt(msg) => wm.errors.push(msg),
            }
            wm.store = Some(store);
        }
        wm
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
            .map(|(name, kind)| {
                Req::Cmd(Cmd::RoomNew {
                    name: ShortName::new(name),
                    context: matches!(kind, RoomKind::Context(_)),
                    go: false,
                })
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

/// A message from the clipboard thread (on the loop, with the hub).
fn on_host(st: &mut Nkw, m: FromHost) {
    match m {
        FromHost::Up(b) => st.ext.wm.clip.host = b,
        FromHost::Selection(mimes) => {
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
            // our client writes into the host app's fd (dropped on error)
            let _ = request_data_device_client_selection(&st.seat, mime, fd);
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

fn toast(text: &str) -> Req {
    Req::Toast {
        text: text.into(),
        kind: ToastKind::Info,
        ms: 3000,
    }
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
                vec![Req::Spawn(self.terminal())]
            }
            "world.launcher" => vec![Req::Action(Action::Palette)],
            "world.library" => vec![toast("biblioteca: chega na fase 4")],
            "world.grab" | "world.nearer" | "world.farther" => {
                vec![toast("tela na mão: chega na fase 3")]
            }
            "world.quit" => vec![Req::Quit(0)],
            LAUNCH => {
                let argv = launcher::split_exec(args.get("exec")?);
                if argv.is_empty() {
                    return None;
                }
                vec![Req::Spawn(argv)]
            }
            n => {
                let id = n.strip_prefix("world.launch:")?;
                let app = self.apps().iter().find(|a| a.id == id)?;
                vec![Req::Spawn(app.argv.clone())]
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
        })
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

    fn cli_in(dir: &std::path::Path) -> Cli {
        Cli {
            state_dir: Some(dir.to_path_buf()),
            headless: true,
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
        assert_eq!(wishes, [Wish::compose(Want::At(2.0))]);
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
        assert_eq!(
            r,
            [Req::Spawn(vec!["foot".into(), "-o".into(), "x=1".into()])]
        );
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
            assert!(matches!(
                wm.command("world.library", &no, c).unwrap()[0],
                Req::Toast { .. }
            ));
            assert_eq!(
                wm.command("world.terminal", &no, c),
                Some(vec![Req::Spawn(vec![
                    "testclient".into(),
                    "--app-id".into(),
                    "demo.term".into()
                ])])
            );
            let exec = BTreeMap::from([("exec".to_string(), "mpv --loop \"a b.mkv\"".to_string())]);
            assert_eq!(
                wm.command(LAUNCH, &exec, c),
                Some(vec![Req::Spawn(vec![
                    "mpv".into(),
                    "--loop".into(),
                    "a b.mkv".into()
                ])])
            );
            assert!(
                matches!(&wm.command("world.launch:demo.bravo", &no, c).unwrap()[0], Req::Spawn(a) if a[0] == "testclient")
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
