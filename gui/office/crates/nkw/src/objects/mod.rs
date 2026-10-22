//! The world's objects and its sound (addendum `docs/plano-mundo-objetos.md`):
//! `nkw-objects`' pure models fed by `nkw-sys`' adapters, wired into the hub
//! through the S7 seam.
//!
//! - the reception's furniture (`place`): turntable (MPRIS + the sink's
//!   volume), wall clocks (+ the day band), the battery's oil jar, the
//!   antenna plant …, positions from `layout`, stands to walk up to;
//! - the rooms' hall doors (`doors`): collision, culling, swing, auto-close,
//!   their sounds;
//! - proximity audio v1 (`audio`): PipeWire per-stream volumes anchored to
//!   the screens and the turntable;
//! - our own sounds (`sound`) on a PipeWire stream; footsteps in ANDAR;
//! - HUD badges, readable-tier readouts, the palette's `media.*`/`volume.*`/
//!   `world.goto_object`.
//!
//! `inject objects <json>` takes a `nkw_sys::SysEvent` as JSON
//! (`{"Power":{"present":true,"pct":12,"charging":false,"time_to_empty_s":1800}}`,
//! `{"Media":{…}}`, `"MediaGone"`, `{"Clock":{…}}`, `{"Net":{…}}`,
//! `{"Pressure":{"cpu":true,"mem":false}}`, `{"Audio":{"Added":{…}}}`), or
//! `{"Door":{"id":0,"toggle":true}}`, `{"Command":{"name":"world.interact","args":{}}}`,
//! `{"Cue":"Meow"}`, `{"Shot":true}`.
//!
//! `dump` (`/ext/objects`): `{"objects":[{id,kind,room,tier,data_kind}],
//! "doors":[{id,rooms,openness,passable}], "audio":{streams,anchored,gains,calls},
//! "badges":[…], "rate_wishes":n, "visible_rooms":[…], "sound":{played,active,last},
//! "sys":{"mpris":"ok|error|off",…}}`. No titles, no SSIDs, no device names.

pub mod audio;
pub mod doors;
pub mod draw;
pub mod map;
pub mod place;
pub mod sound;
pub mod vault;

use std::collections::BTreeMap;
use std::sync::Arc;

use glam::{Vec2, Vec3};
use nkw_core::layout::{self, FOV_Y};
use nkw_core::strip::{RoomSlot, Side};
use nkw_core::{
    Aabb, DoorId, ObjectAction, ObjectId, Pick, Pose, RoomId, ScreenId, Time, Want, Wish,
};
use nkw_gl::draw::{DrawList, Instance, MeshId};
use nkw_objects::acoustic::{Listener, RoomGraph};
use nkw_objects::object::{self, Data, Effect as ObjEffect, Kind, MediaCmd};
use nkw_objects::proximity::{self, Radii, Tier};
use nkw_objects::sound::Cue;
use nkw_sys::{MediaControl, Playback, StreamVolume, SysEvent};
use serde::Deserialize;
use serde_json::Value;

use crate::ext::{
    self, Badge, Ctx, DialogView, PaletteItem, Pickable, Plugin, Req, Tag, ToastKind,
};
use audio::{Anchor, Audio, Known};
use doors::{Doors, HALL};
use draw::AnimState;
use map::Need;
use place::Placement;
use sound::Sound;

/// What the objects need from the system: `nkw_sys::Sys`, or a fake in
/// tests (nothing here ever reaches the user's real players or volumes in
/// a test or a `--demo` run).
pub trait Port {
    /// Everything that arrived, without blocking.
    fn events(&self) -> Vec<SysEvent>;
    fn media(&self) -> &dyn MediaControl;
    fn audio(&self) -> &dyn StreamVolume;
    fn audio_handle(&self) -> Option<Arc<dyn StreamVolume>> {
        None
    }
    fn playback(&self) -> Option<Arc<dyn Playback>> {
        None
    }
    fn shutdown(self: Box<Self>) {}
}

impl Port for nkw_sys::Sys {
    fn events(&self) -> Vec<SysEvent> {
        nkw_sys::Sys::events(self).try_iter().collect()
    }
    fn media(&self) -> &dyn MediaControl {
        nkw_sys::Sys::media(self)
    }
    fn audio(&self) -> &dyn StreamVolume {
        nkw_sys::Sys::audio(self)
    }
    fn audio_handle(&self) -> Option<Arc<dyn StreamVolume>> {
        Some(nkw_sys::Sys::audio_handle(self))
    }
    fn playback(&self) -> Option<Arc<dyn Playback>> {
        self.playback_handle()
    }
    fn shutdown(self: Box<Self>) {
        nkw_sys::Sys::shutdown(*self)
    }
}

/// The shapes `inject objects` takes besides a `SysEvent` (`Shot`: lock the
/// vault before a screenshot).
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
enum Inject {
    Door {
        id: u32,
        #[serde(default)]
        toggle: bool,
    },
    Command {
        name: String,
        #[serde(default)]
        args: BTreeMap<String, String>,
    },
    Cue(Cue),
    Shot(bool),
    Media(nkw_sys::MediaState),
    MediaGone,
    Power(nkw_sys::PowerState),
    Net(nkw_sys::NetState),
    Clock(nkw_sys::ClockTick),
    Pressure {
        cpu: bool,
        mem: bool,
    },
    Audio(nkw_sys::AudioEvent),
}

/// The world volume step of `volume.up/down` without a turntable.
const VOLUME_STEP: f32 = object::STEP;
/// How far in the view a thing may be off the frame's edge and still count
/// as in view, degrees.
const VIEW_SLACK_DEG: f32 = 15.0;
/// Readouts show within this, metres (the overlay hides them beyond).
const TAG_M: f32 = 2.2;

pub struct Objects {
    demo: bool,
    sys_on: bool,
    pub world: object::World,
    placement: Placement,
    pub doors: Doors,
    pub audio: Audio,
    pub sound: Sound,
    anims: BTreeMap<ObjectId, AnimState>,
    port: Option<Box<dyn Port>>,
    /// `--demo`: the fixture's events, applied on the first drain
    pending: Vec<SysEvent>,
    radii: Radii,
    sink_volume: Option<f32>,
    player: Option<String>,
    /// `volume.mute`: the volume before it
    muted: Option<f32>,
    /// failed adapters (names only; the messages go to the log)
    errors: BTreeMap<String, String>,
    /// the windows the audio anchors were resolved against
    screens_key: Vec<(ScreenId, u32, String)>,
    // computed by `tick`, read by the `&self` hooks
    tiers: BTreeMap<ObjectId, Tier>,
    needs: Vec<Need>,
    tags: Vec<Tag>,
    visible: Vec<RoomId>,
    now: Time,
    /// the World's day band as of the last tick
    band: u8,
    pub vault: vault::VaultGlue,
}

impl Objects {
    pub fn new(cli: &crate::cli::Cli) -> Objects {
        Objects::with(cli.demo, cli.sys(), None)
    }

    /// `demo`: the fixture world; `sys_on`: the adapters will run; `port`:
    /// a system port already started (tests pass a fake).
    pub fn with(demo: bool, sys_on: bool, port: Option<Box<dyn Port>>) -> Objects {
        let (world, reception) = if demo {
            let mut w = object::demo_world();
            // the demo starts calm: its updates were looked at already (a
            // badge only comes from an event, so the shots stay clean)
            let _ = w.apply(place::kind_id(Kind::PackageBox), object::Action::Open, 0.0);
            let kinds = w.objects.iter().map(|o| o.kind).collect();
            (w, kinds)
        } else {
            (object::World::default(), place::DEFAULT_RECEPTION.to_vec())
        };
        let pending = if demo {
            // the clock is the fixture world's 14:05: wall time would make
            // every shot depend on the hour (the band only moves on inject)
            nkw_sys::demo::fixture()
                .into_iter()
                .filter(|e| !matches!(e, SysEvent::Clock(_)))
                .collect()
        } else {
            Vec::new()
        };
        let mut o = Objects {
            demo,
            sys_on,
            world,
            placement: Placement::new(reception),
            doors: Doors::default(),
            audio: Audio::default(),
            sound: Sound::new(),
            anims: BTreeMap::new(),
            port: None,
            pending,
            radii: Radii::default(),
            sink_volume: None,
            player: None,
            muted: None,
            errors: BTreeMap::new(),
            screens_key: Vec::new(),
            tiers: BTreeMap::new(),
            needs: Vec::new(),
            tags: Vec::new(),
            visible: Vec::new(),
            now: 0.0,
            band: crate::world::DEFAULT_BAND,
            vault: vault::VaultGlue::default(),
        };
        if let Some(p) = port {
            o.attach(p);
        }
        o
    }

    fn attach(&mut self, p: Box<dyn Port>) {
        if let Some(pb) = p.playback() {
            self.sound.attach(pb);
        }
        if let Some(h) = p.audio_handle() {
            audio::install_panic_restore(self.audio.touched.clone(), h);
        }
        self.port = Some(p);
    }

    /// `Out::Object`: do `a` to the object.
    pub fn on_object(&mut self, id: ObjectId, a: ObjectAction, ctx: &Ctx) -> Vec<Req> {
        self.sync(ctx);
        let Some(o) = self.world.object(id).cloned() else {
            return Vec::new();
        };
        if let Kind::Door(d) = o.kind {
            return self.use_door(d, &o, a, ctx);
        }
        let act = match a {
            ObjectAction::Interact => object::Action::Interact,
            ObjectAction::Adjust(s) => object::Action::Adjust(s),
            ObjectAction::Prev => object::Action::Prev,
            ObjectAction::Next => object::Action::Next,
        };
        let fx = self.world.apply(id, act, ctx.now);
        let mut out = self.effects(id, fx, ctx);
        out.push(redraw());
        out
    }

    /// `interact` on a door: walking, it opens or shuts (§4.4); from a NAV
    /// stand, an open door is walked through, a shut one opens.
    fn use_door(&mut self, d: DoorId, o: &object::Object, a: ObjectAction, ctx: &Ctx) -> Vec<Req> {
        if a != ObjectAction::Interact {
            return Vec::new();
        }
        let passable = self
            .doors
            .get(d)
            .is_some_and(|x| x.passable(ctx.now, &self.doors.p));
        if passable && ctx.world.walking.is_none() {
            // through to the hall, facing along it
            let beyond = o.pos + layout::facing(o.yaw) * 1.6;
            let p = Pose::new(Vec3::new(beyond.x, layout::EYE, beyond.z), o.yaw, 0.0);
            return vec![Req::GlideTo(p)];
        }
        let mut out = self.toggle_door(d, ctx);
        out.push(redraw());
        out
    }

    fn toggle_door(&mut self, d: DoorId, ctx: &Ctx) -> Vec<Req> {
        let fx = self.doors.toggle(d, ctx.now, ctx.reduced_motion);
        self.door_effects(d, fx, ctx);
        self.refresh_visible(ctx);
        Vec::new()
    }

    /// The culling set as the doors see it now (for the dump; the hub asks
    /// `opens_rooms` itself each frame).
    fn refresh_visible(&mut self, ctx: &Ctx) {
        let c = ctx.world.camera.pos;
        let cur = ctx.world.current_room_id();
        let opens = self
            .doors
            .opens_rooms(cur, doors::in_hall(Vec2::new(c.x, c.z)), ctx.now);
        self.visible = ctx.world.visible_rooms(&opens);
    }

    fn door_effects(&mut self, d: DoorId, fx: Vec<nkw_objects::door::Effect>, ctx: &Ctx) {
        let at = self.doors.get(d).map(|x| x.pos);
        for e in fx {
            if let nkw_objects::door::Effect::Cue(c) = e {
                let room = RoomId(d.0);
                self.play(c, at.map(|p| (p, room)), ctx);
            }
        }
    }

    /// CONVERSA with purpose `VaultUnlock`: the receptionist asks for the
    /// phrase.
    pub fn dialog(&self) -> Option<DialogView> {
        self.vault.dialog()
    }

    fn vault_cues(&mut self, cues: Vec<Cue>, ctx: &Ctx) {
        let at = self
            .world
            .find(Kind::Vault)
            .map(|o| (Vec2::new(o.pos.x, o.pos.z), o.room));
        for c in cues {
            self.play(c, at, ctx);
        }
    }

    /// `Req::Cue`: one of our own sounds, at `at` (`None`: the listener).
    pub fn cue(&mut self, cue: Cue, at: Option<Vec3>, ctx: &Ctx) {
        let at = at.map(|p| {
            let f = Vec2::new(p.x, p.z);
            (f, self.room_at(ctx, f))
        });
        self.play(cue, at, ctx);
    }

    fn play(&mut self, cue: Cue, at: Option<(Vec2, RoomId)>, ctx: &Ctx) {
        let l = self.listener(ctx);
        let g = self.graph(ctx);
        let p = self.audio.params;
        self.sound.play(cue, at, &l, &g, &p);
    }

    fn rooms(ctx: &Ctx) -> Vec<(RoomId, RoomSlot)> {
        ctx.world
            .rooms
            .iter()
            .map(|r| r.id)
            .zip(ctx.world.slots())
            .collect()
    }

    /// The room a floor point is in (the hall when between the rows).
    fn room_at(&self, ctx: &Ctx, p: Vec2) -> RoomId {
        if doors::in_hall(p) {
            return HALL;
        }
        for (r, s) in Self::rooms(ctx) {
            let inside_x = p.x >= s.x0 && p.x <= s.x0 + s.len;
            let inside_z = match s.side {
                Side::North => p.y <= 0.0,
                Side::South => p.y >= 0.0,
            };
            if inside_x && inside_z {
                return r;
            }
        }
        ctx.world.current_room_id()
    }

    fn listener(&self, ctx: &Ctx) -> Listener {
        let c = ctx.world.camera;
        let pos = Vec2::new(c.pos.x, c.pos.z);
        Listener {
            pos,
            yaw: c.yaw,
            room: self.room_at(ctx, pos),
        }
    }

    fn graph(&self, ctx: &Ctx) -> RoomGraph {
        let mut ids: Vec<RoomId> = ctx.world.rooms.iter().map(|r| r.id).collect();
        ids.push(HALL);
        RoomGraph::new(&ids, &self.doors.portals(ctx.now))
    }

    /// Keep the objects and doors in step with the rooms.
    fn sync(&mut self, ctx: &Ctx) {
        let rooms = Self::rooms(ctx);
        if self.placement.sync(&rooms, &mut self.world.objects) {
            // new clocks show the time the others show
            let clock = self
                .world
                .data
                .values()
                .find(|d| matches!(d, Data::Clock(_)))
                .cloned();
            let ids: Vec<ObjectId> = self.world.objects.iter().map(|o| o.id).collect();
            self.world.data.retain(|id, _| ids.contains(id));
            if let Some(c) = clock {
                let clocks: Vec<ObjectId> = self
                    .world
                    .objects
                    .iter()
                    .filter(|o| o.kind == Kind::WallClock)
                    .map(|o| o.id)
                    .collect();
                for id in clocks {
                    self.world.data.entry(id).or_insert_with(|| c.clone());
                }
            }
        }
        self.doors.sync(&rooms);
    }

    fn ids_of(&self, kind: Kind) -> Vec<ObjectId> {
        self.world
            .objects
            .iter()
            .filter(|o| o.kind == kind)
            .map(|o| o.id)
            .collect()
    }

    /// The object effects as requests and local state.
    fn effects(&mut self, id: ObjectId, fx: Vec<ObjEffect>, ctx: &Ctx) -> Vec<Req> {
        let mut out = Vec::new();
        for e in fx {
            match e {
                ObjEffect::Media(cmd) => {
                    if let Some(p) = &self.port {
                        match cmd {
                            MediaCmd::PlayPause => p.media().play_pause(),
                            MediaCmd::Next => p.media().next(),
                            MediaCmd::Prev => p.media().previous(),
                        }
                    }
                }
                ObjEffect::SetVolume(v) => self.set_sink(v),
                ObjEffect::SetBrightness(v) => tracing::debug!(v, "brightness (phase 6)"),
                ObjEffect::OpenList(_) => out.push(Req::Toast {
                    text: "em breve (fase 6)".into(),
                    kind: ToastKind::Info,
                    ms: 1500,
                }),
                ObjEffect::ToggleDoor(d) => out.extend(self.toggle_door(d, ctx)),
                ObjEffect::VaultPrompt => {
                    let cat = ctx
                        .world
                        .rooms
                        .first()
                        .and_then(|r| r.desks.first())
                        .and_then(|d| d.cat);
                    out.extend(self.vault.prompt(cat));
                }
                ObjEffect::Cue(c) => {
                    let at = self
                        .world
                        .object(id)
                        .map(|o| (Vec2::new(o.pos.x, o.pos.z), o.room));
                    self.play(c, at, ctx);
                }
                ObjEffect::Anim(oid, a) => self.anims.entry(oid).or_default().apply(a, ctx.now),
            }
        }
        out
    }

    fn set_sink(&mut self, v: f32) {
        let v = v.clamp(0.0, 1.0);
        self.sink_volume = Some(v);
        if let Some(p) = &self.port {
            p.audio().set_default_sink_volume(v);
        }
    }

    fn turntable(&self) -> Option<ObjectId> {
        self.world.find(Kind::Turntable).map(|o| o.id)
    }

    fn has_media(&self) -> bool {
        self.turntable()
            .is_some_and(|id| matches!(self.world.data.get(&id), Some(Data::Media(_))))
    }

    /// A system event (from the adapters or `inject`).
    fn on_sys(&mut self, ev: SysEvent, ctx: &Ctx) -> Vec<Req> {
        let mut out = Vec::new();
        for (kind, data) in map::feeds(&ev, self.sink_volume) {
            for id in self.ids_of(kind) {
                let fx = self.world.set(id, data.clone());
                out.extend(self.effects(id, fx, ctx));
            }
        }
        match ev {
            SysEvent::Media(m) => {
                if self.player.as_deref() != Some(m.player.as_str()) {
                    self.player = Some(m.player);
                    self.reanchor(ctx, true);
                }
            }
            SysEvent::MediaGone => {
                if let Some(tt) = self.turntable() {
                    self.world.data.remove(&tt);
                    let a = self.anims.entry(tt).or_default();
                    a.apply(object::Anim::NeedleLift, ctx.now);
                    a.apply(object::Anim::Spin(false), ctx.now);
                }
                self.player = None;
                self.reanchor(ctx, true);
            }
            SysEvent::Clock(c) => {
                if c.band.min(8) != ctx.world.band {
                    out.push(Req::Command {
                        name: "world.band".into(),
                        args: BTreeMap::from([("band".into(), c.band.min(8).to_string())]),
                    });
                }
            }
            SysEvent::Audio(a) => match a {
                nkw_sys::AudioEvent::DefaultSinkVolume(v) => {
                    self.sink_volume = Some(v.clamp(0.0, 1.0));
                    if let Some(tt) = self.turntable()
                        && let Some(Data::Media(m)) = self.world.data.get_mut(&tt)
                    {
                        m.volume = v.clamp(0.0, 1.0);
                    }
                }
                other => {
                    self.audio.on_event(&other);
                    self.reanchor(ctx, true);
                }
            },
            SysEvent::Error { source, msg } => {
                tracing::warn!(source, %msg, "nkw-sys adapter failed");
                self.errors.insert(source.to_string(), String::new());
            }
            _ => {}
        }
        out.push(redraw());
        out
    }

    /// Resolve the streams' anchors again when the windows changed (or
    /// `force`: a stream or the player changed).
    fn reanchor(&mut self, ctx: &Ctx, force: bool) {
        if self.audio.streams.is_empty() {
            return;
        }
        let key: Vec<(ScreenId, u32, String)> = ctx
            .world
            .screens
            .values()
            .map(|s| {
                (
                    s.id,
                    ctx.pids.get(&s.id).copied().unwrap_or(0),
                    s.app_id.clone(),
                )
            })
            .collect();
        if !force && key == self.screens_key {
            return;
        }
        self.screens_key = key;
        let app_ids: Vec<(ScreenId, String)> = ctx
            .world
            .screens
            .values()
            .map(|s| (s.id, s.app_id.clone()))
            .collect();
        let tt = self.turntable();
        let k = Known {
            pids: ctx.pids,
            app_ids: &app_ids,
            player: self.player.as_deref().zip(tt),
        };
        let Some(p) = &self.port else { return };
        self.audio.reanchor(&k, &nkw_sys::pidmap::ProcFs, p.audio());
    }

    /// Where an anchor sounds from: a screen's centre or an object's place.
    fn anchor_at(&self, ctx: &Ctx, a: Anchor) -> Option<(Vec2, RoomId)> {
        match a {
            Anchor::Screen(s) => {
                let (r, d, k) = ctx.world.locate(s)?;
                let c = ctx.world.screen_place(r, d, k).center;
                Some((Vec2::new(c.x, c.z), ctx.world.rooms[r].id))
            }
            Anchor::Object(id) => self
                .world
                .object(id)
                .map(|o| (Vec2::new(o.pos.x, o.pos.z), o.room)),
            Anchor::None => None,
        }
    }

    /// Whether `p` is within the camera's view (horizontally, with slack).
    fn in_view(ctx: &Ctx, p: Vec3) -> bool {
        let cam = ctx.world.camera;
        let v = Vec2::new(p.x - cam.pos.x, p.z - cam.pos.z);
        if v.length_squared() < 0.25 {
            return true;
        }
        let aspect = ctx.size_px[0] as f32 / ctx.size_px[1].max(1) as f32;
        let half = ((FOV_Y / 2.0).tan() * aspect).atan().to_degrees();
        cam.heading().angle_to(v).abs().to_degrees() <= half + VIEW_SLACK_DEG
    }

    /// The object the crosshair is on: the proximity rule, then a door whose
    /// gap the view ray crosses within reach.
    pub fn aimed(&self, ctx: &Ctx) -> Option<ObjectId> {
        let cam = ctx.world.camera;
        let near: Vec<object::Object> = self
            .world
            .objects
            .iter()
            .filter(|o| {
                self.visible.is_empty()
                    || self.visible.contains(&o.room)
                    || matches!(o.kind, Kind::Door(_))
            })
            .cloned()
            .collect();
        if let Some(o) = proximity::aim(&self.radii, &cam, &near) {
            return Some(o.id);
        }
        let from = Vec2::new(cam.pos.x, cam.pos.z);
        let dir = cam.heading();
        let mut best: Option<(f32, ObjectId)> = None;
        for o in &near {
            let Kind::Door(d) = o.kind else { continue };
            let Some(door) = self.doors.get(d) else {
                continue;
            };
            // the gap runs along x (hall walls); the ray meets z = door.z
            if dir.y.abs() < 1e-4 {
                continue;
            }
            let t = (door.pos.y - from.y) / dir.y;
            if t <= 0.0 || t > self.radii.interact {
                continue;
            }
            let x = from.x + dir.x * t;
            if (x - door.pos.x).abs() <= door.geom.width / 2.0 && best.is_none_or(|b| t < b.0) {
                best = Some((t, o.id));
            }
        }
        best.map(|b| b.1)
    }

    fn goto(&self, name: &str, ctx: &Ctx) -> Option<Pose> {
        let here = ctx.world.current_room_id();
        let mut hits: Vec<&object::Object> = self
            .world
            .objects
            .iter()
            .filter(|o| map::matches_name(o.kind, name))
            .collect();
        hits.sort_by_key(|o| (o.room != here, o.id));
        hits.first().map(|o| o.stand)
    }

    fn media_cmd(&mut self, cmd: MediaCmd, ctx: &Ctx) -> Vec<Req> {
        match self.turntable() {
            Some(tt) if self.has_media() => {
                let a = match cmd {
                    MediaCmd::PlayPause => object::Action::Interact,
                    MediaCmd::Next => object::Action::Next,
                    MediaCmd::Prev => object::Action::Prev,
                };
                let fx = self.world.apply(tt, a, ctx.now);
                self.effects(tt, fx, ctx)
            }
            _ => self.effects(ObjectId(0), vec![ObjEffect::Media(cmd)], ctx),
        }
    }

    fn volume(&mut self, step: i8, ctx: &Ctx) -> Vec<Req> {
        self.muted = None;
        match self.turntable() {
            Some(tt) if self.has_media() => {
                let fx = self.world.apply(tt, object::Action::Adjust(step), ctx.now);
                self.effects(tt, fx, ctx)
            }
            _ => {
                let v = self.sink_volume.unwrap_or(0.5) + VOLUME_STEP * step as f32;
                self.set_sink(v);
                Vec::new()
            }
        }
    }

    /// `volume.mute`: the sink to 0 and the needle up; again, back.
    fn mute(&mut self, ctx: &Ctx) {
        let tt = self.turntable();
        match self.muted.take() {
            Some(before) => {
                self.set_sink(before);
                let playing = tt.is_some_and(
                    |id| matches!(self.world.data.get(&id), Some(Data::Media(m)) if m.playing),
                );
                if let Some(id) = tt
                    && playing
                {
                    self.anims
                        .entry(id)
                        .or_default()
                        .apply(object::Anim::NeedleDrop, ctx.now);
                }
            }
            None => {
                self.muted = Some(self.sink_volume.unwrap_or(1.0));
                self.set_sink(0.0);
                if let Some(id) = tt {
                    self.anims
                        .entry(id)
                        .or_default()
                        .apply(object::Anim::NeedleLift, ctx.now);
                }
            }
        }
    }

    fn vault_unlocked(&self) -> bool {
        self.vault.unlocked()
    }

    fn data_kind(d: Option<&Data>) -> Value {
        match d {
            None => Value::Null,
            Some(d) => {
                let s = match d {
                    Data::Media(_) => "Media",
                    Data::Power(_) => "Power",
                    Data::Net(_) => "Net",
                    Data::Load(_) => "Load",
                    Data::Updates(_) => "Updates",
                    Data::Clock(_) => "Clock",
                    Data::Tray(_) => "Tray",
                    Data::Bt(_) => "Bt",
                    Data::Brightness(_) => "Brightness",
                    Data::OnAir(_) => "OnAir",
                    Data::Layout(_) => "Layout",
                };
                s.into()
            }
        }
    }

    fn kind_name(k: Kind) -> &'static str {
        match k {
            Kind::Turntable => "Turntable",
            Kind::WallClock => "WallClock",
            Kind::Battery => "Battery",
            Kind::NetPlant => "NetPlant",
            Kind::Fireplace => "Fireplace",
            Kind::Aquarium => "Aquarium",
            Kind::PackageBox => "PackageBox",
            Kind::TrayShelf => "TrayShelf",
            Kind::OnAir => "OnAir",
            Kind::KeyboardFlag => "KeyboardFlag",
            Kind::Calendar => "Calendar",
            Kind::BtRadio => "BtRadio",
            Kind::Lamp => "Lamp",
            Kind::Vault => "Vault",
            Kind::Door(_) => "Door",
        }
    }

    fn badges_now(&self) -> Vec<nkw_objects::hud::Badge> {
        nkw_objects::hud::badges(&self.world, self.vault_unlocked())
    }
}

fn redraw() -> Req {
    Req::Wish(Wish::world(Want::Once))
}

impl Plugin for Objects {
    fn name(&self) -> &'static str {
        "objects"
    }

    fn start(
        &mut self,
        cli: &crate::cli::Cli,
        lh: &calloop::LoopHandle<'static, crate::state::Nkw>,
        ping: calloop::ping::Ping,
    ) {
        let _ = lh;
        if !cli.sys() || self.port.is_some() {
            return;
        }
        let wake: nkw_sys::Wake = Arc::new(move || ping.ping());
        let sys = nkw_sys::start(nkw_sys::SysOpts {
            wake: Some(wake),
            ..nkw_sys::SysOpts::default()
        });
        self.attach(Box::new(sys));
    }

    fn drain(&mut self, ctx: &Ctx) -> Vec<Req> {
        self.sync(ctx);
        let mut evs = std::mem::take(&mut self.pending);
        if let Some(p) = &self.port {
            evs.extend(p.events());
        }
        let mut out = Vec::new();
        for ev in evs {
            out.extend(self.on_sys(ev, ctx));
        }
        out
    }

    fn tick(&mut self, ctx: &Ctx) -> Vec<Req> {
        let now = ctx.now;
        self.now = now;
        self.band = ctx.world.band;
        self.sync(ctx);
        self.sound.drain(now);
        let mut out = Vec::new();
        let cam = ctx.world.camera;
        let me = Vec2::new(cam.pos.x, cam.pos.z);

        // doors: swings end, auto-closes start; their sounds
        let fx = self.doors.tick(now, me, ctx.reduced_motion);
        if !fx.is_empty() {
            out.push(redraw());
        }
        for (d, e) in fx {
            self.door_effects(d, vec![e], ctx);
        }

        // the vault: our CONVERSA ends, the auto-locks
        let talking = ctx.world.mode().purpose() == Some(nkw_core::TalkPurpose::VaultUnlock);
        let reception = ctx.world.rooms.first().map(|r| r.id);
        let here = self.room_at(ctx, me);
        let cues = self.vault.tick(now, talking, reception == Some(here));
        if !cues.is_empty() {
            out.push(redraw());
        }
        self.vault_cues(cues, ctx);

        // footsteps in ANDAR
        let walking = ctx.world.walking.as_ref().map(|_| me);
        let steps = self.sound.steps.walk(walking);
        if steps > 0 {
            let room = ctx
                .world
                .rooms
                .iter()
                .position(|r| r.id == self.room_at(ctx, me));
            let floor = sound::floor_of(room);
            for _ in 0..steps {
                self.play(Cue::Footstep(floor), None, ctx);
            }
        }

        // the windows changed: the streams' anchors too
        self.reanchor(ctx, false);

        // the culling set as the doors see it (for the dump)
        self.refresh_visible(ctx);

        // proximity: tiers, what animates, the readouts
        self.tiers.clear();
        let mut needs = Vec::new();
        let mut tags = Vec::new();
        for o in &self.world.objects {
            let shown =
                self.visible.contains(&o.room) && !ctx.world.frozen && Self::in_view(ctx, o.pos);
            let tier = if shown {
                proximity::tier(&self.radii, &cam, o)
            } else {
                Tier::Far
            };
            self.tiers.insert(o.id, tier);
            let dist = Vec2::new(o.pos.x, o.pos.z).distance(me);
            if let Some(w) = self.world.want(o.id, tier, ctx.reduced_motion) {
                needs.push(Need {
                    id: o.id,
                    want: w,
                    dist,
                });
            }
            if shown
                && !ctx.reduced_motion
                && self.anims.get(&o.id).is_some_and(|a| a.jiggling(now))
            {
                needs.push(Need {
                    id: o.id,
                    want: Want::Continuous,
                    dist,
                });
            }
            if tier >= Tier::Readable
                && let Some(text) = map::readout(o.kind, self.world.data.get(&o.id))
            {
                tags.push(Tag {
                    id: 0x0b1e_0000 + o.id.0 as u64,
                    pos: o.pos + Vec3::Y * 0.35,
                    text,
                    rgb: [0xf2, 0xe8, 0xd5],
                    max_m: TAG_M,
                });
            }
        }
        if let Some(t) = self.vault.next_wake(now) {
            needs.push(Need {
                id: ObjectId(u32::MAX - 2),
                want: Want::At(t.max(now)),
                dist: 0.0,
            });
        }
        if !ctx.world.frozen {
            for w in self.doors.wants(now, ctx.reduced_motion) {
                needs.push(Need {
                    id: ObjectId(u32::MAX),
                    want: w,
                    dist: 0.0,
                });
            }
        }

        // proximity audio: a step when something moved, ≤ 20 Hz
        if self.audio.anchored() > 0 && self.port.is_some() {
            let l = self.listener(ctx);
            let g = self.graph(ctx);
            let open: Vec<f32> = self.doors.portals(now).iter().map(|p| p.openness).collect();
            let places: Vec<(Anchor, Option<(Vec2, RoomId)>)> = self
                .audio
                .streams
                .values()
                .map(|s| (s.anchor, self.anchor_at(ctx, s.anchor)))
                .collect();
            let at = |a: Anchor| places.iter().find(|(x, _)| *x == a).and_then(|(_, p)| *p);
            if self.audio.due(&l, &open, &at)
                && let Some(p) = &self.port
            {
                self.audio.step(now, &l, &g, &open, &at, p.audio());
                if self.audio.due(&l, &open, &at) {
                    needs.push(Need {
                        id: ObjectId(u32::MAX - 1),
                        want: Want::Rate(audio::MAX_HZ as f32),
                        // the audio wins the cap
                        dist: -1.0,
                    });
                }
            }
        }
        self.needs = map::cap_rates(needs, map::MAX_RATES);
        self.tags = tags;
        out
    }

    fn wishes(&self, ctx: &Ctx) -> Vec<Wish> {
        let _ = ctx;
        self.needs
            .iter()
            .map(|n| match n.want {
                // the doors' timers and the auto-closes only need a tick
                Want::At(t) => Wish::compose(Want::At(t)),
                w => Wish::world(w),
            })
            .collect()
    }

    fn draw(&self, ctx: &Ctx, list: &mut DrawList) {
        let cam = list.camera.pose.pos;
        for o in &self.world.objects {
            if !ctx.visible_rooms.contains(&o.room) {
                continue;
            }
            let c = draw::Ctx {
                cam,
                now: ctx.now,
                reduced_motion: ctx.reduced_motion,
                data: self.world.data.get(&o.id),
                anim: self.anims.get(&o.id).copied().unwrap_or_default(),
            };
            draw::object(o, &c, &mut list.instances);
        }
        const FRAME: [u8; 4] = [0x8a, 0x6a, 0x50, 0xff];
        const LEAF: [u8; 4] = [0xc0, 0x90, 0x60, 0xff];
        for (_, frame, leaf) in self.doors.models(ctx.now) {
            for (mesh, m, tint) in [
                (MeshId::DoorFrame, frame, FRAME),
                (MeshId::DoorLeaf, leaf, LEAF),
            ] {
                let centre = m.transform_point3(Vec3::new(0.0, 1.0, 0.0));
                list.instances.push(Instance {
                    mesh,
                    model: m,
                    tint,
                    emissive: 0.0,
                    fade: layout::fade_for(cam, centre),
                });
            }
        }
    }

    fn command(
        &mut self,
        name: &str,
        args: &BTreeMap<String, String>,
        ctx: &Ctx,
    ) -> Option<Vec<Req>> {
        self.sync(ctx);
        let aim_req =
            |me: &Self, a: ObjectAction| me.aimed(ctx).map(|id| vec![ext::object_req(id, a)]);
        Some(match name {
            "media.play_pause" => self.media_cmd(MediaCmd::PlayPause, ctx),
            "media.next" => self.media_cmd(MediaCmd::Next, ctx),
            "media.prev" => self.media_cmd(MediaCmd::Prev, ctx),
            "volume.up" => self.volume(1, ctx),
            "volume.down" => self.volume(-1, ctx),
            "volume.mute" => {
                self.mute(ctx);
                vec![redraw()]
            }
            "world.goto_object" => {
                let n = args.get("name").map(String::as_str).unwrap_or("");
                match self.goto(n, ctx) {
                    Some(p) => vec![Req::GlideTo(p)],
                    None => vec![Req::Toast {
                        text: format!("objeto não encontrado: {n}"),
                        kind: ToastKind::Error,
                        ms: 2000,
                    }],
                }
            }
            "world.interact" => return aim_req(self, ObjectAction::Interact),
            "world.adjust_up" => return aim_req(self, ObjectAction::Adjust(1)),
            "world.adjust_down" => return aim_req(self, ObjectAction::Adjust(-1)),
            "world.prev" => return aim_req(self, ObjectAction::Prev),
            "world.next" => return aim_req(self, ObjectAction::Next),
            "notice.meow" => {
                self.play(Cue::Meow, None, ctx);
                Vec::new()
            }
            _ => return None,
        })
    }

    fn effect(&mut self, e: &nkw_core::Effect, ctx: &Ctx) -> Vec<Req> {
        let (mut reqs, cues) = self.vault.effect(e, ctx.line, ctx.now);
        self.vault_cues(cues, ctx);
        reqs.push(Req::Wish(Wish::compose(Want::Once)));
        reqs
    }

    fn inject(&mut self, json: &Value, ctx: &Ctx) -> Vec<Req> {
        self.sync(ctx);
        let parsed: Result<Inject, _> = serde_json::from_value(json.clone());
        let ev = match parsed {
            Ok(Inject::Door { id, toggle }) => {
                if toggle {
                    let mut out = self.toggle_door(DoorId(id), ctx);
                    out.push(redraw());
                    return out;
                }
                return Vec::new();
            }
            Ok(Inject::Command { name, args }) => {
                return self.command(&name, &args, ctx).unwrap_or_default();
            }
            Ok(Inject::Cue(c)) => {
                self.play(c, None, ctx);
                return Vec::new();
            }
            Ok(Inject::Shot(_)) => {
                let cues = self.vault.shot(ctx.now);
                self.vault_cues(cues, ctx);
                return vec![redraw()];
            }
            Ok(Inject::Media(m)) => SysEvent::Media(m),
            Ok(Inject::MediaGone) => SysEvent::MediaGone,
            Ok(Inject::Power(p)) => SysEvent::Power(p),
            Ok(Inject::Net(n)) => SysEvent::Net(n),
            Ok(Inject::Clock(c)) => SysEvent::Clock(c),
            Ok(Inject::Pressure { cpu, mem }) => SysEvent::Pressure { cpu, mem },
            Ok(Inject::Audio(a)) => SysEvent::Audio(a),
            Err(e) => {
                tracing::warn!(%e, "inject objects: unknown shape");
                return vec![Req::Toast {
                    text: format!("inject objects: {e}"),
                    kind: ToastKind::Error,
                    ms: 2000,
                }];
            }
        };
        self.on_sys(ev, ctx)
    }

    fn palette_items(&self, ctx: &Ctx) -> Vec<PaletteItem> {
        let _ = ctx;
        map::palette()
    }

    fn pickables(&self, ctx: &Ctx) -> Vec<Pickable> {
        self.world
            .objects
            .iter()
            .filter(|o| ctx.visible_rooms.contains(&o.room))
            .map(|o| Pickable {
                pick: Pick::Object {
                    id: o.id,
                    stand: o.stand,
                },
                pos: o.pos,
                radius: 0.3,
            })
            .collect()
    }

    fn tags(&self, ctx: &Ctx) -> Vec<Tag> {
        let _ = ctx;
        self.tags.clone()
    }

    fn badges(&self, ctx: &Ctx) -> Vec<Badge> {
        let _ = ctx;
        self.badges_now().into_iter().map(map::badge).collect()
    }

    fn obstacles(&self, room: RoomId, ctx: &Ctx) -> Vec<Aabb> {
        let mut v = self.doors.obstacles(room, ctx.now);
        v.extend(
            self.world
                .objects
                .iter()
                .filter(|o| o.room == room && place::blocks_floor(o.kind))
                .map(|o| o.aabb),
        );
        v
    }

    fn opens_rooms(&self, from: RoomId, ctx: &Ctx) -> Vec<RoomId> {
        let c = ctx.world.camera.pos;
        self.doors
            .opens_rooms(from, doors::in_hall(Vec2::new(c.x, c.z)), ctx.now)
    }

    fn dump(&self) -> Value {
        let objects: Vec<Value> = self
            .world
            .objects
            .iter()
            .map(|o| {
                serde_json::json!({
                    "id": o.id.0,
                    "kind": Self::kind_name(o.kind),
                    "room": o.room.0,
                    "tier": format!("{:?}", self.tiers.get(&o.id).copied().unwrap_or(Tier::Far)),
                    "data_kind": Self::data_kind(self.world.data.get(&o.id)),
                })
            })
            .collect();
        let rates = self
            .needs
            .iter()
            .filter(|n| matches!(n.want, Want::Rate(_)))
            .count();
        serde_json::json!({
            "objects": objects,
            "doors": self.doors.dump(self.now),
            "audio": self.audio.dump(),
            "badges": serde_json::to_value(self.badges_now()).unwrap_or(Value::Null),
            "rate_wishes": rates,
            "visible_rooms": self.visible.iter().map(|r| r.0).collect::<Vec<_>>(),
            "sound": {
                "played": self.sound.played.count,
                "active": self.sound.active(),
                "last": self.sound.played.last.iter().map(|c| format!("{c:?}")).collect::<Vec<_>>(),
            },
            "sys": map::sys_status(self.sys_on && self.port.is_some(), &self.errors),
            "demo": self.demo,
            "band": self.band,
        })
    }

    fn shutdown(&mut self) {
        if let Some(p) = self.port.take() {
            self.audio.restore(p.audio());
            p.shutdown();
        }
    }
}

#[cfg(test)]
mod tests;
