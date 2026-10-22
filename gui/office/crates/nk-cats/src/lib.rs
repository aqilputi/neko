//! nk-cats — the engine-agnostic half of the office of cats, shared by the
//! Bevy app (`nkg`) and the world compositor (`nkw`).
//!
//! Everything here is pure: no engine, no windowing, no client, no I/O or
//! threads (the only clock read is [`model::now_ms`]; everything else takes
//! `now` as a parameter). The renderer owns the side effects.
//!
//! - [`model`]: core state (snapshot, tree rows, theme) reduced to what a
//!   scene draws — columns, contexts, poses, collars, palette.
//! - [`catpose`]: cat motion as plain joint numbers per [`model::Pose`].
//! - [`inbox`]: the inbox walkers' queue along the aisle.
//! - [`monitor`]: a monitor's activity tail, scrolling in line by line.
//! - [`raster`]: CPU monospace text rasterizer (fontdue) into RGBA8, with
//!   the embedded JetBrains Mono NL font ([`raster::FONT_BYTES`], SIL OFL,
//!   see `assets/fonts/OFL.txt`).
//! - [`keys`]: key names → actions, resolved against the core keymap, with
//!   multi-key sequences.

pub mod catpose;
pub mod inbox;
pub mod keys;
pub mod model;
pub mod monitor;
pub mod raster;
