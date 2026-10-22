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
    /// without the testclient)
    Spawn(Vec<String>),
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

#[derive(Clone, Debug, PartialEq, Eq)]
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LineView {
    pub text: String,
    pub cursor: usize,
}

#[derive(Clone, Debug, PartialEq)]
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
}

/// One 2D element, logical px, top-left origin. The overlay owns the buffer
/// and its cache.
pub struct OverlayElem {
    pub id: u64,
    pub pos: (i32, i32),
    pub buf: MemoryRenderBuffer,
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
        Ext {
            router: crate::router::Router::new(cli.world_mod, FOCUS_ESCAPE),
            wm: crate::wm::Wm::new(cli),
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
            (Effect::OpenBook(BookId(1)), s, T::Wm),
        ];
        for (e, p, want) in rows {
            assert_eq!(route_effect(&e, p), want, "{e:?} {p:?}");
        }
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
