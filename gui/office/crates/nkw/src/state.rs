//! The compositor state: Smithay's protocol states, the windows, the world
//! and the graphics. The shape follows Smithay's `smallvil` (MIT).

use std::collections::HashSet;
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

use nkw_core::{ScreenId, Time, Want, Wish};

use crate::cli::Cli;
use crate::frame::Gfx;
use crate::render::{self, Lod};
use crate::stats::{Stats, mono_s};
use crate::world::{Out, World};

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
