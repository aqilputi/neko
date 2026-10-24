//! `zwp_primary_selection_v1` (middle-click paste) between the world's
//! clients: the selection follows the keyboard focus like the clipboard. No
//! host bridge (the audit found nobody needs one for in-world paste; the
//! clipboard keeps its own, in the WM).

use smithay::reexports::wayland_server::Resource;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::wayland::selection::primary_selection::{
    PrimarySelectionHandler, PrimarySelectionState, set_primary_focus,
};

use crate::state::Nkw;

impl PrimarySelectionHandler for Nkw {
    fn primary_selection_state(&mut self) -> &mut PrimarySelectionState {
        &mut self.protos.primary
    }
}

/// The keyboard focus moved: the focused client may read and set the primary
/// selection.
pub(super) fn focus(st: &mut Nkw, focus: Option<&WlSurface>) {
    let client = focus.and_then(|s| st.dh.get_client(s.id()).ok());
    let seat = st.seat.clone();
    set_primary_focus(&st.dh, &seat, client);
}
