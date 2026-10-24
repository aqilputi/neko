//! One frame (§5.1, §5.6): the scheduler decides; the world pass runs
//! through `nkw-gl` on the renderer's own GL context, outside any frame;
//! then `OutputDamageTracker` composes [HUD][fade][FOCO window 1:1][world].

use std::borrow::Borrow;
use std::collections::HashMap;
use std::time::Instant;

use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::damage::OutputDamageTracker;
use smithay::backend::renderer::element::memory::MemoryRenderBufferRenderElement;
use smithay::backend::renderer::element::solid::{SolidColorBuffer, SolidColorRenderElement};
use smithay::backend::renderer::element::surface::WaylandSurfaceRenderElement;
use smithay::backend::renderer::element::texture::{TextureRenderBuffer, TextureRenderElement};
use smithay::backend::renderer::element::{AsRenderElements, Kind};
use smithay::backend::renderer::gles::GlesTexture;
use smithay::backend::renderer::glow::GlowRenderer;
use smithay::backend::renderer::utils::{
    CommitCounter, import_surface_tree, with_renderer_surface_state,
};
use smithay::backend::renderer::{Bind, ExportMem, Renderer, Texture, TextureMapping};
use smithay::backend::winit::WinitGraphicsBackend;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{Physical, Point, Rectangle, Scale, Size, Transform};

use nkw_core::{Kind as ModeKind, ScreenId, Time, Want, schedule};
use nkw_gl::draw::ClientTex;
use nkw_gl::{RenderOpts, Target, WorldRenderer};

use crate::ext::{self, HudInput, Plugin};
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
    /// the last world pass's list and each screen's commit then: what the
    /// next pass compares against to damage only what changed (S4)
    last_list: Option<nkw_gl::draw::DrawList>,
    /// the list before that, cleared and refilled by the next pass
    spare_list: Option<nkw_gl::draw::DrawList>,
    commits: HashMap<ScreenId, CommitCounter>,
    /// world passes skipped because nothing visible changed
    pub skipped_passes: u64,
    /// `NKW_FULL_DAMAGE=1`: the phase-0 path (upload every window, damage the
    /// whole world texture every pass), for A/B measurements
    full_damage: bool,
    fade: SolidColorBuffer,
    dim: SolidColorBuffer,
    dim_size: (i32, i32),
    pub gl_debug: bool,
    pub gl_errors: u32,
    /// the last composed HUD elements as placed (`dump()["host"]["hud"]`)
    pub hud_placed: Vec<HudPlaced>,
}

/// One composed HUD element: where it went (physical px) and, for a
/// physical-px buffer, its buffer size.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize)]
pub struct HudPlaced {
    pub id: u64,
    pub at: (f64, f64),
    pub buf_scale: f64,
    /// buffer px (`None` for integer-scale buffers)
    pub px: Option<(f64, f64)>,
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
            last_list: None,
            spare_list: None,
            commits: HashMap::new(),
            skipped_passes: 0,
            full_damage: std::env::var_os("NKW_FULL_DAMAGE").is_some_and(|v| v == "1"),
            fade: SolidColorBuffer::default(),
            dim: SolidColorBuffer::default(),
            dim_size: (0, 0),
            gl_debug,
            gl_errors: 0,
            hud_placed: Vec::new(),
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

/// What the host window's cursor shows (S20): the system default, nothing,
/// or a named shape a client asked for (`wp_cursor_shape_v1`, W4).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HostCursor {
    #[default]
    Default,
    Hidden,
    Named(smithay::input::pointer::CursorIcon),
}

impl HostCursor {
    /// `dump()["host"]["cursor"]`: `"default"`, `"hidden"` or the icon's
    /// CSS name.
    pub fn name(&self) -> &'static str {
        match self {
            HostCursor::Default => "default",
            HostCursor::Hidden => "hidden",
            HostCursor::Named(i) => i.name(),
        }
    }
}

/// What one composed frame cost (µs) and drew.
#[derive(Default)]
struct Cost {
    import_us: u64,
    hud_us: u64,
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
        let t_ext = Instant::now();
        self.ext_step(now);
        let ext_us = t_ext.elapsed().as_micros() as u64;
        self.world.tick(now);
        let wishes = self.all_wishes();
        let rates = wishes
            .iter()
            .filter(|w| matches!(w.want, Want::Rate(_)))
            .count();
        self.stats.ext(ext_us, rates, now);
        self.sync_pointer_lock();
        let grid = self.grid(now);
        let mut plan = schedule::plan(&grid, now, &wishes);
        if !was_requested || self.sched.full || !self.shots.is_empty() {
            // host-initiated redraw (expose, resize) or a pending shot
            plan.compose = true;
        }
        // compose on HUD change (S14): a dialog that finished revealing, a
        // badge that changed … with nobody wishing for it. Only on frames
        // that run anyway, and only while the HUD can change by itself.
        let mut hud = None;
        if !plan.compose && self.hud_may_change() {
            let h = self.hud_input(now);
            if self.hud_hash != Some(ext::hud_hash(&h)) {
                plan.compose = true;
                self.stats.compose_hud_changes += 1;
            }
            hud = Some(h);
        }
        if self.sched.last_world == 0.0 && plan.compose {
            plan.world_pass = true;
        }
        let update_us = t0.elapsed().as_micros() as u64;
        if plan.compose {
            let cost = self.render(plan.world_pass, now, hud);
            let animated = wishes.iter().any(|w| w.want == Want::Continuous);
            let f = stats::Frame {
                t_us: stats::mono_us(),
                frame: self.stats.frames + 1,
                pass: if plan.world_pass { "world" } else { "compose" },
                update_us,
                import_us: cost.import_us,
                cull_us: cost.cull_us,
                submit_us: cost.submit_us,
                compose_us: cost.compose_us,
                hud_us: cost.hud_us,
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

    /// Show `c` on the host window (winit); headless: only recorded
    /// (`dump()["host"]["cursor"]`). The walk's pointer lock hides the
    /// cursor on its own and wins while it holds.
    pub fn set_host_cursor(&mut self, c: HostCursor) {
        if self.host_cursor == c {
            return;
        }
        self.host_cursor = c;
        if let Host::Winit(b) = &self.gfx.host {
            let w = b.window();
            match c {
                HostCursor::Hidden => w.set_cursor_visible(false),
                HostCursor::Default => {
                    w.set_cursor(smithay::input::pointer::CursorIcon::Default.into());
                    w.set_cursor_visible(!self.pointer_locked);
                }
                HostCursor::Named(i) => {
                    w.set_cursor(i.into());
                    w.set_cursor_visible(!self.pointer_locked);
                }
            }
        }
    }

    /// Ask for a frame when the wishes want one, else arm the next wakeup.
    pub fn wake(&mut self) {
        let w = self.all_wishes();
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

    /// Whether the HUD may differ from the last composed one without a
    /// wish: the last one was busy, or a dialog or SALTO is up now.
    fn hud_may_change(&self) -> bool {
        self.hud_busy
            || matches!(self.world.mode().kind(), Some(ModeKind::Hint { .. }))
            || self.ext.cats.dialog().is_some()
            || self.ext.objects.dialog().is_some()
            || self.phone_state().is_some()
    }

    /// The phone's state while the overlay has it open (S21).
    pub fn phone_state(&self) -> Option<crate::ui::phone::PhoneState> {
        self.ext.overlay.phone_state()
    }

    fn render(&mut self, world_pass: bool, now: Time, hud: Option<HudInput>) -> Cost {
        let mut cost = Cost::default();
        // the client surfaces this frame shows (presentation feedback, S23)
        let mut presented: Vec<WlSurface> = Vec::new();
        let out_px = [self.world.out.width_px, self.world.out.height_px];
        let scale = self.world.out.scale as f64;
        let sky = nkw_gl::draw::sky_for_band(self.world.band);
        // the culling set: the current room and what the plugins open
        let opens = {
            let ctx = crate::hook_ctx!(self, now);
            ext::opens_rooms(&self.ext.plugins_ref(), self.world.current_room_id(), &ctx)
        };
        self.visible = self.world.visible_rooms(&opens);
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
            {
                let ctx = crate::hook_ctx!(self, now);
                let (objects, cats) = (&mut self.ext.objects, &mut self.ext.cats);
                let _ = self.gfx.renderer().with_context(|gl| unsafe {
                    objects.upload(gl, &ctx);
                    cats.upload(gl, &ctx);
                });
            }
            // upload only what this pass can show: a window out of view, in
            // another room or frozen behind FOCO keeps its last texture (S4, b)
            let shown: Vec<bool> = self
                .windows
                .iter()
                .map(|(id, _)| {
                    self.gfx.full_damage || render::lod(&self.world, *id) != render::Lod::Hidden
                })
                .collect();
            let t_import = Instant::now();
            let renderer = self.gfx.renderer();
            for ((_, w), show) in self.windows.iter().zip(&shown) {
                if *show && let Some(tl) = w.toplevel() {
                    let _ = import_surface_tree(renderer, tl.wl_surface());
                    presented.push(tl.wl_surface().clone());
                }
            }
            cost.import_us = t_import.elapsed().as_micros() as u64;
            let ctx = renderer.context_id();
            let mut commits: HashMap<ScreenId, CommitCounter> = HashMap::new();
            let texs: HashMap<ScreenId, ClientTex> = self
                .windows
                .iter()
                .zip(&shown)
                .filter_map(|((id, w), show)| {
                    let s = w.toplevel()?.wl_surface().clone();
                    let (t, c) = with_renderer_surface_state(&s, |st| {
                        let t = st.texture::<GlesTexture>(ctx.clone()).map(|t| ClientTex {
                            id: t.tex_id(),
                            external: external(t.format()),
                            size: [t.width(), t.height()],
                            y_flip: t.is_y_inverted(),
                            opaque: true,
                        });
                        (t, st.current_commit())
                    })?;
                    // a window not uploaded keeps the commit its texture shows
                    let seen = if *show {
                        Some(c)
                    } else {
                        self.gfx.commits.get(id).copied()
                    };
                    if let Some(c) = seen {
                        commits.insert(*id, c);
                    }
                    Some((*id, t?))
                })
                .collect();
            // two lists, swapped: the last pass's and this one (S4, a)
            let mut list = self.gfx.spare_list.take().unwrap_or_else(|| {
                let c = nkw_gl::draw::Camera {
                    pose: self.world.camera,
                    fov_y: 1.0,
                    near: 0.1,
                    far: 1.0,
                    aspect: 1.0,
                };
                nkw_gl::draw::DrawList::empty(c, sky)
            });
            crate::scene::draw_list_into(
                &self.world,
                &|id| texs.get(&id).copied(),
                &self.visible,
                &mut list,
            );
            {
                let ctx = crate::hook_ctx!(self, now);
                self.ext.wm.draw(&ctx, &mut list);
                self.ext.objects.draw(&ctx, &mut list);
                self.ext.cats.draw(&ctx, &mut list);
            }
            // what changed since the last pass: the list itself, or only the
            // contents of some screens (a client commit; a plugin's texture
            // may change under the same list, so those always count)
            let by_tex: HashMap<u32, ScreenId> = texs.iter().map(|(id, t)| (t.id, *id)).collect();
            let committed: Vec<usize> = list
                .screens
                .iter()
                .enumerate()
                .filter(|(_, q)| match by_tex.get(&q.tex.id) {
                    Some(id) => self.gfx.commits.get(id) != commits.get(id),
                    None => true,
                })
                .map(|(i, _)| i)
                .collect();
            let tex_rows_gl = self
                .gfx
                .world
                .as_ref()
                .and_then(|w| w.world_texture())
                .is_none_or(|t| t.y_flipped);
            let buf_ok = !self.gfx.full_damage
                && self
                    .gfx
                    .world_buf
                    .as_ref()
                    .is_some_and(|(_, size, _)| *size == out_px);
            let damage = if buf_ok {
                render::world_damage(
                    self.gfx.last_list.as_ref(),
                    &list,
                    &committed,
                    out_px,
                    tex_rows_gl,
                )
            } else {
                render::WorldDamage::Full
            };
            self.gfx.commits = commits;
            cost.cull_us = (t.elapsed().as_micros() as u64).saturating_sub(cost.import_us);
            if damage == render::WorldDamage::None {
                // the texture already shows this list: no GL work at all
                self.gfx.skipped_passes += 1;
                self.sched.last_world = now;
                self.gfx.spare_list = Some(list);
            } else {
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
                self.refresh_world_buf(&damage);
                self.gfx.spare_list = self.gfx.last_list.replace(list);
            }
        }

        let t = Instant::now();
        let mut hud = hud;
        let fading = self.world.fade(now) > 0.0;
        let mut elems: Vec<Elem> = Vec::with_capacity(16);
        for layer in render::layers(&self.world, fading) {
            match layer {
                Layer::Hud => {
                    let t_hud = Instant::now();
                    // built once per frame: the HUD-change check may have it
                    let hud = match hud.take() {
                        Some(h) => h,
                        None => self.hud_input(now),
                    };
                    self.hud_hash = Some(ext::hud_hash(&hud));
                    self.hud_busy = hud.busy();
                    let overlay = {
                        let ctx = crate::hook_ctx!(self, now);
                        self.ext.overlay.elements(&ctx, &hud)
                    };
                    let mut placed = std::mem::take(&mut self.gfx.hud_placed);
                    placed.clear();
                    let r = self.gfx.renderer();
                    for o in overlay {
                        if let Some((e, p)) = hud_element(r, &o, scale) {
                            elems.push(Elem::Hud(e));
                            placed.push(p);
                        }
                    }
                    self.gfx.hud_placed = placed;
                    cost.hud_us = t_hud.elapsed().as_micros() as u64;
                }
                Layer::FocusCat(cat) => {
                    let got = {
                        let ctx = crate::hook_ctx!(self, now);
                        self.ext.cats.focus_buffer(cat, &ctx)
                    };
                    let Some((buf, pos)) = got else { continue };
                    let r = self.gfx.renderer();
                    let at = (pos.0 as f64 * scale, pos.1 as f64 * scale);
                    if let Ok(e) = MemoryRenderBufferRenderElement::from_buffer(
                        r,
                        at,
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
                    if let Some(tl) = w.toplevel()
                        && !presented.contains(tl.wl_surface())
                    {
                        presented.push(tl.wl_surface().clone());
                    }
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
        let mut shown = false;
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
                if bound && (damage.is_some() || !shots.is_empty()) {
                    match b.submit(damage.as_deref()) {
                        Ok(()) => shown = true,
                        Err(e) => eprintln!("nkw: submit: {e:?}"),
                    }
                }
            }
            Host::Headless { renderer, target } => match renderer.bind(target) {
                Ok(mut fb) => {
                    match gfx
                        .damage
                        .render_output(&mut **renderer, &mut fb, 1, &elems, clear)
                    {
                        Ok(_) => shown = true,
                        Err(e) => eprintln!("nkw: compose: {e:?}"),
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
        if shown {
            let at = std::time::Duration::from_micros(stats::mono_us());
            let refresh = self.period();
            let seq = self.stats.frames + 1;
            self.protos_presented(&presented, at, refresh, seq);
        }
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

    /// What the overlay composes this frame, gathered from the plugins.
    fn hud_input(&self, now: Time) -> HudInput {
        let ctx = crate::hook_ctx!(self, now);
        let ps = self.ext.plugins_ref();
        let salto = matches!(self.world.mode().kind(), Some(ModeKind::Hint { .. }));
        let s = self.world.out.scale;
        HudInput {
            mode_label: self.world.mode().label(),
            badges: ps.iter().flat_map(|p| p.badges(&ctx)).collect(),
            dialog: self.ext.cats.dialog().or(self.ext.objects.dialog()),
            focus_rect: self.world.focus_2d_target().map(|(_, o)| {
                let (p, z) = (o.origin / s, o.size / s);
                [p.x as i32, p.y as i32, z.x as i32, z.y as i32]
            }),
            tags: ps.iter().flat_map(|p| p.tags(&ctx)).collect(),
            // SALTO's labels, and the map's shelves (R3: `Pick::Book`)
            pickables: if salto || self.ext.overlay.submode() == Some("map") {
                ps.iter().flat_map(|p| p.pickables(&ctx)).collect()
            } else {
                Vec::new()
            },
            // ANDAR only (the aim is the walker's crosshair)
            aimed: self.world.walking.is_some()
                && (self.world.aim().is_some() || self.ext.objects.aimed(&ctx).is_some()),
            status: ps
                .iter()
                .fold(ext::Status::default(), |a, p| a.merge(p.status(&ctx))),
            phone: self.phone_state().map(|st| ext::phone_view(&ps, st, &ctx)),
        }
    }

    /// Wrap the world renderer's texture for Smithay; on a new GL id (a
    /// resize), the old wrapper is leaked on purpose: `GlesTexture` would
    /// delete an id that `nkw-gl` owns.
    fn refresh_world_buf(&mut self, damage: &render::WorldDamage) {
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
            let rects = match damage {
                render::WorldDamage::Rects(r) if !stale => r
                    .iter()
                    .map(|&[x, y, w, h]| Rectangle::new((x, y).into(), (w, h).into()))
                    .collect(),
                render::WorldDamage::None if !stale => return,
                _ => vec![full],
            };
            let _ = buf.render().draw(|_| Ok::<_, ()>(rects));
        }
    }
}

/// `(at, src, size)` of [`hud_placement`].
pub type HudGeom = ((f64, f64), Option<(f64, f64)>, Option<(i32, i32)>);

/// Where a HUD element lands and at what size (S14). An integer-scale
/// buffer (`buf_scale == 1`) is placed as Smithay sizes it. A physical-px
/// buffer (`buf_scale = output scale`, rastered with Smithay scale 1) goes
/// on whole physical pixels, its logical size `round(px / buf_scale)`, and
/// its source rectangle exactly the pixels drawn, so texels map 1:1 (no
/// resampling blur at fractional scales). Returns `(at, src, size)`: the
/// physical location, the source rectangle's size in buffer px (`None`:
/// the whole buffer) and the logical size (`None`: the buffer's own).
pub fn hud_placement(
    pos: (i32, i32),
    buf_px: (f64, f64),
    buf_scale: f64,
    out_scale: f64,
) -> HudGeom {
    if buf_scale == 1.0 {
        return (
            (pos.0 as f64 * out_scale, pos.1 as f64 * out_scale),
            None,
            None,
        );
    }
    let at = (
        (pos.0 as f64 * out_scale).round(),
        (pos.1 as f64 * out_scale).round(),
    );
    let logical = (
        (buf_px.0 / buf_scale).round() as i32,
        (buf_px.1 / buf_scale).round() as i32,
    );
    // what Smithay will draw: round(logical × scale) px from a whole-px origin
    let drawn = (
        (logical.0 as f64 * out_scale).round(),
        (logical.1 as f64 * out_scale).round(),
    );
    let k = buf_scale / out_scale;
    (at, Some((drawn.0 * k, drawn.1 * k)), Some(logical))
}

/// The render element of one overlay element ([`hud_placement`]).
fn hud_element(
    r: &mut GlowRenderer,
    o: &crate::ext::OverlayElem,
    scale: f64,
) -> Option<(MemoryRenderBufferRenderElement<GlowRenderer>, HudPlaced)> {
    use smithay::backend::renderer::element::Element;
    let plain = |r: &mut GlowRenderer, at: (f64, f64)| {
        MemoryRenderBufferRenderElement::from_buffer(
            r,
            at,
            &o.buf,
            None,
            None,
            None,
            Kind::Unspecified,
        )
        .ok()
    };
    let placed = |at, px| HudPlaced {
        id: o.id,
        at,
        buf_scale: o.buf_scale,
        px,
    };
    if o.buf_scale == 1.0 {
        let (at, _, _) = hud_placement(o.pos, (0.0, 0.0), 1.0, scale);
        return plain(r, at).map(|e| (e, placed(at, None)));
    }
    // the buffer's size in px: the whole-buffer element's source
    let px = plain(r, (0.0, 0.0))?.src().size;
    let (at, src, size) = hud_placement(o.pos, (px.w, px.h), o.buf_scale, scale);
    MemoryRenderBufferRenderElement::from_buffer(
        r,
        at,
        &o.buf,
        None,
        src.map(|(w, h)| Rectangle::from_size((w, h).into())),
        size.map(|(w, h)| (w, h).into()),
        Kind::Unspecified,
    )
    .ok()
    .map(|e| (e, placed(at, Some((px.w, px.h)))))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integer_buffers_are_placed_as_before() {
        assert_eq!(
            hud_placement((10, 20), (99.0, 9.0), 1.0, 1.25),
            ((12.5, 25.0), None, None)
        );
        assert_eq!(
            hud_placement((3, 4), (1.0, 1.0), 1.0, 2.0),
            ((6.0, 8.0), None, None)
        );
    }

    #[test]
    fn physical_buffers_map_one_to_one() {
        // 125 x 30 px rastered at 1.25: 100 x 24 logical, drawn 125 x 30 px
        let (at, src, size) = hud_placement((10, 21), (125.0, 30.0), 1.25, 1.25);
        assert_eq!(at, (13.0, 26.0), "whole physical pixels");
        assert_eq!(size, Some((100, 24)));
        assert_eq!(src, Some((125.0, 30.0)));
        // sizes that do not divide: the source is exactly what is drawn
        for w in 90..140 {
            let (_, src, size) = hud_placement((0, 0), (w as f64, 17.0), 1.25, 1.25);
            let (sw, sh) = src.unwrap();
            let (lw, lh) = size.unwrap();
            assert_eq!(sw, (lw as f64 * 1.25).round(), "1:1 at {w}");
            assert_eq!(sh, (lh as f64 * 1.25).round());
            assert!(
                (sw - w as f64).abs() <= 1.0,
                "at most one px off the buffer: {w}"
            );
        }
    }
}
