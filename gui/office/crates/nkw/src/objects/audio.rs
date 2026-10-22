//! Proximity audio v1 (addendum §4.5–4.6): PipeWire output streams anchored
//! to the screens of their windows (pid, then up the `ppid`s, then the
//! `app_id`) or, for the media player's, to the turntable; every anchored
//! stream gets the acoustic model's gain and pan as per-channel volumes, at
//! most 20 times a second and only when the smoothed value moved. v1 is
//! attenuation and pan: the low-pass is computed, never applied.
//!
//! The world only ever lowers what it anchored; everything else is left as
//! it is, and what it touched goes back to `[1, 1]` on exit (and from a
//! panic hook).

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

use glam::Vec2;
use nkw_core::{ObjectId, RoomId, ScreenId, SourceId, Time};
use nkw_objects::acoustic::{
    self, Listener, Params, RoomGraph, Smoothed, Source, SourceGain, pan_to_stereo,
};
use nkw_sys::pidmap::{self, ProcPort};
use nkw_sys::{AudioEvent, StreamId, StreamInfo, StreamVolume};

/// At most this many volume updates a second.
pub const MAX_HZ: f64 = 20.0;
/// `ppid` hops from a stream's pid to a window's.
pub const MAX_HOPS: usize = 6;

/// Where a stream sounds from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Anchor {
    Screen(ScreenId),
    /// the media player's streams: at the turntable
    Object(ObjectId),
    /// not the world's: untouched
    None,
}

/// What a stream is matched against.
pub struct Known<'a> {
    pub pids: &'a BTreeMap<ScreenId, u32>,
    /// every screen's `app_id`
    pub app_ids: &'a [(ScreenId, String)],
    /// the MPRIS player the turntable shows (bus name suffix) and the
    /// turntable's id
    pub player: Option<(&'a str, ObjectId)>,
}

fn last_segment(s: &str) -> String {
    s.rsplit('.').next().unwrap_or(s).to_lowercase()
}

/// Whether a stream belongs to the MPRIS player `player`
/// (`org.mpris.MediaPlayer2.<player>`; `firefox.instance_1_2` → `firefox`).
pub fn is_player(player: &str, info: &StreamInfo) -> bool {
    let base = player.split('.').next().unwrap_or(player).to_lowercase();
    if base.is_empty() {
        return false;
    }
    info.binary
        .as_deref()
        .is_some_and(|b| b.to_lowercase() == base)
        || info
            .app_id
            .as_deref()
            .is_some_and(|a| last_segment(a) == base || a.eq_ignore_ascii_case(player))
}

/// The anchor of a stream: its pid (or an ancestor's, ≤ 6 hops) is a
/// window's → that screen; else the media player's → the turntable; else
/// its `app_id`/binary names a window's `app_id` → that screen; else none.
pub fn resolve(info: &StreamInfo, k: &Known, proc_: &dyn ProcPort) -> Anchor {
    if !info.output {
        return Anchor::None;
    }
    if let Some(pid) = info.pid {
        let known = |p: u32| k.pids.values().any(|&x| x == p);
        if let Some(hit) = pidmap::anchor(pid, &known, proc_, MAX_HOPS)
            && let Some((s, _)) = k.pids.iter().find(|(_, p)| **p == hit)
        {
            return Anchor::Screen(*s);
        }
    }
    if let Some((player, tt)) = k.player
        && is_player(player, info)
    {
        return Anchor::Object(tt);
    }
    let names: Vec<String> = [info.app_id.as_deref(), info.binary.as_deref()]
        .into_iter()
        .flatten()
        .map(|s| s.to_lowercase())
        .collect();
    for (s, app) in k.app_ids {
        let a = app.to_lowercase();
        if a.is_empty() {
            continue;
        }
        if names
            .iter()
            .any(|n| *n == a || last_segment(n) == last_segment(&a))
        {
            return Anchor::Screen(*s);
        }
    }
    Anchor::None
}

#[derive(Clone, Debug, PartialEq)]
pub struct Stream {
    pub info: StreamInfo,
    pub anchor: Anchor,
    pub smooth: Smoothed,
    /// the last target, for `settled`
    pub target: Option<SourceGain>,
}

impl Stream {
    fn settled(&self) -> bool {
        self.target
            .is_none_or(|t| self.smooth.gain == t.gain && self.smooth.pan == t.pan)
    }
}

/// The inputs of the last step: when they change, a new one is due.
#[derive(Clone, Debug, Default, PartialEq)]
struct Inputs {
    listener: Option<(Vec2, f32, RoomId)>,
    openness: Vec<f32>,
    sources: Vec<(StreamId, Vec2, RoomId)>,
}

#[derive(Default)]
pub struct Audio {
    pub streams: BTreeMap<StreamId, Stream>,
    pub params: Params,
    /// what we changed (restored to `[1, 1]`), shared with the panic hook
    pub touched: Arc<Mutex<BTreeSet<StreamId>>>,
    last_step: Option<Time>,
    last: Inputs,
    /// volume updates sent so far
    pub calls: u64,
}

impl Audio {
    /// A stream came or went. `Added` streams start unanchored; call
    /// [`Audio::reanchor`] after.
    pub fn on_event(&mut self, ev: &AudioEvent) {
        match ev {
            AudioEvent::Added(info) => {
                self.streams.insert(
                    info.id,
                    Stream {
                        info: info.clone(),
                        anchor: Anchor::None,
                        // as the stream is: untouched, full volume
                        smooth: Smoothed::new(1.0, 0.0, acoustic::NO_LOWPASS_HZ),
                        target: None,
                    },
                );
            }
            AudioEvent::Removed(id) => {
                self.streams.remove(id);
                // gone: PipeWire forgets it, nothing to restore
                self.touched.lock().unwrap().remove(id);
            }
            AudioEvent::DefaultSinkVolume(_) => {}
        }
    }

    /// Resolve every stream's anchor again (a window came or went, the
    /// player changed). A stream that lost its anchor goes back to `[1, 1]`.
    pub fn reanchor(&mut self, k: &Known, proc_: &dyn ProcPort, out: &dyn StreamVolume) {
        for (id, s) in &mut self.streams {
            let a = resolve(&s.info, k, proc_);
            if a == s.anchor {
                continue;
            }
            if a == Anchor::None && self.touched.lock().unwrap().remove(id) {
                out.set_channel_volumes(*id, [1.0, 1.0]);
                self.calls += 1;
            }
            s.anchor = a;
            s.smooth = Smoothed::new(1.0, 0.0, acoustic::NO_LOWPASS_HZ);
            s.target = None;
        }
        self.last = Inputs::default();
    }

    pub fn anchored(&self) -> usize {
        self.streams
            .values()
            .filter(|s| s.anchor != Anchor::None)
            .count()
    }

    /// Whether a step is due (the listener, a door or a source moved since
    /// the last one, or a smoothed value is still on its way).
    pub fn due(
        &self,
        l: &Listener,
        openness: &[f32],
        at: &dyn Fn(Anchor) -> Option<(Vec2, RoomId)>,
    ) -> bool {
        self.anchored() > 0
            && (self.inputs(l, openness, at) != self.last
                || self.streams.values().any(|s| !s.settled()))
    }

    fn inputs(
        &self,
        l: &Listener,
        openness: &[f32],
        at: &dyn Fn(Anchor) -> Option<(Vec2, RoomId)>,
    ) -> Inputs {
        Inputs {
            listener: Some((l.pos, l.yaw, l.room)),
            openness: openness.to_vec(),
            sources: self
                .streams
                .iter()
                .filter_map(|(id, s)| at(s.anchor).map(|(p, r)| (*id, p, r)))
                .collect(),
        }
    }

    /// One smoothing step at `now` (skipped within 1/20 s of the last):
    /// targets from `gains()`, `Smoothed::step`, and a volume call only for
    /// the streams whose value moved. Returns the calls made.
    pub fn step(
        &mut self,
        now: Time,
        l: &Listener,
        g: &RoomGraph,
        openness: &[f32],
        at: &dyn Fn(Anchor) -> Option<(Vec2, RoomId)>,
        out: &dyn StreamVolume,
    ) -> usize {
        if self
            .last_step
            .is_some_and(|t| now - t < 1.0 / MAX_HZ - 1e-6)
        {
            return 0;
        }
        let dt = self.last_step.map_or(1.0 / MAX_HZ, |t| (now - t).min(0.25)) as f32;
        self.last_step = Some(now);
        self.last = self.inputs(l, openness, at);
        let mut srcs = Vec::new();
        for (id, s) in &self.streams {
            if let Some((pos, room)) = at(s.anchor) {
                srcs.push(Source {
                    id: SourceId(id.0),
                    pos,
                    room,
                    own_volume: 1.0,
                });
            }
        }
        let targets = acoustic::gains(&self.params, l, &srcs, g);
        let mut n = 0;
        for t in targets {
            let id = StreamId(t.id.0);
            let Some(s) = self.streams.get_mut(&id) else {
                continue;
            };
            // v1: attenuation and pan; the low-pass stays computed only
            let t = SourceGain {
                lowpass_hz: None,
                ..t
            };
            s.target = Some(t);
            if s.smooth.step(&t, dt) {
                let (left, right) = pan_to_stereo(s.smooth.gain, s.smooth.pan, false);
                out.set_channel_volumes(id, [left, right]);
                self.touched.lock().unwrap().insert(id);
                n += 1;
            }
        }
        self.calls += n as u64;
        n
    }

    /// Everything we touched back to `[1, 1]`.
    pub fn restore(&mut self, out: &dyn StreamVolume) {
        let ids: Vec<StreamId> = std::mem::take(&mut *self.touched.lock().unwrap())
            .into_iter()
            .collect();
        for id in ids {
            out.set_channel_volumes(id, [1.0, 1.0]);
            self.calls += 1;
        }
    }

    /// For `dump`: `{streams, anchored, gains: [{id, gain, pan}]}`.
    pub fn dump(&self) -> serde_json::Value {
        let r = |x: f32| (x * 1000.0).round() / 1000.0;
        let gains: Vec<_> = self
            .streams
            .iter()
            .filter(|(_, s)| s.anchor != Anchor::None)
            .map(|(id, s)| serde_json::json!({"id": id.0, "gain": r(s.smooth.gain), "pan": r(s.smooth.pan)}))
            .collect();
        serde_json::json!({
            "streams": self.streams.len(),
            "anchored": self.anchored(),
            "gains": gains,
            "calls": self.calls,
        })
    }
}

/// Restores `touched` through `out` if the process panics (the volumes we
/// lowered must not outlive us). Installed once; chains the previous hook.
pub fn install_panic_restore(touched: Arc<Mutex<BTreeSet<StreamId>>>, out: Arc<dyn StreamVolume>) {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let prev = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            if let Ok(t) = touched.try_lock() {
                for id in t.iter() {
                    out.set_channel_volumes(*id, [1.0, 1.0]);
                }
            }
            prev(info);
        }));
    });
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::objects::doors::{Doors, HALL};
    use nkw_core::strip::{RoomSlot, rooms_along_hall};
    use std::collections::HashMap;
    use std::sync::Mutex as StdMutex;

    /// Records every volume call.
    #[derive(Default)]
    pub struct Rec {
        pub calls: StdMutex<Vec<(StreamId, [f32; 2])>>,
        pub sink: StdMutex<Vec<f32>>,
    }
    impl StreamVolume for Rec {
        fn set_channel_volumes(&self, id: StreamId, lr: [f32; 2]) {
            self.calls.lock().unwrap().push((id, lr));
        }
        fn set_default_sink_volume(&self, v: f32) {
            self.sink.lock().unwrap().push(v);
        }
    }

    struct Tree(HashMap<u32, u32>);
    impl ProcPort for Tree {
        fn ppid(&self, pid: u32) -> Option<u32> {
            self.0.get(&pid).copied()
        }
        fn comm(&self, _: u32) -> Option<String> {
            None
        }
    }

    fn info(id: u32, pid: Option<u32>, binary: Option<&str>, app_id: Option<&str>) -> StreamInfo {
        StreamInfo {
            id: StreamId(id),
            pid,
            binary: binary.map(Into::into),
            app_id: app_id.map(Into::into),
            output: true,
        }
    }

    #[test]
    fn anchor_resolution_table() {
        // 300 → 200 → 100 (a window's client)
        let tree = Tree(HashMap::from([(300, 200), (200, 100), (100, 1), (50, 1)]));
        let pids = BTreeMap::from([(ScreenId(1), 100), (ScreenId(2), 77)]);
        let app_ids = vec![
            (ScreenId(1), "acme.alpha".to_string()),
            (ScreenId(3), "org.demo.Bravo".to_string()),
        ];
        let tt = ObjectId(1);
        let k = Known {
            pids: &pids,
            app_ids: &app_ids,
            player: Some(("mpv.instance_2", tt)),
        };
        let rows = [
            (
                info(1, Some(100), None, None),
                Anchor::Screen(ScreenId(1)),
                "direct pid",
            ),
            (
                info(2, Some(300), Some("x"), None),
                Anchor::Screen(ScreenId(1)),
                "ppid hops",
            ),
            (
                info(3, Some(50), Some("mpv"), None),
                Anchor::Object(tt),
                "the player",
            ),
            (
                info(4, Some(50), Some("bravo"), None),
                Anchor::Screen(ScreenId(3)),
                "binary ~ app_id",
            ),
            (
                info(5, None, None, Some("org.demo.bravo")),
                Anchor::Screen(ScreenId(3)),
                "flatpak app_id",
            ),
            (info(6, Some(50), Some("other"), None), Anchor::None, "none"),
            (info(7, None, None, None), Anchor::None, "nothing to go on"),
        ];
        for (i, want, what) in rows {
            assert_eq!(resolve(&i, &k, &tree), want, "{what}");
        }
        let mut input = info(8, Some(100), None, None);
        input.output = false;
        assert_eq!(
            resolve(&input, &k, &tree),
            Anchor::None,
            "capture streams never"
        );
        // a window's own pid wins over the player name
        let both = info(9, Some(100), Some("mpv"), None);
        assert_eq!(resolve(&both, &k, &tree), Anchor::Screen(ScreenId(1)));
    }

    #[test]
    fn player_matching() {
        assert!(is_player(
            "firefox.instance_1_23",
            &info(1, None, Some("Firefox"), None)
        ));
        assert!(!is_player(
            "spotify",
            &info(1, None, None, Some("com.spotify.Client"))
        ));
        assert!(is_player("spotify", &info(1, None, Some("spotify"), None)));
        assert!(!is_player("", &info(1, None, Some(""), None)));
    }

    fn rooms() -> Vec<(RoomId, RoomSlot)> {
        rooms_along_hall(&[6.0, 6.0, 6.0])
            .into_iter()
            .enumerate()
            .map(|(i, s)| (RoomId(i as u32), s))
            .collect()
    }

    fn graph(d: &Doors, now: Time) -> RoomGraph {
        let mut ids: Vec<RoomId> = rooms().iter().map(|r| r.0).collect();
        ids.push(HALL);
        RoomGraph::new(&ids, &d.portals(now))
    }

    /// A stream in room 1 (across the hall), the listener in room 0.
    fn setup() -> (Audio, Doors, Listener, Vec2) {
        let mut a = Audio::default();
        a.on_event(&AudioEvent::Added(info(7, Some(100), None, None)));
        let pids = BTreeMap::from([(ScreenId(1), 100)]);
        let k = Known {
            pids: &pids,
            app_ids: &[],
            player: None,
        };
        let rec = Rec::default();
        a.reanchor(&k, &Tree(HashMap::new()), &rec);
        let mut d = Doors::default();
        d.sync(&rooms());
        let r = rooms();
        let src = nkw_core::layout::to_world(&r[1].1, glam::Vec3::new(3.0, 1.0, 1.0));
        let (door0, _) = nkw_core::layout::door_world(&r[0].1);
        // the listener left of its door, facing the back wall
        let l = Listener {
            pos: door0 + Vec2::new(-1.5, -1.5),
            yaw: 0.0,
            room: RoomId(0),
        };
        (a, d, l, Vec2::new(src.x, src.z))
    }

    fn settle(a: &mut Audio, d: &Doors, l: &Listener, src: Vec2, out: &Rec) -> f32 {
        let at = |an: Anchor| (an != Anchor::None).then_some((src, RoomId(1)));
        let open: Vec<f32> = d.portals(0.0).iter().map(|p| p.openness).collect();
        let g = graph(d, 0.0);
        let t0 = a.last_step.unwrap_or(0.0);
        for i in 1..=200 {
            a.step(t0 + i as f64 * 0.05, l, &g, &open, &at, out);
        }
        a.streams[&StreamId(7)].smooth.gain
    }

    #[test]
    fn a_closed_door_is_minus_24_db_and_pans_towards_the_door() {
        let (mut a, mut d, l, src) = setup();
        let rec = Rec::default();
        let open_gain = settle(&mut a, &d, &l, src, &rec);
        assert!(open_gain > 0.0);
        d.toggle(nkw_core::DoorId(0), 0.0, true);
        d.tick(0.0, Vec2::splat(-99.0), true);
        let shut_gain = settle(&mut a, &d, &l, src, &rec);
        let ratio = shut_gain / open_gain;
        assert!((ratio - 0.063).abs() < 0.002, "−24 dB: {ratio}");
        // the door is to the listener's right
        assert!(a.streams[&StreamId(7)].smooth.pan > 0.3);
        let last = rec.calls.lock().unwrap().last().copied().unwrap();
        assert!(last.1[1] > last.1[0], "right channel louder");
        assert!(a.touched.lock().unwrap().contains(&StreamId(7)));
    }

    #[test]
    fn smoothing_steps_at_most_20_hz_and_calls_only_on_change() {
        let (mut a, d, l, src) = setup();
        let rec = Rec::default();
        let at = |an: Anchor| (an != Anchor::None).then_some((src, RoomId(1)));
        let open: Vec<f32> = d.portals(0.0).iter().map(|p| p.openness).collect();
        let g = graph(&d, 0.0);
        assert!(a.due(&l, &open, &at));
        // 120 Hz of ticks for 2 s: at most 40 steps, so at most 40 calls
        let mut steps_with_calls = 0;
        for i in 0..240 {
            if a.step(i as f64 / 120.0, &l, &g, &open, &at, &rec) > 0 {
                steps_with_calls += 1;
            }
        }
        let calls = rec.calls.lock().unwrap().len();
        assert!((3..=41).contains(&calls), "{calls}");
        assert!(steps_with_calls <= 41);
        // settled: nothing more is due or sent
        let before = calls;
        for i in 240..480 {
            a.step(i as f64 / 120.0, &l, &g, &open, &at, &rec);
        }
        assert!(!a.due(&l, &open, &at), "settled and nothing moved");
        assert_eq!(
            rec.calls.lock().unwrap().len(),
            before,
            "no change, no call"
        );
        // the listener turns: due again
        let turned = Listener { yaw: 1.0, ..l };
        assert!(a.due(&turned, &open, &at));
    }

    #[test]
    fn restore_puts_back_what_we_touched_and_only_that() {
        let (mut a, d, l, src) = setup();
        a.on_event(&AudioEvent::Added(info(9, Some(5), Some("x"), None)));
        let rec = Rec::default();
        settle(&mut a, &d, &l, src, &rec);
        let out = Rec::default();
        a.restore(&out);
        assert_eq!(*out.calls.lock().unwrap(), [(StreamId(7), [1.0, 1.0])]);
        assert!(a.touched.lock().unwrap().is_empty());
        let again = Rec::default();
        a.restore(&again);
        assert!(again.calls.lock().unwrap().is_empty());
    }

    #[test]
    fn losing_the_anchor_restores_and_removal_forgets() {
        let (mut a, d, l, src) = setup();
        let rec = Rec::default();
        settle(&mut a, &d, &l, src, &rec);
        let none = BTreeMap::new();
        let k = Known {
            pids: &none,
            app_ids: &[],
            player: None,
        };
        let out = Rec::default();
        a.reanchor(&k, &Tree(HashMap::new()), &out);
        assert_eq!(*out.calls.lock().unwrap(), [(StreamId(7), [1.0, 1.0])]);
        assert_eq!(a.anchored(), 0);
        a.on_event(&AudioEvent::Removed(StreamId(7)));
        assert!(a.streams.is_empty());
        let d = a.dump();
        assert_eq!(d["streams"], 0);
    }

    #[test]
    fn unanchored_streams_are_never_touched() {
        let mut a = Audio::default();
        a.on_event(&AudioEvent::Added(info(3, Some(5), Some("x"), None)));
        let rec = Rec::default();
        let (_, d, l, _) = setup();
        let at = |an: Anchor| (an != Anchor::None).then_some((Vec2::ZERO, RoomId(0)));
        let open: Vec<f32> = d.portals(0.0).iter().map(|p| p.openness).collect();
        assert!(!a.due(&l, &open, &at));
        for i in 0..20 {
            a.step(i as f64 * 0.1, &l, &graph(&d, 0.0), &open, &at, &rec);
        }
        assert!(rec.calls.lock().unwrap().is_empty());
    }
}
