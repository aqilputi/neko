//! The 2D overlay: HUD, SALTO, COMANDOS, dialogue box, toasts, tags, map
//! (§3.7–3.10). P3 fills this in wave 2b; the stub draws phase 0's mode
//! label (hidden in FOCO) and nothing else.

pub mod label;

use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::element::memory::MemoryRenderBuffer;
use smithay::utils::Transform;

use nkw_core::Time;
use serde_json::Value;

use crate::ext::{Ctx, HudInput, OverlayElem, Plugin, Req, ToastKind};
use crate::router::KeyEv;

/// Where the mode label sits, logical px from the top-left.
pub const LABEL_POS: (i32, i32) = (16, 16);
/// Its element id.
pub const LABEL_ID: u64 = 1;

pub struct Overlay {
    /// the label's text and buffer, rebuilt only when the text changes
    label: Option<(&'static str, MemoryRenderBuffer)>,
}

impl Overlay {
    pub fn new(cli: &crate::cli::Cli) -> Overlay {
        let _ = cli;
        Overlay { label: None }
    }

    /// `Route::Text`: a key typed into the overlay (palette, line, SALTO).
    pub fn key(&mut self, ev: &KeyEv, ctx: &Ctx) -> Vec<Req> {
        let _ = (ev, ctx);
        Vec::new()
    }

    /// `Some("map")` while the map is open.
    pub fn submode(&self) -> Option<&'static str> {
        None
    }

    pub fn toast(&mut self, text: &str, kind: ToastKind, ms: u32, now: Time) {
        tracing::info!(?kind, ms, now, "toast: {text}");
    }

    /// The hub fills `Ctx.line` from it on `SendLine`.
    pub fn take_line(&mut self) -> Option<String> {
        None
    }

    /// The 2D elements of this frame, topmost first.
    pub fn elements(&mut self, ctx: &Ctx, hud: &HudInput) -> Vec<OverlayElem> {
        let _ = ctx;
        if hud.focus_rect.is_some() {
            return Vec::new();
        }
        let text = hud.mode_label;
        if self.label.as_ref().map(|l| l.0) != Some(text) {
            let (w, h, px) = label::label(text, 2);
            let buf = MemoryRenderBuffer::from_slice(
                &px,
                Fourcc::Abgr8888,
                (w as i32, h as i32),
                1,
                Transform::Normal,
                None,
            );
            self.label = Some((text, buf));
        }
        let buf = self.label.as_ref().map(|l| l.1.clone()).unwrap();
        vec![OverlayElem {
            id: LABEL_ID,
            pos: LABEL_POS,
            buf,
        }]
    }
}

impl Plugin for Overlay {
    fn name(&self) -> &'static str {
        "overlay"
    }

    fn dump(&self) -> Value {
        serde_json::json!({"stub": true, "label": self.label.as_ref().map(|l| l.0)})
    }
}
