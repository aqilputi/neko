//! The pure DrawList: what one world pass draws (no GL types).

use glam::{Mat4, Vec3};
use nkw_core::Pose;

#[derive(Clone, Debug, PartialEq)]
pub struct DrawList {
    pub camera: Camera,
    pub sky: Sky,
    /// kit meshes (rooms, desks, monitors, cat placeholders …)
    pub instances: Vec<Instance>,
    /// client windows as textured quads
    pub screens: Vec<ScreenQuad>,
    /// blob shadows
    pub blobs: Vec<Blob>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    pub pose: Pose,
    /// radians
    pub fov_y: f32,
    pub near: f32,
    pub far: f32,
    pub aspect: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sky {
    pub color: [f32; 3],
    pub fog_color: [f32; 3],
    pub fog_start: f32,
    /// 40 m
    pub fog_end: f32,
    pub sun_dir: Vec3,
    pub sun_color: [f32; 3],
    pub ambient_sky: [f32; 3],
    pub ambient_ground: [f32; 3],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MeshId {
    Box,
    RoundedBox,
    Cylinder,
    Plane,
    Desk,
    MonitorFrame,
    Chair,
    Shelf,
    Book,
    DoorFrame,
    DoorLeaf,
    CatBody,
    CatHead,
    CatEar,
    CatTail,
}

impl MeshId {
    pub const ALL: [MeshId; 15] = [
        MeshId::Box,
        MeshId::RoundedBox,
        MeshId::Cylinder,
        MeshId::Plane,
        MeshId::Desk,
        MeshId::MonitorFrame,
        MeshId::Chair,
        MeshId::Shelf,
        MeshId::Book,
        MeshId::DoorFrame,
        MeshId::DoorLeaf,
        MeshId::CatBody,
        MeshId::CatHead,
        MeshId::CatEar,
        MeshId::CatTail,
    ];

    /// Position in [`MeshId::ALL`].
    pub fn index(self) -> usize {
        self as usize
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Instance {
    pub mesh: MeshId,
    pub model: Mat4,
    /// rgba, multiplies the vertex colour
    pub tint: [u8; 4],
    pub emissive: f32,
    /// 0..1 dither-out near the camera
    pub fade: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClientTex {
    pub id: u32,
    /// GL_TEXTURE_EXTERNAL_OES
    pub external: bool,
    pub size: [u32; 2],
    pub y_flip: bool,
    pub opaque: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScreenQuad {
    /// unit quad, centre origin, +y up, facing -z
    pub model: Mat4,
    pub tex: ClientTex,
    /// 0 = normal, 0.2 = FOCO surroundings
    pub dim: f32,
    /// soft outline
    pub selected: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Blob {
    pub center: Vec3,
    pub radius: f32,
    pub alpha: f32,
}

impl DrawList {
    pub fn empty(camera: Camera, sky: Sky) -> DrawList {
        DrawList {
            camera,
            sky,
            instances: Vec::new(),
            screens: Vec::new(),
            blobs: Vec::new(),
        }
    }

    /// Triangles this list submits (kit meshes + 2 per screen and blob).
    pub fn triangle_estimate(&self) -> u32 {
        let kit: u32 = self
            .instances
            .iter()
            .map(|i| crate::mesh::triangle_count(i.mesh))
            .sum();
        kit + 2 * (self.screens.len() + self.blobs.len()) as u32
    }
}

impl Camera {
    pub fn view(&self) -> Mat4 {
        self.pose.view()
    }

    pub fn proj(&self) -> Mat4 {
        glam::camera::rh::proj::opengl::perspective(self.fov_y, self.aspect, self.near, self.far)
    }
}

/// The sky for a time-of-day band (`band % 8`; band `b` covers the local
/// hours `3b..3b+3`, so 0 is deep night and 3/4 straddle noon).
///
/// The sun travels east to west along an arc tilted towards +z (where you
/// stand), so its height rises until noon and falls after; at night the
/// "sun" is a dim bluish moonlight and the warm ambient (lamps) carries the
/// room.
pub fn sky_for_band(band: u8) -> Sky {
    // (sky, sun, ambient sky, ambient ground)
    const BANDS: [[[f32; 3]; 4]; 8] = [
        [
            [0.16, 0.18, 0.30],
            [0.20, 0.22, 0.35],
            [0.36, 0.36, 0.50],
            [0.30, 0.25, 0.26],
        ],
        [
            [0.56, 0.52, 0.66],
            [0.70, 0.56, 0.50],
            [0.50, 0.48, 0.58],
            [0.36, 0.30, 0.30],
        ],
        [
            [0.70, 0.82, 0.92],
            [1.00, 0.90, 0.76],
            [0.56, 0.60, 0.68],
            [0.40, 0.34, 0.30],
        ],
        [
            [0.62, 0.78, 0.94],
            [1.00, 0.96, 0.88],
            [0.58, 0.62, 0.70],
            [0.42, 0.36, 0.32],
        ],
        [
            [0.64, 0.79, 0.93],
            [1.00, 0.95, 0.84],
            [0.58, 0.62, 0.70],
            [0.42, 0.36, 0.32],
        ],
        [
            [0.86, 0.80, 0.70],
            [1.00, 0.84, 0.62],
            [0.58, 0.56, 0.58],
            [0.44, 0.34, 0.28],
        ],
        [
            [0.80, 0.62, 0.68],
            [0.98, 0.68, 0.52],
            [0.58, 0.52, 0.62],
            [0.44, 0.34, 0.32],
        ],
        [
            [0.20, 0.20, 0.34],
            [0.24, 0.24, 0.38],
            [0.38, 0.36, 0.50],
            [0.30, 0.25, 0.26],
        ],
    ];
    const WARM: [f32; 3] = [0.96, 0.90, 0.80];
    let b = band % 8;
    let [color, sun_color, ambient_sky, ambient_ground] = BANDS[b as usize];
    let hour = 3.0 * b as f32 + 1.5;
    let t = std::f32::consts::PI * (hour - 6.0) / 12.0;
    let fog_color = std::array::from_fn(|i| color[i] * 0.8 + WARM[i] * 0.2);
    Sky {
        color,
        fog_color,
        fog_start: 14.0,
        fog_end: 40.0,
        sun_dir: Vec3::new(t.cos(), t.sin(), 0.5).normalize(),
        sun_color,
        ambient_sky,
        ambient_ground,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh;

    fn camera() -> Camera {
        Camera {
            pose: Pose::new(Vec3::new(0.0, 1.5, 3.0), 0.2, -0.1),
            fov_y: 1.0,
            near: 0.05,
            far: 60.0,
            aspect: 16.0 / 9.0,
        }
    }

    fn inst(mesh: MeshId) -> Instance {
        Instance {
            mesh,
            model: Mat4::IDENTITY,
            tint: [255; 4],
            emissive: 0.0,
            fade: 0.0,
        }
    }

    fn tex() -> ClientTex {
        ClientTex {
            id: 1,
            external: false,
            size: [64, 64],
            y_flip: false,
            opaque: true,
        }
    }

    #[test]
    fn mesh_ids_index_their_own_slot() {
        for (i, id) in MeshId::ALL.iter().enumerate() {
            assert_eq!(id.index(), i);
        }
    }

    #[test]
    fn triangle_estimate_sums_meshes_screens_and_blobs() {
        let mut l = DrawList::empty(camera(), sky_for_band(4));
        assert_eq!(l.triangle_estimate(), 0);
        l.instances = vec![
            inst(MeshId::Desk),
            inst(MeshId::Desk),
            inst(MeshId::CatBody),
        ];
        l.screens = vec![ScreenQuad {
            model: Mat4::IDENTITY,
            tex: tex(),
            dim: 0.0,
            selected: false,
        }];
        l.blobs = vec![
            Blob {
                center: Vec3::ZERO,
                radius: 0.3,
                alpha: 0.4
            };
            3
        ];
        let want = 2 * mesh::triangle_count(MeshId::Desk)
            + mesh::triangle_count(MeshId::CatBody)
            + 2
            + 3 * 2;
        assert_eq!(l.triangle_estimate(), want);
    }

    #[test]
    fn camera_matrices_match_glam() {
        let c = camera();
        assert_eq!(c.view(), c.pose.view());
        #[allow(deprecated)]
        let want = Mat4::perspective_rh_gl(c.fov_y, c.aspect, c.near, c.far);
        assert!(c.proj().abs_diff_eq(want, 1e-6));
    }

    fn sun_height(band: u8) -> f32 {
        sky_for_band(band).sun_dir.y
    }

    #[test]
    fn sun_rises_until_midday_and_sets_after() {
        for b in 0..3 {
            assert!(sun_height(b) < sun_height(b + 1), "band {b}");
        }
        for b in 4..7 {
            assert!(sun_height(b) > sun_height(b + 1), "band {b}");
        }
        assert!(sun_height(0) < 0.0 && sun_height(3) > 0.5 && sun_height(4) > 0.5);
        assert!(
            (sun_height(3) - sun_height(4)).abs() < 1e-5,
            "symmetric around noon"
        );
        // out of range wraps instead of panicking
        assert_eq!(sky_for_band(8), sky_for_band(0));
    }

    #[test]
    fn every_band_is_a_sane_sky() {
        let lum = |c: [f32; 3]| c[0] * 0.3 + c[1] * 0.59 + c[2] * 0.11;
        for b in 0..8 {
            let s = sky_for_band(b);
            assert!((s.sun_dir.length() - 1.0).abs() < 1e-5, "band {b}");
            assert!(s.fog_start < s.fog_end && (s.fog_end - 40.0).abs() < 1e-6);
            for c in [
                s.color,
                s.fog_color,
                s.sun_color,
                s.ambient_sky,
                s.ambient_ground,
            ] {
                assert!(c.iter().all(|v| (0.0..=1.0).contains(v)), "band {b}: {c:?}");
            }
        }
        // night is darker than day
        assert!(lum(sky_for_band(0).color) < lum(sky_for_band(4).color));
        assert!(lum(sky_for_band(0).sun_color) < lum(sky_for_band(4).sun_color));
    }
}
