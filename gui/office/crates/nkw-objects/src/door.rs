//! Doors with state (addendum §3.3, §4.4). One source of truth,
//! `Door::openness(now)`, feeds collision (`blocks`), culling
//! (`see_through`), audio (the acoustic `Portal`) and the frame-callback
//! throttle of the windows behind it.

use glam::Vec2;
use serde::{Deserialize, Serialize};

use crate::Time;
use crate::ids::DoorId;
use crate::sound::Cue;
use nkw_core::collide::Wall;
use nkw_core::{Aabb, RoomId, Want};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum State {
    Closed,
    /// partly open, 0..1
    Ajar(f32),
    Open,
}

impl State {
    fn openness(self) -> f32 {
        match self {
            State::Closed => 0.0,
            State::Ajar(f) => f.clamp(0.0, 1.0),
            State::Open => 1.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Params {
    /// a full swing, seconds
    pub swing_s: f64,
    /// close by itself after this long with nobody near; 0 = never
    pub auto_close_s: f64,
    /// the auto-close waits while you are this close, metres
    pub hold_radius: f32,
    /// walkable from this openness on
    pub passable_min: f32,
}

impl Default for Params {
    fn default() -> Params {
        Params {
            swing_s: 0.32,
            auto_close_s: 8.0,
            hold_radius: 2.0,
            passable_min: 0.5,
        }
    }
}

/// The auto-close swing is slower than a hand's (480 vs 320 ms).
pub const AUTO_CLOSE_SLOWDOWN: f64 = 1.5;
/// The leaf's thickness, metres.
pub const LEAF_THICKNESS: f32 = 0.1;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Effect {
    /// a swing towards open started
    Opening,
    /// a swing towards closed started
    Closing,
    /// the swing reached fully open
    Opened,
    /// the swing reached the frame
    Closed,
    Cue(Cue),
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Swing {
    from: f32,
    to: f32,
    start: Time,
    dur: f64,
    auto: bool,
}

impl Swing {
    fn end(&self) -> Time {
        self.start + self.dur
    }

    /// Ease-out cubic, no bounce.
    fn at(&self, now: Time) -> f32 {
        let t = if self.dur <= 0.0 {
            1.0
        } else {
            ((now - self.start) / self.dur).clamp(0.0, 1.0) as f32
        };
        let e = 1.0 - (1.0 - t).powi(3);
        self.from + (self.to - self.from) * e
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Door {
    pub id: DoorId,
    /// the gap on its room's wall (collision)
    pub geom: nkw_core::Door,
    pub rooms: (RoomId, RoomId),
    /// centre of the gap on the floor plane
    pub pos: Vec2,
    state: State,
    swing: Option<Swing>,
    /// last time the player was near, or the door came to rest open
    last_near: Option<Time>,
}

impl Door {
    pub fn new(
        id: DoorId,
        geom: nkw_core::Door,
        rooms: (RoomId, RoomId),
        pos: Vec2,
        state: State,
    ) -> Door {
        Door {
            id,
            geom,
            rooms,
            pos,
            state,
            swing: None,
            last_near: None,
        }
    }

    /// The resting state (persisted in `world.json`); mid-swing it is the
    /// state the swing started from.
    pub fn state(&self) -> State {
        self.state
    }

    /// Whether it is swinging (at `now`).
    pub fn swinging(&self, now: Time) -> bool {
        self.swing.is_some_and(|s| now < s.end())
    }

    /// Opens a closed or ajar door, closes an open one; mid-swing it turns
    /// back from where it is.
    pub fn toggle(&mut self, now: Time, p: &Params) -> Vec<Effect> {
        let cur = self.openness(now);
        let opening = match self.swing {
            Some(s) => s.to < s.from,
            None => !matches!(self.state, State::Open),
        };
        let to = if opening { 1.0 } else { 0.0 };
        self.swing = Some(Swing {
            from: cur,
            to,
            start: now,
            dur: p.swing_s * (to - cur).abs() as f64,
            auto: false,
        });
        if opening {
            vec![Effect::Opening, Effect::Cue(Cue::DoorOpen)]
        } else {
            vec![Effect::Closing]
        }
    }

    /// Finishes swings and runs the auto-close: `auto_close_s` after the
    /// door came to rest or you were last within `hold_radius`.
    pub fn tick(&mut self, now: Time, player: Vec2, p: &Params) -> Vec<Effect> {
        let mut out = Vec::new();
        if let Some(s) = self.swing {
            if now < s.end() {
                return out;
            }
            self.swing = None;
            if s.to >= 1.0 {
                self.state = State::Open;
                self.last_near = Some(now);
                out.push(Effect::Opened);
            } else {
                self.state = State::Closed;
                out.push(Effect::Closed);
                out.push(Effect::Cue(if s.auto {
                    Cue::DoorThunk
                } else {
                    Cue::DoorClose
                }));
            }
            return out;
        }
        if self.state == State::Closed || p.auto_close_s <= 0.0 {
            return out;
        }
        if player.distance(self.pos) < p.hold_radius {
            self.last_near = Some(now);
            return out;
        }
        let since = *self.last_near.get_or_insert(now);
        if now - since >= p.auto_close_s {
            let cur = self.state.openness();
            self.swing = Some(Swing {
                from: cur,
                to: 0.0,
                start: now,
                dur: p.swing_s * AUTO_CLOSE_SLOWDOWN * cur as f64,
                auto: true,
            });
            out.push(Effect::Closing);
        }
        out
    }

    /// 0 closed … 1 open, eased during a swing.
    pub fn openness(&self, now: Time) -> f32 {
        match self.swing {
            Some(s) => s.at(now),
            None => self.state.openness(),
        }
    }

    pub fn passable(&self, now: Time, p: &Params) -> bool {
        self.openness(now) >= p.passable_min
    }

    /// The leaf across the gap, for the collision grid, while not passable.
    pub fn blocks(&self, now: Time, p: &Params) -> Option<Aabb> {
        if self.passable(now, p) {
            return None;
        }
        let (hw, ht) = (self.geom.width / 2.0, LEAF_THICKNESS / 2.0);
        let half = match self.geom.wall {
            Wall::North | Wall::South => Vec2::new(hw, ht),
            Wall::West | Wall::East => Vec2::new(ht, hw),
        };
        Some(Aabb::new(self.pos - half, self.pos + half))
    }

    /// Any gap at all: the room beyond stays in the culling set.
    pub fn see_through(&self, now: Time) -> bool {
        self.openness(now) > 0.0
    }

    /// When `tick` has something to do: the end of a swing, or the
    /// auto-close deadline. `None` at rest with nothing pending.
    pub fn next_wake(&self, now: Time, p: &Params) -> Option<Time> {
        if let Some(s) = self.swing {
            return Some(s.end());
        }
        if self.state == State::Closed || p.auto_close_s <= 0.0 {
            return None;
        }
        Some(self.last_near.unwrap_or(now) + p.auto_close_s)
    }

    /// Every frame while it swings; nothing at rest. Reduced motion is the
    /// glue's job (it cuts the swing), not this one's.
    pub fn want(&self, now: Time) -> Option<Want> {
        self.swinging(now).then_some(Want::Continuous)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FAR: Vec2 = Vec2::new(50.0, 50.0);

    fn door(state: State) -> Door {
        Door::new(
            DoorId(1),
            nkw_core::Door::new(Wall::North, 4.0),
            (RoomId(1), RoomId(2)),
            Vec2::new(4.0, 0.0),
            state,
        )
    }

    #[test]
    fn toggle_opens_then_tick_reports_opened_after_the_swing() {
        let p = Params::default();
        let mut d = door(State::Closed);
        assert_eq!(
            d.toggle(10.0, &p),
            [Effect::Opening, Effect::Cue(Cue::DoorOpen)]
        );
        assert!(d.tick(10.2, FAR, &p).is_empty(), "still swinging");
        assert_eq!(d.tick(10.0 + p.swing_s, FAR, &p), [Effect::Opened]);
        assert_eq!(d.state(), State::Open);
        assert_eq!(d.openness(11.0), 1.0);
    }

    #[test]
    fn toggle_closes_with_a_thunk_at_the_end() {
        let p = Params::default();
        let mut d = door(State::Open);
        assert_eq!(d.toggle(0.0, &p), [Effect::Closing]);
        assert_eq!(
            d.tick(0.5, FAR, &p),
            [Effect::Closed, Effect::Cue(Cue::DoorClose)]
        );
        assert_eq!(d.state(), State::Closed);
    }

    #[test]
    fn openness_is_eased_and_monotone_mid_swing() {
        let p = Params::default();
        let mut d = door(State::Closed);
        d.toggle(0.0, &p);
        let mut last = 0.0;
        for i in 1..32 {
            let t = i as f64 * 0.01;
            let o = d.openness(t);
            assert!(o > 0.0 && o < 1.0, "in (0,1) at {t}: {o}");
            assert!(o > last);
            last = o;
        }
        // ease-out: past halfway in the first third
        assert!(d.openness(0.32 / 3.0) > 0.5);
        assert_eq!(d.openness(0.0), 0.0);
        assert_eq!(d.openness(5.0), 1.0, "done even before the tick");
    }

    #[test]
    fn toggling_mid_swing_turns_back_from_where_it_is() {
        let p = Params::default();
        let mut d = door(State::Closed);
        d.toggle(0.0, &p);
        let at = d.openness(0.1);
        assert_eq!(d.toggle(0.1, &p), [Effect::Closing]);
        assert!((d.openness(0.1) - at).abs() < 1e-6, "no jump");
        assert!(d.openness(0.15) < at);
        let end = d.next_wake(0.1, &p).unwrap();
        assert!(end < 0.1 + p.swing_s, "shorter swing back");
        assert_eq!(d.tick(end, FAR, &p)[0], Effect::Closed);
    }

    #[test]
    fn passable_and_blocks_follow_openness() {
        let p = Params::default();
        let closed = door(State::Closed);
        assert!(!closed.passable(0.0, &p));
        let leaf = closed.blocks(0.0, &p).expect("closed blocks");
        assert!(leaf.contains(Vec2::new(4.0, 0.0)));
        assert!(leaf.contains(Vec2::new(4.55, 0.0)));
        assert!(!leaf.contains(Vec2::new(4.0, 0.2)), "thin");
        let open = door(State::Open);
        assert!(open.passable(0.0, &p));
        assert_eq!(open.blocks(0.0, &p), None);
        for (f, ok) in [(0.2, false), (0.49, false), (0.5, true), (0.8, true)] {
            assert_eq!(door(State::Ajar(f)).passable(0.0, &p), ok, "ajar {f}");
        }
        // a west wall door's leaf runs along z
        let mut w = door(State::Closed);
        w.geom = nkw_core::Door::new(Wall::West, 4.0);
        let leaf = w.blocks(0.0, &p).unwrap();
        assert!(leaf.contains(Vec2::new(4.0, 0.55)) && !leaf.contains(Vec2::new(4.2, 0.0)));
    }

    #[test]
    fn see_through_whenever_not_shut() {
        assert!(!door(State::Closed).see_through(0.0));
        assert!(door(State::Ajar(0.1)).see_through(0.0));
        assert!(door(State::Open).see_through(0.0));
        let p = Params::default();
        let mut d = door(State::Closed);
        d.toggle(0.0, &p);
        assert!(d.see_through(0.01), "visible as soon as it moves");
    }

    #[test]
    fn auto_close_after_8_s_only_with_the_player_away() {
        let p = Params::default();
        let mut d = door(State::Closed);
        d.toggle(0.0, &p);
        d.tick(0.32, FAR, &p);
        assert!(d.tick(8.0, FAR, &p).is_empty(), "7.68 s rested");
        assert_eq!(d.tick(8.4, FAR, &p), [Effect::Closing]);
        let end = d.next_wake(8.4, &p).unwrap();
        assert!((end - (8.4 + 0.48)).abs() < 1e-9, "slow auto swing");
        assert_eq!(
            d.tick(end, FAR, &p),
            [Effect::Closed, Effect::Cue(Cue::DoorThunk)]
        );
    }

    #[test]
    fn auto_close_is_held_while_the_player_is_near() {
        let p = Params::default();
        let mut d = door(State::Open);
        let near = Vec2::new(4.0, 1.0);
        for t in 0..30 {
            assert!(d.tick(t as f64, near, &p).is_empty(), "held at {t}");
        }
        // walks away at 29: the timer starts over from there
        assert!(d.tick(36.9, FAR, &p).is_empty());
        assert_eq!(d.tick(37.0, FAR, &p), [Effect::Closing]);
    }

    #[test]
    fn auto_close_zero_never_closes() {
        let p = Params {
            auto_close_s: 0.0,
            ..Params::default()
        };
        let mut d = door(State::Open);
        for t in [0.0, 10.0, 1000.0] {
            assert!(d.tick(t, FAR, &p).is_empty());
        }
        assert_eq!(d.next_wake(1000.0, &p), None);
        assert_eq!(d.state(), State::Open);
    }

    #[test]
    fn next_wake_table() {
        let p = Params::default();
        assert_eq!(
            door(State::Closed).next_wake(3.0, &p),
            None,
            "closed and idle"
        );
        let mut d = door(State::Open);
        assert_eq!(d.next_wake(3.0, &p), Some(11.0), "not ticked yet: from now");
        d.tick(3.0, FAR, &p);
        assert_eq!(d.next_wake(5.0, &p), Some(11.0));
        d.toggle(6.0, &p);
        assert_eq!(d.next_wake(6.0, &p), Some(6.0 + p.swing_s));
    }

    #[test]
    fn want_is_continuous_only_during_the_swing() {
        let p = Params::default();
        let mut d = door(State::Closed);
        assert_eq!(d.want(0.0), None);
        d.toggle(1.0, &p);
        assert_eq!(d.want(1.1), Some(Want::Continuous));
        assert_eq!(d.want(1.0 + p.swing_s), None);
        d.tick(2.0, FAR, &p);
        assert_eq!(d.want(2.0), None);
    }

    #[test]
    fn ajar_toggles_to_open() {
        let p = Params::default();
        let mut d = door(State::Ajar(0.3));
        assert_eq!(d.toggle(0.0, &p)[0], Effect::Opening);
        assert!((d.next_wake(0.0, &p).unwrap() - 0.7 * p.swing_s).abs() < 1e-6);
        assert_eq!(d.tick(1.0, FAR, &p), [Effect::Opened]);
    }
}
