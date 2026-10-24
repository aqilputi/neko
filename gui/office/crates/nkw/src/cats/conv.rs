//! CONVERSA glue: the machine's effects and the `world.talk*` /
//! `world.choice_*` commands become [`talk`] steps, `Req`s and core calls.
//! The table itself is [`talk`]; this is the plugin's state around it.

use std::collections::BTreeMap;

use glam::Vec3;
use nkw_core::{Action, CatId, Effect, Pose, Station, TalkPurpose, Time, layout};
use serde_json::{Value, json};

use super::Cats;
use super::office::{LIBRARIAN, RECEPTIONIST};
use super::talk::{self, Call, Choice, Reveal, Row, Step, Who};
use crate::ext::{Ctx, DialogView, LineView, Req, ToastKind};

/// How far before a cat's face the camera stops (§3.7).
pub const FACE_DIST: f32 = 0.9;
/// What the tour's end tells the WM (S22: it saves `tour_done`).
pub const TOUR_DONE: &str = "world.tour_done";
/// "foi embora" stays this long before the box closes.
pub const GONE_S: Time = 1.5;

/// The conversation being held.
#[derive(Clone, Debug, PartialEq)]
pub struct Talk {
    pub cat: CatId,
    pub who: Who,
    pub row: Row,
    pub name: String,
    pub rgb: [u8; 3],
    pub choices: Vec<Choice>,
    pub reveal: Reveal,
    /// the confirmation the box is asking about (`Sim`/`Não`)
    pub pending: Option<String>,
    /// a core call is on its way (its tag)
    pub waiting: Option<u64>,
    pub line: bool,
    pub framed: bool,
    /// when the cat left (the box closes `GONE_S` later)
    pub gone_at: Option<Time>,
    /// the finished text (and its choices) was asked to compose once; reset
    /// with every new reveal (the overlay only re-rasters on compose frames)
    pub composed_done: bool,
}

/// The camera pose that frames a face at `face`, seen along `yaw` from
/// `FACE_DIST` away at eye height (looking down at it).
pub fn face_pose(face: Vec3, yaw: f32) -> Pose {
    let level = layout::stand_before(face, yaw, FACE_DIST);
    let pitch = ((face.y - level.pos.y) / FACE_DIST).atan();
    Pose::new(level.pos, yaw, pitch)
}

impl Cats {
    /// Who a cat is, for a conversation.
    pub(super) fn who(&self, cat: CatId) -> Option<(Who, String, [u8; 3])> {
        let cream = [0xe8, 0x9a, 0x6a];
        if cat == RECEPTIONIST {
            let who = match (&self.walks.confirm, self.tour) {
                (_, Some(step)) => Who::Tour(step),
                (Some((t, q)), None) => Who::Confirm(t.clone(), q.clone()),
                (None, None) => Who::Receptionist,
            };
            return Some((who, "recepcionista".into(), cream));
        }
        if cat == LIBRARIAN {
            return Some((Who::Librarian, "bibliotecária".into(), cream));
        }
        if let Some((from, to, text)) = self.walks.postman(cat) {
            return Some((Who::Postman(from, to, text), "carteiro".into(), cream));
        }
        let v = self.office.get(cat)?;
        Some((
            Who::Session(v.sess.clone()),
            v.label().to_string(),
            v.ctx_rgb,
        ))
    }

    /// Start (or restart) the conversation with `cat`.
    pub(super) fn open_talk(&mut self, cat: CatId, now: Time) {
        let Some((who, name, rgb)) = self.who(cat) else {
            return;
        };
        let (row, text, choices) = talk::opening(&who);
        let labels = choices.iter().map(Choice::label).collect();
        self.talk = Some(Talk {
            cat,
            pending: None,
            waiting: None,
            line: false,
            framed: false,
            gone_at: None,
            composed_done: false,
            reveal: Reveal::new(&text, labels, now, self.reduced_motion),
            who,
            row,
            name,
            rgb,
            choices,
        });
    }

    /// Ask the box a new question (a confirmation): Sim / Não.
    pub(super) fn ask(&mut self, token: &str, question: &str, now: Time) {
        let reduced = self.reduced_motion;
        if let Some(t) = self.talk.as_mut() {
            t.choices = vec![Choice::Yes, Choice::No];
            t.reveal = Reveal::new(question, vec!["Sim".into(), "Não".into()], now, reduced);
            t.pending = Some(token.to_string());
            t.waiting = None;
            t.composed_done = false;
        }
    }

    /// Where a cat's face is now (seated, or walking to you).
    pub(super) fn face_of(&self, ctx: &Ctx, cat: CatId) -> Option<(Vec3, f32)> {
        if let Some((p, yaw)) = self.walks.face(cat, ctx.now) {
            return Some((p, yaw));
        }
        let (room, desk) = self.desk_of(ctx, cat)?;
        let seat = ctx.world.cat_seat(room, desk)?;
        // it turns on its chair to face you: its face is over the seat
        Some((
            seat.pos + Vec3::Y * (super::body::SEAT_TOP + super::body::FACE_Y),
            seat.yaw,
        ))
    }

    /// The camera glide that frames the cat's face.
    fn frame(&mut self, ctx: &Ctx, cat: CatId) -> Vec<Req> {
        if let Some(t) = self.talk.as_mut() {
            t.framed = true;
        }
        match self.face_of(ctx, cat) {
            Some((face, yaw)) => vec![Req::GlideTo(face_pose(face, yaw))],
            None => Vec::new(),
        }
    }

    /// A machine effect for CONVERSA / FOCO on a cat (purpose `Session`).
    pub(super) fn talk_effect(&mut self, e: &Effect, ctx: &Ctx) -> Vec<Req> {
        // the vault's conversation (the librarian, purpose `VaultUnlock`) is
        // the objects plugin's: the cats only frame her face
        if ctx.world.mode().purpose() == Some(TalkPurpose::VaultUnlock) {
            return match *e {
                Effect::FrameFace(cat) => match self.face_of(ctx, cat) {
                    Some((face, yaw)) => vec![Req::GlideTo(face_pose(face, yaw))],
                    None => Vec::new(),
                },
                _ => Vec::new(),
            };
        }
        match *e {
            Effect::FrameFace(cat) => {
                if self.talk.as_ref().is_none_or(|t| t.cat != cat) {
                    self.open_talk(cat, ctx.now);
                }
                self.frame(ctx, cat)
            }
            Effect::CatTurns(cat) => {
                if self.talk.as_ref().is_none_or(|t| t.cat != cat) {
                    self.open_talk(cat, ctx.now);
                }
                // summoned: no FrameFace came, frame it here
                match self.talk.as_ref().is_some_and(|t| !t.framed) {
                    true => self.frame(ctx, cat),
                    false => Vec::new(),
                }
            }
            Effect::CatBackToWork(cat) => {
                let mut out = Vec::new();
                if let Some(t) = self.talk.take_if(|t| t.cat == cat)
                    && matches!(t.who, Who::Tour(_))
                {
                    // the tour ends however the box closes: once
                    self.tour = None;
                    out.push(Req::Command {
                        name: TOUR_DONE.into(),
                        args: BTreeMap::new(),
                    });
                }
                self.walks.seen(&self.office, cat, self.clock_ms(ctx.now));
                out
            }
            Effect::OpenLine => {
                if let Some(t) = self.talk.as_mut() {
                    t.line = true;
                }
                Vec::new()
            }
            Effect::CloseLine => {
                if let Some(t) = self.talk.as_mut() {
                    t.line = false;
                }
                Vec::new()
            }
            Effect::SendLine => {
                let Some(t) = self.talk.as_ref() else {
                    return Vec::new();
                };
                match talk::on_line(&t.who, ctx.line.unwrap_or("")) {
                    Some(call) => {
                        self.core_call(call, ctx);
                        vec![Req::Action(Action::Back)]
                    }
                    None => Vec::new(),
                }
            }
            Effect::TalkNext => self.next(ctx),
            _ => Vec::new(),
        }
    }

    /// `space`/`enter`: all the text, then the selected choice.
    fn next(&mut self, ctx: &Ctx) -> Vec<Req> {
        let Some(t) = self.talk.as_mut() else {
            return Vec::new();
        };
        if !t.reveal.done(ctx.now) {
            t.reveal.reveal_all();
            return Vec::new();
        }
        if t.choices.is_empty() {
            return vec![Req::Action(Action::Back)];
        }
        let i = t.reveal.selected();
        self.choose(i, ctx)
    }

    /// Choice `i` (0-based) of the box.
    pub(super) fn choose(&mut self, i: usize, ctx: &Ctx) -> Vec<Req> {
        let Some(t) = self.talk.as_ref() else {
            return Vec::new();
        };
        if t.waiting.is_some() {
            return Vec::new();
        }
        let Some(c) = t.choices.get(i).cloned() else {
            return Vec::new();
        };
        let (cat, who, pending) = (t.cat, t.who.clone(), t.pending.clone());
        match talk::on_choice(&who, &c, pending.as_deref()) {
            Step::Core(call) => {
                let confirm = matches!(call, Call::Confirm { .. });
                self.core_call(call, ctx);
                if confirm && cat == RECEPTIONIST {
                    self.walks.confirm = None;
                }
                vec![Req::Action(Action::Back)]
            }
            Step::Ask(call) => {
                let label = match &who {
                    Who::Session(s) => s.label.clone(),
                    _ => String::new(),
                };
                // the fixture answers at once with a confirmation; the core
                // later (`FromCore::Done`)
                if let Some(token) = self.core_ask(call.clone(), ctx) {
                    let q = talk::confirm_text("", &call, &label);
                    self.ask(&token, &q, ctx.now);
                }
                Vec::new()
            }
            Step::Line => vec![Req::Action(Action::Reply)],
            Step::Sit => self.sit(ctx, cat),
            Step::Open(href) => {
                let mut r = vec![Req::Action(Action::Back)];
                if self.source == super::Source::Fixture || ctx.demo {
                    self.record(json!({"call": "open", "href": href}));
                    r.push(Req::Toast {
                        text: format!("abrir {href}"),
                        kind: ToastKind::Info,
                        ms: 3000,
                    });
                } else {
                    r.push(Req::Spawn(vec!["xdg-open".into(), href]));
                }
                r
            }
            Step::GoTo(to) => {
                let mut r = vec![Req::Action(Action::Back)];
                let desk = self
                    .office
                    .cats
                    .iter()
                    .find(|v| v.label() == to || v.sess.session_id == to)
                    .and_then(|v| ctx.world.cat_desk(v.cat))
                    .and_then(|(room, d)| ctx.world.desk_pose(room, d));
                if let Some(p) = desk {
                    r.push(Req::GlideTo(p));
                }
                r
            }
            Step::Command(name, args) => vec![
                Req::Action(Action::Back),
                Req::Command {
                    name,
                    args: args.into_iter().collect(),
                },
            ],
            Step::Tour(Some(step)) => {
                self.tour = Some(step);
                self.retell(Who::Tour(step), ctx.now);
                Vec::new()
            }
            Step::Tour(None) | Step::End => vec![Req::Action(Action::Back)],
        }
    }

    /// The box says something new to the same listener (the tour's next
    /// balloon): text, choices and the reveal start over.
    fn retell(&mut self, who: Who, now: Time) {
        let reduced = self.reduced_motion;
        let Some(t) = self.talk.as_mut() else {
            return;
        };
        let (row, text, choices) = talk::opening(&who);
        let labels = choices.iter().map(Choice::label).collect();
        t.reveal = Reveal::new(&text, labels, now, reduced);
        t.who = who;
        t.row = row;
        t.choices = choices;
        t.composed_done = false;
    }

    /// `world.tour` (and `inject cats {"tour": true}`): the receptionist's
    /// tour from its first balloon, wherever you are — out of any overlay,
    /// to the reception, then the talk.
    pub(super) fn start_tour(&mut self, ctx: &Ctx) -> Vec<Req> {
        self.tour = Some(0);
        let mut out = Vec::new();
        if ctx.world.mode().kind().is_some() {
            out.push(Req::Action(Action::Back));
        }
        if let Some((room, _)) = ctx.world.cat_desk(RECEPTIONIST) {
            out.push(Req::Cmd(crate::world::Cmd::GotoRoom(
                crate::world::RoomRef::Id(room),
            )));
        }
        out.push(Req::Action(Action::Talk {
            cat: RECEPTIONIST,
            purpose: TalkPurpose::Session,
        }));
        out
    }

    /// "Ver o terminal": FOCO on the cat's terminal, back lands on its desk.
    fn sit(&self, ctx: &Ctx, cat: CatId) -> Vec<Req> {
        let desk = ctx
            .world
            .cat_desk(cat)
            .and_then(|(room, d)| ctx.world.desk_pose(room, d))
            .unwrap_or(ctx.world.camera);
        vec![Req::Action(Action::Sit { desk })]
    }

    /// The cat at the station's desk.
    pub(super) fn station_cat(&self, ctx: &Ctx) -> Option<CatId> {
        match ctx.world.station {
            Station::Desk { room, desk, .. } => ctx
                .world
                .rooms
                .iter()
                .find(|r| r.id == room)
                .and_then(|r| r.desks.get(desk))
                .and_then(|d| d.cat)
                .or_else(|| {
                    // the library's counter is the librarian's
                    (desk == 0 && self.librarian_seat(ctx).is_some_and(|(r, _)| r == room))
                        .then_some(LIBRARIAN)
                }),
            _ => None,
        }
    }

    /// `world.talk*`, `world.choice_*`, `world.summon`: `None` = not ours.
    pub(super) fn talk_command(
        &mut self,
        name: &str,
        args: &BTreeMap<String, String>,
        ctx: &Ctx,
    ) -> Option<Vec<Req>> {
        let _ = args;
        let talking = self.talk.is_some();
        Some(match name {
            "world.talk" => match self.near_cat(ctx) {
                Some(cat) => vec![Req::Action(Action::Talk {
                    cat,
                    purpose: TalkPurpose::Session,
                })],
                None => Vec::new(),
            },
            "world.tour" => self.start_tour(ctx),
            "world.summon" => match self.walks.most_urgent(&self.office) {
                Some(cat) => vec![Req::Action(Action::Summon(cat))],
                None => vec![Req::Toast {
                    text: "ninguém precisa de você agora".into(),
                    kind: ToastKind::Info,
                    ms: 3000,
                }],
            },
            "world.talk_next" if talking => vec![Req::Action(Action::TalkNext)],
            "world.talk_reply" if talking => vec![Req::Action(Action::Reply)],
            "world.talk_end" if talking => vec![Req::Action(Action::Back)],
            "world.talk_sit" if talking => {
                let cat = self.talk.as_ref().map(|t| t.cat)?;
                self.sit(ctx, cat)
            }
            "world.choice_down" | "world.choice_up" if talking => {
                if let Some(t) = self.talk.as_mut() {
                    match name {
                        "world.choice_down" => t.reveal.down(),
                        _ => t.reveal.up(),
                    }
                }
                Vec::new()
            }
            n if talking && n.starts_with("world.choice_") => {
                let k: usize = n["world.choice_".len()..].parse().ok()?;
                if !(1..=9).contains(&k) {
                    return None;
                }
                if let Some(t) = self.talk.as_mut() {
                    t.reveal.select(k - 1);
                    t.reveal.reveal_all();
                }
                self.choose(k - 1, ctx)
            }
            _ => return None,
        })
    }

    /// The cat to talk to: the station's, else the nearest one that came to
    /// you.
    fn near_cat(&self, ctx: &Ctx) -> Option<CatId> {
        self.walks
            .nearest_arrived(ctx.world.camera.pos, ctx.now)
            .or_else(|| self.station_cat(ctx))
    }

    /// The conversation's tick: a cat that left closes the box.
    pub(super) fn talk_tick(&mut self, ctx: &Ctx) -> Vec<Req> {
        let Some(t) = self.talk.as_mut() else {
            return Vec::new();
        };
        let gone = matches!(t.who, Who::Session(_)) && self.office.get(t.cat).is_none();
        if gone && t.gone_at.is_none() {
            let label = t.name.clone();
            let who = Who::Gone(label);
            let (row, text, _) = talk::opening(&who);
            t.reveal = Reveal::new(&text, Vec::new(), ctx.now, self.reduced_motion);
            t.choices.clear();
            t.who = who;
            t.row = row;
            t.gone_at = Some(ctx.now);
            t.composed_done = false;
        }
        if t.gone_at.is_some_and(|g| ctx.now - g >= GONE_S) {
            self.talk = None;
            return vec![Req::Action(Action::Back)];
        }
        // the frame the reveal completes composes (text and choices), even
        // if no other wish asks for it (the stale box of wave 2)
        if !t.composed_done && t.reveal.done(ctx.now) {
            t.composed_done = true;
            return vec![Req::Wish(nkw_core::Wish::compose(nkw_core::Want::Once))];
        }
        Vec::new()
    }

    /// The box, as the overlay draws it.
    pub fn dialog_view(&self) -> Option<DialogView> {
        let t = self.talk.as_ref()?;
        let done = t.reveal.done(self.now);
        Some(DialogView {
            name: t.name.clone(),
            rgb: t.rgb,
            text: t.reveal.shown(self.now).to_string(),
            more: done && t.choices.is_empty(),
            choices: if done {
                t.reveal.choices().to_vec()
            } else {
                Vec::new()
            },
            selected: t.reveal.selected(),
            line: t.line.then(|| LineView {
                text: String::new(),
                cursor: 0,
            }),
        })
    }

    /// The box as JSON (`dump()["dialog"]`).
    pub(super) fn dialog_json(&self) -> Value {
        match (self.dialog_view(), self.talk.as_ref()) {
            (Some(d), Some(t)) => json!({
                "cat": t.cat.0,
                "row": format!("{:?}", t.row),
                "name": d.name,
                "text": t.reveal.text(),
                "shown": d.text,
                "more": d.more,
                "choices": t.reveal.choices(),
                "selected": d.selected,
                "line": d.line.is_some(),
                "pending": t.pending,
            }),
            _ => Value::Null,
        }
    }

    /// The next reveal frame of the box, if it is revealing.
    pub(super) fn talk_wake(&self, now: Time) -> Option<Time> {
        let t = self.talk.as_ref()?;
        t.reveal.next_wake(now).or(t.gone_at.map(|g| g + GONE_S))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cats::demo;
    use crate::cats::tests::{fixture_world, with_ctx};
    use crate::ext::Plugin;
    use nkw_core::{Mode, RoomId};

    fn acme(w: &mut crate::world::World) -> RoomId {
        w.command(crate::world::Cmd::RoomNext, 0.0);
        w.tick(10.0);
        w.current_room_id()
    }

    /// Run the reqs that are machine actions on the world and hand the
    /// effects back to the cats (what the hub does), `n` rounds.
    fn pump(w: &mut crate::world::World, cats: &mut Cats, reqs: Vec<Req>, now: Time) -> Vec<Req> {
        let mut rest = Vec::new();
        let mut queue = reqs;
        for _ in 0..6 {
            let mut next = Vec::new();
            for r in queue {
                match r {
                    Req::Action(a) => {
                        for o in w.action(a, now) {
                            if let crate::world::Out::Effect(e) = o {
                                let rooms = w.visible_rooms(&[]);
                                let got = with_ctx(w, &rooms, now, |ctx| cats.effect(&e, ctx));
                                next.extend(got);
                            }
                        }
                    }
                    other => rest.push(other),
                }
            }
            if next.is_empty() {
                break;
            }
            queue = next;
        }
        rest
    }

    fn command(w: &mut crate::world::World, cats: &mut Cats, name: &str, now: Time) -> Vec<Req> {
        let rooms = w.visible_rooms(&[]);
        let r = with_ctx(w, &rooms, now, |ctx| {
            cats.command(name, &BTreeMap::new(), ctx)
        })
        .unwrap_or_else(|| panic!("{name} not taken"));
        pump(w, cats, r, now)
    }

    #[test]
    fn talk_at_api_gateway_opens_its_question() {
        let (mut w, mut cats) = fixture_world();
        acme(&mut w);
        w.command(crate::world::Cmd::Move(nkw_core::Dir::Right), 10.0);
        w.tick(20.0);
        let rest = command(&mut w, &mut cats, "world.talk", 20.0);
        assert_eq!(w.mode().label(), "CONVERSA");
        assert!(matches!(rest.as_slice(), [Req::GlideTo(_)]), "{rest:?}");
        cats.now = 20.0;
        let d = cats.dialog_view().unwrap();
        assert_eq!(d.name, "api-gateway");
        assert_eq!(d.text, "", "revealing");
        assert!(d.choices.is_empty());
        cats.now = 30.0;
        let d = cats.dialog_view().unwrap();
        assert_eq!(d.text, demo::QUESTION);
        assert_eq!(d.choices[0], demo::OPTIONS[0]);
        assert_eq!(cats.dialog_json()["choices"][0], demo::OPTIONS[0]);
    }

    #[test]
    fn option_one_replies_through_the_pane_and_ends() {
        let (mut w, mut cats) = fixture_world();
        acme(&mut w);
        w.command(crate::world::Cmd::Move(nkw_core::Dir::Right), 10.0);
        w.tick(20.0);
        command(&mut w, &mut cats, "world.talk", 20.0);
        command(&mut w, &mut cats, "world.choice_1", 21.0);
        assert_eq!(*w.mode(), Mode::base(nkw_core::Base::Nav), "back to NAV");
        assert!(cats.talk.is_none());
        let gw = demo::id_of("api-gateway");
        assert_eq!(
            cats.calls,
            [json!({"call": "reply", "session": gw, "reply": "1"})]
        );
    }

    #[test]
    fn next_reveals_then_picks_the_selected_choice() {
        let (mut w, mut cats) = fixture_world();
        acme(&mut w);
        // acme (busy): Mandar mensagem… · Parar · Ver o terminal
        command(&mut w, &mut cats, "world.talk", 20.0);
        command(&mut w, &mut cats, "world.talk_next", 20.1);
        assert!(cats.talk.as_ref().unwrap().reveal.done(20.1), "all of it");
        command(&mut w, &mut cats, "world.choice_down", 20.2);
        assert_eq!(cats.talk.as_ref().unwrap().reveal.selected(), 1);
        // Parar → the box asks first
        command(&mut w, &mut cats, "world.talk_next", 20.3);
        assert_eq!(w.mode().label(), "CONVERSA");
        let t = cats.talk.as_ref().unwrap();
        assert_eq!(t.reveal.text(), "Parar acme?");
        assert_eq!(t.choices, [Choice::Yes, Choice::No]);
        let token = t.pending.clone().unwrap();
        // Sim → confirm(token, yes)
        command(&mut w, &mut cats, "world.choice_1", 21.0);
        assert_eq!(w.mode().label(), "NAVEGAR");
        assert_eq!(
            cats.calls.last().unwrap(),
            &json!({"call": "confirm", "token": token, "yes": true})
        );
        assert_eq!(cats.calls[0]["call"], "run");
        assert_eq!(cats.calls[0]["name"], "session.stop");
    }

    #[test]
    fn reply_line_sends_the_text() {
        let (mut w, mut cats) = fixture_world();
        acme(&mut w);
        command(&mut w, &mut cats, "world.talk", 20.0);
        command(&mut w, &mut cats, "world.talk_reply", 20.5);
        assert!(cats.talk.as_ref().unwrap().line);
        assert!(cats.dialog_view().unwrap().line.is_some());
        // the overlay's line_send: Action::Send → Effect::SendLine with Ctx.line
        let outs = w.action(Action::Send, 21.0);
        let reqs: Vec<Req> = outs
            .into_iter()
            .filter_map(|o| match o {
                crate::world::Out::Effect(e @ Effect::SendLine) => Some(e),
                _ => None,
            })
            .flat_map(|e| {
                let pids = BTreeMap::new();
                let rooms = w.visible_rooms(&[]);
                let ctx = Ctx {
                    world: &w,
                    now: 21.0,
                    frame: 0,
                    reduced_motion: false,
                    demo: true,
                    scale: 1.0,
                    size_px: [1280, 720],
                    pids: &pids,
                    visible_rooms: &rooms,
                    line: Some("pode seguir"),
                };
                cats.effect(&e, &ctx)
            })
            .collect();
        pump(&mut w, &mut cats, reqs, 21.0);
        assert_eq!(w.mode().label(), "NAVEGAR");
        assert_eq!(cats.calls.last().unwrap()["reply"], "pode seguir");
    }

    #[test]
    fn view_terminal_sits_at_the_desk_in_focus() {
        let (mut w, mut cats) = fixture_world();
        let room = acme(&mut w);
        command(&mut w, &mut cats, "world.talk", 20.0);
        command(&mut w, &mut cats, "world.talk_sit", 20.5);
        assert_eq!(w.mode().focus(), Some(nkw_core::Target::CatTerm(CatId(2))));
        // ctrl+] goes back to the desk's station, not the face
        w.tick(30.0);
        w.command(crate::world::Cmd::Back, 30.0);
        w.tick(40.0);
        assert_eq!(w.camera, w.desk_pose(room, 0).unwrap());
    }

    #[test]
    fn a_cat_that_leaves_says_so_and_the_box_closes() {
        let (mut w, mut cats) = fixture_world();
        acme(&mut w);
        command(&mut w, &mut cats, "world.talk", 20.0);
        let (mut snap, tree) = demo::snapshot(demo::Fixture::Demo);
        snap.sessions.remove(0);
        cats.apply_snapshot(&snap, &tree.rows, 21.0);
        let rooms = w.visible_rooms(&[]);
        let r = with_ctx(&w, &rooms, 21.0, |ctx| cats.talk_tick(ctx));
        assert!(r.is_empty());
        assert_eq!(
            cats.talk.as_ref().unwrap().reveal.text(),
            "acme foi embora."
        );
        let r = with_ctx(&w, &rooms, 21.0 + GONE_S, |ctx| cats.talk_tick(ctx));
        assert_eq!(r, [Req::Action(Action::Back)]);
        assert!(cats.talk.is_none());
    }

    #[test]
    fn the_done_cat_hands_its_link_without_spawning_in_the_demo() {
        let (mut w, mut cats) = fixture_world();
        acme(&mut w);
        w.command(crate::world::Cmd::RoomNext, 10.0);
        w.tick(15.0);
        // demo's first desk is the old empty one; alpha is next to it
        w.command(crate::world::Cmd::Move(nkw_core::Dir::Right), 15.0);
        w.tick(20.0);
        // alpha: Abrir PR #1 · Mandar mais… · Arquivar
        command(&mut w, &mut cats, "world.talk", 20.0);
        assert_eq!(cats.talk.as_ref().unwrap().name, "alpha");
        let rest = command(&mut w, &mut cats, "world.choice_1", 21.0);
        assert!(rest.iter().all(|r| !matches!(r, Req::Spawn(_))));
        assert!(rest.iter().any(|r| matches!(r, Req::Toast { .. })));
        assert_eq!(cats.calls[0]["call"], "open");
    }

    #[test]
    fn commands_are_ours_only_while_talking() {
        let (mut w, mut cats) = fixture_world();
        acme(&mut w);
        let rooms = w.visible_rooms(&[]);
        for n in [
            "world.talk_next",
            "world.choice_1",
            "world.talk_end",
            "world.left",
        ] {
            let r = with_ctx(&w, &rooms, 0.0, |ctx| {
                cats.command(n, &BTreeMap::new(), ctx)
            });
            assert!(r.is_none(), "{n}");
        }
        command(&mut w, &mut cats, "world.talk", 20.0);
        command(&mut w, &mut cats, "world.talk_end", 20.5);
        assert_eq!(w.mode().label(), "NAVEGAR");
        assert!(cats.talk.is_none(), "CatBackToWork ends it");
    }

    #[test]
    fn the_frame_the_reveal_completes_asks_to_compose_once() {
        let (mut w, mut cats) = fixture_world();
        acme(&mut w);
        w.command(crate::world::Cmd::Move(nkw_core::Dir::Right), 10.0);
        w.tick(20.0);
        command(&mut w, &mut cats, "world.talk", 20.0);
        let once = Req::Wish(nkw_core::Wish::compose(nkw_core::Want::Once));
        let rooms = w.visible_rooms(&[]);
        // 25 characters at 45 c/s: done at 20.556
        let end = 20.0 + demo::QUESTION.chars().count() as f64 / talk::CPS as f64;
        let mut asked = Vec::new();
        for i in 0..40 {
            let now = 20.0 + i as f64 / 30.0;
            let r = with_ctx(&w, &rooms, now, |ctx| cats.talk_tick(ctx));
            if r.contains(&once) {
                asked.push(now);
            }
        }
        assert_eq!(asked.len(), 1, "once: {asked:?}");
        assert!(
            asked[0] >= end && asked[0] < end + 1.0 / 30.0 + 1e-9,
            "{asked:?}"
        );
        // a new question (Sim / Não) asks again when it is done
        cats.ask("t1", "Tem certeza?", 22.0);
        let r = with_ctx(&w, &rooms, 25.0, |ctx| cats.talk_tick(ctx));
        assert_eq!(r, [once]);
        let r = with_ctx(&w, &rooms, 25.1, |ctx| cats.talk_tick(ctx));
        assert!(r.is_empty());
    }

    fn tour_done(rest: &[Req]) -> usize {
        rest.iter()
            .filter(|r| matches!(r, Req::Command { name, .. } if name == TOUR_DONE))
            .count()
    }

    #[test]
    fn the_tour_walks_six_balloons_and_says_done_once() {
        let (mut w, mut cats) = fixture_world();
        acme(&mut w);
        let rest = command(&mut w, &mut cats, "world.tour", 20.0);
        assert_eq!(w.mode().label(), "CONVERSA");
        assert!(
            rest.iter().any(|r| matches!(
                r,
                Req::Cmd(crate::world::Cmd::GotoRoom(crate::world::RoomRef::Id(
                    RoomId(0)
                )))
            )),
            "to the reception first: {rest:?}"
        );
        assert!(
            rest.iter().any(|r| matches!(r, Req::GlideTo(_))),
            "her face"
        );
        assert_eq!(cats.tour, Some(0));
        let t = cats.talk.as_ref().unwrap();
        assert_eq!((t.cat, t.row), (RECEPTIONIST, Row::Tour));
        assert_eq!(t.reveal.text(), talk::TOUR[0]);
        assert_eq!(cats.dump()["tour"], 0);
        let mut done = 0;
        for step in 1..6u8 {
            let rest = command(&mut w, &mut cats, "world.choice_1", 20.0 + step as f64);
            done += tour_done(&rest);
            assert_eq!(w.mode().label(), "CONVERSA", "{step}");
            assert_eq!(cats.tour, Some(step));
            assert_eq!(
                cats.talk.as_ref().unwrap().reveal.text(),
                talk::TOUR[step as usize]
            );
        }
        assert_eq!(cats.talk.as_ref().unwrap().choices, [Choice::TourDone]);
        let rest = command(&mut w, &mut cats, "world.choice_1", 30.0);
        done += tour_done(&rest);
        assert_eq!(w.mode().label(), "NAVEGAR");
        assert_eq!((done, cats.tour), (1, None), "done once, at the end");
        assert!(cats.talk.is_none());
        // plain talks with her afterwards are hers again
        assert_eq!(cats.dump()["tour"], Value::Null);
    }

    #[test]
    fn skipping_or_leaving_the_tour_ends_it_once() {
        for how in ["world.choice_2", "world.talk_end"] {
            let (mut w, mut cats) = fixture_world();
            let mut done = tour_done(&command(&mut w, &mut cats, "world.tour", 1.0));
            done += tour_done(&command(&mut w, &mut cats, "world.choice_1", 2.0));
            assert_eq!(cats.tour, Some(1));
            done += tour_done(&command(&mut w, &mut cats, how, 3.0));
            assert_eq!(w.mode().label(), "NAVEGAR", "{how}");
            assert_eq!((done, cats.tour), (1, None), "{how}");
        }
    }

    #[test]
    fn the_tour_under_reduced_motion_shows_each_balloon_at_once() {
        let (mut w, _) = fixture_world();
        let mut cats = Cats::new(&crate::cli::Cli {
            demo: true,
            reduced_motion: true,
            ..crate::cli::Cli::default()
        });
        let ctxs = cats.take_sync().unwrap();
        w.sync_contexts(&ctxs, 0.0);
        command(&mut w, &mut cats, "world.tour", 5.0);
        cats.now = 5.0;
        assert_eq!(cats.dialog_view().unwrap().text, talk::TOUR[0]);
        command(&mut w, &mut cats, "world.choice_1", 6.0);
        cats.now = 6.0;
        assert_eq!(cats.dialog_view().unwrap().text, talk::TOUR[1]);
    }

    #[test]
    fn the_face_pose_looks_down_at_the_face() {
        let face = Vec3::new(1.0, 1.0, -2.0);
        let p = face_pose(face, 0.0);
        assert!((p.pos.y - layout::EYE).abs() < 1e-5);
        let to = (face - p.pos).normalize();
        assert!(p.forward().dot(to) > 0.999, "{}", p.forward());
        assert!((Vec3::new(p.pos.x, face.y, p.pos.z).distance(face) - FACE_DIST).abs() < 1e-4);
    }
}
