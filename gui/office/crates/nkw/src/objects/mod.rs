//! The world's objects and its sound: `nkw-objects` and `nkw-sys` wired in,
//! doors, proximity audio, own sounds, badges (addendum). P5 fills this in
//! wave 2b; the stub does nothing.

use glam::Vec3;
use nkw_core::{ObjectAction, ObjectId};
use serde_json::Value;

use crate::ext::{Ctx, DialogView, Plugin, Req};

pub struct Objects {}

impl Objects {
    pub fn new(cli: &crate::cli::Cli) -> Objects {
        let _ = cli;
        Objects {}
    }

    /// `Out::Object`: do `a` to the object.
    pub fn on_object(&mut self, id: ObjectId, a: ObjectAction, ctx: &Ctx) -> Vec<Req> {
        let _ = ctx;
        tracing::debug!(?id, ?a, "object (stub)");
        Vec::new()
    }

    /// CONVERSA with purpose `VaultUnlock`.
    pub fn dialog(&self) -> Option<DialogView> {
        None
    }

    /// `Req::Cue`: one of our own sounds, at `at` (`None`: the listener).
    pub fn cue(&mut self, cue: nkw_objects::sound::Cue, at: Option<Vec3>, ctx: &Ctx) {
        let _ = (cue, at, ctx);
    }
}

impl Plugin for Objects {
    fn name(&self) -> &'static str {
        "objects"
    }

    fn dump(&self) -> Value {
        serde_json::json!({"stub": true})
    }
}
