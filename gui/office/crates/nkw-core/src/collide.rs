//! ANDAR: walking physics and collision (§3.5). Pure and stepped at a fixed
//! 1/120 s, so it behaves the same at any frame rate.
//!
//! Each room gets a 0.25 m occupancy grid on the floor plane (x, z): its
//! walls, with 1.2 m door gaps, and the furniture and cats as AABBs
//! inflated by the player's radius. Movement is resolved per axis (x, then
//! z): a blocked axis is zeroed and the other one keeps going, so you slide
//! along walls. At 1/120 s even 3× the running speed moves less than a cell
//! per step, so nothing is tunnelled through.

use glam::{BVec2, Mat4, Vec2, Vec3};
use serde::{Deserialize, Serialize};

use crate::Time;
use crate::pose::{Pose, angle_diff};

/// Grid cell, metres.
pub const CELL: f32 = 0.25;
/// Door gap, metres.
pub const DOOR_W: f32 = 1.2;
/// Simulation step, seconds.
pub const STEP: Time = 1.0 / 120.0;
/// A screen in the air only blocks if its bottom edge is below this, metres.
pub const AIR_BLOCK_BELOW: f32 = 1.9;
/// How far the aim reaches, metres (§3.5).
pub const REACH_M: f32 = 4.0;
/// Half-angle of the aim cone, radians.
pub const AIM_CONE_RAD: f32 = 8.0_f32.to_radians();
/// How far outside its walls a room's grid reaches (to walk out of doors).
const MARGIN: f32 = 1.0;
/// At most this many steps per update (a quarter second); a longer stall
/// is dropped rather than replayed.
const MAX_STEPS: usize = 30;

/// An axis-aligned box on the floor plane: `x` is x, `y` is z.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Aabb {
    pub min: Vec2,
    pub max: Vec2,
}

impl Aabb {
    pub fn new(min: Vec2, max: Vec2) -> Aabb {
        Aabb { min, max }
    }

    /// Grown by `r` on every side (negative `r` shrinks).
    pub fn inflate(&self, r: f32) -> Aabb {
        Aabb::new(self.min - Vec2::splat(r), self.max + Vec2::splat(r))
    }

    pub fn contains(&self, p: Vec2) -> bool {
        p.cmpge(self.min).all() && p.cmple(self.max).all()
    }
}

/// Whether a screen in the air with its bottom edge at `bottom_y` blocks
/// walking.
pub fn air_screen_blocks(bottom_y: f32) -> bool {
    bottom_y < AIR_BLOCK_BELOW
}

/// A wall of a room's floor box.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wall {
    /// at `min.z`
    North,
    /// at `max.z`
    South,
    /// at `min.x`
    West,
    /// at `max.x`
    East,
}

/// A door gap: on `wall`, centred at `at` along it (x for north/south, z
/// for west/east).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Door {
    pub wall: Wall,
    pub at: f32,
    pub width: f32,
}

impl Door {
    pub fn new(wall: Wall, at: f32) -> Door {
        Door {
            wall,
            at,
            width: DOOR_W,
        }
    }

    /// The passage through the wall, out to the edge of the grid, narrowed
    /// by the player's radius.
    fn passage(&self, floor: &Aabb, r: f32) -> Aabb {
        let (a0, a1) = (
            self.at - self.width / 2.0 + r,
            self.at + self.width / 2.0 - r,
        );
        match self.wall {
            Wall::North => Aabb::new(
                Vec2::new(a0, floor.min.y - MARGIN),
                Vec2::new(a1, floor.min.y + r),
            ),
            Wall::South => Aabb::new(
                Vec2::new(a0, floor.max.y - r),
                Vec2::new(a1, floor.max.y + MARGIN),
            ),
            Wall::West => Aabb::new(
                Vec2::new(floor.min.x - MARGIN, a0),
                Vec2::new(floor.min.x + r, a1),
            ),
            Wall::East => Aabb::new(
                Vec2::new(floor.max.x - r, a0),
                Vec2::new(floor.max.x + MARGIN, a1),
            ),
        }
    }
}

/// A room's occupancy grid. Outside the grid counts as blocked.
#[derive(Clone, Debug, PartialEq)]
pub struct Grid {
    origin: Vec2,
    cols: usize,
    rows: usize,
    blocked: Vec<bool>,
}

impl Grid {
    /// The grid of a room with walls around `floor`, `doors` in them and
    /// `obstacles` inside, for a player of radius `r`. A cell is free when
    /// its centre is on the floor (or in a door passage) at least `r` from
    /// the walls and at least `r` from every obstacle.
    pub fn room(floor: Aabb, doors: &[Door], obstacles: &[Aabb], r: f32) -> Grid {
        let origin = floor.min - Vec2::splat(MARGIN);
        let size = (floor.max - floor.min + Vec2::splat(2.0 * MARGIN)) / CELL;
        let (cols, rows) = (size.x.ceil() as usize, size.y.ceil() as usize);
        let inner = floor.inflate(-r);
        let passages: Vec<Aabb> = doors.iter().map(|d| d.passage(&floor, r)).collect();
        let solid: Vec<Aabb> = obstacles.iter().map(|o| o.inflate(r)).collect();
        let mut blocked = Vec::with_capacity(cols * rows);
        for j in 0..rows {
            for i in 0..cols {
                let c = origin + (Vec2::new(i as f32, j as f32) + 0.5) * CELL;
                let walkable = inner.contains(c) || passages.iter().any(|p| p.contains(c));
                blocked.push(!walkable || solid.iter().any(|s| s.contains(c)));
            }
        }
        Grid {
            origin,
            cols,
            rows,
            blocked,
        }
    }

    /// Whether the point (x, z) is blocked.
    pub fn blocked(&self, p: Vec2) -> bool {
        let c = ((p - self.origin) / CELL).floor();
        if c.x < 0.0 || c.y < 0.0 {
            return true;
        }
        let (i, j) = (c.x as usize, c.y as usize);
        i >= self.cols || j >= self.rows || self.blocked[j * self.cols + i]
    }
}

/// Move from `pos` by `delta`, x then z; an axis whose destination is
/// blocked does not move (reported in the result). Starting inside a
/// blocked cell, anything goes, so a player can always get out.
pub fn slide(grid: &Grid, pos: Vec2, delta: Vec2) -> (Vec2, BVec2) {
    if grid.blocked(pos) {
        return (pos + delta, BVec2::FALSE);
    }
    let mut p = pos;
    let mut hit = BVec2::FALSE;
    let x = Vec2::new(p.x + delta.x, p.y);
    if delta.x != 0.0 && grid.blocked(x) {
        hit.x = true;
    } else {
        p = x;
    }
    let z = Vec2::new(p.x, p.y + delta.y);
    if delta.y != 0.0 && grid.blocked(z) {
        hit.y = true;
    } else {
        p = z;
    }
    (p, hit)
}

/// How held turn keys turn.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TurnMode {
    /// smooth, with a short ramp
    Smooth,
    /// one step per press, instant
    Snap,
}

/// `[walk]` and `[camera]` of `world.toml` (§3.5). Angles in degrees.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WalkParams {
    /// walking speed, m/s (1–5)
    pub speed: f32,
    /// running multiplier (1–3)
    pub run: f32,
    /// m/s²
    pub accel: f32,
    /// m/s²
    pub decel: f32,
    pub turn: TurnMode,
    /// smooth turn, °/s (45–240)
    pub turn_speed: f32,
    /// seconds to reach full turn speed
    pub turn_ramp: f32,
    /// snap turn, ° per press (15/30/45)
    pub snap_step: f32,
    /// ± pitch limit, °
    pub pitch_max: f32,
    /// °/s
    pub pitch_speed: f32,
    /// ° per mouse count, no acceleration
    pub mouse: f32,
    /// metres (1.2–1.8)
    pub eye_height: f32,
    /// vertical field of view, ° (50–75), fixed
    pub fov_y: f32,
    /// 0 (off) to 1
    pub head_bob: f32,
    /// player radius, metres
    pub radius: f32,
    /// how far the crosshair reaches, metres
    pub reach: f32,
}

impl Default for WalkParams {
    fn default() -> WalkParams {
        WalkParams {
            speed: 2.8,
            run: 2.0,
            accel: 10.0,
            decel: 18.0,
            turn: TurnMode::Smooth,
            turn_speed: 110.0,
            turn_ramp: 0.12,
            snap_step: 30.0,
            pitch_max: 55.0,
            pitch_speed: 80.0,
            mouse: 0.12,
            eye_height: 1.5,
            fov_y: 60.0,
            head_bob: 0.0,
            radius: 0.3,
            reach: 4.0,
        }
    }
}

impl WalkParams {
    /// The params of `world.toml`'s `[walk]`/`[camera]` (angles in degrees),
    /// clamped to the plan's ranges; the rest stays default.
    pub fn from_config(
        speed: f32,
        run_mul: f32,
        turn: TurnMode,
        turn_rate_dps: f32,
        snap_deg: f32,
        pitch_max_deg: f32,
        eye: f32,
    ) -> WalkParams {
        let d = WalkParams::default();
        // NaN keeps the default; out of range clamps
        let clamp = |v: f32, lo: f32, hi: f32, def: f32| {
            if v.is_nan() { def } else { v.clamp(lo, hi) }
        };
        let snap = [15.0f32, 30.0, 45.0]
            .into_iter()
            .min_by(|a, b| (a - snap_deg).abs().total_cmp(&(b - snap_deg).abs()))
            .filter(|_| !snap_deg.is_nan())
            .unwrap_or(d.snap_step);
        WalkParams {
            speed: clamp(speed, 1.0, 5.0, d.speed),
            run: clamp(run_mul, 1.0, 3.0, d.run),
            turn,
            turn_speed: clamp(turn_rate_dps, 45.0, 240.0, d.turn_speed),
            snap_step: snap,
            pitch_max: clamp(pitch_max_deg, 10.0, 85.0, d.pitch_max),
            eye_height: clamp(eye, 1.2, 1.8, d.eye_height),
            ..d
        }
    }
}

/// The walls of the rooms along the hall, for [`line_of_sight`]: each
/// room's box (the strip's `RoomSlot`, `ROOM_DEPTH` deep off the hall) and
/// the doorway of its hall wall.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WallPlan {
    pub rooms: Vec<(crate::RoomId, crate::strip::RoomSlot)>,
    /// the doorway of a room's hall wall: its floor centre (x, z) and width
    pub doors: Vec<(crate::RoomId, Vec2, f32)>,
    /// metres; a segment entirely above the walls is not cut
    pub wall_h: f32,
}

/// Whether no wall of `plan` cuts the segment `from`–`to`; a doorway is
/// open when `open` lists its room (the culling set), closed otherwise.
///
/// Walls are planes at the rooms' edges (`RoomSlot` along the hall,
/// `layout::ROOM_DEPTH` deep, the hall wall at `HALL_W / 2`), so anything
/// inside a room, even against a wall, sees the rest of it; touching a wall
/// edge counts as cut. The doorway spans the wall's height.
pub fn line_of_sight(plan: &WallPlan, from: Vec3, to: Vec3, open: &[crate::RoomId]) -> bool {
    use crate::layout::{HALL_W, ROOM_DEPTH};
    use crate::strip::Side;
    /// half the thickness of a wall plane, metres
    const T: f32 = 5e-4;
    let d = to - from;
    let len = d.length();
    if len < 1e-6 || (from.y > plan.wall_h && to.y > plan.wall_h) {
        return true;
    }
    let dir = d / len;
    let h = plan.wall_h;
    // a hit strictly before the far end (an end on a wall is not cut)
    let cut =
        |min: Vec3, max: Vec3| ray_hit_aabb(from, dir, min, max).is_some_and(|t| t < len - 1e-4);
    // the segment's own bounds on the floor, to skip far rooms cheaply
    let (lo, hi) = (from.min(to), from.max(to));
    for (id, s) in &plan.rooms {
        let (x0, x1) = (s.x0, s.x0 + s.len);
        let (hall, back) = match s.side {
            Side::North => (-HALL_W / 2.0, -(HALL_W / 2.0 + ROOM_DEPTH)),
            Side::South => (HALL_W / 2.0, HALL_W / 2.0 + ROOM_DEPTH),
        };
        let (z0, z1) = (hall.min(back), hall.max(back));
        if hi.x < x0 - T || lo.x > x1 + T || hi.z < z0 - T || lo.z > z1 + T {
            continue;
        }
        let xwall = |x: f32| (Vec3::new(x - T, 0.0, z0 - T), Vec3::new(x + T, h, z1 + T));
        let zwall = |z: f32, a: f32, b: f32| (Vec3::new(a, 0.0, z - T), Vec3::new(b, h, z + T));
        let (bmin, bmax) = zwall(back, x0 - T, x1 + T);
        let (lmin, lmax) = xwall(x0);
        let (rmin, rmax) = xwall(x1);
        if cut(bmin, bmax) || cut(lmin, lmax) || cut(rmin, rmax) {
            return false;
        }
        // the hall wall, with its doorway open or not
        let door = plan
            .doors
            .iter()
            .find(|(r, _, _)| r == id)
            .map(|(_, c, w)| (c.x - w / 2.0, c.x + w / 2.0));
        let pieces: [(f32, f32); 2] = match door {
            Some((a, b)) if open.contains(id) => [(x0 - T, a), (b, x1 + T)],
            _ => [(x0 - T, x1 + T), (0.0, 0.0)],
        };
        for (a, b) in pieces {
            if b > a {
                let (min, max) = zwall(hall, a, b);
                if cut(min, max) {
                    return false;
                }
            }
        }
    }
    true
}

/// Where a ray (origin, unit `dir`) enters the box `min..max`: the distance
/// along `dir` (0 when it starts inside), `None` when it misses or the box
/// is behind.
pub fn ray_hit_aabb(origin: Vec3, dir: Vec3, min: Vec3, max: Vec3) -> Option<f32> {
    let (mut t0, mut t1) = (0.0f32, f32::INFINITY);
    for a in 0..3 {
        let (o, d) = (origin[a], dir[a]);
        if d.abs() < 1e-9 {
            if o < min[a] || o > max[a] {
                return None;
            }
            continue;
        }
        let (mut n, mut f) = ((min[a] - o) / d, (max[a] - o) / d);
        if n > f {
            std::mem::swap(&mut n, &mut f);
        }
        t0 = t0.max(n);
        t1 = t1.min(f);
        if t0 > t1 {
            return None;
        }
    }
    Some(t0)
}

/// Where a ray first touches a sphere (0 from inside), `None` if it misses.
pub fn ray_hit_sphere(origin: Vec3, dir: Vec3, centre: Vec3, r: f32) -> Option<f32> {
    let oc = origin - centre;
    let c = oc.length_squared() - r * r;
    if c <= 0.0 {
        return Some(0.0);
    }
    let b = oc.dot(dir);
    let disc = b * b - c;
    if b > 0.0 || disc < 0.0 {
        return None;
    }
    Some(-b - disc.sqrt())
}

/// Where a ray hits the unit quad (x, y in -0.5..0.5, z = 0, facing +z)
/// placed by `model`; either side counts. `None` if it misses.
pub fn ray_hit_quad(origin: Vec3, dir: Vec3, model: Mat4) -> Option<f32> {
    let inv = model.inverse();
    if !inv.is_finite() {
        return None;
    }
    // the ray parameter is the same in both spaces (affine map)
    let o = inv.transform_point3(origin);
    let d = inv.transform_vector3(dir);
    if d.z.abs() < 1e-7 {
        return None;
    }
    let t = -o.z / d.z;
    if t < 0.0 {
        return None;
    }
    let p = o + d * t;
    (p.x.abs() <= 0.5 && p.y.abs() <= 0.5).then_some(t)
}

/// What the player is doing this frame.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct WalkInput {
    /// held: +1 forward (w), -1 back (s)
    pub forward: f32,
    /// held: +1 right (d), -1 left (a)
    pub strafe: f32,
    /// held: run key
    pub run: bool,
    /// held: +1 turn right (e/→), -1 left (q/←); smooth mode only
    pub turn: f32,
    /// held: +1 look up (↑), -1 down (↓)
    pub look: f32,
    /// presses this frame, + right; snap mode only
    pub snap_turns: i32,
    /// relative mouse counts this frame (x right, y down)
    pub mouse: Vec2,
    /// c: look at the horizon
    pub recenter: bool,
}

/// The walking player.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Walker {
    /// on the floor (x, z)
    pub pos: Vec2,
    /// m/s on the floor
    pub vel: Vec2,
    pub yaw: f32,
    pub pitch: f32,
    /// current smooth turn rate, rad/s
    pub turn_vel: f32,
    /// time not yet simulated, seconds
    acc: Time,
}

impl Walker {
    /// Start walking from a camera pose.
    pub fn new(pose: &Pose) -> Walker {
        Walker {
            pos: Vec2::new(pose.pos.x, pose.pos.z),
            vel: Vec2::ZERO,
            yaw: pose.yaw,
            pitch: pose.pitch,
            turn_vel: 0.0,
            acc: 0.0,
        }
    }

    /// The camera: eye at `eye_height`.
    pub fn pose(&self, p: &WalkParams) -> Pose {
        Pose::new(
            glam::Vec3::new(self.pos.x, p.eye_height, self.pos.y),
            self.yaw,
            self.pitch,
        )
    }

    /// Whether anything still moves (the scheduler then wants frames).
    pub fn moving(&self) -> bool {
        self.vel != Vec2::ZERO || self.turn_vel != 0.0
    }

    /// Advance by `dt` seconds of wall time in fixed steps; returns how many
    /// steps ran. Presses, mouse and recentre apply once, up front.
    pub fn update(&mut self, grid: &Grid, input: &WalkInput, p: &WalkParams, dt: Time) -> usize {
        if p.turn == TurnMode::Snap {
            self.yaw += input.snap_turns as f32 * p.snap_step.to_radians();
        }
        self.yaw += input.mouse.x * p.mouse.to_radians();
        self.pitch -= input.mouse.y * p.mouse.to_radians();
        if input.recenter {
            self.pitch = 0.0;
        }
        self.settle(p);

        self.acc += dt;
        let mut steps = (self.acc / STEP + 1e-9).floor() as usize;
        if steps > MAX_STEPS {
            steps = MAX_STEPS;
            self.acc = 0.0;
        } else {
            self.acc = (self.acc - steps as f64 * STEP).max(0.0);
        }
        for _ in 0..steps {
            self.step(grid, input, p, STEP as f32);
        }
        steps
    }

    /// One fixed step of `h` seconds.
    fn step(&mut self, grid: &Grid, input: &WalkInput, p: &WalkParams, h: f32) {
        if p.turn == TurnMode::Smooth {
            let full = p.turn_speed.to_radians();
            let target = input.turn.clamp(-1.0, 1.0) * full;
            let ramp = full / p.turn_ramp.max(1e-3) * h;
            self.turn_vel += (target - self.turn_vel).clamp(-ramp, ramp);
            self.yaw += self.turn_vel * h;
        } else {
            self.turn_vel = 0.0;
        }
        self.pitch += input.look.clamp(-1.0, 1.0) * p.pitch_speed.to_radians() * h;
        self.settle(p);

        let wish = Vec2::new(input.strafe, input.forward).clamp_length_max(1.0);
        let (sy, cy) = self.yaw.sin_cos();
        let (heading, right) = (Vec2::new(sy, -cy), Vec2::new(cy, sy));
        let speed = p.speed * if input.run { p.run } else { 1.0 };
        let target = (right * wish.x + heading * wish.y) * speed;
        let braking =
            target.length_squared() < self.vel.length_squared() || target.dot(self.vel) < 0.0;
        let rate = if braking { p.decel } else { p.accel };
        self.vel += (target - self.vel).clamp_length_max(rate * h);

        let (pos, hit) = slide(grid, self.pos, self.vel * h);
        self.pos = pos;
        if hit.x {
            self.vel.x = 0.0;
        }
        if hit.y {
            self.vel.y = 0.0;
        }
    }

    /// Clamp the pitch, wrap the yaw.
    fn settle(&mut self, p: &WalkParams) {
        let max = p.pitch_max.to_radians();
        self.pitch = self.pitch.clamp(-max, max);
        self.yaw = angle_diff(self.yaw, 0.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::FRAC_PI_2;

    /// Three 6 m rooms: 0 north (x 0..6, door at 3), 1 south (x 0..6, door
    /// at 3), 2 north (x 6.6..12.6, door at 9.6); walls 2.8 m.
    fn plan() -> WallPlan {
        use crate::RoomId;
        let slots = crate::strip::rooms_along_hall(&[6.0, 6.0, 6.0]);
        WallPlan {
            rooms: slots
                .iter()
                .enumerate()
                .map(|(i, s)| (RoomId(i as u32), *s))
                .collect(),
            doors: slots
                .iter()
                .enumerate()
                .map(|(i, s)| (RoomId(i as u32), crate::layout::door_world(s).0, DOOR_W))
                .collect(),
            wall_h: 2.8,
        }
    }

    #[test]
    fn line_of_sight_table() {
        use crate::RoomId;
        let p = plan();
        let (r0, r1, r2) = (RoomId(0), RoomId(1), RoomId(2));
        let v = |x: f32, z: f32| Vec3::new(x, 1.5, z);
        let rows: [(&str, Vec3, Vec3, &[RoomId], bool); 14] = [
            ("same room", v(3.0, -5.0), v(1.0, -3.0), &[], true),
            (
                "same room, corner to corner",
                v(0.1, -6.4),
                v(5.9, -1.6),
                &[],
                true,
            ),
            (
                "the next room, through two side walls",
                v(3.0, -4.0),
                v(9.6, -4.0),
                &[r0, r2],
                false,
            ),
            (
                "across the hall, both doors open",
                v(3.0, -2.5),
                v(3.0, 2.5),
                &[r0, r1],
                true,
            ),
            (
                "across the hall, the far door closed",
                v(3.0, -2.5),
                v(3.0, 2.5),
                &[r0],
                false,
            ),
            (
                "across the hall, beside the doors",
                v(5.0, -2.5),
                v(5.0, 2.5),
                &[r0, r1],
                false,
            ),
            (
                "hall to room, open door",
                v(3.0, 0.0),
                v(3.0, -3.0),
                &[r0],
                true,
            ),
            (
                "hall to room, closed door",
                v(3.0, 0.0),
                v(3.0, -3.0),
                &[],
                false,
            ),
            (
                "hall to room, the wall beside the door",
                v(5.0, 0.0),
                v(5.0, -3.0),
                &[r0],
                false,
            ),
            ("hall to hall", v(-1.0, 0.0), v(20.0, 0.5), &[], true),
            (
                "grazing past the corner, outside",
                v(6.01, 0.0),
                v(6.01, -3.0),
                &[],
                true,
            ),
            (
                "grazing the corner, inside",
                v(5.99, 0.0),
                v(5.99, -3.0),
                &[r0],
                false,
            ),
            (
                "over the walls",
                Vec3::new(3.0, 3.0, -4.0),
                Vec3::new(9.6, 3.0, -4.0),
                &[],
                true,
            ),
            ("a point", v(3.0, -4.0), v(3.0, -4.0), &[], true),
        ];
        for (what, a, b, open, want) in rows {
            assert_eq!(line_of_sight(&p, a, b, open), want, "{what}");
            assert_eq!(line_of_sight(&p, b, a, open), want, "{what}, reversed");
        }
        // a tag right against the inside of a wall is still in its room
        assert!(line_of_sight(&p, v(3.0, -3.0), v(5.999, -6.499), &[]));
        // no plan, no walls
        assert!(line_of_sight(
            &WallPlan::default(),
            v(0.0, 0.0),
            v(9.0, -9.0),
            &[]
        ));
    }

    /// A 10 × 8 m room from (0, 0) to (10, 8), a door on the north wall
    /// (z = 0) at x = 5, a desk from (2, 2) to (4, 3).
    fn room() -> Grid {
        Grid::room(
            Aabb::new(Vec2::ZERO, Vec2::new(10.0, 8.0)),
            &[Door::new(Wall::North, 5.0)],
            &[Aabb::new(Vec2::new(2.0, 2.0), Vec2::new(4.0, 3.0))],
            0.3,
        )
    }

    fn walker_at(x: f32, z: f32, yaw: f32) -> Walker {
        Walker::new(&Pose::new(Vec3::new(x, 1.5, z), yaw, 0.0))
    }

    fn run_for(w: &mut Walker, g: &Grid, i: &WalkInput, p: &WalkParams, secs: f64) {
        let frames = (secs * 60.0).round() as usize;
        for _ in 0..frames {
            w.update(g, i, p, 1.0 / 60.0);
        }
    }

    const FWD: WalkInput = WalkInput {
        forward: 1.0,
        strafe: 0.0,
        run: false,
        turn: 0.0,
        look: 0.0,
        snap_turns: 0,
        mouse: Vec2::ZERO,
        recenter: false,
    };

    #[test]
    fn the_grid_knows_floor_walls_furniture_and_doors() {
        let g = room();
        assert!(!g.blocked(Vec2::new(7.0, 5.0)), "open floor");
        assert!(g.blocked(Vec2::new(3.0, 2.5)), "the desk");
        assert!(
            g.blocked(Vec2::new(3.0, 1.8)),
            "the desk, inflated by the radius"
        );
        assert!(g.blocked(Vec2::new(7.0, 0.1)), "too close to the wall");
        assert!(g.blocked(Vec2::new(7.0, -0.5)), "behind the wall");
        assert!(!g.blocked(Vec2::new(5.0, 0.1)), "the door gap");
        assert!(!g.blocked(Vec2::new(5.0, -0.5)), "through the door");
        assert!(g.blocked(Vec2::new(50.0, 50.0)), "off the grid");
    }

    #[test]
    fn doors_open_every_wall() {
        let floor = Aabb::new(Vec2::ZERO, Vec2::new(10.0, 8.0));
        let doors = [
            Door::new(Wall::South, 3.0),
            Door::new(Wall::West, 4.0),
            Door::new(Wall::East, 2.0),
        ];
        let g = Grid::room(floor, &doors, &[], 0.3);
        assert!(!g.blocked(Vec2::new(3.0, 8.5)), "south");
        assert!(!g.blocked(Vec2::new(-0.5, 4.0)), "west");
        assert!(!g.blocked(Vec2::new(10.5, 2.0)), "east");
        assert!(g.blocked(Vec2::new(6.0, 8.5)), "south wall, no door");
        assert!(g.blocked(Vec2::new(-0.5, 6.5)), "west wall, no door");
        assert!(g.blocked(Vec2::new(10.5, 6.0)), "east wall, no door");
        assert!(g.blocked(Vec2::new(-1.5, 4.0)), "beyond the grid");
    }

    #[test]
    fn air_screens_block_only_when_low() {
        assert!(air_screen_blocks(1.2));
        assert!(!air_screen_blocks(2.0));
    }

    #[test]
    fn slide_stops_one_axis_and_keeps_the_other() {
        let g = room();
        // into the east wall, diagonally: x stops, z goes on
        let (p, hit) = slide(&g, Vec2::new(9.6, 5.0), Vec2::new(0.2, 0.1));
        assert_eq!(p, Vec2::new(9.6, 5.1));
        assert!(hit.x && !hit.y);
        let (p, hit) = slide(&g, Vec2::new(7.0, 5.0), Vec2::new(0.1, 0.1));
        assert_eq!(p, Vec2::new(7.1, 5.1));
        assert!(!hit.any());
        // stuck inside the desk: let them out
        let (p, _) = slide(&g, Vec2::new(3.0, 2.5), Vec2::new(0.1, 0.0));
        assert_eq!(p, Vec2::new(3.1, 2.5));
    }

    #[test]
    fn walking_into_a_wall_slides_along_it() {
        let g = room();
        // facing north-east-ish at the north wall, away from the door
        let mut w = walker_at(7.0, 1.0, 0.6);
        let p = WalkParams::default();
        run_for(&mut w, &g, &FWD, &p, 1.5);
        assert!(
            w.pos.y > 0.0 && w.pos.y < 0.6,
            "held by the wall: {}",
            w.pos
        );
        assert!(w.pos.x > 8.0, "slid along it: {}", w.pos);
        assert!(w.vel.y.abs() < 1e-6, "the blocked axis is zeroed");
    }

    #[test]
    fn walking_through_the_door_leaves_the_room() {
        let g = room();
        let mut w = walker_at(5.0, 2.0, 0.0);
        run_for(&mut w, &g, &FWD, &WalkParams::default(), 1.2);
        assert!(w.pos.y < -0.2, "out of the door: {}", w.pos);
        assert!((w.pos.x - 5.0).abs() < 1e-4);
    }

    #[test]
    fn no_tunnelling_at_three_times_the_speed() {
        // a single row of blocked cells (centre z = 4.125), no radius
        let thin = Aabb::new(Vec2::new(0.0, 4.0), Vec2::new(10.0, 4.25));
        let g = Grid::room(
            Aabb::new(Vec2::ZERO, Vec2::new(10.0, 8.0)),
            &[],
            &[thin],
            0.0,
        );
        let p = WalkParams {
            speed: 2.8 * 3.0,
            run: 2.0,
            radius: 0.0,
            accel: 1000.0,
            ..WalkParams::default()
        };
        let i = WalkInput { run: true, ..FWD };
        let mut w = walker_at(3.0, 7.0, 0.0);
        // long, uneven frames: the fixed steps still catch the wall
        for _ in 0..40 {
            w.update(&g, &i, &p, 0.05);
        }
        assert!(w.pos.y > 4.0, "went through the wall: {}", w.pos);
    }

    #[test]
    fn accelerates_and_brakes_per_the_plan() {
        let g = room();
        let p = WalkParams::default();
        let mut w = walker_at(7.0, 7.0, 0.0);
        run_for(&mut w, &g, &FWD, &p, 0.15);
        assert!(
            w.vel.length() < 2.0,
            "still accelerating: {}",
            w.vel.length()
        );
        run_for(&mut w, &g, &FWD, &p, 0.15);
        assert!((w.vel.length() - 2.8).abs() < 1e-3, "full speed by ~0.28 s");
        assert!(w.vel.y < 0.0, "forward is -z at yaw 0");
        run_for(&mut w, &g, &WalkInput::default(), &p, 0.1);
        assert!(w.vel.length() > 0.0);
        run_for(&mut w, &g, &WalkInput::default(), &p, 0.07);
        assert_eq!(w.vel, Vec2::ZERO, "stopped by ~0.16 s");
        assert!(!w.moving());
    }

    #[test]
    fn running_doubles_and_diagonals_are_not_faster() {
        let g = room();
        let p = WalkParams::default();
        let mut w = walker_at(7.0, 7.5, 0.0);
        let i = WalkInput { run: true, ..FWD };
        run_for(&mut w, &g, &i, &p, 0.6);
        assert!((w.vel.length() - 5.6).abs() < 1e-3);
        let mut w = walker_at(7.0, 7.5, 0.0);
        let i = WalkInput { strafe: 1.0, ..FWD };
        run_for(&mut w, &g, &i, &p, 0.5);
        assert!((w.vel.length() - 2.8).abs() < 1e-3);
        assert!(w.vel.x > 0.0, "strafe right is +x at yaw 0");
    }

    #[test]
    fn smooth_turn_ramps_up() {
        let g = room();
        let p = WalkParams::default();
        let mut w = walker_at(7.0, 5.0, 0.0);
        let i = WalkInput {
            turn: 1.0,
            ..WalkInput::default()
        };
        run_for(&mut w, &g, &i, &p, 0.05);
        assert!(w.turn_vel > 0.0 && w.turn_vel < 110f32.to_radians());
        run_for(&mut w, &g, &i, &p, 0.1);
        assert!((w.turn_vel - 110f32.to_radians()).abs() < 1e-4);
        assert!(w.moving());
        let yaw = w.yaw;
        run_for(&mut w, &g, &WalkInput::default(), &p, 0.2);
        assert_eq!(w.turn_vel, 0.0);
        assert!(w.yaw > yaw, "eases out, still turning right");
    }

    #[test]
    fn snap_turn_is_one_step_per_press_and_ignores_held_turns() {
        let g = room();
        let p = WalkParams {
            turn: TurnMode::Snap,
            ..WalkParams::default()
        };
        let mut w = walker_at(7.0, 5.0, 0.0);
        let i = WalkInput {
            snap_turns: -2,
            turn: 1.0,
            ..WalkInput::default()
        };
        w.update(&g, &i, &p, 1.0 / 60.0);
        assert!((w.yaw + 60f32.to_radians()).abs() < 1e-5);
        assert_eq!(w.turn_vel, 0.0);
    }

    #[test]
    fn yaw_wraps_around() {
        let g = room();
        let p = WalkParams {
            turn: TurnMode::Snap,
            snap_step: 45.0,
            ..WalkParams::default()
        };
        let mut w = walker_at(7.0, 5.0, 0.0);
        let i = WalkInput {
            snap_turns: 5,
            ..WalkInput::default()
        };
        w.update(&g, &i, &p, 0.0);
        assert!((w.yaw + 135f32.to_radians()).abs() < 1e-5, "{}", w.yaw);
    }

    #[test]
    fn pitch_is_clamped_and_recentres() {
        let g = room();
        let p = WalkParams::default();
        let mut w = walker_at(7.0, 5.0, 0.0);
        let up = WalkInput {
            look: 1.0,
            ..WalkInput::default()
        };
        run_for(&mut w, &g, &up, &p, 0.5);
        assert!((w.pitch - 40f32.to_radians()).abs() < 1e-3, "80°/s");
        run_for(&mut w, &g, &up, &p, 1.0);
        assert!((w.pitch - 55f32.to_radians()).abs() < 1e-5);
        let mouse = WalkInput {
            mouse: Vec2::new(100.0, 2000.0),
            ..WalkInput::default()
        };
        w.update(&g, &mouse, &p, 0.0);
        assert!(
            (w.pitch + 55f32.to_radians()).abs() < 1e-5,
            "mouse down, clamped"
        );
        assert!((w.yaw - 12f32.to_radians()).abs() < 1e-5, "0.12° a count");
        w.update(
            &g,
            &WalkInput {
                recenter: true,
                ..WalkInput::default()
            },
            &p,
            0.0,
        );
        assert_eq!(w.pitch, 0.0);
    }

    #[test]
    fn fixed_steps_carry_the_remainder() {
        let g = room();
        let p = WalkParams::default();
        let mut w = walker_at(7.0, 5.0, 0.0);
        assert_eq!(w.update(&g, &FWD, &p, 0.05), 6);
        assert_eq!(w.update(&g, &FWD, &p, 0.005), 0);
        assert_eq!(w.update(&g, &FWD, &p, 0.005), 1);
        assert_eq!(
            w.update(&g, &FWD, &p, 10.0),
            MAX_STEPS,
            "a stall is not replayed"
        );
        assert_eq!(w.update(&g, &FWD, &p, 0.0), 0);
    }

    #[test]
    fn pose_puts_the_eye_up_and_keeps_the_heading() {
        let w = walker_at(1.0, 2.0, 0.4);
        let p = WalkParams {
            eye_height: 1.7,
            ..WalkParams::default()
        };
        let pose = w.pose(&p);
        assert_eq!(pose.pos, Vec3::new(1.0, 1.7, 2.0));
        assert_eq!(pose.yaw, 0.4);
    }

    #[test]
    fn params_read_from_config_with_defaults() {
        let p: WalkParams = serde_json::from_str(r#"{"speed": 3.5, "turn": "snap"}"#).unwrap();
        assert_eq!(p.speed, 3.5);
        assert_eq!(p.turn, TurnMode::Snap);
        assert_eq!(p.run, 2.0);
        assert_eq!(p.fov_y, 60.0);
    }

    #[test]
    fn ray_hits_boxes_by_slabs() {
        let (min, max) = (Vec3::new(-1.0, 0.0, -6.0), Vec3::new(1.0, 2.0, -4.0));
        let o = Vec3::new(0.0, 1.0, 0.0);
        // (origin, dir, want)
        let rows: [(Vec3, Vec3, Option<f32>); 7] = [
            (o, Vec3::NEG_Z, Some(4.0)),                         // straight ahead
            (o, Vec3::Z, None),                                  // behind
            (o, Vec3::X, None),                                  // beside
            (Vec3::new(0.0, 1.0, -5.0), Vec3::NEG_Z, Some(0.0)), // from inside
            (Vec3::new(1.0, 1.0, 0.0), Vec3::NEG_Z, Some(4.0)),  // on the edge
            (Vec3::new(1.01, 1.0, 0.0), Vec3::NEG_Z, None),      // just past it
            (o, Vec3::new(0.0, 3.0, -4.0).normalize(), None),    // over the top
        ];
        for (i, (o, d, want)) in rows.into_iter().enumerate() {
            let got = ray_hit_aabb(o, d, min, max);
            match (got, want) {
                (Some(g), Some(w)) => assert!((g - w).abs() < 1e-4, "row {i}: {g} vs {w}"),
                (g, w) => assert_eq!(g, w, "row {i}"),
            }
        }
    }

    #[test]
    fn ray_hits_spheres() {
        let c = Vec3::new(0.0, 0.0, -5.0);
        let rows: [(Vec3, Vec3, Option<f32>); 5] = [
            (Vec3::ZERO, Vec3::NEG_Z, Some(4.0)),
            (Vec3::ZERO, Vec3::Z, None),
            (
                Vec3::new(0.99, 0.0, 0.0),
                Vec3::NEG_Z,
                Some(5.0 - (1.0f32 - 0.99 * 0.99).sqrt()),
            ),
            (Vec3::new(1.01, 0.0, 0.0), Vec3::NEG_Z, None),
            (c, Vec3::X, Some(0.0)),
        ];
        for (i, (o, d, want)) in rows.into_iter().enumerate() {
            let got = ray_hit_sphere(o, d, c, 1.0);
            match (got, want) {
                (Some(g), Some(w)) => assert!((g - w).abs() < 1e-3, "row {i}: {g} vs {w}"),
                (g, w) => assert_eq!(g, w, "row {i}"),
            }
        }
    }

    #[test]
    fn ray_hits_placed_quads() {
        // a 0.8 × 0.5 m screen 3 m ahead, turned 30° about y
        let model = Mat4::from_translation(Vec3::new(0.0, 1.5, -3.0))
            * Mat4::from_rotation_y(30f32.to_radians())
            * Mat4::from_scale(Vec3::new(0.8, 0.5, 1.0));
        let eye = Vec3::new(0.0, 1.5, 0.0);
        let t = ray_hit_quad(eye, Vec3::NEG_Z, model).expect("the centre");
        assert!((t - 3.0).abs() < 1e-4, "{t}");
        assert_eq!(ray_hit_quad(eye, Vec3::Z, model), None, "behind");
        let over = (Vec3::new(0.0, 1.8, -3.0) - eye).normalize();
        assert_eq!(ray_hit_quad(eye, over, model), None, "over the top edge");
        let low = (Vec3::new(0.0, 1.3, -3.0) - eye).normalize();
        assert!(ray_hit_quad(eye, low, model).is_some(), "inside the bottom");
        // from behind the screen still counts (either side)
        let back = Vec3::new(0.0, 1.5, -6.0);
        assert!(ray_hit_quad(back, Vec3::Z, model).is_some());
        // edge-on: no hit
        let side =
            Mat4::from_translation(Vec3::new(0.0, 1.5, -3.0)) * Mat4::from_rotation_y(FRAC_PI_2);
        assert_eq!(
            ray_hit_quad(Vec3::new(0.0, 1.5, -1.0), Vec3::NEG_Z, side),
            None
        );
        let t = ray_hit_quad(Vec3::new(0.5, 1.5, -3.0), Vec3::NEG_X, side);
        assert!(t.is_some_and(|t| (t - 0.5).abs() < 1e-5), "face-on: {t:?}");
    }

    #[test]
    fn params_from_config_clamp_to_the_plan() {
        let p = WalkParams::from_config(2.8, 2.0, TurnMode::Smooth, 110.0, 30.0, 55.0, 1.5);
        assert_eq!(p, WalkParams::default());
        let p = WalkParams::from_config(9.0, 0.5, TurnMode::Snap, 400.0, 33.0, 120.0, 2.5);
        assert_eq!((p.speed, p.run, p.turn_speed), (5.0, 1.0, 240.0));
        assert_eq!(p.turn, TurnMode::Snap);
        assert_eq!(p.snap_step, 30.0, "the nearest of 15/30/45");
        assert_eq!(p.pitch_max, 85.0);
        assert_eq!(p.eye_height, 1.8);
        let p = WalkParams::from_config(0.0, 9.0, TurnMode::Smooth, 1.0, 10.0, 5.0, 0.2);
        assert_eq!((p.speed, p.run, p.turn_speed), (1.0, 3.0, 45.0));
        assert_eq!((p.snap_step, p.pitch_max, p.eye_height), (15.0, 10.0, 1.2));
        let p = WalkParams::from_config(
            f32::NAN,
            f32::NAN,
            TurnMode::Smooth,
            f32::NAN,
            44.0,
            f32::NAN,
            f32::NAN,
        );
        assert_eq!(p.speed, 2.8, "NaN keeps the default");
        assert_eq!(p.snap_step, 45.0);
        assert_eq!(p.eye_height, 1.5);
    }

    #[test]
    fn aabb_helpers() {
        let b = Aabb::new(Vec2::ZERO, Vec2::ONE).inflate(0.5);
        assert!(b.contains(Vec2::splat(-0.5)) && !b.contains(Vec2::splat(-0.6)));
        let _ = angle_diff(0.0, 0.0);
    }
}
