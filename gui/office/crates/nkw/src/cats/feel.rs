//! Cats reacting to you (`plano-mundo-objetos.md` §3.5): within 2.5 m and
//! inside your reading cone (±50°) a seated cat turns its head and ears to
//! you for 400 ms (15 fps), then goes back to work, and not again for 20 s;
//! within 1.3 m its tail sways while you stay and a blocked cat purrs once.
//! Nobody walks to you for proximity (that is the inbox), nothing runs in
//! FOCO, and with no cat near there is no wish at all. Pure: the plugin
//! feeds the camera and the seated cats each tick.

use std::collections::BTreeMap;

use glam::{Vec2, Vec3};
use nkw_core::{CatId, Pose, Time, Want};
use serde_json::{Value, json};

/// A cat notices you within this floor distance (camera to face).
pub const NOTICE_M: f32 = 2.5;
/// …and sways its tail within this one.
pub const TAIL_M: f32 = 1.3;
/// Your reading cone, half angle.
pub const CONE_RAD: f32 = 50.0 * std::f32::consts::PI / 180.0;
/// How long the head stays turned to you.
pub const NOTICE_S: Time = 0.4;
/// No second notice (nor purr) of the same cat before this.
pub const COOLDOWN_S: Time = 20.0;
/// The rate of the turn and the sway.
pub const FEEL_HZ: f32 = 15.0;
/// How far a head turns on its neck (rad): not all the way round.
pub const TURN_MAX: f32 = 2.6;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Work,
    Notice,
    Tail,
}

impl State {
    pub fn name(self) -> &'static str {
        match self {
            State::Work => "work",
            State::Notice => "notice",
            State::Tail => "tail",
        }
    }
}

/// A seated cat as the feel sees it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Seen {
    pub cat: CatId,
    pub face: Vec3,
    /// blocked on you (it purrs when you come close)
    pub blocked: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Entry {
    /// within `NOTICE_M` and the cone at the last step
    inside: bool,
    /// within `TAIL_M` at the last step
    near: bool,
    notice_at: Option<Time>,
    purred_at: Option<Time>,
    dist: f32,
}

/// Every cat that noticed you lately or is near.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Feel {
    cats: BTreeMap<CatId, Entry>,
    /// until this instant steps only seed (cats you start next to do not
    /// jump; the first frames may not see every room yet)
    seed_until: Option<Time>,
}

/// How long after the first step the feel only seeds.
pub const SEED_S: Time = 1.0;

fn floor(p: Vec3) -> Vec2 {
    Vec2::new(p.x, p.z)
}

/// Whether `p` is inside the reading cone of `cam` (on the floor plane).
pub fn in_cone(cam: &Pose, p: Vec3) -> bool {
    let d = floor(p) - floor(cam.pos);
    if d.length() < 1e-4 {
        return true;
    }
    d.normalize().dot(cam.heading()) >= CONE_RAD.cos()
}

impl Feel {
    /// One step at `now` with the camera at `cam` and the seated cats in
    /// reach; `active` false (FOCO, a frozen world) ends everything. Returns
    /// the faces that purr now.
    pub fn step(&mut self, cam: &Pose, seen: &[Seen], now: Time, active: bool) -> Vec<Vec3> {
        let mut purrs = Vec::new();
        if !active {
            for e in self.cats.values_mut() {
                e.inside = false;
                e.near = false;
                // a turn in progress ends now (its cooldown stays)
                if let Some(t) = e.notice_at
                    && now < t + NOTICE_S
                {
                    e.notice_at = Some(now - NOTICE_S);
                }
            }
            self.forget(now);
            return purrs;
        }
        let until = *self.seed_until.get_or_insert(now + SEED_S);
        let seed = now < until;
        for e in self.cats.values_mut() {
            e.dist = f32::MAX;
        }
        for s in seen {
            let dist = floor(s.face).distance(floor(cam.pos));
            if dist > NOTICE_M && !self.cats.contains_key(&s.cat) {
                continue;
            }
            let e = self.cats.entry(s.cat).or_default();
            let inside = dist <= NOTICE_M && in_cone(cam, s.face);
            let near = dist <= TAIL_M;
            let cooled = |at: Option<Time>| at.is_none_or(|t| now - t >= COOLDOWN_S);
            if seed {
                // already there when the world starts: no jump
                e.inside = inside;
                e.near = near;
                e.dist = dist;
                continue;
            }
            if inside && !e.inside && cooled(e.notice_at) {
                e.notice_at = Some(now);
            }
            if near && !e.near && s.blocked && cooled(e.purred_at) {
                e.purred_at = Some(now);
                purrs.push(s.face);
            }
            e.inside = inside;
            e.near = near;
            e.dist = dist;
        }
        // cats no longer seen (another room, walking, talking) are outside
        for e in self.cats.values_mut().filter(|e| e.dist == f32::MAX) {
            e.inside = false;
            e.near = false;
        }
        self.forget(now);
        purrs
    }

    /// Drop the cats that are far, still and cooled down.
    fn forget(&mut self, now: Time) {
        self.cats.retain(|_, e| {
            let warm = |t: Option<Time>| t.is_some_and(|t| now - t < COOLDOWN_S);
            e.inside || e.near || warm(e.notice_at) || warm(e.purred_at)
        });
    }

    pub fn state(&self, cat: CatId, now: Time) -> State {
        let Some(e) = self.cats.get(&cat) else {
            return State::Work;
        };
        match e.notice_at {
            Some(t) if now >= t && now < t + NOTICE_S => State::Notice,
            _ if e.near => State::Tail,
            _ => State::Work,
        }
    }

    /// When the current state ends by itself (a notice), if it does.
    pub fn until(&self, cat: CatId, now: Time) -> Option<Time> {
        match self.state(cat, now) {
            State::Notice => self.cats[&cat].notice_at.map(|t| t + NOTICE_S),
            _ => None,
        }
    }

    /// How far the head is turned to you, 0..1 (the 400 ms envelope: in
    /// over the first 30 %, held, out over the last 30 %; reduced motion:
    /// all or nothing).
    pub fn turn(&self, cat: CatId, now: Time, reduced: bool) -> f32 {
        if self.state(cat, now) != State::Notice {
            return 0.0;
        }
        if reduced {
            return 1.0;
        }
        let t0 = self.cats[&cat].notice_at.unwrap_or(now);
        let u = ((now - t0) / NOTICE_S).clamp(0.0, 1.0) as f32;
        let k = (u / 0.3).min((1.0 - u) / 0.3).clamp(0.0, 1.0);
        k * k * (3.0 - 2.0 * k)
    }

    /// The scheduling the feel needs: a burst while a head turns or a tail
    /// sways; under reduced motion only the instant a notice ends.
    pub fn want(&self, now: Time, reduced: bool) -> Option<Want> {
        let mut end: Option<Time> = None;
        let mut moving = false;
        for cat in self.cats.keys() {
            match self.state(*cat, now) {
                State::Notice => {
                    moving = true;
                    let u = self.until(*cat, now).unwrap_or(now);
                    end = Some(end.map_or(u, |e: Time| e.min(u)));
                }
                State::Tail => moving = true,
                State::Work => {}
            }
        }
        match (reduced, moving) {
            (false, true) => Some(Want::Rate(FEEL_HZ)),
            (true, _) => end.map(Want::At),
            _ => None,
        }
    }

    /// `dump()["feel"]`: the cats near or cooling down, nearest first.
    pub fn dump(&self, now: Time) -> Value {
        let mut v: Vec<(&CatId, &Entry)> = self.cats.iter().collect();
        v.sort_by(|a, b| a.1.dist.total_cmp(&b.1.dist).then(a.0.cmp(b.0)));
        Value::Array(
            v.into_iter()
                .map(|(c, _)| {
                    json!({
                        "cat": c.0,
                        "state": self.state(*c, now).name(),
                        "until": self.until(*c, now),
                    })
                })
                .collect(),
        )
    }
}

/// The head yaw (the rig's `head_rot_y`) of a cat whose root is at `pos`
/// facing `yaw` that looks at `at`, clamped to what a neck does.
pub fn head_yaw_to(pos: Vec3, yaw: f32, at: Vec3) -> f32 {
    let d = floor(at) - floor(pos);
    // into the cat's frame: its nose is −z (heading), its right +x
    let h = Pose::new(Vec3::ZERO, yaw, 0.0).heading();
    let right = Vec2::new(-h.y, h.x);
    let (fwd, side) = (d.dot(h), d.dot(right));
    // Ry(θ) turns the nose (−z) to (−sin θ, −cos θ): θ = atan2(−side, fwd)
    (-side).atan2(fwd).clamp(-TURN_MAX, TURN_MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cam_at(x: f32, z: f32, yaw: f32) -> Pose {
        Pose::new(Vec3::new(x, 1.5, z), yaw, 0.0)
    }

    /// A cat 2 m before a camera at the origin looking along −z.
    fn cat(d: f32) -> Seen {
        Seen {
            cat: CatId(3),
            face: Vec3::new(0.0, 0.97, -d),
            blocked: false,
        }
    }

    fn primed() -> Feel {
        let mut f = Feel::default();
        f.step(&cam_at(0.0, 10.0, 0.0), &[], -SEED_S, true);
        f
    }

    #[test]
    fn notice_on_entering_then_back_to_work_and_a_cooldown() {
        let mut f = primed();
        let cam = cam_at(0.0, 0.0, 0.0);
        f.step(&cam, &[cat(3.0)], 1.0, true);
        assert_eq!(f.state(CatId(3), 1.0), State::Work, "3 m: too far");
        f.step(&cam, &[cat(2.4)], 2.0, true);
        assert_eq!(f.state(CatId(3), 2.0), State::Notice);
        assert_eq!(f.until(CatId(3), 2.0), Some(2.4));
        assert_eq!(f.want(2.1, false), Some(Want::Rate(FEEL_HZ)));
        assert!(f.turn(CatId(3), 2.2, false) > 0.99, "held in the middle");
        assert!(f.turn(CatId(3), 2.01, false) < 0.2, "eases in");
        f.step(&cam, &[cat(2.4)], 2.5, true);
        assert_eq!(f.state(CatId(3), 2.5), State::Work, "back to work");
        assert_eq!(f.want(2.5, false), None, "no wish at rest near a cat");
        // out and in again within 20 s: nothing; after: again
        f.step(&cam, &[cat(3.0)], 5.0, true);
        f.step(&cam, &[cat(2.0)], 6.0, true);
        assert_eq!(f.state(CatId(3), 6.0), State::Work, "cooldown");
        f.step(&cam, &[cat(3.0)], 21.0, true);
        f.step(&cam, &[cat(2.0)], 22.5, true);
        assert_eq!(f.state(CatId(3), 22.5), State::Notice);
    }

    #[test]
    fn only_inside_the_reading_cone() {
        let mut f = primed();
        // the cat 2 m to the side: 90° off your heading
        let side = Seen {
            face: Vec3::new(2.0, 0.97, 0.0),
            ..cat(0.0)
        };
        f.step(&cam_at(0.0, 0.0, 0.0), &[side], 1.0, true);
        assert_eq!(f.state(CatId(3), 1.0), State::Work);
        // turning to it (45°): within ±50°
        let yaw = 45f32.to_radians();
        let c = Pose::new(Vec3::new(0.0, 1.5, 0.0), yaw, 0.0);
        let p = Vec3::new(c.heading().x * 2.0, 0.97, c.heading().y * 2.0);
        let turned = Pose::new(c.pos, yaw - 40f32.to_radians(), 0.0);
        assert!(
            in_cone(&turned, p) && !in_cone(&Pose::new(c.pos, yaw - 60f32.to_radians(), 0.0), p)
        );
        f.step(&c, &[Seen { face: p, ..side }], 2.0, true);
        assert_eq!(f.state(CatId(3), 2.0), State::Notice);
    }

    #[test]
    fn tail_while_you_stay_and_a_blocked_cat_purrs_once() {
        let mut f = primed();
        let cam = cam_at(0.0, 0.0, 0.0);
        let blocked = Seen {
            blocked: true,
            ..cat(1.2)
        };
        let purrs = f.step(&cam, &[blocked], 1.0, true);
        assert_eq!(purrs, [blocked.face]);
        assert_eq!(f.state(CatId(3), 1.0), State::Notice, "it turns first");
        assert_eq!(f.state(CatId(3), 1.5), State::Tail);
        assert!(f.step(&cam, &[blocked], 1.5, true).is_empty(), "once");
        assert_eq!(
            f.want(3.0, false),
            Some(Want::Rate(FEEL_HZ)),
            "while you stay"
        );
        // stepping back: the tail stops, no wish
        f.step(&cam, &[cat(2.0)], 4.0, true);
        assert_eq!(f.state(CatId(3), 4.0), State::Work);
        assert_eq!(f.want(4.0, false), None);
        // a calm cat does not purr
        let mut g = primed();
        assert!(g.step(&cam, &[cat(1.0)], 1.0, true).is_empty());
    }

    #[test]
    fn nothing_in_focus_nothing_far_nothing_at_start() {
        let mut f = Feel::default();
        let cam = cam_at(0.0, 0.0, 0.0);
        // the first second seeds: a cat you start next to does not jump,
        // even when the first frames do not see it yet
        f.step(&cam, &[], 0.0, true);
        f.step(&cam, &[cat(1.0)], 0.5, true);
        assert_eq!(f.state(CatId(3), 0.5), State::Tail);
        f.step(&cam, &[cat(1.0)], 1.5, true);
        assert_eq!(f.state(CatId(3), 1.5), State::Tail, "no jump later either");
        let mut f = primed();
        f.step(&cam, &[cat(2.0)], 1.0, true);
        assert_eq!(f.state(CatId(3), 1.1), State::Notice);
        // FOCO: everything stops at once
        f.step(&cam, &[cat(2.0)], 1.2, false);
        assert_eq!(f.state(CatId(3), 1.2), State::Work);
        assert_eq!(f.want(1.2, false), None);
        // far from every cat: no entry, no wish
        let mut g = primed();
        g.step(&cam, &[cat(6.0)], 1.0, true);
        assert_eq!(g.dump(1.0), json!([]));
        assert_eq!(g.want(1.0, false), None);
    }

    #[test]
    fn reduced_motion_turns_at_once_and_asks_for_no_rate() {
        let mut f = primed();
        let cam = cam_at(0.0, 0.0, 0.0);
        f.step(&cam, &[cat(2.0)], 1.0, true);
        assert_eq!(f.turn(CatId(3), 1.0, true), 1.0, "instant");
        assert_eq!(f.want(1.0, true), Some(Want::At(1.0 + NOTICE_S)));
        f.step(&cam, &[cat(1.0)], 3.0, true);
        assert_eq!(f.state(CatId(3), 3.0), State::Tail);
        assert_eq!(f.want(3.0, true), None, "a still tail: no wish");
    }

    #[test]
    fn dump_nearest_first() {
        let mut f = primed();
        let cam = cam_at(0.0, 0.0, 0.0);
        let far = Seen {
            cat: CatId(2),
            ..cat(2.2)
        };
        f.step(&cam, &[far, cat(1.2)], 1.0, true);
        let d = f.dump(1.0);
        assert_eq!(d[0]["cat"], 3);
        assert_eq!(d[0]["state"], "notice");
        assert_eq!(d[1]["cat"], 2);
        assert!((d[0]["until"].as_f64().unwrap() - 1.4).abs() < 1e-9);
        assert_eq!(f.dump(1.5)[0]["state"], "tail");
        assert_eq!(f.dump(1.5)[0]["until"], Value::Null);
    }

    #[test]
    fn the_head_turns_towards_you_up_to_its_neck() {
        // a cat facing −z (yaw 0), you behind it (+z): as far as it turns
        let behind = head_yaw_to(Vec3::ZERO, 0.0, Vec3::new(0.0, 0.0, 2.0));
        assert!((behind.abs() - TURN_MAX).abs() < 1e-5);
        // you ahead: no turn
        assert!(head_yaw_to(Vec3::ZERO, 0.0, Vec3::new(0.0, 0.0, -2.0)).abs() < 1e-5);
        // you to its right (+x): the nose (−z) turns to +x, θ = −π/2
        let r = head_yaw_to(Vec3::ZERO, 0.0, Vec3::new(2.0, 0.0, 0.0));
        let nose = glam::Quat::from_rotation_y(r) * Vec3::NEG_Z;
        assert!(nose.distance(Vec3::X) < 1e-4, "{nose}");
        // the same in a cat yawed 1 rad (the rig's root turns by −yaw)
        let yaw = 1.0f32;
        let at = Vec3::new(3.0, 0.0, 1.0);
        let r = head_yaw_to(Vec3::ZERO, yaw, at);
        let nose =
            (glam::Quat::from_rotation_y(-yaw) * glam::Quat::from_rotation_y(r)) * Vec3::NEG_Z;
        let want = Vec3::new(at.x, 0.0, at.z).normalize();
        assert!(nose.distance(want) < 1e-4, "{nose} vs {want}");
    }
}
