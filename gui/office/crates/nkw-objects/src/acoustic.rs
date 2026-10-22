//! Proximity audio (addendum §4.1–4.3): a pure `gains()` from the listener,
//! the sources and the room graph, and the per-source smoothing that sits
//! outside it.
//!
//! Same room: Web Audio's "inverse distance clamped" plus a constant-power
//! pan. Another room: the sound leaves through the doors on the shortest
//! room path, so the distance runs through them, the pan points at the
//! nearest door, and each door adds `lerp(closed_db, 0, openness)` dB and a
//! low-pass at `lerp(closed_lowpass_hz, open_lowpass_hz, openness)`.

use std::collections::VecDeque;
use std::f32::consts::{FRAC_1_SQRT_2, FRAC_PI_4};

use glam::Vec2;
use serde::{Deserialize, Serialize};

use crate::ids::{DoorId, SourceId};
use nkw_core::RoomId;

/// The model's knobs (`[audio]` in `world.toml`).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Params {
    pub ref_distance: f32,
    pub max_distance: f32,
    pub rolloff: f32,
    pub closed_db: f32,
    pub closed_lowpass_hz: f32,
    pub open_lowpass_hz: f32,
    pub max_hops: u8,
}

impl Default for Params {
    fn default() -> Params {
        Params {
            ref_distance: 1.0,
            max_distance: 12.0,
            rolloff: 1.0,
            closed_db: -24.0,
            closed_lowpass_hz: 800.0,
            open_lowpass_hz: 20_000.0,
            max_hops: 2,
        }
    }
}

/// You: on the floor plane (`x`, `z` as `Vec2`), yaw as in `nkw_core::Pose`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Listener {
    pub pos: Vec2,
    pub yaw: f32,
    pub room: RoomId,
}

/// Something that sounds, anchored at a place in a room.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Source {
    pub id: SourceId,
    pub pos: Vec2,
    pub room: RoomId,
    /// 0..1; the world only ever lowers it
    pub own_volume: f32,
}

/// A door as sound sees it: 0 closed … 1 open.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Portal {
    pub id: DoorId,
    pub rooms: (RoomId, RoomId),
    pub pos: Vec2,
    pub openness: f32,
}

/// Rooms joined by portals; shortest paths by BFS (there are few rooms).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RoomGraph {
    rooms: Vec<RoomId>,
    portals: Vec<Portal>,
}

impl RoomGraph {
    pub fn new(rooms: &[RoomId], portals: &[Portal]) -> RoomGraph {
        RoomGraph {
            rooms: rooms.to_vec(),
            portals: portals.to_vec(),
        }
    }

    /// The doors from `a` to `b`, in order (empty when `a == b`); `None`
    /// when no path joins them.
    pub fn path(&self, a: RoomId, b: RoomId) -> Option<Vec<DoorId>> {
        self.path_idx(a, b)
            .map(|p| p.iter().map(|&i| self.portals[i].id).collect())
    }

    /// The portal with that id.
    pub fn portal(&self, id: DoorId) -> Option<&Portal> {
        self.portals.iter().find(|p| p.id == id)
    }

    fn known(&self, r: RoomId) -> bool {
        self.rooms.contains(&r)
    }

    /// Portal indices on the shortest path; ties break by portal order, so
    /// the answer is deterministic.
    fn path_idx(&self, a: RoomId, b: RoomId) -> Option<Vec<usize>> {
        if a == b {
            return Some(Vec::new());
        }
        if !self.known(a) || !self.known(b) {
            return None;
        }
        // room -> (previous room, portal index)
        let mut prev: Vec<(RoomId, RoomId, usize)> = Vec::new();
        let mut seen = vec![a];
        let mut todo = VecDeque::from([a]);
        while let Some(r) = todo.pop_front() {
            for (i, p) in self.portals.iter().enumerate() {
                let next = if p.rooms.0 == r {
                    p.rooms.1
                } else if p.rooms.1 == r {
                    p.rooms.0
                } else {
                    continue;
                };
                if seen.contains(&next) || !self.known(next) {
                    continue;
                }
                seen.push(next);
                prev.push((next, r, i));
                if next == b {
                    let mut out = Vec::new();
                    let mut cur = b;
                    while cur != a {
                        let &(_, from, i) = prev.iter().find(|(n, _, _)| *n == cur).unwrap();
                        out.push(i);
                        cur = from;
                    }
                    out.reverse();
                    return Some(out);
                }
                todo.push_back(next);
            }
        }
        None
    }
}

/// What the adapter applies to one source.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SourceGain {
    pub id: SourceId,
    /// 0..own_volume
    pub gain: f32,
    /// -1 left … 1 right
    pub pan: f32,
    /// `None` = no filter (same room, or muted anyway)
    pub lowpass_hz: Option<f32>,
}

/// "Inverse distance clamped": 1 at `ref_distance`, decreasing, flat from
/// `max_distance` on (a plateau, so leaving the range has no step).
pub fn gain_dist(p: &Params, d: f32) -> f32 {
    let r = p.ref_distance.max(1e-3);
    let dc = d.max(r).min(p.max_distance.max(r));
    r / (r + p.rolloff.max(0.0) * (dc - r))
}

pub fn db_to_linear(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

fn linear_to_db(g: f32) -> f32 {
    20.0 * g.max(1e-5).log10()
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Pan of `at` as heard from `l`: the x of the vector in the listener's
/// axes over its length (`Pose::right()` on the floor is `(cos, sin)`).
fn pan_towards(l: &Listener, at: Vec2) -> f32 {
    let v = at - l.pos;
    let right = Vec2::new(l.yaw.cos(), l.yaw.sin());
    (v.dot(right) / (v.length() + 1e-4)).clamp(-1.0, 1.0)
}

/// Gain, pan and low-pass for every source. Pure: same input, same output.
pub fn gains(p: &Params, l: &Listener, sources: &[Source], g: &RoomGraph) -> Vec<SourceGain> {
    sources.iter().map(|s| gain_of(p, l, s, g)).collect()
}

fn gain_of(p: &Params, l: &Listener, s: &Source, g: &RoomGraph) -> SourceGain {
    let vol = s.own_volume.clamp(0.0, 1.0);
    let mute = SourceGain {
        id: s.id,
        gain: 0.0,
        pan: 0.0,
        lowpass_hz: None,
    };
    let Some(path) = g.path_idx(s.room, l.room) else {
        return mute;
    };
    if path.len() > p.max_hops as usize {
        return mute;
    }
    if path.is_empty() {
        return SourceGain {
            id: s.id,
            gain: gain_dist(p, s.pos.distance(l.pos)) * vol,
            pan: pan_towards(l, s.pos),
            lowpass_hz: None,
        };
    }
    let doors: Vec<&Portal> = path.iter().map(|&i| &g.portals[i]).collect();
    let mut d = s.pos.distance(doors[0].pos);
    for w in doors.windows(2) {
        d += w[0].pos.distance(w[1].pos);
    }
    let last = doors[doors.len() - 1];
    d += last.pos.distance(l.pos);
    let mut extra_db = 0.0;
    let mut lowpass = f32::INFINITY;
    for door in &doors {
        let o = door.openness.clamp(0.0, 1.0);
        extra_db += lerp(p.closed_db, 0.0, o);
        lowpass = lowpass.min(lerp(p.closed_lowpass_hz, p.open_lowpass_hz, o));
    }
    SourceGain {
        id: s.id,
        gain: gain_dist(p, d) * db_to_linear(extra_db) * vol,
        pan: pan_towards(l, last.pos),
        lowpass_hz: Some(lowpass),
    }
}

/// One step of exponential smoothing: `cur += (target − cur)(1 − e^(−dt/τ))`.
/// Never overshoots; a long enough `dt` lands exactly on the target.
pub fn smooth_towards(cur: f32, target: f32, dt_s: f32, tau_s: f32) -> f32 {
    if tau_s <= 0.0 {
        return target;
    }
    let k = 1.0 - (-dt_s.max(0.0) / tau_s).exp();
    if k >= 1.0 - 1e-6 {
        return target;
    }
    cur + (target - cur) * k
}

/// Constant-power pan: `l² + r² = gain²`; mono puts `gain/√2` on both.
pub fn pan_to_stereo(gain: f32, pan: f32, mono: bool) -> (f32, f32) {
    if mono {
        let g = gain * FRAC_1_SQRT_2;
        return (g, g);
    }
    let a = (pan.clamp(-1.0, 1.0) + 1.0) * FRAC_PI_4;
    (gain * a.cos(), gain * a.sin())
}

/// Time constants of [`Smoothed`], seconds.
pub const TAU_GAIN: f32 = 0.12;
pub const TAU_PAN: f32 = 0.08;
pub const TAU_LOWPASS: f32 = 0.15;
/// [`Smoothed::step`] reports a change past these.
pub const STEP_DB: f32 = 0.5;
pub const STEP_PAN: f32 = 0.02;
/// ...or a low-pass move past this ratio (about a sixth of an octave).
pub const STEP_LOWPASS_RATIO: f32 = 1.12;
/// The low-pass value meaning "no filter", for sources without one.
pub const NO_LOWPASS_HZ: f32 = 20_000.0;

/// One source's smoothed values. `step` says when they moved enough since
/// the last time it said so: that is what throttles calls to PipeWire.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Smoothed {
    pub gain: f32,
    pub pan: f32,
    pub lowpass_hz: f32,
    sent: (f32, f32, f32),
}

impl Default for Smoothed {
    fn default() -> Smoothed {
        Smoothed::new(0.0, 0.0, NO_LOWPASS_HZ)
    }
}

impl Smoothed {
    /// Starting at these values, taken as already applied.
    pub fn new(gain: f32, pan: f32, lowpass_hz: f32) -> Smoothed {
        Smoothed {
            gain,
            pan,
            lowpass_hz,
            sent: (gain, pan, lowpass_hz),
        }
    }

    /// Moves towards `target`; `true` when the values drifted from the last
    /// reported ones by > 0.5 dB, > 0.02 pan or a sixth of an octave, or
    /// when they just settled on the target (so the final value is exact).
    pub fn step(&mut self, target: &SourceGain, dt_s: f32) -> bool {
        let lp_target = target.lowpass_hz.unwrap_or(NO_LOWPASS_HZ);
        self.gain = settle(
            smooth_towards(self.gain, target.gain, dt_s, TAU_GAIN),
            target.gain,
            1e-4,
        );
        self.pan = settle(
            smooth_towards(self.pan, target.pan, dt_s, TAU_PAN),
            target.pan,
            1e-4,
        );
        self.lowpass_hz = settle(
            smooth_towards(self.lowpass_hz, lp_target, dt_s, TAU_LOWPASS),
            lp_target,
            1.0,
        );
        let (g0, p0, l0) = self.sent;
        let moved = (linear_to_db(self.gain) - linear_to_db(g0)).abs() > STEP_DB
            || (self.pan - p0).abs() > STEP_PAN
            || (self.lowpass_hz.max(1.0) / l0.max(1.0)).max(l0.max(1.0) / self.lowpass_hz.max(1.0))
                > STEP_LOWPASS_RATIO;
        let settled = self.gain == target.gain
            && self.pan == target.pan
            && self.lowpass_hz == lp_target
            && (g0, p0, l0) != (self.gain, self.pan, self.lowpass_hz);
        if moved || settled {
            self.sent = (self.gain, self.pan, self.lowpass_hz);
            true
        } else {
            false
        }
    }
}

fn settle(v: f32, target: f32, eps: f32) -> f32 {
    if (v - target).abs() <= eps { target } else { v }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::FRAC_PI_2;

    const A: RoomId = RoomId(1);
    const B: RoomId = RoomId(2);
    const C: RoomId = RoomId(3);
    const D: RoomId = RoomId(4);
    const E: RoomId = RoomId(5);

    fn near(a: f32, b: f32, eps: f32) -> bool {
        (a - b).abs() <= eps
    }

    fn lis(pos: Vec2, room: RoomId) -> Listener {
        Listener {
            pos,
            yaw: 0.0,
            room,
        }
    }

    fn src(pos: Vec2, room: RoomId) -> Source {
        Source {
            id: SourceId(1),
            pos,
            room,
            own_volume: 1.0,
        }
    }

    fn portal(id: u32, a: RoomId, b: RoomId, pos: Vec2, openness: f32) -> Portal {
        Portal {
            id: DoorId(id),
            rooms: (a, b),
            pos,
            openness,
        }
    }

    /// A line of rooms A–B–C–D, doors at x = 10, 20, 30; E is unconnected.
    fn line(openness: f32) -> RoomGraph {
        RoomGraph::new(
            &[A, B, C, D, E],
            &[
                portal(1, A, B, Vec2::new(10.0, 0.0), openness),
                portal(2, B, C, Vec2::new(20.0, 0.0), openness),
                portal(3, C, D, Vec2::new(30.0, 0.0), openness),
            ],
        )
    }

    fn one(p: &Params, l: &Listener, s: Source, g: &RoomGraph) -> SourceGain {
        gains(p, l, &[s], g)[0]
    }

    #[test]
    fn gain_dist_is_one_at_ref_decreasing_and_flat_past_max() {
        let p = Params::default();
        assert_eq!(gain_dist(&p, 1.0), 1.0);
        assert_eq!(gain_dist(&p, 0.2), 1.0, "no boost inside ref");
        let mut last = 1.0;
        for i in 1..=22 {
            let g = gain_dist(&p, 1.0 + i as f32 * 0.5);
            assert!(g < last, "decreasing at step {i}");
            last = g;
        }
        let at_max = gain_dist(&p, 12.0);
        assert!(near(at_max, 1.0 / 12.0, 1e-6));
        for d in [12.0, 13.0, 40.0, 1e6] {
            assert_eq!(gain_dist(&p, d), at_max, "plateau at {d}");
        }
    }

    #[test]
    fn db_to_linear_table() {
        for (db, lin) in [
            (0.0, 1.0),
            (-6.0206, 0.5),
            (-20.0, 0.1),
            (-24.0, 0.063_095_73),
            (-48.0, 0.003_981_07),
        ] {
            assert!(near(db_to_linear(db), lin, 1e-5), "{db} dB");
        }
    }

    #[test]
    fn pan_to_stereo_keeps_power_for_nine_pans() {
        for i in 0..9 {
            let pan = -1.0 + i as f32 * 0.25;
            for g in [0.0, 0.3, 1.0] {
                let (l, r) = pan_to_stereo(g, pan, false);
                assert!(near(l * l + r * r, g * g, 1e-5), "pan {pan} gain {g}");
            }
        }
        let (l, r) = pan_to_stereo(1.0, -1.0, false);
        assert!(near(l, 1.0, 1e-6) && near(r, 0.0, 1e-6), "hard left");
        let (l, r) = pan_to_stereo(1.0, 1.0, false);
        assert!(near(l, 0.0, 1e-6) && near(r, 1.0, 1e-6), "hard right");
        let (l, r) = pan_to_stereo(1.0, 5.0, false);
        assert!(near(r, 1.0, 1e-6) && near(l, 0.0, 1e-6), "clamped");
    }

    #[test]
    fn mono_preserves_power_and_ignores_pan() {
        for pan in [-1.0, 0.0, 0.7] {
            let (l, r) = pan_to_stereo(0.8, pan, true);
            assert_eq!(l, r);
            assert!(near(l * l + r * r, 0.64, 1e-6));
        }
    }

    #[test]
    fn same_room_distance_pan_and_no_lowpass() {
        let p = Params::default();
        let g = line(1.0);
        let l = lis(Vec2::ZERO, A);
        // ahead (-z), 3 m
        let s = one(&p, &l, src(Vec2::new(0.0, -3.0), A), &g);
        assert!(near(s.gain, gain_dist(&p, 3.0), 1e-6));
        assert!(near(s.pan, 0.0, 1e-4));
        assert_eq!(s.lowpass_hz, None);
        // to the right (+x) and left
        assert!(one(&p, &l, src(Vec2::new(2.0, 0.0), A), &g).pan > 0.99);
        assert!(one(&p, &l, src(Vec2::new(-2.0, 0.0), A), &g).pan < -0.99);
        // turned right 90°: what was ahead is now on the left
        let turned = Listener {
            yaw: FRAC_PI_2,
            ..l
        };
        assert!(one(&p, &turned, src(Vec2::new(0.0, -3.0), A), &g).pan < -0.99);
        // own volume scales; never above it
        let quiet = Source {
            own_volume: 0.5,
            ..src(Vec2::new(0.0, -0.5), A)
        };
        assert_eq!(one(&p, &l, quiet, &g).gain, 0.5);
        // on top of the listener: finite, centred
        let s = one(&p, &l, src(Vec2::ZERO, A), &g);
        assert_eq!(s.gain, 1.0);
        assert!(s.pan.is_finite());
    }

    #[test]
    fn neighbour_room_goes_through_the_open_door() {
        let p = Params::default();
        let g = line(1.0);
        // listener in A at (5, 0) facing -z; source in B at (14, 3)
        let l = lis(Vec2::new(5.0, 0.0), A);
        let s = one(&p, &l, src(Vec2::new(14.0, 3.0), B), &g);
        let d = 5.0 + 5.0; // |src-door| = 5, |door-lis| = 5
        assert!(near(s.gain, gain_dist(&p, d), 1e-6));
        assert!(s.pan > 0.99, "pans towards the door at +x: {}", s.pan);
        assert_eq!(s.lowpass_hz, Some(20_000.0));
    }

    #[test]
    fn closed_door_is_minus_24_db_and_800_hz() {
        let p = Params::default();
        let l = lis(Vec2::new(5.0, 0.0), A);
        let s = src(Vec2::new(14.0, 3.0), B);
        let open = one(&p, &l, s, &line(1.0));
        let closed = one(&p, &l, s, &line(0.0));
        assert!(near(
            linear_to_db(closed.gain) - linear_to_db(open.gain),
            -24.0,
            1e-3
        ));
        assert_eq!(closed.lowpass_hz, Some(800.0));
    }

    #[test]
    fn ajar_half_is_minus_12_db_and_10_4_khz() {
        let p = Params::default();
        let l = lis(Vec2::new(5.0, 0.0), A);
        let s = src(Vec2::new(14.0, 3.0), B);
        let open = one(&p, &l, s, &line(1.0));
        let ajar = one(&p, &l, s, &line(0.5));
        assert!(near(
            linear_to_db(ajar.gain) - linear_to_db(open.gain),
            -12.0,
            1e-3
        ));
        assert!(near(ajar.lowpass_hz.unwrap(), 10_400.0, 1e-2));
    }

    #[test]
    fn two_closed_doors_sum_db_take_min_lowpass_and_path_distance() {
        let p = Params::default();
        let l = lis(Vec2::new(5.0, 0.0), A);
        let s = src(Vec2::new(25.0, 0.0), C);
        let closed = one(&p, &l, s, &line(0.0));
        let d = 5.0 + 10.0 + 5.0;
        assert!(near(
            closed.gain,
            gain_dist(&p, d) * db_to_linear(-48.0),
            1e-7
        ));
        assert_eq!(closed.lowpass_hz, Some(800.0));
        // one closed, one open: the closed one's filter dominates
        let mixed = RoomGraph::new(
            &[A, B, C],
            &[
                portal(1, A, B, Vec2::new(10.0, 0.0), 1.0),
                portal(2, B, C, Vec2::new(20.0, 0.0), 0.0),
            ],
        );
        let m = one(&p, &l, s, &mixed);
        assert_eq!(m.lowpass_hz, Some(800.0));
        assert!(near(m.gain, gain_dist(&p, d) * db_to_linear(-24.0), 1e-7));
    }

    #[test]
    fn three_hops_and_unconnected_rooms_are_silent() {
        let p = Params::default();
        let l = lis(Vec2::new(5.0, 0.0), A);
        let far = one(&p, &l, src(Vec2::new(35.0, 0.0), D), &line(1.0));
        assert_eq!(far.gain, 0.0);
        let cut = one(&p, &l, src(Vec2::new(5.0, 1.0), E), &line(1.0));
        assert_eq!(cut.gain, 0.0);
        assert_eq!(cut.lowpass_hz, None);
        // and an unknown room
        let ghost = one(&p, &l, src(Vec2::ZERO, RoomId(99)), &line(1.0));
        assert_eq!(ghost.gain, 0.0);
    }

    #[test]
    fn room_graph_paths() {
        let g = line(1.0);
        assert_eq!(g.path(A, A), Some(vec![]));
        assert_eq!(g.path(A, B), Some(vec![DoorId(1)]));
        assert_eq!(g.path(A, D), Some(vec![DoorId(1), DoorId(2), DoorId(3)]));
        assert_eq!(g.path(D, A), Some(vec![DoorId(3), DoorId(2), DoorId(1)]));
        assert_eq!(g.path(A, E), None);
        assert_eq!(g.path(A, RoomId(99)), None);
        assert_eq!(g.portal(DoorId(2)).unwrap().rooms, (B, C));
        // a shortcut wins over the long way round
        let ring = RoomGraph::new(
            &[A, B, C],
            &[
                portal(1, A, B, Vec2::ZERO, 1.0),
                portal(2, B, C, Vec2::ZERO, 1.0),
                portal(3, C, A, Vec2::ZERO, 1.0),
            ],
        );
        assert_eq!(ring.path(A, C), Some(vec![DoorId(3)]));
    }

    #[test]
    fn gains_keeps_order_and_ids() {
        let p = Params::default();
        let l = lis(Vec2::ZERO, A);
        let srcs: Vec<Source> = (0..4)
            .map(|i| Source {
                id: SourceId(i),
                ..src(Vec2::new(i as f32, 0.0), A)
            })
            .collect();
        let out = gains(&p, &l, &srcs, &line(1.0));
        assert_eq!(out.iter().map(|s| s.id.0).collect::<Vec<_>>(), [0, 1, 2, 3]);
        assert_eq!(gains(&p, &l, &srcs, &line(1.0)), out, "pure");
    }

    #[test]
    fn smooth_towards_converges_without_overshoot() {
        for (cur, target) in [(0.0f32, 1.0f32), (1.0, 0.0), (-0.5, 0.25)] {
            let mut v = cur;
            let mut last_gap = (target - v).abs();
            for _ in 0..200 {
                v = smooth_towards(v, target, 0.01, 0.12);
                let gap = (target - v).abs();
                assert!(gap <= last_gap, "monotone");
                assert!(
                    (v - cur).abs() <= (target - cur).abs() + 1e-6,
                    "no overshoot"
                );
                last_gap = gap;
            }
            assert!(last_gap < 1e-4, "converged: {last_gap}");
            assert_eq!(smooth_towards(cur, target, f32::INFINITY, 0.12), target);
            assert_eq!(smooth_towards(cur, target, 100.0, 0.12), target);
            assert_eq!(smooth_towards(cur, target, 0.0, 0.12), cur);
            assert_eq!(smooth_towards(cur, target, 0.1, 0.0), target, "tau 0 jumps");
        }
        // one tau gets 1 - 1/e of the way
        assert!(near(
            smooth_towards(0.0, 1.0, 0.12, 0.12),
            1.0 - (-1.0f32).exp(),
            1e-6
        ));
    }

    #[test]
    fn smoothed_step_reports_only_real_changes() {
        let target = SourceGain {
            id: SourceId(1),
            gain: 1.0,
            pan: 0.0,
            lowpass_hz: None,
        };
        let mut s = Smoothed::new(1.0, 0.0, NO_LOWPASS_HZ);
        assert!(!s.step(&target, 0.05), "already there");
        // a tiny gain change (< 0.5 dB) is not reported
        let tiny = SourceGain {
            gain: db_to_linear(-0.3),
            ..target
        };
        let mut t = Smoothed::new(1.0, 0.0, NO_LOWPASS_HZ);
        let mut reports = 0;
        for _ in 0..100 {
            reports += t.step(&tiny, 0.05) as u32;
        }
        assert_eq!(reports, 1, "only the final settle");
        assert_eq!(t.gain, tiny.gain);
        // a door closing (-24 dB) reports right away and settles exactly
        let closed = SourceGain {
            gain: db_to_linear(-24.0),
            ..target
        };
        assert!(s.step(&closed, 0.05));
        let mut n = 0;
        for _ in 0..200 {
            n += s.step(&closed, 0.05) as u32;
        }
        assert!(n > 0 && n < 60, "throttled reports: {n}");
        assert_eq!(s.gain, closed.gain);
        assert!(!s.step(&closed, 0.05), "quiet once settled");
        // pan past 0.02 is reported; 0.01 is only the final settle
        let mut q = Smoothed::new(1.0, 0.0, NO_LOWPASS_HZ);
        assert!(q.step(&SourceGain { pan: 0.5, ..target }, 0.05));
        let mut r = Smoothed::new(1.0, 0.0, NO_LOWPASS_HZ);
        assert!(!r.step(
            &SourceGain {
                pan: 0.01,
                ..target
            },
            0.01
        ));
        // a low-pass sweep is reported
        let mut f = Smoothed::new(1.0, 0.0, NO_LOWPASS_HZ);
        assert!(f.step(
            &SourceGain {
                lowpass_hz: Some(800.0),
                ..target
            },
            0.05
        ));
        assert_eq!(Smoothed::default().lowpass_hz, NO_LOWPASS_HZ);
    }
}
