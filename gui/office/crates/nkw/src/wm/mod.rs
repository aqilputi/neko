//! The window manager's side of the world: layout ops, dialogs, fullscreen,
//! activation, persistence, clipboard, launcher (§4). P2 fills this in wave
//! 2b; the stub does nothing.

use serde_json::Value;

use crate::ext::Plugin;

pub struct Wm {}

impl Wm {
    pub fn new(cli: &crate::cli::Cli) -> Wm {
        let _ = cli;
        Wm {}
    }
}

impl Plugin for Wm {
    fn name(&self) -> &'static str {
        "wm"
    }

    fn dump(&self) -> Value {
        serde_json::json!({"stub": true})
    }
}
