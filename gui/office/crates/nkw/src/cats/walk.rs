//! The walkers (§6.6, §6.7): the inbox queue (`nk_cats::inbox`: order,
//! stagger, dismiss) decides who comes; each comes along an A* path
//! ([`inbox::route`]) to its stop before your station, and goes back to
//! its desk once seen. The postman brings letters from the reception; the
//! receptionist brings confirmations from outside the conversation. In FOCO
//! nobody walks: on the way out they are already there.

use std::collections::{BTreeMap, HashMap};

use glam::{Vec2, Vec3};
use nk_cats::inbox::{self as q, Kind, Queue, Source};
use nkw_core::{CatId, Pose, Time};

use super::inbox::{self, Path, floor, route, stop};
use super::office::{Office, RECEPTIONIST, fnv1a};
use crate::ext::Ctx;

/// The postmen's cat ids (never a session's).
pub const POSTMAN_BASE: u32 = 1_000_000;
/// The postman's and the receptionist's coat seeds.
pub const POSTMAN_SEED: &str = "carteiro";
/// A walker's face above its floor point (sitting before you).
pub const FLOOR_FACE: f32 = super::body::FACE_Y;
/// Who is talking to you counts as near within this distance.
pub const NEAR_TALK_M: f32 = 2.5;

/// One cat on its way.
#[derive(Clone, Debug, PartialEq)]
pub struct Walk {
    /// the queue's key (`att:<id>`, `mail:<id>`) or `recep`
    pub key: String,
    pub cat: CatId,
    pub seed: u64,
    pub postman: bool,
    pub path: Path,
    pub t0: Time,
    /// going back to its desk (then it is gone)
    pub home: bool,
    /// the yaw it ends facing (you, or its screen)
    pub end_yaw: f32,
    /// where it hops up to on arrival (a desk top beside your screen)
    pub perch: Option<Vec3>,
    /// metres walked on the paths before this one (the stride carries on
    /// across a re-route)
    pub before_m: f32,
    /// the spot it was sent to (the path may end short of it, at the
    /// nearest free floor)
    pub goal: Vec2,
}

impl Walk {
    /// Where it is at `now`, which way it faces, and whether it moves.
    pub fn at(&self, now: Time) -> (Vec2, f32, bool) {
        let d = ((now - self.t0).max(0.0) * inbox::SPEED as f64) as f32;
        let (p, dir) = self.path.at(d);
        if d >= self.path.len {
            (p, self.end_yaw, false)
        } else {
            (p, inbox::yaw_of(dir), true)
        }
    }

    /// How far along its path it is at `now` (the gait's clock: pausing
    /// keeps the legs where they were).
    pub fn dist(&self, now: Time) -> f32 {
        (((now - self.t0).max(0.0) * inbox::SPEED as f64) as f32).min(self.path.len)
    }

    /// Its legs at `now`: the stride from the distance walked; standing
    /// (arrived, or a cut) is the neutral stance.
    pub fn gait(&self, now: Time) -> nkw_gl::rig::Gait {
        match self.at(now).2 {
            true => nkw_gl::rig::gait_at(self.before_m + self.dist(now), inbox::SPEED),
            false => nkw_gl::rig::Gait::NEUTRAL,
        }
    }

    pub fn arrived(&self, now: Time) -> bool {
        now >= self.t0 + self.path.secs()
    }

    /// Where it is in 3D (on its perch once arrived), its yaw, whether it
    /// moves.
    pub fn at3(&self, now: Time) -> (Vec3, f32, bool) {
        let (p, yaw, moving) = self.at(now);
        match (self.perch, moving) {
            (Some(q), false) => (q, yaw, false),
            _ => (Vec3::new(p.x, 0.0, p.y), yaw, moving),
        }
    }
}

/// A spot a walker can end at: the floor point it walks to, the yaw it
/// ends with, and the perch it hops up to (a desk top).
pub type Spot = (Vec2, f32, Option<Vec3>);

/// The desk-top points a walker may perch on at desk `d` of `room`: the
/// four corners of its top, inset so a sitting cat fits (front first), each
/// with the floor point it jumps up from.
fn desk_corners(w: &crate::world::World, room: nkw_core::RoomId, d: usize) -> Vec<(Vec3, Vec3)> {
    use nkw_core::layout::{
        CHAIR_FROM_WALL, DESK_DEPTH, DESK_FROM_WALL, DESK_TOP, desk_width, facing,
    };
    let (Some(seat), Some((_, logical))) = (w.cat_seat(room, d), w.monitor_place(room, d)) else {
        return Vec::new();
    };
    let f = facing(seat.yaw);
    let f = Vec3::new(f.x, 0.0, f.z).normalize_or_zero();
    let right = Vec3::new(-f.z, 0.0, f.x);
    let dw = desk_width(logical.x * nkw_core::onetoone::M_PER_PX);
    let centre = seat.pos + f * (CHAIR_FROM_WALL - DESK_FROM_WALL - DESK_DEPTH / 2.0);
    let mut v = Vec::new();
    for depth in [-1.0f32, 1.0] {
        for side in [-1.0f32, 1.0] {
            let x = right * side * (dw / 2.0 - 0.17);
            let c = centre + x + f * depth * (DESK_DEPTH / 2.0 - 0.2);
            let below = centre + x - f * (DESK_DEPTH / 2.0 + 0.25);
            v.push((Vec3::new(c.x, DESK_TOP, c.z), below));
        }
    }
    v
}

/// Your screen at a desk station, in NDC from `you`.
fn station_screen(ctx: &Ctx, you: &Pose) -> Option<inbox::NdcRect> {
    let w = ctx.world;
    let nkw_core::Station::Desk { room, desk, .. } = w.station else {
        return None;
    };
    let (place, logical) = w.monitor_place(room, desk)?;
    let aspect = ctx.size_px[0] as f32 / ctx.size_px[1].max(1) as f32;
    Some(inbox::screen_rect(
        you,
        aspect,
        &place,
        logical * nkw_core::onetoone::M_PER_PX,
    ))
}

/// The spots walkers go to, best first: at a desk station, the desk-top
/// corners of the room that frame a sitting cat comfortably from your
/// station ([`inbox::framing`]: head in the middle band, ears inside, off
/// your screen, never nearer than 1.6 m), nearest first (a cat sitting on
/// the floor before an eye-height camera is below the frame); then the
/// floor semicircle before you (§6.6), which is all there is in ANDAR or
/// in the hall.
pub fn spots(ctx: &Ctx, you: &Pose, kittens: &dyn Fn(CatId) -> usize) -> Vec<Spot> {
    let w = ctx.world;
    let aspect = ctx.size_px[0] as f32 / ctx.size_px[1].max(1) as f32;
    let mut out: Vec<(inbox::Framing, Spot)> = Vec::new();
    if let nkw_core::Station::Desk { room, .. } = w.station
        && !w.mode().walking()
        && let Some(r) = w.rooms.iter().find(|r| r.id == room)
    {
        let screen = station_screen(ctx, you);
        for d in 0..r.desks.len() {
            let seat = w.cat_seat(room, d);
            let n = r.desks[d].cat.map_or(0, kittens);
            for (perch, below) in desk_corners(w, room, d) {
                // a cat's kittens keep their corners
                let taken = seat.is_some_and(|s| {
                    (0..n.min(4)).any(|i| super::kitten_spot(&s, i).distance(perch) < 0.3)
                });
                if taken {
                    continue;
                }
                let fr = inbox::framing(you, aspect, perch, screen);
                if !fr.ok() {
                    continue;
                }
                let look = floor(you.pos) - floor(perch);
                out.push((fr, (floor(below), inbox::yaw_of(look), Some(perch))));
            }
        }
    }
    out.sort_by(|a, b| a.0.dist.total_cmp(&b.0.dist));
    // nearest first, each clear of the ones before it in the frame (cats
    // side by side, never one in front of the other)
    let mut taken: Vec<inbox::Framing> = Vec::new();
    let mut v: Vec<Spot> = Vec::new();
    for (fr, s) in out {
        if taken
            .iter()
            .any(|t| (t.head_x - fr.head_x).abs() < inbox::SPOT_GAP)
        {
            continue;
        }
        taken.push(fr);
        v.push(s);
    }
    for i in 0..inbox::NEAR_MAX + 1 {
        let (p, yaw) = stop(you, i);
        v.push((p, yaw, None));
    }
    v
}

#[derive(Clone, Debug, Default)]
pub struct Walks {
    pub q: Queue,
    /// session → (at ms, sent)
    pub mail: HashMap<String, (i64, bool)>,
    /// letters: recipient session → (from, text)
    pub notes: HashMap<String, (String, String)>,
    pub walks: Vec<Walk>,
    /// a confirmation the receptionist brings (token, question)
    pub confirm: Option<(String, String)>,
    /// FOCO was on: the next plan puts everyone at their stops
    held: bool,
    /// the queue's clock: ms = now·1000 + offset
    pub offset_ms: i64,
    postmen: BTreeMap<String, CatId>,
    /// the spots, for the station they were computed at
    #[allow(clippy::type_complexity)]
    spots: Option<((nkw_core::Station, Pose, [u32; 2], usize), Vec<Spot>)>,
    /// the queue was stepped at this ms (again on change or at its wake)
    stepped: Option<i64>,
}

/// What the queue reads from the office.
pub fn sources(o: &Office) -> Vec<Source> {
    o.cats
        .iter()
        .map(|v| Source {
            session_id: v.sess.session_id.clone(),
            label: v.label().to_string(),
            attention: v.sess.attention,
            archived: v.sess.archived,
            last_at_ms: nk_cats::model::parse_rfc3339(&v.sess.last_at),
            needs: v.sess.needs.clone(),
        })
        .collect()
}

impl Walks {
    /// Bring the queue in line with the office at `now_ms`.
    pub fn step(&mut self, o: &Office, now_ms: i64) {
        self.q = q::step(&self.q, &sources(o), &self.mail, now_ms, 0.0, &|_| {
            Some(0.0)
        });
        self.stepped = Some(now_ms);
    }

    /// `step` when due: the queue changes by itself only at its wake.
    pub fn step_due(&mut self, o: &Office, now_ms: i64) {
        let due = match self.stepped {
            None => true,
            Some(at) => q::next_wake(&self.q, at).is_some_and(|t| t <= now_ms),
        };
        if due {
            self.step(o, now_ms);
        }
    }

    /// Everything that wants you now is "already seen" (the demo starts
    /// with the cats at their desks).
    pub fn prime(&mut self, o: &Office, now_ms: i64) {
        self.step(o, now_ms);
        let ids: Vec<String> = self
            .q
            .walkers
            .iter()
            .map(|w| w.session_id.clone())
            .collect();
        for id in ids {
            self.q = q::dismiss_session(&self.q, &id);
        }
    }

    /// The postman of a letter's queue key.
    fn postman_id(&mut self, key: &str) -> CatId {
        let n = self.postmen.len() as u32;
        *self
            .postmen
            .entry(key.to_string())
            .or_insert(CatId(POSTMAN_BASE + n))
    }

    /// The letter a postman carries: (from, to label, text).
    pub fn postman(&self, cat: CatId) -> Option<(String, String, String)> {
        let key = self.postmen.iter().find(|(_, c)| **c == cat)?.0;
        let w = self.q.walkers.iter().find(|w| &w.key == key)?;
        let (from, text) = self
            .notes
            .get(&w.session_id)
            .cloned()
            .unwrap_or_else(|| (String::new(), w.text.clone()));
        Some((from, w.label.clone(), text))
    }

    /// The walkers near you: the queue's most urgent that already left.
    pub fn near(&self, now_ms: i64) -> Vec<&q::Walker> {
        self.q
            .ordered()
            .into_iter()
            .filter(|w| w.start_at.is_some_and(|s| s <= now_ms))
            .take(inbox::NEAR_MAX)
            .collect()
    }

    /// Plan the walks for this frame.
    pub fn plan(&mut self, ctx: &Ctx, o: &Office, now_ms: i64) {
        let w = ctx.world;
        if w.mode().focus().is_some() {
            self.held = true;
            return;
        }
        // leaving FOCO they are already there; under reduced motion nobody
        // walks at all (a cut, like the camera's)
        let instant = std::mem::take(&mut self.held) || ctx.reduced_motion;
        let now = ctx.now;
        let you = w.station_pose(&w.station).unwrap_or(w.camera);
        // who should be out, in slot order
        let mut want: Vec<(String, CatId, bool, Vec2)> = Vec::new();
        let near: Vec<q::Walker> = self.near(now_ms).into_iter().cloned().collect();
        for (slot, wk) in near.iter().enumerate() {
            let (cat, postman) = match wk.kind {
                Kind::Mail => (self.postman_id(&wk.key), true),
                _ => match o.by_session(&wk.session_id) {
                    Some(v) => (v.cat, false),
                    None => continue,
                },
            };
            want.push((wk.key.clone(), cat, postman, stop(&you, slot).0));
        }
        if self.confirm.is_some() {
            let slot = want.len().min(inbox::NEAR_MAX);
            want.push(("recep".into(), RECEPTIONIST, false, stop(&you, slot).0));
        }
        let key = (
            w.station,
            you,
            ctx.size_px,
            w.rooms.iter().map(|r| r.desks.len()).sum::<usize>(),
        );
        if self.spots.as_ref().is_none_or(|(k, _)| *k != key) {
            let kittens = |c: CatId| o.get(c).map_or(0, |v| v.kittens().0.len());
            self.spots = Some((key, spots(ctx, &you, &kittens)));
        }
        let spots = self
            .spots
            .as_ref()
            .map(|(_, s)| s.clone())
            .unwrap_or_default();
        for (slot, (key, cat, postman, _)) in want.iter().enumerate() {
            let (to, end_yaw, perch) = spots[slot.min(spots.len() - 1)];
            let to = &to;
            let cur = self.walks.iter().position(|x| &x.key == key);
            if let Some(i) = cur {
                let x = &self.walks[i];
                if !x.home && x.perch == perch && x.goal.distance(*to) < 0.3 {
                    continue;
                }
            }
            let from = match cur {
                Some(i) => self.walks[i].at(now).0,
                None => start_of(ctx, *cat, *postman),
            };
            let before_m = cur.map_or(0.0, |i| self.walks[i].before_m + self.walks[i].dist(now));
            let path = route(w, from, *to, 0.7).unwrap_or_else(|| Path::new(vec![from, *to]));
            let t0 = if instant {
                now - path.secs() - 1.0
            } else {
                now
            };
            let walk = Walk {
                key: key.clone(),
                cat: *cat,
                seed: if *postman {
                    fnv1a(POSTMAN_SEED)
                } else {
                    o.get(*cat).map_or(fnv1a("recepção"), |v| v.seed)
                },
                postman: *postman,
                path,
                t0,
                home: false,
                end_yaw,
                perch,
                before_m,
                goal: *to,
            };
            match cur {
                Some(i) => self.walks[i] = walk,
                None => self.walks.push(walk),
            }
        }
        // the ones no longer wanted go back
        for x in self.walks.iter_mut() {
            if x.home || want.iter().any(|(k, ..)| *k == x.key) {
                continue;
            }
            let from = x.at(now).0;
            x.before_m += x.dist(now);
            let to = home_of(ctx, x.cat, x.postman);
            x.path = route(w, from, to.0, 0.0).unwrap_or_else(|| Path::new(vec![from, to.0]));
            x.t0 = if ctx.reduced_motion {
                now - x.path.secs() - 1.0
            } else {
                now
            };
            x.home = true;
            x.end_yaw = to.1;
            x.perch = None;
            x.goal = to.0;
        }
        self.walks.retain(|x| !(x.home && x.arrived(now)));
    }

    /// The cat was talked to: its walkers go back (and a letter is read).
    pub fn seen(&mut self, o: &Office, cat: CatId, now_ms: i64) {
        let _ = now_ms;
        if let Some(v) = o.get(cat) {
            self.q = q::dismiss_session(&self.q, &v.sess.session_id);
        }
        if let Some((key, _)) = self.postmen.iter().find(|(_, c)| **c == cat)
            && let Some(w) = self.q.walkers.iter().find(|w| &w.key == key)
        {
            self.mail.remove(&w.session_id);
            self.notes.remove(&w.session_id);
        }
    }

    /// `world.summon`: a confirmation first, then the queue's order.
    pub fn most_urgent(&self, o: &Office) -> Option<CatId> {
        if self.confirm.is_some() {
            return Some(RECEPTIONIST);
        }
        let w = self.q.ordered().into_iter().next()?;
        match w.kind {
            Kind::Mail => self.postmen.get(&w.key).copied(),
            _ => o.by_session(&w.session_id).map(|v| v.cat),
        }
    }

    /// A walker's face and the camera yaw that looks at it.
    pub fn face(&self, cat: CatId, now: Time) -> Option<(Vec3, f32)> {
        let x = self.walks.iter().find(|x| x.cat == cat && !x.home)?;
        let (p, yaw, _) = x.at3(now);
        Some((p + Vec3::Y * FLOOR_FACE, yaw + std::f32::consts::PI))
    }

    /// The nearest cat sitting before you.
    pub fn nearest_arrived(&self, cam: Vec3, now: Time) -> Option<CatId> {
        self.walks
            .iter()
            .filter(|x| !x.home && x.arrived(now))
            .map(|x| (x.cat, floor(x.at3(now).0).distance(floor(cam))))
            .filter(|(_, d)| *d <= NEAR_TALK_M)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(c, _)| c)
    }

    /// Whether a cat is out of its chair.
    pub fn out(&self, cat: CatId) -> bool {
        self.walks.iter().any(|x| x.cat == cat)
    }

    pub fn moving(&self, now: Time) -> bool {
        self.walks.iter().any(|x| !x.arrived(now))
    }

    /// The next instant the queue changes by itself (a stagger start).
    pub fn next_wake_ms(&self, now_ms: i64) -> Option<i64> {
        q::next_wake(&self.q, now_ms)
    }

    /// (queue, near you, overflow) for the dump and the badge.
    pub fn counts(&self) -> (usize, usize, usize) {
        let out = self.walks.iter().filter(|x| !x.home).count();
        let n = self.q.walkers.len();
        (n, out, n.saturating_sub(inbox::NEAR_MAX))
    }
}

/// Where a walker leaves from: its seat, or the reception (postman).
fn start_of(ctx: &Ctx, cat: CatId, postman: bool) -> Vec2 {
    home_of(ctx, cat, postman).0
}

/// Where a walker goes back to, and the yaw it ends with.
fn home_of(ctx: &Ctx, cat: CatId, postman: bool) -> (Vec2, f32) {
    let w = ctx.world;
    if !postman
        && let Some((room, desk)) = w.cat_desk(cat)
        && let Some(seat) = w.cat_seat(room, desk)
    {
        return (floor(seat.pos), seat.yaw);
    }
    // the reception's entry (the postman's door)
    let r0 = w.rooms.first().map(|r| r.id);
    let p = r0
        .and_then(|r| w.room_slot(r))
        .map(|s| nkw_core::layout::room_entry(&s))
        .unwrap_or_default();
    (floor(p.pos), p.yaw)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cats::demo::{self, Fixture};
    use crate::cats::tests::{fixture_world, with_ctx};

    fn office(f: Fixture) -> Office {
        let (snap, tree) = demo::snapshot(f);
        let mut o = Office::new();
        o.apply(&snap, &tree.rows, 0.0);
        o
    }

    #[test]
    fn primed_demo_keeps_them_at_their_desks_until_something_new() {
        let o = office(Fixture::Demo);
        let mut w = Walks::default();
        w.prime(&o, 1000);
        assert!(w.q.walkers.is_empty(), "already seen");
        w.step(&o, 2000);
        assert!(w.q.walkers.is_empty());
        // busy: fresh activity on the two that need you, and a letter
        let mut b = office(Fixture::Busy);
        let (snap, _) = demo::snapshot(Fixture::Busy);
        w.mail = snap
            .mail
            .iter()
            .map(|(k, m)| (k.clone(), (nk_cats::model::parse_rfc3339(&m.at), m.sent)))
            .collect();
        let now = nk_cats::model::parse_rfc3339(&snap.mail[&demo::id_of("bravo")].at) + 1000;
        w.step(&b, now);
        let kinds: Vec<Kind> = w.q.ordered().iter().map(|x| x.kind).collect();
        assert_eq!(kinds, [Kind::Blocked, Kind::Returned, Kind::Mail]);
        // they leave single file
        let starts: Vec<i64> = w.q.ordered().iter().map(|x| x.start_at.unwrap()).collect();
        assert!(starts.windows(2).all(|s| s[1] >= s[0] + q::STAGGER));
        assert_eq!(w.near(now).len(), 1);
        assert_eq!(w.near(now + 2 * q::STAGGER).len(), 3);
        // the queue's most urgent: api-gateway
        assert_eq!(
            w.most_urgent(&b),
            b.by_session(&demo::id_of("api-gateway")).map(|v| v.cat)
        );
        b.cats.clear();
        w.confirm = Some(("t".into(), "?".into()));
        assert_eq!(
            w.most_urgent(&b),
            Some(RECEPTIONIST),
            "a confirmation first"
        );
    }

    fn busy_walks() -> (crate::world::World, Office, Walks, i64) {
        let (w, _) = fixture_world();
        let o = office(Fixture::Busy);
        let (snap, _) = demo::snapshot(Fixture::Busy);
        let t = nk_cats::model::parse_rfc3339(&snap.mail[&demo::id_of("bravo")].at) + 1000;
        let mut walks = Walks {
            mail: snap
                .mail
                .iter()
                .map(|(k, m)| (k.clone(), (nk_cats::model::parse_rfc3339(&m.at), m.sent)))
                .collect(),
            offset_ms: t,
            ..Walks::default()
        };
        walks.step(&o, t);
        (w, o, walks, t)
    }

    /// The world with the busy fixture's rooms (8 sessions).
    fn busy_world(o: &Office) -> crate::world::World {
        let (mut w, _) = fixture_world();
        w.sync_contexts(&o.contexts(), 0.0);
        w
    }

    #[test]
    fn three_walk_to_their_stops_and_sit_facing_you() {
        let (_, o, mut walks, t) = busy_walks();
        let w = busy_world(&o);
        let rooms = w.visible_rooms(&[]);
        let you = w.station_pose(&w.station).unwrap();
        for i in 0..40 {
            let now = i as f64 * 0.25;
            with_ctx(&w, &rooms, now, |ctx| {
                walks.plan(ctx, &o, t + (now * 1000.0) as i64)
            });
        }
        let (queue, out, over) = walks.counts();
        assert_eq!((queue, out, over), (3, 3, 0));
        assert!(walks.walks.iter().any(|x| x.postman), "the postman");
        // late enough, everyone has arrived at a different stop
        let end = 60.0;
        let mut stops = Vec::new();
        for x in &walks.walks {
            assert!(x.arrived(end), "{} arrives", x.key);
            let (p, yaw, moving) = x.at(end);
            assert!(!moving);
            let d = p.distance(floor(you.pos));
            assert!(
                (d - inbox::STOP_M).abs() < 0.35,
                "{}: {d} m from you",
                x.key
            );
            let face = Pose::new(Vec3::ZERO, yaw, 0.0).heading();
            let to_you = (floor(you.pos) - p).normalize();
            assert!(face.dot(to_you) > 0.9, "{} faces you", x.key);
            stops.push(p);
        }
        assert!(stops[0].distance(stops[1]) > 0.5 && stops[1].distance(stops[2]) > 0.5);
        // they are out of their chairs; a cat is talked to: it goes back
        let gw = o.by_session(&demo::id_of("api-gateway")).unwrap().cat;
        assert!(walks.out(gw));
        assert!(walks.nearest_arrived(you.pos, end).is_some());
        walks.seen(&o, gw, t);
        walks.step(&o, t + 61_000);
        with_ctx(&w, &rooms, end, |ctx| walks.plan(ctx, &o, t + 61_000));
        let x = walks.walks.iter().find(|x| x.cat == gw).unwrap();
        assert!(x.home, "back to its desk");
        with_ctx(&w, &rooms, end + 60.0, |ctx| {
            walks.plan(ctx, &o, t + 121_000)
        });
        assert!(!walks.out(gw), "home and seated");
    }

    #[test]
    fn at_a_desk_they_hop_onto_desk_tops_in_view_beside_your_screen() {
        let (_, o, mut walks, t) = busy_walks();
        let mut w = busy_world(&o);
        // acme's first desk: acme-2 and api-gateway's desks are its neighbours
        w.command(crate::world::Cmd::RoomNext, 0.0);
        w.tick(10.0);
        let you = w.station_pose(&w.station).unwrap();
        let rooms = w.visible_rooms(&[]);
        let sp = with_ctx(&w, &rooms, 10.0, |ctx| spots(ctx, &you, &|_| 0));
        let perched: Vec<&Spot> = sp.iter().filter(|s| s.2.is_some()).collect();
        assert!(!perched.is_empty(), "some desk top in view");
        for (_, _, p) in &perched {
            let p = p.unwrap();
            assert!((p.y - nkw_core::layout::DESK_TOP).abs() < 1e-6);
        }
        // the floor semicircle stays as the fallback, after the perches
        assert!(sp.iter().skip(perched.len()).all(|s| s.2.is_none()));
        for i in 0..40 {
            let now = 10.0 + i as f64 * 0.25;
            with_ctx(&w, &rooms, now, |ctx| {
                walks.plan(ctx, &o, t + (now * 1000.0) as i64)
            });
        }
        let first = walks
            .walks
            .iter()
            .find(|x| x.perch.is_some())
            .expect("a perched walker");
        let (p, yaw, moving) = first.at3(100.0);
        assert!(!moving && p == first.perch.unwrap());
        let face = Pose::new(Vec3::ZERO, yaw, 0.0).heading();
        assert!(
            face.dot((floor(you.pos) - floor(p)).normalize()) > 0.99,
            "facing you"
        );
        // it reads comfortably from your station
        let fr = inbox::framing(&you, 16.0 / 9.0, p, None);
        assert!(fr.ok(), "{fr:?}");
    }

    /// The busy world at `px`, the camera at acme's first desk.
    fn busy_world_at(o: &Office, px: (u32, u32)) -> crate::world::World {
        let mut w = crate::demo::world(
            crate::cats::tests::out(px.0, px.1, 1.0),
            crate::world::Opts {
                reduced_motion: false,
            },
        );
        w.sync_contexts(&o.contexts(), 0.0);
        w.command(crate::world::Cmd::RoomNext, 0.0);
        w.tick(10.0);
        w
    }

    #[test]
    fn the_inbox_frames_every_walker_comfortably_from_the_station() {
        for px in [(1280, 720), (1920, 1080)] {
            let (_, o, mut walks, t) = busy_walks();
            let w = busy_world_at(&o, px);
            let you = w.station_pose(&w.station).unwrap();
            let rooms = w.visible_rooms(&[]);
            let screen = with_ctx(&w, &rooms, 10.0, |ctx| station_screen(ctx, &you));
            assert!(screen.is_some());
            for i in 0..40 {
                let now = 10.0 + i as f64 * 0.25;
                with_ctx(&w, &rooms, now, |ctx| {
                    walks.plan(ctx, &o, t + (now * 1000.0) as i64)
                });
            }
            assert_eq!(walks.counts().1, 3, "three near you");
            let aspect = px.0 as f32 / px.1 as f32;
            // acme's room at 16:9 frames two desk tops clear of each other
            // (api-gateway's); the third comes to the floor before you
            let perched = walks.walks.iter().filter(|x| x.perch.is_some()).count();
            assert_eq!(perched, 2, "{px:?}");
            let mut xs = Vec::new();
            for x in walks.walks.iter().filter(|x| x.perch.is_some()) {
                let (p, _, moving) = x.at3(200.0);
                assert!(!moving);
                let fr = inbox::framing(&you, aspect, p, screen);
                xs.push(fr.head_x);
                assert!(
                    (inbox::HEAD_Y_MIN..=inbox::HEAD_Y_MAX).contains(&fr.head_y),
                    "{}: head at {} of the frame",
                    x.key,
                    fr.head_y
                );
                assert!(fr.ears_inside, "{}", x.key);
                assert!(fr.screen_overlap <= 0.10, "{}: {fr:?}", x.key);
                assert!(fr.dist >= 1.6, "{}: {} m", x.key, fr.dist);
            }
            assert!(
                (xs[0] - xs[1]).abs() >= inbox::SPOT_GAP,
                "side by side: {xs:?}"
            );
        }
    }

    #[test]
    fn the_stride_follows_the_distance_walked() {
        let walk = Walk {
            key: "k".into(),
            cat: CatId(3),
            seed: 1,
            postman: false,
            path: Path::new(vec![Vec2::ZERO, Vec2::new(0.0, -2.8)]),
            t0: 10.0,
            home: false,
            end_yaw: 0.0,
            perch: None,
            before_m: 0.0,
            goal: Vec2::new(0.0, -2.8),
        };
        // 0.25 m in: half a stride (STRIDE_M = 0.5)
        let t = 10.0 + 0.25 / inbox::SPEED as f64;
        let g = walk.gait(t);
        assert!((g.phase - std::f32::consts::PI).abs() < 1e-3, "{g:?}");
        assert_eq!(g, nkw_gl::rig::gait_at(0.25, inbox::SPEED));
        // arrived (2 s at 1.4 m/s): standing, the neutral stance
        assert_eq!(walk.gait(12.5), nkw_gl::rig::Gait::NEUTRAL);
        // a re-route carries the stride on: the same legs at the same place
        let on = Walk {
            path: Path::new(vec![Vec2::new(0.0, -0.25), Vec2::new(1.0, -0.25)]),
            t0: t,
            before_m: walk.dist(t),
            ..walk.clone()
        };
        assert!((on.gait(t).phase - g.phase).abs() < 1e-4);
        // the same distance, the same legs, whatever the clock says
        let late = Walk {
            t0: 50.0,
            ..walk.clone()
        };
        assert_eq!(late.gait(50.0 + 0.25 / inbox::SPEED as f64), g);
    }

    #[test]
    fn framing_rules_table() {
        let cam = Pose::new(Vec3::new(0.0, nkw_core::layout::EYE, 0.0), 0.0, -0.09);
        let fwd = cam.heading();
        let ahead =
            |d: f32, x: f32, y: f32| Vec3::new(fwd.x * d - fwd.y * x, y, fwd.y * d + fwd.x * x);
        let rows: [(Vec3, bool, &str); 6] = [
            (ahead(2.2, 0.8, 0.75), true, "a desk top beside"),
            (ahead(1.2, 0.5, 0.75), false, "too near"),
            (ahead(1.8, 0.6, 0.0), false, "on the floor: below the band"),
            (ahead(2.4, 0.0, 0.75), false, "over your screen"),
            (ahead(2.2, 3.0, 0.75), false, "out to the side"),
            (ahead(-2.0, 0.0, 0.75), false, "behind"),
        ];
        let screen = Some((Vec2::new(0.0, 0.05), Vec2::new(0.25, 0.3)));
        for (base, ok, what) in rows {
            let fr = inbox::framing(&cam, 16.0 / 9.0, base, screen);
            assert_eq!(fr.ok(), ok, "{what}: {fr:?}");
        }
        // the ear tips are above the rig's ears
        let mut v = Vec::new();
        super::super::body::cat_instances(
            &nkw_gl::rig::CatJoints::default(),
            &super::super::body::Coat::from_seed(1),
            glam::Mat4::IDENTITY,
            None,
            0.0,
            &mut v,
        );
        let ear = v
            .iter()
            .filter(|i| i.mesh == nkw_gl::draw::MeshId::CatEarOuter)
            .map(|i| i.model.transform_point3(Vec3::ZERO).y)
            .fold(0.0f32, f32::max);
        assert!(inbox::EAR_TIP_Y >= ear + 0.08, "ears at {ear}");
    }

    #[test]
    fn in_focus_nobody_walks_and_on_exit_they_are_there() {
        let (_, o, mut walks, t) = busy_walks();
        let mut w = busy_world(&o);
        let rooms = w.visible_rooms(&[]);
        w.action(
            nkw_core::Action::Focus(nkw_core::Target::CatTerm(RECEPTIONIST)),
            0.0,
        );
        w.tick(5.0);
        with_ctx(&w, &rooms, 5.0, |ctx| walks.plan(ctx, &o, t + 5000));
        assert!(walks.walks.is_empty(), "no walking in FOCO");
        w.action(nkw_core::Action::Back, 6.0);
        with_ctx(&w, &rooms, 6.0, |ctx| walks.plan(ctx, &o, t + 6000));
        assert_eq!(walks.walks.len(), 3);
        assert!(walks.walks.iter().all(|x| x.arrived(6.0)), "already there");
        assert!(!walks.moving(6.0));
    }

    #[test]
    fn letters_and_the_receptionist() {
        let (_, o, mut walks, t) = busy_walks();
        let w = busy_world(&o);
        walks
            .notes
            .insert(demo::id_of("bravo"), ("acme".into(), "oi".into()));
        let rooms = w.visible_rooms(&[]);
        with_ctx(&w, &rooms, 2.0, |ctx| walks.plan(ctx, &o, t + 2000));
        let p = walks.walks.iter().find(|x| x.postman).unwrap().cat;
        assert!(p.0 >= POSTMAN_BASE);
        assert_eq!(
            walks.postman(p),
            Some(("acme".into(), "bravo".into(), "oi".into()))
        );
        walks.seen(&o, p, t);
        assert!(walks.mail.is_empty(), "read");
        walks.confirm = Some(("tok".into(), "Arquivar?".into()));
        with_ctx(&w, &rooms, 3.0, |ctx| walks.plan(ctx, &o, t + 3000));
        assert!(walks.out(RECEPTIONIST));
        let (face, _) = walks.face(RECEPTIONIST, 3.0).unwrap();
        assert!((face.y - FLOOR_FACE).abs() < 1e-6);
    }
}
