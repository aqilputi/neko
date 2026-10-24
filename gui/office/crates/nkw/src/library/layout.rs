//! Where the library's furniture goes (pure): a bookcase run per root along
//! a side wall of the library room (root 0 left, root 1 right; the back wall
//! has the librarian's counter and the vault), two stacked `Shelf` units
//! high so it has six boards of ≈ 0.35 m, books standing along each board
//! with their spines into the room.

use glam::{Mat4, Quat, Vec3};
use nkw_core::Pose;
use nkw_core::layout::{self, ROOM_DEPTH};
use nkw_core::strip::RoomSlot;

use super::model::{BOARDS, Shelf, Spine, spine};

/// One `MeshId::Shelf` unit: 0.9 m wide, 1.6 m tall, 0.32 m deep.
pub const UNIT_W: f32 = 0.9;
const MESH_H: f32 = 1.6;
const MESH_BOOK_T: f32 = 0.035;
const MESH_BOOK_H: f32 = 0.22;
/// A unit drawn this tall (two stacked make the six boards).
pub const UNIT_H: f32 = 1.05;
pub const CASE_DEPTH: f32 = 0.32;
/// The bookcase's centre off the wall.
pub const OFF_WALL: f32 = 0.18;
/// Wall margin at each end of a run.
pub const END_GAP: f32 = 0.3;
/// Gap between books.
pub const BOOK_GAP: f32 = 0.004;
/// How far you stand from the bookcase (its centre line) to browse it:
/// far enough that a whole unit and half of each neighbour fit (wave 4).
pub const STAND_M: f32 = 2.4;
/// The browse stand's pitch never exceeds this (radians).
pub const STAND_PITCH: f32 = 0.5;
/// A board's centre: this far above its top and in front of the run's line.
pub const BOARD_MID: f32 = 0.15;
pub const BOARD_FRONT: f32 = 0.1;
/// The selected book is pulled out this much.
pub const PULL_M: f32 = 0.05;
/// A unit's side panel plus a hair: books never stand behind it.
const POST: f32 = 0.05;
/// The width inside one unit.
const INNER: f32 = UNIT_W - 2.0 * POST;

/// A run of bookcases along one wall.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Run {
    /// floor point at the run's middle, `OFF_WALL` in front of the wall
    pub centre: Vec3,
    /// into the room
    pub normal: Vec3,
    /// to the right as you face the wall
    pub along: Vec3,
    pub len: f32,
}

impl Run {
    /// The yaw that looks at the wall (`ScreenPlace` convention).
    pub fn yaw(&self) -> f32 {
        // forward of yaw y is (sin y, 0, −cos y)
        let f = -self.normal;
        f.x.atan2(-f.z)
    }

    /// A world point at `x` metres along the run (0 = its left end) and `y` up.
    pub fn at(&self, x: f32, y: f32) -> Vec3 {
        self.centre + self.along * (x - self.len / 2.0) + Vec3::Y * y
    }

    /// Units that fit.
    pub fn units(&self) -> usize {
        (self.len / UNIT_W).floor().max(1.0) as usize
    }

    /// The width books can take: the units' insides, posts left out.
    pub fn usable(&self) -> f32 {
        self.units() as f32 * INNER
    }
}

/// The run of root `root` in a library room at `slot`: 0 = left wall,
/// 1 = right wall; other roots have no wall (they are browsed from the
/// palette).
pub fn run(slot: &RoomSlot, root: usize) -> Option<Run> {
    let depth = ROOM_DEPTH;
    let (x, nx) = match root {
        0 => (OFF_WALL, 1.0),
        1 => (slot.len - OFF_WALL, -1.0),
        _ => return None,
    };
    let centre = layout::to_world(slot, Vec3::new(x, 0.0, depth / 2.0));
    let normal = (layout::to_world(slot, Vec3::new(x + nx, 0.0, depth / 2.0)) - centre).normalize();
    let along = Vec3::Y.cross(normal);
    Some(Run {
        centre,
        normal,
        along,
        len: depth - 2.0 * END_GAP,
    })
}

/// The tops of the six boards, from the top board down (metres above the
/// floor), for units of `UNIT_H` drawn from the 1.6 m mesh.
pub fn floors() -> [f32; BOARDS] {
    let s = UNIT_H / MESH_H;
    let tops = [0.055, 0.575, 1.095].map(|y| y * s);
    [
        UNIT_H + tops[2],
        UNIT_H + tops[1],
        UNIT_H + tops[0],
        tops[2],
        tops[1],
        tops[0],
    ]
}

/// The physical board the shelf's first row stands on: a shelf of fewer
/// rows starts one board down, at about eye height.
pub fn first_board(rows: usize) -> usize {
    if rows >= BOARDS {
        0
    } else {
        1.min(BOARDS - rows)
    }
}

/// The tallest book a board takes.
pub fn max_book_h() -> f32 {
    let f = floors();
    (f[4] - f[5]).min(f[3] - f[4]) - 0.03
}

/// The model of every `Shelf` unit of the run (two high).
pub fn cases(r: &Run) -> Vec<Mat4> {
    let n = r.units();
    let w = n as f32 * UNIT_W;
    let rot = Quat::from_rotation_y(yaw_to(r.normal));
    let s = Vec3::new(1.0, UNIT_H / MESH_H, 1.0);
    let mut out = Vec::with_capacity(2 * n);
    for i in 0..n {
        let x = (r.len - w) / 2.0 + UNIT_W * (i as f32 + 0.5);
        for level in 0..2 {
            let p = r.at(x, level as f32 * UNIT_H);
            out.push(Mat4::from_scale_rotation_translation(s, rot, p));
        }
    }
    out
}

/// The rotation about y that turns a mesh's +z towards `n`.
pub fn yaw_to(n: Vec3) -> f32 {
    n.x.atan2(n.z)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BookAt {
    /// the book's index on the shelf
    pub index: usize,
    pub board: usize,
    pub model: Mat4,
    /// the spine's centre (labels, aim)
    pub centre: Vec3,
    pub spine: Spine,
}

/// Every book of `shelf` on the run: row-major, `shelf.cols()` per board,
/// spaced by their thickness (squeezed when a board is full); `pulled`:
/// the selected book's index, `pull` 0..1 how far out.
pub fn books(shelf: &Shelf, r: &Run, pulled: Option<usize>, pull: f32) -> Vec<BookAt> {
    let cols = shelf.cols();
    let f = floors();
    let usable = r.usable();
    let units = r.units();
    let case_left = (r.len - units as f32 * UNIT_W) / 2.0;
    let rot = Quat::from_rotation_y(yaw_to(r.normal));
    let hmax = max_book_h();
    let mut out = Vec::with_capacity(shelf.entries.len());
    let first = first_board(shelf.rows());
    for (board, row) in shelf.entries.chunks(cols).enumerate().take(BOARDS) {
        let fy = f[(board + first).min(BOARDS - 1)];
        let spines: Vec<Spine> = row.iter().map(spine).collect();
        let total: f32 = spines.iter().map(|s| s.thick + BOOK_GAP).sum();
        let squeeze = if total > usable { usable / total } else { 1.0 };
        // `u` runs along the units' insides (never behind a post); a short
        // board keeps its books in the middle of the run
        let mut u = (usable - total * squeeze).max(0.0) / 2.0;
        for (k, sp) in spines.into_iter().enumerate() {
            let index = board * cols + k;
            let t = (sp.thick + BOOK_GAP) * squeeze;
            let h = sp.height.min(hmax);
            let mut unit = ((u / INNER) as usize).min(units - 1);
            if u - unit as f32 * INNER + t > INNER && unit + 1 < units {
                unit += 1;
                u = unit as f32 * INNER;
            }
            let x = case_left + unit as f32 * UNIT_W + POST + (u - unit as f32 * INNER);
            u += t;
            let mut base = r.at(x + t / 2.0, fy);
            if pulled == Some(index) {
                base += r.normal * PULL_M * pull.clamp(0.0, 1.0);
            }
            let s = Vec3::new(
                (sp.thick * squeeze.max(0.6)) / MESH_BOOK_T,
                h / MESH_BOOK_H,
                1.0,
            );
            out.push(BookAt {
                index,
                board,
                model: Mat4::from_scale_rotation_translation(s, rot, base),
                centre: base + Vec3::Y * (h / 2.0) + r.normal * 0.08,
                spine: Spine { height: h, ..sp },
            });
        }
    }
    out
}

/// Where the "+N" box sits: after the last book of the bottom board.
pub fn more_at(r: &Run) -> Vec3 {
    let f = floors();
    r.at(
        r.len - (r.len - r.units() as f32 * UNIT_W) / 2.0 - 0.12,
        f[BOARDS - 1],
    )
}

/// The middle of a board (its pickable and the breadcrumb's anchor).
pub fn board_centre(r: &Run, board: usize) -> Vec3 {
    r.at(r.len / 2.0, floors()[board.min(BOARDS - 1)] + BOARD_MID) + r.normal * BOARD_FRONT
}

/// The board a point at height `y` stands on: the highest board top at or
/// below it (the bottom board below every top).
pub fn board_at(y: f32) -> usize {
    floors()
        .iter()
        .position(|&f| f <= y + 1e-3)
        .unwrap_or(BOARDS - 1)
}

/// The breadcrumb plate above the run.
pub fn plate_at(r: &Run) -> Vec3 {
    r.at(r.len / 2.0, 2.0 * UNIT_H + 0.12) + r.normal * 0.12
}

/// Where you stand to browse at `p` (on the run): `STAND_M` before it at
/// eye height, level, facing the wall.
pub fn stand(r: &Run, p: Vec3) -> Pose {
    let along = (p - r.centre).dot(r.along);
    let foot = r.centre + r.along * along;
    layout::stand_before(foot, r.yaw(), STAND_M)
}

/// Whether `p` is in the run's floor box (+ margin): the aim and SALTO.
pub fn hits_run(r: &Run, origin: Vec3, dir: Vec3, reach: f32) -> Option<f32> {
    // the slab of the bookcase front: a plane at OFF_WALL + depth/2 from the wall
    let front = r.centre + r.normal * (CASE_DEPTH / 2.0);
    let denom = dir.dot(-r.normal);
    if denom <= 1e-4 {
        return None;
    }
    let t = (origin - front).dot(r.normal) / denom;
    if t <= 0.0 || t > reach {
        return None;
    }
    let hit = origin + dir * t;
    let along = (hit - r.centre).dot(r.along).abs();
    (along <= r.len / 2.0 && hit.y >= 0.0 && hit.y <= 2.0 * UNIT_H).then_some(t)
}

#[cfg(test)]
mod tests {
    use super::super::model::{Config, Entry, MemFs, read_shelf};
    use super::*;
    use nkw_core::strip::Side;

    fn slot(side: Side) -> RoomSlot {
        RoomSlot {
            x0: 10.0,
            len: 6.0,
            side,
            door_x: 13.0,
        }
    }

    #[test]
    fn runs_stand_on_the_side_walls_facing_into_the_room() {
        for side in [Side::North, Side::South] {
            let s = slot(side);
            let mid = layout::to_world(&s, Vec3::new(s.len / 2.0, 0.0, ROOM_DEPTH / 2.0));
            for root in [0, 1] {
                let r = run(&s, root).unwrap();
                assert!((r.normal.length() - 1.0).abs() < 1e-5);
                // the normal points at the room's middle
                assert!((mid - r.centre).dot(r.normal) > 2.0, "{side:?} {root}");
                assert!(r.along.dot(r.normal).abs() < 1e-5);
                assert!(r.along.y.abs() < 1e-6);
                // facing it: yaw's forward is −normal
                let f = Pose::new(Vec3::ZERO, r.yaw(), 0.0).forward();
                assert!(f.dot(-r.normal) > 0.999, "{side:?} {root} {f}");
                // its right hand is `along`
                let rt = Pose::new(Vec3::ZERO, r.yaw(), 0.0).right();
                assert!(rt.dot(r.along) > 0.999, "{side:?} {root}");
                assert_eq!(r.units(), 4);
            }
            assert!(run(&s, 2).is_none());
        }
    }

    #[test]
    fn six_boards_from_the_top_and_books_that_fit_them() {
        let f = floors();
        for w in f.windows(2) {
            let gap = w[0] - w[1];
            assert!((0.3..0.4).contains(&gap), "{gap}");
        }
        assert!(f[0] < 2.0 * UNIT_H && f[5] > 0.0);
        assert!(max_book_h() >= 0.26 && max_book_h() <= 0.32);
    }

    fn shelf(n: usize) -> super::super::model::Shelf {
        let mut m = MemFs::new(1);
        for i in 0..n {
            m.file(0, &format!("f{i:03}.txt"), (i * 997) as u64);
        }
        read_shelf(&Config::default(), &m, 0, "").unwrap()
    }

    #[test]
    fn books_go_row_major_on_the_boards_and_stay_on_the_run() {
        let s = slot(Side::North);
        let r = run(&s, 0).unwrap();
        let sh = shelf(400);
        let b = books(&sh, &r, None, 0.0);
        assert_eq!(b.len(), 400);
        assert_eq!(sh.cols(), 67);
        assert_eq!(b[66].board, 0);
        assert_eq!(b[67].board, 1);
        assert_eq!(b[399].board, 5);
        for x in &b {
            let along = (x.centre - r.centre).dot(r.along);
            assert!(along.abs() <= r.len / 2.0 + 1e-3, "{along}");
            assert!((x.centre.y - floors()[x.board]) > 0.0);
        }
        // left to right along a board
        let a = (b[0].centre - r.centre).dot(r.along);
        let z = (b[1].centre - r.centre).dot(r.along);
        assert!(z > a);
        // pulling the selected book out
        let p = books(&sh, &r, Some(5), 1.0);
        let d = (p[5].centre - b[5].centre).dot(r.normal);
        assert!((d - PULL_M).abs() < 1e-4);
        assert_eq!(p[6].centre, b[6].centre);
    }

    #[test]
    fn standing_before_a_book_faces_the_wall_from_stand_m() {
        let s = slot(Side::South);
        let r = run(&s, 1).unwrap();
        let sh = shelf(12);
        let b = books(&sh, &r, None, 0.0);
        let st = stand(&r, b[3].centre);
        assert!((st.pos.y - layout::EYE).abs() < 1e-5);
        let to = b[3].centre - st.pos;
        let flat = Vec3::new(to.x, 0.0, to.z);
        assert!((flat.dot(r.normal).abs() - (STAND_M - 0.08)).abs() < 0.2);
        let f = st.forward();
        assert!(f.dot(-r.normal) > 0.99);
        // the aim from there hits the run
        assert!(hits_run(&r, st.pos, f, 4.0).is_some());
        assert!(hits_run(&r, st.pos, -f, 4.0).is_none());
        let _ = Entry {
            rel: String::new(),
            is_dir: false,
            size: 0,
            mtime: 0,
            hidden: false,
        };
    }

    #[test]
    fn no_book_hides_behind_a_post() {
        let r = run(&slot(Side::South), 0).unwrap();
        let w = r.units() as f32 * UNIT_W;
        for n in [1, 5, 8, 60, 400] {
            for b in books(&shelf(n), &r, None, 0.0) {
                let x = (b.centre - r.centre).dot(r.along) + w / 2.0;
                let in_unit = x.rem_euclid(UNIT_W);
                let half = b.spine.thick / 2.0;
                assert!(
                    in_unit - half >= POST - 0.03 && in_unit + half <= UNIT_W - POST + 0.03,
                    "{n} books: #{} at {in_unit}",
                    b.index
                );
            }
        }
    }

    #[test]
    fn board_at_finds_the_board_under_a_height() {
        let f = floors();
        for (i, top) in f.iter().enumerate() {
            assert_eq!(board_at(*top), i);
            assert_eq!(board_at(top + 0.2), i, "a book on board {i}");
        }
        assert_eq!(board_at(-1.0), BOARDS - 1);
        assert_eq!(board_at(5.0), 0);
    }

    #[test]
    fn cases_are_two_high_per_unit() {
        let r = run(&slot(Side::North), 0).unwrap();
        let c = cases(&r);
        assert_eq!(c.len(), 2 * r.units());
        let y0 = c[0].w_axis.y;
        let y1 = c[1].w_axis.y;
        assert!((y1 - y0 - UNIT_H).abs() < 1e-5);
        assert!(more_at(&r).y >= 0.0);
        assert!(plate_at(&r).y > 2.0);
        assert!(board_centre(&r, 9).y < board_centre(&r, 0).y);
    }
}
