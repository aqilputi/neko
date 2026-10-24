//! `zxdg_exporter_v2`/`zxdg_importer_v2` (xdg-foreign): a client exports a
//! toplevel and another makes its dialog a child of it (portals and
//! out-of-process pickers do this). Smithay keeps the handles; an imported
//! parent arrives as `XdgShellHandler::parent_changed`, so the World's
//! dialog rules apply unchanged.

use smithay::wayland::xdg_foreign::{XdgForeignHandler, XdgForeignState};

use crate::state::Nkw;

impl XdgForeignHandler for Nkw {
    fn xdg_foreign_state(&mut self) -> &mut XdgForeignState {
        &mut self.protos.foreign
    }
}
