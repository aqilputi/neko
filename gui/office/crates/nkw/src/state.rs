//! The compositor state: Smithay's protocol states, the windows, the world
//! and the graphics. The shape follows Smithay's `smallvil` (MIT).

use std::collections::{BTreeMap, HashSet};
use std::ffi::OsString;
use std::sync::Arc;
use std::time::{Duration, Instant};

use calloop::{LoopHandle, LoopSignal, RegistrationToken};
use smithay::desktop::{PopupKind, PopupManager, Window, WindowSurfaceType};
use smithay::input::keyboard::Keycode;
use smithay::input::{Seat, SeatState};
use smithay::output::Output;
use smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel;
use smithay::reexports::wayland_server::DisplayHandle;
use smithay::reexports::wayland_server::backend::{ClientData, ClientId, DisconnectReason};
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{Logical, Point, SERIAL_COUNTER};
use smithay::wayland::compositor::{CompositorClientState, CompositorState};
use smithay::wayland::dmabuf::{DmabufGlobal, DmabufState};
use smithay::wayland::output::OutputManagerState;
use smithay::wayland::selection::data_device::DataDeviceState;
use smithay::wayland::shell::xdg::XdgShellState;
use smithay::wayland::shell::xdg::decoration::XdgDecorationState;
use smithay::wayland::shm::ShmState;

use nkw_core::{Effect, RoomId, ScreenId, Time, Want, Wish};
use serde_json::Value;

use crate::cli::Cli;
use crate::ext::{self, EffectTo, Ext, Plugin, Req};
use crate::frame::Gfx;
use crate::render::{self, Lod};
use crate::stats::{Stats, mono_s};
use crate::world::{Cmd, Out, World};

/// A `Ctx` for a hook call, borrowing the hub's fields (so `self.ext` stays
/// free to be borrowed mutably next to it).
#[macro_export]
#[doc(hidden)]
macro_rules! hook_ctx {
    ($st:expr, $now:expr) => {
        $crate::hook_ctx!($st, $now, None)
    };
    ($st:expr, $now:expr, $line:expr) => {
        $crate::ext::Ctx {
            world: &$st.world,
            now: $now,
            frame: $st.stats.frames,
            reduced_motion: $st.cli.reduced_motion,
            demo: $st.cli.demo,
            scale: $st.world.out.scale as f64,
            size_px: [$st.world.out.width_px, $st.world.out.height_px],
            pids: &$st.pids,
            visible_rooms: &$st.visible,
            line: $line,
        }
    };
}

/// Per-client data.
#[derive(Default)]
pub struct ClientState {
    pub compositor_state: CompositorClientState,
}

impl ClientData for ClientState {
    fn initialized(&self, _client_id: ClientId) {}
    fn disconnected(&self, _client_id: ClientId, _reason: DisconnectReason) {}
}

/// Frame scheduling state (§5.6): who is waiting for which wakeup.
#[derive(Debug, Default)]
pub struct Sched {
    /// a host frame was requested (winit) or a vblank is armed (headless)
    pub frame_requested: bool,
    /// the calloop timer of a future wakeup, and its instant
    pub timer: Option<(RegistrationToken, Time)>,
    /// the vblank grid's epoch (first frame) and period
    pub epoch: Option<Time>,
    pub period: f64,
    /// the last world pass
    pub last_world: Time,
    /// the next frame must recompose everything (resize, expose)
    pub full: bool,
}

pub struct Nkw {
    pub cli: Cli,
    pub dh: DisplayHandle,
    pub lh: LoopHandle<'static, Nkw>,
    pub signal: LoopSignal,
    pub socket: OsString,
    /// base of the `time` sent with frame callbacks
    pub clock0: Instant,

    pub compositor_state: CompositorState,
    pub xdg_shell_state: XdgShellState,
    pub shm_state: ShmState,
    pub output_manager_state: OutputManagerState,
    pub seat_state: SeatState<Nkw>,
    pub data_device_state: DataDeviceState,
    pub decoration_state: XdgDecorationState,
    /// mpv and other GL clients need `wp_viewporter`
    pub viewporter_state: smithay::wayland::viewporter::ViewporterState,
    /// `wp_fractional_scale_v1` (see `backend::fractional`)
    pub fractional_state: smithay::wayland::fractional_scale::FractionalScaleManagerState,
    pub dmabuf: Option<(DmabufState, DmabufGlobal)>,
    pub popups: PopupManager,
    pub seat: Seat<Nkw>,

    pub windows: Vec<(ScreenId, Window)>,
    pub world: World,
    pub output: Output,
    pub gfx: Gfx,
    pub sched: Sched,
    /// key presses the world took; their releases are the world's too
    pub intercepted: HashSet<Keycode>,
    pub stats: Stats,
    /// set when the process should exit with this code
    pub exit: Option<i32>,
    /// screenshots to take on the next composed frame
    pub shots: Vec<std::path::PathBuf>,
    /// children spawned with the world's display
    pub children: Vec<std::process::Child>,
    pub pointer: Point<f64, Logical>,
    pub script: Option<crate::app::Script>,
    /// the keysym of the key being handled (the filter sees it)
    pub last_sym: u32,
    /// the plugins and the router (S7)
    pub ext: Ext,
    /// client pids, from Smithay credentials
    pub pids: BTreeMap<ScreenId, u32>,
    /// the rooms the last world pass drew (current + what plugins opened)
    pub visible: Vec<RoomId>,
    /// ANDAR's mouse-look holds the host pointer (headless: recorded only)
    pub pointer_locked: bool,
    /// the hash of the last composed HUD (`ext::hud_hash`): a different
    /// one composes even with no wish (S14)
    pub hud_hash: Option<u64>,
    /// the last composed HUD had a dialog, badges or labels
    pub hud_busy: bool,
    /// the extra protocols (S23; W4's module)
    pub protos: crate::protocols::Protocols,
    /// what the host window's cursor shows (S20)
    pub host_cursor: crate::frame::HostCursor,
    /// the mode was FOCO at the end of the last `apply` (to call
    /// `protos_mode_left_focus` once when it leaves)
    pub was_focus: bool,
    /// some client inhibits idle (S23): told to the objects on a change (W6)
    pub idle_inhibit: bool,
}

impl Nkw {
    pub fn now(&self) -> Time {
        mono_s()
    }

    pub fn window(&self, id: ScreenId) -> Option<&Window> {
        self.windows.iter().find(|(i, _)| *i == id).map(|(_, w)| w)
    }

    pub fn screen_of(&self, surface: &WlSurface) -> Option<ScreenId> {
        self.windows
            .iter()
            .find(|(_, w)| w.toplevel().is_some_and(|t| t.wl_surface() == surface))
            .map(|(i, _)| *i)
    }

    /// The focus window's location (buffer origin) in logical px, when composed 2D.
    pub fn focus_location(&self) -> Option<(ScreenId, Point<f64, Logical>)> {
        let (id, o) = self.world.focus_2d()?;
        let w = self.window(id)?;
        let s = self.world.out.scale as f64;
        let origin = Point::from((o.origin.x as f64 / s, o.origin.y as f64 / s));
        Some((id, origin - w.geometry().loc.to_f64()))
    }

    /// The surface under `pos` (FOCO only: the world itself takes no pointer).
    pub fn surface_under(
        &self,
        pos: Point<f64, Logical>,
    ) -> Option<(WlSurface, Point<f64, Logical>)> {
        let (id, loc) = self.focus_location()?;
        let w = self.window(id)?;
        w.surface_under(pos - loc, WindowSurfaceType::ALL)
            .map(|(s, p)| (s, p.to_f64() + loc))
    }

    /// Apply what the world asked for.
    pub fn apply(&mut self, outs: Vec<Out>) {
        let kb = self.seat.get_keyboard().unwrap();
        for o in outs {
            let serial = SERIAL_COUNTER.next_serial();
            match o {
                Out::Effect(e) => self.route_effect(e),
                Out::Object(id, a) => {
                    let now = self.now();
                    let reqs = {
                        let ctx = crate::hook_ctx!(self, now);
                        self.ext.objects.on_object(id, a, &ctx)
                    };
                    self.apply_reqs(reqs);
                }
                Out::Configure {
                    screen,
                    size,
                    fullscreen,
                    maximized,
                } => self.configure(screen, size, fullscreen, maximized),
                Out::Close(s) => self.close(s),
                Out::Spawn(argv) => self.spawn(&argv),
                Out::KeyboardEnter(id) => {
                    let Some(w) = self.window(id).cloned() else {
                        continue;
                    };
                    let Some(t) = w.toplevel() else { continue };
                    kb.set_focus(self, Some(t.wl_surface().clone()), serial);
                    w.set_activated(true);
                    t.send_pending_configure();
                }
                Out::KeyboardLeave => {
                    if kb.is_grabbed() {
                        kb.unset_grab(self);
                    }
                    kb.set_focus(self, None, serial);
                    for (_, w) in &self.windows {
                        w.set_activated(false);
                        if let Some(t) = w.toplevel() {
                            t.send_pending_configure();
                        }
                    }
                }
                Out::PopupsDone => {
                    if let Some(p) = self.seat.get_pointer()
                        && p.is_grabbed()
                    {
                        p.unset_grab(self, serial, smithay::backend::input::InputTime::now());
                    }
                    for (_, w) in &self.windows {
                        let Some(t) = w.toplevel() else { continue };
                        let popups: Vec<_> = PopupManager::popups_for_surface(t.wl_surface())
                            .map(|(p, _)| p)
                            .collect();
                        for p in popups.into_iter().rev() {
                            if let PopupKind::Xdg(x) = p {
                                x.send_popup_done();
                            }
                        }
                    }
                }
            }
        }
        self.sync_pointer_lock();
        let focus = self.in_focus_mode();
        if self.was_focus && !focus {
            self.protos_mode_left_focus();
        }
        self.was_focus = focus;
    }

    /// The machine is in FOCO (a window or a cat's terminal has the keys).
    pub fn in_focus_mode(&self) -> bool {
        matches!(self.world.mode().kind(), Some(nkw_core::Kind::Focus(_)))
    }

    /// Lock the host pointer while walking with mouse-look, release it
    /// otherwise (S14: on `WalkStart`/`WalkStop`, whatever ended the walk).
    pub fn sync_pointer_lock(&mut self) {
        let walk = self.world.walking.is_some()
            && self.world.focus_2d().is_none()
            && self.world.walk_config().mouse_look;
        // a FOCO client's lock (S23) holds the host pointer too
        self.set_pointer_lock(walk || self.protos.client_locked());
    }

    /// `Host::Winit`: `CursorGrabMode::Locked` (falling back to `Confined`)
    /// and a hidden cursor; released and visible again. Headless: only the
    /// flag (`dump()["host"]["pointer_locked"]`).
    pub fn set_pointer_lock(&mut self, on: bool) {
        if self.pointer_locked == on {
            return;
        }
        self.pointer_locked = on;
        if let crate::frame::Host::Winit(b) = &self.gfx.host {
            use smithay::reexports::winit::window::CursorGrabMode;
            let w = b.window();
            if on {
                if let Err(e) = w
                    .set_cursor_grab(CursorGrabMode::Locked)
                    .or_else(|_| w.set_cursor_grab(CursorGrabMode::Confined))
                {
                    tracing::warn!("pointer lock: {e}");
                }
            } else {
                let _ = w.set_cursor_grab(CursorGrabMode::None);
            }
            w.set_cursor_visible(!on);
        }
    }

    /// An effect the World passed through, to its plugin (S7 table).
    pub fn route_effect(&mut self, e: Effect) {
        let now = self.now();
        let line = match e {
            Effect::SendLine => self.ext.overlay.take_line(),
            _ => None,
        };
        let to = ext::route_effect(&e, self.world.purpose());
        if matches!(e, Effect::OpenPalette) {
            // every plugin's items (wm, objects, cats, overlay), before the
            // overlay opens COMANDOS
            let items = {
                let ctx = crate::hook_ctx!(self, now);
                self.ext
                    .plugins_ref()
                    .iter()
                    .flat_map(|p| p.palette_items(&ctx))
                    .collect()
            };
            self.ext.overlay.set_palette_items(items);
        }
        let reqs = {
            let ctx = crate::hook_ctx!(self, now, line.as_deref());
            match to {
                EffectTo::Overlay => self.ext.overlay.effect(&e, &ctx),
                EffectTo::Cats => self.ext.cats.effect(&e, &ctx),
                EffectTo::Objects => self.ext.objects.effect(&e, &ctx),
                EffectTo::Wm => self.ext.wm.effect(&e, &ctx),
            }
        };
        self.apply_reqs(reqs);
    }

    /// Apply what a plugin asked for, in order.
    pub fn apply_reqs(&mut self, reqs: Vec<Req>) {
        for r in reqs {
            let now = self.now();
            match r {
                Req::Action(a) => {
                    let outs = self.world.action(a, now);
                    self.apply(outs);
                }
                Req::Cmd(c) => {
                    let outs = self.world.command(c, now);
                    self.apply(outs);
                }
                Req::Command { name, args } => self.dispatch(&name, &args),
                Req::Wish(w) => self.world.want(w),
                Req::GlideTo(p) => self.world.glide_free(p, now),
                Req::Spawn(argv) => self.spawn(&argv),
                Req::SpawnWith { argv, env } => self.spawn_with(&argv, &env),
                Req::Configure {
                    screen,
                    size,
                    fullscreen,
                    maximized,
                } => self.configure(screen, size, fullscreen, maximized),
                Req::Close(s) => self.close(s),
                Req::Toast { text, kind, ms } => self.ext.overlay.toast(&text, kind, ms, now),
                Req::Cue { cue, at } => {
                    let ctx = crate::hook_ctx!(self, now);
                    self.ext.objects.cue(cue, at, &ctx);
                }
                Req::Keymap(b) => {
                    self.ext.router.set_bindings(b);
                    // Ajuda and the phone's printable keys follow rebinds (W2)
                    let binds = self.ext.router.binds().to_vec();
                    self.ext.overlay.set_help(binds);
                }
                Req::Quit(code) => self.quit(code),
                Req::SyncContexts(c) => {
                    let outs = self.world.sync_contexts(&c, now);
                    self.apply(outs);
                }
            }
        }
    }

    /// A command by name (router, palette, plugins): the World's when
    /// `Cmd::from_name` knows it, else the first plugin that takes it.
    pub fn dispatch(&mut self, name: &str, args: &BTreeMap<String, String>) {
        if let Some(c) = Cmd::from_name(name, args) {
            self.run_cmd(c);
            return;
        }
        let now = self.now();
        let reqs = {
            let ctx = crate::hook_ctx!(self, now);
            let e = &mut self.ext;
            let mut ps: [&mut dyn Plugin; 4] =
                [&mut e.overlay, &mut e.cats, &mut e.objects, &mut e.wm];
            ext::command(&mut ps, name, args, &ctx)
        };
        match reqs {
            Some(r) => self.apply_reqs(r),
            None => tracing::warn!("command {name}: nobody takes it"),
        }
        self.wake();
    }

    /// The plugins' wakeup: drain, tick, their obstacles into the World.
    pub fn ext_step(&mut self, now: Time) {
        // `NKW_STATS=ext`: each plugin's drain + tick (its own hook time,
        // not what its requests then cost the hub)
        let mut us = [0u64; 4];
        for (i, u) in us.iter_mut().enumerate() {
            let t = Instant::now();
            let reqs = {
                let ctx = crate::hook_ctx!(self, now);
                self.ext.plugins()[i].drain(&ctx)
            };
            *u += t.elapsed().as_micros() as u64;
            self.apply_reqs(reqs);
        }
        for (i, u) in us.iter_mut().enumerate() {
            let t = Instant::now();
            let reqs = {
                let ctx = crate::hook_ctx!(self, now);
                self.ext.plugins()[i].tick(&ctx)
            };
            *u += t.elapsed().as_micros() as u64;
            self.apply_reqs(reqs);
        }
        if self.stats.ext_detail {
            for (i, u) in us.into_iter().enumerate() {
                self.stats.ext_plugin(i, u, now);
            }
        }
        let room = self.world.current_room_id();
        let extra = {
            let ctx = crate::hook_ctx!(self, now);
            ext::obstacles(&self.ext.plugins_ref(), room, &ctx)
        };
        self.world.set_obstacles(room, extra);
        let inhibit = self.protos.idle_inhibited();
        if inhibit != self.idle_inhibit {
            self.idle_inhibit = inhibit;
            self.ext.objects.set_idle_inhibit(inhibit);
        }
    }

    /// A plugin thread pinged the loop.
    pub fn ext_wake(&mut self) {
        let now = self.now();
        self.ext_step(now);
        self.world.tick(now);
        self.wake();
    }

    /// Every wish of this frame: the World's and the plugins'.
    pub fn all_wishes(&self) -> Vec<Wish> {
        let mut w = self.world.wishes();
        let ctx = crate::hook_ctx!(self, self.now());
        for p in self.ext.plugins_ref() {
            w.extend(p.wishes(&ctx));
        }
        w
    }

    /// `dump`: the World's JSON and `"ext"` (S12).
    pub fn dump(&self) -> Value {
        let mut v = self.world.dump(&self.all_wishes());
        v["ext"] = self.ext.dump();
        let status = {
            let ctx = crate::hook_ctx!(self, self.now());
            self.ext
                .plugins_ref()
                .iter()
                .fold(ext::Status::default(), |a, p| a.merge(p.status(&ctx)))
        };
        v["status"] = serde_json::to_value(&status).unwrap_or(Value::Null);
        let protocols = self.protos.dump();
        // a client's lock (S23, W4's `locked_by`) or the walk's
        let by_client = !protocols["locked_by"].is_null();
        v["protocols"] = protocols;
        self.protos.annotate_screens(&mut v);
        v["host"] = serde_json::json!({
            "cursor": self.host_cursor.name(),
            "pointer_locked_by": if by_client {
                Value::from("client")
            } else if self.pointer_locked {
                Value::from("walk")
            } else {
                Value::Null
            },
            "pointer_locked": self.pointer_locked,
            "compose_hud_changes": self.stats.compose_hud_changes,
            "look_events": self.stats.look_events,
            "hud": self.gfx.hud_placed,
        });
        v
    }

    /// Configure a toplevel (size, fullscreen, maximized) and send it.
    pub fn configure(
        &mut self,
        screen: ScreenId,
        size: Option<glam::Vec2>,
        fullscreen: Option<bool>,
        maximized: Option<bool>,
    ) {
        let Some(t) = self.window(screen).and_then(|w| w.toplevel().cloned()) else {
            return;
        };
        t.with_pending_state(|s| {
            if let Some(v) = size {
                s.size = Some((v.x as i32, v.y as i32).into());
            }
            for (on, st) in [
                (fullscreen, xdg_toplevel::State::Fullscreen),
                (maximized, xdg_toplevel::State::Maximized),
            ] {
                match on {
                    Some(true) => {
                        s.states.set(st);
                    }
                    Some(false) => {
                        s.states.unset(st);
                    }
                    None => {}
                }
            }
        });
        if t.is_initial_configure_sent() {
            t.send_pending_configure();
        }
    }

    /// Ask a window to close (`xdg_toplevel.close`; never kills).
    pub fn close(&mut self, screen: ScreenId) {
        if let Some(t) = self.window(screen).and_then(|w| w.toplevel()) {
            t.send_close();
        }
    }

    /// A window committed: the wish its LOD asks for (§5.3).
    pub fn screen_committed(&mut self, id: ScreenId) {
        let now = self.now();
        let lod = render::lod(&self.world, id);
        let wish = match (lod, render::world_rate(lod)) {
            (Lod::Focus, _) => Some(Wish::compose(Want::Once)),
            (_, Some(hz)) if hz > 0.0 => {
                let t = self.sched.last_world + 1.0 / hz;
                Some(Wish::world(if t <= now { Want::Once } else { Want::At(t) }))
            }
            // frozen or out of view: no redraw, but the frame callback is
            // still due within the 1 s throttle
            _ => Some(Wish::compose(Want::At(
                now + render::throttle(Lod::Hidden).as_secs_f64(),
            ))),
        };
        if let Some(w) = wish {
            self.world.want(w);
        }
        self.wake();
    }

    /// Configure a new toplevel to the size of its desk.
    pub fn configure_new(
        &self,
        t: &smithay::wayland::shell::xdg::ToplevelSurface,
        size: glam::Vec2,
    ) {
        t.with_pending_state(|s| {
            s.size = Some((size.x as i32, size.y as i32).into());
            for st in [
                xdg_toplevel::State::TiledLeft,
                xdg_toplevel::State::TiledRight,
                xdg_toplevel::State::TiledTop,
                xdg_toplevel::State::TiledBottom,
            ] {
                s.states.set(st);
            }
        });
    }

    /// Frame callbacks with the throttle of each window's LOD.
    pub fn send_frames(&mut self) {
        let t = self.clock0.elapsed();
        for (id, w) in &self.windows {
            let throttle = render::throttle(render::lod(&self.world, *id));
            // never "the primary output": the throttle alone decides (§5.3)
            w.send_frame(&self.output, t, Some(throttle), |_, _| None);
        }
    }

    pub fn quit(&mut self, code: i32) {
        if self.exit.is_none() {
            self.exit = Some(code);
        }
        self.signal.stop();
        self.signal.wakeup();
    }

    pub fn period(&self) -> Duration {
        Duration::from_secs_f64(self.sched.period)
    }
}

/// Keep `Arc` in scope for the client data insertion sites.
pub fn client_data() -> Arc<ClientState> {
    Arc::new(ClientState::default())
}
