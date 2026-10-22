//! The mode state machine (§3.3). Two base modes, NAV and ANDAR; on top of
//! them at most one overlay: FOCO, CONVERSA, SALTO or COMANDOS. Each overlay
//! remembers where to go back to: the mode it was opened from (which may
//! itself be an overlay, e.g. FOCO under a quick CONVERSA) and the camera
//! pose at that moment.
//!
//! Pure: `apply(action, camera)` changes the mode and returns the effects
//! for the `nkw` side (camera glides, keyboard focus, pointer lock …). The
//! effects that follow from *being* in a mode (keyboard focus in FOCO, the
//! frozen world, the locked pointer of ANDAR, the hint labels …) are derived
//! by comparing the mode before and after, so every path in and out of a
//! mode produces them alike.
//!
//! It also keeps the keys held across a mode change: a key already down
//! when a mode is entered only counts again once released (so the `w` that
//! enters ANDAR does not walk).

use std::collections::BTreeSet;

use crate::ids::{BookId, CatId, RoomId, ScreenId};
use crate::pose::Pose;
use crate::strip::Station;

/// The two base modes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Base {
    /// NAVEGAR: stations, single keys
    Nav,
    /// ANDAR: free first person
    Walk,
}

/// What FOCO is on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    /// a screen (a window …)
    Screen(ScreenId),
    /// a cat's terminal (opened or attached with `POST /v1/panes`)
    CatTerm(CatId),
}

/// The overlays.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// FOCO: the window in 1:1, raw keys go to it
    Focus(Target),
    /// CONVERSA with a cat; `line`: typing a reply
    Talk { cat: CatId, line: bool },
    /// SALTO; `focus`: `F`, a screen label goes straight to FOCO
    Hint { focus: bool },
    /// COMANDOS
    Palette,
}

/// An overlay and where it goes back to.
#[derive(Clone, Debug, PartialEq)]
pub struct Overlay {
    pub kind: Kind,
    /// the mode it was opened from
    pub back: Box<Mode>,
    /// the camera pose when it was opened
    pub pose: Pose,
}

/// The whole mode: a base and maybe an overlay.
#[derive(Clone, Debug, PartialEq)]
pub struct Mode {
    pub base: Base,
    pub overlay: Option<Overlay>,
}

/// What a label of SALTO points at.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Pick {
    /// a screen and the station it is seen from
    Screen {
        station: Station,
        screen: ScreenId,
    },
    Cat(CatId),
    Door(RoomId),
    Book(BookId),
}

/// What the user (or the world) asked for; the keymap turns keys into these.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Action {
    /// `enter`/`i` on a screen or a cat's desk, `interact` on a screen
    Focus(Target),
    /// `w` / `world+w`
    Walk,
    /// `t`, `interact` on a cat
    Talk(CatId),
    /// `shift+space` / `world+space`: the most urgent cat
    Summon(CatId),
    /// `f` / `F`
    Hint { focus: bool },
    /// `:` / `ctrl+p` / `world+p`
    Palette,
    /// `esc`, `q` in CONVERSA, `ctrl+]` in FOCO: go back
    Back,
    /// `world+h/j/k/l` in FOCO: the neighbour (resolved by the strip)
    Neighbour(Target),
    /// the window in FOCO closed
    WindowClosed,
    /// `i`/`r` in CONVERSA
    Reply,
    /// `enter` on the reply line
    Send,
    /// `space`/`enter` in CONVERSA
    TalkNext,
    /// `f` in CONVERSA: FOCO on the cat's terminal; back lands on its desk
    Sit { desk: Pose },
    /// a SALTO label was typed
    Pick(Pick),
    /// the palette ran a command (the caller then applies any mode change
    /// the command asks for)
    PaletteDone,
    /// a cat arrived, `confirm.requested` …
    Notice,
    /// xdg-activation with a valid token for this screen
    Activation(ScreenId),
}

/// What the `nkw` side must do.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Effect {
    /// glide to the target's 1:1 pose (≤ 260 ms); on arrival the quad
    /// becomes the 2D element
    GlideOneToOne(Target),
    /// lateral travelling to the neighbour's 1:1 pose
    TravelOneToOne(Target),
    /// glide back to a saved pose (the 2D element becomes a quad again)
    GlideTo(Pose),
    /// frame the cat's face (≤ 260 ms)
    FrameFace(CatId),
    /// open or attach the cat's pane, sized to the focus rectangle
    OpenPane(CatId),
    /// keyboard focus and `activated` to the target
    KeyboardEnter(Target),
    /// `wl_keyboard.leave`
    KeyboardLeave,
    /// close popups (`popup_done`)
    PopupsDone,
    /// stop the world pass (FOCO)
    Freeze,
    Unfreeze,
    /// lock the pointer (if `walk.mouse_look`), show the crosshair
    WalkStart,
    /// unlock the pointer, hide the crosshair
    WalkStop,
    /// apply `walk.on_exit` (see `walk_exit`)
    WalkExit,
    ShowHints {
        focus: bool,
    },
    HideHints,
    OpenPalette,
    ClosePalette,
    /// the reply line opens / closes
    OpenLine,
    CloseLine,
    /// send the reply line's text
    SendLine,
    /// advance the dialogue
    TalkNext,
    /// the cat turns on its chair to face you
    CatTurns(CatId),
    /// the cat goes back to work
    CatBackToWork(CatId),
    /// go to a station (NAV)
    GoTo(Station),
    GoToRoom(RoomId),
    OpenBook(BookId),
    /// select the next screen of the desk (its window closed)
    SelectNextOnDesk,
    /// select the screen's station, nothing else (activation in NAV)
    Select(ScreenId),
    /// a badge on the HUD; nothing steals the focus
    Badge,
}

impl Mode {
    pub fn base(base: Base) -> Mode {
        Mode {
            base,
            overlay: None,
        }
    }

    /// `kind` on top of `back`, opened with the camera at `pose`.
    fn over(kind: Kind, back: &Mode, pose: Pose) -> Mode {
        Mode {
            base: back.base,
            overlay: Some(Overlay {
                kind,
                back: Box::new(back.clone()),
                pose,
            }),
        }
    }

    pub fn kind(&self) -> Option<Kind> {
        self.overlay.as_ref().map(|o| o.kind)
    }

    /// The HUD name (§3.1).
    pub fn label(&self) -> &'static str {
        match (self.kind(), self.base) {
            (Some(Kind::Focus(_)), _) => "FOCO",
            (Some(Kind::Talk { .. }), _) => "CONVERSA",
            (Some(Kind::Hint { .. }), _) => "SALTO",
            (Some(Kind::Palette), _) => "COMANDOS",
            (None, Base::Nav) => "NAVEGAR",
            (None, Base::Walk) => "ANDAR",
        }
    }

    /// The window that has keyboard focus, if in FOCO.
    pub fn focus(&self) -> Option<Target> {
        match self.kind() {
            Some(Kind::Focus(t)) => Some(t),
            _ => None,
        }
    }

    /// Plain ANDAR: walking keys and the locked pointer are live.
    pub fn walking(&self) -> bool {
        self.base == Base::Walk && self.overlay.is_none()
    }

    fn hint(&self) -> Option<bool> {
        match self.kind() {
            Some(Kind::Hint { focus }) => Some(focus),
            _ => None,
        }
    }

    fn palette(&self) -> bool {
        self.kind() == Some(Kind::Palette)
    }

    fn talk(&self) -> Option<(CatId, bool)> {
        match self.kind() {
            Some(Kind::Talk { cat, line }) => Some((cat, line)),
            _ => None,
        }
    }

    fn line(&self) -> bool {
        matches!(self.talk(), Some((_, true)))
    }

    /// The same overlay with another kind (same way back).
    fn with_kind(&self, kind: Kind) -> Mode {
        let mut m = self.clone();
        if let Some(o) = m.overlay.as_mut() {
            o.kind = kind;
        }
        m
    }
}

/// Effects of leaving what `old` was and `new` is not.
fn leave(old: &Mode, new: &Mode, fx: &mut Vec<Effect>) {
    if old.focus().is_some() && old.focus() != new.focus() {
        fx.extend([Effect::KeyboardLeave, Effect::PopupsDone]);
    }
    if old.focus().is_some() && new.focus().is_none() {
        fx.push(Effect::Unfreeze);
    }
    if old.walking() && !new.walking() {
        fx.push(Effect::WalkStop);
    }
    if old.hint().is_some() && old.hint() != new.hint() {
        fx.push(Effect::HideHints);
    }
    if old.palette() && !new.palette() {
        fx.push(Effect::ClosePalette);
    }
    if old.line() && !new.line() {
        fx.push(Effect::CloseLine);
    }
    if let Some((cat, _)) = old.talk()
        && new.talk().map(|t| t.0) != Some(cat)
    {
        fx.push(Effect::CatBackToWork(cat));
    }
}

/// Effects of entering what `new` is and `old` was not.
fn enter(old: &Mode, new: &Mode, fx: &mut Vec<Effect>) {
    if new.walking() && !old.walking() {
        fx.push(Effect::WalkStart);
    }
    if let Some(focus) = new.hint()
        && old.hint() != new.hint()
    {
        fx.push(Effect::ShowHints { focus });
    }
    if new.palette() && !old.palette() {
        fx.push(Effect::OpenPalette);
    }
    if new.line() && !old.line() {
        fx.push(Effect::OpenLine);
    }
    if let Some((cat, _)) = new.talk()
        && old.talk().map(|t| t.0) != Some(cat)
    {
        fx.push(Effect::CatTurns(cat));
    }
    if new.focus().is_some() && old.focus().is_none() {
        fx.push(Effect::Freeze);
    }
    if let Some(t) = new.focus()
        && old.focus() != new.focus()
    {
        fx.push(Effect::KeyboardEnter(t));
    }
}

/// FOCO on `t` over `back`, opened from `pose`.
fn focus_on(t: Target, back: &Mode, pose: Pose, fx: &mut Vec<Effect>) -> Mode {
    if let Target::CatTerm(cat) = t {
        fx.push(Effect::OpenPane(cat));
    }
    fx.push(Effect::GlideOneToOne(t));
    Mode::over(Kind::Focus(t), back, pose)
}

/// Back to where the overlay came from, gliding to the saved pose.
fn go_back(o: &Overlay, fx: &mut Vec<Effect>) -> Mode {
    fx.push(Effect::GlideTo(o.pose));
    (*o.back).clone()
}

/// The machine: the mode and the keys held across mode changes.
#[derive(Clone, Debug)]
pub struct Machine {
    mode: Mode,
    pressed: BTreeSet<u32>,
    stale: BTreeSet<u32>,
}

impl Default for Machine {
    fn default() -> Machine {
        Machine::new()
    }
}

impl Machine {
    /// Starts in NAV.
    pub fn new() -> Machine {
        Machine {
            mode: Mode::base(Base::Nav),
            pressed: BTreeSet::new(),
            stale: BTreeSet::new(),
        }
    }

    pub fn mode(&self) -> &Mode {
        &self.mode
    }

    /// Apply an action with the camera currently at `cam` (saved when an
    /// overlay opens). Actions that do not apply in the current mode do
    /// nothing.
    pub fn apply(&mut self, act: Action, cam: Pose) -> Vec<Effect> {
        let mut moves = Vec::new();
        let Some(new) = self.next(act, cam, &mut moves) else {
            return moves;
        };
        let mut fx = Vec::new();
        leave(&self.mode, &new, &mut fx);
        fx.append(&mut moves);
        enter(&self.mode, &new, &mut fx);
        if new != self.mode {
            self.stale = self.pressed.clone();
        }
        self.mode = new;
        fx
    }

    /// The mode after `act` (`None`: unchanged), pushing the effects that
    /// are not derived from the change itself.
    fn next(&self, act: Action, cam: Pose, fx: &mut Vec<Effect>) -> Option<Mode> {
        use Action as A;
        let m = &self.mode;
        let o = m.overlay.as_ref();
        let new = match (o.map(|o| o.kind), act) {
            (_, A::Notice) => {
                fx.push(Effect::Badge);
                return None;
            }
            (None, A::Activation(s)) if m.base == Base::Nav => {
                fx.push(Effect::Select(s));
                return None;
            }
            (_, A::Activation(_)) => {
                fx.push(Effect::Badge);
                return None;
            }

            // NAV and ANDAR
            (None, A::Focus(t)) => focus_on(t, m, cam, fx),
            (None, A::Walk) if m.base == Base::Nav => Mode::base(Base::Walk),
            (None, A::Walk | A::Back) if m.base == Base::Walk => {
                fx.push(Effect::WalkExit);
                Mode::base(Base::Nav)
            }
            (None, A::Talk(cat)) => {
                fx.push(Effect::FrameFace(cat));
                Mode::over(Kind::Talk { cat, line: false }, m, cam)
            }
            (None | Some(Kind::Focus(_)), A::Summon(cat)) => {
                Mode::over(Kind::Talk { cat, line: false }, m, cam)
            }
            (None, A::Hint { focus }) => Mode::over(Kind::Hint { focus }, m, cam),
            (None | Some(Kind::Focus(_)), A::Palette) => Mode::over(Kind::Palette, m, cam),

            // FOCO
            (Some(Kind::Focus(_)), A::Back) => go_back(o?, fx),
            (Some(Kind::Focus(_)), A::Neighbour(t)) => {
                if let Target::CatTerm(cat) = t {
                    fx.push(Effect::OpenPane(cat));
                }
                fx.push(Effect::TravelOneToOne(t));
                m.with_kind(Kind::Focus(t))
            }
            (Some(Kind::Focus(_)), A::Walk) => Mode::base(Base::Walk),
            (Some(Kind::Focus(_)), A::WindowClosed) => {
                fx.push(Effect::SelectNextOnDesk);
                Mode::base(Base::Nav)
            }

            // CONVERSA
            (Some(Kind::Talk { cat, line: true }), A::Back) => {
                m.with_kind(Kind::Talk { cat, line: false })
            }
            (Some(Kind::Talk { cat, line: true }), A::Send) => {
                fx.push(Effect::SendLine);
                m.with_kind(Kind::Talk { cat, line: false })
            }
            (Some(Kind::Talk { cat, line: false }), A::Reply) => {
                m.with_kind(Kind::Talk { cat, line: true })
            }
            (Some(Kind::Talk { line: false, .. }), A::TalkNext) => {
                fx.push(Effect::TalkNext);
                return None;
            }
            (Some(Kind::Talk { cat, line: false }), A::Sit { desk }) => {
                focus_on(Target::CatTerm(cat), &Mode::base(Base::Nav), desk, fx)
            }
            (Some(Kind::Talk { line: false, .. }), A::Back) => go_back(o?, fx),

            // SALTO
            (Some(Kind::Hint { focus }), A::Pick(pick)) => {
                let o = o?;
                match pick {
                    Pick::Screen { screen, .. } if focus => {
                        focus_on(Target::Screen(screen), &o.back, o.pose, fx)
                    }
                    Pick::Screen { station, .. } => {
                        fx.push(Effect::GoTo(station));
                        Mode::base(Base::Nav)
                    }
                    Pick::Cat(cat) => {
                        fx.push(Effect::FrameFace(cat));
                        Mode::over(Kind::Talk { cat, line: false }, &o.back, o.pose)
                    }
                    Pick::Door(room) => {
                        fx.push(Effect::GoToRoom(room));
                        (*o.back).clone()
                    }
                    Pick::Book(book) => {
                        fx.push(Effect::OpenBook(book));
                        (*o.back).clone()
                    }
                }
            }
            (Some(Kind::Hint { .. }), A::Back) => go_back(o?, fx),

            // COMANDOS
            (Some(Kind::Palette), A::Back | A::PaletteDone) => go_back(o?, fx),

            _ => return None,
        };
        Some(new)
    }

    /// A key went down or up (any code: evdev, keysym …). Returns whether
    /// it counts: keys already held when the mode last changed are ignored,
    /// press, repeat and release, until they are released once.
    pub fn key(&mut self, code: u32, down: bool) -> bool {
        if down {
            if self.stale.contains(&code) {
                return false;
            }
            self.pressed.insert(code);
            true
        } else {
            self.pressed.remove(&code);
            !self.stale.remove(&code)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;

    const S1: Target = Target::Screen(ScreenId(1));
    const S2: Target = Target::Screen(ScreenId(2));
    const CAT: CatId = CatId(7);
    const URGENT: CatId = CatId(8);

    fn p(x: f32) -> Pose {
        Pose::new(Vec3::new(x, 1.5, 2.4), 0.0, 0.0)
    }

    /// Setup actions run with the camera at p(0); the action under test at
    /// p(1).
    fn run(setup: &[Action], act: Action) -> (Machine, Vec<Effect>) {
        let mut m = Machine::new();
        for a in setup {
            m.apply(*a, p(0.0));
        }
        let fx = m.apply(act, p(1.0));
        (m, fx)
    }

    use Action as A;
    use Effect as E;

    const IN_FOCUS: &[Action] = &[A::Focus(S1)];
    const IN_WALK: &[Action] = &[A::Walk];
    const IN_TALK: &[Action] = &[A::Talk(CAT)];

    /// (name, setup, action, HUD label after, base after, effects it must
    /// include)
    type Row = (
        &'static str,
        &'static [Action],
        Action,
        &'static str,
        Base,
        Vec<Effect>,
    );

    #[test]
    fn every_transition_of_the_table() {
        let desk = p(9.0);
        let station = Station::Air(ScreenId(5));
        #[rustfmt::skip]
        let rows: Vec<Row> = vec![
            ("nav enter screen", &[], A::Focus(S1), "FOCO", Base::Nav,
                vec![E::GlideOneToOne(S1), E::Freeze, E::KeyboardEnter(S1)]),
            ("nav enter cat desk", &[], A::Focus(Target::CatTerm(CAT)), "FOCO", Base::Nav,
                vec![E::OpenPane(CAT), E::GlideOneToOne(Target::CatTerm(CAT))]),
            ("nav w", &[], A::Walk, "ANDAR", Base::Walk, vec![E::WalkStart]),
            ("nav t", &[], A::Talk(CAT), "CONVERSA", Base::Nav,
                vec![E::FrameFace(CAT), E::CatTurns(CAT)]),
            ("nav summon", &[], A::Summon(URGENT), "CONVERSA", Base::Nav, vec![E::CatTurns(URGENT)]),
            ("walk summon", IN_WALK, A::Summon(URGENT), "CONVERSA", Base::Walk,
                vec![E::WalkStop, E::CatTurns(URGENT)]),
            ("nav f", &[], A::Hint { focus: false }, "SALTO", Base::Nav,
                vec![E::ShowHints { focus: false }]),
            ("walk F", IN_WALK, A::Hint { focus: true }, "SALTO", Base::Walk,
                vec![E::WalkStop, E::ShowHints { focus: true }]),
            ("nav palette", &[], A::Palette, "COMANDOS", Base::Nav, vec![E::OpenPalette]),
            ("walk palette", IN_WALK, A::Palette, "COMANDOS", Base::Walk, vec![E::OpenPalette]),
            ("focus palette", IN_FOCUS, A::Palette, "COMANDOS", Base::Nav,
                vec![E::KeyboardLeave, E::OpenPalette]),
            ("walk esc", IN_WALK, A::Back, "NAVEGAR", Base::Nav, vec![E::WalkStop, E::WalkExit]),
            ("walk world+w", IN_WALK, A::Walk, "NAVEGAR", Base::Nav, vec![E::WalkStop, E::WalkExit]),
            ("walk enter screen", IN_WALK, A::Focus(S1), "FOCO", Base::Walk,
                vec![E::WalkStop, E::GlideOneToOne(S1), E::KeyboardEnter(S1)]),
            ("walk talk", IN_WALK, A::Talk(CAT), "CONVERSA", Base::Walk,
                vec![E::WalkStop, E::FrameFace(CAT), E::CatTurns(CAT)]),
            ("focus ctrl+]", IN_FOCUS, A::Back, "NAVEGAR", Base::Nav,
                vec![E::KeyboardLeave, E::PopupsDone, E::Unfreeze, E::GlideTo(p(0.0))]),
            ("focus world+l", IN_FOCUS, A::Neighbour(S2), "FOCO", Base::Nav,
                vec![E::KeyboardLeave, E::TravelOneToOne(S2), E::KeyboardEnter(S2)]),
            ("focus world+space", IN_FOCUS, A::Summon(URGENT), "CONVERSA", Base::Nav,
                vec![E::KeyboardLeave, E::Unfreeze, E::CatTurns(URGENT)]),
            ("focus world+w", IN_FOCUS, A::Walk, "ANDAR", Base::Walk,
                vec![E::KeyboardLeave, E::Unfreeze, E::WalkStart]),
            ("focus closed", IN_FOCUS, A::WindowClosed, "NAVEGAR", Base::Nav,
                vec![E::KeyboardLeave, E::SelectNextOnDesk]),
            ("talk reply", IN_TALK, A::Reply, "CONVERSA", Base::Nav, vec![E::OpenLine]),
            ("talk next", IN_TALK, A::TalkNext, "CONVERSA", Base::Nav, vec![E::TalkNext]),
            ("talk sit", IN_TALK, A::Sit { desk }, "FOCO", Base::Nav,
                vec![E::CatBackToWork(CAT), E::OpenPane(CAT),
                     E::GlideOneToOne(Target::CatTerm(CAT)), E::KeyboardEnter(Target::CatTerm(CAT))]),
            ("talk esc", IN_TALK, A::Back, "NAVEGAR", Base::Nav,
                vec![E::CatBackToWork(CAT), E::GlideTo(p(0.0))]),
            ("hint pick screen", &[A::Hint { focus: false }],
                A::Pick(Pick::Screen { station, screen: ScreenId(5) }), "NAVEGAR", Base::Nav,
                vec![E::HideHints, E::GoTo(station)]),
            ("hint F pick screen", &[A::Hint { focus: true }],
                A::Pick(Pick::Screen { station, screen: ScreenId(5) }), "FOCO", Base::Nav,
                vec![E::HideHints, E::GlideOneToOne(Target::Screen(ScreenId(5)))]),
            ("hint pick cat", &[A::Hint { focus: false }], A::Pick(Pick::Cat(CAT)), "CONVERSA",
                Base::Nav, vec![E::HideHints, E::FrameFace(CAT), E::CatTurns(CAT)]),
            ("hint pick door", &[A::Walk, A::Hint { focus: false }], A::Pick(Pick::Door(RoomId(3))),
                "ANDAR", Base::Walk, vec![E::HideHints, E::GoToRoom(RoomId(3)), E::WalkStart]),
            ("hint pick book", &[A::Hint { focus: false }], A::Pick(Pick::Book(BookId(4))),
                "NAVEGAR", Base::Nav, vec![E::HideHints, E::OpenBook(BookId(4))]),
            ("hint esc", &[A::Hint { focus: false }], A::Back, "NAVEGAR", Base::Nav,
                vec![E::HideHints]),
            ("palette esc", &[A::Palette], A::Back, "NAVEGAR", Base::Nav, vec![E::ClosePalette]),
            ("palette done", &[A::Walk, A::Palette], A::PaletteDone, "ANDAR", Base::Walk,
                vec![E::ClosePalette, E::WalkStart]),
            ("notice in nav", &[], A::Notice, "NAVEGAR", Base::Nav, vec![E::Badge]),
            ("notice in focus", IN_FOCUS, A::Notice, "FOCO", Base::Nav, vec![E::Badge]),
            ("activation in nav", &[], A::Activation(ScreenId(3)), "NAVEGAR", Base::Nav,
                vec![E::Select(ScreenId(3))]),
            ("activation in focus", IN_FOCUS, A::Activation(ScreenId(3)), "FOCO", Base::Nav,
                vec![E::Badge]),
        ];
        for (name, setup, act, label, base, want) in rows {
            let (m, fx) = run(setup, act);
            assert_eq!(m.mode().label(), label, "{name}: {fx:?}");
            assert_eq!(m.mode().base, base, "{name}");
            for w in &want {
                assert!(fx.contains(w), "{name}: missing {w:?} in {fx:?}");
            }
        }
    }

    #[test]
    fn effects_come_in_order_leave_then_move_then_enter() {
        let (_, fx) = run(IN_FOCUS, A::Neighbour(S2));
        assert_eq!(
            fx,
            [
                E::KeyboardLeave,
                E::PopupsDone,
                E::TravelOneToOne(S2),
                E::KeyboardEnter(S2)
            ],
            "travelling keeps the world frozen"
        );
        let (_, fx) = run(&[], A::Focus(S1));
        assert_eq!(fx, [E::GlideOneToOne(S1), E::Freeze, E::KeyboardEnter(S1)]);
    }

    #[test]
    fn actions_out_of_place_do_nothing() {
        let rows: Vec<(&[Action], Action)> = vec![
            (&[], A::Back),
            (&[], A::Neighbour(S2)),
            (&[], A::WindowClosed),
            (&[], A::Reply),
            (&[], A::Pick(Pick::Cat(CAT))),
            (IN_WALK, A::Neighbour(S2)),
            (IN_FOCUS, A::Hint { focus: false }),
            (IN_FOCUS, A::Talk(CAT)),
            (IN_FOCUS, A::Focus(S2)),
            (IN_TALK, A::Palette),
            (IN_TALK, A::Walk),
            (&[A::Hint { focus: false }], A::Walk),
            (&[A::Palette], A::Focus(S1)),
            (&[A::Palette], A::Pick(Pick::Cat(CAT))),
        ];
        for (setup, act) in rows {
            let mut before = Machine::new();
            for a in setup {
                before.apply(*a, p(0.0));
            }
            let (m, fx) = run(setup, act);
            assert_eq!(m.mode(), before.mode(), "{setup:?} then {act:?}");
            assert!(fx.is_empty(), "{setup:?} then {act:?}: {fx:?}");
        }
    }

    #[test]
    fn focus_from_walk_returns_to_walk_at_the_same_pose() {
        let mut m = Machine::new();
        m.apply(A::Walk, p(0.0));
        m.apply(A::Focus(S1), p(3.0));
        let fx = m.apply(A::Back, p(4.0));
        assert_eq!(m.mode(), &Mode::base(Base::Walk));
        assert!(fx.contains(&E::GlideTo(p(3.0))));
        assert!(fx.contains(&E::WalkStart));
        assert!(!fx.contains(&E::WalkExit));
    }

    #[test]
    fn a_quick_talk_from_focus_comes_back_to_focus() {
        let mut m = Machine::new();
        m.apply(A::Focus(S1), p(0.0));
        m.apply(A::Summon(URGENT), p(1.0));
        let fx = m.apply(A::Back, p(1.0));
        assert_eq!(m.mode().kind(), Some(Kind::Focus(S1)));
        assert!(fx.contains(&E::KeyboardEnter(S1)) && fx.contains(&E::Freeze));
        assert!(fx.contains(&E::CatBackToWork(URGENT)));
        assert!(fx.contains(&E::GlideTo(p(1.0))));
        // and FOCO still remembers its own way back
        let fx = m.apply(A::Back, p(1.0));
        assert_eq!(m.mode(), &Mode::base(Base::Nav));
        assert!(fx.contains(&E::GlideTo(p(0.0))));
    }

    #[test]
    fn palette_over_focus_returns_to_focus() {
        let mut m = Machine::new();
        m.apply(A::Focus(S1), p(0.0));
        m.apply(A::Palette, p(1.0));
        assert_eq!(m.mode().focus(), None, "the palette has the keyboard");
        let fx = m.apply(A::PaletteDone, p(1.0));
        assert_eq!(m.mode().focus(), Some(S1));
        assert!(fx.contains(&E::KeyboardEnter(S1)));
    }

    #[test]
    fn sitting_down_lands_on_the_cats_desk_in_nav() {
        let mut m = Machine::new();
        m.apply(A::Walk, p(0.0));
        m.apply(A::Talk(CAT), p(1.0));
        m.apply(A::Sit { desk: p(9.0) }, p(1.0));
        assert_eq!(m.mode().kind(), Some(Kind::Focus(Target::CatTerm(CAT))));
        let fx = m.apply(A::Back, p(2.0));
        assert_eq!(m.mode(), &Mode::base(Base::Nav), "NAV, not back to ANDAR");
        assert!(fx.contains(&E::GlideTo(p(9.0))));
        assert!(!fx.contains(&E::WalkStart));
    }

    #[test]
    fn the_reply_line_takes_esc_and_enter() {
        let mut m = Machine::new();
        m.apply(A::Talk(CAT), p(0.0));
        m.apply(A::Reply, p(0.0));
        assert_eq!(
            m.mode().kind(),
            Some(Kind::Talk {
                cat: CAT,
                line: true
            })
        );
        assert!(
            m.apply(A::TalkNext, p(0.0)).is_empty(),
            "keys are text on the line"
        );
        let fx = m.apply(A::Back, p(0.0));
        assert_eq!(fx, [E::CloseLine], "esc cancels the line, not the talk");
        m.apply(A::Reply, p(0.0));
        let fx = m.apply(A::Send, p(0.0));
        assert_eq!(fx, [E::CloseLine, E::SendLine]);
        assert_eq!(m.mode().label(), "CONVERSA");
        assert!(
            m.apply(A::Send, p(0.0)).is_empty(),
            "no line, nothing to send"
        );
    }

    #[test]
    fn hint_pick_from_walk_goes_to_nav_station_and_stops_walking() {
        let mut m = Machine::new();
        m.apply(A::Walk, p(0.0));
        m.apply(A::Hint { focus: false }, p(0.0));
        let st = Station::Board;
        let fx = m.apply(
            A::Pick(Pick::Screen {
                station: st,
                screen: ScreenId(1),
            }),
            p(0.0),
        );
        assert_eq!(m.mode(), &Mode::base(Base::Nav));
        assert_eq!(fx, [E::HideHints, E::GoTo(st)]);
    }

    #[test]
    fn labels_and_helpers() {
        assert_eq!(Mode::base(Base::Nav).label(), "NAVEGAR");
        assert_eq!(Mode::base(Base::Walk).label(), "ANDAR");
        assert!(Mode::base(Base::Walk).walking());
        assert!(!Mode::base(Base::Nav).walking());
        let (m, _) = run(&[A::Walk], A::Palette);
        assert!(!m.mode().walking(), "an overlay over ANDAR is not walking");
        assert_eq!(Machine::default().mode(), Machine::new().mode());
    }

    #[test]
    fn keys_held_when_entering_a_mode_are_ignored_until_released() {
        const W: u32 = 17;
        const SHIFT: u32 = 42;
        const A_KEY: u32 = 30;
        let mut m = Machine::new();
        assert!(m.key(SHIFT, true));
        assert!(m.key(W, true), "w in NAV is the command itself");
        m.apply(A::Walk, p(0.0));
        assert!(!m.key(W, true), "repeat of the w that entered ANDAR");
        assert!(!m.key(SHIFT, true));
        assert!(m.key(A_KEY, true), "pressed after entering: counts");
        assert!(!m.key(W, false), "its release is swallowed too");
        assert!(m.key(W, true), "pressed again: now it walks");
        assert!(!m.key(SHIFT, false));
        assert!(m.key(A_KEY, false));
        // no mode change: nothing becomes stale
        m.apply(A::Notice, p(0.0));
        assert!(m.key(W, true));
        // a key that was released while stale and the mode changes again
        m.apply(A::Back, p(0.0));
        assert!(!m.key(W, false));
        assert!(m.key(W, true));
    }
}
