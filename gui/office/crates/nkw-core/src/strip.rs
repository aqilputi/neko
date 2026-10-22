//! The strip of a room and the stations of NAV (§3.4, §4.3, §4.4).
//!
//! Each room is a niri strip of desks along its back wall, growing to the
//! right; a desk's width is a preset of the output's usable width and its
//! screens split the usable height. Rooms sit along the hall, doors
//! alternating sides. Stations are where the NAV camera stops; `h`/`l` go
//! along the room, `j`/`k` along a desk's stack.

use glam::Vec2;
use serde::{Deserialize, Serialize};

use crate::ids::{CatId, DirId, RoomId, ScreenId};
use crate::onetoone::M_PER_PX;
use crate::pose::Pose;

/// Margin taken off a desk's preset width, logical px.
pub const MARGIN_PX: f32 = 16.0;
/// Space between two desks (elbow room, the cat's chair), metres.
pub const DESK_GAP: f32 = 0.8;
/// Space between two rooms on the same side of the hall, metres.
pub const ROOM_GAP: f32 = 0.6;
/// At most this many screens of a desk are visible; the rest pile behind.
pub const STACK_VISIBLE: usize = 3;

/// A desk's width, as a share of the usable output width.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Width {
    Third,
    #[default]
    Half,
    TwoThirds,
    Full,
}

impl Width {
    pub fn fraction(self) -> f32 {
        self.share(1.0)
    }

    /// This preset's share of `total` (exact for whole pixels).
    fn share(self, total: f32) -> f32 {
        match self {
            Width::Third => total / 3.0,
            Width::Half => total / 2.0,
            Width::TwoThirds => total * 2.0 / 3.0,
            Width::Full => total,
        }
    }

    /// `-`: one preset narrower (stays at a third).
    pub fn narrower(self) -> Width {
        match self {
            Width::Third | Width::Half => Width::Third,
            Width::TwoThirds => Width::Half,
            Width::Full => Width::TwoThirds,
        }
    }

    /// `=`: one preset wider (stays at full).
    pub fn wider(self) -> Width {
        match self {
            Width::Third => Width::Half,
            Width::Half => Width::TwoThirds,
            Width::TwoThirds | Width::Full => Width::Full,
        }
    }
}

/// The logical size (px) of each visible screen of a desk: the preset
/// share of `usable` width less the margin, and the usable height split
/// among the visible screens (1 to 3). It is also the FOCO size.
pub fn screen_size_px(width: Width, usable: Vec2, stack: usize) -> Vec2 {
    let visible = stack.clamp(1, STACK_VISIBLE) as f32;
    Vec2::new(width.share(usable.x) - MARGIN_PX, usable.y / visible)
}

/// x of each desk's centre, metres: desks `widths` metres wide, `gap`
/// apart, the first one's left edge at 0.
pub fn desk_xs(widths: &[f32], gap: f32) -> Vec<f32> {
    let mut left = 0.0;
    widths
        .iter()
        .map(|w| {
            let x = left + w / 2.0;
            left += w + gap;
            x
        })
        .collect()
}

/// The metres a desk of `width` takes, for `desk_xs`.
pub fn desk_width_m(width: Width, usable: Vec2) -> f32 {
    screen_size_px(width, usable, 1).x * M_PER_PX
}

/// Which side of the hall a room opens on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    North,
    South,
}

/// Where a room sits along the hall (x).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RoomSlot {
    pub side: Side,
    /// left end along the hall
    pub x0: f32,
    pub len: f32,
    /// centre of its door along the hall
    pub door_x: f32,
}

/// Rooms of `lens` metres along the hall, in order, alternating sides
/// (north first), each side packed left to right `ROOM_GAP` apart. The
/// door is in the middle of the room's hall wall.
pub fn rooms_along_hall(lens: &[f32]) -> Vec<RoomSlot> {
    let mut next = [0.0f32; 2];
    lens.iter()
        .enumerate()
        .map(|(i, &len)| {
            let x0 = next[i % 2];
            next[i % 2] += len + ROOM_GAP;
            RoomSlot {
                side: if i % 2 == 0 { Side::North } else { Side::South },
                x0,
                len,
                door_x: x0 + len / 2.0,
            }
        })
        .collect()
}

/// A camera stop of NAV (§3.4). Serialised in snake_case (`{"desk": {…}}`,
/// `{"free": {"pos": […], …}}`, `"board"`), the shape of `world.dump()`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Station {
    /// at a desk, facing its stack; `slot` is the selected screen
    Desk {
        room: RoomId,
        desk: usize,
        slot: usize,
    },
    /// a screen in the air
    Air(ScreenId),
    /// a library shelf
    Shelf { dir: DirId, row: u32 },
    /// a room's door
    Door(RoomId),
    /// the reception's notice board
    Board,
    /// where ANDAR was left ("you are here"); forgotten on the next move
    Free(Pose),
}

impl Station {
    /// The same place, whatever screen of a desk is selected.
    pub fn same_place(&self, other: &Station) -> bool {
        match (self, other) {
            (
                Station::Desk { room, desk, .. },
                Station::Desk {
                    room: r2, desk: d2, ..
                },
            ) => room == r2 && desk == d2,
            _ => self == other,
        }
    }
}

/// A station of the current room with its camera pose and, for desks, how
/// many screens its stack has. A room's spots are in `h`/`l` order.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Spot {
    pub station: Station,
    pub pose: Pose,
    pub stack: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dir {
    /// h
    Left,
    /// l
    Right,
    /// k: the screen above in the stack
    Up,
    /// j: the screen below
    Down,
}

/// Where `dir` goes from `cur` among the room's `spots`; `None` when there
/// is nowhere to go. Onto a desk, the top screen is selected. From `Free`,
/// `h`/`l` go to the nearest station to the left/right of the view.
pub fn neighbour(spots: &[Spot], cur: &Station, dir: Dir) -> Option<Station> {
    if let Station::Free(here) = cur {
        let sign = match dir {
            Dir::Left => -1.0,
            Dir::Right => 1.0,
            Dir::Up | Dir::Down => return None,
        };
        return spots
            .iter()
            .filter(|s| (s.pose.pos - here.pos).dot(here.right()) * sign > 0.0)
            .min_by(|a, b| {
                let da = a.pose.pos.distance_squared(here.pos);
                da.total_cmp(&b.pose.pos.distance_squared(here.pos))
            })
            .map(|s| top_of(s.station));
    }
    let i = spots.iter().position(|s| s.station.same_place(cur))?;
    match (dir, cur) {
        (Dir::Left, _) => i.checked_sub(1).map(|j| top_of(spots[j].station)),
        (Dir::Right, _) => spots.get(i + 1).map(|s| top_of(s.station)),
        (Dir::Up, Station::Desk { room, desk, slot }) if *slot > 0 => Some(Station::Desk {
            room: *room,
            desk: *desk,
            slot: slot - 1,
        }),
        (Dir::Down, Station::Desk { room, desk, slot }) if slot + 1 < spots[i].stack => {
            Some(Station::Desk {
                room: *room,
                desk: *desk,
                slot: slot + 1,
            })
        }
        _ => None,
    }
}

/// A station as arrived at: a desk with its top screen selected.
fn top_of(s: Station) -> Station {
    match s {
        Station::Desk { room, desk, .. } => Station::Desk {
            room,
            desk,
            slot: 0,
        },
        other => other,
    }
}

/// A desk of a room's strip: a column of screens, maybe a cat's.
#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
pub struct Desk {
    pub width: Width,
    pub screens: Vec<ScreenId>,
    pub cat: Option<CatId>,
}

/// Bring a room's desks in line with its cats (`sessions`, in tree order):
/// a new cat's desk goes right after the desk of the cat before it in the
/// order (before the first cat desk if it is first), without moving user
/// desks around; a cat that left takes only itself: its desk stays as a
/// user desk if it has screens of yours, and goes otherwise.
pub fn reconcile(desks: &[Desk], sessions: &[CatId]) -> Vec<Desk> {
    let mut out: Vec<Desk> = desks
        .iter()
        .filter_map(|d| match d.cat {
            Some(c) if !sessions.contains(&c) => (!d.screens.is_empty()).then(|| Desk {
                cat: None,
                ..d.clone()
            }),
            _ => Some(d.clone()),
        })
        .collect();
    let desk_of = |out: &[Desk], c: CatId| out.iter().position(|d| d.cat == Some(c));
    for (k, &cat) in sessions.iter().enumerate() {
        if desk_of(&out, cat).is_some() {
            continue;
        }
        let after_prev = sessions[..k]
            .iter()
            .rev()
            .find_map(|&p| desk_of(&out, p))
            .map(|i| i + 1);
        let at = after_prev
            .or_else(|| out.iter().position(|d| d.cat.is_some()))
            .unwrap_or(out.len());
        out.insert(
            at,
            Desk {
                cat: Some(cat),
                ..Desk::default()
            },
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;

    const USABLE: Vec2 = Vec2::new(1920.0, 1040.0);

    #[test]
    fn presets_step_and_stop_at_the_ends() {
        assert_eq!(Width::Third.fraction(), 1.0 / 3.0);
        assert_eq!(Width::Half.fraction(), 0.5);
        assert_eq!(Width::TwoThirds.fraction(), 2.0 / 3.0);
        assert_eq!(Width::Full.fraction(), 1.0);
        assert_eq!(Width::Half.wider(), Width::TwoThirds);
        assert_eq!(Width::TwoThirds.wider(), Width::Full);
        assert_eq!(Width::Full.wider(), Width::Full);
        assert_eq!(Width::Half.narrower(), Width::Third);
        assert_eq!(Width::Third.narrower(), Width::Third);
        assert_eq!(Width::Full.narrower(), Width::TwoThirds);
        assert_eq!(Width::default(), Width::Half);
    }

    #[test]
    fn screen_sizes_follow_the_presets_and_the_stack() {
        assert_eq!(
            screen_size_px(Width::Half, USABLE, 1),
            Vec2::new(944.0, 1040.0)
        );
        assert_eq!(
            screen_size_px(Width::Full, USABLE, 2),
            Vec2::new(1904.0, 520.0)
        );
        let third = screen_size_px(Width::Third, USABLE, 3);
        assert_eq!(third, Vec2::new(624.0, 1040.0 / 3.0));
        assert_eq!(
            screen_size_px(Width::Third, USABLE, 7),
            third,
            "only 3 visible"
        );
        assert_eq!(screen_size_px(Width::Half, USABLE, 0).y, 1040.0);
        assert!((desk_width_m(Width::Half, USABLE) - 0.944).abs() < 1e-6);
    }

    #[test]
    fn desks_sit_side_by_side_with_gaps() {
        let xs = desk_xs(&[1.0, 2.0, 0.5], 0.8);
        let want = [0.5, 1.0 + 0.8 + 1.0, 1.0 + 0.8 + 2.0 + 0.8 + 0.25];
        for (x, w) in xs.iter().zip(want) {
            assert!((x - w).abs() < 1e-6, "{xs:?}");
        }
        assert!(desk_xs(&[], 0.8).is_empty());
    }

    #[test]
    fn rooms_alternate_sides_along_the_hall() {
        let r = rooms_along_hall(&[6.0, 4.0, 3.0, 5.0]);
        assert_eq!(r.len(), 4);
        assert_eq!(
            r.iter().map(|s| s.side).collect::<Vec<_>>(),
            [Side::North, Side::South, Side::North, Side::South]
        );
        assert_eq!((r[0].x0, r[1].x0), (0.0, 0.0));
        assert!((r[2].x0 - 6.6).abs() < 1e-6);
        assert!((r[3].x0 - 4.6).abs() < 1e-6);
        assert!((r[2].door_x - 8.1).abs() < 1e-6);
        assert_eq!(r[1].len, 4.0);
    }

    fn pose_at(x: f32) -> Pose {
        Pose::new(Vec3::new(x, 1.5, 2.4), 0.0, 0.0)
    }

    fn desk(desk: usize, slot: usize) -> Station {
        Station::Desk {
            room: RoomId(1),
            desk,
            slot,
        }
    }

    /// desk 0 (2 screens), a screen in the air, desk 1 (3 screens), a door
    fn spots() -> Vec<Spot> {
        vec![
            Spot {
                station: desk(0, 0),
                pose: pose_at(0.0),
                stack: 2,
            },
            Spot {
                station: Station::Air(ScreenId(9)),
                pose: pose_at(1.5),
                stack: 1,
            },
            Spot {
                station: desk(1, 0),
                pose: pose_at(3.0),
                stack: 3,
            },
            Spot {
                station: Station::Door(RoomId(2)),
                pose: pose_at(5.0),
                stack: 0,
            },
        ]
    }

    #[test]
    fn h_and_l_go_along_the_room() {
        let s = spots();
        assert_eq!(
            neighbour(&s, &desk(0, 1), Dir::Right),
            Some(Station::Air(ScreenId(9)))
        );
        assert_eq!(
            neighbour(&s, &Station::Air(ScreenId(9)), Dir::Right),
            Some(desk(1, 0))
        );
        assert_eq!(
            neighbour(&s, &desk(1, 2), Dir::Left),
            Some(Station::Air(ScreenId(9)))
        );
        assert_eq!(neighbour(&s, &desk(0, 0), Dir::Left), None, "first: stays");
        assert_eq!(neighbour(&s, &Station::Door(RoomId(2)), Dir::Right), None);
        assert_eq!(
            neighbour(&s, &Station::Board, Dir::Right),
            None,
            "not in this room"
        );
    }

    #[test]
    fn j_and_k_go_along_the_stack() {
        let s = spots();
        assert_eq!(neighbour(&s, &desk(1, 0), Dir::Down), Some(desk(1, 1)));
        assert_eq!(neighbour(&s, &desk(1, 2), Dir::Down), None, "bottom");
        assert_eq!(neighbour(&s, &desk(1, 2), Dir::Up), Some(desk(1, 1)));
        assert_eq!(neighbour(&s, &desk(0, 0), Dir::Up), None, "top");
        assert_eq!(neighbour(&s, &Station::Air(ScreenId(9)), Dir::Down), None);
    }

    #[test]
    fn from_free_h_and_l_follow_the_view() {
        let s = spots();
        // standing at x = 2.2, looking down -z: left is the air screen
        let here = Station::Free(Pose::new(Vec3::new(2.2, 1.5, 4.0), 0.0, 0.0));
        assert_eq!(
            neighbour(&s, &here, Dir::Left),
            Some(Station::Air(ScreenId(9)))
        );
        assert_eq!(neighbour(&s, &here, Dir::Right), Some(desk(1, 0)));
        assert_eq!(neighbour(&s, &here, Dir::Up), None);
        // turned around (looking +z), left and right swap
        let back = Station::Free(Pose::new(
            Vec3::new(2.2, 1.5, 4.0),
            std::f32::consts::PI,
            0.0,
        ));
        assert_eq!(neighbour(&s, &back, Dir::Left), Some(desk(1, 0)));
        // nothing to the right of the last one
        let end = Station::Free(Pose::new(Vec3::new(9.0, 1.5, 2.4), 0.0, 0.0));
        assert_eq!(neighbour(&s, &end, Dir::Right), None);
    }

    #[test]
    fn same_place_ignores_the_slot() {
        assert!(desk(1, 0).same_place(&desk(1, 2)));
        assert!(!desk(1, 0).same_place(&desk(0, 0)));
        assert!(Station::Board.same_place(&Station::Board));
        assert!(!Station::Board.same_place(&Station::Door(RoomId(1))));
    }

    fn cat_desk(c: u32) -> Desk {
        Desk {
            cat: Some(CatId(c)),
            ..Desk::default()
        }
    }

    fn user_desk(s: u32) -> Desk {
        Desk {
            screens: vec![ScreenId(s)],
            ..Desk::default()
        }
    }

    fn layout(d: &[Desk]) -> Vec<String> {
        d.iter()
            .map(|d| match (d.cat, d.screens.first()) {
                (Some(c), _) => format!("c{}", c.0),
                (None, Some(s)) => format!("u{}", s.0),
                (None, None) => "-".into(),
            })
            .collect()
    }

    #[test]
    fn a_new_cat_goes_after_the_previous_cat_without_pushing_user_desks() {
        let desks = [cat_desk(1), user_desk(10), cat_desk(3), user_desk(11)];
        let out = reconcile(&desks, &[CatId(1), CatId(2), CatId(3)]);
        assert_eq!(layout(&out), ["c1", "c2", "u10", "c3", "u11"]);
        assert_eq!(out[1].width, Width::Half);
        let out = reconcile(&desks, &[CatId(0), CatId(1), CatId(3), CatId(4)]);
        assert_eq!(layout(&out), ["c0", "c1", "u10", "c3", "c4", "u11"]);
        let out = reconcile(&[user_desk(10)], &[CatId(5)]);
        assert_eq!(layout(&out), ["u10", "c5"], "no cat desks yet: at the end");
    }

    #[test]
    fn a_cat_that_leaves_takes_only_itself() {
        let mut mine = cat_desk(2);
        mine.screens.push(ScreenId(20));
        let desks = [cat_desk(1), mine, user_desk(10), cat_desk(3)];
        let out = reconcile(&desks, &[CatId(3)]);
        assert_eq!(
            layout(&out),
            ["u20", "u10", "c3"],
            "the desk stays, the cat goes"
        );
        assert_eq!(out[0].cat, None);
    }

    #[test]
    fn serde_round_trips() {
        let stations = [
            desk(1, 2),
            Station::Desk {
                room: RoomId(1),
                desk: 2,
                slot: 0,
            },
            Station::Air(ScreenId(9)),
            Station::Shelf {
                dir: DirId(3),
                row: 1,
            },
            Station::Door(RoomId(2)),
            Station::Board,
            Station::Free(Pose::new(Vec3::new(1.0, 1.5, -2.25), 0.5, -0.1)),
        ];
        for st in stations {
            let j = serde_json::to_string(&st).unwrap();
            assert_eq!(serde_json::from_str::<Station>(&j).unwrap(), st, "{j}");
        }
        assert_eq!(
            serde_json::to_value(desk(1, 2)).unwrap(),
            serde_json::json!({"desk": {"room": 1, "desk": 1, "slot": 2}})
        );
        assert_eq!(
            serde_json::to_value(Station::Free(Pose::new(Vec3::X, 0.25, 0.5))).unwrap(),
            serde_json::json!({"free": {"pos": [1.0, 0.0, 0.0], "yaw": 0.25, "pitch": 0.5}})
        );
        assert_eq!(serde_json::to_value(Station::Board).unwrap(), "board");
        let mut d = user_desk(4);
        d.cat = Some(CatId(2));
        d.width = Width::TwoThirds;
        let j = serde_json::to_string(&d).unwrap();
        assert_eq!(serde_json::from_str::<Desk>(&j).unwrap(), d);
        assert!(j.contains("\"two_thirds\""), "{j}");
        let spot = spots()[1];
        let j = serde_json::to_string(&spot).unwrap();
        assert_eq!(serde_json::from_str::<Spot>(&j).unwrap(), spot);
    }

    #[test]
    fn reconcile_is_idempotent() {
        let desks = [cat_desk(1), user_desk(10), cat_desk(3)];
        let order = [CatId(1), CatId(2), CatId(3)];
        let once = reconcile(&desks, &order);
        assert_eq!(reconcile(&once, &order), once);
    }
}
