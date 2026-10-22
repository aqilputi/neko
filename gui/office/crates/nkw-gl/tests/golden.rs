//! Golden images of the world pass (320×180, surfaceless EGL), the GL
//! state contract and the error-free passes. `UPDATE_GOLDEN=1` rewrites
//! the PNGs in tests/golden/. Fixture content only: boxes, cats, a checker.

mod egl;

use egl::{Ctx, compare, context};
use glam::{Mat4, Quat, Vec3};
use nkw_core::Pose;
use nkw_gl::draw::{Blob, Camera, ClientTex, DrawList, Instance, MeshId, ScreenQuad, sky_for_band};
use nkw_gl::glow::{self, HasContext};
use nkw_gl::{RenderOpts, Stats, Target, WorldRenderer, assert_default_state};

const SIZE: [u32; 2] = [320, 180];
/// A pixel (x, y from the top) of `blobs_and_fog` on the end wall, past the fog.
const HORIZON: [usize; 2] = [120, 72];

fn opts() -> RenderOpts {
    RenderOpts {
        gl_debug: true,
        ..RenderOpts::default()
    }
}

/// Bit-exact between runs: anisotropic sampling is not deterministic on
/// every GPU (seen on Intel), so exact-equality tests turn it off.
fn exact() -> RenderOpts {
    RenderOpts {
        anisotropy: 1.0,
        ..opts()
    }
}

fn camera(pos: Vec3, yaw: f32, pitch: f32) -> Camera {
    Camera {
        pose: Pose::new(pos, yaw, pitch),
        fov_y: 60f32.to_radians(),
        near: 0.05,
        far: 60.0,
        aspect: SIZE[0] as f32 / SIZE[1] as f32,
    }
}

fn at(mesh: MeshId, pos: Vec3, yaw: f32, scale: Vec3, tint: [u8; 4]) -> Instance {
    Instance {
        mesh,
        model: Mat4::from_scale_rotation_translation(scale, Quat::from_rotation_y(yaw), pos),
        tint,
        emissive: 0.0,
        fade: 0.0,
    }
}

const WHITE: [u8; 4] = [255; 4];
const FUR: [[u8; 4]; 2] = [[250, 196, 140, 255], [200, 200, 214, 255]];

/// A placeholder cat sitting at `base` (on its seat), facing -z.
fn cat(list: &mut DrawList, base: Vec3, fur: [u8; 4], fade: f32) {
    let one = Vec3::ONE;
    let mut parts = vec![
        at(
            MeshId::CatBody,
            base + Vec3::new(0.0, 0.13, 0.0),
            0.0,
            one,
            fur,
        ),
        at(
            MeshId::CatHead,
            base + Vec3::new(0.0, 0.34, -0.08),
            0.0,
            one,
            fur,
        ),
        at(
            MeshId::CatEar,
            base + Vec3::new(-0.07, 0.46, -0.08),
            0.0,
            one,
            fur,
        ),
        at(
            MeshId::CatEar,
            base + Vec3::new(0.07, 0.46, -0.08),
            0.0,
            one,
            fur,
        ),
    ];
    let mut tail = at(
        MeshId::CatTail,
        base + Vec3::new(0.08, 0.05, 0.16),
        0.0,
        one,
        fur,
    );
    tail.model *= Mat4::from_rotation_y(0.5) * Mat4::from_rotation_x(-0.5);
    parts.push(tail);
    for p in &mut parts {
        p.fade = fade;
    }
    list.instances.extend(parts);
    list.blobs.push(Blob {
        center: base,
        radius: 0.32,
        alpha: 0.35,
    });
}

/// Floor, walls, three desks with monitors and chairs, a shelf, a door, two cats.
fn room() -> DrawList {
    let mut l = DrawList::empty(
        camera(Vec3::new(0.0, 1.6, 2.4), 0.0, -0.18),
        sky_for_band(4),
    );
    let one = Vec3::ONE;
    let i = &mut l.instances;
    i.push(at(
        MeshId::Plane,
        Vec3::new(0.0, 0.0, -0.5),
        0.0,
        Vec3::new(9.0, 1.0, 7.0),
        [232, 214, 178, 255],
    ));
    i.push(at(
        MeshId::Box,
        Vec3::new(0.0, 1.5, -3.05),
        0.0,
        Vec3::new(9.0, 3.0, 0.1),
        [247, 226, 204, 255],
    ));
    i.push(at(
        MeshId::Box,
        Vec3::new(-4.5, 1.5, -0.5),
        0.0,
        Vec3::new(0.1, 3.0, 7.0),
        [214, 204, 232, 255],
    ));
    i.push(at(
        MeshId::Box,
        Vec3::new(4.5, 1.5, -0.5),
        0.0,
        Vec3::new(0.1, 3.0, 7.0),
        [214, 204, 232, 255],
    ));
    for x in [-2.2, 0.0, 2.2] {
        i.push(at(MeshId::Desk, Vec3::new(x, 0.0, -2.3), 0.0, one, WHITE));
        i.push(at(
            MeshId::MonitorFrame,
            Vec3::new(x, 1.2, -2.45),
            0.0,
            Vec3::new(0.95, 0.6, 1.0),
            WHITE,
        ));
        let mut glow = at(
            MeshId::Box,
            Vec3::new(x, 1.2, -2.452),
            0.0,
            Vec3::new(0.86, 0.52, 0.004),
            [164, 206, 236, 255],
        );
        glow.emissive = 1.0;
        i.push(glow);
        i.push(at(MeshId::Chair, Vec3::new(x, 0.0, -1.55), 0.0, one, WHITE));
    }
    i.push(at(
        MeshId::Shelf,
        Vec3::new(-3.6, 0.0, -2.8),
        0.0,
        one,
        WHITE,
    ));
    let books = [
        [240, 168, 176, 255],
        [150, 200, 140, 255],
        [164, 206, 236, 255],
        [250, 226, 150, 255],
        [200, 182, 228, 255],
    ];
    for (k, tint) in books.iter().enumerate() {
        let x = -3.95 + 0.045 * k as f32;
        i.push(at(
            MeshId::Book,
            Vec3::new(x, 0.575, -2.78),
            0.0,
            one,
            *tint,
        ));
        i.push(at(
            MeshId::Book,
            Vec3::new(x + 0.4, 1.095, -2.78),
            0.0,
            one,
            books[(k + 2) % 5],
        ));
    }
    i.push(at(
        MeshId::DoorFrame,
        Vec3::new(3.5, 0.0, -2.95),
        0.0,
        one,
        WHITE,
    ));
    i.push(at(
        MeshId::DoorLeaf,
        Vec3::new(3.01, 0.0, -2.95),
        -0.6,
        one,
        WHITE,
    ));
    for x in [-2.2, 0.0, 2.2] {
        l.blobs.push(Blob {
            center: Vec3::new(x, 0.0, -1.55),
            radius: 0.36,
            alpha: 0.3,
        });
    }
    cat(&mut l, Vec3::new(-2.75, 0.75, -2.15), FUR[0], 0.0);
    cat(&mut l, Vec3::new(1.65, 0.75, -2.15), FUR[1], 0.0);
    l
}

/// A long floor with pillars every 4 m, disappearing into the fog.
fn fog_hall() -> DrawList {
    let mut l = DrawList::empty(
        camera(Vec3::new(0.0, 1.2, 2.0), 0.25, -0.05),
        sky_for_band(6),
    );
    l.instances.push(at(
        MeshId::Plane,
        Vec3::new(0.0, 0.0, -30.0),
        0.0,
        Vec3::new(10.0, 1.0, 70.0),
        [232, 214, 178, 255],
    ));
    // the end wall, beyond the fog: it must come out as pure fog colour
    l.instances.push(at(
        MeshId::Box,
        Vec3::new(0.0, 3.0, -50.0),
        0.0,
        Vec3::new(10.0, 6.0, 0.1),
        [150, 200, 140, 255],
    ));
    for k in 0..15 {
        let z = -4.0 * k as f32;
        for x in [-2.5, 2.5] {
            l.instances.push(at(
                MeshId::RoundedBox,
                Vec3::new(x, 0.9, z),
                0.3,
                Vec3::new(0.6, 1.8, 0.6),
                [248, 194, 160, 255],
            ));
            l.blobs.push(Blob {
                center: Vec3::new(x, 0.0, z),
                radius: 0.6,
                alpha: 0.4,
            });
        }
        l.instances.push(at(
            MeshId::Cylinder,
            Vec3::new(0.0, 0.2, z - 2.0),
            0.0,
            Vec3::new(0.3, 0.4, 0.3),
            [176, 226, 204, 255],
        ));
        l.blobs.push(Blob {
            center: Vec3::new(0.0, 0.0, z - 2.0),
            radius: 0.3,
            alpha: 0.5,
        });
    }
    l
}

/// A 64×40 checker in two cozy colours, 4 px cells, with one marked corner
/// (top-left red) so orientation mistakes show.
fn checker() -> (Vec<u8>, [u32; 2]) {
    let (w, h) = (64u32, 40u32);
    let mut px = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            let c = if x < 8 && y < 8 {
                [220, 60, 60, 255]
            } else if (x / 4 + y / 4) % 2 == 0 {
                [250, 244, 230, 255]
            } else {
                [70, 60, 90, 255]
            };
            px.extend(c);
        }
    }
    (px, [w, h])
}

fn screens(tex: u32, external: bool, size: [u32; 2]) -> DrawList {
    let mut l = DrawList::empty(camera(Vec3::new(0.0, 1.5, 1.2), 0.0, 0.0), sky_for_band(3));
    l.instances.push(at(
        MeshId::Box,
        Vec3::new(0.0, 1.5, -2.1),
        0.0,
        Vec3::new(10.0, 6.0, 0.1),
        [247, 226, 204, 255],
    ));
    let t = ClientTex {
        id: tex,
        external,
        size,
        y_flip: false,
        opaque: true,
    };
    let quad = |pos: Vec3, yaw: f32| {
        Mat4::from_scale_rotation_translation(
            Vec3::new(1.28, 0.8, 1.0),
            Quat::from_rotation_y(yaw),
            pos,
        )
    };
    l.screens.push(ScreenQuad {
        model: quad(Vec3::new(-0.75, 1.5, -2.0), 0.25),
        tex: t,
        dim: 0.0,
        selected: false,
    });
    l.screens.push(ScreenQuad {
        model: quad(Vec3::new(0.85, 1.5, -2.0), -0.35),
        tex: t,
        dim: 0.2,
        selected: true,
    });
    for x in [-0.75, 0.85] {
        let mut frame = at(
            MeshId::MonitorFrame,
            Vec3::new(x, 1.5, -2.0),
            if x < 0.0 { 0.25 } else { -0.35 },
            Vec3::ONE,
            WHITE,
        );
        frame.model *= Mat4::from_scale(Vec3::new(1.36, 0.88, 1.0));
        l.instances.push(frame);
    }
    l
}

fn fades() -> DrawList {
    let mut l = DrawList::empty(camera(Vec3::new(0.0, 0.9, 1.4), 0.0, -0.2), sky_for_band(2));
    l.instances.push(at(
        MeshId::Plane,
        Vec3::new(0.0, 0.0, -1.0),
        0.0,
        Vec3::new(6.0, 1.0, 6.0),
        [232, 214, 178, 255],
    ));
    l.instances.push(at(
        MeshId::Box,
        Vec3::new(0.0, 1.0, -1.2),
        0.0,
        Vec3::new(4.0, 2.0, 0.1),
        [150, 200, 140, 255],
    ));
    cat(&mut l, Vec3::new(-0.45, 0.0, -0.3), FUR[0], 0.5);
    cat(&mut l, Vec3::new(0.45, 0.0, -0.3), FUR[1], 0.0);
    let mut ghost = at(
        MeshId::RoundedBox,
        Vec3::new(0.0, 0.35, 0.1),
        0.6,
        Vec3::splat(0.3),
        [200, 182, 228, 128],
    );
    ghost.fade = 0.25;
    l.instances.push(ghost);
    l
}

struct Run {
    px: Vec<u8>,
    stats: Stats,
}

fn render(ctx: &Ctx, list: &DrawList, opts: RenderOpts) -> Run {
    let gl = &ctx.gl;
    let target = ctx.fbo(SIZE);
    unsafe {
        let mut w = WorldRenderer::new(gl, opts).expect("renderer");
        let stats = w
            .draw(
                gl,
                list,
                Target::Fbo {
                    fbo: target.id(),
                    size: SIZE,
                },
            )
            .expect("draw");
        assert_default_state(gl).expect("default state");
        let px = ctx.read(Some(target.fbo), SIZE);
        w.destroy(gl);
        Run { px, stats }
    }
}

#[test]
fn empty_sky() {
    let Some(ctx) = context() else { return };
    let list = DrawList::empty(camera(Vec3::Y, 0.0, 0.0), sky_for_band(4));
    let r = render(&ctx, &list, opts());
    let c = sky_for_band(4).color.map(|v| (v * 255.0).round() as u8);
    assert!(
        r.px.chunks(4)
            .all(|p| p[..3].iter().zip(c).all(|(a, b)| a.abs_diff(b) <= 1) && p[3] == 255)
    );
    compare("empty_sky", &r.px, SIZE);
}

#[test]
fn room_three_desks() {
    let Some(ctx) = context() else { return };
    let list = room();
    let r = render(&ctx, &list, opts());
    assert_eq!(
        r.stats.instances as usize,
        list.instances.len() + list.blobs.len()
    );
    assert_eq!(r.stats.triangles, list.triangle_estimate());
    assert!(r.stats.draws <= 80, "{} draws", r.stats.draws);
    assert!(r.stats.triangles <= 100_000);
    compare("room_three_desks", &r.px, SIZE);
}

#[test]
fn offscreen_target_matches_the_fbo_path() {
    let Some(ctx) = context() else { return };
    let gl = &ctx.gl;
    unsafe {
        let mut w = WorldRenderer::new(gl, opts()).unwrap();
        assert_eq!(w.world_texture(), None);
        w.resize(gl, SIZE).unwrap();
        w.resize(gl, SIZE).unwrap();
        w.draw(gl, &room(), Target::Offscreen).unwrap();
        assert_default_state(gl).unwrap();
        let t = w.world_texture().unwrap();
        assert_eq!((t.size, t.y_flipped), (SIZE, true));
        compare("room_three_desks", &ctx.read_texture(t.id, t.size), SIZE);
        // a resize gives a new texture of the new size
        w.resize(gl, [64, 36]).unwrap();
        assert_eq!(w.world_texture().unwrap().size, [64, 36]);
        w.destroy(gl);
    }
}

#[test]
fn screen_quad_checker() {
    let Some(ctx) = context() else { return };
    let (px, size) = checker();
    let tex = ctx.texture(&px, size);
    let r = render(&ctx, &screens(tex, false, size), exact());
    compare("screen_quad_checker", &r.px, SIZE);
}

#[test]
fn screen_quad_external_matches_2d() {
    let Some(ctx) = context() else { return };
    let (px, size) = checker();
    let Some(tex) = ctx.external_texture(&px, size) else {
        eprintln!("SKIP: no GL_OES_EGL_image_external_essl3 / EGL_KHR_gl_texture_2D_image");
        return;
    };
    let r = render(&ctx, &screens(tex, true, size), exact());
    compare("screen_quad_checker", &r.px, SIZE);
}

#[test]
fn y_flip_mirrors_the_screen_vertically() {
    let Some(ctx) = context() else { return };
    let (px, size) = checker();
    // the same picture stored bottom-up, as an FBO texture would be
    let flipped = egl::flip_rows(&px, size[0] as usize);
    let tex = ctx.texture(&flipped, size);
    let mut list = screens(tex, false, size);
    for s in &mut list.screens {
        s.tex.y_flip = true;
    }
    let r = render(&ctx, &list, exact());
    compare("screen_quad_checker", &r.px, SIZE);
}

#[test]
fn blobs_and_fog() {
    let Some(ctx) = context() else { return };
    let r = render(&ctx, &fog_hall(), opts());
    compare("blobs_and_fog", &r.px, SIZE);
    // past fog_end everything is fog
    let fog = sky_for_band(6).fog_color.map(|v| (v * 255.0).round() as u8);
    let horizon = &r.px[(HORIZON[1] * SIZE[0] as usize + HORIZON[0]) * 4..][..3];
    assert!(
        horizon.iter().zip(fog).all(|(a, b)| a.abs_diff(b) <= 12),
        "{horizon:?} vs {fog:?}"
    );
}

#[test]
fn preset64() {
    let Some(ctx) = context() else { return };
    let r = render(
        &ctx,
        &room(),
        RenderOpts {
            preset64: true,
            ..opts()
        },
    );
    // nearest upscale: pixels come in 2×2 blocks at 320×180
    let px = |x: usize, y: usize| &r.px[(y * SIZE[0] as usize + x) * 4..][..4];
    let blocks = (0..90)
        .flat_map(|y| (0..160).map(move |x| (x, y)))
        .filter(|&(x, y)| {
            let a = px(2 * x, 2 * y);
            a == px(2 * x + 1, 2 * y) && a == px(2 * x, 2 * y + 1) && a == px(2 * x + 1, 2 * y + 1)
        });
    assert_eq!(blocks.count(), 160 * 90);
    compare("preset64", &r.px, SIZE);
}

#[test]
fn fade_dither() {
    let Some(ctx) = context() else { return };
    compare("fade_dither", &render(&ctx, &fades(), opts()).px, SIZE);
}

#[test]
fn msaa_off_and_option_changes_still_draw() {
    let Some(ctx) = context() else { return };
    let gl = &ctx.gl;
    let target = ctx.fbo(SIZE);
    let t = Target::Fbo {
        fbo: target.id(),
        size: SIZE,
    };
    unsafe {
        let mut w = WorldRenderer::new(gl, RenderOpts { msaa: 0, ..opts() }).unwrap();
        w.draw(gl, &room(), t).unwrap();
        for o in [
            RenderOpts {
                preset64: true,
                ..opts()
            },
            RenderOpts {
                anisotropy: 1.0,
                ..opts()
            },
            opts(),
        ] {
            w.set_opts(gl, o).unwrap();
            assert_eq!(w.opts(), o);
            w.draw(gl, &room(), t).unwrap();
        }
        assert_default_state(gl).unwrap();
        compare("room_three_desks", &ctx.read(Some(target.fbo), SIZE), SIZE);
        w.destroy(gl);
    }
}

#[test]
fn state_restored() {
    let Some(ctx) = context() else { return };
    let gl = &ctx.gl;
    let (px, size) = checker();
    let tex = ctx.texture(&px, size);
    let target = ctx.fbo(SIZE);
    unsafe {
        let mut w = WorldRenderer::new(gl, exact()).unwrap();
        w.resize(gl, SIZE).unwrap();
        let mut list = room();
        list.screens = screens(tex, false, size).screens;
        for target in [
            Target::Offscreen,
            Target::Fbo {
                fbo: target.id(),
                size: SIZE,
            },
        ] {
            // deliberately dirty state
            let vao = gl.create_vertex_array().unwrap();
            gl.bind_vertex_array(Some(vao));
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(ctx.fbo([8, 8]).fbo));
            gl.active_texture(glow::TEXTURE3);
            gl.bind_texture(
                glow::TEXTURE_2D,
                std::num::NonZeroU32::new(tex).map(glow::NativeTexture),
            );
            gl.disable(glow::BLEND);
            gl.blend_func(glow::ZERO, glow::ONE);
            for cap in [
                glow::DEPTH_TEST,
                glow::CULL_FACE,
                glow::SCISSOR_TEST,
                glow::STENCIL_TEST,
            ] {
                gl.enable(cap);
            }
            gl.scissor(0, 0, 1, 1);
            gl.color_mask(false, true, false, true);
            gl.depth_mask(false);
            gl.cull_face(glow::FRONT);
            gl.viewport(1, 2, 33, 44);
            assert!(assert_default_state(gl).is_err());
            w.draw(gl, &list, target).unwrap();
            assert_default_state(gl).unwrap();
            let mut vp = [0; 4];
            gl.get_parameter_i32_slice(glow::VIEWPORT, &mut vp);
            assert_eq!(vp, [1, 2, 33, 44], "viewport restored");
            gl.delete_vertex_array(vao);
        }
        // and the dirty state did not leak into the picture
        compare_room_with_screens(&ctx, &target);
        w.destroy(gl);
    }
}

/// The room with screens drawn from a dirty state equals the same frame drawn from a clean one.
fn compare_room_with_screens(ctx: &Ctx, dirty: &egl::Fbo) {
    let (px, size) = checker();
    let tex = ctx.texture(&px, size);
    let mut list = room();
    list.screens = screens(tex, false, size).screens;
    let clean = render(ctx, &list, exact()).px;
    let got = ctx.read(Some(dirty.fbo), SIZE);
    let differ = clean
        .chunks(4)
        .zip(got.chunks(4))
        .filter(|(a, b)| a != b)
        .count();
    if differ > 0 {
        let dir = std::env::temp_dir();
        egl::write_png(&dir.join("nkw-gl-state-clean.png"), &clean, SIZE);
        egl::write_png(&dir.join("nkw-gl-state-dirty.png"), &got, SIZE);
        let at: Vec<_> = clean
            .chunks(4)
            .zip(got.chunks(4))
            .enumerate()
            .filter(|(_, (a, b))| a != b)
            .take(8)
            .map(|(i, (a, b))| {
                (
                    i % SIZE[0] as usize,
                    i / SIZE[0] as usize,
                    a.to_vec(),
                    b.to_vec(),
                )
            })
            .collect();
        panic!("dirty entry state changed {differ} px (images in {dir:?}): {at:?}");
    }
}

#[test]
fn no_gl_error() {
    let Some(ctx) = context() else { return };
    let gl = &ctx.gl;
    let (px, size) = checker();
    let tex = ctx.texture(&px, size);
    let ext = ctx.external_texture(&px, size);
    unsafe {
        while gl.get_error() != glow::NO_ERROR {}
        for preset64 in [false, true] {
            for msaa in [0, 4] {
                let o = RenderOpts {
                    msaa,
                    preset64,
                    gl_debug: true,
                    ..RenderOpts::default()
                };
                let mut w = WorldRenderer::new(gl, o).unwrap();
                w.resize(gl, SIZE).unwrap();
                let mut list = room();
                list.screens = screens(tex, false, size).screens;
                if let Some(e) = ext {
                    list.screens.extend(screens(e, true, size).screens);
                }
                list.blobs.extend(fog_hall().blobs);
                list.instances.extend(fades().instances);
                w.draw(gl, &list, Target::Offscreen).unwrap();
                let fbo = ctx.fbo(SIZE);
                w.draw(
                    gl,
                    &list,
                    Target::Fbo {
                        fbo: fbo.id(),
                        size: SIZE,
                    },
                )
                .unwrap();
                w.draw(
                    gl,
                    &DrawList::empty(list.camera, list.sky),
                    Target::Offscreen,
                )
                .unwrap();
                w.destroy(gl);
                assert_eq!(
                    gl.get_error(),
                    glow::NO_ERROR,
                    "msaa {msaa} preset64 {preset64}"
                );
            }
        }
    }
}

#[test]
fn perf_smoke() {
    let Some(ctx) = context() else { return };
    let gl = &ctx.gl;
    let (px, size) = checker();
    let tex = ctx.texture(&px, size);
    let mut list = DrawList::empty(camera(Vec3::new(0.0, 1.6, 6.0), 0.0, -0.1), sky_for_band(4));
    let kinds = [
        MeshId::Desk,
        MeshId::Chair,
        MeshId::CatBody,
        MeshId::CatHead,
        MeshId::Book,
        MeshId::RoundedBox,
    ];
    for (k, mesh) in kinds.iter().enumerate() {
        for n in 0..80 {
            let pos = Vec3::new(-8.0 + 0.2 * n as f32, 0.0, -2.0 * k as f32);
            list.instances
                .push(at(*mesh, pos, n as f32 * 0.1, Vec3::ONE, WHITE));
        }
    }
    for n in 0..6 {
        let model = Mat4::from_scale_rotation_translation(
            Vec3::new(1.6, 1.0, 1.0),
            Quat::from_rotation_y(if n % 2 == 0 { 0.2 } else { -0.2 }),
            Vec3::new(-5.0 + 2.0 * n as f32, 1.5, -1.0),
        );
        let t = ClientTex {
            id: tex,
            external: false,
            size,
            y_flip: false,
            opaque: true,
        };
        list.screens.push(ScreenQuad {
            model,
            tex: t,
            dim: 0.0,
            selected: n == 2,
        });
    }
    unsafe {
        let mut w = WorldRenderer::new(
            gl,
            RenderOpts {
                gl_debug: false,
                ..RenderOpts::default()
            },
        )
        .unwrap();
        w.resize(gl, [1920, 1080]).unwrap();
        let mut stats = w.draw(gl, &list, Target::Offscreen).unwrap();
        gl.finish();
        let t0 = std::time::Instant::now();
        for _ in 0..100 {
            stats = w.draw(gl, &list, Target::Offscreen).unwrap();
        }
        let submit = t0.elapsed() / 100;
        gl.finish();
        let total = t0.elapsed() / 100;
        eprintln!(
            "perf_smoke 1080p: {} instances, {} screens → {} draws, {} tris; CPU submit {:?}/frame, with GPU {:?}/frame, gpu_ns {:?}",
            list.instances.len(),
            list.screens.len(),
            stats.draws,
            stats.triangles,
            submit,
            total,
            stats.gpu_ns
        );
        assert!(stats.draws <= 80, "{} draws", stats.draws);
        w.destroy(gl);
    }
}
