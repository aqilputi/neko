//! nkw-objects: the pure model of the world's objects
//! (docs/plano-mundo-objetos.md).
//!
//! Plain data and math, like `nkw-core`: events in, effects out; time comes
//! in as a parameter (`now`, seconds); no I/O, no threads, no clocks, no
//! D-Bus, no PipeWire, no GL.
//!
//! - [`acoustic`]: proximity audio, `gains()` through rooms and doors (§4.1–4.3)
//! - [`door`]: doors with state, swing and auto-close (§3.3, §4.4)
//! - [`vault`]: the vault's lock state machine and its path policy (§5)
//! - [`object`]: widgets as furniture, their data and interactions (§2)
//! - [`proximity`]: the radii and tiers (§2.3, §3.5)
//! - [`hud`]: the critical-only badges (§2.3)
//! - [`sound`]: our own procedural sounds and their mixer (§3.2–3.4, §4.7)

pub mod acoustic;
pub mod door;
pub mod hud;
pub mod ids;
pub mod object;
pub mod proximity;
pub mod sound;
pub mod vault;

pub use ids::{DoorId, ObjectId, SourceId};
pub use nkw_core::{Pose, RoomId, Time, Want};
