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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
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
    // S10 (wave 2): the monitor body, the villagers' parts, the furniture
    MonitorStand,
    MonitorFoot,
    CatMuzzle,
    CatPawFront,
    CatPawBack,
    CatChest,
    CatStripe,
    CatLaptop,
    Envelope,
    Turntable,
    Platter,
    ToneArm,
    Speaker,
    WallClock,
    ClockHand,
    OilJar,
    Cushion,
    Plant,
    Pot,
    Fireplace,
    Flame,
    Aquarium,
    Calendar,
    Lamp,
    Safe,
    SafeDoor,
    Jar,
    Board,
    // additive to S10: parts `rig` needs that S10 does not name
    CatTorso,
    CatSkull,
    CatEarOuter,
    CatEye,
    CatEarInner,
    CatTailSeg,
    CatPatch,
    CatCollar,
    Ball,
    Knob,
    Fish,
    Package,
    OnAir,
    // S17 (wave 3): the room shell
    FloorPlanks,
    FloorTiles,
    Rug,
    WallPanel,
    Ceiling,
    WindowFrame,
    WindowPane,
    Lintel,
    Skirting,
    // additive to S17: the two patterned wallpapers (`WallPanel` is the plain one)
    WallStripes,
    WallDots,
    /// the window's light on the floor: four panes' worth, the glazing cross as gaps
    LightPatch,
}

impl MeshId {
    /// How many kit meshes there are.
    pub const COUNT: usize = 68;

    pub const ALL: [MeshId; MeshId::COUNT] = [
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
        MeshId::MonitorStand,
        MeshId::MonitorFoot,
        MeshId::CatMuzzle,
        MeshId::CatPawFront,
        MeshId::CatPawBack,
        MeshId::CatChest,
        MeshId::CatStripe,
        MeshId::CatLaptop,
        MeshId::Envelope,
        MeshId::Turntable,
        MeshId::Platter,
        MeshId::ToneArm,
        MeshId::Speaker,
        MeshId::WallClock,
        MeshId::ClockHand,
        MeshId::OilJar,
        MeshId::Cushion,
        MeshId::Plant,
        MeshId::Pot,
        MeshId::Fireplace,
        MeshId::Flame,
        MeshId::Aquarium,
        MeshId::Calendar,
        MeshId::Lamp,
        MeshId::Safe,
        MeshId::SafeDoor,
        MeshId::Jar,
        MeshId::Board,
        MeshId::CatTorso,
        MeshId::CatSkull,
        MeshId::CatEarOuter,
        MeshId::CatEye,
        MeshId::CatEarInner,
        MeshId::CatTailSeg,
        MeshId::CatPatch,
        MeshId::CatCollar,
        MeshId::Ball,
        MeshId::Knob,
        MeshId::Fish,
        MeshId::Package,
        MeshId::OnAir,
        MeshId::FloorPlanks,
        MeshId::FloorTiles,
        MeshId::Rug,
        MeshId::WallPanel,
        MeshId::Ceiling,
        MeshId::WindowFrame,
        MeshId::WindowPane,
        MeshId::Lintel,
        MeshId::Skirting,
        MeshId::WallStripes,
        MeshId::WallDots,
        MeshId::LightPatch,
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
    // (sky, sun, ambient sky, ambient ground). Interiors carry the mood: the
    // ambient is cool blue-grey at dawn (1), clear and bright by day (2–4),
    // warm at late afternoon and sunset (5–6), and at night (7, 0) a dim
    // warm lamp-light under a blue moon.
    const BANDS: [[[f32; 3]; 4]; 8] = [
        [
            [0.14, 0.16, 0.28],
            [0.30, 0.30, 0.44],
            [0.50, 0.46, 0.52],
            [0.46, 0.37, 0.32],
        ],
        [
            [0.60, 0.64, 0.76],
            [0.72, 0.70, 0.78],
            [0.62, 0.64, 0.74],
            [0.46, 0.45, 0.50],
        ],
        [
            [0.70, 0.82, 0.92],
            [1.00, 0.93, 0.82],
            [0.70, 0.72, 0.78],
            [0.54, 0.48, 0.44],
        ],
        [
            [0.62, 0.78, 0.94],
            [1.00, 0.97, 0.90],
            [0.74, 0.76, 0.80],
            [0.56, 0.50, 0.46],
        ],
        [
            [0.64, 0.79, 0.93],
            [1.00, 0.96, 0.86],
            [0.74, 0.75, 0.78],
            [0.57, 0.50, 0.45],
        ],
        [
            [0.90, 0.80, 0.66],
            [1.00, 0.84, 0.62],
            [0.76, 0.68, 0.62],
            [0.58, 0.47, 0.39],
        ],
        [
            [0.82, 0.64, 0.66],
            [0.96, 0.72, 0.56],
            [0.70, 0.60, 0.60],
            [0.56, 0.44, 0.38],
        ],
        [
            [0.18, 0.18, 0.32],
            [0.32, 0.30, 0.44],
            [0.52, 0.46, 0.50],
            [0.48, 0.38, 0.33],
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

/// The light through a window at `band`: the pane's tint and how much it
/// glows (0..1). Warm late afternoon (5–6), cool at dawn (1–2), pale
/// daylight around noon, dark blue at night (7, 0) with the lamps' 0.15.
pub fn window_light(band: u8) -> ([u8; 4], f32) {
    const PANES: [([u8; 4], f32); 8] = [
        ([70, 82, 128, 255], 0.15),
        ([178, 196, 226, 255], 0.45),
        ([196, 224, 246, 255], 0.65),
        ([188, 222, 248, 255], 0.75),
        ([200, 226, 244, 255], 0.75),
        ([255, 222, 168, 255], 0.75),
        ([255, 190, 150, 255], 0.7),
        ([92, 96, 146, 255], 0.15),
    ];
    PANES[(band % 8) as usize]
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

    #[test]
    fn interiors_are_warm_late_and_blue_grey_at_dawn() {
        // warmth of the light a wall facing you gets: red minus blue of the ambient's middle
        let warmth = |b: u8| {
            let s = sky_for_band(b);
            let m = |i: usize| (s.ambient_sky[i] + s.ambient_ground[i]) / 2.0;
            m(0) - m(2)
        };
        assert!(warmth(1) < 0.0, "dawn is cool: {}", warmth(1));
        for b in [5, 6] {
            assert!(warmth(b) > 0.1, "band {b}: {}", warmth(b));
            assert!(warmth(b) > warmth(3) + 0.05);
        }
        // the day is the brightest inside
        let lum = |b: u8| {
            let s = sky_for_band(b);
            (0..3)
                .map(|i| s.ambient_sky[i] + s.ambient_ground[i])
                .sum::<f32>()
        };
        for b in [0, 1, 5, 6, 7] {
            assert!(lum(b) < lum(3), "band {b}");
        }
    }

    #[test]
    fn window_light_warms_through_the_afternoon_and_glows_at_night() {
        let warmth = |b: u8| {
            let (c, _) = window_light(b);
            c[0] as i32 - c[2] as i32
        };
        for b in 3..6 {
            assert!(warmth(b) < warmth(b + 1), "band {b}");
        }
        assert!(warmth(1) < 0 && warmth(2) < 0, "cool mornings");
        for b in [7u8, 0] {
            let (c, e) = window_light(b);
            assert!(e >= 0.1, "lamps at night");
            assert!(c[0] < 120 && c[2] > c[0], "dark blue night: {c:?}");
        }
        for b in 1..7 {
            assert!(window_light(b).1 > window_light(0).1);
        }
        assert_eq!(window_light(8), window_light(0));
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
        assert!(lum(sky_for_band(7).ambient_sky) < lum(sky_for_band(3).ambient_sky));
        assert!(lum(sky_for_band(0).sun_color) < lum(sky_for_band(4).sun_color));
    }
}
