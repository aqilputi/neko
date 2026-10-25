//! The pointer machine, pure (plan §2.3, D15). Skeleton by P1 (S28 types,
//! trivial bodies); P2 owns it from here.

use glam::Vec2;

use crate::arrange::Grip;
use crate::ids::{FrameId, ScreenId};
use crate::Time;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Button {
    Left,
    Middle,
    Right,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Mods {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub logo: bool,
}

/// What `world+` is: `ctrl+alt` nested, `super` in an own session
/// (router::WorldMod).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorldKey {
    CtrlAlt,
    Super,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PtrMode {
    Nav,
    Arrange,
    WalkFree,
    WalkAim,
    Focus,
    /// CONVERSA, SALTO, COMANDOS, phone, menu
    Ui,
}

/// What is under the pointer (the `nkw` side picks it).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Hit {
    /// a resize handle of the selected/hovered screen
    Handle { screen: ScreenId, grip: Grip },
    /// a quad in the world: rim or content
    Screen(ScreenId),
    /// a dock rect; `live` = 1×: plain events go to its client
    Dock { screen: ScreenId, live: bool },
    Frame(FrameId),
    /// index into the hub's pickables of this press
    Pickable(u16),
    /// wall/floor/air (the hub keeps the point)
    Empty,
    /// FOCO: the focused window or its popups
    Client,
    /// FOCO: the dimmed world around it
    Outside,
    /// the balloon, the phone, the dialogue box
    Ui,
    Nothing,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DragMod {
    Ctrl,
    Alt,
    World,
    None,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Config {
    pub drag: bool,
    pub modifier: DragMod,
    pub world: WorldKey,
    pub threshold_px: f32,
    pub double_s: Time,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gesture {
    Move,
    Resize(Grip),
    ResizeNearest,
    Lasso { additive: bool },
    Carry,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum State {
    Idle,
    /// a press not yet a drag; `gesture`: what crossing the threshold
    /// starts (None: a click or nothing)
    Pressed {
        button: Button,
        at: Vec2,
        hit: Hit,
        mods: Mods,
        t: Time,
        gesture: Option<Gesture>,
    },
    Drag {
        button: Button,
        gesture: Gesture,
        hit: Hit,
        start: Vec2,
        latched: Mods,
    },
    /// the press went to a client (FOCO, a live dock): the release goes
    /// there too
    Passing { button: Button },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PtrEv {
    Motion { at: Vec2, hit: Hit },
    Press { button: Button, at: Vec2, hit: Hit },
    Release { button: Button, at: Vec2 },
    /// every modifier change (from the seat keyboard)
    Mods(Mods),
    /// + = away from you (wheel up)
    Wheel { steps: i32, hit: Hit },
    /// `esc` during a drag, host focus lost
    Cancel,
    /// any machine mode change: a live drag ends as a release
    ModeChanged,
}

/// Gesture modifiers after the latch: a modifier held at the press does not
/// count until released.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Effective {
    /// shift
    pub strong: bool,
    /// alt
    pub rotate: bool,
    /// ctrl
    pub no_magnets: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CursorKind {
    Default,
    /// the drag modifier is held over a screen, or a plain press crossed
    /// the threshold with no gesture: it teaches the `ctrl`
    Grab,
    Grabbing,
    Resize(Grip),
    Crosshair,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PtrOut {
    Hover(Hit),
    PassPress(Button),
    PassRelease(Button),
    Click {
        button: Button,
        hit: Hit,
        mods: Mods,
        double: bool,
    },
    /// `at` = the press point
    DragStart { gesture: Gesture, hit: Hit, at: Vec2 },
    DragMove { at: Vec2, eff: Effective },
    /// `scale`: alt effective
    DragWheel { steps: i32, scale: bool },
    DragEnd { at: Vec2, eff: Effective },
    DragCancel,
    /// alt+wheel over a screen, no drag
    Scale { hit: Hit, steps: i32 },
    Cursor(CursorKind),
}

pub fn world_held(m: Mods, w: WorldKey) -> bool {
    match w {
        WorldKey::CtrlAlt => m.ctrl && m.alt,
        WorldKey::Super => m.logo,
    }
}

/// The table of plan §2.3/D15: the gesture a press starts, or None (a
/// click, a pass-through, nothing).
pub fn gesture_for(
    mode: PtrMode,
    hit: Hit,
    button: Button,
    mods: Mods,
    cfg: &Config,
) -> Option<Gesture> {
    let _ = (mode, hit, button, mods, cfg);
    None
}

/// The press goes to a client (FOCO without world+, a live dock without a
/// moving modifier).
pub fn passes(mode: PtrMode, hit: Hit, button: Button, mods: Mods, cfg: &Config) -> bool {
    let _ = (mode, hit, button, mods, cfg);
    false
}

pub fn effective(held: Mods, latched: Mods) -> Effective {
    let _ = (held, latched);
    Effective::default()
}

#[derive(Clone, Debug)]
pub struct Machine {
    state: State,
    held: Mods,
    last_click: Option<(Button, Time)>,
}

impl Default for Machine {
    fn default() -> Machine {
        Machine::new()
    }
}

impl Machine {
    pub fn new() -> Machine {
        Machine {
            state: State::Idle,
            held: Mods::default(),
            last_click: None,
        }
    }

    pub fn state(&self) -> &State {
        &self.state
    }

    pub fn step(&mut self, ev: PtrEv, mode: PtrMode, cfg: &Config, now: Time) -> Vec<PtrOut> {
        let _ = (ev, mode, cfg, now, self.held, self.last_click);
        Vec::new()
    }
}
