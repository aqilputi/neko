//! The frozen public API: S1 (phase 0) and S10 (wave 2) as their consumers
//! name it. Compiling is the test: a changed signature breaks this file
//! before it breaks `nkw`.

use glam::{Mat4, Vec2, Vec3};
use nkw_core::Pose;
use nkw_gl::draw::{
    self, Blob, Camera, ClientTex, DrawList, Instance, MeshId, ScreenQuad, Sky, sky_for_band,
};
use nkw_gl::rig::{self, CatJoints, Coat, Ear, Pattern};
use nkw_gl::{GlError, RenderOpts, Stats, Target, WorldRenderer, WorldTexture, glow};

#[test]
fn s1_signatures() {
    let _new: unsafe fn(&glow::Context, RenderOpts) -> Result<WorldRenderer, GlError> =
        WorldRenderer::new;
    let _resize: unsafe fn(&mut WorldRenderer, &glow::Context, [u32; 2]) -> Result<(), GlError> =
        WorldRenderer::resize;
    let _draw: unsafe fn(
        &mut WorldRenderer,
        &glow::Context,
        &DrawList,
        Target,
    ) -> Result<Stats, GlError> = WorldRenderer::draw;
    let _tex: fn(&WorldRenderer) -> Option<WorldTexture> = WorldRenderer::world_texture;
    let _opts: fn(&WorldRenderer) -> RenderOpts = WorldRenderer::opts;
    let _set: unsafe fn(&mut WorldRenderer, &glow::Context, RenderOpts) -> Result<(), GlError> =
        WorldRenderer::set_opts;
    let _destroy: unsafe fn(WorldRenderer, &glow::Context) = WorldRenderer::destroy;
    let _state: unsafe fn(&glow::Context) -> Result<(), String> = nkw_gl::assert_default_state;
    let _empty: fn(Camera, Sky) -> DrawList = DrawList::empty;
    let _tris: fn(&DrawList) -> u32 = DrawList::triangle_estimate;
    let _view: fn(&Camera) -> Mat4 = Camera::view;
    let _proj: fn(&Camera) -> Mat4 = Camera::proj;
    let _sky: fn(u8) -> Sky = sky_for_band;
    let _index: fn(MeshId) -> usize = MeshId::index;
    let _build: fn(MeshId) -> nkw_gl::mesh::Mesh = nkw_gl::mesh::build;
    let _count: fn(MeshId) -> u32 = nkw_gl::mesh::triangle_count;

    let o = RenderOpts::default();
    let RenderOpts {
        msaa: _,
        preset64: _,
        anisotropy: _,
        gl_debug: _,
    } = o;
    let _ = [
        Target::Offscreen,
        Target::Fbo {
            fbo: 0,
            size: [1, 1],
        },
    ];
    let WorldTexture {
        id: _,
        size: _,
        y_flipped: _,
    } = WorldTexture {
        id: 0,
        size: [1, 1],
        y_flipped: true,
    };
    let Stats {
        draws: _,
        triangles: _,
        instances: _,
        gpu_ns: _,
    } = Stats::default();
    let _ = [
        GlError::Shader(String::new()),
        GlError::Link(String::new()),
        GlError::Fbo(0),
        GlError::Gl(0),
        GlError::Unsupported(""),
    ];
    let camera = Camera {
        pose: Pose::new(Vec3::ZERO, 0.0, 0.0),
        fov_y: 1.0,
        near: 0.1,
        far: 10.0,
        aspect: 1.0,
    };
    let Sky {
        color: _,
        fog_color: _,
        fog_start: _,
        fog_end: _,
        sun_dir: _,
        sun_color: _,
        ambient_sky: _,
        ambient_ground: _,
    } = sky_for_band(3);
    let mut list = DrawList {
        camera,
        sky: sky_for_band(3),
        instances: Vec::new(),
        screens: Vec::new(),
        blobs: Vec::new(),
    };
    list.instances.push(Instance {
        mesh: MeshId::Box,
        model: Mat4::IDENTITY,
        tint: [255; 4],
        emissive: 0.0,
        fade: 0.0,
    });
    let tex = ClientTex {
        id: 1,
        external: false,
        size: [1, 1],
        y_flip: false,
        opaque: true,
    };
    list.screens.push(ScreenQuad {
        model: Mat4::IDENTITY,
        tex,
        dim: 0.0,
        selected: false,
    });
    list.blobs.push(Blob {
        center: Vec3::ZERO,
        radius: 1.0,
        alpha: 1.0,
    });
    // the phase-0 kit is still there, in its slots
    let phase0 = [
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
    for (i, id) in phase0.iter().enumerate() {
        assert_eq!(draw::MeshId::ALL[i], *id);
    }
}

#[test]
fn s10_signatures() {
    let _monitor: fn(Mat4, Vec2, f32, &mut Vec<Instance>) = rig::monitor;
    type CatFn = fn(&CatJoints, &Coat, Mat4, Option<[u8; 4]>, f32, &mut Vec<Instance>);
    let _cat: CatFn = rig::cat_instances;
    let _kitten: fn(&Coat, Mat4, bool, f32, &mut Vec<Instance>) = rig::kitten_instances;
    let _door: fn(Mat4, f32, bool) -> Mat4 = rig::door_leaf;
    let _hands: fn(Mat4, u8, u8) -> [Instance; 2] = rig::clock_hands;
    let _turntable: fn(Mat4, f32, bool, f32, &mut Vec<Instance>) = rig::turntable;
    let _seed: fn(u64) -> Coat = Coat::from_seed;
    let CatJoints {
        body_y: _,
        body_rot_x: _,
        body_scale: _,
        head_y: _,
        head_z: _,
        head_rot_x: _,
        head_rot_y: _,
        head_rot_z: _,
        ears_back: _,
        eyes_open: _,
        leg_l: _,
        leg_r: _,
        tail_pitch: _,
        tail_sway: _,
        tail_curl: _,
        tail_scale: _,
    } = CatJoints::default();
    let Coat {
        body: _,
        head: _,
        ears: _,
        tail: _,
        muzzle: _,
        accent: _,
        pattern: _,
        ear: _,
    } = Coat::from_seed(1);
    let _ = [
        Pattern::Solid,
        Pattern::Tabby,
        Pattern::Tuxedo,
        Pattern::Calico,
    ];
    let _ = [Ear::Pointy, Ear::Round, Ear::Folded];
    let _ = [
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
    ];
    assert_eq!(MeshId::ALL.len(), MeshId::COUNT);
}
