//! The procedural kit: pure geometry (no GL types).
//!
//! Toy proportions, low poly, vertex colour from a small pastel palette;
//! instances multiply it by a tint, so the parts meant to be recoloured
//! (cats, books, the generic primitives) are near-white.
//!
//! Conventions (metres, y up, counter-clockwise front faces):
//! - primitives (`Box`, `RoundedBox`, `Cylinder`, `Plane`) are unit-sized
//!   and centred on the origin (`Plane` lies on y = 0, facing +y);
//! - furniture (`Desk`, `Chair`, `Shelf`, `DoorFrame`) stands on y = 0,
//!   centred in x and z, its front towards +z (where you stand);
//! - `DoorLeaf` hangs from its hinge at x = 0 and spans x 0..0.98;
//! - `Book` stands on y = 0, spine towards +z;
//! - `MonitorFrame` is the shell behind a unit screen quad (x, y in
//!   -0.5..0.5, z < 0): scale it with the screen;
//! - cat parts are centred, the cat looking towards -z; the tail starts at
//!   the origin and runs towards +z, so it swings around its base.

use glam::{Mat3, Mat4, Vec2, Vec3};

use crate::draw::MeshId;

/// One kit vertex: 20 bytes.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub pos: [f32; 3],
    /// unit normal × 127, w = 0
    pub normal: [i8; 4],
    pub color: [u8; 4],
}

/// Indexed triangles.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Mesh {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u16>,
}

/// The pastel palette (16 named colours; `nkw-core/src/palette.rs` takes
/// over in the next wave).
pub mod palette {
    pub type Rgba = [u8; 4];
    pub const SNOW: Rgba = [250, 248, 244, 255];
    pub const CREAM: Rgba = [247, 236, 210, 255];
    pub const SAND: Rgba = [232, 214, 178, 255];
    pub const WOOD: Rgba = [214, 168, 120, 255];
    pub const WOOD_DARK: Rgba = [158, 112, 80, 255];
    pub const INK: Rgba = [96, 72, 60, 255];
    pub const LEAF: Rgba = [150, 200, 140, 255];
    pub const MINT: Rgba = [176, 226, 204, 255];
    pub const SKY: Rgba = [164, 206, 236, 255];
    pub const LILAC: Rgba = [200, 182, 228, 255];
    pub const ROSE: Rgba = [240, 168, 176, 255];
    pub const PEACH: Rgba = [248, 194, 160, 255];
    pub const BUTTER: Rgba = [250, 226, 150, 255];
    pub const CLAY: Rgba = [214, 140, 110, 255];
    pub const STONE: Rgba = [190, 188, 184, 255];
    pub const IRON: Rgba = [140, 150, 168, 255];

    pub const ALL: [(&str, Rgba); 16] = [
        ("snow", SNOW),
        ("cream", CREAM),
        ("sand", SAND),
        ("wood", WOOD),
        ("wood_dark", WOOD_DARK),
        ("ink", INK),
        ("leaf", LEAF),
        ("mint", MINT),
        ("sky", SKY),
        ("lilac", LILAC),
        ("rose", ROSE),
        ("peach", PEACH),
        ("butter", BUTTER),
        ("clay", CLAY),
        ("stone", STONE),
        ("iron", IRON),
    ];
}

/// The most triangles any kit mesh may have.
pub const MAX_TRIANGLES: u32 = 600;

impl Mesh {
    pub fn triangles(&self) -> u32 {
        (self.indices.len() / 3) as u32
    }

    /// Axis-aligned bounds (min, max) of the vertex positions.
    pub fn bounds(&self) -> (Vec3, Vec3) {
        self.vertices.iter().fold(
            (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)),
            |(lo, hi), v| {
                let p = Vec3::from(v.pos);
                (lo.min(p), hi.max(p))
            },
        )
    }

    /// Appends `other` transformed by `m` (normals by its inverse transpose;
    /// a mirroring `m` flips the winding back to counter-clockwise).
    pub fn append(&mut self, other: &Mesh, m: Mat4) {
        let base = self.vertices.len() as u16;
        let nm = Mat3::from_mat4(m).inverse().transpose();
        self.vertices.extend(other.vertices.iter().map(|v| Vertex {
            pos: m.transform_point3(Vec3::from(v.pos)).to_array(),
            normal: pack_normal(nm * unpack_normal(v.normal)),
            color: v.color,
        }));
        let flip = Mat3::from_mat4(m).determinant() < 0.0;
        for t in other.indices.chunks(3) {
            if flip {
                self.indices.extend([t[0] + base, t[2] + base, t[1] + base]);
            } else {
                self.indices.extend(t.iter().map(|i| i + base));
            }
        }
    }

    fn with(mut self, part: Mesh, at: Vec3) -> Mesh {
        self.append(&part, Mat4::from_translation(at));
        self
    }
}

/// Packs a unit normal into i8×4 (w = 0).
pub fn pack_normal(n: Vec3) -> [i8; 4] {
    let n = n.normalize_or(Vec3::Y) * 127.0;
    [n.x.round() as i8, n.y.round() as i8, n.z.round() as i8, 0]
}

pub fn unpack_normal(n: [i8; 4]) -> Vec3 {
    Vec3::new(n[0] as f32, n[1] as f32, n[2] as f32) / 127.0
}

/// A box of `size`, centred, with every edge rounded by `bevel` over
/// `segments` (1 = chamfer-like, 2 = soft) and smooth normals.
///
/// Each face is a grid whose outer strips are projected onto the rounding
/// (the "spherified cube" trick): a face covers 45° of each bevel, so
/// neighbouring faces meet on the bevel's midline without cracks.
pub fn rounded_box(size: Vec3, bevel: f32, segments: u32, color: [u8; 4]) -> Mesh {
    let half = size / 2.0;
    let r = if segments == 0 {
        0.0
    } else {
        bevel.clamp(0.0, half.min_element())
    };
    let inner = half - Vec3::splat(r);
    // samples along one axis of half-extent `h`: s bevel steps, flat middle, s bevel steps
    let samples = |h: f32| -> Vec<f32> {
        let s = segments.max(1) as f32;
        let flat = h - r;
        let mut v: Vec<f32> = (1..=segments)
            .rev()
            .map(|k| -flat - r * k as f32 / s)
            .collect();
        v.push(-flat);
        v.push(flat);
        v.extend((1..=segments).map(|k| flat + r * k as f32 / s));
        if segments == 0 {
            v = vec![-h, h];
        }
        v
    };
    let faces = [
        (Vec3::X, Vec3::NEG_Z, Vec3::Y),
        (Vec3::NEG_X, Vec3::Z, Vec3::Y),
        (Vec3::Y, Vec3::X, Vec3::NEG_Z),
        (Vec3::NEG_Y, Vec3::X, Vec3::Z),
        (Vec3::Z, Vec3::X, Vec3::Y),
        (Vec3::NEG_Z, Vec3::NEG_X, Vec3::Y),
    ];
    let mut m = Mesh::default();
    for (n, u, v) in faces {
        let us = samples(u.abs().dot(half));
        let vs = samples(v.abs().dot(half));
        let base = m.vertices.len() as u16;
        for &b in &vs {
            for &a in &us {
                let p = n * n.abs().dot(half) + u * a + v * b;
                let core = p.clamp(-inner, inner);
                let d = p - core;
                let (pos, normal) = if d.length() < 1e-7 {
                    (p, n)
                } else {
                    (core + d.normalize() * r, d.normalize())
                };
                m.vertices.push(Vertex {
                    pos: pos.to_array(),
                    normal: pack_normal(normal),
                    color,
                });
            }
        }
        let w = us.len() as u16;
        for j in 0..vs.len() as u16 - 1 {
            for i in 0..w - 1 {
                let a = base + j * w + i;
                m.indices.extend([a, a + 1, a + 1 + w, a, a + 1 + w, a + w]);
            }
        }
    }
    m
}

/// A sharp box of `size`, centred, flat normals.
pub fn cuboid(size: Vec3, color: [u8; 4]) -> Mesh {
    rounded_box(size, 0.0, 0, color)
}

/// A capped cylinder along y, centred, smooth sides and flat caps.
pub fn cylinder(radius: f32, height: f32, sides: u32, color: [u8; 4]) -> Mesh {
    let n = sides.max(3) as u16;
    let h = height / 2.0;
    let ring = |i: u16| {
        let a = std::f32::consts::TAU * i as f32 / n as f32;
        Vec3::new(a.cos(), 0.0, -a.sin())
    };
    let mut m = Mesh::default();
    let vtx = |p: Vec3, nrm: Vec3| Vertex {
        pos: p.to_array(),
        normal: pack_normal(nrm),
        color,
    };
    for i in 0..n {
        let d = ring(i);
        m.vertices.push(vtx(d * radius - Vec3::Y * h, d));
        m.vertices.push(vtx(d * radius + Vec3::Y * h, d));
    }
    for i in 0..n {
        let (b0, t0) = (2 * i, 2 * i + 1);
        let (b1, t1) = (2 * ((i + 1) % n), 2 * ((i + 1) % n) + 1);
        m.indices.extend([b0, b1, t1, b0, t1, t0]);
    }
    for (y, nrm) in [(h, Vec3::Y), (-h, Vec3::NEG_Y)] {
        let base = m.vertices.len() as u16;
        for i in 0..n {
            m.vertices.push(vtx(ring(i) * radius + Vec3::Y * y, nrm));
        }
        for i in 1..n - 1 {
            if y > 0.0 {
                m.indices.extend([base, base + i, base + i + 1]);
            } else {
                m.indices.extend([base, base + i + 1, base + i]);
            }
        }
    }
    m
}

/// A plane on y = 0 facing +y, centred.
pub fn plane(size: Vec2, color: [u8; 4]) -> Mesh {
    let (x, z) = (size.x / 2.0, size.y / 2.0);
    let normal = pack_normal(Vec3::Y);
    let vertices = [[-x, 0.0, -z], [-x, 0.0, z], [x, 0.0, z], [x, 0.0, -z]]
        .map(|pos| Vertex { pos, normal, color })
        .to_vec();
    Mesh {
        vertices,
        indices: vec![0, 1, 2, 0, 2, 3],
    }
}

/// The kit mesh for `id`.
pub fn build(id: MeshId) -> Mesh {
    use palette::*;
    let v = Vec3::new;
    let rb = rounded_box;
    let empty = Mesh::default;
    match id {
        MeshId::Box => cuboid(Vec3::ONE, SNOW),
        MeshId::RoundedBox => rb(Vec3::ONE, 0.12, 2, SNOW),
        MeshId::Cylinder => cylinder(0.5, 1.0, 16, SNOW),
        MeshId::Plane => plane(Vec2::ONE, SNOW),
        MeshId::Desk => {
            let mut m = empty().with(rb(v(1.6, 0.06, 0.8), 0.025, 1, WOOD), v(0.0, 0.72, 0.0));
            for (x, z) in [(-0.72, -0.32), (0.72, -0.32), (-0.72, 0.32), (0.72, 0.32)] {
                m = m.with(cylinder(0.035, 0.69, 12, WOOD_DARK), v(x, 0.345, z));
            }
            m.with(rb(v(0.42, 0.12, 0.6), 0.02, 1, CREAM), v(0.45, 0.63, 0.0))
        }
        MeshId::MonitorFrame => {
            empty().with(rb(v(1.0, 1.0, 0.075), 0.03, 1, CREAM), v(0.0, 0.0, -0.0425))
        }
        MeshId::Chair => {
            // a cushioned seat and a low back pad on two posts: the villager's
            // back, tail and ears stay in view from the station (wave 2)
            let mut m = empty()
                .with(rb(v(0.44, 0.07, 0.42), 0.025, 1, LILAC), v(0.0, 0.435, 0.0))
                .with(rb(v(0.32, 0.13, 0.05), 0.02, 1, WOOD), v(0.0, 0.7, 0.19));
            for x in [-0.12, 0.12] {
                m = m.with(cuboid(v(0.025, 0.2, 0.025), WOOD_DARK), v(x, 0.57, 0.19));
            }
            for (x, z) in [(-0.18, -0.17), (0.18, -0.17), (-0.18, 0.17), (0.18, 0.17)] {
                m = m.with(cylinder(0.022, 0.4, 10, WOOD_DARK), v(x, 0.2, z));
            }
            m
        }
        MeshId::Shelf => {
            let mut m = empty()
                .with(rb(v(0.04, 1.6, 0.32), 0.015, 1, WOOD), v(-0.43, 0.8, 0.0))
                .with(rb(v(0.04, 1.6, 0.32), 0.015, 1, WOOD), v(0.43, 0.8, 0.0))
                .with(cuboid(v(0.82, 1.56, 0.02), SAND), v(0.0, 0.8, -0.15));
            for y in [0.04, 0.56, 1.08, 1.56] {
                m = m.with(cuboid(v(0.82, 0.03, 0.3), WOOD), v(0.0, y, 0.0));
            }
            m
        }
        MeshId::Book => empty().with(rb(v(0.035, 0.22, 0.16), 0.006, 1, SNOW), v(0.0, 0.11, 0.0)),
        MeshId::DoorFrame => empty()
            .with(
                rb(v(0.1, 2.2, 0.16), 0.02, 1, WOOD_DARK),
                v(-0.55, 1.1, 0.0),
            )
            .with(rb(v(0.1, 2.2, 0.16), 0.02, 1, WOOD_DARK), v(0.55, 1.1, 0.0))
            .with(rb(v(1.0, 0.1, 0.16), 0.02, 1, WOOD_DARK), v(0.0, 2.15, 0.0)),
        MeshId::DoorLeaf => empty()
            .with(rb(v(0.98, 2.08, 0.05), 0.015, 1, PEACH), v(0.49, 1.04, 0.0))
            .with(rb(v(0.05, 0.05, 0.03), 0.012, 1, IRON), v(0.86, 1.0, 0.045))
            .with(
                rb(v(0.05, 0.05, 0.03), 0.012, 1, IRON),
                v(0.86, 1.0, -0.045),
            ),
        MeshId::CatBody => rb(v(0.30, 0.26, 0.38), 0.11, 2, SNOW),
        MeshId::CatHead => empty()
            .with(rb(v(0.24, 0.21, 0.21), 0.09, 2, SNOW), Vec3::ZERO)
            .with(
                rb(v(0.04, 0.03, 0.03), 0.012, 1, ROSE),
                v(0.0, -0.02, -0.115),
            ),
        MeshId::CatEar => rb(v(0.07, 0.08, 0.03), 0.012, 1, SNOW),
        MeshId::CatTail => empty().with(rb(v(0.05, 0.05, 0.32), 0.022, 1, SNOW), v(0.0, 0.0, 0.16)),
        other => crate::kit::build(other).expect("every wave-2 mesh has a builder"),
    }
}

/// Triangles of the kit mesh for `id` (computed once).
pub fn triangle_count(id: MeshId) -> u32 {
    static COUNTS: std::sync::OnceLock<[u32; MeshId::COUNT]> = std::sync::OnceLock::new();
    COUNTS.get_or_init(|| MeshId::ALL.map(|id| build(id).triangles()))[id.index()]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: Vec3, b: Vec3, eps: f32) -> bool {
        (a - b).abs().max_element() <= eps
    }

    #[test]
    fn vertex_is_20_bytes_and_tightly_packed() {
        assert_eq!(std::mem::size_of::<Vertex>(), 20);
        assert_eq!(std::mem::align_of::<Vertex>(), 4);
        assert_eq!(std::mem::offset_of!(Vertex, normal), 12);
        assert_eq!(std::mem::offset_of!(Vertex, color), 16);
    }

    #[test]
    fn normals_pack_within_one_step() {
        for n in [
            Vec3::X,
            Vec3::NEG_Y,
            Vec3::new(1.0, 2.0, -3.0).normalize(),
            Vec3::ONE.normalize(),
        ] {
            let p = pack_normal(n);
            assert_eq!(p[3], 0);
            assert!(
                (unpack_normal(p) - n).length() <= 1.0 / 127.0,
                "{n} → {p:?}"
            );
        }
        assert_eq!(pack_normal(Vec3::X), [127, 0, 0, 0]);
        assert_eq!(pack_normal(Vec3::NEG_Z), [0, 0, -127, 0]);
    }

    #[test]
    fn primitives_have_the_expected_counts() {
        let c = cuboid(Vec3::ONE, palette::SNOW);
        assert_eq!((c.vertices.len(), c.triangles()), (24, 12));
        let p = plane(Vec2::ONE, palette::SNOW);
        assert_eq!((p.vertices.len(), p.triangles()), (4, 2));
        // 16 sides: 2 per side quad, 14 per cap fan
        let cy = cylinder(0.5, 1.0, 16, palette::SNOW);
        assert_eq!(cy.triangles(), 16 * 2 + 14 * 2);
        // segments s: (2s+1)² quads per face
        let r1 = rounded_box(Vec3::ONE, 0.1, 1, palette::SNOW);
        assert_eq!(r1.triangles(), 6 * 9 * 2);
        let r2 = rounded_box(Vec3::ONE, 0.1, 2, palette::SNOW);
        assert_eq!(r2.triangles(), 6 * 25 * 2);
    }

    #[test]
    fn rounded_box_keeps_its_size_and_rounds_corners() {
        let size = Vec3::new(1.6, 0.06, 0.8);
        let m = rounded_box(size, 0.025, 2, palette::WOOD);
        let (lo, hi) = m.bounds();
        assert!(
            near(lo, -size / 2.0, 1e-5) && near(hi, size / 2.0, 1e-5),
            "{lo} {hi}"
        );
        // no vertex sits on the sharp corner
        let corner = size / 2.0;
        assert!(
            m.vertices
                .iter()
                .all(|v| (Vec3::from(v.pos) - corner).length() > 0.005)
        );
        // a bevel bigger than the box is clamped, not inverted
        let tiny = rounded_box(Vec3::splat(0.1), 1.0, 2, palette::SNOW);
        let (lo, hi) = tiny.bounds();
        assert!(near(lo, Vec3::splat(-0.05), 1e-5) && near(hi, Vec3::splat(0.05), 1e-5));
    }

    #[test]
    fn append_transforms_positions_and_normals() {
        let mut m = Mesh::default();
        m.append(
            &plane(Vec2::ONE, palette::SNOW),
            Mat4::from_translation(Vec3::Y),
        );
        m.append(
            &plane(Vec2::ONE, palette::SNOW),
            Mat4::from_rotation_x(std::f32::consts::FRAC_PI_2)
                * Mat4::from_scale(Vec3::new(2.0, 1.0, 0.5)),
        );
        assert_eq!(m.triangles(), 4);
        assert!(
            m.indices[6..].iter().all(|&i| i >= 4),
            "second part indexes its own vertices"
        );
        assert!(m.vertices[..4].iter().all(|v| v.pos[1] == 1.0));
        // +y rotated 90° about x points to +z
        assert!(
            m.vertices[4..]
                .iter()
                .all(|v| unpack_normal(v.normal).distance(Vec3::Z) < 0.01)
        );
    }

    fn expected_bounds(id: MeshId) -> (Vec3, Vec3) {
        use MeshId::*;
        let v = Vec3::new;
        match id {
            Box | RoundedBox => (Vec3::splat(-0.5), Vec3::splat(0.5)),
            Cylinder => (Vec3::splat(-0.5), Vec3::splat(0.5)),
            Plane => (v(-0.5, 0.0, -0.5), v(0.5, 0.0, 0.5)),
            Desk => (v(-0.8, 0.0, -0.4), v(0.8, 0.75, 0.4)),
            MonitorFrame => (v(-0.5, -0.5, -0.08), v(0.5, 0.5, -0.005)),
            Chair => (v(-0.22, 0.0, -0.21), v(0.22, 0.765, 0.215)),
            Shelf => (v(-0.45, 0.0, -0.16), v(0.45, 1.6, 0.16)),
            Book => (v(-0.0175, 0.0, -0.08), v(0.0175, 0.22, 0.08)),
            DoorFrame => (v(-0.6, 0.0, -0.08), v(0.6, 2.2, 0.08)),
            DoorLeaf => (v(0.0, 0.0, -0.06), v(0.98, 2.08, 0.06)),
            CatBody => (v(-0.15, -0.13, -0.19), v(0.15, 0.13, 0.19)),
            CatHead => (v(-0.12, -0.105, -0.13), v(0.12, 0.105, 0.105)),
            CatEar => (v(-0.035, -0.04, -0.015), v(0.035, 0.04, 0.015)),
            CatTail => (v(-0.025, -0.025, 0.0), v(0.025, 0.025, 0.32)),
            // wave 2: inside the stated box and filling ≥ 80 % of it on every axis
            other => crate::kit::stated_bounds(other).expect("every wave-2 mesh states its box"),
        }
    }

    #[test]
    fn every_kit_mesh_fits_its_bounds_and_budget() {
        for id in MeshId::ALL {
            let m = build(id);
            let (lo, hi) = m.bounds();
            let (elo, ehi) = expected_bounds(id);
            if id.index() < 15 {
                assert!(
                    near(lo, elo, 2e-3) && near(hi, ehi, 2e-3),
                    "{id:?}: {lo} {hi}, want {elo} {ehi}"
                );
            } else {
                let inside = lo.cmpge(elo - 1e-3).all() && hi.cmple(ehi + 1e-3).all();
                let fill = (hi - lo) / (ehi - elo);
                assert!(
                    inside && fill.min_element() >= 0.8,
                    "{id:?}: {lo} {hi} in {elo} {ehi}, fill {fill}"
                );
            }
            assert!(
                m.triangles() > 0 && m.triangles() <= MAX_TRIANGLES,
                "{id:?}: {} tris",
                m.triangles()
            );
            assert!(m.vertices.len() < u16::MAX as usize);
            assert!(
                m.indices.iter().all(|&i| (i as usize) < m.vertices.len()),
                "{id:?}"
            );
            assert_eq!(triangle_count(id), m.triangles());
        }
    }

    #[test]
    fn every_kit_normal_is_unit_and_faces_out_of_its_triangle() {
        for id in MeshId::ALL {
            let m = build(id);
            for v in &m.vertices {
                let n = unpack_normal(v.normal);
                assert!(
                    (n.length() - 1.0).abs() <= 1.0 / 127.0,
                    "{id:?}: |n| = {}",
                    n.length()
                );
                assert_eq!(v.normal[3], 0);
            }
            // counter-clockwise winding: the face normal agrees with the vertex normals
            for t in m.indices.chunks(3) {
                let [a, b, c] = [t[0], t[1], t[2]].map(|i| m.vertices[i as usize]);
                let face = (Vec3::from(b.pos) - Vec3::from(a.pos))
                    .cross(Vec3::from(c.pos) - Vec3::from(a.pos));
                if face.length() < 1e-9 {
                    continue;
                }
                let avg =
                    unpack_normal(a.normal) + unpack_normal(b.normal) + unpack_normal(c.normal);
                assert!(
                    face.dot(avg) > 0.0,
                    "{id:?}: triangle {t:?} wound clockwise"
                );
            }
        }
    }

    #[test]
    fn recolourable_parts_are_near_white() {
        for id in [
            MeshId::Box,
            MeshId::RoundedBox,
            MeshId::Cylinder,
            MeshId::Plane,
            MeshId::Book,
            MeshId::CatBody,
            MeshId::CatEar,
            MeshId::CatTail,
        ] {
            assert!(
                build(id).vertices.iter().all(|v| v.color == palette::SNOW),
                "{id:?}"
            );
        }
    }

    #[test]
    fn palette_names_are_unique() {
        let mut names: Vec<_> = palette::ALL.iter().map(|(n, _)| *n).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), 16);
    }
}
