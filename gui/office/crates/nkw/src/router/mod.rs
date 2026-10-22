//! Keys → what to do with them (§3.6, §3.12). P3 replaces this stub in wave
//! 2b with the keymap modes of S9; the stub is phase 0's key filter moved
//! here: in FOCO only `ctrl+]` and the `world+` chords are the world's,
//! everything else goes to the client; in every other mode all keys are the
//! world's.

use std::collections::BTreeMap;

use nkw_core::{Dir, Mode};
use serde_json::Value;

use crate::world::Cmd;

/// The modifiers of a key event.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct KeyMods {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub logo: bool,
}

/// One key event as the router sees it.
#[derive(Clone, Debug, PartialEq)]
pub struct KeyEv {
    /// evdev + 8
    pub code: u32,
    /// xkb keysym (modified)
    pub sym: u32,
    /// core key name, e.g. "ctrl+]", "shift+space", "enter"
    pub name: String,
    pub utf8: Option<String>,
    pub mods: KeyMods,
    pub pressed: bool,
}

/// What the router decided.
#[derive(Clone, Debug, PartialEq)]
pub enum Route {
    /// to the FOCO client / cat pane
    Forward,
    Swallow,
    /// first key of a sequence
    Pending,
    Command {
        name: String,
        args: BTreeMap<String, String>,
    },
    /// to `Overlay::key`
    Text(KeyEv),
}

/// What `world+` means: `ctrl+alt` nested, `super` in an own session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorldMod {
    CtrlAlt,
    Super,
}

impl WorldMod {
    pub fn name(self) -> &'static str {
        match self {
            WorldMod::CtrlAlt => "ctrl+alt",
            WorldMod::Super => "super",
        }
    }

    fn held(self, m: KeyMods) -> bool {
        match self {
            WorldMod::CtrlAlt => m.ctrl && m.alt,
            WorldMod::Super => m.logo,
        }
    }
}

pub struct Router {
    world_mod: WorldMod,
    focus_escape: String,
    bindings: Vec<nk_client::Binding>,
}

impl Router {
    /// Starts with the fallback table.
    pub fn new(world_mod: WorldMod, focus_escape: &str) -> Router {
        Router {
            world_mod,
            focus_escape: focus_escape.into(),
            bindings: Vec::new(),
        }
    }

    /// `/v1/keymap` replaces the fallback; empty = fallback. (The stub keeps
    /// them for `dump` and routes with the fallback.)
    pub fn set_bindings(&mut self, b: Vec<nk_client::Binding>) {
        self.bindings = b;
    }

    pub fn route(&mut self, mode: &Mode, sub: Option<&str>, ev: &KeyEv) -> Route {
        let _ = sub;
        match filter_with(mode, ev.mods, ev.sym, self.world_mod) {
            Verdict::Forward => Route::Forward,
            Verdict::Intercept(None) => Route::Swallow,
            Verdict::Intercept(Some(c)) => {
                let (name, args) = c.name();
                Route::Command {
                    name: name.into(),
                    args,
                }
            }
        }
    }

    pub fn dump(&self) -> Value {
        serde_json::json!({
            "stub": true,
            "world_mod": self.world_mod.name(),
            "focus_escape": self.focus_escape,
            "bindings": self.bindings.len(),
        })
    }
}

/// What the key filter decided.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// to the focused client (FOCO only)
    Forward,
    /// the world's; maybe a command
    Intercept(Option<Cmd>),
}

/// keysyms the filter needs (xkb values; avoids depending on the keymap here)
pub mod sym {
    pub const H: u32 = 0x68;
    pub const J: u32 = 0x6a;
    pub const K: u32 = 0x6b;
    pub const L: u32 = 0x6c;
    pub const I: u32 = 0x69;
    pub const W: u32 = 0x77;
    pub const UPPER_J: u32 = 0x4a;
    pub const UPPER_K: u32 = 0x4b;
    pub const BRACKETRIGHT: u32 = 0x5d;
    pub const RETURN: u32 = 0xff0d;
    pub const KP_ENTER: u32 = 0xff8d;
    pub const ESCAPE: u32 = 0xff1b;
    pub const LEFT: u32 = 0xff51;
    pub const UP: u32 = 0xff52;
    pub const RIGHT: u32 = 0xff53;
    pub const DOWN: u32 = 0xff54;
}

fn dir_of(s: u32) -> Option<Dir> {
    match s {
        sym::H | sym::LEFT => Some(Dir::Left),
        sym::L | sym::RIGHT => Some(Dir::Right),
        sym::K | sym::UP => Some(Dir::Up),
        sym::J | sym::DOWN => Some(Dir::Down),
        _ => None,
    }
}

/// The key filter (§3.6) with `world+` = `ctrl+alt`. `s`: the modified
/// keysym.
pub fn filter(mode: &Mode, m: KeyMods, s: u32) -> Verdict {
    filter_with(mode, m, s, WorldMod::CtrlAlt)
}

fn filter_with(mode: &Mode, m: KeyMods, s: u32, wm: WorldMod) -> Verdict {
    let world = wm.held(m);
    if mode.focus().is_some() {
        if world {
            let lower = if (0x41..=0x5a).contains(&s) {
                s | 0x20
            } else {
                s
            };
            return Verdict::Intercept(match lower {
                sym::W => Some(Cmd::Walk),
                _ => dir_of(lower).map(Cmd::Neighbour),
            });
        }
        if m.ctrl && s == sym::BRACKETRIGHT {
            return Verdict::Intercept(Some(Cmd::Back));
        }
        return Verdict::Forward;
    }
    if m.ctrl || m.alt || m.logo {
        return Verdict::Intercept(if world && s == sym::W {
            Some(Cmd::Walk)
        } else {
            None
        });
    }
    let walking = mode.walking();
    Verdict::Intercept(match s {
        sym::ESCAPE => Some(Cmd::Back),
        _ if walking => None,
        sym::W => Some(Cmd::Walk),
        sym::UPPER_J => Some(Cmd::RoomNext),
        sym::UPPER_K => Some(Cmd::RoomPrev),
        sym::RETURN | sym::KP_ENTER | sym::I => Some(Cmd::Focus),
        s => dir_of(s).map(Cmd::Move),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::World;
    use nkw_core::Base;

    fn nav() -> Mode {
        Mode::base(Base::Nav)
    }

    fn focus() -> Mode {
        let mut w = crate::render::tests::world(&[&[1]]);
        w.command(Cmd::Focus, 0.0);
        w.mode().clone()
    }

    fn walking() -> Mode {
        let mut w: World = crate::render::tests::world(&[&[1]]);
        w.command(Cmd::Walk, 0.0);
        w.mode().clone()
    }

    const NONE: KeyMods = KeyMods {
        ctrl: false,
        alt: false,
        shift: false,
        logo: false,
    };
    const CTRL: KeyMods = KeyMods { ctrl: true, ..NONE };
    const WORLD: KeyMods = KeyMods {
        ctrl: true,
        alt: true,
        ..NONE
    };
    const SHIFT: KeyMods = KeyMods {
        shift: true,
        ..NONE
    };
    const ALT: KeyMods = KeyMods { alt: true, ..NONE };
    const LOGO: KeyMods = KeyMods { logo: true, ..NONE };

    #[test]
    fn key_filter_table() {
        use Verdict::{Forward as F, Intercept as I};
        let cases: &[(Mode, KeyMods, u32, Verdict)] = &[
            // NAV: everything is the world's
            (nav(), NONE, sym::H, I(Some(Cmd::Move(Dir::Left)))),
            (nav(), NONE, sym::L, I(Some(Cmd::Move(Dir::Right)))),
            (nav(), NONE, sym::LEFT, I(Some(Cmd::Move(Dir::Left)))),
            (nav(), NONE, sym::J, I(Some(Cmd::Move(Dir::Down)))),
            (nav(), NONE, sym::K, I(Some(Cmd::Move(Dir::Up)))),
            (nav(), SHIFT, sym::UPPER_J, I(Some(Cmd::RoomNext))),
            (nav(), SHIFT, sym::UPPER_K, I(Some(Cmd::RoomPrev))),
            (nav(), NONE, sym::RETURN, I(Some(Cmd::Focus))),
            (nav(), NONE, sym::I, I(Some(Cmd::Focus))),
            (nav(), NONE, sym::W, I(Some(Cmd::Walk))),
            (nav(), NONE, sym::ESCAPE, I(Some(Cmd::Back))),
            (nav(), NONE, 0x61, I(None)),
            (nav(), CTRL, sym::BRACKETRIGHT, I(None)),
            (nav(), ALT, sym::H, I(None)),
            (nav(), WORLD, sym::W, I(Some(Cmd::Walk))),
            // FOCO: only ctrl+] and world+ are intercepted
            (focus(), NONE, sym::H, F),
            (focus(), NONE, sym::RETURN, F),
            (focus(), NONE, sym::ESCAPE, F),
            (focus(), SHIFT, sym::UPPER_J, F),
            (focus(), CTRL, 0x63, F),
            (focus(), ALT, sym::H, F),
            (focus(), LOGO, sym::H, F),
            (focus(), CTRL, sym::BRACKETRIGHT, I(Some(Cmd::Back))),
            (focus(), WORLD, sym::H, I(Some(Cmd::Neighbour(Dir::Left)))),
            (focus(), WORLD, sym::L, I(Some(Cmd::Neighbour(Dir::Right)))),
            (focus(), WORLD, sym::J, I(Some(Cmd::Neighbour(Dir::Down)))),
            (focus(), WORLD, sym::K, I(Some(Cmd::Neighbour(Dir::Up)))),
            (focus(), WORLD, sym::W, I(Some(Cmd::Walk))),
            (focus(), WORLD, 0x57, I(Some(Cmd::Walk))),
            (focus(), WORLD, 0x61, I(None)),
            (focus(), WORLD, sym::BRACKETRIGHT, I(None)),
            // ANDAR keeps only esc and world+w
            (walking(), NONE, sym::H, I(None)),
            (walking(), NONE, sym::W, I(None)),
            (walking(), WORLD, sym::W, I(Some(Cmd::Walk))),
            (walking(), NONE, sym::ESCAPE, I(Some(Cmd::Back))),
        ];
        for (mode, m, s, want) in cases {
            assert_eq!(filter(mode, *m, *s), *want, "{} {m:?} {s:#x}", mode.label());
        }
    }

    fn ev(sym: u32, mods: KeyMods) -> KeyEv {
        KeyEv {
            code: 0,
            sym,
            name: String::new(),
            utf8: None,
            mods,
            pressed: true,
        }
    }

    #[test]
    fn route_gives_the_filters_verdicts_by_name() {
        let mut r = Router::new(WorldMod::CtrlAlt, "ctrl+]");
        let cmd = |n: &str| Route::Command {
            name: n.into(),
            args: BTreeMap::new(),
        };
        assert_eq!(r.route(&nav(), None, &ev(sym::H, NONE)), cmd("world.left"));
        assert_eq!(r.route(&nav(), None, &ev(0x61, NONE)), Route::Swallow);
        assert_eq!(r.route(&focus(), None, &ev(sym::H, NONE)), Route::Forward);
        assert_eq!(
            r.route(&focus(), None, &ev(sym::BRACKETRIGHT, CTRL)),
            cmd("world.back")
        );
        assert_eq!(
            r.route(&focus(), None, &ev(sym::L, WORLD)),
            cmd("world.focus_right")
        );
        // every command the table can give resolves back to itself
        for (mode, m, s) in [
            (nav(), NONE, sym::J),
            (nav(), SHIFT, sym::UPPER_K),
            (nav(), NONE, sym::RETURN),
            (nav(), NONE, sym::W),
            (focus(), WORLD, sym::K),
        ] {
            let Verdict::Intercept(Some(c)) = filter(&mode, m, s) else {
                panic!("{s:#x}");
            };
            let Route::Command { name, args } = r.route(&mode, None, &ev(s, m)) else {
                panic!("{s:#x}");
            };
            assert_eq!(Cmd::from_name(&name, &args), Some(c), "{name}");
        }
    }

    #[test]
    fn super_is_the_world_key_in_an_own_session() {
        let mut r = Router::new(WorldMod::Super, "ctrl+]");
        assert_eq!(
            r.route(&focus(), None, &ev(sym::H, LOGO)),
            Route::Command {
                name: "world.focus_left".into(),
                args: BTreeMap::new()
            }
        );
        assert_eq!(r.route(&focus(), None, &ev(sym::H, WORLD)), Route::Forward);
        assert_eq!(r.route(&nav(), None, &ev(sym::H, LOGO)), Route::Swallow);
        r.set_bindings(vec![nk_client::Binding::default()]);
        let d = r.dump();
        assert_eq!(d["world_mod"], "super");
        assert_eq!(d["focus_escape"], "ctrl+]");
        assert_eq!(d["bindings"], 1);
    }
}
