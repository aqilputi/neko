//! The cats (§6): one villager per core session at a desk of its context's
//! room, deterministic coats, poses from `nk-cats`, monitors with the tail
//! or the live pane, FOCO on a cat's terminal, CONVERSA mapped to core
//! calls, the inbox walking to you, the postman and the receptionist.
//!
//! Where the sessions come from (`--core`): `none` (the default under
//! `--demo`) is the fixture of [`demo`], whose core calls are recorded in
//! `dump()["calls"]`; the others are the [`core_link`] thread. Either way
//! they go through [`office::Office`], whose contexts lay out the rooms
//! through `World::sync_contexts` (applied on the loop, see
//! [`Cats::take_sync`]).
//!
//! `inject cats` shapes: `{"fixture":"demo"|"busy"}`; `{"sessions":[…]}`
//! (core `Session` JSON, tree order = array order); `{"event":{"name":…,
//! "data":…}}` (a raw core event: `session.state` patches a fixture
//! session by `session_id`); `{"cmd":"world.talk","args":{…}}` (a command
//! by name, as a key would send it).
//!
//! `dump()`: `{"core","cats":[{id,session,label,room,pose,state,walking}],
//! "dialog","inbox":{queue,visible,overflow},"pane":{cat,cols,rows}|null,
//! "calls":[…]}`.

pub mod body;
pub mod conv;
pub mod core_link;
pub mod demo;
pub mod inbox;
pub mod monitor;
pub mod office;
pub mod screens;
pub mod talk;
pub mod walk;

use std::collections::{BTreeMap, HashMap};

use glam::{Mat4, Vec3};
use nk_cats::catpose::{self, CatParams};
use nk_cats::model::Pose as CatPose;
use nkw_core::{CatId, Effect, Pick, RoomId, Target, Time, Want, Wish, layout};
use nkw_gl::draw::{Blob, DrawList, Instance, MeshId};
use serde_json::{Value, json};

use crate::ext::{Badge, Ctx, DialogView, Pickable, Plugin, Req, Tag, ToastKind};
#[cfg(test)]
use crate::router::KeyEv;
use body::{CAT_CENTRE, Coat, FACE_Y, SEAT_TOP};
use core_link::{FromCore, Link, ToCore};
use office::{Office, RECEPTIONIST};
use talk::Call;

/// Ambient motion rate (§6.2: 12–15 fps).
pub const AMBIENT_HZ: f32 = 15.0;
/// Cats farther than this from the camera do not animate.
pub const ANIMATE_M: f32 = 12.0;
/// Name badges are hidden beyond this distance.
pub const BADGE_M: f32 = 7.0;
/// The receptionist's coat seed.
const RECEPTIONIST_SEED: &str = "recepção";
/// The fixture's clock: the world's second 0 is this ms (a little after the
/// fixture's last activity).
fn fixture_epoch_ms() -> i64 {
    nk_cats::model::parse_rfc3339(demo::T0) + 11 * 60_000 + 5_000
}

/// Where the cats come from.
#[derive(Clone, Debug, PartialEq)]
pub enum Source {
    /// `--core none`: the fixture
    Fixture,
    /// the core link (`auto`, `demo`, a URL)
    Core(crate::cli::Core),
}

/// The rooms `World::sync_contexts` takes.
pub type Contexts = Vec<(String, Vec<CatId>)>;

pub struct Cats {
    source: Source,
    reduced_motion: bool,
    pub office: Office,
    /// "none" | "connecting" | "ok" | "error"
    status: &'static str,
    /// the contexts the World must be brought in line with
    sync: Option<Contexts>,
    lh: Option<calloop::LoopHandle<'static, crate::state::Nkw>>,
    /// fixture mode: the core calls that would have been made
    pub calls: Vec<Value>,
    /// the fixture's snapshot (events patch it)
    fixture: Option<(nk_client::Snapshot, nk_client::Tree)>,
    /// the world clock at the last hook
    now: Time,
    pub talk: Option<conv::Talk>,
    pub walks: walk::Walks,
    mons: HashMap<CatId, screens::Mon>,
    panes: HashMap<String, screens::Pane>,
    focus: Option<screens::Focus>,
    fonts: screens::Fonts,
    link: Option<Link>,
    next_tag: u64,
}

impl Cats {
    pub fn new(cli: &crate::cli::Cli) -> Cats {
        let source = match cli.core() {
            crate::cli::Core::None => Source::Fixture,
            other => Source::Core(other),
        };
        let mut c = Cats {
            status: if source == Source::Fixture {
                "none"
            } else {
                "connecting"
            },
            source,
            reduced_motion: cli.reduced_motion,
            office: Office::new(),
            sync: None,
            lh: None,
            calls: Vec::new(),
            fixture: None,
            now: 0.0,
            talk: None,
            walks: walk::Walks::default(),
            mons: HashMap::new(),
            panes: HashMap::new(),
            focus: None,
            fonts: screens::Fonts::default(),
            link: None,
            next_tag: 1,
        };
        if c.source == Source::Fixture {
            c.walks.offset_ms = fixture_epoch_ms();
            if cli.demo {
                c.load_fixture(demo::Fixture::Demo, 0.0);
                // the demo starts with everyone at their desks
                let now = c.clock_ms(0.0);
                c.walks.prime(&c.office, now);
            }
        }
        c
    }

    /// The queue's clock (ms) at world time `now`.
    pub fn clock_ms(&self, now: Time) -> i64 {
        (now * 1000.0) as i64 + self.walks.offset_ms
    }

    /// Replace the office with a fixture's sessions.
    pub fn load_fixture(&mut self, f: demo::Fixture, now: Time) {
        let (snap, tree) = demo::snapshot(f);
        // the busy fixture's letter is fresh at `now`
        let at = self.clock_ms(now);
        self.walks.mail = snap
            .mail
            .iter()
            .map(|(k, m)| (k.clone(), (at, m.sent)))
            .collect();
        if f == demo::Fixture::Busy {
            let (from, to, text) = demo::LETTER;
            self.walks
                .notes
                .insert(demo::id_of(to), (from.to_string(), text.to_string()));
        }
        self.apply_snapshot(&snap, &tree.rows, now);
        self.fixture = Some((snap, tree));
    }

    /// A snapshot and tree rows from the core (or the fixture).
    pub fn apply_snapshot(
        &mut self,
        snap: &nk_client::Snapshot,
        rows: &[nk_client::TreeRow],
        now: Time,
    ) -> office::Change {
        let change = self.office.apply(snap, rows, now);
        if change.layout || !change.added.is_empty() || !change.removed.is_empty() {
            self.sync = Some(self.office.contexts());
        }
        for cat in &change.added {
            let Some(v) = self.office.get(*cat) else {
                continue;
            };
            let (id, label) = (v.sess.session_id.clone(), v.label().to_string());
            match &self.link {
                Some(l) => {
                    let _ = l.tx.send(ToCore::Tail(id));
                }
                None => {
                    let lines = demo::tail(&label);
                    self.feed_tail(*cat, &lines, false);
                }
            }
        }
        for cat in change.added.iter().chain(&change.posed) {
            if let Some(m) = self.mons.get_mut(cat) {
                m.stale = true;
            }
        }
        for cat in &change.removed {
            self.mons.remove(cat);
        }
        let step_at = self.clock_ms(now);
        self.walks.step(&self.office, step_at);
        change
    }

    /// The contexts the World still has to take (`World::sync_contexts`),
    /// once. The plugin hands them to the loop itself; tests apply them.
    pub fn take_sync(&mut self) -> Option<Contexts> {
        self.sync.take()
    }

    /// CONVERSA with purpose `Session`.
    pub fn dialog(&self) -> Option<DialogView> {
        self.dialog_view()
    }

    /// A core call (fixture: recorded).
    pub(crate) fn core_call(&mut self, call: Call, ctx: &Ctx) {
        let Some(link) = &self.link else {
            self.record_call(&call);
            return;
        };
        match call {
            Call::Reply { session, text } => {
                let label = self
                    .office
                    .by_session(&session)
                    .map(|v| v.label().to_string())
                    .unwrap_or_default();
                let (cols, rows) = self.pane_size_for(ctx, &session);
                self.ensure_pane(&session, &label, cols, rows, Some(text));
            }
            Call::Run { name, session } => {
                let tag = self.next_tag;
                self.next_tag += 1;
                let _ = link.tx.send(ToCore::Run {
                    tag,
                    name,
                    session,
                    args: BTreeMap::new(),
                });
            }
            Call::Confirm { token, yes } => {
                let tag = self.next_tag;
                self.next_tag += 1;
                let _ = link.tx.send(ToCore::Confirm { tag, token, yes });
            }
        }
    }

    /// A call whose answer the box waits for: the fixture answers at once
    /// with a confirmation token; the core later (`None`).
    pub(crate) fn core_ask(&mut self, call: Call, ctx: &Ctx) -> Option<String> {
        if self.link.is_none() {
            self.record_call(&call);
            return Some(format!("demo-{}", self.calls.len()));
        }
        let tag = self.next_tag;
        self.core_call(call, ctx);
        if let Some(t) = self.talk.as_mut() {
            t.waiting = Some(tag);
        }
        None
    }

    fn record(&mut self, v: Value) {
        self.calls.push(v);
    }

    fn record_call(&mut self, call: &Call) {
        let v = match call {
            Call::Reply { session, text } => {
                self.fixture_reply(session, text);
                json!({"call": "reply", "session": session, "reply": text})
            }
            Call::Run { name, session } => json!({"call": "run", "name": name, "session": session}),
            Call::Confirm { token, yes } => json!({"call": "confirm", "token": token, "yes": yes}),
        };
        self.record(v);
    }

    /// The fixture's answer to a reply: the tail shows it typed (the
    /// option's label for a number, like the server's ResolveOption).
    fn fixture_reply(&mut self, session: &str, text: &str) {
        let Some(v) = self.office.by_session(session) else {
            return;
        };
        let typed = text
            .trim()
            .parse::<usize>()
            .ok()
            .and_then(|n| {
                v.sess
                    .questions
                    .iter()
                    .find(|q| !q.options.is_empty())
                    .and_then(|q| q.options.get(n.wrapping_sub(1)))
                    .map(|o| o.label.clone())
            })
            .unwrap_or_else(|| text.to_string());
        let cat = v.cat;
        let mut lines: Vec<nk_client::TailLine> = demo::tail(v.label());
        lines.push(nk_client::TailLine {
            at: demo::T0.into(),
            kind: "user".into(),
            text: format!("> {typed}"),
        });
        self.feed_tail(cat, &lines, true);
    }

    /// A pane's size for a session's monitor (cells of the FOCO font).
    fn pane_size_for(&mut self, ctx: &Ctx, session: &str) -> (u16, u16) {
        let logical = self
            .office
            .by_session(session)
            .and_then(|v| ctx.world.cat_desk(v.cat))
            .and_then(|(r, d)| ctx.world.monitor_place(r, d))
            .map(|(_, l)| l)
            .unwrap_or(glam::Vec2::new(800.0, 600.0));
        let face = self.fonts.face(monitor::FOCUS_PX);
        monitor::fit_cells(logical.x, logical.y, face.cw, face.ch)
    }

    /// The coat, pose and collar of a cat.
    fn look(&self, cat: CatId) -> (Coat, CatPose, Time, Option<[u8; 4]>, String) {
        if cat == RECEPTIONIST {
            let seed = office::fnv1a(RECEPTIONIST_SEED);
            return (
                Coat::from_seed(seed),
                CatPose::Idle,
                0.0,
                None,
                String::new(),
            );
        }
        if cat.0 >= walk::POSTMAN_BASE {
            let seed = office::fnv1a(walk::POSTMAN_SEED);
            return (
                Coat::from_seed(seed),
                CatPose::Idle,
                0.0,
                None,
                String::new(),
            );
        }
        match self.office.get(cat) {
            Some(v) => (
                Coat::from_seed(v.seed),
                v.pose,
                v.pose_since,
                v.collar.map(|c| [c[0], c[1], c[2], 0xff]),
                v.sess.session_id.clone(),
            ),
            // a desk cat the office does not know (yet): a plain sitter
            None => (
                Coat::from_seed(cat.0 as u64),
                CatPose::Idle,
                0.0,
                None,
                String::new(),
            ),
        }
    }

    /// Whether a cat at `at` moves at `now`: not under reduced motion, not
    /// in FOCO, close enough, busy or alert always, calm ones in their
    /// bouts, a puff while it lasts. The receptionist sits still.
    fn animates(&self, cat: CatId, ctx: &Ctx, cam: Vec3, at: Vec3) -> bool {
        if self.still(ctx) || cat == RECEPTIONIST || cam.distance(at) > ANIMATE_M {
            return false;
        }
        // seen: in the camera's room (the others are behind walls, an open
        // door only shows a sliver) and inside the view
        let here = ctx.world.current_room_id();
        if ctx.world.cat_desk(cat).is_some_and(|(r, _)| r != here)
            || !in_frustum(ctx, at + Vec3::Y * 0.4)
        {
            return false;
        }
        let (_, pose, since, _, id) = self.look(cat);
        match pose {
            CatPose::Typing | CatPose::Alert => true,
            CatPose::Puff if ctx.now - since < catpose::PUFF_S as f64 => true,
            _ => catpose::in_bout(ctx.now as f32, catpose::phase_of(&id)),
        }
    }

    fn still(&self, ctx: &Ctx) -> bool {
        self.reduced_motion || ctx.reduced_motion || ctx.world.frozen
    }

    /// The joints of a cat at `now`.
    fn joints(&self, cat: CatId, now: Time, animate: bool) -> CatParams {
        let (_, pose, since, _, id) = self.look(cat);
        let base = catpose::pose_params(pose);
        let mut j = if animate {
            catpose::ambient(pose, base, now as f32, catpose::phase_of(&id))
        } else {
            base
        };
        let u = ((now - since) / catpose::PUFF_S as f64) as f32;
        if pose == CatPose::Puff && (0.0..1.0).contains(&u) && animate {
            j = catpose::puff_at(j, u);
        }
        // in CONVERSA it turns to you
        if self.talk.as_ref().is_some_and(|t| t.cat == cat) {
            j.head_rot_y = std::f32::consts::PI;
            j.head_rot_x = 0.1;
            if !self.talk.as_ref().is_some_and(|t| t.reveal.done(now)) && !self.reduced_motion {
                // the mouth moves while the text appears
                j.head_y += 0.012 * ((now * 18.0) as f32).sin();
            }
        }
        j
    }

    /// Every desk cat of the visible rooms with its seat, except those out
    /// walking.
    fn seated(&self, ctx: &Ctx) -> Vec<(RoomId, usize, CatId, nkw_core::Pose)> {
        let w = ctx.world;
        let mut v = Vec::new();
        for room in w.rooms.iter().filter(|r| ctx.visible_rooms.contains(&r.id)) {
            for (d, desk) in room.desks.iter().enumerate() {
                if let (Some(cat), Some(seat)) = (desk.cat, w.cat_seat(room.id, d))
                    && !self.walks.out(cat)
                {
                    v.push((room.id, d, cat, seat));
                }
            }
        }
        v
    }

    /// A core event (from the link, or injected).
    fn event(&mut self, name: &str, data: &Value, ctx: &Ctx) -> Vec<Req> {
        let s = |k: &str| {
            data.get(k)
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string()
        };
        match name {
            "toast" => {
                let err = data.get("err").and_then(Value::as_bool).unwrap_or(false);
                toast(&s("text"), err)
            }
            "notice" | "notice.meow" => {
                let mut r = Vec::new();
                if !s("meow").is_empty() || name == "notice.meow" {
                    r.push(Req::Cue {
                        cue: nkw_objects::sound::Cue::Meow,
                        at: self.receptionist_at(ctx),
                    });
                }
                if !s("toast").is_empty() {
                    let err = data
                        .get("toast_err")
                        .and_then(Value::as_bool)
                        .unwrap_or(false);
                    r.extend(toast(&s("toast"), err));
                }
                r
            }
            "confirm.requested" => {
                let (token, question) = (s("token"), s("question"));
                if token.is_empty() {
                    return Vec::new();
                }
                match self.talk.is_some() {
                    // inside the conversation: Sim / Não in the box itself
                    true => self.ask(&token, &question, ctx.now),
                    false => self.walks.confirm = Some((token, question)),
                }
                Vec::new()
            }
            "confirm.resolved" => {
                let token = s("token");
                if self.walks.confirm.as_ref().is_some_and(|c| c.0 == token) {
                    self.walks.confirm = None;
                }
                if self
                    .talk
                    .as_ref()
                    .is_some_and(|t| t.pending.as_deref() == Some(&token))
                {
                    self.talk = None;
                    return vec![Req::Action(nkw_core::Action::Back)];
                }
                Vec::new()
            }
            "peer.message" => {
                let (to, from, text) = (s("to"), s("from"), s("text"));
                self.walks.q = nk_cats::inbox::note_peer(&self.walks.q, &to, &from, &text);
                self.walks.notes.insert(to, (from, text));
                Vec::new()
            }
            "mail.cue" => {
                let sent = data.get("sent").and_then(Value::as_bool).unwrap_or(false);
                let at = self.clock_ms(ctx.now);
                self.walks.mail.insert(s("id"), (at, sent));
                let at = self.clock_ms(ctx.now);
                self.walks.step(&self.office, at);
                Vec::new()
            }
            "ui.scene" => {
                let args = data
                    .get("args")
                    .and_then(Value::as_object)
                    .map(|o| {
                        o.iter()
                            .map(|(k, v)| {
                                (k.clone(), v.as_str().map_or(v.to_string(), str::to_string))
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                vec![Req::Command {
                    name: s("command"),
                    args,
                }]
            }
            "session.state" if self.link.is_none() => {
                // the fixture: patch the session and apply again
                let Some((mut snap, tree)) = self.fixture.take() else {
                    return Vec::new();
                };
                let id = s("session_id");
                if let Some(sess) = snap.sessions.iter_mut().find(|x| x.session_id == id) {
                    let mut v = serde_json::to_value(&*sess).unwrap_or(Value::Null);
                    if let (Some(o), Some(patch)) = (v.as_object_mut(), data.as_object()) {
                        for (k, x) in patch {
                            o.insert(k.clone(), x.clone());
                        }
                    }
                    if let Ok(n) = serde_json::from_value(v) {
                        *sess = n;
                    }
                }
                self.apply_snapshot(&snap, &tree.rows, ctx.now);
                self.fixture = Some((snap, tree));
                vec![Req::Wish(Wish::world(Want::Once))]
            }
            _ => Vec::new(),
        }
    }

    /// Where the receptionist is (for her meows).
    fn receptionist_at(&self, ctx: &Ctx) -> Option<Vec3> {
        if let Some((p, _)) = self.walks.face(RECEPTIONIST, ctx.now) {
            return Some(p);
        }
        let (room, desk) = ctx.world.cat_desk(RECEPTIONIST)?;
        ctx.world.cat_seat(room, desk).map(|s| s.pos)
    }

    /// A message of the core link.
    fn on_core(&mut self, m: FromCore, ctx: &Ctx) -> Vec<Req> {
        match m {
            FromCore::Status(st, msg) => {
                let was = self.status;
                self.status = st;
                if st == "error" && was != "error" && !msg.is_empty() {
                    return toast(&msg, true);
                }
                Vec::new()
            }
            FromCore::Theme(p) => {
                self.office.palette = p;
                for m in self.mons.values_mut() {
                    m.raster = None;
                }
                Vec::new()
            }
            FromCore::Keymap(b) => vec![Req::Keymap(b)],
            FromCore::World(snap, tree) => {
                let at = self.clock_ms(ctx.now);
                for (id, m) in &snap.mail {
                    let t = nk_cats::model::parse_rfc3339(&m.at);
                    self.walks
                        .mail
                        .insert(id.clone(), (if t > 0 { t } else { at }, m.sent));
                }
                self.walks.mail.retain(|id, _| snap.mail.contains_key(id));
                self.apply_snapshot(&snap, &tree.rows, ctx.now);
                vec![Req::Wish(Wish::world(Want::Once))]
            }
            FromCore::Tail(id, lines) => {
                if let Some(cat) = self.office.by_session(&id).map(|v| v.cat) {
                    self.feed_tail(cat, &lines, true);
                }
                Vec::new()
            }
            FromCore::Event(name, data) => self.event(&name, &data, ctx),
            FromCore::Done(tag, res) => {
                let waited = self.talk.as_ref().is_some_and(|t| t.waiting == Some(tag));
                match (res, waited) {
                    (Ok(r), true) if r.pending.is_some() => {
                        let p = r.pending.unwrap_or_default();
                        let label = self
                            .talk
                            .as_ref()
                            .map(|t| t.name.clone())
                            .unwrap_or_default();
                        let call = Call::Run {
                            name: String::new(),
                            session: String::new(),
                        };
                        let q = talk::confirm_text(&p.question, &call, &label);
                        self.ask(&p.token, &q, ctx.now);
                        Vec::new()
                    }
                    (Ok(r), true) => {
                        if let Some(t) = self.talk.as_mut() {
                            t.waiting = None;
                        }
                        let mut out = toast(&r.toast, r.err);
                        out.push(Req::Action(nkw_core::Action::Back));
                        out
                    }
                    (Ok(r), false) => toast(&r.toast, r.err),
                    (Err(e), w) => {
                        if w && let Some(t) = self.talk.as_mut() {
                            t.waiting = None;
                        }
                        toast(&e, true)
                    }
                }
            }
            FromCore::Pane(session, conn) => {
                match self.panes.get_mut(&session) {
                    Some(p) => p.conn = Some(conn),
                    None => {
                        let mut term = nk_client::Term::new(80, 24);
                        let pal = &self.office.palette;
                        term.set_default_colors(pal.text, pal.bg);
                        self.panes.insert(
                            session.clone(),
                            screens::Pane {
                                session,
                                conn: Some(conn),
                                term,
                                cols: 80,
                                rows: 24,
                                exited: false,
                            },
                        );
                    }
                }
                Vec::new()
            }
            FromCore::PaneFailed(_, e) => toast(&format!("painel: {e}"), true),
            FromCore::PaneMsg(session, msg) => {
                match msg {
                    nk_client::PaneMsg::Data(b) => self.pane_data(&session, &b),
                    nk_client::PaneMsg::Title(_) => {}
                    nk_client::PaneMsg::Exit(_) | nk_client::PaneMsg::Closed => {
                        if let Some(p) = self.panes.get_mut(&session) {
                            p.exited = true;
                            p.conn = None;
                        }
                    }
                }
                match ctx.world.frozen {
                    true => vec![Req::Wish(Wish::compose(Want::Once))],
                    false => vec![Req::Wish(Wish::world(Want::Once))],
                }
            }
        }
    }

    /// A walker's instances: sitting before you, or on its way.
    fn draw_walkers(&self, ctx: &Ctx, cam: Vec3, list: &mut DrawList) {
        for x in &self.walks.walks {
            let (p, yaw, moving) = x.at3(ctx.now);
            let mut j = catpose::SIT;
            let mut lift = 0.0;
            if moving && !self.still(ctx) {
                let (legs, bob) = catpose::walk_cycle(ctx.now as f32);
                j.leg_l = 0.6 + legs[0];
                j.leg_r = 0.6 + legs[1];
                j.body_rot_x = -0.35;
                lift = bob;
            } else if self.talk.as_ref().is_some_and(|t| t.cat == x.cat) {
                j = self.joints(x.cat, ctx.now, false);
                j.head_rot_y = 0.0; // already facing you
            }
            let at = p + Vec3::Y * lift;
            let fade = layout::fade_for(cam, at + Vec3::Y * CAT_CENTRE);
            let coat = Coat::from_seed(x.seed);
            body::cat_instances(
                &body::joints(&j),
                &coat,
                body::root(at, yaw),
                None,
                fade,
                &mut list.instances,
            );
            if x.postman {
                let m = body::root(at, yaw) * Mat4::from_translation(Vec3::new(0.0, 0.3, -0.2));
                list.instances.push(Instance {
                    mesh: MeshId::Envelope,
                    model: m,
                    tint: [0xff; 4],
                    emissive: 0.0,
                    fade,
                });
            }
            list.blobs.push(Blob {
                center: p + Vec3::Y * 0.01,
                radius: 0.3,
                alpha: 0.35,
            });
        }
    }

    fn draw_kittens(&self, cat: CatId, seat: &nkw_core::Pose, cam: Vec3, list: &mut DrawList) {
        let Some(v) = self.office.get(cat) else {
            return;
        };
        let (kittens, _) = v.kittens();
        for (i, k) in kittens.iter().enumerate() {
            let p = kitten_spot(seat, i);
            let coat = Coat::from_seed(office::fnv1a(&k.id));
            let fade = layout::fade_for(cam, p + Vec3::Y * 0.1);
            body::kitten_instances(
                &coat,
                body::root(p, seat.yaw),
                !k.done,
                fade,
                &mut list.instances,
            );
        }
    }
}

/// Whether `p` is inside the camera's view (with a small margin).
fn in_frustum(ctx: &Ctx, p: Vec3) -> bool {
    let w = ctx.world;
    let cam = nkw_gl::draw::Camera {
        pose: w.camera,
        fov_y: crate::world::FOV_Y,
        near: 0.05,
        far: 60.0,
        aspect: w.out.width_px as f32 / w.out.height_px.max(1) as f32,
    };
    let c = cam.proj() * cam.view() * p.extend(1.0);
    c.w > 0.0 && (c.x / c.w).abs() < 1.15 && (c.y / c.w).abs() < 1.15
}

fn toast(text: &str, err: bool) -> Vec<Req> {
    if text.is_empty() {
        return Vec::new();
    }
    vec![Req::Toast {
        text: text.to_string(),
        kind: if err {
            ToastKind::Error
        } else {
            ToastKind::Info
        },
        ms: if err { 6000 } else { 3000 },
    }]
}

/// The model matrix of a seated cat: on the seat, facing its screen.
pub fn seat_root(seat: &nkw_core::Pose) -> Mat4 {
    body::root(seat.pos + Vec3::Y * SEAT_TOP, seat.yaw)
}

/// A stable tag id for a cat's badge of `kind`.
fn tag_id(cat: CatId, kind: u64) -> u64 {
    0x4341_5400_0000_0000 | ((cat.0 as u64) << 8) | kind
}

/// Where kitten `i` sits on the desk of the cat at `seat`: on the desk top
/// around the monitor, two on each side.
pub fn kitten_spot(seat: &nkw_core::Pose, i: usize) -> Vec3 {
    let f = layout::facing(seat.yaw);
    let f = Vec3::new(f.x, 0.0, f.z).normalize_or_zero();
    let right = Vec3::new(-f.z, 0.0, f.x);
    let side = if i.is_multiple_of(2) { -1.0 } else { 1.0 };
    let (out, deep) = if i < 2 { (0.42, 0.40) } else { (0.62, 0.62) };
    let floor = Vec3::new(seat.pos.x, layout::DESK_TOP, seat.pos.z);
    floor + f * deep + right * (side * out)
}

impl Plugin for Cats {
    fn name(&self) -> &'static str {
        "cats"
    }

    fn start(
        &mut self,
        cli: &crate::cli::Cli,
        lh: &calloop::LoopHandle<'static, crate::state::Nkw>,
        ping: calloop::ping::Ping,
    ) {
        let _ = cli;
        self.lh = Some(lh.clone());
        self.fonts.warm();
        if let Source::Core(c) = &self.source
            && let Some(opts) = core_link::opts_for(c)
        {
            self.walks.offset_ms = nk_cats::model::now_ms();
            self.link = Some(core_link::spawn(
                opts,
                std::sync::Arc::new(move || ping.ping()),
            ));
        }
    }

    fn drain(&mut self, ctx: &Ctx) -> Vec<Req> {
        self.now = ctx.now;
        let Some(link) = &self.link else {
            return Vec::new();
        };
        let msgs: Vec<FromCore> = link.rx.try_iter().collect();
        let mut reqs = Vec::new();
        for m in msgs {
            reqs.extend(self.on_core(m, ctx));
        }
        reqs
    }

    fn tick(&mut self, ctx: &Ctx) -> Vec<Req> {
        self.now = ctx.now;
        if self.walks.offset_ms == 0 {
            self.walks.offset_ms = nk_cats::model::now_ms() - (ctx.now * 1000.0) as i64;
        }
        if let Some(lh) = &self.lh
            && let Some(c) = self.sync.take()
        {
            // the World is the hub's: apply on the loop, right after this
            lh.insert_idle(move |st| {
                let now = st.now();
                let outs = st.world.sync_contexts(&c, now);
                st.apply(outs);
                st.wake();
            });
        }
        let mut reqs = self.talk_tick(ctx);
        let now_ms = self.clock_ms(ctx.now);
        self.walks.step_due(&self.office, now_ms);
        self.walks.plan(ctx, &self.office, now_ms);
        if !ctx.world.frozen && self.paint_monitors(ctx) {
            reqs.push(Req::Wish(Wish::world(Want::Once)));
        }
        reqs
    }

    fn wishes(&self, ctx: &Ctx) -> Vec<Wish> {
        let mut out = Vec::new();
        // the box's letters, at 30 Hz while they appear (composed only)
        if let Some(t) = self.talk_wake(ctx.now) {
            match self.reduced_motion || ctx.reduced_motion {
                true => out.push(Wish::compose(Want::At(t))),
                false => out.push(Wish::compose(Want::Rate(talk::REVEAL_HZ))),
            }
        }
        if ctx.world.frozen {
            return out;
        }
        if let Some(t) = self.monitor_wake(ctx) {
            out.push(Wish::world(Want::At(t)));
        }
        let still = self.still(ctx);
        if self.walks.moving(ctx.now) && !still {
            out.push(Wish::world(Want::Rate(inbox::WALK_HZ)));
        }
        if let Some(t) = self.walks.next_wake_ms(self.clock_ms(ctx.now)) {
            let at = (t - self.walks.offset_ms) as f64 / 1000.0;
            out.push(Wish::world(Want::At(at)));
        }
        if still {
            return out;
        }
        let cam = ctx.world.camera.pos;
        let mut next: Option<Time> = None;
        for (_, _, cat, seat) in self.seated(ctx) {
            if self.animates(cat, ctx, cam, seat.pos) {
                out.push(Wish::world(Want::Rate(AMBIENT_HZ)));
                return out;
            }
            let (_, pose, _, _, id) = self.look(cat);
            if cat == RECEPTIONIST
                || cam.distance(seat.pos) > ANIMATE_M
                || !matches!(pose, CatPose::Idle | CatPose::Sleeping)
            {
                continue;
            }
            // a calm cat: wake for its next bout
            let phase = catpose::phase_of(&id);
            let off = phase / std::f32::consts::TAU * catpose::BOUT_EVERY_S;
            let every = catpose::BOUT_EVERY_S as f64;
            let k = ((ctx.now + off as f64) / every).floor() + 1.0;
            let at = k * every - off as f64;
            next = Some(next.map_or(at, |n: Time| n.min(at)));
        }
        if let Some(t) = next {
            out.push(Wish::world(Want::At(t)));
        }
        out
    }

    unsafe fn upload(&mut self, gl: &nkw_gl::glow::Context, ctx: &Ctx) {
        let _ = ctx;
        unsafe { self.upload_monitors(gl) };
    }

    fn draw(&self, ctx: &Ctx, list: &mut DrawList) {
        let cam = list.camera.pose.pos;
        for (_, _, cat, seat) in self.seated(ctx) {
            let fade = layout::fade_for(cam, seat.pos + Vec3::Y * CAT_CENTRE);
            let animate = self.animates(cat, ctx, cam, seat.pos);
            let j = self.joints(cat, ctx.now, animate);
            let (coat, _, _, collar, _) = self.look(cat);
            body::cat_instances(
                &body::joints(&j),
                &coat,
                seat_root(&seat),
                collar,
                fade,
                &mut list.instances,
            );
            list.blobs.push(Blob {
                center: Vec3::new(seat.pos.x, 0.01, seat.pos.z),
                radius: 0.32,
                alpha: 0.35,
            });
            self.draw_kittens(cat, &seat, cam, list);
        }
        self.draw_walkers(ctx, cam, list);
        self.draw_monitors(ctx, list);
    }

    fn command(
        &mut self,
        name: &str,
        args: &BTreeMap<String, String>,
        ctx: &Ctx,
    ) -> Option<Vec<Req>> {
        self.now = ctx.now;
        self.talk_command(name, args, ctx)
    }

    fn effect(&mut self, e: &Effect, ctx: &Ctx) -> Vec<Req> {
        self.now = ctx.now;
        match *e {
            Effect::OpenPane(cat) => {
                self.open_pane(cat, ctx);
                Vec::new()
            }
            Effect::KeyboardEnter(Target::CatTerm(_)) => vec![Req::Wish(Wish::compose(Want::Once))],
            Effect::KeyboardLeave => {
                self.leave_pane();
                Vec::new()
            }
            _ => self.talk_effect(e, ctx),
        }
    }

    fn tags(&self, ctx: &Ctx) -> Vec<Tag> {
        let mut tags = Vec::new();
        for (_, _, cat, seat) in self.seated(ctx) {
            let over = seat.pos + Vec3::Y * 0.95;
            let Some(v) = self.office.get(cat) else {
                if cat == RECEPTIONIST && self.walks.confirm.is_some() {
                    tags.push(question_tag(over));
                }
                continue;
            };
            tags.push(Tag {
                id: tag_id(cat, 0),
                pos: over,
                text: v.label().to_string(),
                rgb: v.ctx_rgb,
                max_m: BADGE_M,
            });
            let mark = match v.pose {
                CatPose::Alert => Some("!"),
                CatPose::Sleeping => Some("zZ"),
                _ => None,
            };
            if let Some(m) = mark {
                tags.push(Tag {
                    id: tag_id(cat, 1),
                    pos: over + Vec3::Y * 0.12,
                    text: m.into(),
                    rgb: v.ctx_rgb,
                    max_m: BADGE_M,
                });
            }
            let (_, more) = v.kittens();
            if more > 0 {
                tags.push(Tag {
                    id: tag_id(cat, 2),
                    pos: kitten_spot(&seat, office::MAX_KITTENS) + Vec3::Y * 0.2,
                    text: format!("+{more}"),
                    rgb: v.ctx_rgb,
                    max_m: BADGE_M,
                });
            }
        }
        for x in self.walks.walks.iter().filter(|x| !x.home) {
            let (p, _, _) = x.at3(ctx.now);
            let over = p + Vec3::Y * 0.95;
            let label = match self.office.get(x.cat) {
                Some(v) => v.label().to_string(),
                None if x.postman => "carteiro".into(),
                None => "recepcionista".into(),
            };
            tags.push(Tag {
                id: tag_id(x.cat, 3),
                pos: over,
                text: label,
                rgb: [0xe8, 0x9a, 0x6a],
                max_m: BADGE_M,
            });
            if x.cat == RECEPTIONIST {
                tags.push(question_tag(over + Vec3::Y * 0.12));
            }
        }
        tags
    }

    fn badges(&self, ctx: &Ctx) -> Vec<Badge> {
        let (queue, _, overflow) = self.walks.counts();
        let mut b = Vec::new();
        if ctx.world.mode().focus().is_some() {
            if queue > 0 {
                b.push(Badge::Inbox(queue as u32));
            }
            if self.walks.confirm.is_some() {
                b.push(Badge::Confirm);
            }
        } else if overflow > 0 {
            b.push(Badge::Inbox(overflow as u32));
        }
        b
    }

    fn pickables(&self, ctx: &Ctx) -> Vec<Pickable> {
        let mut v: Vec<Pickable> = self
            .seated(ctx)
            .into_iter()
            .map(|(_, _, cat, seat)| Pickable {
                pick: Pick::Cat(cat),
                pos: seat.pos + Vec3::Y * (SEAT_TOP + FACE_Y),
                radius: 0.3,
            })
            .collect();
        for x in self.walks.walks.iter().filter(|x| !x.home) {
            let (p, _, _) = x.at3(ctx.now);
            v.push(Pickable {
                pick: Pick::Cat(x.cat),
                pos: p + Vec3::Y * FACE_Y,
                radius: 0.3,
            });
        }
        v
    }

    fn inject(&mut self, json: &Value, ctx: &Ctx) -> Vec<Req> {
        self.now = ctx.now;
        let mut reqs = Vec::new();
        if let Some(f) = json.get("fixture").and_then(Value::as_str) {
            match demo::Fixture::parse(f) {
                Some(f) => self.load_fixture(f, ctx.now),
                None => tracing::warn!("inject cats: no fixture {f:?}"),
            }
            reqs.push(Req::Wish(Wish::world(Want::Once)));
        }
        if let Some(list) = json.get("sessions") {
            match serde_json::from_value::<Vec<nk_client::Session>>(list.clone()) {
                Ok(sessions) => {
                    let rows = demo::rows(&sessions);
                    let snap = nk_client::Snapshot {
                        sessions,
                        ..nk_client::Snapshot::default()
                    };
                    self.apply_snapshot(&snap, &rows, ctx.now);
                    self.fixture = Some((
                        snap,
                        nk_client::Tree {
                            rows,
                            ..nk_client::Tree::default()
                        },
                    ));
                }
                Err(e) => tracing::warn!("inject cats: sessions: {e}"),
            }
            reqs.push(Req::Wish(Wish::world(Want::Once)));
        }
        if let Some(ev) = json.get("event") {
            let name = ev.get("name").and_then(Value::as_str).unwrap_or("");
            let data = ev.get("data").cloned().unwrap_or(Value::Null);
            reqs.extend(self.event(name, &data, ctx));
        }
        if let Some(label) = json.get("talk").and_then(Value::as_str) {
            // CONVERSA with a cat by its label (e2e scripts: no station needed)
            match self.office.cats.iter().find(|v| v.label() == label) {
                Some(v) => reqs.push(Req::Action(nkw_core::Action::Talk {
                    cat: v.cat,
                    purpose: nkw_core::TalkPurpose::Session,
                })),
                None => tracing::warn!("inject cats: no cat {label:?}"),
            }
        }
        if let Some(name) = json.get("cmd").and_then(Value::as_str) {
            let args = json
                .get("args")
                .and_then(Value::as_object)
                .map(|o| {
                    o.iter()
                        .map(|(k, v)| (k.clone(), v.as_str().map_or(v.to_string(), str::to_string)))
                        .collect()
                })
                .unwrap_or_default();
            reqs.push(Req::Command {
                name: name.into(),
                args,
            });
        }
        reqs
    }

    fn dump(&self) -> Value {
        let cats: Vec<Value> = self
            .office
            .cats
            .iter()
            .map(|v| {
                json!({
                    "id": v.cat.0,
                    "session": v.sess.session_id,
                    "label": v.label(),
                    "room": v.ctx,
                    "pose": format!("{:?}", v.pose),
                    "state": v.sess.state,
                    "walking": self.walks.out(v.cat),
                })
            })
            .collect();
        let by_label: serde_json::Map<String, Value> = self
            .office
            .cats
            .iter()
            .map(|v| {
                let screen = self.screen_text(v.cat);
                (
                    v.label().to_string(),
                    json!({"id": v.cat.0, "state": v.sess.state, "room": v.ctx, "screen": screen}),
                )
            })
            .collect();
        let (queue, visible, overflow) = self.walks.counts();
        let pane = self.focus.as_ref().and_then(|f| {
            let s = self.office.get(f.cat)?.sess.session_id.clone();
            let p = self.panes.get(&s)?;
            Some(json!({"cat": f.cat.0, "cols": p.cols, "rows": p.rows}))
        });
        json!({
            "core": self.status,
            "cats": cats,
            "dialog": self.dialog_json(),
            "inbox": {"queue": queue, "visible": visible, "overflow": overflow},
            "pane": pane,
            "calls": self.calls,
            "by_label": by_label,
            "confirm": self.walks.confirm.as_ref().map(|c| c.0.clone()),
            "monitors": self.mons.values().filter(|m| m.tex.is_some() || m.raster.is_some()).count(),
        })
    }

    fn shutdown(&mut self) {
        // panes stay alive on the server; dropping the link detaches
        for p in self.panes.values_mut() {
            p.conn = None;
        }
        self.link = None;
    }
}

fn question_tag(pos: Vec3) -> Tag {
    Tag {
        id: tag_id(RECEPTIONIST, 4),
        pos,
        text: "?".into(),
        rgb: [0xe8, 0x9a, 0x6a],
        max_m: BADGE_M,
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::cli::Cli;
    use crate::world::{Opts, World};
    use nkw_core::Output;
    use std::collections::BTreeMap;

    pub fn out(w: u32, h: u32, scale: f32) -> Output {
        Output {
            width_px: w,
            height_px: h,
            scale,
        }
    }

    pub fn demo_world() -> World {
        crate::demo::world(
            out(1280, 720, 1.0),
            Opts {
                reduced_motion: false,
            },
        )
    }

    pub fn demo_cli() -> Cli {
        Cli {
            demo: true,
            ..Cli::default()
        }
    }

    /// The demo world with the fixture's rooms applied, and its cats.
    pub fn fixture_world() -> (World, Cats) {
        let mut w = demo_world();
        let mut c = Cats::new(&demo_cli());
        let ctxs = c.take_sync().expect("the fixture lays out rooms");
        w.sync_contexts(&ctxs, 0.0);
        (w, c)
    }

    pub fn with_ctx<R>(w: &World, rooms: &[RoomId], now: Time, f: impl FnOnce(&Ctx) -> R) -> R {
        let pids = BTreeMap::new();
        let ctx = Ctx {
            world: w,
            now,
            frame: 0,
            reduced_motion: false,
            demo: true,
            scale: w.out.scale as f64,
            size_px: [w.out.width_px, w.out.height_px],
            pids: &pids,
            visible_rooms: rooms,
            line: None,
        };
        f(&ctx)
    }

    fn draw(w: &World, cats: &Cats, rooms: &[RoomId], now: Time) -> DrawList {
        let mut list = crate::scene::draw_list(w, &|_| None, rooms);
        let before = list.instances.len();
        with_ctx(w, rooms, now, |ctx| cats.draw(ctx, &mut list));
        list.instances.drain(..before);
        list
    }

    fn heads(l: &DrawList) -> Vec<&Instance> {
        l.instances
            .iter()
            .filter(|i| i.mesh == MeshId::CatSkull)
            .collect()
    }

    fn room(w: &World, name: &str) -> RoomId {
        w.rooms.iter().find(|r| r.name == name).unwrap().id
    }

    fn to_acme(w: &mut World) -> RoomId {
        w.command(crate::world::Cmd::RoomNext, 0.0);
        w.tick(10.0);
        w.current_room_id()
    }

    #[test]
    fn without_a_fixture_the_desk_cats_still_sit() {
        let w = demo_world();
        let cats = Cats::new(&Cli::default());
        let all = [RoomId(0), RoomId(1), RoomId(2)];
        assert_eq!(
            heads(&draw(&w, &cats, &all, 0.0)).len(),
            2,
            "the demo has two cats"
        );
        assert!(
            heads(&draw(&w, &cats, &[RoomId(2)], 0.0)).is_empty(),
            "no cat in demo"
        );
    }

    #[test]
    fn the_fixture_sits_one_villager_per_session_at_its_context() {
        let (w, cats) = fixture_world();
        let names: Vec<_> = w.rooms.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, ["recepção", "acme", "demo", "minha sala"]);
        let acme = room(&w, "acme");
        let demo = room(&w, "demo");
        let cats_in = |r: RoomId| {
            let room = w.rooms.iter().find(|x| x.id == r).unwrap();
            room.desks.iter().filter_map(|d| d.cat).collect::<Vec<_>>()
        };
        assert_eq!(cats_in(acme), [CatId(2), CatId(3)], "acme, api-gateway");
        assert_eq!(cats_in(demo), [CatId(4), CatId(5), CatId(6)]);
        assert_eq!(cats_in(RoomId(0)), [RECEPTIONIST], "she keeps her desk");
        assert_eq!(
            heads(&draw(&w, &cats, &[acme], 0.0)).len(),
            2 + 2,
            "+ 2 kittens"
        );
        assert_eq!(heads(&draw(&w, &cats, &[demo], 0.0)).len(), 3);
        assert_eq!(heads(&draw(&w, &cats, &[RoomId(0)], 0.0)).len(), 1);
        assert!(
            cats.walks.walks.is_empty(),
            "the demo starts with everyone seated"
        );
    }

    #[test]
    fn villagers_sit_low_with_their_backs_to_you() {
        let (mut w, cats) = fixture_world();
        let acme = to_acme(&mut w);
        let l = draw(&w, &cats, &[acme], 0.0);
        let seat = w.cat_seat(acme, 0).unwrap();
        let head = heads(&l)
            .into_iter()
            .min_by(|a, b| {
                let d = |i: &&Instance| i.model.transform_point3(Vec3::ZERO).distance(seat.pos);
                d(a).total_cmp(&d(b))
            })
            .unwrap();
        let c = head.model.transform_point3(Vec3::ZERO);
        assert!(c.y + 0.14 <= layout::EYE - 0.25, "head top {}", c.y + 0.14);
        let nose = head.model.transform_vector3(Vec3::NEG_Z).normalize();
        assert!(nose.dot(l.camera.pose.forward()) > 0.7, "facing its screen");
        let (place, _) = w.monitor_place(acme, 0).unwrap();
        let cam = l.camera.pose.pos;
        assert!(cam.distance(c) < cam.distance(place.center));
        assert!(c.y < place.center.y, "below the screen's centre");
    }

    #[test]
    fn coats_follow_the_session_seed() {
        let (w, cats) = fixture_world();
        let acme = room(&w, "acme");
        let l = draw(&w, &cats, &[acme], 0.0);
        let v = cats.office.by_session(&demo::id_of("acme")).unwrap();
        let c = Coat::from_seed(v.seed);
        assert!(l.instances.iter().any(|i| i.tint == c.body));
        let (w2, cats2) = fixture_world();
        assert_eq!(l, draw(&w2, &cats2, &[acme], 0.0), "same run, same picture");
    }

    #[test]
    fn badges_name_the_villagers() {
        let (w, cats) = fixture_world();
        let acme = room(&w, "acme");
        let tags = with_ctx(&w, &[acme], 0.0, |ctx| cats.tags(ctx));
        let texts: Vec<_> = tags.iter().map(|t| t.text.as_str()).collect();
        assert_eq!(texts, ["acme", "api-gateway", "!"]);
        let demo = room(&w, "demo");
        let tags = with_ctx(&w, &[demo], 0.0, |ctx| cats.tags(ctx));
        let texts: Vec<_> = tags.iter().map(|t| t.text.as_str()).collect();
        assert_eq!(texts, ["alpha", "zZ", "bravo", "demo"]);
        assert!(tags.iter().all(|t| t.max_m == BADGE_M));
    }

    #[test]
    fn ambient_motion_only_when_visible_near_and_not_reduced() {
        let (mut w, cats) = fixture_world();
        let wishes = with_ctx(&w, &[RoomId(0)], 0.0, |ctx| cats.wishes(ctx));
        assert!(wishes.is_empty(), "the reception at rest: {wishes:?}");
        let acme = to_acme(&mut w);
        let wishes = with_ctx(&w, &[acme], 1.0, |ctx| cats.wishes(ctx));
        assert!(
            wishes.contains(&Wish::world(Want::Rate(AMBIENT_HZ))),
            "{wishes:?}"
        );
        let still = Cats::new(&Cli {
            reduced_motion: true,
            ..demo_cli()
        });
        let wishes = with_ctx(&w, &[acme], 1.0, |ctx| still.wishes(ctx));
        assert!(
            wishes.iter().all(|x| !matches!(x.want, Want::Rate(_))),
            "{wishes:?}"
        );
        let a = draw(&w, &still, &[acme], 1.0);
        let b = draw(&w, &still, &[acme], 2.3);
        assert_eq!(a, b, "a fixed pose");
        let a = draw(&w, &cats, &[acme], 1.0);
        let b = draw(&w, &cats, &[acme], 1.1);
        assert_ne!(a, b, "typing paws move");
    }

    #[test]
    fn calm_cats_wake_the_loop_only_for_their_bouts() {
        let (mut w, cats) = fixture_world();
        let demo = room(&w, "demo");
        w.command(crate::world::Cmd::RoomNext, 0.0);
        w.command(crate::world::Cmd::RoomNext, 0.1);
        w.tick(10.0);
        assert_eq!(w.current_room_id(), demo);
        let mut rated = 0;
        let mut at = 0;
        for i in 0..90 {
            let now = 100.0 + i as f64 * 0.1;
            let wishes = with_ctx(&w, &[demo], now, |ctx| cats.wishes(ctx));
            match wishes.as_slice() {
                [
                    Wish {
                        want: Want::Rate(_),
                        ..
                    },
                ] => rated += 1,
                [
                    Wish {
                        want: Want::At(t), ..
                    },
                ] => {
                    assert!(*t > now && *t <= now + catpose::BOUT_EVERY_S as f64 + 1e-6);
                    at += 1
                }
                other => panic!("{other:?}"),
            }
        }
        assert!(rated > 0 && rated < 60, "bouts only: {rated} of 90");
        assert!(at > 0);
    }

    #[test]
    fn the_glide_into_focus_fades_the_cat_it_passes() {
        use nkw_core::Action;
        for (w_px, h_px, scale) in [(1280, 720, 1.0), (1920, 1080, 1.0), (1280, 720, 1.25)] {
            let mut w = crate::demo::world(
                out(w_px, h_px, scale),
                Opts {
                    reduced_motion: false,
                },
            );
            let cat = RECEPTIONIST;
            w.action(Action::Focus(Target::CatTerm(cat)), 0.0);
            assert!(w.gliding());
            let end = w.one_to_one_target(Target::CatTerm(cat)).unwrap().pose;
            let start = w.camera;
            let seat = w.cat_seat(RoomId(0), 0).unwrap();
            let centre = seat.pos + Vec3::Y * CAT_CENTRE;
            let cats = Cats::new(&Cli::default());
            let mut closest = f32::MAX;
            for i in 1..=10 {
                let now = 0.26 * i as f64 / 10.0;
                w.tick(now);
                let list = draw(&w, &cats, &[RoomId(0)], now);
                let cam = w.camera.pos;
                let d = cam.distance(centre);
                closest = closest.min(d);
                for inst in &list.instances {
                    if d <= layout::FADE_NEAR * 0.5 {
                        assert_eq!(inst.fade, 1.0, "inside the cat at sample {i}");
                    }
                    let part = cam.distance(inst.model.transform_point3(Vec3::ZERO));
                    assert!(
                        inst.fade == 1.0 || part > crate::scene::NEAR + 0.2,
                        "a visible part at {part} m (sample {i})"
                    );
                }
            }
            assert_eq!(w.camera, end, "arrived");
            let depth = |p: Vec3| (p - seat.pos).dot(-layout::facing(seat.yaw));
            assert!(depth(start.pos) > 0.0 && depth(end.pos) < depth(start.pos));
            assert!(closest < start.pos.distance(centre));
        }
    }

    #[test]
    fn inject_fixture_sessions_events_and_commands() {
        let (w, mut cats) = fixture_world();
        with_ctx(&w, &[RoomId(0)], 0.0, |ctx| {
            cats.inject(&json!({"fixture": "busy"}), ctx)
        });
        assert_eq!(cats.office.cats.len(), 8);
        assert!(cats.take_sync().is_some());
        // a state change of a fixture session
        let gw = demo::id_of("api-gateway");
        let ev = json!({"event": {"name": "session.state", "data": {"session_id": gw, "state": "busy", "attention": 0}}});
        with_ctx(&w, &[RoomId(0)], 1.0, |ctx| cats.inject(&ev, ctx));
        assert_eq!(cats.office.by_session(&gw).unwrap().pose, CatPose::Typing);
        // a toast and a meow
        let r = with_ctx(&w, &[RoomId(0)], 1.0, |ctx| {
            cats.inject(
                &json!({"event": {"name": "toast", "data": {"text": "oi", "err": true}}}),
                ctx,
            )
        });
        assert_eq!(
            r,
            [Req::Toast {
                text: "oi".into(),
                kind: ToastKind::Error,
                ms: 6000
            }]
        );
        let r = with_ctx(&w, &[RoomId(0)], 1.0, |ctx| {
            cats.inject(
                &json!({"event": {"name": "notice", "data": {"meow": "miau"}}}),
                ctx,
            )
        });
        assert!(matches!(
            r.as_slice(),
            [Req::Cue {
                cue: nkw_objects::sound::Cue::Meow,
                at: Some(_)
            }]
        ));
        // ui.scene → a command by name
        let r = with_ctx(&w, &[RoomId(0)], 1.0, |ctx| {
            cats.inject(&json!({"event": {"name": "ui.scene", "data": {"command": "world.room_next", "args": {"n": 2}}}}), ctx)
        });
        assert_eq!(
            r,
            [Req::Command {
                name: "world.room_next".into(),
                args: BTreeMap::from([("n".to_string(), "2".to_string())]),
            }]
        );
        // sessions, replacing the office
        let s = json!({"sessions": [{"session_id": "x1", "label": "alpha", "context": "acme", "state": "busy"}]});
        with_ctx(&w, &[RoomId(0)], 0.0, |ctx| cats.inject(&s, ctx));
        assert_eq!(cats.office.cats.len(), 1);
        let r = with_ctx(&w, &[RoomId(0)], 0.0, |ctx| {
            cats.inject(&json!({"cmd": "world.talk", "args": {"n": 1}}), ctx)
        });
        assert_eq!(
            r,
            [Req::Command {
                name: "world.talk".into(),
                args: BTreeMap::from([("n".to_string(), "1".to_string())]),
            }]
        );
    }

    #[test]
    fn confirmations_outside_the_talk_bring_the_receptionist() {
        let (w, mut cats) = fixture_world();
        let ev = json!({"event": {"name": "confirm.requested", "data": {"token": "t1", "question": "Arquivar 3?"}}});
        with_ctx(&w, &[RoomId(0)], 1.0, |ctx| cats.inject(&ev, ctx));
        assert_eq!(
            cats.walks.confirm,
            Some(("t1".into(), "Arquivar 3?".into()))
        );
        let rooms = w.visible_rooms(&[]);
        with_ctx(&w, &rooms, 1.0, |ctx| cats.tick(ctx));
        assert!(cats.walks.out(RECEPTIONIST), "she walks to you");
        let tags = with_ctx(&w, &rooms, 1.0, |ctx| cats.tags(ctx));
        assert!(tags.iter().any(|t| t.text == "?"));
        assert_eq!(cats.walks.most_urgent(&cats.office), Some(RECEPTIONIST));
        let ev = json!({"event": {"name": "confirm.resolved", "data": {"token": "t1"}}});
        with_ctx(&w, &[RoomId(0)], 2.0, |ctx| cats.inject(&ev, ctx));
        assert!(cats.walks.confirm.is_none());
    }

    fn reduced_ctx<R>(w: &World, rooms: &[RoomId], now: Time, f: impl FnOnce(&Ctx) -> R) -> R {
        let pids = BTreeMap::new();
        let ctx = Ctx {
            world: w,
            now,
            frame: 0,
            reduced_motion: true,
            demo: true,
            scale: 1.0,
            size_px: [w.out.width_px, w.out.height_px],
            pids: &pids,
            visible_rooms: rooms,
            line: None,
        };
        f(&ctx)
    }

    #[test]
    fn reduced_motion_the_inbox_cuts_and_asks_for_no_rate() {
        let mut w = demo_world();
        let mut cats = Cats::new(&Cli {
            reduced_motion: true,
            ..demo_cli()
        });
        w.sync_contexts(&cats.take_sync().unwrap(), 0.0);
        let acme = to_acme(&mut w);
        reduced_ctx(&w, &[acme], 10.0, |ctx| {
            cats.inject(&json!({"fixture": "busy"}), ctx)
        });
        w.sync_contexts(&cats.take_sync().unwrap(), 10.0);
        for i in 0..20 {
            let now = 10.0 + i as f64 * 0.1;
            reduced_ctx(&w, &[acme], now, |ctx| cats.tick(ctx));
            let wishes = reduced_ctx(&w, &[acme], now, |ctx| cats.wishes(ctx));
            assert!(
                wishes.iter().all(|x| matches!(x.want, Want::At(_))),
                "only timers at {now}: {wishes:?}"
            );
        }
        assert_eq!(cats.dump()["inbox"]["visible"], 3);
        assert!(!cats.walks.moving(12.0), "they are there at once");
        assert!(cats.walks.walks.iter().all(|x| x.arrived(12.0)));
        // tails do not scroll in either
        assert!(cats.mons.values().all(|m| !m.feed.pending()));
    }

    #[test]
    fn a_tick_stays_short_once_the_fonts_are_warm() {
        let (mut w, mut cats) = fixture_world();
        cats.fonts.warm(); // what `start` does
        let acme = to_acme(&mut w);
        with_ctx(&w, &[acme], 10.0, |ctx| {
            cats.inject(&json!({"fixture": "busy"}), ctx)
        });
        w.sync_contexts(&cats.take_sync().unwrap(), 10.0);
        let mut us: Vec<u128> = (0..200)
            .map(|i| {
                let now = 10.0 + i as f64 / 30.0;
                let t = std::time::Instant::now();
                with_ctx(&w, &[acme], now, |ctx| {
                    cats.drain(ctx);
                    cats.tick(ctx);
                });
                t.elapsed().as_micros()
            })
            .collect();
        us.sort();
        let (p95, max) = (us[189], us[199]);
        // dev profile, a loaded machine: generous bounds (measured ~0.1 / ~1 ms)
        assert!(p95 < 2_000 && max < 20_000, "p95 {p95} µs, max {max} µs");
    }

    #[test]
    fn the_busy_fixture_walks_three_to_you() {
        let (mut w, mut cats) = fixture_world();
        with_ctx(&w, &[RoomId(0)], 1.0, |ctx| {
            cats.inject(&json!({"fixture": "busy"}), ctx)
        });
        w.sync_contexts(&cats.take_sync().unwrap(), 1.0);
        let rooms = w.visible_rooms(&[]);
        for i in 0..20 {
            let now = 1.0 + i as f64 * 0.25;
            with_ctx(&w, &rooms, now, |ctx| cats.tick(ctx));
        }
        let d = cats.dump();
        assert_eq!(d["inbox"]["visible"], 3, "{}", d["inbox"]);
        assert_eq!(d["inbox"]["queue"], 3);
        let wishes = with_ctx(&w, &rooms, 2.0, |ctx| cats.wishes(ctx));
        assert!(
            wishes.contains(&Wish::world(Want::Rate(inbox::WALK_HZ))),
            "{wishes:?}"
        );
        // in FOCO: a badge, no walking
        w.action(nkw_core::Action::Focus(Target::CatTerm(RECEPTIONIST)), 6.0);
        w.tick(7.0);
        let b = with_ctx(&w, &rooms, 7.0, |ctx| cats.badges(ctx));
        assert_eq!(b, [Badge::Inbox(3)]);
    }

    #[test]
    fn summon_talks_to_the_most_urgent() {
        let (mut w, mut cats) = fixture_world();
        with_ctx(&w, &[RoomId(0)], 1.0, |ctx| {
            cats.inject(&json!({"fixture": "busy"}), ctx)
        });
        w.sync_contexts(&cats.take_sync().unwrap(), 1.0);
        let rooms = w.visible_rooms(&[]);
        let r = with_ctx(&w, &rooms, 2.0, |ctx| {
            cats.command("world.summon", &BTreeMap::new(), ctx)
        });
        let gw = cats
            .office
            .by_session(&demo::id_of("api-gateway"))
            .unwrap()
            .cat;
        assert_eq!(r, Some(vec![Req::Action(nkw_core::Action::Summon(gw))]));
    }

    #[test]
    fn dump_lists_the_cats() {
        let (_, cats) = fixture_world();
        let d = cats.dump();
        assert_eq!(d["core"], "none");
        assert_eq!(d["cats"].as_array().unwrap().len(), 5);
        assert_eq!(d["cats"][1]["label"], "api-gateway");
        assert_eq!(d["cats"][1]["pose"], "Alert");
        assert_eq!(d["cats"][1]["walking"], false);
        assert_eq!(d["dialog"], Value::Null);
        assert_eq!(d["pane"], Value::Null);
        assert_eq!(d["inbox"], json!({"queue": 0, "visible": 0, "overflow": 0}));
        let live = Cats::new(&Cli::default());
        assert_eq!(live.dump()["core"], "connecting");
        assert!(live.office.cats.is_empty(), "no fixture outside --demo");
    }

    #[test]
    fn monitors_paint_the_visible_tails() {
        let (mut w, mut cats) = fixture_world();
        let acme = to_acme(&mut w);
        let rooms = [acme];
        let painted = with_ctx(&w, &rooms, 10.0, |ctx| cats.paint_monitors(ctx));
        assert!(painted);
        // one fresh monitor per tick (the tick stays short), the next one after
        assert_eq!(cats.mons.values().filter(|m| m.raster.is_some()).count(), 1);
        with_ctx(&w, &rooms, 10.05, |ctx| cats.paint_monitors(ctx));
        assert_eq!(cats.mons.values().filter(|m| m.raster.is_some()).count(), 2);
        // nothing new: nothing painted
        let again = with_ctx(&w, &rooms, 10.1, |ctx| cats.paint_monitors(ctx));
        assert!(!again);
        // a reply in the fixture shows up typed on the tail
        let gw = demo::id_of("api-gateway");
        let ctx_call = Call::Reply {
            session: gw.clone(),
            text: "1".into(),
        };
        with_ctx(&w, &rooms, 11.0, |ctx| cats.core_call(ctx_call, ctx));
        let cat = cats.office.by_session(&gw).unwrap().cat;
        let last = cats.mons[&cat].feed.lines.last().unwrap().text.clone();
        assert_eq!(last, format!("> {}", demo::OPTIONS[0]));
    }

    #[test]
    fn focus_on_a_cat_opens_its_pane_at_the_1_to_1_size() {
        let (mut w, mut cats) = fixture_world();
        let acme = to_acme(&mut w);
        let cat = CatId(2);
        w.action(nkw_core::Action::Focus(Target::CatTerm(cat)), 10.0);
        let rooms = [acme];
        with_ctx(&w, &rooms, 10.0, |ctx| {
            cats.effect(&Effect::OpenPane(cat), ctx)
        });
        let d = cats.dump();
        let (cols, rows) = (
            d["pane"]["cols"].as_u64().unwrap(),
            d["pane"]["rows"].as_u64().unwrap(),
        );
        assert!(cols >= 40 && rows >= 20, "{cols}x{rows}");
        let o = w.one_to_one_target(Target::CatTerm(cat)).unwrap();
        let f = cats.focus.as_ref().unwrap();
        assert_eq!(
            (f.raster.canvas.w as f32, f.raster.canvas.h as f32),
            (o.size.x, o.size.y)
        );
        // typing in the fixture's pane echoes
        let ev = KeyEv {
            code: 0,
            sym: 0x6f,
            name: "o".into(),
            utf8: Some("o".into()),
            mods: Default::default(),
            pressed: true,
        };
        cats.key_to_pane(cat, &ev);
        let p = &cats.panes[&demo::id_of("acme")];
        assert!(
            p.term.grid().text().contains("❯ o"),
            "{}",
            p.term.grid().text()
        );
        // leaving keeps the pane, drops the buffer
        with_ctx(&w, &rooms, 11.0, |ctx| {
            cats.effect(&Effect::KeyboardLeave, ctx)
        });
        assert!(cats.focus.is_none() && cats.panes.contains_key(&demo::id_of("acme")));
    }

    #[test]
    fn kittens_sit_on_the_desk_top() {
        let seat = nkw_core::Pose::new(Vec3::new(0.0, 0.45, 2.0), 0.0, 0.0);
        for i in 0..4 {
            let p = kitten_spot(&seat, i);
            assert_eq!(p.y, layout::DESK_TOP);
            assert!(p.z < seat.pos.z - 0.3, "towards the screen: {p}");
        }
        assert_ne!(kitten_spot(&seat, 0), kitten_spot(&seat, 1));
    }
}
