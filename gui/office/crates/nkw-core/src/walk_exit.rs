//! Leaving ANDAR (§3.5, `walk.on_exit`): glide onto a nearby station, or
//! stay put as a `Free` station ("you are here").

use glam::Vec3Swizzles;
use serde::{Deserialize, Serialize};

use crate::pose::{Pose, angle_diff};
use crate::strip::{Spot, Station};

/// `auto` snaps to a station at most this far away, metres (on the floor).
pub const SNAP_DIST: f32 = 1.5;
/// … and at most this far off your heading, degrees.
pub const SNAP_ANGLE: f32 = 40.0;
/// The exit glide is a little shorter than the others, seconds (use it as
/// `TweenOpts::max`).
pub const EXIT_GLIDE: f64 = 0.2;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExitRule {
    /// a station close by and roughly ahead, else stay
    #[default]
    Auto,
    /// always the nearest station (a cut if it is beyond 8 m)
    Snap,
    /// never snap
    Stay,
}

/// Where NAV starts after ANDAR: the station and the camera pose to glide
/// to (for `Free`, the pose you are at).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ExitTo {
    pub station: Station,
    pub pose: Pose,
}

/// Apply `rule` at `here` with the room's `spots`.
pub fn exit(rule: ExitRule, here: &Pose, spots: &[Spot]) -> ExitTo {
    let floor_dist = |s: &Spot| here.pos.xz().distance(s.pose.pos.xz());
    let ahead = |s: &Spot| angle_diff(s.pose.yaw, here.yaw).abs() <= SNAP_ANGLE.to_radians();
    let candidates = spots.iter().filter(|s| match rule {
        ExitRule::Auto => floor_dist(s) <= SNAP_DIST && ahead(s),
        ExitRule::Snap => true,
        ExitRule::Stay => false,
    });
    match candidates.min_by(|a, b| floor_dist(a).total_cmp(&floor_dist(b))) {
        Some(s) => ExitTo {
            station: s.station,
            pose: s.pose,
        },
        None => ExitTo {
            station: Station::Free(*here),
            pose: *here,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{RoomId, ScreenId};
    use glam::Vec3;

    fn spot(x: f32, z: f32, yaw_deg: f32, id: u32) -> Spot {
        Spot {
            station: Station::Air(ScreenId(id)),
            pose: Pose::new(Vec3::new(x, 1.5, z), yaw_deg.to_radians(), 0.0),
            stack: 1,
        }
    }

    fn me(x: f32, z: f32, yaw_deg: f32) -> Pose {
        Pose::new(Vec3::new(x, 1.5, z), yaw_deg.to_radians(), 0.1)
    }

    #[test]
    fn auto_glides_to_a_close_station_ahead() {
        let spots = [spot(0.0, 0.0, 0.0, 1), spot(1.0, 0.5, 10.0, 2)];
        let out = exit(ExitRule::Auto, &me(1.2, 0.0, 20.0), &spots);
        assert_eq!(
            out.station,
            Station::Air(ScreenId(2)),
            "the nearest that fits"
        );
        assert_eq!(out.pose, spots[1].pose);
    }

    #[test]
    fn auto_stays_free_when_too_far_or_turned_away() {
        let spots = [spot(0.0, 0.0, 0.0, 1)];
        let far = me(1.6, 0.0, 0.0);
        assert_eq!(
            exit(ExitRule::Auto, &far, &spots).station,
            Station::Free(far)
        );
        let turned = me(0.5, 0.0, 45.0);
        let out = exit(ExitRule::Auto, &turned, &spots);
        assert_eq!(out.station, Station::Free(turned));
        assert_eq!(out.pose, turned);
        // 39° across the ±π seam still counts as close
        let spots = [spot(0.0, 0.0, 175.0, 3)];
        let seam = me(0.3, 0.0, -146.0);
        assert_eq!(
            exit(ExitRule::Auto, &seam, &spots).station,
            Station::Air(ScreenId(3))
        );
    }

    #[test]
    fn auto_ignores_height_for_distance() {
        let mut s = spot(0.0, 0.0, 0.0, 1);
        s.pose.pos.y = 3.0;
        let out = exit(ExitRule::Auto, &me(1.4, 0.0, 0.0), &[s]);
        assert_eq!(out.station, Station::Air(ScreenId(1)));
    }

    #[test]
    fn snap_always_takes_the_nearest() {
        let spots = [
            Spot {
                station: Station::Door(RoomId(4)),
                ..spot(30.0, 0.0, 90.0, 0)
            },
            spot(12.0, 0.0, 180.0, 2),
        ];
        let out = exit(ExitRule::Snap, &me(0.0, 0.0, 0.0), &spots);
        assert_eq!(out.station, Station::Air(ScreenId(2)));
        let here = me(0.0, 0.0, 0.0);
        assert_eq!(
            exit(ExitRule::Snap, &here, &[]).station,
            Station::Free(here)
        );
    }

    #[test]
    fn stay_never_snaps() {
        let spots = [spot(0.0, 0.0, 0.0, 1)];
        let here = me(0.1, 0.0, 0.0);
        assert_eq!(
            exit(ExitRule::Stay, &here, &spots).station,
            Station::Free(here)
        );
    }

    #[test]
    fn rule_reads_from_config() {
        let r: ExitRule = serde_json::from_str("\"snap\"").unwrap();
        assert_eq!(r, ExitRule::Snap);
        assert_eq!(ExitRule::default(), ExitRule::Auto);
        let _ = angle_diff(0.0, 0.0);
    }
}
