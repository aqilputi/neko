//! Camera glides (§3.11): short (≤ 260 ms), cubic ease-in-out; `h`/`l` are
//! a lateral travelling (the camera slides, it never turns towards the
//! target); farther than 8 m, or with reduced motion, a cut behind a 120 ms
//! fade to the sky colour instead.

use crate::Time;
use crate::pose::{Pose, angle_diff};

/// Longest glide, seconds.
pub const GLIDE_MAX: f64 = 0.26;
/// Shortest glide (anything that moves at all), seconds.
pub const GLIDE_MIN: f64 = 0.12;
/// Beyond this distance a glide becomes a cut, metres.
pub const CUT_DIST: f32 = 8.0;
/// The whole fade of a cut (out and back in), seconds.
pub const FADE: f64 = 0.12;
/// How fast short glides go (sets their length below `GLIDE_MAX`), m/s.
const GLIDE_SPEED: f32 = 12.0;
/// Metres of glide one radian of turning is worth, for its length.
const TURN_M_PER_RAD: f32 = 0.5;

/// How the camera gets there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Path {
    /// position and orientation eased together
    Glide,
    /// position eased, orientation held (h/l travelling, no yaw)
    Lateral,
    /// fade out, jump, fade in
    Cut,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TweenOpts {
    /// h/l travelling: keep the orientation, slide sideways
    pub lateral: bool,
    /// `--reduced-motion`: always cut
    pub reduced_motion: bool,
    /// longest glide, seconds (the walk exit uses 0.2)
    pub max: f64,
}

impl Default for TweenOpts {
    fn default() -> TweenOpts {
        TweenOpts {
            lateral: false,
            reduced_motion: false,
            max: GLIDE_MAX,
        }
    }
}

/// One camera move, sampled by time.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tween {
    pub from: Pose,
    pub to: Pose,
    pub start: Time,
    /// seconds; 0 when there is nothing to move
    pub dur: f64,
    pub path: Path,
}

/// The camera at one instant of a tween.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    pub pose: Pose,
    /// opacity of the sky-coloured fade over the world (cuts only)
    pub fade: f32,
    /// the tween is over: `pose` is `to`
    pub done: bool,
}

/// Cubic ease-in-out on [0, 1].
pub fn ease_in_out_cubic(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        1.0 - (2.0 - 2.0 * t).powi(3) / 2.0
    }
}

impl Tween {
    /// Plan a move from `from` to `to` starting at `now`.
    pub fn plan(from: Pose, to: Pose, now: Time, opts: TweenOpts) -> Tween {
        let dist = from.pos.distance(to.pos);
        let turn = angle_diff(to.yaw, from.yaw).abs() + (to.pitch - from.pitch).abs();
        let (path, dur) = if dist == 0.0 && turn == 0.0 {
            (Path::Glide, 0.0)
        } else if opts.reduced_motion || dist > CUT_DIST {
            (Path::Cut, FADE)
        } else {
            let len = (dist + turn * TURN_M_PER_RAD) / GLIDE_SPEED;
            let dur = (GLIDE_MIN + len as f64).min(opts.max);
            let path = if opts.lateral {
                Path::Lateral
            } else {
                Path::Glide
            };
            (path, dur)
        };
        Tween {
            from,
            to,
            start: now,
            dur,
            path,
        }
    }

    /// When the move is over.
    pub fn end(&self) -> Time {
        self.start + self.dur
    }

    /// The camera at `now`.
    pub fn sample(&self, now: Time) -> Frame {
        if now >= self.end() {
            return Frame {
                pose: self.to,
                fade: 0.0,
                done: true,
            };
        }
        let t = ((now - self.start) / self.dur).max(0.0) as f32;
        let pose = match self.path {
            Path::Glide => lerp_pose(&self.from, &self.to, ease_in_out_cubic(t)),
            Path::Lateral => Pose {
                pos: self.from.pos.lerp(self.to.pos, ease_in_out_cubic(t)),
                ..self.from
            },
            Path::Cut => {
                let pose = if t < 0.5 { self.from } else { self.to };
                let fade = 1.0 - (2.0 * t - 1.0).abs();
                return Frame {
                    pose,
                    fade,
                    done: false,
                };
            }
        };
        Frame {
            pose,
            fade: 0.0,
            done: false,
        }
    }
}

/// Linear interpolation of positions and shortest-way interpolation of yaw.
fn lerp_pose(a: &Pose, b: &Pose, t: f32) -> Pose {
    Pose {
        pos: a.pos.lerp(b.pos, t),
        yaw: a.yaw + angle_diff(b.yaw, a.yaw) * t,
        pitch: a.pitch + (b.pitch - a.pitch) * t,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;

    fn at(x: f32, z: f32, yaw: f32) -> Pose {
        Pose::new(Vec3::new(x, 1.5, z), yaw, 0.0)
    }

    #[test]
    fn ease_is_cubic_and_symmetric() {
        assert_eq!(ease_in_out_cubic(0.0), 0.0);
        assert_eq!(ease_in_out_cubic(1.0), 1.0);
        assert!((ease_in_out_cubic(0.5) - 0.5).abs() < 1e-6);
        assert!((ease_in_out_cubic(0.25) - 0.0625).abs() < 1e-6);
        assert!((ease_in_out_cubic(0.25) + ease_in_out_cubic(0.75) - 1.0).abs() < 1e-6);
        assert_eq!(ease_in_out_cubic(-1.0), 0.0);
        assert_eq!(ease_in_out_cubic(2.0), 1.0);
    }

    #[test]
    fn glides_are_short_and_end_exactly_on_target() {
        for d in [0.05, 0.5, 2.4, 7.9] {
            let tw = Tween::plan(
                at(0.0, 0.0, 0.0),
                at(d, -1.0, 0.3),
                10.0,
                TweenOpts::default(),
            );
            assert_eq!(tw.path, Path::Glide);
            assert!(
                tw.dur <= GLIDE_MAX && tw.dur >= GLIDE_MIN,
                "{d}: {}",
                tw.dur
            );
            let end = tw.sample(tw.end());
            assert!(end.done);
            assert_eq!(end.pose, tw.to);
            assert_eq!(end.fade, 0.0);
            assert!(!tw.sample(10.0 + tw.dur / 2.0).done);
        }
        let short = Tween::plan(
            at(0.0, 0.0, 0.0),
            at(0.2, 0.0, 0.0),
            0.0,
            TweenOpts::default(),
        );
        let long = Tween::plan(
            at(0.0, 0.0, 0.0),
            at(6.0, 0.0, 0.0),
            0.0,
            TweenOpts::default(),
        );
        assert!(short.dur < long.dur, "short moves are quicker");
    }

    #[test]
    fn glide_follows_the_ease() {
        let tw = Tween::plan(
            at(0.0, 0.0, 0.0),
            at(4.0, 0.0, 0.0),
            0.0,
            TweenOpts::default(),
        );
        let q = tw.sample(tw.dur * 0.25).pose.pos.x;
        assert!((q - 4.0 * 0.0625).abs() < 1e-4, "{q}");
        assert_eq!(
            tw.sample(-1.0).pose,
            tw.from,
            "before the start: still at from"
        );
    }

    #[test]
    fn glide_turns_the_short_way() {
        let tw = Tween::plan(
            at(0.0, 0.0, 3.0),
            at(0.0, 0.0, -3.0),
            0.0,
            TweenOpts::default(),
        );
        let mid = tw.sample(tw.dur / 2.0).pose.yaw;
        assert!(mid.abs() > 3.0, "through π, not through 0: {mid}");
    }

    #[test]
    fn lateral_travelling_never_turns() {
        let opts = TweenOpts {
            lateral: true,
            ..TweenOpts::default()
        };
        let tw = Tween::plan(at(0.0, 0.0, 0.2), at(3.0, 0.0, 0.5), 0.0, opts);
        assert_eq!(tw.path, Path::Lateral);
        for k in 1..10 {
            let f = tw.sample(tw.dur * k as f64 / 10.0);
            assert_eq!(f.pose.yaw, 0.2);
        }
        assert_eq!(tw.sample(tw.end()).pose, tw.to);
    }

    #[test]
    fn far_moves_cut_behind_a_fade() {
        let tw = Tween::plan(
            at(0.0, 0.0, 0.0),
            at(8.5, 0.0, 0.0),
            0.0,
            TweenOpts::default(),
        );
        assert_eq!(tw.path, Path::Cut);
        assert_eq!(tw.dur, FADE);
        let early = tw.sample(FADE * 0.25);
        assert_eq!(early.pose, tw.from);
        assert!((early.fade - 0.5).abs() < 1e-4);
        let peak = tw.sample(FADE * 0.5);
        assert_eq!(peak.fade, 1.0);
        let late = tw.sample(FADE * 0.75);
        assert_eq!(late.pose, tw.to, "jumped behind the fade");
        assert!((late.fade - 0.5).abs() < 1e-4);
        let end = tw.sample(FADE);
        assert!(end.done && end.fade == 0.0);
    }

    #[test]
    fn reduced_motion_always_cuts() {
        let opts = TweenOpts {
            reduced_motion: true,
            ..TweenOpts::default()
        };
        let tw = Tween::plan(at(0.0, 0.0, 0.0), at(0.5, 0.0, 0.0), 0.0, opts);
        assert_eq!(tw.path, Path::Cut);
        let opts = TweenOpts {
            reduced_motion: true,
            lateral: true,
            ..TweenOpts::default()
        };
        assert_eq!(
            Tween::plan(at(0.0, 0.0, 0.0), at(1.0, 0.0, 0.0), 0.0, opts).path,
            Path::Cut
        );
    }

    #[test]
    fn nothing_to_move_is_done_at_once() {
        let tw = Tween::plan(
            at(1.0, 2.0, 0.3),
            at(1.0, 2.0, 0.3),
            5.0,
            TweenOpts::default(),
        );
        assert_eq!(tw.dur, 0.0);
        assert!(tw.sample(5.0).done);
    }

    #[test]
    fn a_shorter_cap_is_honoured() {
        let opts = TweenOpts {
            max: 0.2,
            ..TweenOpts::default()
        };
        let tw = Tween::plan(at(0.0, 0.0, 0.0), at(5.0, 0.0, 0.0), 0.0, opts);
        assert!(tw.dur <= 0.2);
    }
}
