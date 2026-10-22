//! Our own sounds (addendum §4.7): `nkw_objects::sound::Mixer` behind a
//! lock, rendered by `nkw-sys`'s PipeWire playback stream (48 kHz, stereo;
//! the adapter disconnects it after 200 ms of silence). A cue is placed
//! like an app stream: the acoustics give it gain, pan and, through a door,
//! its low-pass. Without a playback stream (`--demo --headless`, no
//! PipeWire) the mixer still runs, so the tests hear it, and the output is
//! dropped.

use std::sync::{Arc, Mutex};

use glam::Vec2;
use nkw_core::{RoomId, SourceId, Time};
use nkw_objects::acoustic::{self, Listener, Params, RoomGraph, Source};
use nkw_objects::sound::{Cue, Floor, Mixer};
use nkw_sys::Playback;

/// The playback rate.
pub const RATE: u32 = 48_000;
/// One footstep per this much walking, metres.
pub const STRIDE_M: f32 = 0.55;
/// Without a stream, at most this much of the mixer is run per tick, s.
const MAX_DRAIN_S: f64 = 0.25;

/// How loud each cue is at the listener, before the acoustics.
pub fn base_gain(cue: &Cue) -> f32 {
    match cue {
        Cue::Footstep(_) => 0.25,
        Cue::DoorOpen | Cue::DoorClose => 0.6,
        Cue::DoorThunk => 0.35,
        Cue::Meow | Cue::Purr => 0.5,
        Cue::Blip { .. } => 0.3,
        Cue::Click => 0.3,
        Cue::VaultDial | Cue::VaultUnlock | Cue::VaultLock => 0.5,
    }
}

/// Footsteps from the walker's travel: one per `STRIDE_M`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Steps {
    last: Option<Vec2>,
    travel: f32,
}

impl Steps {
    /// The walker is at `at` (`None`: not walking, the count resets).
    /// Returns how many footsteps are due.
    pub fn walk(&mut self, at: Option<Vec2>) -> usize {
        let Some(p) = at else {
            *self = Steps::default();
            return 0;
        };
        if let Some(q) = self.last {
            // a teleport (a glide, a room change) is not a walk
            let d = p.distance(q);
            if d < 2.0 {
                self.travel += d;
            }
        }
        self.last = Some(p);
        let n = (self.travel / STRIDE_M).floor();
        self.travel -= n * STRIDE_M;
        n as usize
    }
}

/// The floor of a room for its footsteps: the reception's boards, the
/// hall's tiles, the work rooms' carpet.
pub fn floor_of(room: Option<usize>) -> Floor {
    match room {
        Some(0) => Floor::Wood,
        Some(_) => Floor::Carpet,
        None => Floor::Tile,
    }
}

/// What was played, for `dump` and the tests.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Played {
    pub count: u64,
    pub last: Vec<Cue>,
}

pub struct Sound {
    pub mixer: Arc<Mutex<Mixer>>,
    playback: Option<Arc<dyn Playback>>,
    pub played: Played,
    pub steps: Steps,
    drained: Option<Time>,
    scratch: Vec<f32>,
}

impl Default for Sound {
    fn default() -> Sound {
        Sound::new()
    }
}

impl Sound {
    pub fn new() -> Sound {
        Sound {
            mixer: Arc::new(Mutex::new(Mixer::new(RATE))),
            playback: None,
            played: Played::default(),
            steps: Steps::default(),
            drained: None,
            scratch: Vec::new(),
        }
    }

    /// Hand the mixer to a playback stream (its `process` callback renders it).
    pub fn attach(&mut self, pb: Arc<dyn Playback>) {
        let m = self.mixer.clone();
        pb.set_render(Box::new(move |out: &mut [f32]| match m.lock() {
            Ok(mut m) => m.render(out),
            Err(_) => {
                out.fill(0.0);
                false
            }
        }));
        self.playback = Some(pb);
    }

    pub fn attached(&self) -> bool {
        self.playback.is_some()
    }

    /// Play `cue` at `at` (floor point and room; `None`: at the listener).
    pub fn play(
        &mut self,
        cue: Cue,
        at: Option<(Vec2, RoomId)>,
        l: &Listener,
        g: &RoomGraph,
        p: &Params,
    ) {
        let (gain, pan, lowpass) = match at {
            None => (1.0, 0.0, None),
            Some((pos, room)) => {
                let s = Source {
                    id: SourceId(0),
                    pos,
                    room,
                    own_volume: 1.0,
                };
                let r = acoustic::gains(p, l, &[s], g)[0];
                (r.gain, r.pan, r.lowpass_hz)
            }
        };
        let gain = gain * base_gain(&cue);
        if gain <= 1e-4 {
            return;
        }
        if let Ok(mut m) = self.mixer.lock() {
            m.play(cue, gain, pan, lowpass);
        }
        self.played.count += 1;
        self.played.last.push(cue);
        if self.played.last.len() > 16 {
            self.played.last.remove(0);
        }
        if let Some(pb) = &self.playback {
            pb.wake();
        }
    }

    /// Without a playback stream, run the mixer for the time that passed
    /// (bounded) and drop the samples, so voices end as they would.
    pub fn drain(&mut self, now: Time) {
        let last = self.drained.replace(now);
        if self.playback.is_some() {
            return;
        }
        let Ok(mut m) = self.mixer.lock() else { return };
        if m.active() == 0 {
            return;
        }
        let dt = last.map_or(0.0, |t| (now - t).clamp(0.0, MAX_DRAIN_S));
        let frames = (dt * RATE as f64) as usize;
        if frames == 0 {
            return;
        }
        self.scratch.resize(frames * 2, 0.0);
        m.render(&mut self.scratch);
    }

    pub fn active(&self) -> usize {
        self.mixer.lock().map_or(0, |m| m.active())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::objects::doors::{Doors, HALL};
    use nkw_core::strip::rooms_along_hall;

    #[test]
    fn footstep_cadence_by_distance() {
        let mut s = Steps::default();
        assert_eq!(s.walk(Some(Vec2::ZERO)), 0);
        let mut total = 0;
        // 2.3 m in 1 cm steps: 4 footsteps
        for i in 1..=230 {
            total += s.walk(Some(Vec2::new(i as f32 * 0.01, 0.0)));
        }
        assert_eq!(total, 4);
        // standing still: nothing
        assert_eq!(s.walk(Some(Vec2::new(2.3, 0.0))), 0);
        // a jump (a glide) does not count
        assert_eq!(s.walk(Some(Vec2::new(20.0, 0.0))), 0);
        // leaving ANDAR resets the stride
        s.walk(Some(Vec2::new(20.5, 0.0)));
        assert_eq!(s.walk(None), 0);
        assert_eq!(s.walk(Some(Vec2::new(20.6, 0.0))), 0);
        assert_eq!(s.walk(Some(Vec2::new(21.0, 0.0))), 0, "0.4 m < a stride");
    }

    #[test]
    fn floors() {
        assert_eq!(floor_of(Some(0)), Floor::Wood);
        assert_eq!(floor_of(Some(3)), Floor::Carpet);
        assert_eq!(floor_of(None), Floor::Tile);
    }

    fn graph(doors: &Doors) -> RoomGraph {
        RoomGraph::new(&[RoomId(0), RoomId(1), HALL], &doors.portals(0.0))
    }

    #[test]
    fn cues_play_through_the_acoustics_and_end_without_a_stream() {
        let rooms: Vec<_> = rooms_along_hall(&[6.0, 6.0])
            .into_iter()
            .enumerate()
            .map(|(i, s)| (RoomId(i as u32), s))
            .collect();
        let mut doors = Doors::default();
        doors.sync(&rooms);
        let l = Listener {
            pos: Vec2::new(3.0, -4.0),
            yaw: 0.0,
            room: RoomId(0),
        };
        let p = Params::default();
        let mut s = Sound::new();
        s.play(Cue::Meow, None, &l, &graph(&doors), &p);
        s.play(
            Cue::DoorOpen,
            Some((Vec2::new(3.0, -1.5), RoomId(0))),
            &l,
            &graph(&doors),
            &p,
        );
        assert_eq!(s.played.count, 2);
        assert_eq!(s.active(), 2);
        assert!(!s.attached());
        // nothing beyond max_hops reaches us: not even queued
        let none = RoomGraph::new(&[RoomId(0)], &[]);
        s.play(Cue::Click, Some((Vec2::ZERO, RoomId(1))), &l, &none, &p);
        assert_eq!(s.played.count, 2);
        // the mixer runs on with no stream and the voices end
        s.drain(0.0);
        for i in 1..=12 {
            s.drain(i as f64 * 0.1);
        }
        assert_eq!(s.active(), 0);
    }

    #[test]
    fn base_gains_are_sane() {
        for c in [
            Cue::Footstep(Floor::Wood),
            Cue::DoorOpen,
            Cue::DoorClose,
            Cue::DoorThunk,
            Cue::Meow,
            Cue::Blip { pitch: 200.0 },
            Cue::VaultDial,
            Cue::VaultUnlock,
            Cue::VaultLock,
            Cue::Click,
            Cue::Purr,
        ] {
            let g = base_gain(&c);
            assert!(g > 0.0 && g <= 1.0, "{c:?}");
        }
    }
}
