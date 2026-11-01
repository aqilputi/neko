//! The pointer machine, pure (plan §2.3, D15). Skeleton by P1 (S28 types,
//! trivial bodies); P2 owns it from here.

use glam::Vec2;

use crate::Time;
use crate::arrange::Grip;
use crate::ids::{FrameId, ScreenId};

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
    Handle {
        screen: ScreenId,
        grip: Grip,
    },
    /// a quad in the world: rim or content
    Screen(ScreenId),
    /// a dock rect; `live` = 1×: plain events go to its client
    Dock {
        screen: ScreenId,
        live: bool,
    },
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
    Passing {
        button: Button,
    },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PtrEv {
    Motion {
        at: Vec2,
        hit: Hit,
    },
    Press {
        button: Button,
        at: Vec2,
        hit: Hit,
    },
    Release {
        button: Button,
        at: Vec2,
    },
    /// every modifier change (from the seat keyboard)
    Mods(Mods),
    /// + = away from you (wheel up)
    Wheel {
        steps: i32,
        hit: Hit,
    },
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
    DragStart {
        gesture: Gesture,
        hit: Hit,
        at: Vec2,
    },
    DragMove {
        at: Vec2,
        eff: Effective,
    },
    /// `scale`: alt effective
    DragWheel {
        steps: i32,
        scale: bool,
    },
    DragEnd {
        at: Vec2,
        eff: Effective,
    },
    DragCancel,
    /// alt+wheel over a screen, no drag
    Scale {
        hit: Hit,
        steps: i32,
    },
    Cursor(CursorKind),
}

pub fn world_held(m: Mods, w: WorldKey) -> bool {
    match w {
        WorldKey::CtrlAlt => m.ctrl && m.alt,
        WorldKey::Super => m.logo,
    }
}

/// Whether `mods` holds the drag modifier (plan §2.3, `[mouse]
/// drag_modifier`): the configured one, or `world+` (which always moves,
/// in every mode). `cfg.drag = false` turns off every mouse drag.
fn moving_modifier(mods: Mods, cfg: &Config) -> bool {
    if !cfg.drag {
        return false;
    }
    let specific = match cfg.modifier {
        DragMod::Ctrl => mods.ctrl,
        DragMod::Alt => mods.alt,
        DragMod::World => world_held(mods, cfg.world),
        DragMod::None => true,
    };
    specific || world_held(mods, cfg.world)
}

/// Whether `mode` lets a resize handle be grabbed (plan §2.4: NAV, ARRUMAR,
/// ANDAR with the pointer free).
fn handle_mode(mode: PtrMode) -> bool {
    matches!(mode, PtrMode::Nav | PtrMode::Arrange | PtrMode::WalkFree)
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
    if !cfg.drag {
        return None;
    }
    // "Alça é alça": no modifier needed, left button only, where the
    // pointer picks handles at all.
    if let Hit::Handle { grip, .. } = hit {
        return (button == Button::Left && handle_mode(mode)).then_some(Gesture::Resize(grip));
    }
    // Anchored screens: ctrl+/world+left moves them in every mode (§2.7).
    if let Hit::Dock { .. } = hit {
        return (button == Button::Left && moving_modifier(mods, cfg)).then_some(Gesture::Move);
    }
    let world = world_held(mods, cfg.world);
    match mode {
        PtrMode::Focus => match (button, hit) {
            // ctrl+click (and everything else) in FOCO is the app's;
            // only world+ moves or resizes.
            (Button::Left, Hit::Screen(_) | Hit::Client) if world => Some(Gesture::Move),
            (Button::Right, Hit::Screen(_) | Hit::Client) if world => Some(Gesture::ResizeNearest),
            _ => None,
        },
        PtrMode::Ui => match (button, hit) {
            (Button::Left, Hit::Screen(_) | Hit::Frame(_)) if world => Some(Gesture::Move),
            (Button::Right, Hit::Screen(_) | Hit::Frame(_)) if world => {
                Some(Gesture::ResizeNearest)
            }
            _ => None,
        },
        PtrMode::WalkAim => match (button, hit) {
            // "ctrl+esquerdo segurado carrega": holding takes it to hand;
            // no pointer movement is needed (the Machine starts it at once).
            (Button::Left, Hit::Screen(_) | Hit::Frame(_) | Hit::Pickable(_))
                if moving_modifier(mods, cfg) =>
            {
                Some(Gesture::Carry)
            }
            // resizing only happens in ARRUMAR
            _ => None,
        },
        PtrMode::Nav | PtrMode::Arrange | PtrMode::WalkFree => match (button, hit) {
            (Button::Left, Hit::Screen(_) | Hit::Frame(_)) if moving_modifier(mods, cfg) => {
                Some(Gesture::Move)
            }
            (Button::Left, Hit::Empty) => Some(Gesture::Lasso {
                additive: mods.shift,
            }),
            (Button::Right, Hit::Screen(_) | Hit::Frame(_)) if moving_modifier(mods, cfg) => {
                Some(Gesture::ResizeNearest)
            }
            _ => None,
        },
    }
}

/// The press goes to a client (FOCO without world+, a live dock without a
/// moving modifier).
pub fn passes(mode: PtrMode, hit: Hit, button: Button, mods: Mods, cfg: &Config) -> bool {
    if gesture_for(mode, hit, button, mods, cfg).is_some() {
        return false;
    }
    let world = world_held(mods, cfg.world);
    match hit {
        Hit::Ui => true,
        Hit::Dock { live: true, .. } => !moving_modifier(mods, cfg),
        Hit::Client => !world,
        Hit::Screen(_) if matches!(mode, PtrMode::Focus) => !world,
        _ => false,
    }
}

/// `held`'s shift/alt/ctrl, minus whichever of alt/ctrl started the current
/// drag and is still down (the "trava", §2.3): the modifier that began the
/// gesture does not count as a live gesture-modifier until it is released
/// and pressed again (which clears the matching bit of `latched` — the
/// Machine's job, not this pure function's).
pub fn effective(held: Mods, latched: Mods) -> Effective {
    Effective {
        strong: held.shift,
        rotate: held.alt && !latched.alt,
        no_magnets: held.ctrl && !latched.ctrl,
    }
}

/// Whether `a → b` crosses the drag threshold; `Carry` has none (holding
/// the button in ANDAR's mira is the gesture, not moving the pointer).
fn crossed(gesture: Option<Gesture>, a: Vec2, b: Vec2, cfg: &Config) -> bool {
    matches!(gesture, Some(Gesture::Carry)) || (a - b).length() >= cfg.threshold_px
}

#[derive(Clone, Debug)]
pub struct Machine {
    state: State,
    held: Mods,
    last_click: Option<(Button, Time)>,
    /// the last pointer position seen (Motion or Press), for events that
    /// carry none of their own (`ModeChanged`)
    last_pos: Vec2,
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
            last_pos: Vec2::ZERO,
        }
    }

    pub fn state(&self) -> &State {
        &self.state
    }

    pub fn step(&mut self, ev: PtrEv, mode: PtrMode, cfg: &Config, now: Time) -> Vec<PtrOut> {
        match ev {
            PtrEv::Motion { at, hit } => {
                self.last_pos = at;
                match self.state {
                    State::Idle => vec![PtrOut::Hover(hit)],
                    State::Pressed {
                        button,
                        at: press_at,
                        hit: press_hit,
                        mods: press_mods,
                        gesture,
                        ..
                    } => {
                        if crossed(gesture, press_at, at, cfg) {
                            if let Some(g) = gesture {
                                self.state = State::Drag {
                                    button,
                                    gesture: g,
                                    hit: press_hit,
                                    start: press_at,
                                    latched: press_mods,
                                };
                                let eff = effective(self.held, press_mods);
                                vec![
                                    PtrOut::DragStart {
                                        gesture: g,
                                        hit: press_hit,
                                        at: press_at,
                                    },
                                    PtrOut::DragMove { at, eff },
                                ]
                            } else {
                                Vec::new()
                            }
                        } else {
                            Vec::new()
                        }
                    }
                    State::Drag { latched, .. } => {
                        let eff = effective(self.held, latched);
                        vec![PtrOut::DragMove { at, eff }]
                    }
                    State::Passing { .. } => Vec::new(),
                }
            }
            PtrEv::Press { button, at, hit } => {
                self.last_pos = at;
                if !matches!(self.state, State::Idle) {
                    return Vec::new();
                }
                if passes(mode, hit, button, self.held, cfg) {
                    self.state = State::Passing { button };
                    return vec![PtrOut::PassPress(button)];
                }
                let gesture = gesture_for(mode, hit, button, self.held, cfg);
                if let Some(g @ Gesture::Carry) = gesture {
                    self.state = State::Drag {
                        button,
                        gesture: g,
                        hit,
                        start: at,
                        latched: self.held,
                    };
                    return vec![PtrOut::DragStart {
                        gesture: g,
                        hit,
                        at,
                    }];
                }
                self.state = State::Pressed {
                    button,
                    at,
                    hit,
                    mods: self.held,
                    t: now,
                    gesture,
                };
                Vec::new()
            }
            PtrEv::Release { button, at } => {
                self.last_pos = at;
                match self.state {
                    State::Passing { button: b } if b == button => {
                        self.state = State::Idle;
                        vec![PtrOut::PassRelease(button)]
                    }
                    State::Pressed {
                        button: b,
                        at: press_at,
                        hit,
                        mods: press_mods,
                        gesture,
                        ..
                    } if b == button => {
                        self.state = State::Idle;
                        if crossed(gesture, press_at, at, cfg) {
                            match gesture {
                                Some(g) => {
                                    let eff = effective(self.held, press_mods);
                                    vec![
                                        PtrOut::DragStart {
                                            gesture: g,
                                            hit,
                                            at: press_at,
                                        },
                                        PtrOut::DragMove { at, eff },
                                        PtrOut::DragEnd { at, eff },
                                    ]
                                }
                                None => Vec::new(),
                            }
                        } else {
                            let double = self
                                .last_click
                                .is_some_and(|(b2, t2)| b2 == button && now - t2 <= cfg.double_s);
                            self.last_click = Some((button, now));
                            vec![PtrOut::Click {
                                button,
                                hit,
                                mods: self.held,
                                double,
                            }]
                        }
                    }
                    State::Drag {
                        button: b, latched, ..
                    } if b == button => {
                        self.state = State::Idle;
                        let eff = effective(self.held, latched);
                        vec![PtrOut::DragEnd { at, eff }]
                    }
                    _ => Vec::new(),
                }
            }
            PtrEv::Mods(m) => {
                if let State::Drag {
                    ref mut latched, ..
                } = self.state
                {
                    latched.ctrl &= m.ctrl;
                    latched.alt &= m.alt;
                    latched.shift &= m.shift;
                    latched.logo &= m.logo;
                }
                self.held = m;
                Vec::new()
            }
            PtrEv::Wheel { steps, hit } => match self.state {
                State::Drag {
                    gesture: Gesture::Move,
                    ..
                } => vec![PtrOut::DragWheel {
                    steps,
                    scale: self.held.alt,
                }],
                _ if self.held.alt => vec![PtrOut::Scale { hit, steps }],
                _ => Vec::new(),
            },
            PtrEv::Cancel => match self.state {
                State::Drag { .. } => {
                    self.state = State::Idle;
                    vec![PtrOut::DragCancel]
                }
                State::Pressed { .. } => {
                    self.state = State::Idle;
                    Vec::new()
                }
                _ => Vec::new(),
            },
            PtrEv::ModeChanged => match self.state {
                State::Drag { latched, .. } => {
                    self.state = State::Idle;
                    let eff = effective(self.held, latched);
                    vec![PtrOut::DragEnd {
                        at: self.last_pos,
                        eff,
                    }]
                }
                State::Passing { button } => {
                    self.state = State::Idle;
                    vec![PtrOut::PassRelease(button)]
                }
                _ => {
                    self.state = State::Idle;
                    Vec::new()
                }
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> Config {
        Config {
            drag: true,
            modifier: DragMod::Ctrl,
            world: WorldKey::CtrlAlt,
            threshold_px: 6.0,
            double_s: 0.4,
        }
    }

    fn mods(ctrl: bool, alt: bool, shift: bool, logo: bool) -> Mods {
        Mods {
            ctrl,
            alt,
            shift,
            logo,
        }
    }

    const NONE: Mods = Mods {
        ctrl: false,
        alt: false,
        shift: false,
        logo: false,
    };
    const CTRL: Mods = Mods {
        ctrl: true,
        alt: false,
        shift: false,
        logo: false,
    };
    const ALT: Mods = Mods {
        ctrl: false,
        alt: true,
        shift: false,
        logo: false,
    };
    const WORLD: Mods = Mods {
        ctrl: true,
        alt: true,
        shift: false,
        logo: false,
    };
    const SHIFT: Mods = Mods {
        ctrl: false,
        alt: false,
        shift: true,
        logo: false,
    };

    #[test]
    fn world_held_matches_the_configured_key() {
        assert!(world_held(WORLD, WorldKey::CtrlAlt));
        assert!(!world_held(CTRL, WorldKey::CtrlAlt));
        assert!(!world_held(ALT, WorldKey::CtrlAlt));
        assert!(world_held(mods(false, false, false, true), WorldKey::Super));
        assert!(!world_held(WORLD, WorldKey::Super));
    }

    const SCREEN: Hit = Hit::Screen(ScreenId(1));
    const FRAME: Hit = Hit::Frame(FrameId(1));
    const HANDLE: Hit = Hit::Handle {
        screen: ScreenId(1),
        grip: Grip::Se,
    };
    const DOCK_LIVE: Hit = Hit::Dock {
        screen: ScreenId(1),
        live: true,
    };
    const DOCK_PIP: Hit = Hit::Dock {
        screen: ScreenId(1),
        live: false,
    };

    /// (mode, hit, button, mods) → gesture, the table of §2.3/D15.
    #[test]
    fn gesture_table() {
        use Button::*;
        use Gesture as G;
        use PtrMode::*;
        let c = cfg();
        let rows: &[(PtrMode, Hit, Button, Mods, Option<Gesture>)] = &[
            // handles: no modifier needed, left only, 3D-pointer modes
            (Nav, HANDLE, Left, NONE, Some(G::Resize(Grip::Se))),
            (Arrange, HANDLE, Left, NONE, Some(G::Resize(Grip::Se))),
            (WalkFree, HANDLE, Left, NONE, Some(G::Resize(Grip::Se))),
            (WalkAim, HANDLE, Left, NONE, None),
            (Focus, HANDLE, Left, NONE, None),
            (Nav, HANDLE, Right, NONE, None),
            // NAV/ARRUMAR/ANDAR-livre: plain click never moves
            (Nav, SCREEN, Left, NONE, None),
            (Arrange, SCREEN, Left, NONE, None),
            (WalkFree, SCREEN, Left, NONE, None),
            // ctrl+drag moves from anywhere on the screen
            (Nav, SCREEN, Left, CTRL, Some(G::Move)),
            (Arrange, FRAME, Left, CTRL, Some(G::Move)),
            // world+drag always moves too
            (Nav, SCREEN, Left, WORLD, Some(G::Move)),
            // empty space: left-drag lassos, additive with shift
            (
                Nav,
                Hit::Empty,
                Left,
                NONE,
                Some(G::Lasso { additive: false }),
            ),
            (
                Nav,
                Hit::Empty,
                Left,
                SHIFT,
                Some(G::Lasso { additive: true }),
            ),
            (Nav, Hit::Empty, Right, NONE, None),
            // resize by the nearest corner: ctrl+right or world+right
            (Nav, SCREEN, Right, CTRL, Some(G::ResizeNearest)),
            (Arrange, SCREEN, Right, WORLD, Some(G::ResizeNearest)),
            (Nav, SCREEN, Right, NONE, None),
            // docks: ctrl/world+left moves in any mode, never resizes
            (Nav, DOCK_LIVE, Left, CTRL, Some(G::Move)),
            (Focus, DOCK_LIVE, Left, WORLD, Some(G::Move)),
            (Nav, DOCK_LIVE, Left, NONE, None),
            (Nav, DOCK_PIP, Right, CTRL, None),
            // FOCO: ctrl+click (even on the window) is the app's
            (Focus, Hit::Client, Left, CTRL, None),
            (Focus, Hit::Client, Left, NONE, None),
            (Focus, Hit::Client, Left, WORLD, Some(G::Move)),
            (Focus, Hit::Client, Right, CTRL, None),
            (Focus, Hit::Client, Right, WORLD, Some(G::ResizeNearest)),
            (Focus, Hit::Outside, Left, WORLD, None),
            // CONVERSA/SALTO/COMANDOS/telefone: only world+ moves/resizes
            (Ui, SCREEN, Left, CTRL, None),
            (Ui, SCREEN, Left, WORLD, Some(G::Move)),
            (Ui, SCREEN, Right, WORLD, Some(G::ResizeNearest)),
            (Ui, Hit::Ui, Left, WORLD, None),
            // ANDAR com mira: segurar (não arrastar) carrega; nunca resize
            (WalkAim, SCREEN, Left, CTRL, Some(G::Carry)),
            (WalkAim, Hit::Pickable(3), Left, CTRL, Some(G::Carry)),
            (WalkAim, SCREEN, Left, NONE, None),
            (WalkAim, SCREEN, Right, WORLD, None),
        ];
        for (mode, hit, button, m, want) in rows.iter().copied() {
            assert_eq!(
                gesture_for(mode, hit, button, m, &c),
                want,
                "{mode:?} {hit:?} {button:?} {m:?}"
            );
        }
    }

    #[test]
    fn drag_off_disables_every_gesture() {
        let c = Config {
            drag: false,
            ..cfg()
        };
        assert_eq!(
            gesture_for(PtrMode::Nav, SCREEN, Button::Left, WORLD, &c),
            None
        );
        assert_eq!(
            gesture_for(PtrMode::Nav, HANDLE, Button::Left, NONE, &c),
            None
        );
        // with no gesture available, a plain click in FOCO still reaches the app
        assert!(passes(PtrMode::Focus, Hit::Client, Button::Left, NONE, &c));
    }

    #[test]
    fn drag_modifier_is_configurable() {
        let alt_cfg = Config {
            modifier: DragMod::Alt,
            ..cfg()
        };
        assert_eq!(
            gesture_for(PtrMode::Nav, SCREEN, Button::Left, ALT, &alt_cfg),
            Some(Gesture::Move)
        );
        assert_eq!(
            gesture_for(PtrMode::Nav, SCREEN, Button::Left, CTRL, &alt_cfg),
            None,
            "ctrl alone no longer moves"
        );
        // world+ still works whatever the configured modifier is
        assert_eq!(
            gesture_for(PtrMode::Nav, SCREEN, Button::Left, WORLD, &alt_cfg),
            Some(Gesture::Move)
        );
        let none_cfg = Config {
            modifier: DragMod::None,
            ..cfg()
        };
        assert_eq!(
            gesture_for(PtrMode::Nav, SCREEN, Button::Left, NONE, &none_cfg),
            Some(Gesture::Move),
            "no modifier at all: the bare drag moves"
        );
    }

    #[test]
    fn passes_table() {
        let c = cfg();
        // FOCO: no moving modifier → goes to the client
        assert!(passes(PtrMode::Focus, Hit::Client, Button::Left, CTRL, &c));
        assert!(passes(PtrMode::Focus, Hit::Client, Button::Right, NONE, &c));
        assert!(!passes(
            PtrMode::Focus,
            Hit::Client,
            Button::Left,
            WORLD,
            &c
        ));
        // a live (1×) dock: plain events reach its client; below 1× they select
        assert!(passes(PtrMode::Nav, DOCK_LIVE, Button::Left, NONE, &c));
        assert!(!passes(PtrMode::Nav, DOCK_LIVE, Button::Left, CTRL, &c));
        assert!(!passes(PtrMode::Nav, DOCK_PIP, Button::Left, NONE, &c));
        // the balloon/phone/dialogue always keep the event
        assert!(passes(PtrMode::Nav, Hit::Ui, Button::Left, NONE, &c));
        assert!(passes(PtrMode::Ui, Hit::Ui, Button::Left, WORLD, &c));
        // the dimmed world outside FOCO is the world's own click
        assert!(!passes(
            PtrMode::Focus,
            Hit::Outside,
            Button::Left,
            NONE,
            &c
        ));
        // NAV never passes a plain screen hit
        assert!(!passes(PtrMode::Nav, SCREEN, Button::Left, NONE, &c));
    }

    #[test]
    fn effective_strips_the_modifier_that_started_the_drag() {
        // ctrl+drag: ctrl held from the start never disables magnets...
        let eff = effective(CTRL, CTRL);
        assert!(!eff.no_magnets);
        assert!(!eff.strong);
        assert!(!eff.rotate);
        // ...until it is released and pressed again (latched bit cleared)
        let eff = effective(CTRL, NONE);
        assert!(eff.no_magnets);
        // world+drag (ctrl+alt): alt started it too, so it never rotates
        let eff = effective(WORLD, WORLD);
        assert!(!eff.rotate);
        assert!(!eff.no_magnets);
        // shift is never a start modifier: always live
        let eff = effective(mods(true, false, true, false), CTRL);
        assert!(eff.strong);
    }

    fn drag_cfg() -> Config {
        cfg()
    }

    #[test]
    fn click_vs_drag_by_the_threshold() {
        let c = drag_cfg();
        let mut m = Machine::new();
        // a press and release with no motion in between is a click
        assert_eq!(m.step(PtrEv::Mods(NONE), PtrMode::Nav, &c, 0.0), vec![]);
        assert_eq!(
            m.step(
                PtrEv::Press {
                    button: Button::Left,
                    at: Vec2::new(10.0, 10.0),
                    hit: SCREEN,
                },
                PtrMode::Nav,
                &c,
                0.0,
            ),
            vec![]
        );
        let out = m.step(
            PtrEv::Release {
                button: Button::Left,
                at: Vec2::new(11.0, 10.0),
            },
            PtrMode::Nav,
            &c,
            0.05,
        );
        assert_eq!(
            out,
            vec![PtrOut::Click {
                button: Button::Left,
                hit: SCREEN,
                mods: NONE,
                double: false,
            }]
        );
        assert_eq!(*m.state(), State::Idle);
    }

    #[test]
    fn a_double_click_within_the_window_is_flagged() {
        let c = drag_cfg();
        let mut m = Machine::new();
        let click = |m: &mut Machine, t: Time| {
            m.step(
                PtrEv::Press {
                    button: Button::Left,
                    at: Vec2::ZERO,
                    hit: SCREEN,
                },
                PtrMode::Nav,
                &c,
                t,
            );
            m.step(
                PtrEv::Release {
                    button: Button::Left,
                    at: Vec2::ZERO,
                },
                PtrMode::Nav,
                &c,
                t,
            )
        };
        let first = click(&mut m, 0.0);
        assert_eq!(
            first,
            vec![PtrOut::Click {
                button: Button::Left,
                hit: SCREEN,
                mods: NONE,
                double: false,
            }]
        );
        let second = click(&mut m, 0.2);
        let PtrOut::Click { double, .. } = second[0] else {
            panic!()
        };
        assert!(double, "within double_s");
        let third = click(&mut m, 1.0);
        let PtrOut::Click { double, .. } = third[0] else {
            panic!()
        };
        assert!(!double, "too slow");
    }

    #[test]
    fn crossing_the_threshold_with_ctrl_starts_a_move() {
        let c = drag_cfg();
        let mut m = Machine::new();
        m.step(PtrEv::Mods(CTRL), PtrMode::Nav, &c, 0.0);
        m.step(
            PtrEv::Press {
                button: Button::Left,
                at: Vec2::new(0.0, 0.0),
                hit: SCREEN,
            },
            PtrMode::Nav,
            &c,
            0.0,
        );
        let out = m.step(
            PtrEv::Motion {
                at: Vec2::new(0.0, 20.0),
                hit: SCREEN,
            },
            PtrMode::Nav,
            &c,
            0.1,
        );
        assert_eq!(
            out,
            vec![
                PtrOut::DragStart {
                    gesture: Gesture::Move,
                    hit: SCREEN,
                    at: Vec2::ZERO,
                },
                PtrOut::DragMove {
                    at: Vec2::new(0.0, 20.0),
                    eff: Effective::default(),
                },
            ]
        );
        assert!(matches!(m.state(), State::Drag { .. }));
        let end = m.step(
            PtrEv::Release {
                button: Button::Left,
                at: Vec2::new(0.0, 20.0),
            },
            PtrMode::Nav,
            &c,
            0.1,
        );
        assert_eq!(
            end,
            vec![PtrOut::DragEnd {
                at: Vec2::new(0.0, 20.0),
                eff: Effective::default(),
            }]
        );
        assert_eq!(*m.state(), State::Idle);
    }

    #[test]
    fn a_plain_press_past_the_threshold_without_a_gesture_is_nothing() {
        let c = drag_cfg();
        let mut m = Machine::new();
        m.step(
            PtrEv::Press {
                button: Button::Left,
                at: Vec2::ZERO,
                hit: SCREEN,
            },
            PtrMode::Nav,
            &c,
            0.0,
        );
        let out = m.step(
            PtrEv::Motion {
                at: Vec2::new(50.0, 0.0),
                hit: SCREEN,
            },
            PtrMode::Nav,
            &c,
            0.1,
        );
        assert_eq!(out, Vec::new());
        let out = m.step(
            PtrEv::Release {
                button: Button::Left,
                at: Vec2::new(50.0, 0.0),
            },
            PtrMode::Nav,
            &c,
            0.1,
        );
        assert_eq!(out, Vec::new(), "no click: it moved without a modifier");
    }

    #[test]
    fn carry_starts_at_once_with_no_pointer_motion() {
        let c = drag_cfg();
        let mut m = Machine::new();
        m.step(PtrEv::Mods(CTRL), PtrMode::WalkAim, &c, 0.0);
        let out = m.step(
            PtrEv::Press {
                button: Button::Left,
                at: Vec2::ZERO,
                hit: SCREEN,
            },
            PtrMode::WalkAim,
            &c,
            0.0,
        );
        assert_eq!(
            out,
            vec![PtrOut::DragStart {
                gesture: Gesture::Carry,
                hit: SCREEN,
                at: Vec2::ZERO,
            }]
        );
        assert!(matches!(m.state(), State::Drag { .. }));
    }

    #[test]
    fn releasing_and_repressing_the_latch_makes_it_count() {
        let c = drag_cfg();
        let mut m = Machine::new();
        m.step(PtrEv::Mods(CTRL), PtrMode::Nav, &c, 0.0);
        m.step(
            PtrEv::Press {
                button: Button::Left,
                at: Vec2::ZERO,
                hit: SCREEN,
            },
            PtrMode::Nav,
            &c,
            0.0,
        );
        m.step(
            PtrEv::Motion {
                at: Vec2::new(0.0, 10.0),
                hit: SCREEN,
            },
            PtrMode::Nav,
            &c,
            0.0,
        );
        // ctrl is still held: it started the drag, so it does not turn off magnets
        let still = m.step(
            PtrEv::Motion {
                at: Vec2::new(0.0, 11.0),
                hit: SCREEN,
            },
            PtrMode::Nav,
            &c,
            0.0,
        );
        let PtrOut::DragMove { eff, .. } = still[0] else {
            panic!()
        };
        assert!(!eff.no_magnets);
        // release ctrl, then press it again: now it counts
        m.step(PtrEv::Mods(NONE), PtrMode::Nav, &c, 0.0);
        m.step(PtrEv::Mods(CTRL), PtrMode::Nav, &c, 0.0);
        let now_off = m.step(
            PtrEv::Motion {
                at: Vec2::new(0.0, 12.0),
                hit: SCREEN,
            },
            PtrMode::Nav,
            &c,
            0.0,
        );
        let PtrOut::DragMove { eff, .. } = now_off[0] else {
            panic!()
        };
        assert!(eff.no_magnets, "pressed again: it counts now");
    }

    #[test]
    fn esc_cancels_a_drag_mode_change_commits_it() {
        let c = drag_cfg();
        let mut m = Machine::new();
        m.step(PtrEv::Mods(CTRL), PtrMode::Nav, &c, 0.0);
        m.step(
            PtrEv::Press {
                button: Button::Left,
                at: Vec2::ZERO,
                hit: SCREEN,
            },
            PtrMode::Nav,
            &c,
            0.0,
        );
        m.step(
            PtrEv::Motion {
                at: Vec2::new(0.0, 10.0),
                hit: SCREEN,
            },
            PtrMode::Nav,
            &c,
            0.0,
        );
        assert!(matches!(m.state(), State::Drag { .. }));
        let out = m.step(PtrEv::Cancel, PtrMode::Nav, &c, 0.0);
        assert_eq!(out, vec![PtrOut::DragCancel]);
        assert_eq!(*m.state(), State::Idle);

        // start again, then change mode: ends as a drop (DragEnd), not a cancel
        m.step(
            PtrEv::Press {
                button: Button::Left,
                at: Vec2::ZERO,
                hit: SCREEN,
            },
            PtrMode::Nav,
            &c,
            1.0,
        );
        m.step(
            PtrEv::Motion {
                at: Vec2::new(0.0, 10.0),
                hit: SCREEN,
            },
            PtrMode::Nav,
            &c,
            1.0,
        );
        let out = m.step(PtrEv::ModeChanged, PtrMode::Nav, &c, 1.0);
        assert_eq!(
            out,
            vec![PtrOut::DragEnd {
                at: Vec2::new(0.0, 10.0),
                eff: Effective::default(),
            }]
        );
        assert_eq!(*m.state(), State::Idle);
    }

    #[test]
    fn a_passed_press_releases_to_the_client_too() {
        let c = drag_cfg();
        let mut m = Machine::new();
        let out = m.step(
            PtrEv::Press {
                button: Button::Left,
                at: Vec2::ZERO,
                hit: Hit::Client,
            },
            PtrMode::Focus,
            &c,
            0.0,
        );
        assert_eq!(out, vec![PtrOut::PassPress(Button::Left)]);
        assert_eq!(
            *m.state(),
            State::Passing {
                button: Button::Left
            }
        );
        let out = m.step(
            PtrEv::Release {
                button: Button::Left,
                at: Vec2::new(3.0, 3.0),
            },
            PtrMode::Focus,
            &c,
            0.0,
        );
        assert_eq!(out, vec![PtrOut::PassRelease(Button::Left)]);
        assert_eq!(*m.state(), State::Idle);
    }

    #[test]
    fn mode_changed_releases_a_passed_button() {
        let c = drag_cfg();
        let mut m = Machine::new();
        m.step(
            PtrEv::Press {
                button: Button::Left,
                at: Vec2::ZERO,
                hit: Hit::Client,
            },
            PtrMode::Focus,
            &c,
            0.0,
        );
        let out = m.step(PtrEv::ModeChanged, PtrMode::Focus, &c, 0.0);
        assert_eq!(out, vec![PtrOut::PassRelease(Button::Left)]);
    }

    #[test]
    fn wheel_scales_with_alt_and_adjusts_depth_while_moving() {
        let c = drag_cfg();
        let mut m = Machine::new();
        // alt+wheel with nothing pressed: a direct scale
        m.step(PtrEv::Mods(ALT), PtrMode::Nav, &c, 0.0);
        let out = m.step(
            PtrEv::Wheel {
                steps: 1,
                hit: SCREEN,
            },
            PtrMode::Nav,
            &c,
            0.0,
        );
        assert_eq!(
            out,
            vec![PtrOut::Scale {
                hit: SCREEN,
                steps: 1
            }]
        );
        // no alt, nothing pressed: wheel does nothing by itself
        m.step(PtrEv::Mods(NONE), PtrMode::Nav, &c, 0.0);
        let out = m.step(
            PtrEv::Wheel {
                steps: 1,
                hit: SCREEN,
            },
            PtrMode::Nav,
            &c,
            0.0,
        );
        assert_eq!(out, Vec::new());
        // while moving a screen, the wheel steps its depth (or scales with alt)
        m.step(PtrEv::Mods(CTRL), PtrMode::Nav, &c, 0.0);
        m.step(
            PtrEv::Press {
                button: Button::Left,
                at: Vec2::ZERO,
                hit: SCREEN,
            },
            PtrMode::Nav,
            &c,
            0.0,
        );
        m.step(
            PtrEv::Motion {
                at: Vec2::new(0.0, 10.0),
                hit: SCREEN,
            },
            PtrMode::Nav,
            &c,
            0.0,
        );
        let out = m.step(
            PtrEv::Wheel {
                steps: -1,
                hit: SCREEN,
            },
            PtrMode::Nav,
            &c,
            0.0,
        );
        assert_eq!(
            out,
            vec![PtrOut::DragWheel {
                steps: -1,
                scale: false,
            }]
        );
    }
}
