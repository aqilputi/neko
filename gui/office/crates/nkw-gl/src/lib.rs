//! nkw-gl: the world pass of `nkw` (docs/plano-mundo.md §5).
//!
//! GLES 3.0 only, on whatever context the caller makes current: Smithay's
//! GlowRenderer in `nkw`, a surfaceless EGL context in the golden tests.
//! [`draw`] and [`mesh`] are pure; everything with GL lives behind
//! [`WorldRenderer`].
//!
//! One frame: clear to the sky → kit meshes (instanced, one draw per mesh)
//! → opaque screens → blob shadows → blended screens (far to near) → MSAA
//! resolve blit, or the preset-64 nearest upscale, into the target.

pub use glow;

pub mod draw;
pub mod mesh;
mod pack;

use glow::HasContext;

use draw::{DrawList, MeshId, ScreenQuad};
use pack::{BlobData, InstanceData, Run};

const TEXTURE_EXTERNAL_OES: u32 = 0x8D65;
const TEXTURE_BINDING_EXTERNAL_OES: u32 = 0x8D67;
const TEXTURE_MAX_ANISOTROPY: u32 = 0x84FE;
const MAX_TEXTURE_MAX_ANISOTROPY: u32 = 0x84FF;
const TIME_ELAPSED: u32 = 0x88BF;
const GPU_DISJOINT: u32 = 0x8FBB;

const EXT_EXTERNAL: &str = "GL_OES_EGL_image_external_essl3";
const EXT_ANISOTROPY: &str = "GL_EXT_texture_filter_anisotropic";
const EXT_TIMER: &str = "GL_EXT_disjoint_timer_query";

/// Selection outline width around a screen, metres.
const OUTLINE_M: f32 = 0.035;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RenderOpts {
    /// 0 or 4; 4 falls back to 0 if GL_MAX_SAMPLES < 4
    pub msaa: u8,
    /// low-res internal target + nearest upscale + Bayer dither
    pub preset64: bool,
    /// 1.0 = off; clamped to GL_MAX_TEXTURE_MAX_ANISOTROPY_EXT if present
    pub anisotropy: f32,
    /// glGetError after every pass (tests, --gl-debug)
    pub gl_debug: bool,
}

impl Default for RenderOpts {
    fn default() -> RenderOpts {
        RenderOpts {
            msaa: 4,
            preset64: false,
            anisotropy: 4.0,
            gl_debug: cfg!(debug_assertions),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    /// the renderer's own offscreen target (MSAA, resolved); read it back with `world_texture()`
    Offscreen,
    /// a caller-owned FBO (golden tests, `--shot` in headless); `size` in physical px
    Fbo { fbo: u32, size: [u32; 2] },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorldTexture {
    /// GLuint, GL_TEXTURE_2D, RGBA8
    pub id: u32,
    pub size: [u32; 2],
    /// always true: GL origin is bottom-left
    pub y_flipped: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stats {
    /// draw calls (kit meshes, screens, blobs, the preset-64 upscale)
    pub draws: u32,
    /// kit triangles + 2 per screen and blob drawn
    pub triangles: u32,
    /// kit instances + blobs + screens drawn
    pub instances: u32,
    /// GPU time of an earlier frame (EXT_disjoint_timer_query, when present)
    pub gpu_ns: Option<u64>,
}

#[derive(Debug)]
pub enum GlError {
    Shader(String),
    Link(String),
    Fbo(u32),
    Gl(u32),
    Unsupported(&'static str),
}

impl std::fmt::Display for GlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GlError::Shader(s) => write!(f, "shader compile failed: {s}"),
            GlError::Link(s) => write!(f, "program link failed: {s}"),
            GlError::Fbo(s) => write!(f, "framebuffer incomplete: 0x{s:x}"),
            GlError::Gl(e) => write!(f, "GL error 0x{e:x}"),
            GlError::Unsupported(s) => write!(f, "unsupported: {s}"),
        }
    }
}

impl std::error::Error for GlError {}

/// What this context can do, probed once.
#[derive(Clone, Copy, Debug)]
struct Caps {
    max_samples: i32,
    max_anisotropy: Option<f32>,
    timer: bool,
}

/// A linked program and its uniform locations, looked up once (glow
/// allocates a CString per lookup, so never per frame).
struct Program {
    id: glow::Program,
    uniforms: Vec<(&'static str, Option<glow::UniformLocation>)>,
}

impl Program {
    fn u(&self, name: &str) -> Option<&glow::UniformLocation> {
        self.uniforms
            .iter()
            .find(|(n, _)| *n == name)
            .and_then(|(_, l)| l.as_ref())
    }
}

struct GpuMesh {
    vao: glow::VertexArray,
    vbo: glow::Buffer,
    ibo: glow::Buffer,
    indices: i32,
    triangles: u32,
}

enum SceneColor {
    Multisample(glow::Renderbuffer),
    Texture(glow::Texture),
}

/// Where the 3D scene is drawn before it reaches the target.
struct Scene {
    size: [u32; 2],
    samples: i32,
    fbo: glow::Framebuffer,
    color: SceneColor,
    depth: glow::Renderbuffer,
}

struct Offscreen {
    size: [u32; 2],
    fbo: glow::Framebuffer,
    tex: glow::Texture,
}

/// Two timer queries used alternately: read the older one when it is ready,
/// never wait for it.
struct Timer {
    queries: [glow::Query; 2],
    pending: [bool; 2],
    next: usize,
}

/// The renderer: programs, VAOs, instance buffers and the offscreen target.
pub struct WorldRenderer {
    opts: RenderOpts,
    caps: Caps,
    kit: Program,
    screen: Program,
    screen_ext: Option<Program>,
    blob: Program,
    upscale: Program,
    meshes: Vec<GpuMesh>,
    instance_buf: glow::Buffer,
    instance_cap: usize,
    blob_buf: glow::Buffer,
    blob_cap: usize,
    blob_vao: glow::VertexArray,
    empty_vao: glow::VertexArray,
    sampler: glow::Sampler,
    scene: Option<Scene>,
    offscreen: Option<Offscreen>,
    timer: Option<Timer>,
    gpu_ns: Option<u64>,
    // per-frame scratch, kept to avoid allocating
    instances: Vec<InstanceData>,
    blobs: Vec<BlobData>,
    runs: [Run; 15],
    order: Vec<u32>,
}

const KIT_UNIFORMS: &[&str] = &[
    "u_view_proj",
    "u_eye",
    "u_sun_dir",
    "u_sun_color",
    "u_amb_sky",
    "u_amb_ground",
    "u_fog_color",
    "u_fog",
];
const SCREEN_UNIFORMS: &[&str] = &[
    "u_model",
    "u_view_proj",
    "u_margin",
    "u_tex",
    "u_size_m",
    "u_yflip",
    "u_opaque",
    "u_dim",
    "u_outline",
    "u_eye",
    "u_fog_color",
    "u_fog",
];
const BLOB_UNIFORMS: &[&str] = &["u_view_proj", "u_eye", "u_fog"];
const UPSCALE_UNIFORMS: &[&str] = &["u_src", "u_div"];

fn gl_err(_: String) -> GlError {
    GlError::Unsupported("GL object creation failed")
}

impl WorldRenderer {
    /// Compiles shaders, uploads the mesh kit. Allocates nothing per frame afterwards.
    ///
    /// # Safety
    /// `gl` must be current on this thread.
    pub unsafe fn new(gl: &glow::Context, opts: RenderOpts) -> Result<WorldRenderer, GlError> {
        unsafe {
            if gl.version().major < 3 {
                return Err(GlError::Unsupported("GLES 3.0"));
            }
            let has = |e: &str| gl.supported_extensions().contains(e);
            let caps = Caps {
                max_samples: gl.get_parameter_i32(glow::MAX_SAMPLES),
                max_anisotropy: has(EXT_ANISOTROPY)
                    .then(|| gl.get_parameter_f32(MAX_TEXTURE_MAX_ANISOTROPY)),
                timer: has(EXT_TIMER),
            };
            let screen_frag = |head: &str| format!("{head}{}", include_str!("shaders/screen.frag"));
            let kit = program(
                gl,
                "kit",
                include_str!("shaders/kit.vert"),
                include_str!("shaders/kit.frag"),
                KIT_UNIFORMS,
            )?;
            let screen_vert = include_str!("shaders/screen.vert");
            let screen = program(
                gl,
                "screen",
                screen_vert,
                &screen_frag(include_str!("shaders/screen_2d.frag")),
                SCREEN_UNIFORMS,
            )?;
            // external textures are optional: without the extension those screens are skipped
            let screen_ext = if has(EXT_EXTERNAL) {
                program(
                    gl,
                    "screen_ext",
                    screen_vert,
                    &screen_frag(include_str!("shaders/screen_ext.frag")),
                    SCREEN_UNIFORMS,
                )
                .ok()
            } else {
                None
            };
            let blob = program(
                gl,
                "blob",
                include_str!("shaders/blob.vert"),
                include_str!("shaders/blob.frag"),
                BLOB_UNIFORMS,
            )?;
            let upscale = program(
                gl,
                "upscale",
                include_str!("shaders/upscale.vert"),
                include_str!("shaders/upscale.frag"),
                UPSCALE_UNIFORMS,
            )?;

            let instance_buf = gl.create_buffer().map_err(gl_err)?;
            let meshes = MeshId::ALL
                .iter()
                .map(|&id| upload_mesh(gl, id, instance_buf))
                .collect::<Result<Vec<_>, _>>()?;

            let blob_buf = gl.create_buffer().map_err(gl_err)?;
            let blob_vao = gl.create_vertex_array().map_err(gl_err)?;
            gl.bind_vertex_array(Some(blob_vao));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(blob_buf));
            let stride = std::mem::size_of::<BlobData>() as i32;
            gl.enable_vertex_attrib_array(0);
            gl.vertex_attrib_pointer_f32(0, 4, glow::FLOAT, false, stride, 0);
            gl.vertex_attrib_divisor(0, 1);
            gl.enable_vertex_attrib_array(1);
            gl.vertex_attrib_pointer_f32(1, 1, glow::FLOAT, false, stride, 16);
            gl.vertex_attrib_divisor(1, 1);
            gl.bind_vertex_array(None);
            gl.bind_buffer(glow::ARRAY_BUFFER, None);

            let empty_vao = gl.create_vertex_array().map_err(gl_err)?;
            let sampler = gl.create_sampler().map_err(gl_err)?;
            for (p, v) in [
                (glow::TEXTURE_MIN_FILTER, glow::LINEAR),
                (glow::TEXTURE_MAG_FILTER, glow::LINEAR),
                (glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE),
                (glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE),
            ] {
                gl.sampler_parameter_i32(sampler, p, v as i32);
            }
            let timer = if caps.timer {
                match (gl.create_query(), gl.create_query()) {
                    (Ok(a), Ok(b)) => Some(Timer {
                        queries: [a, b],
                        pending: [false; 2],
                        next: 0,
                    }),
                    _ => None,
                }
            } else {
                None
            };

            let mut r = WorldRenderer {
                opts,
                caps,
                kit,
                screen,
                screen_ext,
                blob,
                upscale,
                meshes,
                instance_buf,
                instance_cap: 0,
                blob_buf,
                blob_cap: 0,
                blob_vao,
                empty_vao,
                sampler,
                scene: None,
                offscreen: None,
                timer,
                gpu_ns: None,
                instances: Vec::new(),
                blobs: Vec::new(),
                runs: [(0, 0); 15],
                order: Vec::new(),
            };
            r.apply_anisotropy(gl);
            check(gl, true)?;
            Ok(r)
        }
    }

    /// (Re)creates the offscreen target at `size` physical px. No-op when unchanged.
    ///
    /// # Safety
    /// `gl` must be current on this thread.
    pub unsafe fn resize(&mut self, gl: &glow::Context, size: [u32; 2]) -> Result<(), GlError> {
        if self.offscreen.as_ref().is_some_and(|o| o.size == size) {
            return Ok(());
        }
        unsafe {
            if let Some(o) = self.offscreen.take() {
                gl.delete_framebuffer(o.fbo);
                gl.delete_texture(o.tex);
            }
            let tex = color_texture(gl, size)?;
            let fbo = gl.create_framebuffer().map_err(gl_err)?;
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
            gl.framebuffer_texture_2d(
                glow::FRAMEBUFFER,
                glow::COLOR_ATTACHMENT0,
                glow::TEXTURE_2D,
                Some(tex),
                0,
            );
            let status = gl.check_framebuffer_status(glow::FRAMEBUFFER);
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            self.offscreen = Some(Offscreen { size, fbo, tex });
            if status != glow::FRAMEBUFFER_COMPLETE {
                return Err(GlError::Fbo(status));
            }
        }
        Ok(())
    }

    /// Draws one frame into `target`. On return GL is in the documented default state.
    ///
    /// # Safety
    /// `gl` must be current on this thread; a `Target::Fbo` must name a complete FBO.
    pub unsafe fn draw(
        &mut self,
        gl: &glow::Context,
        list: &DrawList,
        target: Target,
    ) -> Result<Stats, GlError> {
        unsafe {
            let mut vp = [0i32; 4];
            gl.get_parameter_i32_slice(glow::VIEWPORT, &mut vp);
            if self.opts.gl_debug {
                // do not blame this pass for the caller's errors
                for _ in 0..16 {
                    if gl.get_error() == glow::NO_ERROR {
                        break;
                    }
                }
            }
            let r = self.draw_frame(gl, list, target);
            restore_default_state(gl, vp, self.screen_ext.is_some());
            r
        }
    }

    unsafe fn draw_frame(
        &mut self,
        gl: &glow::Context,
        list: &DrawList,
        target: Target,
    ) -> Result<Stats, GlError> {
        let debug = self.opts.gl_debug;
        let (dst, dst_size) = match target {
            Target::Offscreen => {
                let o = self
                    .offscreen
                    .as_ref()
                    .ok_or(GlError::Unsupported("Target::Offscreen before resize"))?;
                (Some(o.fbo), o.size)
            }
            Target::Fbo { fbo, size } => (native_fbo(fbo), size),
        };
        let mut stats = Stats::default();
        if dst_size[0] == 0 || dst_size[1] == 0 {
            return Ok(stats);
        }
        let samples = pack::samples(&self.opts, self.caps.max_samples);
        let (size, div) = if self.opts.preset64 {
            pack::low_res(dst_size)
        } else {
            (dst_size, 1)
        };
        unsafe {
            self.ensure_scene(gl, size, samples)?;
            let scene_fbo = self.scene.as_ref().expect("scene target").fbo;
            let timing = self.begin_timer(gl);

            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(scene_fbo));
            gl.viewport(0, 0, size[0] as i32, size[1] as i32);
            for cap in [
                glow::SCISSOR_TEST,
                glow::STENCIL_TEST,
                glow::POLYGON_OFFSET_FILL,
                glow::SAMPLE_ALPHA_TO_COVERAGE,
                glow::SAMPLE_COVERAGE,
                glow::RASTERIZER_DISCARD,
                glow::DITHER,
                glow::BLEND,
            ] {
                gl.disable(cap);
            }
            gl.color_mask(true, true, true, true);
            gl.depth_mask(true);
            gl.depth_func(glow::LESS);
            gl.front_face(glow::CCW);
            gl.cull_face(glow::BACK);
            gl.blend_equation(glow::FUNC_ADD);
            gl.blend_func(glow::ONE, glow::ONE_MINUS_SRC_ALPHA);
            let sky = &list.sky;
            gl.clear_color(sky.color[0], sky.color[1], sky.color[2], 1.0);
            gl.clear_depth_f32(1.0);
            gl.clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT);
            check(gl, debug)?;

            let view_proj = (list.camera.proj() * list.camera.view()).to_cols_array();
            let eye = list.camera.pose.pos;

            // kit meshes, one instanced draw per mesh
            pack::pack_instances(&list.instances, &mut self.instances, &mut self.runs);
            if !self.instances.is_empty() {
                self.instance_cap = stream(
                    gl,
                    self.instance_buf,
                    bytemuck::cast_slice(&self.instances),
                    self.instance_cap,
                );
                gl.enable(glow::DEPTH_TEST);
                gl.enable(glow::CULL_FACE);
                let p = &self.kit;
                gl.use_program(Some(p.id));
                gl.uniform_matrix_4_f32_slice(p.u("u_view_proj"), false, &view_proj);
                gl.uniform_3_f32_slice(p.u("u_eye"), &eye.to_array());
                gl.uniform_3_f32_slice(
                    p.u("u_sun_dir"),
                    &sky.sun_dir.normalize_or(glam::Vec3::Y).to_array(),
                );
                gl.uniform_3_f32_slice(p.u("u_sun_color"), &sky.sun_color);
                gl.uniform_3_f32_slice(p.u("u_amb_sky"), &sky.ambient_sky);
                gl.uniform_3_f32_slice(p.u("u_amb_ground"), &sky.ambient_ground);
                gl.uniform_3_f32_slice(p.u("u_fog_color"), &sky.fog_color);
                gl.uniform_2_f32(p.u("u_fog"), sky.fog_start, sky.fog_end);
                gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.instance_buf));
                for (m, &(first, count)) in self.meshes.iter().zip(&self.runs) {
                    if count == 0 {
                        continue;
                    }
                    gl.bind_vertex_array(Some(m.vao));
                    point_instances(gl, first);
                    gl.draw_elements_instanced(
                        glow::TRIANGLES,
                        m.indices,
                        glow::UNSIGNED_SHORT,
                        0,
                        count as i32,
                    );
                    stats.draws += 1;
                    stats.triangles += m.triangles * count;
                    stats.instances += count;
                }
                gl.disable(glow::CULL_FACE);
                check(gl, debug)?;
            }

            // screens: opaque ones now, the blended ones after the blobs
            gl.enable(glow::DEPTH_TEST);
            for s in list.screens.iter().filter(|s| pack::is_opaque(s)) {
                if self.draw_screen(gl, s, &view_proj, list) {
                    stats.draws += 1;
                    stats.triangles += 2;
                    stats.instances += 1;
                }
            }
            check(gl, debug)?;

            gl.enable(glow::BLEND);
            gl.depth_mask(false);
            pack::pack_blobs(&list.blobs, &mut self.blobs);
            if !self.blobs.is_empty() {
                self.blob_cap = stream(
                    gl,
                    self.blob_buf,
                    bytemuck::cast_slice(&self.blobs),
                    self.blob_cap,
                );
                gl.enable(glow::POLYGON_OFFSET_FILL);
                gl.polygon_offset(-1.0, -1.0);
                let p = &self.blob;
                gl.use_program(Some(p.id));
                gl.uniform_matrix_4_f32_slice(p.u("u_view_proj"), false, &view_proj);
                gl.uniform_3_f32_slice(p.u("u_eye"), &eye.to_array());
                gl.uniform_2_f32(p.u("u_fog"), sky.fog_start, sky.fog_end);
                gl.bind_vertex_array(Some(self.blob_vao));
                gl.draw_arrays_instanced(glow::TRIANGLE_STRIP, 0, 4, self.blobs.len() as i32);
                gl.disable(glow::POLYGON_OFFSET_FILL);
                stats.draws += 1;
                stats.triangles += 2 * self.blobs.len() as u32;
                stats.instances += self.blobs.len() as u32;
                check(gl, debug)?;
            }

            let mut order = std::mem::take(&mut self.order);
            pack::blended_back_to_front(&list.screens, eye, &mut order);
            for &i in &order {
                if self.draw_screen(gl, &list.screens[i as usize], &view_proj, list) {
                    stats.draws += 1;
                    stats.triangles += 2;
                    stats.instances += 1;
                }
            }
            self.order = order;
            gl.depth_mask(true);
            gl.disable(glow::BLEND);
            gl.disable(glow::DEPTH_TEST);
            check(gl, debug)?;

            // into the target
            let scene = self.scene.as_ref().expect("scene target");
            match (&scene.color, self.opts.preset64) {
                (SceneColor::Texture(tex), true) => {
                    gl.bind_framebuffer(glow::FRAMEBUFFER, dst);
                    gl.viewport(0, 0, dst_size[0] as i32, dst_size[1] as i32);
                    let p = &self.upscale;
                    gl.use_program(Some(p.id));
                    gl.uniform_1_i32(p.u("u_src"), 0);
                    gl.uniform_1_f32(p.u("u_div"), div as f32);
                    gl.active_texture(glow::TEXTURE0);
                    gl.bind_texture(glow::TEXTURE_2D, Some(*tex));
                    gl.bind_vertex_array(Some(self.empty_vao));
                    gl.draw_arrays(glow::TRIANGLES, 0, 3);
                    gl.bind_texture(glow::TEXTURE_2D, None);
                    stats.draws += 1;
                }
                _ => {
                    gl.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(scene.fbo));
                    gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, dst);
                    let (w, h) = (size[0] as i32, size[1] as i32);
                    gl.blit_framebuffer(
                        0,
                        0,
                        w,
                        h,
                        0,
                        0,
                        w,
                        h,
                        glow::COLOR_BUFFER_BIT,
                        glow::NEAREST,
                    );
                }
            }
            // the scene's contents are spent: let tilers skip storing them
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(scene.fbo));
            gl.invalidate_framebuffer(
                glow::FRAMEBUFFER,
                &[glow::COLOR_ATTACHMENT0, glow::DEPTH_ATTACHMENT],
            );
            if timing {
                gl.end_query(TIME_ELAPSED);
            }
            check(gl, debug)?;
        }
        stats.gpu_ns = self.gpu_ns;
        Ok(stats)
    }

    /// One screen quad; false when it cannot be drawn (no texture, or an
    /// external one without the extension).
    unsafe fn draw_screen(
        &self,
        gl: &glow::Context,
        s: &ScreenQuad,
        view_proj: &[f32; 16],
        list: &DrawList,
    ) -> bool {
        let Some(tex) = std::num::NonZeroU32::new(s.tex.id).map(glow::NativeTexture) else {
            return false;
        };
        let (p, target) = if s.tex.external {
            match &self.screen_ext {
                Some(p) => (p, TEXTURE_EXTERNAL_OES),
                None => return false,
            }
        } else {
            (&self.screen, glow::TEXTURE_2D)
        };
        let size_m = glam::Vec2::new(
            s.model.x_axis.truncate().length(),
            s.model.y_axis.truncate().length(),
        );
        let (outline, margin) = if s.selected {
            (
                OUTLINE_M,
                glam::Vec2::splat(OUTLINE_M) / size_m.max(glam::Vec2::splat(1e-4)),
            )
        } else {
            (0.0, glam::Vec2::ZERO)
        };
        let sky = &list.sky;
        unsafe {
            gl.use_program(Some(p.id));
            gl.uniform_matrix_4_f32_slice(p.u("u_model"), false, &s.model.to_cols_array());
            gl.uniform_matrix_4_f32_slice(p.u("u_view_proj"), false, view_proj);
            gl.uniform_2_f32(p.u("u_margin"), margin.x, margin.y);
            gl.uniform_1_i32(p.u("u_tex"), 0);
            gl.uniform_2_f32(p.u("u_size_m"), size_m.x, size_m.y);
            gl.uniform_1_f32(p.u("u_yflip"), if s.tex.y_flip { 1.0 } else { 0.0 });
            gl.uniform_1_f32(p.u("u_opaque"), if s.tex.opaque { 1.0 } else { 0.0 });
            gl.uniform_1_f32(p.u("u_dim"), s.dim.clamp(0.0, 1.0));
            gl.uniform_1_f32(p.u("u_outline"), outline);
            gl.uniform_3_f32_slice(p.u("u_eye"), &list.camera.pose.pos.to_array());
            gl.uniform_3_f32_slice(p.u("u_fog_color"), &sky.fog_color);
            gl.uniform_2_f32(p.u("u_fog"), sky.fog_start, sky.fog_end);
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(target, Some(tex));
            if !s.tex.external {
                // our filtering without touching the client texture's own parameters
                gl.bind_sampler(0, Some(self.sampler));
            }
            gl.bind_vertex_array(Some(self.empty_vao));
            gl.draw_arrays(glow::TRIANGLE_STRIP, 0, 4);
            gl.bind_sampler(0, None);
            gl.bind_texture(target, None);
        }
        true
    }

    unsafe fn ensure_scene(
        &mut self,
        gl: &glow::Context,
        size: [u32; 2],
        samples: i32,
    ) -> Result<(), GlError> {
        if self
            .scene
            .as_ref()
            .is_some_and(|s| s.size == size && s.samples == samples)
        {
            return Ok(());
        }
        unsafe {
            if let Some(s) = self.scene.take() {
                delete_scene(gl, s);
            }
            let (w, h) = (size[0] as i32, size[1] as i32);
            let fbo = gl.create_framebuffer().map_err(gl_err)?;
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
            let color = if samples > 0 {
                let rb = gl.create_renderbuffer().map_err(gl_err)?;
                gl.bind_renderbuffer(glow::RENDERBUFFER, Some(rb));
                gl.renderbuffer_storage_multisample(glow::RENDERBUFFER, samples, glow::RGBA8, w, h);
                gl.framebuffer_renderbuffer(
                    glow::FRAMEBUFFER,
                    glow::COLOR_ATTACHMENT0,
                    glow::RENDERBUFFER,
                    Some(rb),
                );
                SceneColor::Multisample(rb)
            } else {
                let tex = color_texture(gl, size)?;
                gl.framebuffer_texture_2d(
                    glow::FRAMEBUFFER,
                    glow::COLOR_ATTACHMENT0,
                    glow::TEXTURE_2D,
                    Some(tex),
                    0,
                );
                SceneColor::Texture(tex)
            };
            let depth = gl.create_renderbuffer().map_err(gl_err)?;
            gl.bind_renderbuffer(glow::RENDERBUFFER, Some(depth));
            if samples > 0 {
                gl.renderbuffer_storage_multisample(
                    glow::RENDERBUFFER,
                    samples,
                    glow::DEPTH_COMPONENT24,
                    w,
                    h,
                );
            } else {
                gl.renderbuffer_storage(glow::RENDERBUFFER, glow::DEPTH_COMPONENT24, w, h);
            }
            gl.framebuffer_renderbuffer(
                glow::FRAMEBUFFER,
                glow::DEPTH_ATTACHMENT,
                glow::RENDERBUFFER,
                Some(depth),
            );
            gl.bind_renderbuffer(glow::RENDERBUFFER, None);
            let status = gl.check_framebuffer_status(glow::FRAMEBUFFER);
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            self.scene = Some(Scene {
                size,
                samples,
                fbo,
                color,
                depth,
            });
            if status != glow::FRAMEBUFFER_COMPLETE {
                return Err(GlError::Fbo(status));
            }
        }
        Ok(())
    }

    /// Collects the older query's result if ready and starts timing this
    /// frame; false when no query is free (or there is no timer).
    unsafe fn begin_timer(&mut self, gl: &glow::Context) -> bool {
        let Some(t) = self.timer.as_mut() else {
            return false;
        };
        let i = t.next;
        let q = t.queries[i];
        unsafe {
            if t.pending[i] {
                if gl.get_query_parameter_u32(q, glow::QUERY_RESULT_AVAILABLE) == 0 {
                    return false;
                }
                let ns = gl.get_query_parameter_u32(q, glow::QUERY_RESULT);
                // a disjoint event (power state change …) makes the result meaningless
                if gl.get_parameter_i32(GPU_DISJOINT) == 0 {
                    self.gpu_ns = Some(ns as u64);
                }
                t.pending[i] = false;
            }
            gl.begin_query(TIME_ELAPSED, q);
        }
        t.pending[i] = true;
        t.next = 1 - i;
        true
    }

    unsafe fn apply_anisotropy(&mut self, gl: &glow::Context) {
        if let Some(max) = self.caps.max_anisotropy {
            let a = self.opts.anisotropy.clamp(1.0, max.max(1.0));
            unsafe { gl.sampler_parameter_f32(self.sampler, TEXTURE_MAX_ANISOTROPY, a) };
        }
    }

    /// The resolved colour texture of the last `Target::Offscreen` draw.
    pub fn world_texture(&self) -> Option<WorldTexture> {
        self.offscreen.as_ref().map(|o| WorldTexture {
            id: o.tex.0.get(),
            size: o.size,
            y_flipped: true,
        })
    }

    pub fn opts(&self) -> RenderOpts {
        self.opts
    }

    /// Changes the options; the scene target follows on the next `draw`.
    ///
    /// # Safety
    /// `gl` must be current on this thread.
    pub unsafe fn set_opts(&mut self, gl: &glow::Context, opts: RenderOpts) -> Result<(), GlError> {
        self.opts = opts;
        unsafe {
            self.apply_anisotropy(gl);
            check(gl, opts.gl_debug)
        }
    }

    /// # Safety
    /// `gl` must be current on this thread.
    pub unsafe fn destroy(self, gl: &glow::Context) {
        unsafe {
            for p in [self.kit, self.screen, self.blob, self.upscale]
                .into_iter()
                .chain(self.screen_ext)
            {
                gl.delete_program(p.id);
            }
            for m in self.meshes {
                gl.delete_vertex_array(m.vao);
                gl.delete_buffer(m.vbo);
                gl.delete_buffer(m.ibo);
            }
            gl.delete_buffer(self.instance_buf);
            gl.delete_buffer(self.blob_buf);
            gl.delete_vertex_array(self.blob_vao);
            gl.delete_vertex_array(self.empty_vao);
            gl.delete_sampler(self.sampler);
            if let Some(s) = self.scene {
                delete_scene(gl, s);
            }
            if let Some(o) = self.offscreen {
                gl.delete_framebuffer(o.fbo);
                gl.delete_texture(o.tex);
            }
            if let Some(t) = self.timer {
                for q in t.queries {
                    gl.delete_query(q);
                }
            }
        }
    }
}

unsafe fn delete_scene(gl: &glow::Context, s: Scene) {
    unsafe {
        gl.delete_framebuffer(s.fbo);
        match s.color {
            SceneColor::Multisample(rb) => gl.delete_renderbuffer(rb),
            SceneColor::Texture(t) => gl.delete_texture(t),
        }
        gl.delete_renderbuffer(s.depth);
    }
}

/// An RGBA8 texture, nearest-filtered (read 1:1 or upscaled blocky).
unsafe fn color_texture(gl: &glow::Context, size: [u32; 2]) -> Result<glow::Texture, GlError> {
    unsafe {
        let tex = gl.create_texture().map_err(gl_err)?;
        gl.bind_texture(glow::TEXTURE_2D, Some(tex));
        gl.tex_storage_2d(
            glow::TEXTURE_2D,
            1,
            glow::RGBA8,
            size[0] as i32,
            size[1] as i32,
        );
        for (p, v) in [
            (glow::TEXTURE_MIN_FILTER, glow::NEAREST),
            (glow::TEXTURE_MAG_FILTER, glow::NEAREST),
            (glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE),
            (glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE),
        ] {
            gl.tex_parameter_i32(glow::TEXTURE_2D, p, v as i32);
        }
        gl.bind_texture(glow::TEXTURE_2D, None);
        Ok(tex)
    }
}

/// Uploads `data` into a streaming buffer, orphaning the old storage so the
/// driver never stalls on a buffer the GPU still reads; grows by doubling.
/// Returns the new capacity in bytes.
unsafe fn stream(gl: &glow::Context, buf: glow::Buffer, data: &[u8], cap: usize) -> usize {
    let cap = if data.len() > cap {
        data.len().next_power_of_two().max(4096)
    } else {
        cap
    };
    unsafe {
        gl.bind_buffer(glow::ARRAY_BUFFER, Some(buf));
        gl.buffer_data_size(glow::ARRAY_BUFFER, cap as i32, glow::STREAM_DRAW);
        gl.buffer_sub_data_u8_slice(glow::ARRAY_BUFFER, 0, data);
    }
    cap
}

/// Points the bound VAO's instance attributes (3..8) at `first`.
unsafe fn point_instances(gl: &glow::Context, first: u32) {
    let stride = std::mem::size_of::<InstanceData>() as i32;
    let base = first as i32 * stride;
    unsafe {
        for c in 0..4 {
            gl.vertex_attrib_pointer_f32(
                3 + c,
                4,
                glow::FLOAT,
                false,
                stride,
                base + 16 * c as i32,
            );
        }
        gl.vertex_attrib_pointer_f32(7, 4, glow::UNSIGNED_BYTE, true, stride, base + 64);
        gl.vertex_attrib_pointer_f32(8, 2, glow::FLOAT, false, stride, base + 68);
    }
}

unsafe fn upload_mesh(
    gl: &glow::Context,
    id: MeshId,
    instance_buf: glow::Buffer,
) -> Result<GpuMesh, GlError> {
    let m = mesh::build(id);
    unsafe {
        let vao = gl.create_vertex_array().map_err(gl_err)?;
        gl.bind_vertex_array(Some(vao));
        let vbo = gl.create_buffer().map_err(gl_err)?;
        gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
        gl.buffer_data_u8_slice(
            glow::ARRAY_BUFFER,
            bytemuck::cast_slice(&m.vertices),
            glow::STATIC_DRAW,
        );
        let stride = std::mem::size_of::<mesh::Vertex>() as i32;
        gl.enable_vertex_attrib_array(0);
        gl.vertex_attrib_pointer_f32(0, 3, glow::FLOAT, false, stride, 0);
        gl.enable_vertex_attrib_array(1);
        gl.vertex_attrib_pointer_f32(1, 4, glow::BYTE, true, stride, 12);
        gl.enable_vertex_attrib_array(2);
        gl.vertex_attrib_pointer_f32(2, 4, glow::UNSIGNED_BYTE, true, stride, 16);
        let ibo = gl.create_buffer().map_err(gl_err)?;
        gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(ibo));
        gl.buffer_data_u8_slice(
            glow::ELEMENT_ARRAY_BUFFER,
            bytemuck::cast_slice(&m.indices),
            glow::STATIC_DRAW,
        );
        gl.bind_buffer(glow::ARRAY_BUFFER, Some(instance_buf));
        for loc in 3..=8 {
            gl.enable_vertex_attrib_array(loc);
            gl.vertex_attrib_divisor(loc, 1);
        }
        point_instances(gl, 0);
        gl.bind_vertex_array(None);
        gl.bind_buffer(glow::ARRAY_BUFFER, None);
        gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, None);
        Ok(GpuMesh {
            vao,
            vbo,
            ibo,
            indices: m.indices.len() as i32,
            triangles: m.triangles(),
        })
    }
}

unsafe fn program(
    gl: &glow::Context,
    name: &str,
    vs: &str,
    fs: &str,
    uniforms: &[&'static str],
) -> Result<Program, GlError> {
    unsafe {
        let id = gl.create_program().map_err(gl_err)?;
        let mut shaders = Vec::new();
        for (kind, src) in [(glow::VERTEX_SHADER, vs), (glow::FRAGMENT_SHADER, fs)] {
            let sh = gl.create_shader(kind).map_err(gl_err)?;
            gl.shader_source(sh, src);
            gl.compile_shader(sh);
            if !gl.get_shader_compile_status(sh) {
                let log = gl.get_shader_info_log(sh);
                gl.delete_shader(sh);
                gl.delete_program(id);
                return Err(GlError::Shader(format!("{name}: {log}")));
            }
            gl.attach_shader(id, sh);
            shaders.push(sh);
        }
        gl.link_program(id);
        for sh in shaders {
            gl.detach_shader(id, sh);
            gl.delete_shader(sh);
        }
        if !gl.get_program_link_status(id) {
            let log = gl.get_program_info_log(id);
            gl.delete_program(id);
            return Err(GlError::Link(format!("{name}: {log}")));
        }
        let uniforms = uniforms
            .iter()
            .map(|&u| (u, gl.get_uniform_location(id, u)))
            .collect();
        Ok(Program { id, uniforms })
    }
}

/// With `debug`, the first pending GL error.
unsafe fn check(gl: &glow::Context, debug: bool) -> Result<(), GlError> {
    if !debug {
        return Ok(());
    }
    match unsafe { gl.get_error() } {
        glow::NO_ERROR => Ok(()),
        e => Err(GlError::Gl(e)),
    }
}

fn native_fbo(id: u32) -> Option<glow::Framebuffer> {
    std::num::NonZeroU32::new(id).map(glow::NativeFramebuffer)
}

/// Puts GL back in the state `assert_default_state` checks.
unsafe fn restore_default_state(gl: &glow::Context, viewport: [i32; 4], external: bool) {
    unsafe {
        gl.bind_framebuffer(glow::FRAMEBUFFER, None);
        gl.use_program(None);
        gl.bind_vertex_array(None);
        gl.bind_buffer(glow::ARRAY_BUFFER, None);
        gl.active_texture(glow::TEXTURE0);
        gl.bind_sampler(0, None);
        gl.bind_texture(glow::TEXTURE_2D, None);
        if external {
            gl.bind_texture(TEXTURE_EXTERNAL_OES, None);
        }
        gl.enable(glow::BLEND);
        gl.blend_equation(glow::FUNC_ADD);
        gl.blend_func(glow::ONE, glow::ONE_MINUS_SRC_ALPHA);
        gl.disable(glow::DEPTH_TEST);
        gl.disable(glow::CULL_FACE);
        gl.disable(glow::SCISSOR_TEST);
        gl.disable(glow::POLYGON_OFFSET_FILL);
        gl.color_mask(true, true, true, true);
        gl.depth_mask(true);
        gl.viewport(viewport[0], viewport[1], viewport[2], viewport[3]);
    }
}

/// GL state `draw` leaves behind (and that the test `state_restored` asserts):
/// FBO 0, program 0, VAO 0, active texture TEXTURE0 with TEXTURE_2D/EXTERNAL_OES unbound, blend enabled with
/// (ONE, ONE_MINUS_SRC_ALPHA), depth test off, cull off, scissor off, viewport = what it was at entry.
///
/// # Safety
/// `gl` must be current on this thread.
pub unsafe fn assert_default_state(gl: &glow::Context) -> Result<(), String> {
    let mut bad = Vec::new();
    unsafe {
        let int = |p| gl.get_parameter_i32(p);
        if int(glow::DRAW_FRAMEBUFFER_BINDING) != 0 {
            bad.push("draw framebuffer bound");
        }
        if int(glow::READ_FRAMEBUFFER_BINDING) != 0 {
            bad.push("read framebuffer bound");
        }
        if int(glow::CURRENT_PROGRAM) != 0 {
            bad.push("program bound");
        }
        if int(glow::VERTEX_ARRAY_BINDING) != 0 {
            bad.push("VAO bound");
        }
        if int(glow::ACTIVE_TEXTURE) != glow::TEXTURE0 as i32 {
            bad.push("active texture is not TEXTURE0");
        }
        if int(glow::TEXTURE_BINDING_2D) != 0 {
            bad.push("TEXTURE_2D bound");
        }
        if gl
            .supported_extensions()
            .contains("GL_OES_EGL_image_external")
            && int(TEXTURE_BINDING_EXTERNAL_OES) != 0
        {
            bad.push("TEXTURE_EXTERNAL_OES bound");
        }
        if !gl.is_enabled(glow::BLEND) {
            bad.push("blend disabled");
        }
        if int(glow::BLEND_SRC_RGB) != glow::ONE as i32
            || int(glow::BLEND_SRC_ALPHA) != glow::ONE as i32
            || int(glow::BLEND_DST_RGB) != glow::ONE_MINUS_SRC_ALPHA as i32
            || int(glow::BLEND_DST_ALPHA) != glow::ONE_MINUS_SRC_ALPHA as i32
        {
            bad.push("blend func is not (ONE, ONE_MINUS_SRC_ALPHA)");
        }
        if gl.is_enabled(glow::DEPTH_TEST) {
            bad.push("depth test on");
        }
        if gl.is_enabled(glow::CULL_FACE) {
            bad.push("cull face on");
        }
        if gl.is_enabled(glow::SCISSOR_TEST) {
            bad.push("scissor on");
        }
    }
    if bad.is_empty() {
        Ok(())
    } else {
        Err(bad.join(", "))
    }
}
