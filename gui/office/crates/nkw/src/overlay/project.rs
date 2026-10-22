//! From the world to the overlay's plane: the camera's projection to
//! logical px, which tags show (near, in view, at most [`MAX_TAGS`]), and
//! where SALTO labels go so that none covers another.

use glam::{Mat4, Vec2, Vec3};
use nkw_core::Pick;
use nkw_gl::draw::Camera;

use crate::world::{FOV_Y, World};

/// The most name tags shown at once.
pub const MAX_TAGS: usize = 12;
/// Kept off the output's edges, logical px.
pub const EDGE: i32 = 4;

/// The camera as the overlay sees it this frame.
#[derive(Clone, Copy, Debug)]
pub struct View {
    view_proj: Mat4,
    pub cam: Vec3,
    /// logical px
    pub size: Vec2,
}

impl View {
    pub fn of(world: &World) -> View {
        let o = &world.out;
        let aspect = o.width_px as f32 / o.height_px.max(1) as f32;
        let cam = Camera {
            pose: world.camera,
            fov_y: FOV_Y,
            near: crate::scene::NEAR,
            far: crate::scene::FAR,
            aspect,
        };
        let s = o.scale.max(0.01);
        View {
            view_proj: cam.proj() * cam.view(),
            cam: world.camera.pos,
            size: Vec2::new(o.width_px as f32 / s, o.height_px as f32 / s),
        }
    }

    /// Where `p` lands, logical px from the top-left; `None` behind the
    /// camera or outside the view.
    pub fn project(&self, p: Vec3) -> Option<Vec2> {
        let c = self.view_proj * p.extend(1.0);
        if c.w <= 1e-4 {
            return None;
        }
        let n = c.truncate() / c.w;
        if n.x.abs() > 1.0 || n.y.abs() > 1.0 || n.z.abs() > 1.0 {
            return None;
        }
        Some(Vec2::new(
            (n.x + 1.0) * 0.5 * self.size.x,
            (1.0 - n.y) * 0.5 * self.size.y,
        ))
    }

    /// How far from the middle of the view `q` is (px).
    pub fn from_centre(&self, q: Vec2) -> f32 {
        q.distance(self.size * 0.5)
    }
}

/// A stable id for a SALTO target (kept across frames while it is visible).
pub fn pick_id(p: &Pick) -> u64 {
    let (kind, id) = match p {
        Pick::Screen { screen, .. } => (1, screen.0 as u64),
        Pick::Cat(c) => (2, c.0 as u64),
        Pick::Door(r) => (3, r.0 as u64),
        Pick::Book(b) => (4, b.0 as u64),
        Pick::Object { id, .. } => (5, id.0 as u64),
    };
    (kind << 56) | id
}

/// A box's top-left kept inside the output (`size`: the box, logical px).
pub fn clamp(at: (i32, i32), size: (i32, i32), out: Vec2) -> (i32, i32) {
    let maxx = (out.x as i32 - size.0 - EDGE).max(EDGE);
    let maxy = (out.y as i32 - size.1 - EDGE).max(EDGE);
    (at.0.clamp(EDGE, maxx), at.1.clamp(EDGE, maxy))
}

/// Which tags show this frame: projected, within their `max_m`, nearest
/// first, at most [`MAX_TAGS`]. Returns (index into `tags`, point, metres).
pub fn visible_tags(view: &View, tags: &[crate::ext::Tag]) -> Vec<(usize, Vec2, f32)> {
    let mut v: Vec<(usize, Vec2, f32)> = tags
        .iter()
        .enumerate()
        .filter_map(|(i, t)| {
            let d = t.pos.distance(view.cam);
            if d > t.max_m {
                return None;
            }
            view.project(t.pos).map(|q| (i, q, d))
        })
        .collect();
    v.sort_by(|a, b| a.2.total_cmp(&b.2).then(a.0.cmp(&b.0)));
    v.truncate(MAX_TAGS);
    v
}

fn overlaps(a: (i32, i32, i32, i32), b: (i32, i32, i32, i32)) -> bool {
    a.0 < b.0 + b.2 && b.0 < a.0 + a.2 && a.1 < b.1 + b.3 && b.1 < a.1 + a.3
}

/// Place labels (id, anchor point, size) so none overlaps another: each
/// goes centred on its point if it can, else to the nearest free slot
/// around it (in order: the closest first). Returns top-left corners.
pub fn place_labels(items: &[(u64, Vec2, (i32, i32))], out: Vec2) -> Vec<(u64, (i32, i32))> {
    let mut placed: Vec<(i32, i32, i32, i32)> = Vec::new();
    let mut res = Vec::new();
    for (id, at, (w, h)) in items {
        let base = (at.x.round() as i32 - w / 2, at.y.round() as i32 - h / 2);
        let mut tries = vec![(0, 0)];
        for r in 1..=10 {
            for (dx, dy) in [
                (0, -1),
                (0, 1),
                (1, 0),
                (-1, 0),
                (1, -1),
                (-1, -1),
                (1, 1),
                (-1, 1),
            ] {
                tries.push((dx * r * (w + 2), dy * r * (h + 2)));
            }
        }
        let mut chosen = clamp(base, (*w, *h), out);
        for (dx, dy) in tries {
            let p = clamp((base.0 + dx, base.1 + dy), (*w, *h), out);
            let rect = (p.0, p.1, *w, *h);
            if !placed.iter().any(|q| overlaps(*q, rect)) {
                chosen = p;
                break;
            }
        }
        placed.push((chosen.0, chosen.1, *w, *h));
        res.push((*id, chosen));
    }
    res
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ext::Tag;
    use nkw_core::{CatId, ObjectId, Pose, RoomId, ScreenId, Station};

    fn world() -> World {
        crate::render::tests::world(&[&[1, 2]])
    }

    fn ahead(w: &World, d: f32) -> Vec3 {
        let p = w.camera;
        p.pos + crate::world::facing(p.yaw) * d
    }

    #[test]
    fn project_centre_behind_and_outside() {
        let w = world();
        let v = View::of(&w);
        assert_eq!(v.size, Vec2::new(1280.0, 720.0));
        // straight ahead lands in the middle (pitch tilts it a bit)
        let q = v.project(ahead(&w, 3.0)).unwrap();
        assert!((q.x - 640.0).abs() < 1.0, "{q}");
        assert!((q.y - 360.0).abs() < 200.0, "{q}");
        assert_eq!(v.project(ahead(&w, -3.0)), None, "behind");
        let side = ahead(&w, 1.0) + Vec3::new(50.0, 0.0, 0.0);
        assert_eq!(v.project(side), None, "far outside the view");
    }

    #[test]
    fn tags_hide_beyond_max_m_and_keep_the_nearest_12() {
        let w = world();
        let v = View::of(&w);
        let tag = |id: u64, d: f32, max_m: f32| Tag {
            id,
            pos: ahead(&w, d),
            text: format!("t{id}"),
            rgb: [1, 2, 3],
            max_m,
        };
        let mut tags = vec![tag(0, 3.0, 2.0), tag(1, -3.0, 50.0)];
        for i in 0..20 {
            tags.push(tag(10 + i, 1.0 + i as f32 * 0.3, 50.0));
        }
        let vis = visible_tags(&v, &tags);
        assert_eq!(vis.len(), MAX_TAGS);
        assert!(
            vis.iter().all(|t| t.0 >= 2),
            "beyond max_m and behind are hidden"
        );
        assert!(vis.windows(2).all(|p| p[0].2 <= p[1].2), "nearest first");
        assert_eq!(vis[0].0, 2);
    }

    #[test]
    fn clamp_keeps_boxes_on_screen() {
        let out = Vec2::new(1280.0, 720.0);
        assert_eq!(clamp((-50, -50), (100, 20), out), (EDGE, EDGE));
        assert_eq!(
            clamp((1270, 715), (100, 20), out),
            (1280 - 100 - EDGE, 720 - 20 - EDGE)
        );
        assert_eq!(clamp((600, 300), (100, 20), out), (600, 300));
        // a box bigger than the output sticks to the top-left edge
        assert_eq!(clamp((10, 10), (2000, 20), out), (EDGE, 10));
    }

    #[test]
    fn twenty_labels_on_one_point_never_overlap() {
        let out = Vec2::new(1280.0, 720.0);
        let items: Vec<(u64, Vec2, (i32, i32))> = (0..20)
            .map(|i| (i, Vec2::new(640.0, 360.0), (28, 22)))
            .collect();
        let placed = place_labels(&items, out);
        assert_eq!(placed.len(), 20);
        for (i, a) in placed.iter().enumerate() {
            for b in &placed[i + 1..] {
                let ra = (a.1.0, a.1.1, 28, 22);
                let rb = (b.1.0, b.1.1, 28, 22);
                assert!(!overlaps(ra, rb), "{a:?} {b:?}");
            }
        }
        // the first sits centred on its point
        assert_eq!(placed[0].1, (640 - 14, 360 - 11));
        // also near an edge (clamped and still apart)
        let edge: Vec<_> = (0..20)
            .map(|i| (i, Vec2::new(2.0, 2.0), (28, 22)))
            .collect();
        let p = place_labels(&edge, out);
        for (i, a) in p.iter().enumerate() {
            for b in &p[i + 1..] {
                assert!(!overlaps((a.1.0, a.1.1, 28, 22), (b.1.0, b.1.1, 28, 22)));
            }
        }
    }

    #[test]
    fn pick_ids_are_distinct_per_kind() {
        let st = Station::Desk {
            room: RoomId(0),
            desk: 0,
            slot: 0,
        };
        let ids = [
            pick_id(&Pick::Screen {
                station: st,
                screen: ScreenId(1),
            }),
            pick_id(&Pick::Cat(CatId(1))),
            pick_id(&Pick::Door(RoomId(1))),
            pick_id(&Pick::Object {
                id: ObjectId(1),
                stand: Pose::default(),
            }),
        ];
        for (i, a) in ids.iter().enumerate() {
            assert!(ids[i + 1..].iter().all(|b| b != a));
        }
    }
}
