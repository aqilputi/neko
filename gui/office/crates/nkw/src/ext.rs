//! The plugin seam (contract S7 of the wave plan): the hub owns the Smithay
//! side and calls these hooks at fixed points; the plugins (`wm`, `objects`,
//! `cats`, `overlay`) never touch Smithay objects except through the types
//! in the hook signatures. Side effects go out as [`Req`]s, applied in order
//! right after the hook returns (`Nkw::apply_reqs`).

use std::collections::BTreeMap;

use glam::{Vec2, Vec3};
use nkw_core::{
    Aabb, Action, CatId, Effect, ObjectAction, ObjectId, Pick, Pose, RoomId, ScreenId, TalkPurpose,
    Time, Wish,
};
use nkw_gl::draw::DrawList;
use serde_json::Value;
use smithay::backend::renderer::element::memory::MemoryRenderBuffer;

use crate::world::{Cmd, World};

/// Read-only view of the hub for one hook call.
pub struct Ctx<'a> {
    pub world: &'a World,
    /// the world clock, seconds (same as `World::tick`'s `now`)
    pub now: Time,
    /// frames composed so far
    pub frame: u64,
    pub reduced_motion: bool,
    /// `--demo`: fixtures only, no processes, no `$HOME`
    pub demo: bool,
    /// output scale (1.0, 1.25 …)
    pub scale: f64,
    /// output size, physical px
    pub size_px: [u32; 2],
    /// client pids from Smithay credentials (audio anchoring, agent desks)
    pub pids: &'a BTreeMap<ScreenId, u32>,
    /// this frame's culling set: current room + what plugins opened
    pub visible_rooms: &'a [RoomId],
    /// the overlay line's text, only while routing `Effect::SendLine`
    pub line: Option<&'a str>,
}

/// What a plugin asks the hub to do; applied in order right after the hook
/// returns.
#[derive(Clone, Debug, PartialEq)]
pub enum Req {
    /// `World::action` → machine → effects
    Action(Action),
    /// a world command by value (as a key would)
    Cmd(Cmd),
    /// by name: `Cmd::from_name`, else offered to the plugins
    Command {
        name: String,
        args: BTreeMap<String, String>,
    },
    /// scheduling (merged with the World's wishes)
    Wish(Wish),
    /// camera glide ≤ 260 ms (cut under reduced motion); station → `Free(pose)`
    GlideTo(Pose),
    /// argv with the world's `WAYLAND_DISPLAY` (never in `--demo` tests
    /// without the testclient); `= SpawnWith { argv, env: [] }`
    Spawn(Vec<String>),
    /// argv + extra env for the child, applied after the hub's own
    /// (`WAYLAND_DISPLAY`, no `DISPLAY` unless given here, a fresh
    /// `XDG_ACTIVATION_TOKEN`) (S20)
    SpawnWith {
        argv: Vec<String>,
        env: Vec<(String, String)>,
    },
    Configure {
        screen: ScreenId,
        size: Option<Vec2>,
        fullscreen: Option<bool>,
        maximized: Option<bool>,
    },
    /// `xdg_toplevel.close` (never kills)
    Close(ScreenId),
    /// → `Overlay::toast`
    Toast {
        text: String,
        kind: ToastKind,
        ms: u32,
    },
    /// → `Objects::cue` (own sounds; `None` = at the listener)
    Cue {
        cue: nkw_objects::sound::Cue,
        at: Option<Vec3>,
    },
    /// → `Router::set_bindings` (from `/v1/keymap`)
    Keymap(Vec<nk_client::Binding>),
    Quit(i32),
    /// the office's contexts changed: `World::sync_contexts(&c, now)` →
    /// `Nkw::apply(outs)` (S14)
    SyncContexts(Vec<(String, Vec<CatId>)>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToastKind {
    Info,
    Error,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Source {
    World,
    Core,
    App { exec: String },
    Goto,
    Object,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArgKind {
    Text,
    Session,
    Room,
    Dir,
    Int,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArgSpec {
    pub name: String,
    pub prompt: String,
    pub kind: ArgKind,
    pub required: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PaletteItem {
    pub name: String,
    /// pt-BR
    pub title: String,
    pub keys: Vec<String>,
    pub source: Source,
    pub args: Vec<ArgSpec>,
}

/// A SALTO target: the overlay projects `pos`, assigns the label, and on a
/// match sends `Req::Action(Action::Pick(pick))`.
#[derive(Clone, Debug, PartialEq)]
pub struct Pickable {
    pub pick: Pick,
    pub pos: Vec3,
    pub radius: f32,
}

/// A 2D text tag anchored in the world (name badges, readouts, the selected
/// screen's title). `max_m`: hidden beyond.
#[derive(Clone, Debug, PartialEq)]
pub struct Tag {
    pub id: u64,
    pub pos: Vec3,
    pub text: String,
    pub rgb: [u8; 3],
    pub max_m: f32,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Badge {
    Inbox(u32),
    Confirm,
    BatteryLow(u8),
    NetDown,
    VaultOpen,
    Updates(u32),
    OnAir,
    Layout(String),
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct LineView {
    pub text: String,
    pub cursor: usize,
}

#[derive(Clone, Debug, PartialEq, Hash)]
pub struct DialogView {
    pub name: String,
    pub rgb: [u8; 3],
    /// revealed so far
    pub text: String,
    pub more: bool,
    pub choices: Vec<String>,
    pub selected: usize,
    pub line: Option<LineView>,
}

/// What the top strip and the phone's header show (S20). Every plugin fills
/// what it knows ([`Plugin::status`]); the hub merges in plugin order
/// ([`Status::merge`]: `Some` wins over `None`, bools `|`). Never a media
/// title, SSID or device name.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, serde::Serialize)]
pub struct Status {
    /// objects: wall clock hh:mm (fixture 14:05 under `--demo`)
    pub clock: Option<(u8, u8)>,
    /// objects: day band 0..7
    pub band: Option<u8>,
    /// objects: (pct, charging); `None` = no battery / no data
    pub battery: Option<(u8, bool)>,
    /// objects: (connected, bars 0..4)
    pub net: Option<(bool, u8)>,
    /// objects: a player is playing
    pub playing: Option<bool>,
    /// objects
    pub vault_open: bool,
    /// cats: cats waiting for you (queue + overflow)
    pub inbox: Option<u32>,
    /// cats: (active, blocked)
    pub agents: Option<(u32, u32)>,
    /// cats: today, cents
    pub cost_cents: Option<u32>,
    /// cats
    pub usage_5h_pct: Option<u8>,
    /// wm: the xwayland satellite is up
    pub xwayland: Option<bool>,
}

impl Status {
    /// `self` is the earlier plugin: its `Some`s win; `other` fills the
    /// `None`s; bools are or-ed.
    pub fn merge(self, other: Status) -> Status {
        Status {
            clock: self.clock.or(other.clock),
            band: self.band.or(other.band),
            battery: self.battery.or(other.battery),
            net: self.net.or(other.net),
            playing: self.playing.or(other.playing),
            vault_open: self.vault_open | other.vault_open,
            inbox: self.inbox.or(other.inbox),
            agents: self.agents.or(other.agents),
            cost_cents: self.cost_cents.or(other.cost_cents),
            usage_5h_pct: self.usage_5h_pct.or(other.usage_5h_pct),
            xwayland: self.xwayland.or(other.xwayland),
        }
    }
}

/// What the overlay composes each frame (the hub assembles it from the
/// plugins).
pub struct HudInput {
    pub mode_label: &'static str,
    pub badges: Vec<Badge>,
    pub dialog: Option<DialogView>,
    /// logical px
    pub focus_rect: Option<[i32; 4]>,
    pub tags: Vec<Tag>,
    /// gathered from every plugin while SALTO is open, else empty
    pub pickables: Vec<Pickable>,
    /// ANDAR: something interactable is in the aim (`World::aim()` or
    /// `Objects::aimed()`); the crosshair becomes a ring (S14)
    pub aimed: bool,
    /// the plugins' merged status (S20)
    pub status: Status,
    /// the phone, while the overlay says it is open (`Overlay::phone_state()`):
    /// apps of every plugin and the current app's page — gathered by the
    /// hub, drawn and navigated by the overlay (S20/S21)
    pub phone: Option<crate::ui::phone::PhoneView>,
}

impl HudInput {
    /// Something on the HUD can change without a wish of its own (a dialog,
    /// a badge, SALTO's labels, the phone): the hub then compares
    /// [`hud_hash`] on frames that run anyway.
    pub fn busy(&self) -> bool {
        self.dialog.is_some()
            || !self.badges.is_empty()
            || !self.pickables.is_empty()
            || self.phone.is_some()
    }
}

/// A 64-bit hash of what the overlay draws from a [`HudInput`]: `(dialog,
/// badges, tags, focus_rect, mode_label, aimed, status, phone)`. A different hash than the
/// last composed one makes the frame compose even with no wish (S14; the
/// stale dialogue of wave 2).
pub fn hud_hash(h: &HudInput) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut s = std::collections::hash_map::DefaultHasher::new();
    h.mode_label.hash(&mut s);
    h.badges.hash(&mut s);
    h.dialog.hash(&mut s);
    h.focus_rect.hash(&mut s);
    h.aimed.hash(&mut s);
    h.status.hash(&mut s);
    h.phone.hash(&mut s);
    h.tags.len().hash(&mut s);
    for t in &h.tags {
        t.id.hash(&mut s);
        t.text.hash(&mut s);
        t.rgb.hash(&mut s);
        for f in [t.pos.x, t.pos.y, t.pos.z, t.max_m] {
            f.to_bits().hash(&mut s);
        }
    }
    s.finish()
}

/// One 2D element, logical px, top-left origin. The overlay owns the buffer
/// and its cache.
pub struct OverlayElem {
    pub id: u64,
    pub pos: (i32, i32),
    pub buf: MemoryRenderBuffer,
    /// buffer px per logical px: 1.0 = integer-scale raster (the buffer's
    /// own scale); `Ctx.scale` when the overlay rasters at physical px (S14)
    pub buf_scale: f64,
}

pub trait Plugin {
    /// "wm" | "overlay" | "cats" | "objects"
    fn name(&self) -> &'static str;
    /// Once, after the loop exists: register fds/channels on `lh`; threads
    /// wake the loop with `ping.ping()`.
    fn start(
        &mut self,
        cli: &crate::cli::Cli,
        lh: &calloop::LoopHandle<'static, crate::state::Nkw>,
        ping: calloop::ping::Ping,
    ) {
        let _ = (cli, lh, ping);
    }
    /// On every wakeup, before `tick`: drain channels. Must be cheap when
    /// nothing arrived.
    fn drain(&mut self, ctx: &Ctx) -> Vec<Req> {
        let _ = ctx;
        Vec::new()
    }
    /// Every frame, before the plan.
    fn tick(&mut self, ctx: &Ctx) -> Vec<Req> {
        let _ = ctx;
        Vec::new()
    }
    /// The plugin's wishes for this frame (merged with the World's; none at
    /// rest = no timer, S1).
    fn wishes(&self, ctx: &Ctx) -> Vec<Wish> {
        let _ = ctx;
        Vec::new()
    }
    /// Before the world pass, no Smithay frame open. Upload/refresh GL
    /// textures (monitors, covers).
    ///
    /// # Safety
    /// `gl` is current on this thread.
    unsafe fn upload(&mut self, gl: &nkw_gl::glow::Context, ctx: &Ctx) {
        let _ = (gl, ctx);
    }
    /// Add instances/screens/blobs for this frame (only for rooms in
    /// `ctx.visible_rooms`).
    fn draw(&self, ctx: &Ctx, list: &mut DrawList) {
        let _ = (ctx, list);
    }
    /// A command by name the router/palette resolved and `Cmd::from_name`
    /// did not take. `None` = not mine.
    fn command(
        &mut self,
        name: &str,
        args: &BTreeMap<String, String>,
        ctx: &Ctx,
    ) -> Option<Vec<Req>> {
        let _ = (name, args, ctx);
        None
    }
    /// A machine effect the World passed through (`Out::Effect`), routed by
    /// the hub ([`route_effect`]).
    fn effect(&mut self, e: &Effect, ctx: &Ctx) -> Vec<Req> {
        let _ = (e, ctx);
        Vec::new()
    }
    /// `inject <plugin> <json>` from a script (fixtures, fake events). The
    /// shape is the plugin's, documented in its package.
    fn inject(&mut self, json: &Value, ctx: &Ctx) -> Vec<Req> {
        let _ = (json, ctx);
        Vec::new()
    }
    fn palette_items(&self, ctx: &Ctx) -> Vec<PaletteItem> {
        let _ = ctx;
        Vec::new()
    }
    fn pickables(&self, ctx: &Ctx) -> Vec<Pickable> {
        let _ = ctx;
        Vec::new()
    }
    fn tags(&self, ctx: &Ctx) -> Vec<Tag> {
        let _ = ctx;
        Vec::new()
    }
    fn badges(&self, ctx: &Ctx) -> Vec<Badge> {
        let _ = ctx;
        Vec::new()
    }
    /// Extra floor boxes for the room's occupancy grid (closed doors,
    /// furniture).
    fn obstacles(&self, room: RoomId, ctx: &Ctx) -> Vec<Aabb> {
        let _ = (room, ctx);
        Vec::new()
    }
    /// Rooms visible from `from` besides itself (open doors …).
    fn opens_rooms(&self, from: RoomId, ctx: &Ctx) -> Vec<RoomId> {
        let _ = (from, ctx);
        Vec::new()
    }
    /// For `world.dump()["ext"][name]`; an object, documented per package.
    fn dump(&self) -> Value {
        Value::Null
    }
    fn shutdown(&mut self) {}
    /// A keymap sub-mode the router consults (`Route` resolution chain):
    /// the overlay's "map", the library's "library". The hub asks overlay,
    /// objects, cats, wm in that order; the first `Some` wins (S14).
    fn submode(&self) -> Option<&'static str> {
        None
    }
    /// What this plugin knows for the strip and the phone's header (S20).
    fn status(&self, ctx: &Ctx) -> Status {
        let _ = ctx;
        Status::default()
    }
    /// The apps this plugin offers on the phone (S21); ids stable, titles
    /// pt-BR.
    fn phone_apps(&self, ctx: &Ctx) -> Vec<crate::ui::phone::PhoneApp> {
        let _ = ctx;
        Vec::new()
    }
    /// The page of one of its apps for the phone's current state; `None` =
    /// not mine.
    fn phone_page(
        &self,
        app: &str,
        st: &crate::ui::phone::PhoneState,
        ctx: &Ctx,
    ) -> Option<crate::ui::phone::PhonePage> {
        let _ = (app, st, ctx);
        None
    }
}

/// The phone the overlay draws (S20): every plugin's apps sorted by
/// `order` (stable: plugin order breaks ties) and the page of `st.app` from
/// the first plugin that knows it.
pub fn phone_view(
    plugins: &[&dyn Plugin],
    st: crate::ui::phone::PhoneState,
    ctx: &Ctx,
) -> crate::ui::phone::PhoneView {
    let mut apps: Vec<_> = plugins.iter().flat_map(|p| p.phone_apps(ctx)).collect();
    apps.sort_by_key(|a| a.order);
    let page = st
        .app
        .as_deref()
        .and_then(|a| plugins.iter().find_map(|p| p.phone_page(a, &st, ctx)));
    crate::ui::phone::PhoneView {
        state: st,
        apps,
        page,
    }
}

pub struct Ext {
    pub router: crate::router::Router,
    pub wm: crate::wm::Wm,
    pub overlay: crate::overlay::Overlay,
    pub cats: crate::cats::Cats,
    pub objects: crate::objects::Objects,
}

impl Ext {
    pub fn new(cli: &crate::cli::Cli) -> Ext {
        // the WM parses world.toml first; its `[keys]` feed the router (the
        // CLI flags win)
        let wm = crate::wm::Wm::new(cli);
        let keys = &wm.config().keys;
        let router = crate::router::Router::new(
            cli.world_mod_or(keys.world_mod.as_deref()),
            &cli.focus_escape_or(keys.focus_escape.as_deref()),
        );
        Ext {
            router,
            wm,
            overlay: crate::overlay::Overlay::new(cli),
            cats: crate::cats::Cats::new(cli),
            objects: crate::objects::Objects::new(cli),
        }
    }

    /// tick/draw/… order: wm, objects, cats, overlay.
    pub fn plugins(&mut self) -> [&mut dyn Plugin; 4] {
        [
            &mut self.wm,
            &mut self.objects,
            &mut self.cats,
            &mut self.overlay,
        ]
    }

    /// The same, read-only (draw, wishes, badges …).
    pub fn plugins_ref(&self) -> [&dyn Plugin; 4] {
        [&self.wm, &self.objects, &self.cats, &self.overlay]
    }

    /// The keymap sub-mode for the router: overlay, objects, cats, wm; the
    /// first `Some` wins (S14).
    pub fn submode(&self) -> Option<&'static str> {
        self.overlay
            .submode()
            .or_else(|| Plugin::submode(&self.objects))
            .or_else(|| Plugin::submode(&self.cats))
            .or_else(|| Plugin::submode(&self.wm))
    }

    /// `world.dump()["ext"]`.
    pub fn dump(&self) -> Value {
        serde_json::json!({
            "wm": self.wm.dump(),
            "overlay": self.overlay.dump(),
            "cats": self.cats.dump(),
            "objects": self.objects.dump(),
            "router": self.router.dump(),
        })
    }
}

/// `keys.focus_escape` until the core's config says otherwise.
pub const FOCUS_ESCAPE: &str = "ctrl+]";

/// The plugins `inject` accepts.
pub const INJECTABLE: [&str; 4] = ["wm", "overlay", "cats", "objects"];

/// `inject <plugin> <json>`: hand `json` to the plugin of that name.
pub fn inject(
    plugins: &mut [&mut dyn Plugin],
    name: &str,
    json: &Value,
    ctx: &Ctx,
) -> Result<Vec<Req>, String> {
    plugins
        .iter_mut()
        .find(|p| p.name() == name)
        .map(|p| p.inject(json, ctx))
        .ok_or_else(|| format!("inject: no plugin {name:?}"))
}

/// A command by name to the plugins, in dispatch order (overlay, cats,
/// objects, wm); the first `Some` wins.
pub fn command(
    plugins: &mut [&mut dyn Plugin],
    name: &str,
    args: &BTreeMap<String, String>,
    ctx: &Ctx,
) -> Option<Vec<Req>> {
    for want in ["overlay", "cats", "objects", "wm"] {
        if let Some(p) = plugins.iter_mut().find(|p| p.name() == want)
            && let Some(r) = p.command(name, args, ctx)
        {
            return Some(r);
        }
    }
    None
}

/// Who an `Out::Effect` goes to (the S7 routing table).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EffectTo {
    Overlay,
    Cats,
    Objects,
    /// whatever no one else takes (the WM: books, …)
    Wm,
}

/// Route a passed-through effect; `purpose`: `World::purpose()` (the
/// CONVERSA or FOCO it came from).
pub fn route_effect(e: &Effect, purpose: TalkPurpose) -> EffectTo {
    let dialogue = match purpose {
        TalkPurpose::Session => EffectTo::Cats,
        TalkPurpose::VaultUnlock => EffectTo::Objects,
    };
    match e {
        Effect::ShowHints { .. }
        | Effect::HideHints
        | Effect::OpenPalette
        | Effect::ClosePalette
        | Effect::Badge => EffectTo::Overlay,
        Effect::FrameFace(_)
        | Effect::CatTurns(_)
        | Effect::CatBackToWork(_)
        | Effect::OpenPane(_)
        | Effect::OpenLine
        | Effect::CloseLine
        | Effect::SendLine
        | Effect::TalkNext
        | Effect::KeyboardEnter(nkw_core::Target::CatTerm(_))
        | Effect::KeyboardLeave => dialogue,
        // the library lives in the objects plugin (R6)
        Effect::OpenBook(_) => EffectTo::Objects,
        _ => EffectTo::Wm,
    }
}

/// The plugins' extra obstacles for `room`, summed.
pub fn obstacles(plugins: &[&dyn Plugin], room: RoomId, ctx: &Ctx) -> Vec<Aabb> {
    plugins
        .iter()
        .flat_map(|p| p.obstacles(room, ctx))
        .collect()
}

/// The rooms the plugins open from `from`.
pub fn opens_rooms(plugins: &[&dyn Plugin], from: RoomId, ctx: &Ctx) -> Vec<RoomId> {
    plugins
        .iter()
        .flat_map(|p| p.opens_rooms(from, ctx))
        .collect()
}

/// For `Plugin::draw` implementations: is this cat's room drawn this frame?
pub fn cat_visible(ctx: &Ctx, cat: CatId) -> bool {
    ctx.world
        .cat_desk(cat)
        .is_some_and(|(room, _)| ctx.visible_rooms.contains(&room))
}

/// The machine action that does `a` to an object (a keyed interaction,
/// through the mode machine: nothing under an overlay).
pub fn object_req(id: ObjectId, a: ObjectAction) -> Req {
    Req::Action(match a {
        ObjectAction::Interact => Action::Interact(id),
        ObjectAction::Adjust(s) => Action::Adjust(id, s),
        ObjectAction::Prev => Action::Prev(id),
        ObjectAction::Next => Action::Next(id),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use nkw_core::{BookId, Target};

    /// A plugin that records what reached it.
    #[derive(Default)]
    struct Fake {
        name: &'static str,
        injected: Vec<Value>,
        commands: Vec<String>,
    }

    impl Plugin for Fake {
        fn name(&self) -> &'static str {
            self.name
        }
        fn inject(&mut self, json: &Value, _ctx: &Ctx) -> Vec<Req> {
            self.injected.push(json.clone());
            vec![Req::Toast {
                text: "ok".into(),
                kind: ToastKind::Info,
                ms: 10,
            }]
        }
        fn command(
            &mut self,
            name: &str,
            _args: &BTreeMap<String, String>,
            _ctx: &Ctx,
        ) -> Option<Vec<Req>> {
            self.commands.push(name.into());
            (name == format!("{}.x", self.name)).then(Vec::new)
        }
    }

    fn with_ctx(f: impl FnOnce(&Ctx)) {
        let w = crate::render::tests::world(&[&[1]]);
        let pids = BTreeMap::new();
        let ctx = Ctx {
            world: &w,
            now: 0.0,
            frame: 0,
            reduced_motion: false,
            demo: true,
            scale: 1.0,
            size_px: [1280, 720],
            pids: &pids,
            visible_rooms: &[RoomId(0)],
            line: None,
        };
        f(&ctx);
    }

    #[test]
    fn inject_reaches_the_plugin_by_name() {
        with_ctx(|ctx| {
            let mut a = Fake {
                name: "cats",
                ..Fake::default()
            };
            let mut b = Fake {
                name: "objects",
                ..Fake::default()
            };
            let json = serde_json::json!({"postman": 1});
            {
                let mut ps: [&mut dyn Plugin; 2] = [&mut a, &mut b];
                let r = inject(&mut ps, "objects", &json, ctx).unwrap();
                assert_eq!(r.len(), 1);
                assert!(inject(&mut ps, "nobody", &json, ctx).is_err());
            }
            assert!(a.injected.is_empty());
            assert_eq!(b.injected, [json]);
        });
    }

    #[test]
    fn commands_go_overlay_cats_objects_wm_first_some_wins() {
        with_ctx(|ctx| {
            let mut fakes = ["wm", "objects", "cats", "overlay"].map(|name| Fake {
                name,
                ..Fake::default()
            });
            let args = BTreeMap::new();
            {
                let [a, b, c, d] = &mut fakes;
                let mut ps: [&mut dyn Plugin; 4] = [a, b, c, d];
                assert!(command(&mut ps, "objects.x", &args, ctx).is_some());
                assert!(command(&mut ps, "nobody.y", &args, ctx).is_none());
            }
            let seen = |n: usize| fakes[n].commands.clone();
            assert_eq!(seen(3), ["objects.x", "nobody.y"], "overlay asked first");
            assert_eq!(seen(2), ["objects.x", "nobody.y"]);
            assert_eq!(seen(1), ["objects.x", "nobody.y"]);
            assert_eq!(seen(0), ["nobody.y"], "wm never asked once objects took it");
        });
    }

    #[test]
    fn the_effect_routing_table() {
        use EffectTo as T;
        let s = TalkPurpose::Session;
        let v = TalkPurpose::VaultUnlock;
        let rows = [
            (Effect::ShowHints { focus: true }, s, T::Overlay),
            (Effect::HideHints, v, T::Overlay),
            (Effect::OpenPalette, s, T::Overlay),
            (Effect::ClosePalette, s, T::Overlay),
            (Effect::Badge, s, T::Overlay),
            (Effect::FrameFace(CatId(1)), s, T::Cats),
            (Effect::FrameFace(CatId(1)), v, T::Objects),
            (Effect::OpenLine, s, T::Cats),
            (Effect::SendLine, v, T::Objects),
            (Effect::TalkNext, v, T::Objects),
            (Effect::OpenPane(CatId(2)), s, T::Cats),
            (Effect::KeyboardEnter(Target::CatTerm(CatId(2))), s, T::Cats),
            (Effect::KeyboardLeave, s, T::Cats),
            (Effect::OpenBook(BookId(1)), s, T::Objects),
        ];
        for (e, p, want) in rows {
            assert_eq!(route_effect(&e, p), want, "{e:?} {p:?}");
        }
    }

    fn hud() -> HudInput {
        HudInput {
            mode_label: "NAVEGAR",
            badges: Vec::new(),
            dialog: None,
            focus_rect: None,
            tags: Vec::new(),
            pickables: Vec::new(),
            aimed: false,
            status: Status::default(),
            phone: None,
        }
    }

    #[test]
    fn status_merge_some_wins_in_plugin_order_bools_or() {
        let objects = Status {
            clock: Some((14, 5)),
            band: Some(4),
            battery: Some((42, false)),
            vault_open: false,
            ..Status::default()
        };
        let cats = Status {
            clock: Some((9, 0)), // a later plugin never overrides
            inbox: Some(2),
            agents: Some((3, 1)),
            cost_cents: Some(125),
            usage_5h_pct: Some(40),
            vault_open: true,
            ..Status::default()
        };
        let wm = Status {
            xwayland: Some(true),
            net: Some((true, 3)),
            playing: Some(false),
            ..Status::default()
        };
        let all = [objects.clone(), cats.clone(), wm.clone()]
            .into_iter()
            .fold(Status::default(), Status::merge);
        assert_eq!(all.clock, Some((14, 5)), "the first Some wins");
        assert_eq!(all.band, Some(4));
        assert_eq!(all.battery, Some((42, false)));
        assert_eq!(all.net, Some((true, 3)), "a None is filled later");
        assert_eq!(all.playing, Some(false), "Some(false) is still a Some");
        assert!(all.vault_open, "bools are or-ed");
        assert_eq!(all.inbox, Some(2));
        assert_eq!(all.agents, Some((3, 1)));
        assert_eq!(all.cost_cents, Some(125));
        assert_eq!(all.usage_5h_pct, Some(40));
        assert_eq!(all.xwayland, Some(true));
        // order matters for Somes only
        let rev = [wm, cats, objects]
            .into_iter()
            .fold(Status::default(), Status::merge);
        assert_eq!(rev.clock, Some((9, 0)));
        assert!(rev.vault_open);
        // the identity
        assert_eq!(Status::default().merge(all.clone()), all);
        assert_eq!(all.clone().merge(Status::default()), all);
        // serialized as is (dump()["status"])
        let v = serde_json::to_value(&all).unwrap();
        assert_eq!(v["clock"], serde_json::json!([14, 5]));
        assert_eq!(v["vault_open"], serde_json::json!(true));
    }

    fn dialog(text: &str, choices: &[&str]) -> DialogView {
        DialogView {
            name: "acme".into(),
            rgb: [1, 2, 3],
            text: text.into(),
            more: choices.is_empty(),
            choices: choices.iter().map(|c| c.to_string()).collect(),
            selected: 0,
            line: None,
        }
    }

    #[test]
    fn the_hud_hash_sees_what_the_overlay_draws() {
        let base = hud_hash(&hud());
        assert_eq!(base, hud_hash(&hud()), "same input, same hash");
        let with = |f: &dyn Fn(&mut HudInput)| {
            let mut h = hud();
            f(&mut h);
            hud_hash(&h)
        };
        // the wave-2 stale dialogue: the reveal finished and the choices came
        let partial = with(&|h| h.dialog = Some(dialog("Aplicar a migra", &[])));
        let full = with(&|h| h.dialog = Some(dialog("Aplicar a migração?", &["sim", "não"])));
        assert_ne!(partial, full);
        assert_eq!(
            full,
            with(&|h| h.dialog = Some(dialog("Aplicar a migração?", &["sim", "não"])))
        );
        let changes: [&dyn Fn(&mut HudInput); 11] = [
            &|h| h.phone = Some(phone(None, 0)),
            &|h| h.phone = Some(phone(None, 1)),
            &|h| h.status.clock = Some((14, 5)),
            &|h| h.status.vault_open = true,
            &|h| h.mode_label = "ANDAR",
            &|h| h.badges.push(Badge::Inbox(1)),
            &|h| h.focus_rect = Some([0, 0, 10, 10]),
            &|h| h.aimed = true,
            &|h| {
                h.tags.push(Tag {
                    id: 1,
                    pos: Vec3::ZERO,
                    text: "acme".into(),
                    rgb: [0; 3],
                    max_m: 4.0,
                })
            },
            &|h| h.dialog = Some(dialog("a", &["x"])),
            &|h| {
                let mut d = dialog("a", &["x"]);
                d.selected = 1;
                h.dialog = Some(d);
            },
        ];
        let mut seen = vec![base];
        for c in changes {
            let v = with(c);
            assert!(!seen.contains(&v), "every change is seen");
            seen.push(v);
        }
        let moved = |x: f32| {
            with(&|h| {
                h.tags.push(Tag {
                    id: 1,
                    pos: Vec3::new(x, 0.0, 0.0),
                    text: "acme".into(),
                    rgb: [0; 3],
                    max_m: 4.0,
                })
            })
        };
        assert_ne!(moved(0.0), moved(0.5), "a tag that moves");
        assert!(!hud().busy());
        let mut b = hud();
        b.badges.push(Badge::Confirm);
        assert!(b.busy());
        let mut p = hud();
        p.phone = Some(phone(Some("gatos"), 0));
        assert!(p.busy(), "the phone is open");
    }

    fn phone(app: Option<&str>, sel: usize) -> crate::ui::phone::PhoneView {
        crate::ui::phone::PhoneView {
            state: crate::ui::phone::PhoneState {
                app: app.map(String::from),
                sel,
                ..Default::default()
            },
            apps: Vec::new(),
            page: None,
        }
    }

    /// A plugin with phone apps.
    struct Apps(&'static str, Vec<(&'static str, u8)>);

    impl Plugin for Apps {
        fn name(&self) -> &'static str {
            self.0
        }
        fn phone_apps(&self, _ctx: &Ctx) -> Vec<crate::ui::phone::PhoneApp> {
            self.1
                .iter()
                .map(|(id, order)| crate::ui::phone::PhoneApp {
                    id,
                    title: id,
                    icon: crate::ui::Icon::Dot,
                    order: *order,
                    badge: None,
                })
                .collect()
        }
        fn phone_page(
            &self,
            app: &str,
            st: &crate::ui::phone::PhoneState,
            _ctx: &Ctx,
        ) -> Option<crate::ui::phone::PhonePage> {
            self.1
                .iter()
                .any(|(id, _)| *id == app)
                .then(|| crate::ui::phone::PhonePage {
                    title: format!("{}:{app}:{}", self.0, st.sel),
                    ..Default::default()
                })
        }
    }

    #[test]
    fn the_phone_view_gathers_apps_by_order_and_the_owners_page() {
        with_ctx(|ctx| {
            let wm = Apps("wm", vec![("apps", 4), ("janelas", 5)]);
            let cats = Apps("cats", vec![("gatos", 1), ("inbox", 2)]);
            let ov = Apps("overlay", vec![("ajustes", 9), ("mapa", 3)]);
            let ps: [&dyn Plugin; 3] = [&wm, &cats, &ov];
            let home = phone_view(&ps, Default::default(), ctx);
            let ids: Vec<_> = home.apps.iter().map(|a| a.id).collect();
            assert_eq!(
                ids,
                ["gatos", "inbox", "mapa", "apps", "janelas", "ajustes"]
            );
            assert!(home.page.is_none(), "the home grid has no page");
            let st = crate::ui::phone::PhoneState {
                app: Some("inbox".into()),
                sel: 2,
                ..Default::default()
            };
            let v = phone_view(&ps, st.clone(), ctx);
            assert_eq!(v.state, st);
            assert_eq!(v.page.unwrap().title, "cats:inbox:2");
            let st = crate::ui::phone::PhoneState {
                app: Some("nobody".into()),
                ..Default::default()
            };
            assert!(phone_view(&ps, st, ctx).page.is_none());
        });
    }

    #[test]
    fn the_default_hooks_say_nothing() {
        with_ctx(|ctx| {
            let f = Fake::default();
            assert_eq!(f.status(ctx), Status::default());
            assert!(f.phone_apps(ctx).is_empty());
            assert!(f.phone_page("gatos", &Default::default(), ctx).is_none());
        });
    }

    #[test]
    fn object_reqs_are_machine_actions() {
        let id = ObjectId(3);
        assert_eq!(
            object_req(id, ObjectAction::Adjust(-1)),
            Req::Action(Action::Adjust(id, -1))
        );
        assert_eq!(
            object_req(id, ObjectAction::Interact),
            Req::Action(Action::Interact(id))
        );
    }
}
