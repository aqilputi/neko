//! `zwp_idle_inhibit_manager_v1`: recorded, not forwarded. A video in FOCO
//! asks the screen to stay awake; nested, the host's own idle logic does not
//! see our clients. `Protocols::idle_inhibited()` is what the hub would hand
//! to `nkw_sys::idle::Inhibitor` (`org.freedesktop.portal.Inhibit`) once W6's
//! stretch lands: best-effort and documented, nothing is sent this wave.

use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::wayland::idle_inhibit::IdleInhibitHandler;

use crate::state::Nkw;

impl IdleInhibitHandler for Nkw {
    fn inhibit(&mut self, surface: WlSurface) {
        self.protos.inhibitors.insert(surface);
    }

    fn uninhibit(&mut self, surface: WlSurface) {
        self.protos.inhibitors.remove(&surface);
    }
}
