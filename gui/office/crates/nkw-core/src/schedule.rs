//! The frame scheduler (§5.6): whoever needs a frame states a `Want`; at
//! each host frame the scheduler says whether to run the world pass, whether
//! to compose, and when to wake up next. Everything due in one frame is
//! drawn together; `Rate` frames sit on the vblank grid, so different rates
//! share frames; with no wants there is no wakeup at all (the event loop
//! sleeps on its fds).

use crate::Time;

/// Slack for float comparisons, in frames (or rate ticks).
const EPS: f64 = 1e-6;

/// How often something needs a frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Want {
    /// every host frame (camera gliding, walking with a key held)
    Continuous,
    /// `hz` frames a second, on the vblank grid (cats 15, walkers 30,
    /// dialogue text 30)
    Rate(f32),
    /// one frame, now (HUD, hover, a screen commit, theme)
    Once,
    /// one frame at that instant (toast end, time of day)
    At(Time),
}

/// Which pass a want needs: the world pass implies composing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pass {
    /// redraw the 3D world, then compose
    World,
    /// only recompose (a FOCO client's commit, the HUD)
    Compose,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Wish {
    pub want: Want,
    pub pass: Pass,
}

impl Wish {
    pub fn world(want: Want) -> Wish {
        Wish {
            want,
            pass: Pass::World,
        }
    }

    pub fn compose(want: Want) -> Wish {
        Wish {
            want,
            pass: Pass::Compose,
        }
    }
}

/// The host's frame grid: frames at `epoch + k·period`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vblank {
    pub epoch: Time,
    /// seconds per frame (1/60, 1/144 …)
    pub period: f64,
}

impl Vblank {
    /// Index of the frame nearest to `t`.
    pub fn index(&self, t: Time) -> i64 {
        ((t - self.epoch) / self.period).round() as i64
    }

    /// Time of frame `k`.
    pub fn at(&self, k: i64) -> Time {
        self.epoch + k as f64 * self.period
    }

    /// The first frame at or after `t`.
    pub fn at_or_after(&self, t: Time) -> Time {
        self.at(((t - self.epoch) / self.period - EPS).ceil() as i64)
    }
}

/// Whether a `hz` rate has a tick in frame `k`'s slot, ((k-1)·P, k·P].
fn rate_due(grid: &Vblank, hz: f64, k: i64) -> bool {
    let ticks = |k: i64| (k as f64 * grid.period * hz + EPS).floor();
    ticks(k) > ticks(k - 1)
}

/// The first frame after `k` that has a tick of a `hz` rate.
fn rate_next(grid: &Vblank, hz: f64, k: i64) -> Time {
    let next_tick = (k as f64 * grid.period * hz + EPS).floor() + 1.0;
    grid.at_or_after(grid.epoch + next_tick / hz)
}

/// What to do this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Plan {
    pub world_pass: bool,
    pub compose: bool,
    /// when to wake up for the next frame; `None`: no timer at all
    pub next_wakeup: Option<Time>,
}

/// The plan for the frame at `now` (a vblank of `grid`, or a wakeup this
/// scheduler asked for), given everything that currently wants frames.
/// `Once` wants are due now; the caller drops them after drawing, as it
/// drops `At` wants that are past.
pub fn plan(grid: &Vblank, now: Time, wishes: &[Wish]) -> Plan {
    let k = grid.index(now);
    let mut p = Plan {
        world_pass: false,
        compose: false,
        next_wakeup: None,
    };
    for w in wishes {
        let (due, next) = match w.want {
            Want::Continuous => (true, Some(grid.at(k + 1))),
            Want::Once => (true, None),
            Want::Rate(hz) => {
                let hz = f64::from(hz);
                (rate_due(grid, hz, k), Some(rate_next(grid, hz, k)))
            }
            Want::At(t) if t <= now + EPS * grid.period => (true, None),
            Want::At(t) => (false, Some(t)),
        };
        if due {
            p.compose = true;
            p.world_pass |= w.pass == Pass::World;
        }
        if let Some(t) = next {
            p.next_wakeup = Some(p.next_wakeup.map_or(t, |n: Time| n.min(t)));
        }
    }
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    const HZ60: Vblank = Vblank {
        epoch: 0.0,
        period: 1.0 / 60.0,
    };

    fn frame(k: i64) -> Time {
        HZ60.at(k)
    }

    #[test]
    fn nothing_wanted_means_no_timer() {
        let p = plan(&HZ60, 1.0, &[]);
        assert_eq!(
            p,
            Plan {
                world_pass: false,
                compose: false,
                next_wakeup: None
            }
        );
    }

    #[test]
    fn continuous_draws_every_frame() {
        let p = plan(&HZ60, frame(10), &[Wish::world(Want::Continuous)]);
        assert!(p.world_pass && p.compose);
        assert!((p.next_wakeup.unwrap() - frame(11)).abs() < 1e-9);
    }

    #[test]
    fn once_draws_now_and_asks_no_wakeup() {
        let p = plan(&HZ60, frame(3), &[Wish::compose(Want::Once)]);
        assert!(p.compose && !p.world_pass, "a HUD change only recomposes");
        assert_eq!(p.next_wakeup, None);
    }

    #[test]
    fn rate_15_draws_one_frame_in_four_on_the_grid() {
        let w = [Wish::world(Want::Rate(15.0))];
        let drawn: Vec<i64> = (0..12)
            .filter(|k| plan(&HZ60, frame(*k), &w).world_pass)
            .collect();
        assert_eq!(drawn, [0, 4, 8]);
        let p = plan(&HZ60, frame(1), &w);
        assert!(!p.compose);
        assert!((p.next_wakeup.unwrap() - frame(4)).abs() < 1e-9);
        let p = plan(&HZ60, frame(4), &w);
        assert!((p.next_wakeup.unwrap() - frame(8)).abs() < 1e-9);
    }

    #[test]
    fn rates_that_do_not_divide_the_refresh_snap_up_to_a_vblank() {
        // 24 Hz on 60 Hz: ticks at 0, 2.5, 5, 7.5 frames → frames 0, 3, 5, 8
        let w = [Wish::world(Want::Rate(24.0))];
        let drawn: Vec<i64> = (0..10)
            .filter(|k| plan(&HZ60, frame(*k), &w).world_pass)
            .collect();
        assert_eq!(drawn, [0, 3, 5, 8]);
        let p = plan(&HZ60, frame(5), &w);
        assert!((p.next_wakeup.unwrap() - frame(8)).abs() < 1e-9);
    }

    #[test]
    fn different_rates_coalesce_on_shared_frames() {
        let w = [
            Wish::world(Want::Rate(15.0)),
            Wish::world(Want::Rate(30.0)),
            Wish::compose(Want::At(frame(2) - 0.004)),
        ];
        // at frame 1 the 30 Hz wants frame 2 and the toast is due just before
        // it: one wakeup, the earliest
        let p = plan(&HZ60, frame(1), &w);
        assert!((p.next_wakeup.unwrap() - (frame(2) - 0.004)).abs() < 1e-9);
        let p = plan(&HZ60, frame(4), &w[..2]);
        assert!(p.world_pass, "15 and 30 Hz share frame 4");
        assert!((p.next_wakeup.unwrap() - frame(6)).abs() < 1e-9);
    }

    #[test]
    fn at_fires_at_its_instant() {
        let w = [Wish::compose(Want::At(2.5))];
        let p = plan(&HZ60, 1.0, &w);
        assert!(!p.compose);
        assert_eq!(p.next_wakeup, Some(2.5));
        let p = plan(&HZ60, 2.5, &w);
        assert!(p.compose && !p.world_pass);
        assert_eq!(p.next_wakeup, None, "past At: the caller drops it");
    }

    #[test]
    fn world_and_compose_mix() {
        let w = [Wish::compose(Want::Once), Wish::world(Want::Rate(15.0))];
        let p = plan(&HZ60, frame(1), &w);
        assert!(p.compose && !p.world_pass, "only the compose want is due");
        let p = plan(&HZ60, frame(4), &w);
        assert!(p.world_pass && p.compose);
    }

    #[test]
    fn grid_helpers() {
        let g = Vblank {
            epoch: 0.5,
            period: 0.01,
        };
        assert_eq!(g.index(0.5304), 3);
        assert!((g.at_or_after(0.5301) - 0.54).abs() < 1e-9);
        assert!((g.at_or_after(0.53) - 0.53).abs() < 1e-9);
        assert!(
            (g.at_or_after(0.0) - 0.0).abs() < 1e-9,
            "before the epoch too"
        );
    }
}
