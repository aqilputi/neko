//! One frame (§5.1, §5.6): the scheduler decides; the world pass runs
//! through `nkw-gl` on the renderer's own GL context, outside any frame;
//! then `OutputDamageTracker` composes [HUD][fade][FOCO window 1:1][world].

use std::borrow::Borrow;
use std::collections::HashMap;
use std::time::Instant;

use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::damage::OutputDamageTracker;
use smithay::backend::renderer::element::memory::{
    MemoryRenderBuffer, MemoryRenderBufferRenderElement,
};
use smithay::backend::renderer::element::solid::{SolidColorBuffer, SolidColorRenderElement};
use smithay::backend::renderer::element::surface::WaylandSurfaceRenderElement;
use smithay::backend::renderer::element::texture::{TextureRenderBuffer, TextureRenderElement};
use smithay::backend::renderer::element::{AsRenderElements, Kind};
use smithay::backend::renderer::gles::GlesTexture;
use smithay::backend::renderer::glow::GlowRenderer;
use smithay::backend::renderer::utils::{import_surface_tree, with_renderer_surface_state};
use smithay::backend::renderer::{Bind, ExportMem, Renderer, Texture, TextureMapping};
use smithay::backend::winit::WinitGraphicsBackend;
use smithay::utils::{Physical, Point, Rectangle, Scale, Size, Transform};

use nkw_core::{ScreenId, Time, Want, schedule};
use nkw_gl::draw::ClientTex;
use nkw_gl::{RenderOpts, Target, WorldRenderer};

use crate::render::{self, Layer};
use crate::state::Nkw;
use crate::stats;

smithay::backend::renderer::element::render_elements! {
    pub Elem<=GlowRenderer>;
    World=TextureRenderElement<GlesTexture>,
    Surface=WaylandSurfaceRenderElement<GlowRenderer>,
    Hud=MemoryRenderBufferRenderElement<GlowRenderer>,
    Solid=SolidColorRenderElement,
}

/// Where frames go.
pub enum Host {
    /// a window in the host compositor
    Winit(Box<WinitGraphicsBackend<GlowRenderer>>),
    /// surfaceless EGL: an offscreen texture, read back for `--shot`
    Headless {
        renderer: Box<GlowRenderer>,
        target: GlesTexture,
    },
}

pub struct Gfx {
    pub host: Host,
    pub damage: OutputDamageTracker,
    /// `None` when the world renderer failed to start (logged once)
    pub world: Option<WorldRenderer>,
    /// the world texture as Smithay sees it: (GL id, size, buffer)
    world_buf: Option<(u32, [u32; 2], TextureRenderBuffer<GlesTexture>)>,
    hud: Option<(&'static str, MemoryRenderBuffer)>,
    fade: SolidColorBuffer,
    dim: SolidColorBuffer,
    dim_size: (i32, i32),
    pub gl_debug: bool,
    pub gl_errors: u32,
}

const GL_RGBA8: u32 = 0x8058;

impl Gfx {
    pub fn new(
        host: Host,
        damage: OutputDamageTracker,
        gl_debug: bool,
        preset64: bool,
    ) -> Result<Gfx, String> {
        let mut g = Gfx {
            host,
            damage,
            world: None,
            world_buf: None,
            hud: None,
            fade: SolidColorBuffer::default(),
            dim: SolidColorBuffer::default(),
            dim_size: (0, 0),
            gl_debug,
            gl_errors: 0,
        };
        let opts = RenderOpts {
            gl_debug,
            preset64,
            ..RenderOpts::default()
        };
        let r = g
            .renderer()
            .with_context(|gl| unsafe { WorldRenderer::new(gl, opts) })
            .map_err(|e| format!("GL context: {e}"))?;
        match r {
            Ok(w) => g.world = Some(w),
            Err(e) => return Err(format!("world renderer: {e:?}")),
        }
        Ok(g)
    }

    pub fn renderer(&mut self) -> &mut GlowRenderer {
        match &mut self.host {
            Host::Winit(b) => b.renderer(),
            Host::Headless { renderer, .. } => renderer,
        }
    }

    /// `glGetError` after a pass (only with `--gl-debug`).
    fn check(&mut self, what: &str) {
        if !self.gl_debug {
            return;
        }
        let err = self
            .renderer()
            .with_context(|gl| unsafe { glow::HasContext::get_error(&**gl) })
            .unwrap_or(0);
        if err != 0 {
            self.gl_errors += 1;
            eprintln!("nkw: GL error {err:#x} after {what}");
        }
    }
}

use nkw_gl::glow;

/// Formats Mesa imports as `GL_TEXTURE_EXTERNAL_OES` only.
fn external(f: Option<Fourcc>) -> bool {
    matches!(
        f,
        Some(
            Fourcc::Nv12
                | Fourcc::Nv21
                | Fourcc::P010
                | Fourcc::Yuv420
                | Fourcc::Yvu420
                | Fourcc::Yuyv
                | Fourcc::Uyvy
        )
    )
}

/// What one composed frame cost (µs) and drew.
#[derive(Default)]
struct Cost {
    cull_us: u64,
    submit_us: u64,
    compose_us: u64,
    draws: u32,
    tris: u32,
    gpu_us: Option<u64>,
}

impl Nkw {
    /// The vblank grid frames sit on.
    pub fn grid(&self, now: Time) -> schedule::Vblank {
        schedule::Vblank {
            epoch: self.sched.epoch.unwrap_or(now),
            period: self.sched.period,
        }
    }

    /// One frame: tick, plan, maybe draw, frame callbacks, next wakeup.
    pub fn frame(&mut self) {
        let was_requested = self.sched.frame_requested;
        self.sched.frame_requested = false;
        let t0 = Instant::now();
        let now = self.now();
        if self.sched.epoch.is_none() {
            self.sched.epoch = Some(now);
        }
        self.world.tick(now);
        let wishes = self.world.wishes();
        let grid = self.grid(now);
        let mut plan = schedule::plan(&grid, now, &wishes);
        if !was_requested || self.sched.full || !self.shots.is_empty() {
            // host-initiated redraw (expose, resize) or a pending shot
            plan.compose = true;
        }
        if self.sched.last_world == 0.0 && plan.compose {
            plan.world_pass = true;
        }
        let update_us = t0.elapsed().as_micros() as u64;
        if plan.compose {
            let cost = self.render(plan.world_pass, now);
            let animated = wishes.iter().any(|w| w.want == Want::Continuous);
            let f = stats::Frame {
                t_us: stats::mono_us(),
                frame: self.stats.frames + 1,
                pass: if plan.world_pass { "world" } else { "compose" },
                update_us,
                cull_us: cost.cull_us,
                submit_us: cost.submit_us,
                compose_us: cost.compose_us,
                draws: cost.draws,
                tris: cost.tris,
                gpu_us: cost.gpu_us,
            };
            let period = self.sched.period;
            self.stats.frame(&f, now, animated, period);
        }
        self.world.frame_done(now);
        self.sched.full = false;
        self.send_frames();
        let _ = self.dh.flush_clients();
        self.wake();
    }

    /// Ask for a frame when the wishes want one, else arm the next wakeup.
    pub fn wake(&mut self) {
        let w = self.world.wishes();
        if w.is_empty() && self.shots.is_empty() {
            return;
        }
        let now = self.now();
        let p = schedule::plan(&self.grid(now), now, &w);
        if p.compose || !self.shots.is_empty() {
            self.request_frame();
        } else {
            self.arm(p.next_wakeup);
        }
    }

    fn render(&mut self, world_pass: bool, now: Time) -> Cost {
        let mut cost = Cost::default();
        let out_px = [self.world.out.width_px, self.world.out.height_px];
        let scale = self.world.out.scale as f64;
        let sky = nkw_gl::draw::sky_for_band(crate::scene::BAND);
        // the age of the window's back buffer, while its surface is still
        // current (the world pass makes the context current without it)
        let age = match &mut self.gfx.host {
            Host::Winit(b) => {
                // imports and the world pass leave the context current
                // without the window surface; the age query needs it
                let surface: *const smithay::backend::egl::EGLSurface = b.egl_surface();
                // SAFETY: the surface and the renderer are distinct fields of `b`
                let ok = unsafe {
                    b.renderer()
                        .egl_context()
                        .make_current_with_surface(&*surface)
                };
                if ok.is_ok() {
                    b.buffer_age().unwrap_or(0)
                } else {
                    0
                }
            }
            _ => 0,
        };

        // the world pass, outside any Smithay frame
        if world_pass && self.gfx.world.is_some() {
            let t = Instant::now();
            let renderer = self.gfx.renderer();
            for (_, w) in &self.windows {
                if let Some(tl) = w.toplevel() {
                    let _ = import_surface_tree(renderer, tl.wl_surface());
                }
            }
            let ctx = renderer.context_id();
            let texs: HashMap<ScreenId, ClientTex> = self
                .windows
                .iter()
                .filter_map(|(id, w)| {
                    let s = w.toplevel()?.wl_surface().clone();
                    let t = with_renderer_surface_state(&s, |st| {
                        st.texture::<GlesTexture>(ctx.clone()).map(|t| ClientTex {
                            id: t.tex_id(),
                            external: external(t.format()),
                            size: [t.width(), t.height()],
                            y_flip: t.is_y_inverted(),
                            opaque: true,
                        })
                    })??;
                    Some((*id, t))
                })
                .collect();
            let list = crate::scene::draw_list(&self.world, &|id| texs.get(&id).copied());
            cost.cull_us = t.elapsed().as_micros() as u64;
            let t = Instant::now();
            let world = self.gfx.world.as_mut().unwrap();
            let host_r = match &mut self.gfx.host {
                Host::Winit(b) => b.renderer(),
                Host::Headless { renderer, .. } => &mut **renderer,
            };
            let res = host_r.with_context(|gl| unsafe {
                world.resize(gl, out_px)?;
                world.draw(gl, &list, Target::Offscreen)
            });
            cost.submit_us = t.elapsed().as_micros() as u64;
            match res {
                Ok(Ok(s)) => {
                    cost.draws = s.draws;
                    cost.tris = s.triangles;
                    cost.gpu_us = s.gpu_ns.map(|n| n / 1000);
                }
                Ok(Err(e)) => eprintln!("nkw: world pass: {e:?}"),
                Err(e) => eprintln!("nkw: world pass context: {e}"),
            }
            self.sched.last_world = now;
            self.gfx.check("world pass");
            self.refresh_world_buf();
        }

        let t = Instant::now();
        let fading = self.world.fade(now) > 0.0;
        let mut elems: Vec<Elem> = Vec::new();
        for layer in render::layers(&self.world, fading) {
            match layer {
                Layer::Hud => {
                    let label = self.world.mode().label();
                    if self.gfx.hud.as_ref().map(|h| h.0) != Some(label) {
                        let (w, h, px) = crate::hud::label(label, 2);
                        let buf = MemoryRenderBuffer::from_slice(
                            &px,
                            Fourcc::Abgr8888,
                            (w as i32, h as i32),
                            1,
                            Transform::Normal,
                            None,
                        );
                        self.gfx.hud = Some((label, buf));
                    }
                    let buf = self.gfx.hud.as_ref().unwrap().1.clone();
                    let r = self.gfx.renderer();
                    if let Ok(e) = MemoryRenderBufferRenderElement::from_buffer(
                        r,
                        (16.0 * scale, 16.0 * scale),
                        &buf,
                        None,
                        None,
                        None,
                        Kind::Unspecified,
                    ) {
                        elems.push(Elem::Hud(e));
                    }
                }
                Layer::Fade => {
                    let a = self.world.fade(now);
                    let logical = (
                        (out_px[0] as f64 / scale) as i32,
                        (out_px[1] as f64 / scale) as i32,
                    );
                    let c = sky.color;
                    self.gfx
                        .fade
                        .update(logical, [c[0] * a, c[1] * a, c[2] * a, a]);
                    elems.push(Elem::Solid(SolidColorRenderElement::from_buffer(
                        &self.gfx.fade,
                        (0, 0),
                        Scale::from(scale),
                        1.0,
                        Kind::Unspecified,
                    )));
                }
                Layer::Dim => {
                    let logical = (
                        (out_px[0] as f64 / scale).ceil() as i32,
                        (out_px[1] as f64 / scale).ceil() as i32,
                    );
                    let a = crate::scene::FOCUS_DIM;
                    if self.gfx.dim_size != logical {
                        self.gfx.dim_size = logical;
                        self.gfx.dim.update(logical, [0.0, 0.0, 0.0, a]);
                    }
                    elems.push(Elem::Solid(SolidColorRenderElement::from_buffer(
                        &self.gfx.dim,
                        (0, 0),
                        Scale::from(scale),
                        1.0,
                        Kind::Unspecified,
                    )));
                }
                Layer::Focus(_) => {
                    let Some((id, loc)) = self.focus_location() else {
                        continue;
                    };
                    let Some(w) = self.window(id).cloned() else {
                        continue;
                    };
                    let loc: Point<i32, Physical> = loc.to_physical_precise_round(scale);
                    let r = self.gfx.renderer();
                    let e: Vec<WaylandSurfaceRenderElement<GlowRenderer>> =
                        w.render_elements(r, loc, Scale::from(scale), 1.0);
                    elems.extend(e.into_iter().map(Elem::Surface));
                }
                Layer::World => {
                    if let Some((_, _, buf)) = &self.gfx.world_buf {
                        let logical = Size::from((
                            (out_px[0] as f64 / scale).round() as i32,
                            (out_px[1] as f64 / scale).round() as i32,
                        ));
                        elems.push(Elem::World(
                            TextureRenderElement::from_texture_render_buffer(
                                (0.0, 0.0),
                                buf,
                                None,
                                None,
                                Some(logical),
                                Kind::Unspecified,
                            ),
                        ));
                    }
                }
            }
        }

        let clear = [sky.color[0], sky.color[1], sky.color[2], 1.0];
        let shots = std::mem::take(&mut self.shots);
        let size = Size::<i32, Physical>::from((out_px[0] as i32, out_px[1] as i32));
        let mut captured: Option<Vec<u8>> = None;
        let gfx = &mut self.gfx;
        match &mut gfx.host {
            Host::Winit(b) => {
                let mut damage = None;
                let mut bound = false;
                if let Ok((r, mut fb)) = b.bind() {
                    bound = true;
                    match gfx.damage.render_output(r, &mut fb, age, &elems, clear) {
                        Ok(res) => damage = res.damage.cloned(),
                        Err(e) => eprintln!("nkw: compose: {e:?}"),
                    }
                    if !shots.is_empty() {
                        captured = read_back(r, &fb, size, true);
                    }
                } else {
                    eprintln!("nkw: bind failed");
                }
                if bound && !shots.is_empty() {
                    // mapping the read back made the context current without
                    // the window surface; swapping needs it current again
                    let surface: *const smithay::backend::egl::EGLSurface = b.egl_surface();
                    // SAFETY: the surface and the renderer are distinct fields of `b`
                    let _ = unsafe {
                        b.renderer()
                            .egl_context()
                            .make_current_with_surface(&*surface)
                    };
                }
                if bound
                    && (damage.is_some() || !shots.is_empty())
                    && let Err(e) = b.submit(damage.as_deref())
                {
                    eprintln!("nkw: submit: {e:?}");
                }
            }
            Host::Headless { renderer, target } => match renderer.bind(target) {
                Ok(mut fb) => {
                    if let Err(e) =
                        gfx.damage
                            .render_output(&mut **renderer, &mut fb, 1, &elems, clear)
                    {
                        eprintln!("nkw: compose: {e:?}");
                    }
                    if !shots.is_empty() {
                        captured = read_back(&mut **renderer, &fb, size, false);
                    }
                }
                Err(e) => eprintln!("nkw: bind: {e:?}"),
            },
        }
        cost.compose_us = t.elapsed().as_micros() as u64;
        self.gfx.check("compose");
        if !shots.is_empty() {
            match captured {
                Some(px) => {
                    for p in &shots {
                        if let Err(e) = crate::shot::write_png(p, size.w as u32, size.h as u32, &px)
                        {
                            eprintln!("nkw: shot {}: {e}", p.display());
                        }
                    }
                }
                None => eprintln!("nkw: shot: read back failed"),
            }
        }
        cost
    }

    /// Wrap the world renderer's texture for Smithay; on a new GL id (a
    /// resize), the old wrapper is leaked on purpose: `GlesTexture` would
    /// delete an id that `nkw-gl` owns.
    fn refresh_world_buf(&mut self) {
        let Some(t) = self.gfx.world.as_ref().and_then(|w| w.world_texture()) else {
            return;
        };
        let stale = self
            .gfx
            .world_buf
            .as_ref()
            .is_none_or(|(id, size, _)| *id != t.id || *size != t.size);
        if stale {
            if let Some(old) = self.gfx.world_buf.take() {
                std::mem::forget(old);
            }
            let r = self.gfx.renderer();
            let tex = unsafe {
                GlesTexture::from_raw(
                    (*r).borrow(),
                    Some(GL_RGBA8),
                    true,
                    t.id,
                    (t.size[0] as i32, t.size[1] as i32).into(),
                )
            };
            let transform = if t.y_flipped {
                Transform::Flipped180
            } else {
                Transform::Normal
            };
            let buf = TextureRenderBuffer::from_texture(&*r, tex, 1, transform, None);
            self.gfx.world_buf = Some((t.id, t.size, buf));
        }
        if let Some((_, size, buf)) = self.gfx.world_buf.as_mut() {
            let full = Rectangle::from_size((size[0] as i32, size[1] as i32).into());
            let _ = buf.render().draw(|_| Ok::<_, ()>(vec![full]));
        }
    }
}

/// The composed frame as top-down RGBA. `output_flipped`: the output
/// renders with `Transform::Flipped180` (the winit window's GL surface).
fn read_back<R>(
    r: &mut R,
    fb: &R::Framebuffer<'_>,
    size: Size<i32, Physical>,
    output_flipped: bool,
) -> Option<Vec<u8>>
where
    R: ExportMem,
    R::TextureMapping: TextureMapping,
{
    let region = Rectangle::from_size((size.w, size.h).into());
    let m = r.copy_framebuffer(fb, region, Fourcc::Abgr8888).ok()?;
    let flipped = m.flipped();
    let mut px = r.map_texture(&m).ok()?.to_vec();
    if flipped == output_flipped {
        crate::shot::flip_rows(&mut px, size.w as u32, size.h as u32);
    }
    crate::shot::opaque(&mut px);
    Some(px)
}
