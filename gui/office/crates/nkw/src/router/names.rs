//! Key names and keymap modes (§3.12): how a binding's key is spelled so it
//! equals the `KeyEv.name` the hub builds (`crate::keys::key_name`), which
//! keymap mode a world mode is, and the order modes are looked up in.

use nkw_core::{Base, Kind, Mode};

use super::WorldMod;

/// The modifier prefixes in the hub's order.
const ORDER: [&str; 4] = ["ctrl", "alt", "super", "shift"];

/// A binding key in the core's spelling → the name the hub gives the key
/// event: `world+` becomes `ctrl+alt+` (or `super+`), modifiers go in the
/// order `ctrl+ alt+ super+ shift+`, `shift+` on a letter becomes the
/// capital (`world+shift+j` → `ctrl+alt+J`), `logo`/`meta`/`cmd` are
/// `super`, `return` is `enter`, `escape` is `esc`. Sequences (`gg`) and
/// bare names pass through.
pub fn canonical(key: &str, wm: WorldMod) -> String {
    if key == "+" || !key.contains('+') {
        return rename(key).to_string();
    }
    // the last part is the key (it may itself be "+": "ctrl++")
    let (mods, base) = match key.strip_suffix("++") {
        Some(m) => (m, "+"),
        None => match key.rsplit_once('+') {
            Some((m, b)) => (m, b),
            None => ("", key),
        },
    };
    let mut on = [false; 4];
    for m in mods.split('+') {
        match m {
            "world" => match wm {
                WorldMod::CtrlAlt => {
                    on[0] = true;
                    on[1] = true;
                }
                WorldMod::Super => on[2] = true,
            },
            "ctrl" | "control" => on[0] = true,
            "alt" | "mod1" => on[1] = true,
            "super" | "logo" | "meta" | "cmd" | "mod4" => on[2] = true,
            "shift" => on[3] = true,
            _ => {}
        }
    }
    let mut base = rename(base).to_string();
    let mut chars = base.chars();
    if on[3]
        && let (Some(c), None) = (chars.next(), chars.next())
        && c.is_alphabetic()
    {
        base = c.to_uppercase().collect();
        on[3] = false;
    }
    let mut s = String::new();
    for (i, p) in ORDER.iter().enumerate() {
        if on[i] {
            s.push_str(p);
            s.push('+');
        }
    }
    s + &base
}

fn rename(k: &str) -> &str {
    match k {
        "return" | "Return" | "ret" => "enter",
        "escape" | "Escape" => "esc",
        "Tab" => "tab",
        "BackSpace" => "backspace",
        "pageup" | "Prior" => "pgup",
        "pagedown" | "Next" => "pgdown",
        k => k,
    }
}

/// Whether a binding key belongs to the global layer (§3.12: any key with
/// `world+` is the world's, whatever mode it is registered in).
pub fn is_global(key: &str) -> bool {
    key.contains("world+")
}

/// The keymap mode the router resolves keys in, for this world mode and
/// plugin sub-mode (`Plugin::submode`: `Some("map")` while the map is open,
/// `Some("library")` while a shelf is browsed; the library only counts
/// with no overlay mode on, FOCO/CONVERSA/SALTO/COMANDOS win).
pub fn mode_of(mode: &Mode, sub: Option<&str>) -> &'static str {
    match sub {
        Some("map") => return "world.map",
        Some("library") if mode.overlay.is_none() => return "world.library",
        _ => {}
    }
    match (mode.kind(), mode.base) {
        (Some(Kind::Focus(_)), _) => "world.focus",
        (Some(Kind::Talk { line: true, .. }), _) => "world.line",
        (Some(Kind::Talk { .. }), _) => "world.talk",
        (Some(Kind::Hint { .. }), _) => "world.hint",
        (Some(Kind::Palette), _) => "world.palette",
        (None, Base::Walk) => "world.walk",
        (None, Base::Nav) => "normal.world",
    }
}

/// The sub-mode a keymap mode comes from (for the dump).
pub fn submode_of(keymap_mode: &str) -> Option<&'static str> {
    match keymap_mode {
        "world.map" => Some("map"),
        "world.library" => Some("library"),
        _ => None,
    }
}

/// The world modes the router knows.
pub const MODES: [&str; 9] = [
    "normal.world",
    "world.walk",
    "world.focus",
    "world.talk",
    "world.hint",
    "world.palette",
    "world.line",
    "world.map",
    "world.library",
];

/// Modes whose printable keys are text.
pub fn is_text_mode(mode: &str) -> bool {
    matches!(
        mode,
        "world.hint" | "world.palette" | "world.line" | "world.map"
    )
}

/// The resolution order of a mode (§3.12, S9): `world` first everywhere;
/// NAV falls to `normal` then `*`; ANDAR to `*`; FOCO has no fallback;
/// CONVERSA falls to `normal` (the caller keeps only core commands from
/// there); the library falls to NAV's chain; the text modes and the map are
/// only themselves.
pub fn chain(mode: &str) -> Vec<String> {
    let c: &[&str] = match mode {
        "normal.world" => &["world", "normal.world", "normal", "*"],
        "world.library" => &["world", "world.library", "normal.world", "normal", "*"],
        "world.walk" => &["world", "world.walk", "*"],
        "world.talk" => &["world", "world.talk", "normal"],
        "world" => &["world"],
        _ => return vec!["world".to_string(), mode.to_string()],
    };
    c.iter().map(|s| s.to_string()).collect()
}

/// A command the core's clients handle themselves (the TUI's navigation,
/// tabs, palette …): never inherited into CONVERSA from `normal`.
pub fn is_client_command(cmd: &str) -> bool {
    [
        "nav.",
        "tab.",
        "view.",
        "palette.",
        "help.",
        "switcher.",
        "pane.",
        "app.",
    ]
    .iter()
    .any(|p| cmd.starts_with(p))
        || matches!(
            cmd,
            "session.open" | "session.insert" | "session.preview" | "session.fullscreen"
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_spellings() {
        let ca = WorldMod::CtrlAlt;
        let rows = [
            ("h", "h"),
            ("J", "J"),
            ("gg", "gg"),
            ("enter", "enter"),
            ("Return", "enter"),
            ("escape", "esc"),
            ("+", "+"),
            ("ctrl+]", "ctrl+]"),
            ("ctrl+p", "ctrl+p"),
            ("shift+space", "shift+space"),
            ("shift+enter", "shift+enter"),
            ("world+w", "ctrl+alt+w"),
            ("world+shift+j", "ctrl+alt+J"),
            ("world+shift+e", "ctrl+alt+E"),
            ("world+enter", "ctrl+alt+enter"),
            ("world+space", "ctrl+alt+space"),
            ("alt+ctrl+x", "ctrl+alt+x"),
            ("shift+ctrl+tab", "ctrl+shift+tab"),
            ("logo+h", "super+h"),
            ("meta+h", "super+h"),
            ("ctrl++", "ctrl++"),
            ("shift+ç", "Ç"),
            ("shift+1", "shift+1"),
            ("XF86AudioPlay", "XF86AudioPlay"),
            ("f5", "f5"),
            ("ctrl+shift+f5", "ctrl+shift+f5"),
            ("pageup", "pgup"),
        ];
        for (k, want) in rows {
            assert_eq!(canonical(k, ca), want, "{k}");
        }
        assert_eq!(canonical("world+w", WorldMod::Super), "super+w");
        assert_eq!(canonical("world+shift+k", WorldMod::Super), "super+K");
        assert!(is_global("world+w") && !is_global("w"));
    }

    /// The hub names key events with `crate::keys::key_name`; a binding
    /// spelled the core's way must come out the same (30 keys).
    #[test]
    fn canonical_matches_the_hubs_key_names() {
        use crate::router::KeyMods;
        let m = |ctrl: bool, alt: bool, shift: bool| KeyMods {
            ctrl,
            alt,
            shift,
            logo: false,
        };
        let n = m(false, false, false);
        let rows: &[(u32, KeyMods, &str)] = &[
            (0x68, n, "h"),
            (0x6c, n, "l"),
            (0x4a, m(false, false, true), "J"),
            (0x4b, m(false, false, true), "K"),
            (0x47, m(false, false, true), "G"),
            (0x67, n, "g"),
            (0x31, n, "1"),
            (0x39, n, "9"),
            (0x2c, n, ","),
            (0x2e, n, "."),
            (0x2d, n, "-"),
            (0x3d, n, "="),
            (0x3a, m(false, false, true), ":"),
            (0x5b, n, "["),
            (0x5d, m(true, false, false), "ctrl+]"),
            (0x70, m(true, false, false), "ctrl+p"),
            (0x6e, m(true, false, false), "ctrl+n"),
            (0xff0d, n, "enter"),
            (0xff0d, m(false, false, true), "shift+enter"),
            (0xff1b, n, "esc"),
            (0x20, n, "space"),
            (0x20, m(false, false, true), "shift+space"),
            (0xff09, n, "tab"),
            (0xff08, n, "backspace"),
            (0xff52, n, "up"),
            (0xff54, n, "down"),
            (0xff51, n, "left"),
            (0x77, m(true, true, false), "world+w"),
            (0x4a, m(true, true, true), "world+shift+j"),
            (0x20, m(true, true, false), "world+space"),
            (0xff0d, m(true, true, false), "world+enter"),
            (0x1008ff14, n, "XF86AudioPlay"),
        ];
        for (sym, mods, binding) in rows {
            assert_eq!(
                crate::keys::key_name(*sym, *mods),
                canonical(binding, WorldMod::CtrlAlt),
                "{binding}"
            );
        }
    }

    #[test]
    fn modes_and_chains() {
        let nav = Mode::base(Base::Nav);
        assert_eq!(mode_of(&nav, None), "normal.world");
        assert_eq!(mode_of(&nav, Some("map")), "world.map");
        assert_eq!(mode_of(&Mode::base(Base::Walk), None), "world.walk");
        assert_eq!(
            chain("normal.world"),
            ["world", "normal.world", "normal", "*"]
        );
        assert_eq!(chain("world.walk"), ["world", "world.walk", "*"]);
        assert_eq!(
            chain("world.library"),
            ["world", "world.library", "normal.world", "normal", "*"]
        );
        assert_eq!(mode_of(&nav, Some("library")), "world.library");
        assert_eq!(
            mode_of(&Mode::base(Base::Walk), Some("library")),
            "world.library"
        );
        assert_eq!(mode_of(&nav, Some("other")), "normal.world");
        assert_eq!(submode_of("world.library"), Some("library"));
        assert_eq!(submode_of("world.map"), Some("map"));
        assert_eq!(submode_of("world.walk"), None);
        assert!(!is_text_mode("world.library"));
        assert_eq!(chain("world.focus"), ["world", "world.focus"]);
        assert_eq!(chain("world.talk"), ["world", "world.talk", "normal"]);
        assert_eq!(chain("world.hint"), ["world", "world.hint"]);
        assert_eq!(chain("world"), ["world"]);
        for m in MODES {
            assert_eq!(chain(m)[0], "world", "{m}");
        }
        assert!(is_text_mode("world.line") && !is_text_mode("world.talk"));
        assert!(is_client_command("nav.down") && !is_client_command("session.stop"));
    }
}
