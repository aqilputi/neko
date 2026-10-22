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

/// The world mode a keymap mode is typed in.
fn mode_for(keymap_mode: &str) -> Mode {
    match keymap_mode {
        "normal.world" | "world" => nav(),
        "world.walk" => walk(),
        "world.focus" => focus(),
        "world.talk" => talk(false),
        "world.line" => talk(true),
        "world.hint" => hint(),
        "world.palette" => palette(),
        m => panic!("{m}"),
    }
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

fn r() -> Router {
    Router::new(WorldMod::CtrlAlt, "ctrl+]")
}

#[test]
fn every_s9_row_resolves_in_its_mode() {
    for (mode, key, c) in table::FALLBACK {
        let mut router = r();
        let m = mode_for(mode);
        let name = names::canonical(key, WorldMod::CtrlAlt);
        let seq = nk_cats::keys::split_seq(&name);
        let mut got = Route::Swallow;
        for (i, k) in seq.iter().enumerate() {
            got = router.route(&m, None, &ev(k));
            if i + 1 < seq.len() {
                assert_eq!(got, Route::Pending, "{mode} {key}");
            }
        }
        let want = if *mode == "world.walk" && HELD.contains(c) {
            Route::Swallow
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
