//! nkw-core: the pure heart of the world (`nkw`).
//!
//! Everything here is plain data and math: actions in, effects out; time
//! comes in as a parameter (`now`, seconds); no Wayland, no GL, no threads,
//! no processes, no file system. See docs/plano-mundo.md §2.11.
//!
//! - [`mode`]: the mode state machine (NAV/ANDAR + one overlay, §3.3)
//! - [`strip`]: desks, rooms, stations and their neighbours (§3.4, §4.4)
//! - [`layout`]: where rooms, desks, chairs, monitors and cats go (§4.2)
//! - [`onetoone`]: the exact 1:1 camera pose (§4.2)
//! - [`tween`]: camera glides and cuts (§3.11)
//! - [`collide`]: walking physics and the occupancy grid (§3.5)
//! - [`walk_exit`]: where ANDAR lands in NAV (§3.5)
//! - [`schedule`]: the frame scheduler (§5.6)
//! - [`hints`]: SALTO labels (§3.8)
//! - wave 2b stubs: [`persist`], [`rules`] (P2); [`palette`], [`line`],
//!   [`dialog`] (P3)

pub mod aim;
pub mod carry;
pub mod collide;
pub mod dialog;
pub mod hints;
pub mod ids;
pub mod layout;
pub mod library;
pub mod line;
pub mod mode;
pub mod onetoone;
pub mod palette;
pub mod persist;
pub mod pose;
pub mod rules;
pub mod schedule;
pub mod strip;
pub mod tween;
pub mod walk_exit;

pub use collide::{Aabb, Door, Grid, TurnMode, WalkInput, WalkParams, Walker, Wall};
pub use hints::{HintMatch, Hints};
pub use ids::*;
pub use layout::RoomFrame;
pub use mode::{
    Action, Base, Effect, Kind, Machine, Mode, ObjectAction, Pick, TalkPurpose, Target,
};
pub use onetoone::{OneToOne, Output, ScreenPlace};
pub use pose::{Pose, angle_diff};
pub use schedule::{Pass, Plan, Vblank, Want, Wish};
pub use strip::{Desk, Dir, RoomSlot, Side, Spot, Station, Width};
pub use tween::{Path, Tween, TweenOpts};
pub use walk_exit::{ExitRule, ExitTo};

/// Time, in seconds since any fixed epoch (the caller passes `now`).
pub type Time = f64;
