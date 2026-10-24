//! `wp_content_type_v1`: read on every commit of a toplevel, kept per screen
//! (`dump()["protocols"]["content_types"]`, `dump()["screens"][i]["content_type"]`).
//! Nothing acts on it yet: a video could keep its screen at full rate in the
//! world pass (phase 6).

use smithay::reexports::wayland_protocols::wp::content_type::v1::server::wp_content_type_v1::Type;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::wayland::compositor::with_states;
use smithay::wayland::content_type::ContentTypeSurfaceCachedState;

use nkw_core::ScreenId;

use crate::state::Nkw;

/// The protocol's type by the name the dump prints.
pub fn name(t: Type) -> &'static str {
    match t {
        Type::Photo => "photo",
        Type::Video => "video",
        Type::Game => "game",
        _ => "none",
    }
}

impl Nkw {
    /// A toplevel committed: its content type as now current.
    pub(crate) fn content_type_committed(&mut self, id: ScreenId, root: &WlSurface) {
        let t = with_states(root, |s| {
            *s.cached_state
                .get::<ContentTypeSurfaceCachedState>()
                .current()
                .content_type()
        });
        match name(t) {
            "none" => {
                self.protos.content_types.remove(&id);
            }
            n => {
                self.protos.content_types.insert(id, n);
            }
        }
    }

    /// A toplevel is gone.
    pub(crate) fn content_type_forget(&mut self, id: ScreenId) {
        self.protos.content_types.remove(&id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn type_names() {
        assert_eq!(name(Type::None), "none");
        assert_eq!(name(Type::Photo), "photo");
        assert_eq!(name(Type::Video), "video");
        assert_eq!(name(Type::Game), "game");
    }
}
