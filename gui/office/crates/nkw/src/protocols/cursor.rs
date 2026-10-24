//! `wp_cursor_shape_v1` and the client cursor in FOCO: the host's cursor
//! follows the focused app's named shape (`set_host_cursor`), hidden when
//! it hides it, the default otherwise. A client's own cursor *surface*
//! (`wl_pointer.set_cursor` with a buffer) shows the default: drawing custom
//! cursor images is phase 6+.
//!
//! The manager state lives in [`super::Protocols`]; the shape device needs a
//! [`TabletSeatHandler`] (the tablet tool can carry a shape too).

use smithay::input::pointer::CursorImageStatus;
use smithay::input::tablet::TabletSeatHandler;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;

use crate::frame::HostCursor;
use crate::state::Nkw;

/// What the host shows for a client's cursor request.
pub fn host_cursor_for(image: &CursorImageStatus) -> HostCursor {
    match image {
        CursorImageStatus::Named(icon) => HostCursor::Named(*icon),
        CursorImageStatus::Hidden => HostCursor::Hidden,
        // custom cursor images: phase 6+ (the default is the honest fallback)
        CursorImageStatus::Surface(_) => HostCursor::Default,
    }
}

impl Nkw {
    /// `SeatHandler::cursor_image`: only the FOCO client steers the host's
    /// cursor (the world itself takes no pointer); out of FOCO it stays the
    /// default.
    pub(crate) fn client_cursor(&mut self, image: CursorImageStatus) {
        if !self.in_focus_mode() {
            return;
        }
        self.set_host_cursor(host_cursor_for(&image));
    }
}

impl TabletSeatHandler for Nkw {
    type ToolFocus = WlSurface;
}

#[cfg(test)]
mod tests {
    use super::*;
    use smithay::input::pointer::CursorIcon;

    #[test]
    fn cursor_requests_map_to_the_host() {
        let cases = [
            (
                CursorImageStatus::Named(CursorIcon::Text),
                HostCursor::Named(CursorIcon::Text),
            ),
            (
                CursorImageStatus::Named(CursorIcon::Pointer),
                HostCursor::Named(CursorIcon::Pointer),
            ),
            (
                CursorImageStatus::default_named(),
                HostCursor::Named(CursorIcon::Default),
            ),
            (CursorImageStatus::Hidden, HostCursor::Hidden),
        ];
        for (img, want) in cases {
            assert_eq!(host_cursor_for(&img), want, "{img:?}");
        }
        // the names the dump prints are the CSS ones
        assert_eq!(HostCursor::Named(CursorIcon::Text).name(), "text");
        assert_eq!(
            HostCursor::Named(CursorIcon::NwseResize).name(),
            "nwse-resize"
        );
    }
}
