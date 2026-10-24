//! The extra Wayland protocols (contract S23): cursor shape, primary
//! selection, presentation time, pointer constraints, relative pointer,
//! single-pixel buffer, content type, xdg-foreign, idle inhibit and (behind
//! `--text-input`) text-input-v3. One file per protocol with its handler;
//! this file holds the states, the bookkeeping, `dump()` and the four hub
//! hooks (`protos_*`, called from `input.rs`, `state.rs`, `frame.rs` and the
//! seat handler).
//!
//! `smithay::delegate_dispatch2!(Nkw)` (in `handlers.rs`) dispatches every
//! state here: adding a protocol is "construct its state + impl its handler".

pub mod constraints;
pub mod content_type;
pub mod cursor;
pub mod foreign;
pub mod idle;
pub mod presentation;
pub mod primary;
pub mod single_pixel;
pub mod text_input;

use std::collections::{BTreeMap, HashSet};
use std::time::Duration;

use serde_json::Value;
use smithay::reexports::wayland_server::DisplayHandle;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::wayland::content_type::ContentTypeState;
use smithay::wayland::cursor_shape::CursorShapeManagerState;
use smithay::wayland::idle_inhibit::IdleInhibitManagerState;
use smithay::wayland::pointer_constraints::PointerConstraintsState;
use smithay::wayland::presentation::PresentationState;
use smithay::wayland::relative_pointer::RelativePointerManagerState;
use smithay::wayland::selection::primary_selection::PrimarySelectionState;
use smithay::wayland::single_pixel_buffer::SinglePixelBufferState;
use smithay::wayland::text_input::TextInputManagerState;
use smithay::wayland::xdg_foreign::XdgForeignState;

use nkw_core::ScreenId;

use crate::cli::Cli;
use crate::state::Nkw;

/// The globals this module advertises, in `new`'s order (`dump()["protocols"]["globals"]`).
pub const GLOBALS: [&str; 10] = [
    "wp_cursor_shape_manager_v1",
    "zwp_primary_selection_device_manager_v1",
    "wp_presentation",
    "zwp_pointer_constraints_v1",
    "zwp_relative_pointer_manager_v1",
    "wp_single_pixel_buffer_manager_v1",
    "wp_content_type_manager_v1",
    "zxdg_exporter_v2",
    "zxdg_importer_v2",
    "zwp_idle_inhibit_manager_v1",
];
/// Only with `--text-input` (evaluation: no input method behind it).
pub const TEXT_INPUT_GLOBAL: &str = "zwp_text_input_manager_v3";

/// How often each hub hook was called (`dump()["protocols"]["calls"]`, W1's
/// call-site test reads it).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Calls {
    pub relative_motion: u64,
    pub focus_changed: u64,
    pub presented: u64,
    pub mode_left_focus: u64,
}

/// A client's pointer lock (or confinement, which nested behaves as a lock):
/// the surface and the screen it belongs to.
#[derive(Clone, Debug, PartialEq)]
pub struct ClientLock {
    pub surface: WlSurface,
    pub screen: Option<ScreenId>,
}

pub struct Protocols {
    pub cursor_shape: CursorShapeManagerState,
    pub primary: PrimarySelectionState,
    /// `CLOCK_MONOTONIC` (the clock of `stats::mono_us`)
    pub presentation: PresentationState,
    pub constraints: PointerConstraintsState,
    pub relative: RelativePointerManagerState,
    pub single_pixel: SinglePixelBufferState,
    pub content_type: ContentTypeState,
    pub foreign: XdgForeignState,
    pub idle_inhibit: IdleInhibitManagerState,
    pub text_input: Option<TextInputManagerState>,
    pub calls: Calls,
    /// the FOCO client holding the pointer (it gets the relative motion)
    pub locked_by_client: Option<ClientLock>,
    /// `zwp_locked_pointer_v1.set_cursor_position_hint` of the lock, surface-local
    pub cursor_hint: Option<(f64, f64)>,
    /// surfaces with a live idle inhibitor (recorded; the host is not told this wave)
    pub inhibitors: HashSet<WlSurface>,
    /// a client owns the primary selection
    pub has_primary: bool,
    /// `wp_content_type_v1` of each screen, as last committed (`none` is not kept)
    pub content_types: BTreeMap<ScreenId, &'static str>,
}

impl Protocols {
    pub fn new(dh: &DisplayHandle, cli: &Cli) -> Protocols {
        Protocols {
            cursor_shape: CursorShapeManagerState::new::<Nkw>(dh),
            primary: PrimarySelectionState::new::<Nkw>(dh),
            presentation: PresentationState::new::<Nkw>(dh, libc::CLOCK_MONOTONIC as u32),
            constraints: PointerConstraintsState::new::<Nkw>(dh),
            relative: RelativePointerManagerState::new::<Nkw>(dh),
            single_pixel: SinglePixelBufferState::new::<Nkw>(dh),
            content_type: ContentTypeState::new::<Nkw>(dh),
            foreign: XdgForeignState::new::<Nkw>(dh),
            idle_inhibit: IdleInhibitManagerState::new::<Nkw>(dh),
            text_input: cli
                .text_input
                .then(|| TextInputManagerState::new::<Nkw>(dh)),
            calls: Calls::default(),
            locked_by_client: None,
            cursor_hint: None,
            inhibitors: HashSet::new(),
            has_primary: false,
            content_types: BTreeMap::new(),
        }
    }

    /// The names of the globals advertised here.
    pub fn globals(&self) -> Vec<&'static str> {
        let mut g = GLOBALS.to_vec();
        if self.text_input.is_some() {
            g.push(TEXT_INPUT_GLOBAL);
        }
        g
    }

    /// Some live surface asks the screen to stay awake (W6's stretch forwards it).
    pub fn idle_inhibited(&self) -> bool {
        self.inhibitors.iter().any(|s| s.is_alive())
    }

    /// A FOCO client holds the pointer lock (the hub keeps the host lock on).
    pub fn client_locked(&self) -> bool {
        self.locked_by_client.is_some()
    }

    /// `world.dump()["protocols"]`.
    pub fn dump(&self) -> Value {
        let c = &self.calls;
        let locked_by = self
            .locked_by_client
            .as_ref()
            .map(|l| {
                l.screen
                    .map_or(Value::from("surface"), |s| Value::from(s.0))
            })
            .unwrap_or(Value::Null);
        let content: serde_json::Map<String, Value> = self
            .content_types
            .iter()
            .map(|(id, k)| (id.0.to_string(), Value::from(*k)))
            .collect();
        serde_json::json!({
            "globals": self.globals(),
            "locked_by": locked_by,
            "inhibitors": self.inhibitors.iter().filter(|s| s.is_alive()).count(),
            "primary": self.has_primary,
            "content_types": content,
            "text_input": self.text_input.is_some(),
            "calls": {
                "relative_motion": c.relative_motion,
                "focus_changed": c.focus_changed,
                "presented": c.presented,
                "mode_left_focus": c.mode_left_focus,
            },
        })
    }

    /// `dump()["screens"][i]["content_type"]` from [`Protocols::content_types`]
    /// (for the hub's `dump`, which owns the screens array).
    pub fn annotate_screens(&self, dump: &mut Value) {
        let Some(screens) = dump.get_mut("screens").and_then(Value::as_array_mut) else {
            return;
        };
        for s in screens {
            let Some(id) = s.get("id").and_then(Value::as_u64) else {
                continue;
            };
            let kind = self
                .content_types
                .get(&ScreenId(id as _))
                .copied()
                .unwrap_or("none");
            s["content_type"] = Value::from(kind);
        }
    }
}

use smithay::reexports::wayland_server::Resource;

impl Nkw {
    /// A relative motion from the host (the fork's `PointerMotion`): fed to
    /// the FOCO client's `zwp_relative_pointer` while it holds a lock; true =
    /// consumed (the walk's look never sees it).
    pub fn protos_relative_motion(
        &mut self,
        dx: f64,
        dy: f64,
        dxu: f64,
        dyu: f64,
        utime: u64,
    ) -> bool {
        self.protos.calls.relative_motion += 1;
        self.client_relative_motion((dx, dy), (dxu, dyu), utime)
    }

    /// Keyboard focus moved (the seat handler): the primary selection follows
    /// the keyboard; in FOCO the surface's pending lock activates; a `None`
    /// focus releases any client lock and resets the host cursor.
    pub fn protos_focus_changed(&mut self, focus: Option<&WlSurface>) {
        self.protos.calls.focus_changed += 1;
        primary::focus(self, focus);
        match focus {
            Some(s) if self.in_focus_mode() => self.focus_constraints(s),
            Some(_) => {}
            None => {
                self.release_client_lock();
                self.set_host_cursor(crate::frame::HostCursor::Default);
            }
        }
    }

    /// After a submit: every composed surface's presentation feedback is
    /// `presented(at, refresh, seq, VSYNC)`.
    pub fn protos_presented(
        &mut self,
        surfaces: &[WlSurface],
        at: Duration,
        refresh: Duration,
        seq: u64,
    ) {
        self.protos.calls.presented += 1;
        presentation::presented(&self.output, surfaces, at, refresh, seq);
        self.headless_pointer_follows_focus();
    }

    /// The mode left FOCO: constraints off, the host lock back to the walk's
    /// rule, the cursor back to the default.
    pub fn protos_mode_left_focus(&mut self) {
        self.protos.calls.mode_left_focus += 1;
        self.release_client_lock();
        self.set_host_cursor(crate::frame::HostCursor::Default);
    }

    /// The screen a (root) surface belongs to, alive or not.
    pub(crate) fn protos_screen_of(&self, s: &WlSurface) -> Option<ScreenId> {
        if !s.is_alive() {
            return None;
        }
        self.screen_of(s)
    }
}
