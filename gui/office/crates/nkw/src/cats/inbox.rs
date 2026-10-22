//! The inbox walks to you (§6.6): where the walkers stop (a semicircle
//! 1.3 m before the camera, the first 0.6 m to the side), the A* on each
//! room's 0.25 m occupancy grid (`World::room_grid`), the hall between the
//! doors, and the walk along the path at 1.4 m/s. Pure; the queue itself
//! (order, stagger, dismiss, summon) is `nk_cats::inbox`.

use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap};

use glam::{Vec2, Vec3};
use nkw_core::collide::{Aabb, CELL, Grid};
use nkw_core::strip::Side;
use nkw_core::{Pose, RoomId};

use crate::world::{HALL_W, World};

/// Walking speed, m/s.
pub const SPEED: f32 = 1.4;
/// How far before you a walker stops, and how far to the side the first.
pub const STOP_M: f32 = 1.3;
pub const SIDE_M: f32 = 0.6;
/// Walkers near you at once; the rest is a badge.
pub const NEAR_MAX: usize = 3;
/// The walkers' frame rate while one moves.
pub const WALK_HZ: f32 = 30.0;
/// The A* gives up after this many nodes (a room is ~500).
const MAX_NODES: usize = 20_000;

/// The yaw of something that faces along `d` (x, z).
pub fn yaw_of(d: Vec2) -> f32 {
    d.x.atan2(-d.y)
}

/// Stop `i` of `n` around the camera: on a semicircle of `STOP_M` before
/// it, alternating sides, the first two `SIDE_M` to the side. Returns the
/// floor point (x, z) and the yaw facing the camera.
pub fn stop(cam: &Pose, i: usize) -> (Vec2, f32) {
    let h = cam.heading();
    let base = (SIDE_M / STOP_M).asin();
    let k = (i / 2 + 1) as f32;
    let sign = if i.is_multiple_of(2) { 1.0 } else { -1.0 };
    let a = sign * base * k;
    let (s, c) = a.sin_cos();
    // rotate the heading by `a` (towards the right for a > 0)
    let right = Vec2::new(-h.y, h.x);
    let dir = h * c + right * s;
    let p = Vec2::new(cam.pos.x, cam.pos.z) + dir * STOP_M;
    (p, yaw_of(-dir))
}

#[derive(Clone, Copy, PartialEq)]
struct Node {
    f: f32,
    i: (i32, i32),
}

impl Eq for Node {}

impl Ord for Node {
    fn cmp(&self, o: &Node) -> Ordering {
        o.f.total_cmp(&self.f).then_with(|| self.i.cmp(&o.i))
    }
}

impl PartialOrd for Node {
    fn partial_cmp(&self, o: &Node) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}

/// A* on the lattice of `CELL` steps anchored at `from` (8 neighbours, no
/// corner cutting), inside `bounds`. `blocked` is the grid query; cells
/// within `escape` of `from` count as free (a cat gets up from its chair).
/// The goal is the free lattice point nearest `to`; the path ends at `to`
/// itself when that is free. `None`: unreachable.
pub fn astar(
    blocked: &dyn Fn(Vec2) -> bool,
    from: Vec2,
    to: Vec2,
    bounds: Aabb,
    escape: f32,
) -> Option<Vec<Vec2>> {
    let at = |i: (i32, i32)| from + Vec2::new(i.0 as f32, i.1 as f32) * CELL;
    let free = |i: (i32, i32)| {
        let p = at(i);
        bounds.contains(p) && (p.distance(from) <= escape || !blocked(p))
    };
    let goal_i = {
        let g = ((to - from) / CELL).round();
        let g = (g.x as i32, g.y as i32);
        if free(g) {
            g
        } else {
            // the nearest free lattice point (rings of growing radius)
            let mut best = None;
            'rings: for r in 1i32..40 {
                let mut ring = Vec::new();
                for dx in -r..=r {
                    for dy in -r..=r {
                        if dx.abs() == r || dy.abs() == r {
                            ring.push((g.0 + dx, g.1 + dy));
                        }
                    }
                }
                ring.sort_by(|a, b| at(*a).distance(to).total_cmp(&at(*b).distance(to)));
                for c in ring {
                    if free(c) {
                        best = Some(c);
                        break 'rings;
                    }
                }
            }
            best?
        }
    };
    let h = |i: (i32, i32)| at(i).distance(at(goal_i));
    let mut open = BinaryHeap::new();
    let mut g: HashMap<(i32, i32), f32> = HashMap::new();
    let mut came: HashMap<(i32, i32), (i32, i32)> = HashMap::new();
    g.insert((0, 0), 0.0);
    open.push(Node {
        f: h((0, 0)),
        i: (0, 0),
    });
    let mut seen = 0;
    while let Some(Node { i, .. }) = open.pop() {
        if i == goal_i {
            let mut path = vec![at(i)];
            let mut c = i;
            while let Some(p) = came.get(&c) {
                c = *p;
                path.push(at(c));
            }
            path.reverse();
            if !blocked(to) && to.distance(at(goal_i)) > 1e-4 {
                path.push(to);
            }
            return Some(path);
        }
        seen += 1;
        if seen > MAX_NODES {
            return None;
        }
        let gi = g[&i];
        for (dx, dy) in [
            (1, 0),
            (-1, 0),
            (0, 1),
            (0, -1),
            (1, 1),
            (1, -1),
            (-1, 1),
            (-1, -1),
        ] {
            let n = (i.0 + dx, i.1 + dy);
            if !free(n)
                || (dx != 0 && dy != 0 && (!free((i.0 + dx, i.1)) || !free((i.0, i.1 + dy))))
            {
                continue;
            }
            let step = if dx != 0 && dy != 0 {
                CELL * std::f32::consts::SQRT_2
            } else {
                CELL
            };
            let ng = gi + step;
            if g.get(&n).is_none_or(|old| ng < *old) {
                g.insert(n, ng);
                came.insert(n, i);
                open.push(Node { f: ng + h(n), i: n });
            }
        }
    }
    None
}

/// A polyline walked at `SPEED`.
#[derive(Clone, Debug, PartialEq)]
pub struct Path {
    pub pts: Vec<Vec2>,
    pub len: f32,
}

impl Path {
    pub fn new(pts: Vec<Vec2>) -> Path {
        let mut pts = pts;
        pts.dedup_by(|a, b| a.distance(*b) < 1e-4);
        let len = pts.windows(2).map(|w| w[0].distance(w[1])).sum();
        Path { pts, len }
    }

    /// Where the walker is after `d` metres, and which way it faces.
    pub fn at(&self, d: f32) -> (Vec2, Vec2) {
        let mut left = d.max(0.0);
        for w in self.pts.windows(2) {
            let seg = w[0].distance(w[1]);
            let dir = (w[1] - w[0]).normalize_or_zero();
            if left <= seg {
                return (w[0] + dir * left, dir);
            }
            left -= seg;
        }
        let last = *self.pts.last().unwrap_or(&Vec2::ZERO);
        let dir = match self.pts.len() {
            n if n >= 2 => (self.pts[n - 1] - self.pts[n - 2]).normalize_or_zero(),
            _ => Vec2::NEG_Y,
        };
        (last, dir)
    }

    /// Seconds the walk takes.
    pub fn secs(&self) -> f64 {
        (self.len / SPEED) as f64
    }
}

/// The room (index) a floor point is in, if any (else the hall).
pub fn room_of(w: &World, p: Vec2) -> Option<usize> {
    w.slots().iter().position(|s| {
        let inside_x = p.x >= s.x0 && p.x <= s.x0 + s.len;
        let inside_z = match s.side {
            Side::North => p.y <= -HALL_W / 2.0,
            Side::South => p.y >= HALL_W / 2.0,
        };
        inside_x && inside_z
    })
}

/// A room's floor (x, z) with a margin out through its door.
fn room_bounds(w: &World, r: usize) -> Aabb {
    let s = w.slots()[r];
    let (z0, z1) = match s.side {
        Side::North => (
            -(HALL_W / 2.0 + crate::world::ROOM_DEPTH),
            -HALL_W / 2.0 + 0.5,
        ),
        Side::South => (HALL_W / 2.0 - 0.5, HALL_W / 2.0 + crate::world::ROOM_DEPTH),
    };
    Aabb::new(Vec2::new(s.x0, z0), Vec2::new(s.x0 + s.len, z1))
}

/// The door point of room `r` on the hall wall and one step out into the
/// hall.
fn door_points(w: &World, r: usize) -> (Vec2, Vec2) {
    let s = w.slots()[r];
    let (wall, out) = match s.side {
        Side::North => (-HALL_W / 2.0, -HALL_W / 2.0 + 0.5),
        Side::South => (HALL_W / 2.0, HALL_W / 2.0 - 0.5),
    };
    (Vec2::new(s.door_x, wall), Vec2::new(s.door_x, out))
}

/// Inside room `r`, from `a` to `b` on its grid.
fn in_room(w: &World, grid: &Grid, r: usize, a: Vec2, b: Vec2, escape: f32) -> Option<Vec<Vec2>> {
    astar(&|p| grid.blocked(p), a, b, room_bounds(w, r), escape)
}

/// The walk from `from` to `to` (floor points): A* in the rooms, the hall
/// between their doors on its centre line. `escape`: free radius around
/// the start (a cat leaving its chair). `None`: no way.
pub fn route(w: &World, from: Vec2, to: Vec2, escape: f32) -> Option<Path> {
    let (ra, rb) = (room_of(w, from), room_of(w, to));
    let grid = |r: usize| w.room_grid(r);
    let mut pts: Vec<Vec2> = Vec::new();
    match (ra, rb) {
        (Some(a), Some(b)) if a == b => pts = in_room(w, &grid(a), a, from, to, escape)?,
        _ => {
            let mut start = from;
            if let Some(a) = ra {
                let (door, out) = door_points(w, a);
                pts.extend(in_room(w, &grid(a), a, from, door, escape)?);
                pts.push(out);
                start = out;
            } else {
                pts.push(from);
            }
            // along the hall's centre line
            let (end, enter) = match rb {
                Some(b) => {
                    let (door, out) = door_points(w, b);
                    (out, Some(door))
                }
                None => (to, None),
            };
            pts.push(Vec2::new(start.x, 0.0));
            pts.push(Vec2::new(end.x, 0.0));
            pts.push(end);
            if let (Some(b), Some(door)) = (rb, enter) {
                pts.extend(in_room(w, &grid(b), b, door, to, 0.0)?);
            }
        }
    }
    Some(Path::new(pts))
}

/// The floor point of a pose (x, z).
pub fn floor(p: Vec3) -> Vec2 {
    Vec2::new(p.x, p.z)
}

/// A walker's room for the culling set: the room it is in now.
pub fn room_id_of(w: &World, p: Vec2) -> Option<RoomId> {
    room_of(w, p).map(|i| w.rooms[i].id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cats::tests::fixture_world;
    use nkw_core::angle_diff;

    #[test]
    fn stops_make_a_semicircle_before_you() {
        let cam = Pose::new(Vec3::new(2.0, 1.5, 3.0), 0.3, -0.1);
        let h = cam.heading();
        let mut seen = Vec::new();
        for i in 0..NEAR_MAX {
            let (p, yaw) = stop(&cam, i);
            let d = p - floor(cam.pos);
            assert!((d.length() - STOP_M).abs() < 1e-4, "on the circle");
            assert!(d.dot(h) > 0.0, "in front");
            // facing you
            let facing = Pose::new(Vec3::ZERO, yaw, 0.0).heading();
            assert!(facing.dot(-d.normalize()) > 0.999);
            seen.push(p);
        }
        // the first two 0.6 m to either side of your line of sight
        let side = |p: Vec2| (p - floor(cam.pos)).perp_dot(h);
        assert!((side(seen[0]).abs() - SIDE_M).abs() < 1e-4);
        assert!((side(seen[1]).abs() - SIDE_M).abs() < 1e-4);
        assert!(side(seen[0]) * side(seen[1]) < 0.0, "opposite sides");
        assert!(side(seen[2]).abs() > SIDE_M, "the third farther out");
        assert!(seen[0].distance(seen[1]) > 1.0 && seen[0].distance(seen[2]) > 0.5);
    }

    #[test]
    fn yaw_of_matches_pose_heading() {
        for y in [-2.0f32, -0.5, 0.0, 1.0, 3.0] {
            let h = Pose::new(Vec3::ZERO, y, 0.0).heading();
            assert!(angle_diff(yaw_of(h), y).abs() < 1e-5);
        }
    }

    #[test]
    fn astar_goes_around_an_obstacle() {
        let wall = Aabb::new(Vec2::new(1.0, -1.0), Vec2::new(1.5, 2.0));
        let blocked = |p: Vec2| wall.contains(p);
        let bounds = Aabb::new(Vec2::new(-1.0, -3.0), Vec2::new(4.0, 3.0));
        let path = astar(&blocked, Vec2::ZERO, Vec2::new(3.0, 0.0), bounds, 0.0).unwrap();
        assert_eq!(*path.last().unwrap(), Vec2::new(3.0, 0.0));
        assert!(path.iter().all(|p| !blocked(*p)));
        for w in path.windows(2) {
            assert!(w[0].distance(w[1]) <= CELL * 1.5);
            // no step crosses the wall
            for k in 0..=10 {
                assert!(!blocked(w[0].lerp(w[1], k as f32 / 10.0)));
            }
        }
        let len = Path::new(path).len;
        assert!(len > 3.0 && len < 8.0, "{len}");
        // walled in: no way
        let cage = |p: Vec2| (p.length() - 1.0).abs() < 0.3;
        assert!(astar(&cage, Vec2::ZERO, Vec2::new(3.0, 0.0), bounds, 0.0).is_none());
        // a blocked goal: the nearest free point
        let p = astar(&blocked, Vec2::ZERO, Vec2::new(1.2, 0.0), bounds, 0.0).unwrap();
        assert!(p.last().unwrap().distance(Vec2::new(1.2, 0.0)) < 0.5);
        // a blocked start within `escape` gets out
        let inside = |p: Vec2| p.length() < 0.3;
        assert!(astar(&inside, Vec2::ZERO, Vec2::new(2.0, 0.0), bounds, 0.0).is_none());
        assert!(astar(&inside, Vec2::ZERO, Vec2::new(2.0, 0.0), bounds, 0.6).is_some());
    }

    #[test]
    fn path_walks_at_speed() {
        let p = Path::new(vec![Vec2::ZERO, Vec2::new(1.0, 0.0), Vec2::new(1.0, 1.0)]);
        assert_eq!(p.len, 2.0);
        assert!((p.secs() - 2.0 / SPEED as f64).abs() < 1e-6);
        assert_eq!(p.at(0.5), (Vec2::new(0.5, 0.0), Vec2::X));
        assert_eq!(p.at(1.5), (Vec2::new(1.0, 0.5), Vec2::Y));
        assert_eq!(p.at(9.0), (Vec2::new(1.0, 1.0), Vec2::Y));
        assert_eq!(p.at(-1.0).0, Vec2::ZERO);
    }

    #[test]
    fn a_cat_walks_from_its_chair_to_you_in_its_room() {
        let (mut w, _) = fixture_world();
        w.command(crate::world::Cmd::RoomNext, 0.0);
        w.tick(10.0);
        let acme = w.current_room_id();
        let seat = w.cat_seat(acme, 1).unwrap(); // api-gateway, the station's neighbour
        let (to, _) = stop(&w.camera, 0);
        let p = route(&w, floor(seat.pos), to, 0.6).expect("a way");
        assert!(
            p.pts.last().unwrap().distance(to) < 0.3,
            "{:?}",
            p.pts.last()
        );
        let r = w.rooms.iter().position(|r| r.id == acme).unwrap();
        let grid = w.room_grid(r);
        // after leaving the chair, never through a desk or a chair
        for pt in p.pts.iter().filter(|q| q.distance(floor(seat.pos)) > 0.6) {
            assert!(!grid.blocked(*pt), "{pt}");
        }
        // the stop is in front of the camera, at STOP_M
        assert!((to.distance(floor(w.camera.pos)) - STOP_M).abs() < 1e-4);
    }

    #[test]
    fn a_cat_from_another_room_comes_through_the_doors_and_the_hall() {
        let (w, _) = fixture_world();
        // the camera at the reception; a cat of "demo"
        let demo = w.rooms.iter().position(|r| r.name == "demo").unwrap();
        let seat = w.cat_seat(w.rooms[demo].id, 0).unwrap();
        let (to, _) = stop(&w.camera, 0);
        assert_eq!(room_of(&w, to), Some(0));
        assert_eq!(room_of(&w, floor(seat.pos)), Some(demo));
        let p = route(&w, floor(seat.pos), to, 0.6).expect("a way");
        assert!(p.pts.iter().any(|q| q.y.abs() < 1e-4), "through the hall");
        assert!(p.pts.last().unwrap().distance(to) < 0.3);
        let (d0, _) = door_points(&w, demo);
        let (d1, _) = door_points(&w, 0);
        assert!(
            p.pts.contains(&d0) && p.pts.contains(&d1),
            "through both doors"
        );
        // ~ the hall distance between the doors and then some
        assert!(p.len > (d0.x - d1.x).abs());
        assert_eq!(
            room_of(&w, Vec2::new(d0.x, 0.0)),
            None,
            "the hall is no room"
        );
        assert_eq!(room_id_of(&w, to), Some(w.rooms[0].id));
    }
}
