//! `zwp_pointer_constraints_v1` + `zwp_relative_pointer_v1` for the FOCO
//! client (games, 3D viewers, remote desktops): a lock activates only while
//! its surface has the keyboard in FOCO; then the host pointer is locked
//! (`set_pointer_lock(true)`, as the walk's mouse-look does) and every host
//! relative motion goes to the client instead of the walk's look. `ctrl+]`,
//! a focus change or leaving FOCO deactivates it. Nested, a *confinement*
//! behaves as a lock (the host pointer cannot be confined to a region of our
//! window from inside), documented.
//!
//! Headless there is no host pointer: when a surface takes the keyboard in
//! FOCO and the pointer is elsewhere, the pointer enters it at its centre
//! (so scripts can test enter-driven requests: cursor shapes, locks).

use smithay::backend::input::InputTime;
use smithay::input::pointer::{PointerHandle, RelativeMotionEvent};
use smithay::reexports::wayland_server::Resource;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{Logical, Point};
use smithay::wayland::compositor::get_parent;
use smithay::wayland::pointer_constraints::{PointerConstraintsHandler, with_pointer_constraint};
use smithay::wayland::seat::WaylandFocus;

use super::ClientLock;
use crate::frame::Host;
use crate::state::Nkw;

/// The root of a surface tree (subsurfaces → their toplevel's surface).
fn root_of(s: &WlSurface) -> WlSurface {
    let mut r = s.clone();
    while let Some(p) = get_parent(&r) {
        r = p;
    }
    r
}

impl PointerConstraintsHandler for Nkw {
    fn new_constraint(&mut self, surface: &WlSurface, pointer: &PointerHandle<Self>) {
        let focused = self
            .seat
            .get_keyboard()
            .and_then(|k| k.current_focus())
            .is_some_and(|f| f == root_of(surface));
        if focused && self.in_focus_mode() {
            self.activate_constraint(surface, pointer);
        }
    }

    fn cursor_position_hint(
        &mut self,
        surface: &WlSurface,
        _pointer: &PointerHandle<Self>,
        location: Point<f64, Logical>,
    ) {
        if self
            .protos
            .locked_by_client
            .as_ref()
            .is_some_and(|l| &l.surface == surface)
        {
            self.protos.cursor_hint = Some((location.x, location.y));
        }
    }
}

impl Nkw {
    /// FOCO: `s` took the keyboard. Headless, the pointer enters it; its
    /// pending constraint (if any) activates.
    pub(crate) fn focus_constraints(&mut self, s: &WlSurface) {
        let Some(ptr) = self.seat.get_pointer() else {
            return;
        };
        // (inside the keyboard's focus change: never ask the keyboard here)
        self.headless_pointer_follows(s);
        self.activate_constraint(s, &ptr);
    }

    /// Headless only (no host pointer): once FOCO has arrived on a window
    /// that has the keyboard and the pointer is elsewhere, the pointer enters
    /// it at its centre. Called on focus changes and after every frame (FOCO
    /// "arrives" after the camera's glide, frames later).
    pub(crate) fn headless_pointer_follows_focus(&mut self) {
        if !matches!(self.gfx.host, Host::Headless { .. }) || !self.in_focus_mode() {
            return;
        }
        if let Some(focus) = self.seat.get_keyboard().and_then(|k| k.current_focus()) {
            self.headless_pointer_follows(&focus);
        }
    }

    fn headless_pointer_follows(&mut self, focus: &WlSurface) {
        if !matches!(self.gfx.host, Host::Headless { .. }) || !self.in_focus_mode() {
            return;
        }
        let Some(ptr) = self.seat.get_pointer() else {
            return;
        };
        let on = ptr
            .current_focus()
            .and_then(|f| f.wl_surface().map(|w| root_of(&w)))
            .is_some_and(|r| &r == focus);
        if !on && let Some(c) = self.focus_centre() {
            self.pointer_motion(c, InputTime::now());
        }
    }

    /// The FOCO window's centre, logical (where a headless pointer enters).
    fn focus_centre(&self) -> Option<Point<f64, Logical>> {
        let (id, loc) = self.focus_location()?;
        let g = self.window(id)?.geometry();
        Some(loc + g.loc.to_f64() + Point::from((g.size.w as f64 / 2.0, g.size.h as f64 / 2.0)))
    }

    fn activate_constraint(&mut self, s: &WlSurface, ptr: &PointerHandle<Self>) {
        let active = with_pointer_constraint(s, ptr, |c| match c {
            Some(c) => {
                c.activate();
                true
            }
            None => false,
        });
        if !active {
            return;
        }
        self.protos.locked_by_client = Some(ClientLock {
            surface: s.clone(),
            screen: self.protos_screen_of(&root_of(s)),
        });
        self.protos.cursor_hint = None;
        self.set_pointer_lock(true);
    }

    /// Deactivate the client's lock (focus change, `ctrl+]`, leaving FOCO);
    /// the pointer goes where the client hinted; the host lock follows the
    /// walk's rule again.
    pub(crate) fn release_client_lock(&mut self) {
        let Some(l) = self.protos.locked_by_client.take() else {
            return;
        };
        if let Some(ptr) = self.seat.get_pointer() {
            with_pointer_constraint(&l.surface, &ptr, |c| {
                if let Some(c) = c {
                    c.deactivate();
                }
            });
        }
        if let (Some((x, y)), Some((_, loc))) =
            (self.protos.cursor_hint.take(), self.focus_location())
        {
            self.pointer = loc + Point::from((x, y));
        }
        self.set_pointer_lock(false);
        self.sync_pointer_lock();
    }

    /// A host relative motion while a FOCO client holds the lock: its
    /// `relative_motion` (true = consumed).
    pub(crate) fn client_relative_motion(
        &mut self,
        delta: (f64, f64),
        unaccel: (f64, f64),
        utime: u64,
    ) -> bool {
        let Some(l) = self.protos.locked_by_client.clone() else {
            return false;
        };
        if !l.surface.is_alive() || !self.in_focus_mode() {
            self.release_client_lock();
            return false;
        }
        let Some(ptr) = self.seat.get_pointer() else {
            return false;
        };
        let loc = self.focus_location().map(|(_, p)| p).unwrap_or_default();
        ptr.relative_motion(
            self,
            Some((l.surface.clone(), loc)),
            &RelativeMotionEvent {
                delta: delta.into(),
                delta_unaccel: unaccel.into(),
                time: InputTime::from_micros(utime),
            },
        );
        ptr.frame(self);
        true
    }
}
