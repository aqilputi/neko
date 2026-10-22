//! Keys → what to do with them (§3.6, §3.12, S9). Every key event is named
//! the core's way (`KeyEv.name`, built by the hub), looked up in the keymap
//! mode of the world mode along its chain (`names::chain`), with multi-key
//! sequences (`gg`) through `nk_cats::keys::Router::feed_in`:
//!
//! - the bindings are the core's (`/v1/keymap`, `Req::Keymap`) or, until
//!   they come, the S9 table ([`table::FALLBACK`]); any key with `world+`
//!   is global whatever mode it was registered in;
//! - FOCO: what the global layer and `world.focus` do not take goes to the
//!   window (`Forward`);
//! - SALTO, COMANDOS, the reply line and the map: printable keys are text
//!   (`Text`, for the overlay), the rest resolves in the mode;
//! - ANDAR's held keys (`w a s d q e`, arrows) are swallowed: the World
//!   walks with them itself.

pub mod names;
pub mod table;

use std::collections::{BTreeMap, HashSet};

use nk_cats::keys::{self as ck, Bind};
use nkw_core::Mode;
use serde_json::Value;

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
}

/// keysyms the World's walking keys use (xkb values)
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

/// ANDAR's held keys: the World reads them itself (`World::walk_key`).
const HELD: [&str; 8] = [
    "world.forward",
    "world.back",
    "world.strafe_left",
    "world.strafe_right",
    "world.turn_left",
    "world.turn_right",
    "world.look_up",
    "world.look_down",
];

/// A modifier key alone (Shift, Control, Alt, Super, AltGr …).
fn is_modifier(sym: u32) -> bool {
    (0xffe1..=0xffee).contains(&sym) || (0xfe01..=0xfe0f).contains(&sym)
}

pub struct Router {
    world_mod: WorldMod,
    focus_escape: String,
    /// what `/v1/keymap` sent (empty: the fallback is in use)
    keymap: Vec<nk_client::Binding>,
    /// the bindings in effect, keys canonical
    binds: Vec<Bind>,
    inner: ck::Router,
    /// the keymap mode of the last key (a mode change drops a sequence)
    last_mode: &'static str,
    routed: u64,
}

impl Router {
    /// Starts with the S9 fallback table.
    pub fn new(world_mod: WorldMod, focus_escape: &str) -> Router {
        let binds = fallback_binds(world_mod, focus_escape);
        Router {
            world_mod,
            focus_escape: focus_escape.into(),
            keymap: Vec::new(),
            inner: build(&binds),
            binds,
            last_mode: "normal.world",
            routed: 0,
        }
    }

    /// `/v1/keymap` replaces the fallback; empty = fallback. A world mode
    /// the keymap does not mention at all keeps its fallback rows.
    pub fn set_bindings(&mut self, b: Vec<nk_client::Binding>) {
        self.binds = if b.iter().all(|x| x.keys.is_empty()) {
            fallback_binds(self.world_mod, &self.focus_escape)
        } else {
            keymap_binds(&b, self.world_mod, &self.focus_escape)
        };
        self.keymap = b;
        self.inner = build(&self.binds);
    }

    pub fn route(&mut self, mode: &Mode, sub: Option<&str>, ev: &KeyEv) -> Route {
        let m = names::mode_of(mode, sub);
        let focus = mode.focus().is_some();
        if m != self.last_mode {
            self.inner.reset();
            self.last_mode = m;
        }
        if is_modifier(ev.sym) {
            return if focus {
                Route::Forward
            } else {
                Route::Swallow
            };
        }
        self.routed += 1;
        let text = names::is_text_mode(m);
        let printable = ev.utf8.is_some() && !ev.mods.ctrl && !ev.mods.alt && !ev.mods.logo;
        if text && printable {
            return Route::Text(ev.clone());
        }
        match self.inner.feed_in(m, &ev.name) {
            ck::Route::Core(c) | ck::Route::Unsupported(c) => {
                if (m == "world.talk" && names::is_client_command(&c))
                    || (m == "world.walk" && HELD.contains(&c.as_str()))
                {
                    return Route::Swallow;
                }
                Route::Command {
                    name: c,
                    args: BTreeMap::new(),
                }
            }
            ck::Route::Pending => Route::Pending,
            ck::Route::Client(_) => Route::Swallow,
            ck::Route::None if focus => Route::Forward,
            ck::Route::None if text => Route::Text(ev.clone()),
            ck::Route::None => Route::Swallow,
        }
    }

    /// The keys in effect for a command in a mode (for the palette's key
    /// column and the help).
    pub fn keys_of(&self, cmd: &str) -> Vec<String> {
        self.binds
            .iter()
            .filter(|b| b.command == cmd)
            .flat_map(|b| b.keys.iter().cloned())
            .collect()
    }

    pub fn dump(&self) -> Value {
        serde_json::json!({
            "world_mod": self.world_mod.name(),
            "focus_escape": self.focus_escape,
            "source": if self.keymap.is_empty() { "fallback" } else { "keymap" },
            "bindings": self.keymap.len(),
            "binds": self.binds.iter().map(|b| b.keys.len()).sum::<usize>(),
            "mode": self.last_mode,
            "pending": self.inner.pending(),
            "routed": self.routed,
        })
    }
}

/// The router over `binds`, with every world mode's chain known.
fn build(binds: &[Bind]) -> ck::Router {
    let mut all = binds.to_vec();
    // a mode without bindings still needs its chain (the global layer)
    for m in names::MODES.iter().chain(&["world"]) {
        all.push(Bind {
            command: String::new(),
            keys: Vec::new(),
            mode: m.to_string(),
            help: String::new(),
        });
    }
    ck::Router::with_modes(&all, &HashSet::new(), &names::chain)
}

/// One binding, its keys canonical; `world+` keys go to the global layer.
fn push(out: &mut Vec<Bind>, mode: &str, key: &str, cmd: &str, wm: WorldMod) {
    let mode = if names::is_global(key) { "world" } else { mode };
    let key = names::canonical(key, wm);
    match out.iter_mut().find(|b| b.mode == mode && b.command == cmd) {
        Some(b) => {
            if !b.keys.contains(&key) {
                b.keys.push(key);
            }
        }
        None => out.push(Bind {
            command: cmd.into(),
            keys: vec![key],
            mode: mode.into(),
            help: table::title(cmd).unwrap_or_default().into(),
        }),
    }
}

/// The S9 table, with `keys.focus_escape` as FOCO's way out.
pub fn fallback_binds(wm: WorldMod, focus_escape: &str) -> Vec<Bind> {
    let mut out = Vec::new();
    for (mode, key, cmd) in table::FALLBACK {
        let key = if *mode == "world.focus" && *cmd == "world.back" {
            focus_escape
        } else {
            key
        };
        push(&mut out, mode, key, cmd, wm);
    }
    out
}

/// The core's keymap; world modes it never mentions keep the fallback.
fn keymap_binds(b: &[nk_client::Binding], wm: WorldMod, focus_escape: &str) -> Vec<Bind> {
    let mut out = Vec::new();
    for x in b {
        for k in &x.keys {
            push(&mut out, &x.mode, k, &x.command, wm);
        }
    }
    let named: HashSet<&str> = b.iter().map(|x| x.mode.as_str()).collect();
    let global = b
        .iter()
        .any(|x| x.keys.iter().any(|k| names::is_global(k)) || x.mode == "world");
    for f in fallback_binds(wm, focus_escape) {
        let missing = if f.mode == "world" {
            !global
        } else {
            !named.contains(f.mode.as_str())
        };
        if missing {
            for k in &f.keys {
                // already canonical: push as is into the same mode
                match out
                    .iter_mut()
                    .find(|o: &&mut Bind| o.mode == f.mode && o.command == f.command)
                {
                    Some(o) => o.keys.push(k.clone()),
                    None => out.push(Bind {
                        keys: vec![k.clone()],
                        ..f.clone()
                    }),
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests;
