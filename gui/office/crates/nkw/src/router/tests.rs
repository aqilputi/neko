use super::*;
use nkw_core::mode::Overlay as Over;
use nkw_core::{Base, CatId, Kind, Pose, ScreenId, TalkPurpose, Target};

fn over(kind: Kind, base: Base) -> Mode {
    Mode {
        base,
        overlay: Some(Over {
            kind,
            back: Box::new(Mode::base(base)),
            pose: Pose::default(),
        }),
    }
}

fn nav() -> Mode {
    Mode::base(Base::Nav)
}
fn walk() -> Mode {
    Mode::base(Base::Walk)
}
fn focus() -> Mode {
    over(Kind::Focus(Target::Screen(ScreenId(1))), Base::Nav)
}
fn cat_term() -> Mode {
    over(Kind::Focus(Target::CatTerm(CatId(1))), Base::Nav)
}
fn talk(line: bool) -> Mode {
    over(
        Kind::Talk {
            cat: CatId(1),
            line,
            purpose: TalkPurpose::Session,
        },
        Base::Nav,
    )
}
fn hint() -> Mode {
    over(Kind::Hint { focus: false }, Base::Nav)
}
fn palette() -> Mode {
    over(Kind::Palette, Base::Nav)
}

/// The world mode (and plugin sub-mode) a keymap mode is typed in.
fn mode_for(keymap_mode: &str) -> (Mode, Option<&'static str>) {
    let m = match keymap_mode {
        "normal.world" | "world" => nav(),
        "world.walk" => walk(),
        "world.focus" => focus(),
        "world.talk" => talk(false),
        "world.line" => talk(true),
        "world.hint" => hint(),
        "world.palette" => palette(),
        "world.library" => return (nav(), Some("library")),
        m => panic!("{m}"),
    };
    (m, None)
}

/// A key event as the hub would name it from a canonical name.
fn ev(name: &str) -> KeyEv {
    let mut mods = KeyMods::default();
    let mut base = name;
    loop {
        if let Some(r) = base.strip_prefix("ctrl+").filter(|r| !r.is_empty()) {
            mods.ctrl = true;
            base = r;
        } else if let Some(r) = base.strip_prefix("alt+").filter(|r| !r.is_empty()) {
            mods.alt = true;
            base = r;
        } else if let Some(r) = base.strip_prefix("super+").filter(|r| !r.is_empty()) {
            mods.logo = true;
            base = r;
        } else if let Some(r) = base.strip_prefix("shift+").filter(|r| !r.is_empty()) {
            mods.shift = true;
            base = r;
        } else {
            break;
        }
    }
    let mut cs = base.chars();
    let utf8 = match (cs.next(), cs.next()) {
        (Some(c), None) => Some(c.to_string()),
        _ if base == "space" => Some(" ".into()),
        _ => None,
    };
    if base.chars().count() == 1 && base.chars().all(char::is_uppercase) {
        mods.shift = true;
    }
    KeyEv {
        code: 0,
        sym: 0x61,
        name: name.into(),
        utf8,
        mods,
        pressed: true,
    }
}

fn cmd(n: &str) -> Route {
    Route::Command {
        name: n.into(),
        args: BTreeMap::new(),
    }
}

fn held(n: &str, pressed: bool) -> Route {
    Route::Command {
        name: n.into(),
        args: BTreeMap::from([("pressed".to_string(), pressed.to_string())]),
    }
}

fn up(name: &str) -> KeyEv {
    KeyEv {
        pressed: false,
        ..ev(name)
    }
}

fn library() -> Option<&'static str> {
    Some("library")
}

fn r() -> Router {
    Router::new(WorldMod::CtrlAlt, "ctrl+]")
}

#[test]
fn every_s9_row_resolves_in_its_mode() {
    for (mode, key, c) in table::FALLBACK {
        let mut router = r();
        let (m, sub) = mode_for(mode);
        let name = names::canonical(key, WorldMod::CtrlAlt);
        let seq = nk_cats::keys::split_seq(&name);
        let mut got = Route::Swallow;
        for (i, k) in seq.iter().enumerate() {
            got = router.route(&m, sub, &ev(k));
            if i + 1 < seq.len() {
                assert_eq!(got, Route::Pending, "{mode} {key}");
            }
        }
        let want = if *mode == "world.walk" && HELD_CMDS.contains(c) {
            held(c, true)
        } else {
            cmd(c)
        };
        assert_eq!(got, want, "{mode} {key} → {c}");
    }
}

#[test]
fn the_global_layer_works_in_every_mode() {
    for m in [
        nav(),
        walk(),
        focus(),
        cat_term(),
        talk(false),
        talk(true),
        hint(),
        palette(),
    ] {
        let mut router = r();
        assert_eq!(
            router.route(&m, None, &ev("ctrl+alt+l")),
            cmd("world.focus_right"),
            "{m:?}"
        );
        assert_eq!(
            router.route(&m, None, &ev("XF86AudioPlay")),
            cmd("media.play_pause")
        );
        // chords registered in normal.world are global too (S9)
        assert_eq!(router.route(&m, None, &ev("ctrl+alt+w")), cmd("world.walk"));
        assert_eq!(
            router.route(&m, None, &ev("ctrl+alt+J")),
            cmd("world.room_next")
        );
    }
    let mut router = r();
    assert_eq!(
        router.route(&nav(), Some("map"), &ev("ctrl+alt+o")),
        cmd("world.overview")
    );
}

#[test]
fn foco_forwards_what_is_not_the_worlds() {
    for m in [focus(), cat_term()] {
        let mut router = r();
        for k in [
            "h", "enter", "esc", "J", "ctrl+c", "alt+h", "super+h", "gg", "g", ":",
        ] {
            assert_eq!(router.route(&m, None, &ev(k)), Route::Forward, "{k}");
        }
        assert_eq!(router.route(&m, None, &ev("ctrl+]")), cmd("world.back"));
        assert_eq!(
            router.route(&m, None, &ev("ctrl+alt+h")),
            cmd("world.focus_left")
        );
        assert_eq!(router.route(&m, None, &ev("ctrl+alt+x")), Route::Forward);
    }
    // a modifier alone reaches the client in FOCO, is swallowed elsewhere
    let mut router = r();
    let mut shift = ev("Shift_L");
    shift.sym = 0xffe1;
    assert_eq!(router.route(&focus(), None, &shift), Route::Forward);
    assert_eq!(router.route(&nav(), None, &shift), Route::Swallow);
}

#[test]
fn text_only_in_text_modes() {
    let mut router = r();
    for m in [hint(), palette(), talk(true)] {
        for k in ["a", "J", ":", "ç", "1", "space"] {
            let e = ev(k);
            assert_eq!(
                router.route(&m, None, &e),
                Route::Text(e.clone()),
                "{m:?} {k}"
            );
        }
        // unbound non-printables are the overlay's too (arrows in a line …)
        let e = ev("home");
        assert_eq!(router.route(&m, None, &e), Route::Text(e.clone()));
    }
    assert_eq!(
        router.route(&palette(), None, &ev("ctrl+n")),
        cmd("world.palette_next")
    );
    assert_eq!(
        router.route(&talk(true), None, &ev("shift+enter")),
        cmd("world.line_newline")
    );
    assert_eq!(
        router.route(&hint(), None, &ev("backspace")),
        cmd("world.hint_backspace")
    );
    // NAV, ANDAR, CONVERSA: never text
    for m in [nav(), walk(), talk(false)] {
        assert_eq!(router.route(&m, None, &ev("z")), Route::Swallow, "{m:?}");
    }
    // the map: everything not global is the overlay's
    for k in ["h", "o", "enter", "esc"] {
        let e = ev(k);
        assert_eq!(
            router.route(&nav(), Some("map"), &e),
            Route::Text(e.clone())
        );
    }
}

#[test]
fn sequences_and_mode_changes() {
    let mut router = r();
    assert_eq!(router.route(&nav(), None, &ev("g")), Route::Pending);
    assert_eq!(router.route(&nav(), None, &ev("g")), cmd("world.first"));
    assert_eq!(router.route(&nav(), None, &ev("g")), Route::Pending);
    assert_eq!(router.dump()["pending"], serde_json::json!(["g"]));
    // a mode change drops the sequence
    assert_eq!(router.route(&walk(), None, &ev("g")), cmd("world.grab"));
    assert_eq!(router.route(&nav(), None, &ev("g")), Route::Pending);
    // a broken sequence retries the key alone
    assert_eq!(router.route(&nav(), None, &ev("h")), cmd("world.left"));
}

fn binding(mode: &str, keys: &[&str], command: &str) -> nk_client::Binding {
    nk_client::Binding {
        command: command.into(),
        keys: keys.iter().map(|k| k.to_string()).collect(),
        mode: mode.into(),
        ..Default::default()
    }
}

#[test]
fn set_bindings_overrides_and_world_keys_are_global() {
    let mut router = r();
    router.set_bindings(vec![
        binding("normal.world", &["n"], "world.right"),
        binding("normal.world", &["world+a"], "world.goto_room"),
        binding("normal", &["x"], "session.stop"),
        binding("normal", &["dd"], "session.remove"),
        binding("normal", &["j"], "nav.down"),
        binding("world.talk", &["space"], "world.talk_next"),
        binding("world.focus", &["ctrl+g"], "world.back"),
    ]);
    assert_eq!(router.dump()["source"], "keymap");
    assert_eq!(router.dump()["bindings"], 7);
    assert_eq!(router.route(&nav(), None, &ev("n")), cmd("world.right"));
    assert_eq!(
        router.route(&nav(), None, &ev("l")),
        Route::Swallow,
        "replaced"
    );
    assert_eq!(router.route(&nav(), None, &ev("x")), cmd("session.stop"));
    assert_eq!(
        router.route(&nav(), None, &ev("j")),
        cmd("nav.down"),
        "normal inherited"
    );
    assert_eq!(router.route(&nav(), None, &ev("d")), Route::Pending);
    assert_eq!(router.route(&nav(), None, &ev("d")), cmd("session.remove"));
    // world+ registered in normal.world is global, also in FOCO
    assert_eq!(
        router.route(&focus(), None, &ev("ctrl+alt+a")),
        cmd("world.goto_room")
    );
    assert_eq!(
        router.route(&focus(), None, &ev("ctrl+g")),
        cmd("world.back")
    );
    assert_eq!(
        router.route(&focus(), None, &ev("ctrl+]")),
        Route::Forward,
        "escape rebound"
    );
    // CONVERSA inherits normal's core commands only
    assert_eq!(
        router.route(&talk(false), None, &ev("x")),
        cmd("session.stop")
    );
    assert_eq!(
        router.route(&talk(false), None, &ev("j")),
        Route::Swallow,
        "nav.* is client"
    );
    // modes the keymap never names keep the fallback
    assert_eq!(
        router.route(&walk(), None, &ev("esc")),
        cmd("world.walk_exit")
    );
    assert_eq!(
        router.route(&palette(), None, &ev("enter")),
        cmd("world.palette_run")
    );
    // the global layer it never named keeps the fallback chords too
    assert_eq!(
        router.route(&focus(), None, &ev("ctrl+alt+l")),
        Route::Forward
    );
    // empty = fallback again
    router.set_bindings(Vec::new());
    assert_eq!(router.dump()["source"], "fallback");
    assert_eq!(router.route(&nav(), None, &ev("l")), cmd("world.right"));
}

#[test]
fn super_is_the_world_key_in_an_own_session() {
    let mut router = Router::new(WorldMod::Super, "ctrl+]");
    assert_eq!(
        router.route(&focus(), None, &ev("super+h")),
        cmd("world.focus_left")
    );
    assert_eq!(
        router.route(&focus(), None, &ev("ctrl+alt+h")),
        Route::Forward
    );
    assert_eq!(
        router.route(&nav(), None, &ev("super+K")),
        cmd("world.room_prev")
    );
    let d = router.dump();
    assert_eq!(d["world_mod"], "super");
    assert_eq!(d["focus_escape"], "ctrl+]");
    assert_eq!(d["bindings"], 0);
}

#[test]
fn focus_escape_is_configurable() {
    let mut router = Router::new(WorldMod::CtrlAlt, "ctrl+g");
    assert_eq!(
        router.route(&focus(), None, &ev("ctrl+g")),
        cmd("world.back")
    );
    assert_eq!(router.route(&focus(), None, &ev("ctrl+]")), Route::Forward);
    assert_eq!(router.keys_of("world.back"), ["s", "ctrl+g"]);
}

#[test]
fn names_resolve_to_world_commands() {
    // what the World runs today resolves back through Cmd::from_name
    let mut router = r();
    for (m, k) in [
        (nav(), "j"),
        (nav(), "K"),
        (nav(), "enter"),
        (nav(), "w"),
        (focus(), "ctrl+alt+k"),
    ] {
        let Route::Command { name, args } = router.route(&m, None, &ev(k)) else {
            panic!("{k}");
        };
        assert!(
            crate::world::Cmd::from_name(&name, &args).is_some(),
            "{name}"
        );
    }
}

/// ANDAR's held keys resolve through the keymap: a command with `pressed`
/// on the press and on the release; other releases are swallowed.
#[test]
fn held_keys_press_and_release_in_walk() {
    let rows: &[(&str, &str)] = &[
        ("w", "world.forward"),
        ("s", "world.back"),
        ("a", "world.strafe_left"),
        ("d", "world.strafe_right"),
        ("q", "world.turn_left"),
        ("left", "world.turn_left"),
        ("e", "world.turn_right"),
        ("right", "world.turn_right"),
        ("up", "world.look_up"),
        ("down", "world.look_down"),
    ];
    for (k, c) in rows {
        let mut router = r();
        assert_eq!(router.route(&walk(), None, &ev(k)), held(c, true), "{k}");
        assert_eq!(router.route(&walk(), None, &up(k)), held(c, false), "{k}");
    }
    let mut router = r();
    // not held: the press is a command, the release is swallowed
    for k in ["h", "g", "esc", "enter", "c", "[", "z"] {
        assert_eq!(router.route(&walk(), None, &up(k)), Route::Swallow, "{k}");
    }
    assert_eq!(router.route(&walk(), None, &ev("h")), cmd("world.prev"));
    // the release of a NAV key is swallowed (w enters ANDAR on the press)
    assert_eq!(router.route(&nav(), None, &ev("w")), cmd("world.walk"));
    assert_eq!(router.route(&nav(), None, &up("w")), Route::Swallow);
    // held commands hold nothing outside ANDAR: FOCO's way out stays a
    // plain command, its release is the world's
    let mut f = Router::new(WorldMod::CtrlAlt, "s");
    assert_eq!(f.route(&focus(), None, &ev("s")), cmd("world.back"));
    assert_eq!(f.route(&focus(), None, &up("s")), Route::Swallow);
    // releases of the client's keys reach the client
    assert_eq!(f.route(&focus(), None, &up("h")), Route::Forward);
    // a release never breaks a sequence in progress
    assert_eq!(router.route(&nav(), None, &ev("g")), Route::Pending);
    assert_eq!(router.route(&nav(), None, &up("g")), Route::Swallow);
    assert_eq!(router.route(&nav(), None, &ev("g")), cmd("world.first"));
    // a global chord's release is swallowed
    assert_eq!(
        router.route(&walk(), None, &up("ctrl+alt+w")),
        Route::Swallow
    );
    assert_eq!(HELD_CMDS.len(), 9);
    assert!(HELD_CMDS.contains(&"world.run"));
}

#[test]
fn rebinding_a_walking_key_moves_with_it() {
    let mut router = r();
    router.set_bindings(vec![
        binding("world.walk", &["up"], "world.forward"),
        binding("world.walk", &["r"], "world.run"),
        binding("world.walk", &["esc"], "world.walk_exit"),
    ]);
    assert_eq!(
        router.route(&walk(), None, &ev("up")),
        held("world.forward", true)
    );
    assert_eq!(
        router.route(&walk(), None, &up("up")),
        held("world.forward", false)
    );
    assert_eq!(
        router.route(&walk(), None, &ev("r")),
        held("world.run", true)
    );
    assert_eq!(
        router.route(&walk(), None, &up("r")),
        held("world.run", false)
    );
    // the old keys went with the mode's fallback
    assert_eq!(router.route(&walk(), None, &ev("w")), Route::Swallow);
    assert_eq!(router.route(&walk(), None, &up("w")), Route::Swallow);
    // and the World parses the held command back
    let Route::Command { name, args } = router.route(&walk(), None, &ev("up")) else {
        panic!()
    };
    assert!(
        crate::world::Cmd::from_name(&name, &args).is_some(),
        "{name}"
    );
}

/// `nk.keymap.set` with a function arrives as a `lua.key:` binding: a
/// command like any other, in the same chain.
#[test]
fn lua_key_bindings_resolve_as_commands() {
    let mut router = r();
    router.set_bindings(vec![
        binding("normal.world", &["n"], "lua.key:normal.world:n"),
        binding("normal", &["x"], "lua.key:normal:x"),
        binding("world.walk", &["v"], "lua.key:world.walk:v"),
        binding("world.walk", &["w"], "world.forward"),
        binding("world.talk", &["z"], "lua.key:world.talk:z"),
        binding("normal.world", &["world+n"], "lua.key:normal.world:world+n"),
    ]);
    assert_eq!(
        router.route(&nav(), None, &ev("n")),
        cmd("lua.key:normal.world:n")
    );
    assert_eq!(
        router.route(&nav(), None, &ev("x")),
        cmd("lua.key:normal:x")
    );
    assert_eq!(
        router.route(&walk(), None, &ev("v")),
        cmd("lua.key:world.walk:v")
    );
    assert_eq!(router.route(&walk(), None, &up("v")), Route::Swallow);
    assert_eq!(
        router.route(&talk(false), None, &ev("z")),
        cmd("lua.key:world.talk:z")
    );
    // CONVERSA inherits normal's Lua keys (they are not client commands)
    assert_eq!(
        router.route(&talk(false), None, &ev("x")),
        cmd("lua.key:normal:x")
    );
    // ANDAR does not fall to normal
    assert_eq!(router.route(&walk(), None, &ev("x")), Route::Swallow);
    // a world+ Lua key is global, also in FOCO
    assert_eq!(
        router.route(&focus(), None, &ev("ctrl+alt+n")),
        cmd("lua.key:normal.world:world+n")
    );
    assert_eq!(router.route(&focus(), None, &ev("n")), Route::Forward);
    // the library inherits NAV's Lua keys
    assert_eq!(
        router.route(&nav(), library(), &ev("n")),
        cmd("lua.key:normal.world:n")
    );
}

/// The library's shelf keys (S19) and its chain:
/// world.library → normal.world → normal → *.
#[test]
fn library_submode_and_chain() {
    let mut router = r();
    let rows: &[(&str, &str)] = &[
        ("h", "world.book_prev"),
        ("left", "world.book_prev"),
        ("l", "world.book_next"),
        ("right", "world.book_next"),
        ("j", "world.shelf_down"),
        ("down", "world.shelf_down"),
        ("k", "world.shelf_up"),
        ("up", "world.shelf_up"),
        ("enter", "world.book_open"),
        ("backspace", "world.shelf_back"),
        ("/", "world.library_search"),
        ("e", "world.book_edit"),
        ("p", "world.book_preview"),
        ("y", "world.book_copy_path"),
        ("esc", "world.library_leave"),
        // inherited from NAV
        ("t", "world.talk"),
        ("J", "world.room_next"),
        (":", "world.palette"),
        ("ctrl+alt+l", "world.focus_right"),
    ];
    for (k, c) in rows {
        assert_eq!(router.route(&nav(), library(), &ev(k)), cmd(c), "{k}");
    }
    assert_eq!(router.dump()["mode"], "world.library");
    assert_eq!(router.dump()["submode"], "library");
    // printable keys are not text in the library
    assert_eq!(router.route(&nav(), library(), &ev("z")), Route::Swallow);
    // the core's normal mode is in the chain
    router.set_bindings(vec![binding("normal", &["x"], "session.stop")]);
    assert_eq!(
        router.route(&nav(), library(), &ev("x")),
        cmd("session.stop")
    );
    assert_eq!(
        router.route(&nav(), library(), &ev("l")),
        cmd("world.book_next"),
        "a keymap without the library mode keeps its fallback rows"
    );
    // a keymap that names the mode replaces its rows
    router.set_bindings(vec![binding("world.library", &["n"], "world.book_next")]);
    assert_eq!(
        router.route(&nav(), library(), &ev("n")),
        cmd("world.book_next")
    );
    assert_eq!(
        router.route(&nav(), library(), &ev("l")),
        cmd("world.right")
    );
    // the sub-mode means nothing under an overlay mode (FOCO, CONVERSA …)
    assert_eq!(router.route(&focus(), library(), &ev("h")), Route::Forward);
    assert_eq!(
        router.route(&talk(false), library(), &ev("j")),
        cmd("world.choice_down")
    );
    assert_eq!(router.dump()["submode"], serde_json::Value::Null);
    // and back in plain NAV: NAV's keys
    assert_eq!(router.route(&nav(), None, &ev("l")), cmd("world.right"));
}

#[test]
fn dump_names_the_submode() {
    let mut router = r();
    assert_eq!(router.dump()["submode"], serde_json::Value::Null);
    router.route(&nav(), Some("map"), &ev("h"));
    assert_eq!(router.dump()["submode"], "map");
    assert_eq!(router.dump()["mode"], "world.map");
    router.route(&nav(), None, &ev("h"));
    assert_eq!(router.dump()["submode"], serde_json::Value::Null);
}
