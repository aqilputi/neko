//! Pure pieces of the overlay's ANDAR and of the tags' feel (addendum §3.1,
//! §3.6; plan §3.11): the crosshair's dot ↔ ring transition (80 ms), the
//! comfort vignette's quantised alpha, and the 2D tags' fade (80 ms in,
//! 120 ms out). Time in, raster steps out; no Smithay, no fonts.

use std::collections::BTreeMap;

use glam::Vec2;
use nkw_core::Time;

use super::draw::CROSS_STEPS;
pub use super::draw::FADE_STEPS;

/// The crosshair's dot ↔ ring transition.
pub const CROSS_S: Time = 0.08;
/// A tag fades in over this, and out over [`TAG_OUT`] once it leaves its
/// reading radius.
pub const TAG_IN: Time = 0.08;
pub const TAG_OUT: Time = 0.12;

/// The crosshair: a dot, a 6 px ring when something interactable is in
/// the aim.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cross {
    aimed: bool,
    /// the step the last change started from, and when
    from: u8,
    since: Time,
}

impl Default for Cross {
    fn default() -> Cross {
        Cross {
            aimed: false,
            from: 0,
            since: Time::NEG_INFINITY,
        }
    }
}

impl Cross {
    /// The aim this frame; a change starts the transition from where the
    /// last one is.
    pub fn update(&mut self, aimed: bool, now: Time, reduced: bool) {
        if aimed != self.aimed {
            self.from = self.step(now, reduced);
            self.aimed = aimed;
            self.since = now;
        }
    }

    pub fn aimed(&self) -> bool {
        self.aimed
    }

    fn target(&self) -> u8 {
        if self.aimed { CROSS_STEPS } else { 0 }
    }

    /// The raster step at `now`: 0 the dot … [`CROSS_STEPS`] the ring.
    pub fn step(&self, now: Time, reduced: bool) -> u8 {
        let p = if reduced {
            1.0
        } else {
            ((now - self.since) / CROSS_S).clamp(0.0, 1.0)
        };
        let (a, b) = (self.from as f64, self.target() as f64);
        (a + (b - a) * p).round() as u8
    }

    /// Still changing at `now` (a short `Continuous`, like a toast's slide).
    pub fn moving(&self, now: Time, reduced: bool) -> bool {
        self.step(now, reduced) != self.target()
    }
}

/// The vignette's alpha in sixteenths (0 = none: no element).
pub fn vignette_a16(alpha: f32) -> u8 {
    (alpha.clamp(0.0, 1.0) * 16.0).round() as u8
}

/// One tag as the frame shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct Shown {
    pub id: u64,
    pub text: String,
    pub rgb: [u8; 3],
    /// logical px, the tag's anchor
    pub at: Vec2,
    /// 1..=[`FADE_STEPS`]
    pub level: u8,
    /// inside its radius this frame (a fading-out tag is not)
    pub inside: bool,
}

#[derive(Clone, Debug, PartialEq)]
struct Fade {
    inside: bool,
    /// the level the last change started from, and when
    from: f64,
    since: Time,
    last: Shown,
}

impl Fade {
    fn level(&self, now: Time, reduced: bool) -> f64 {
        let (to, dur) = if self.inside {
            (1.0, TAG_IN)
        } else {
            (0.0, TAG_OUT)
        };
        if reduced {
            return to;
        }
        let p = ((now - self.since) / dur).clamp(0.0, 1.0);
        self.from + (to - self.from) * p
    }
}

/// The tags' fades, by tag id.
#[derive(Default, Debug)]
pub struct Fades {
    map: BTreeMap<u64, Fade>,
}

/// An opacity 0..1 as a drawn step (0 hides): rising, a step is reached at
/// its lower boundary (it shows from its first frame); falling, at its
/// upper one (it is gone exactly at 0).
fn quantise(v: f64, rising: bool) -> u8 {
    let n = FADE_STEPS as f64;
    let q = if rising {
        (v * n + 1e-6).floor() + 1.0
    } else {
        (v * n - 1e-6).ceil()
    };
    q.clamp(0.0, n) as u8
}

impl Fades {
    /// This frame's visible tags (id, text, colour, anchor) → what to draw,
    /// fading ones included, by id.
    pub fn update(
        &mut self,
        now: Time,
        reduced: bool,
        visible: impl IntoIterator<Item = (u64, String, [u8; 3], Vec2)>,
    ) -> Vec<Shown> {
        let mut seen = Vec::new();
        for (id, text, rgb, at) in visible {
            seen.push(id);
            let last = Shown {
                id,
                text,
                rgb,
                at,
                level: FADE_STEPS,
                inside: true,
            };
            match self.map.get_mut(&id) {
                Some(f) => {
                    if !f.inside {
                        f.from = f.level(now, reduced);
                        f.since = now;
                        f.inside = true;
                    }
                    f.last = last;
                }
                None => {
                    self.map.insert(
                        id,
                        Fade {
                            inside: true,
                            from: 0.0,
                            since: now,
                            last,
                        },
                    );
                }
            }
        }
        for (id, f) in self.map.iter_mut() {
            if f.inside && !seen.contains(id) {
                f.from = f.level(now, reduced);
                f.since = now;
                f.inside = false;
            }
        }
        self.map
            .retain(|_, f| f.inside || f.level(now, reduced) > 0.0);
        let mut out = Vec::new();
        for f in self.map.values() {
            let level = quantise(f.level(now, reduced), f.inside);
            if level == 0 {
                continue;
            }
            out.push(Shown {
                level,
                inside: f.inside,
                ..f.last.clone()
            });
        }
        out
    }

    /// When the next drawn step changes (a `Want::At`), if a fade runs.
    pub fn next_at(&self, now: Time, reduced: bool) -> Option<Time> {
        if reduced {
            return None;
        }
        self.map
            .values()
            .filter_map(|f| {
                let (to, dur) = if f.inside {
                    (1.0, TAG_IN)
                } else {
                    (0.0, TAG_OUT)
                };
                let end = f.since + dur;
                if now >= end || f.from == to {
                    return None;
                }
                // the next step boundary along the ramp
                let cur = quantise(f.level(now, reduced), f.inside) as f64;
                let n = FADE_STEPS as f64;
                let next = if f.inside { cur / n } else { (cur - 1.0) / n };
                let p = ((next - f.from) / (to - f.from)).clamp(0.0, 1.0);
                let t = f.since + p * dur;
                Some(if t > now + 1e-9 { t.min(end) } else { end })
            })
            .min_by(f64::total_cmp)
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crosshair_dot_to_ring_in_80_ms_and_back() {
        let mut c = Cross::default();
        assert_eq!(c.step(0.0, false), 0, "a dot at first");
        assert!(!c.moving(0.0, false));
        c.update(true, 1.0, false);
        let rows = [
            (1.0, 0),
            (1.015, 1),
            (1.02, 1),
            (1.04, 2),
            (1.06, 3),
            (1.08, CROSS_STEPS),
            (2.0, CROSS_STEPS),
        ];
        for (t, want) in rows {
            assert_eq!(c.step(t, false), want, "{t}");
        }
        assert!(c.moving(1.03, false) && !c.moving(1.08, false));
        assert!(c.aimed());
        // the aim lost halfway back: it shrinks from where it was
        c.update(false, 3.0, false);
        c.update(true, 3.04, false);
        assert_eq!(c.step(3.04, false), 2);
        assert_eq!(c.step(3.12, false), CROSS_STEPS);
        // the same aim again is no change
        c.update(true, 5.0, false);
        assert_eq!(c.step(5.0, false), CROSS_STEPS);
        // reduced motion: a cut
        let mut r = Cross::default();
        r.update(true, 0.0, true);
        assert_eq!(r.step(0.0, true), CROSS_STEPS);
        assert!(!r.moving(0.0, true));
    }

    #[test]
    fn vignette_quantised_to_sixteenths() {
        let rows = [
            (0.0, 0),
            (0.02, 0),
            (0.04, 1),
            (0.5, 8),
            (0.97, 16),
            (1.0, 16),
            (7.0, 16),
            (-1.0, 0),
            (f32::NAN, 0),
        ];
        for (a, want) in rows {
            assert_eq!(vignette_a16(a), want, "{a}");
        }
    }

    fn tag(id: u64) -> (u64, String, [u8; 3], Vec2) {
        (id, format!("t{id}"), [1, 2, 3], Vec2::new(id as f32, 0.0))
    }

    fn levels(v: &[Shown]) -> Vec<(u64, u8)> {
        v.iter().map(|s| (s.id, s.level)).collect()
    }

    #[test]
    fn tags_fade_in_80_and_out_120() {
        let mut f = Fades::default();
        assert_eq!(levels(&f.update(0.0, false, [tag(1)])), [(1, 1)]);
        assert_eq!(f.next_at(0.0, false), Some(0.02));
        assert_eq!(levels(&f.update(0.03, false, [tag(1)])), [(1, 2)]);
        assert_eq!(levels(&f.update(0.08, false, [tag(1)])), [(1, 4)]);
        assert_eq!(f.next_at(0.08, false), None, "at rest: no wish");
        // it leaves its radius: still drawn, fading, at its last place
        let out = f.update(1.0, false, []);
        assert_eq!(levels(&out), [(1, 4)]);
        assert!(!out[0].inside);
        assert_eq!(f.next_at(1.0, false), Some(1.03));
        assert_eq!(levels(&f.update(1.05, false, [])), [(1, 3)]);
        assert_eq!(levels(&f.update(1.10, false, [])), [(1, 1)]);
        assert!(f.update(1.12, false, []).is_empty());
        assert!(f.is_empty(), "forgotten once gone");
        // back inside mid-fade: it rises from where it was
        f.update(2.0, false, [tag(2)]);
        f.update(2.1, false, [tag(2)]);
        f.update(3.0, false, []);
        let back = f.update(3.06, false, [tag(2)]);
        assert_eq!(levels(&back), [(2, 3)]);
        assert!(back[0].inside);
        assert_eq!(levels(&f.update(3.12, false, [tag(2)])), [(2, 4)]);
        // several at once, by id; each its own clock
        let mut g = Fades::default();
        g.update(0.0, false, [tag(5), tag(3)]);
        let v = g.update(0.04, false, [tag(3), tag(4)]);
        assert_eq!(levels(&v), [(3, 3), (4, 1), (5, 2)]);
        assert_eq!(g.len(), 3);
        // the text follows the latest frame
        let mut h = Fades::default();
        h.update(0.0, false, [tag(9)]);
        let v = h.update(0.01, false, [(9, "novo".into(), [9, 9, 9], Vec2::ZERO)]);
        assert_eq!(v[0].text, "novo");
    }

    #[test]
    fn reduced_motion_cuts_tags() {
        let mut f = Fades::default();
        assert_eq!(levels(&f.update(0.0, true, [tag(1)])), [(1, 4)]);
        assert_eq!(f.next_at(0.0, true), None);
        assert!(f.update(0.01, true, []).is_empty());
    }
}
