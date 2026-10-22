//! `wp_fractional_scale_v1`: every surface prefers the output's scale, so a
//! client that speaks it (with `wp_viewporter`) renders its buffer at the
//! output's physical size and FOCO shows it 1:1 (plan §5.5).

use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::wayland::compositor::{TraversalAction, with_states, with_surface_tree_downward};
use smithay::wayland::fractional_scale::{FractionalScaleHandler, with_fractional_scale};

use crate::state::Nkw;

impl FractionalScaleHandler for Nkw {
    fn new_fractional_scale(&mut self, surface: WlSurface) {
        let scale = self.world.out.scale as f64;
        with_states(&surface, |s| {
            with_fractional_scale(s, |fs| fs.set_preferred_scale(scale))
        });
    }
}

impl Nkw {
    /// The output scale changed: tell every surface of every window (popups
    /// and subsurfaces included). Smithay sends the event only on change.
    pub fn broadcast_scale(&mut self) {
        let scale = self.world.out.scale as f64;
        let mut roots: Vec<WlSurface> = self
            .windows
            .iter()
            .filter_map(|(_, w)| w.toplevel().map(|t| t.wl_surface().clone()))
            .collect();
        roots.extend(self.popups_roots());
        for root in roots {
            with_surface_tree_downward(
                &root,
                (),
                |_, _, _| TraversalAction::DoChildren(()),
                |_, s, _| with_fractional_scale(s, |fs| fs.set_preferred_scale(scale)),
                |_, _, _| true,
            );
        }
    }

    fn popups_roots(&self) -> Vec<WlSurface> {
        let mut v = Vec::new();
        for (_, w) in &self.windows {
            if let Some(t) = w.toplevel() {
                for (p, _) in smithay::desktop::PopupManager::popups_for_surface(t.wl_surface()) {
                    v.push(p.wl_surface().clone());
                }
            }
        }
        v
    }
}
