//! Snapping on release (and on ARRUMAR's `g`), with the guides to draw
//! (plan §2.8): dock > desk > wall > cluster > edge > 1:1 > eye > grid > air.

use glam::{Vec2, Vec3};

use crate::arrange::AirPose;
use crate::dock::DockSlot;
use crate::ids::{RoomId, ScreenId};
use crate::pose::Pose;

/// How hard the magnets pull: `Off` (`x`, `ctrl` pressed again), `Soft`
/// (the default), `Strong` (`shift`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Strength {
    Off,
    #[default]
    Soft,
    Strong,
}

pub const SOFT_WALL_M: f32 = 0.08;
pub const SOFT_WALL_DEG: f32 = 8.0;
pub const SOFT_EDGE_M: f32 = 0.02;
pub const SOFT_EDGE_DEG: f32 = 5.0;
pub const STRONG_EDGE_M: f32 = 0.04;
pub const GRID_M: f32 = 0.25;
pub const GRID_DEG: f32 = 15.0;
pub const EYE_M: f32 = 0.03;
/// desks: carry::SNAP_DESK_M; wall gap: carry::WALL_GAP
pub const ONE_TO_ONE_M: f32 = 0.02;

/// What the screen can snap to.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Targets {
    pub walls: Vec<crate::carry::WallSeg>,
    pub desks: Vec<crate::carry::DeskSlot>,
    /// the others in the air (edges; cluster on Strong): id, pose, size m
    pub screens: Vec<(ScreenId, AirPose, Vec2)>,
    pub eye: Pose,
    /// the scale-1 reading distance; the pose's scale multiplies it
    pub one_to_one_m: f32,
    /// the pointer is on a dock edge (the caller found it in 2D)
    pub dock: Option<DockSlot>,
    /// the room's origin (the grid's)
    pub grid_origin: Vec3,
    pub eye_height: f32,
}

/// Where a release lands.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Snap {
    Air,
    Dock(DockSlot),
    Desk { room: RoomId, desk: usize },
    Wall(Pose),
    Cluster(ScreenId),
    Edge(ScreenId),
    OneToOne,
    Eye,
    Grid,
}

/// What the overlay draws.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Guide {
    /// dashed line from the face's bottom centre down, "1,42 m"
    Floor { from: Vec3, height_m: f32 },
    /// "1,2 m"
    Depth { at: Vec3, m: f32 },
    /// where it will land
    Ghost { pose: AirPose, size_m: Vec2 },
    WallShadow { pose: Pose, size_m: Vec2 },
    DeskSlot { room: RoomId, desk: usize, at: Vec3 },
    /// "alinhado"
    EdgeLine { a: Vec3, b: Vec3 },
    GridDots { centre: Vec3, yaw: f32 },
    Horizon { y: f32 },
    /// the ring on the rim + "1:1"
    OneToOne,
    DockSlot(DockSlot),
    ClusterRing(ScreenId),
}

/// The first rule that fits, in this order: Dock > Desk > Wall > Cluster
/// (Strong, over another) > Edge > OneToOne > Eye > Grid (Strong only) >
/// Air; the snapped pose and the guides (always Floor + Depth + Ghost).
/// `Off`: the pose as is, `Snap::Air`, guides Floor + Depth only.
pub fn snap(pose: &AirPose, size_m: Vec2, t: &Targets, s: Strength) -> (AirPose, Snap, Vec<Guide>) {
    let _ = (size_m, t, s);
    (*pose, Snap::Air, Vec::new())
}
