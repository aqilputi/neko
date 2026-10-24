//! Protocol handlers (compositor, xdg-shell, decoration, seat, shm,
//! dmabuf, output, data device), after Smithay's `smallvil`/`anvil` (MIT).

use smithay::backend::allocator::dmabuf::Dmabuf;
use smithay::backend::renderer::ImportDma;
use smithay::backend::renderer::utils::on_commit_buffer_handler;
use smithay::desktop::{PopupKeyboardGrab, PopupKind, PopupPointerGrab, PopupUngrabStrategy, Window, find_popup_root_surface, get_popup_toplevel_coords};
use smithay::input::dnd::{DnDGrab, DndGrabHandler, GrabType, Source};
use smithay::input::pointer::{CursorImageStatus, Focus};
use smithay::input::{Seat, SeatHandler, SeatState};
use smithay::reexports::wayland_protocols::xdg::decoration::zv1::server::zxdg_toplevel_decoration_v1::Mode as DecorationMode;
use smithay::reexports::wayland_server::protocol::{wl_buffer, wl_seat, wl_surface::WlSurface};
use smithay::reexports::wayland_server::{Client, Resource};
use smithay::utils::{Rectangle, Serial};
use smithay::wayland::buffer::BufferHandler;
use smithay::wayland::compositor::{CompositorClientState, CompositorHandler, CompositorState, get_parent, is_sync_subsurface, with_states};
use smithay::wayland::dmabuf::{DmabufGlobal, DmabufHandler, DmabufState, ImportNotifier};
use smithay::wayland::output::OutputHandler;
use smithay::wayland::selection::SelectionHandler;
use smithay::wayland::selection::data_device::{DataDeviceHandler, DataDeviceState, WaylandDndGrabHandler, set_data_device_focus};
use smithay::wayland::shell::xdg::decoration::XdgDecorationHandler;
use smithay::wayland::shell::xdg::{PopupSurface, PositionerState, ToplevelSurface, XdgShellHandler, XdgShellState, XdgToplevelSurfaceData};
use smithay::wayland::shm::{ShmHandler, ShmState};

use std::os::fd::OwnedFd;

use nkw_core::ScreenId;
use smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel;
use smithay::reexports::wayland_server::protocol::wl_output::WlOutput;
use smithay::wayland::selection::{SelectionSource, SelectionTarget};
use smithay::wayland::shell::xdg::dialog::{ToplevelDialogHint, XdgDialogHandler};
use smithay::wayland::xdg_activation::{
    XdgActivationHandler, XdgActivationState, XdgActivationToken, XdgActivationTokenData,
};

use crate::state::{ClientState, Nkw};
use crate::world::Out;

impl CompositorHandler for Nkw {
    fn compositor_state(&mut self) -> &mut CompositorState {
        &mut self.compositor_state
    }

    fn client_compositor_state<'a>(&self, client: &'a Client) -> &'a CompositorClientState {
        &client.get_data::<ClientState>().unwrap().compositor_state
    }

    fn commit(&mut self, surface: &WlSurface) {
        on_commit_buffer_handler::<Self>(surface);
        let mut root = surface.clone();
        if !is_sync_subsurface(surface) {
            while let Some(parent) = get_parent(&root) {
                root = parent;
            }
            if let Some(id) = self.screen_of(&root) {
                let w = self.window(id).unwrap().clone();
                w.on_commit();
                if surface == &root {
                    self.content_type_committed(id, &root);
                    let (app_id, title) = toplevel_ids(&root);
                    self.world.set_app_id(id, &app_id, &title);
                    // its app_id is known: the rules, the restored places
                    // and the place memory say where it goes (before the
                    // first configure, so it gets its desk's size)
                    if !app_id.is_empty() && !self.world.screen_state(id).placed {
                        let now = self.now();
                        // `world.place_next` wins once, over rules and memory
                        let pid = self.pids.get(&id).copied();
                        let ours: Vec<u32> = self.children.iter().map(|c| c.id()).collect();
                        let intent = self.ext.wm.take_intent(pid, &|p| ours.contains(&p), now);
                        match intent.or_else(|| self.ext.wm.place_for(&app_id, now)) {
                            Some(p) => {
                                let outs = self.world.place_new(id, &p, now);
                                self.apply(outs);
                            }
                            None => self.world.mark_placed(id),
                        }
                    }
                    initial_configure(&w);
                    let g = w.geometry().size;
                    if g.w > 0 && g.h > 0 {
                        self.world
                            .set_size(id, glam::Vec2::new(g.w as f32, g.h as f32));
                    }
                }
                self.screen_committed(id);
            }
        }
        self.popups.commit(surface);
        if let Some(PopupKind::Xdg(xdg)) = self.popups.find_popup(surface) {
            if !xdg.is_initial_configure_sent() {
                let _ = xdg.send_configure();
            }
            if let Ok(root) = find_popup_root_surface(&PopupKind::Xdg(xdg))
                && let Some(id) = self.screen_of(&root)
            {
                self.screen_committed(id);
            }
        }
    }
}

/// A toplevel's (app_id, title) as last committed.
fn toplevel_ids(root: &WlSurface) -> (String, String) {
    with_states(root, |s| {
        s.data_map
            .get::<XdgToplevelSurfaceData>()
            .map(|d| {
                let d = d.lock().unwrap();
                (
                    d.app_id.clone().unwrap_or_default(),
                    d.title.clone().unwrap_or_default(),
                )
            })
            .unwrap_or_default()
    })
}

fn initial_configure(w: &Window) {
    let Some(t) = w.toplevel() else { return };
    if !t.is_initial_configure_sent() {
        t.send_configure();
    }
}

impl BufferHandler for Nkw {
    fn buffer_destroyed(&mut self, _buffer: &wl_buffer::WlBuffer) {}
}

impl ShmHandler for Nkw {
    fn shm_state(&self) -> &ShmState {
        &self.shm_state
    }
}

impl DmabufHandler for Nkw {
    fn dmabuf_state(&mut self) -> &mut DmabufState {
        &mut self.dmabuf.as_mut().expect("dmabuf global").0
    }

    fn dmabuf_imported(
        &mut self,
        _global: &DmabufGlobal,
        dmabuf: Dmabuf,
        notifier: ImportNotifier,
    ) {
        if self.gfx.renderer().import_dmabuf(&dmabuf, None).is_ok() {
            let _ = notifier.successful::<Nkw>();
        } else {
            notifier.failed();
        }
    }
}

impl XdgShellHandler for Nkw {
    fn xdg_shell_state(&mut self) -> &mut XdgShellState {
        &mut self.xdg_shell_state
    }

    fn new_toplevel(&mut self, surface: ToplevelSurface) {
        let now = self.now();
        let (id, size) = self.world.add_screen("", "", now);
        let pid = surface
            .wl_surface()
            .client()
            .and_then(|c| c.get_credentials(&self.dh).ok())
            .and_then(|c| u32::try_from(c.pid).ok());
        if let Some(pid) = pid {
            self.pids.insert(id, pid);
        }
        self.configure_new(&surface, size);
        self.windows.push((id, Window::new_wayland_window(surface)));
        self.wake();
    }

    fn toplevel_destroyed(&mut self, surface: ToplevelSurface) {
        let Some(id) = self.screen_of(surface.wl_surface()) else {
            return;
        };
        let now = self.now();
        let outs = self.world.remove_screen(id, now);
        self.apply(outs);
        self.content_type_forget(id);
        if self
            .protos
            .locked_by_client
            .as_ref()
            .is_some_and(|l| l.screen == Some(id))
        {
            self.release_client_lock();
        }
        self.windows.retain(|(i, _)| *i != id);
        self.pids.remove(&id);
        self.wake();
    }

    fn fullscreen_request(&mut self, surface: ToplevelSurface, _output: Option<WlOutput>) {
        self.wm_toplevel(&surface, |w, id, now| w.set_fullscreen(id, true, now));
    }

    fn unfullscreen_request(&mut self, surface: ToplevelSurface) {
        self.wm_toplevel(&surface, |w, id, now| w.set_fullscreen(id, false, now));
    }

    fn maximize_request(&mut self, surface: ToplevelSurface) {
        self.wm_toplevel(&surface, |w, id, now| w.set_maximized(id, true, now));
    }

    fn unmaximize_request(&mut self, surface: ToplevelSurface) {
        self.wm_toplevel(&surface, |w, id, now| w.set_maximized(id, false, now));
    }

    fn parent_changed(&mut self, surface: ToplevelSurface) {
        let parent = surface.parent().and_then(|p| self.screen_of(&p));
        if parent.is_some() {
            // a dialog picks its own size; it is not tiled
            surface.with_pending_state(|s| {
                for st in [
                    xdg_toplevel::State::TiledLeft,
                    xdg_toplevel::State::TiledRight,
                    xdg_toplevel::State::TiledTop,
                    xdg_toplevel::State::TiledBottom,
                ] {
                    s.states.unset(st);
                }
            });
        }
        self.wm_toplevel(&surface, |w, id, now| w.set_parent(id, parent, now));
    }

    fn app_id_changed(&mut self, surface: ToplevelSurface) {
        self.ids_changed(&surface);
    }

    fn title_changed(&mut self, surface: ToplevelSurface) {
        self.ids_changed(&surface);
    }

    fn new_popup(&mut self, surface: PopupSurface, _positioner: PositionerState) {
        self.unconstrain_popup(&surface);
        let _ = self.popups.track_popup(PopupKind::Xdg(surface));
    }

    fn reposition_request(
        &mut self,
        surface: PopupSurface,
        positioner: PositionerState,
        token: u32,
    ) {
        surface.with_pending_state(|state| {
            state.geometry = positioner.get_geometry();
            state.positioner = positioner;
        });
        self.unconstrain_popup(&surface);
        surface.send_repositioned(token);
    }

    fn grab(&mut self, surface: PopupSurface, seat: wl_seat::WlSeat, serial: Serial) {
        // popups only live in FOCO (§4.6)
        if self.world.focus_2d().is_none() {
            surface.send_popup_done();
            return;
        }
        let seat: Seat<Nkw> = Seat::from_resource(&seat).unwrap();
        let kind = PopupKind::Xdg(surface);
        let Ok(root) = find_popup_root_surface(&kind) else {
            return;
        };
        let Ok(mut grab) = self.popups.grab_popup(root, kind, &seat, serial) else {
            return;
        };
        if let Some(keyboard) = seat.get_keyboard() {
            if keyboard.is_grabbed()
                && !(keyboard.has_grab(serial)
                    || keyboard.has_grab(grab.previous_serial().unwrap_or(serial)))
            {
                grab.ungrab(PopupUngrabStrategy::All);
                return;
            }
            keyboard.set_focus(self, grab.current_grab(), serial);
            keyboard.set_grab(self, PopupKeyboardGrab::new(&grab), serial);
        }
        if let Some(pointer) = seat.get_pointer() {
            if pointer.is_grabbed()
                && !(pointer.has_grab(serial)
                    || pointer.has_grab(grab.previous_serial().unwrap_or_else(|| grab.serial())))
            {
                grab.ungrab(PopupUngrabStrategy::All);
                return;
            }
            pointer.set_grab(self, PopupPointerGrab::new(&grab), serial, Focus::Keep);
        }
    }
}

impl Nkw {
    /// A WM change of a toplevel: the World's outs applied, a frame.
    fn wm_toplevel(
        &mut self,
        surface: &ToplevelSurface,
        f: impl FnOnce(&mut crate::world::World, ScreenId, nkw_core::Time) -> Vec<Out>,
    ) {
        let Some(id) = self.screen_of(surface.wl_surface()) else {
            return;
        };
        let now = self.now();
        let outs = f(&mut self.world, id, now);
        self.apply(outs);
        self.wake();
    }

    /// The title or app_id changed (the selected screen's tag shows it).
    fn ids_changed(&mut self, surface: &ToplevelSurface) {
        let Some(id) = self.screen_of(surface.wl_surface()) else {
            return;
        };
        let (app_id, title) = toplevel_ids(surface.wl_surface());
        self.world.set_app_id(id, &app_id, &title);
        self.world
            .want(nkw_core::Wish::compose(nkw_core::Want::Once));
        self.wake();
    }

    /// Keep a popup inside the output, relative to its window as composed.
    fn unconstrain_popup(&self, popup: &PopupSurface) {
        let Ok(root) = find_popup_root_surface(&PopupKind::Xdg(popup.clone())) else {
            return;
        };
        let Some(id) = self.screen_of(&root) else {
            return;
        };
        let Some(w) = self.window(id) else { return };
        let s = self.world.out.scale as f64;
        let out_size = (
            (self.world.out.width_px as f64 / s) as i32,
            (self.world.out.height_px as f64 / s) as i32,
        );
        let win_loc = self
            .focus_location()
            .filter(|(f, _)| *f == id)
            .map(|(_, l)| l.to_i32_round())
            .unwrap_or_default()
            + w.geometry().loc;
        let mut target = Rectangle::new((0, 0).into(), out_size.into());
        target.loc -= get_popup_toplevel_coords(&PopupKind::Xdg(popup.clone()));
        target.loc -= win_loc;
        popup.with_pending_state(|state| {
            state.geometry = state.positioner.get_unconstrained_geometry(target);
        });
    }
}

impl XdgDecorationHandler for Nkw {
    // server side always: the "decoration" is the monitor's frame (§4.6)
    fn new_decoration(&mut self, toplevel: ToplevelSurface) {
        toplevel.with_pending_state(|s| s.decoration_mode = Some(DecorationMode::ServerSide));
    }

    fn request_mode(&mut self, toplevel: ToplevelSurface, _mode: DecorationMode) {
        toplevel.with_pending_state(|s| s.decoration_mode = Some(DecorationMode::ServerSide));
        if toplevel.is_initial_configure_sent() {
            toplevel.send_pending_configure();
        }
    }

    fn unset_mode(&mut self, toplevel: ToplevelSurface) {
        self.request_mode(toplevel, DecorationMode::ServerSide);
    }
}

impl SeatHandler for Nkw {
    type KeyboardFocus = WlSurface;
    type PointerFocus = WlSurface;
    type TouchFocus = WlSurface;

    fn seat_state(&mut self) -> &mut SeatState<Nkw> {
        &mut self.seat_state
    }

    // the cursor is the host's (§0); in FOCO it takes the focused app's
    // named shape (S23, `protocols::cursor`); custom cursor surfaces: phase 6+
    fn cursor_image(&mut self, _seat: &Seat<Self>, image: CursorImageStatus) {
        self.client_cursor(image);
    }

    fn focus_changed(&mut self, seat: &Seat<Self>, focused: Option<&WlSurface>) {
        let dh = &self.dh;
        let client = focused.and_then(|s| dh.get_client(s.id()).ok());
        set_data_device_focus(dh, seat, client);
        // primary selection, the FOCO client's pointer lock, the host cursor
        self.protos_focus_changed(focused);
    }
}

impl SelectionHandler for Nkw {
    type SelectionUserData = ();

    /// A client of ours set the clipboard: the host gets it too (mimes
    /// only; the bytes go fd to fd when someone pastes).
    fn new_selection(
        &mut self,
        ty: SelectionTarget,
        source: Option<SelectionSource>,
        _seat: Seat<Self>,
    ) {
        if ty == SelectionTarget::Primary {
            // in-world only (S23): nothing goes to the host
            self.protos.has_primary = source.is_some();
            return;
        }
        if let Some(src) = source {
            self.ext.wm.our_selection(src.mime_types());
        }
    }

    /// A client of ours pastes the selection we mirror from the host.
    fn send_selection(
        &mut self,
        ty: SelectionTarget,
        mime_type: String,
        fd: OwnedFd,
        _seat: Seat<Self>,
        _user_data: &(),
    ) {
        if ty == SelectionTarget::Clipboard
            && let Err(fd) = self.ext.wm.send_copied(&mime_type, fd)
        {
            self.ext.wm.host_receive(mime_type, fd);
        }
    }
}

impl XdgActivationHandler for Nkw {
    fn activation_state(&mut self) -> &mut XdgActivationState {
        self.ext
            .wm
            .activation
            .as_mut()
            .expect("the activation global exists")
    }

    /// A valid recent token selects the screen's station (NAV) or badges;
    /// none (or a stale one): "quer atenção" on the screen for 6 s.
    fn request_activation(
        &mut self,
        token: XdgActivationToken,
        token_data: XdgActivationTokenData,
        surface: WlSurface,
    ) {
        let valid = token_data.timestamp.elapsed() < crate::wm::TOKEN_MAX_AGE;
        if let Some(a) = self.ext.wm.activation.as_mut() {
            a.remove_token(&token);
        }
        let Some(id) = self.screen_of(&surface) else {
            return;
        };
        let now = self.now();
        let outs = self.world.activate(id, valid, now);
        self.apply(outs);
        self.wake();
    }
}

impl XdgDialogHandler for Nkw {
    fn dialog_hint_changed(&mut self, toplevel: ToplevelSurface, hint: ToplevelDialogHint) {
        let modal = hint == ToplevelDialogHint::Modal;
        self.wm_toplevel(&toplevel, |w, id, now| w.set_modal(id, modal, now));
    }
}

impl DataDeviceHandler for Nkw {
    fn data_device_state(&mut self) -> &mut DataDeviceState {
        &mut self.data_device_state
    }
}

impl DndGrabHandler for Nkw {}

impl WaylandDndGrabHandler for Nkw {
    fn dnd_requested<S: Source>(
        &mut self,
        source: S,
        _icon: Option<WlSurface>,
        seat: Seat<Self>,
        serial: Serial,
        type_: GrabType,
    ) {
        match type_ {
            GrabType::Pointer => {
                let ptr = seat.get_pointer().unwrap();
                let Some(start) = ptr.grab_start_data() else {
                    source.cancel();
                    return;
                };
                let grab = DnDGrab::new_pointer(&self.dh, start, source, seat);
                ptr.set_grab(self, grab, serial, Focus::Keep);
            }
            GrabType::Touch => source.cancel(),
        }
    }
}

impl OutputHandler for Nkw {}

smithay::delegate_dispatch2!(Nkw);
