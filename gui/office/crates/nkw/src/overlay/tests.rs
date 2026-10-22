use super::*;
use crate::ext::{Badge, Tag};
use crate::world::{Cmd, World};
use nkw_core::{CatId, RoomId, ScreenId, Station};

fn world() -> World {
    crate::render::tests::world(&[&[1, 2]])
}

fn with<R>(w: &World, now: Time, f: impl FnOnce(&Ctx) -> R) -> R {
    let pids = BTreeMap::new();
    let vis = [RoomId(0)];
    let ctx = Ctx {
        world: w,
        now,
        frame: 0,
        reduced_motion: false,
        demo: true,
        scale: 1.0,
        size_px: [1280, 720],
        pids: &pids,
        visible_rooms: &vis,
        line: None,
    };
    f(&ctx)
}

fn hud(w: &World) -> HudInput {
    HudInput {
        mode_label: w.mode().label(),
        badges: Vec::new(),
        dialog: None,
        focus_rect: None,
        tags: Vec::new(),
        pickables: Vec::new(),
    }
}

fn ov() -> Overlay {
    Overlay::new(&crate::cli::Cli::default())
}

fn ids(els: &[OverlayElem]) -> Vec<u64> {
    els.iter().map(|e| e.id).collect()
}

fn key(name: &str) -> KeyEv {
    let mut cs = name.chars();
    let utf8 = match (cs.next(), cs.next()) {
        (Some(c), None) => Some(c.to_string()),
        _ => None,
    };
    KeyEv {
        code: 0,
        sym: 0,
        name: name.into(),
        utf8,
        mods: Default::default(),
        pressed: true,
    }
}

/// Apply what a hook asked of the machine (the hub's `apply_reqs`, for
/// the actions) and hand the overlay its effects (the hub's routing).
fn apply(o: &mut Overlay, w: &mut World, reqs: Vec<Req>) -> Vec<Req> {
    let mut rest = Vec::new();
    for r in reqs {
        match r {
            Req::Action(a) => {
                for out in w.action(a, 0.0) {
                    if let crate::world::Out::Effect(e) = out {
                        with(w, 0.0, |c| o.effect(&e, c));
                    }
                }
            }
            other => rest.push(other),
        }
    }
    rest
}

#[test]
fn hud_element_count_by_state() {
    let w = world();
    let mut o = ov();
    let mut h = hud(&w);
    let els = with(&w, 0.0, |c| o.elements(c, &h));
    assert_eq!(ids(&els), [LABEL_ID]);
    assert_eq!(els[0].pos, LABEL_POS);
    h.badges = vec![Badge::Inbox(3), Badge::BatteryLow(12)];
    let els = with(&w, 0.0, |c| o.elements(c, &h));
    assert_eq!(ids(&els), [LABEL_ID, CHIP_ID, CHIP_ID + 1]);
    assert!(els[1].pos.0 > els[0].pos.0, "chips right of the label");
    // FOCO: no label; what would cover the window is left out
    // the window starts where the second chip would be
    let x2 = els[2].pos.0 + 1;
    h.focus_rect = Some([x2, 0, 1280 - x2, 720]);
    let els = with(&w, 0.0, |c| o.elements(c, &h));
    assert_eq!(
        ids(&els),
        [CHIP_ID],
        "the first chip fits left of the window"
    );
    assert_eq!(o.dump()["badges"], 1);
}

#[test]
fn rasters_only_on_change() {
    let w = world();
    let mut o = ov();
    let mut h = hud(&w);
    h.badges = vec![Badge::Inbox(1)];
    with(&w, 0.0, |c| o.elements(c, &h));
    let made = o.dump()["rasters"].as_u64().unwrap();
    for _ in 0..5 {
        with(&w, 0.0, |c| o.elements(c, &h));
    }
    assert_eq!(o.dump()["rasters"], made, "same HUD: no new raster");
    assert!(o.dump()["hits"].as_u64().unwrap() >= 5);
    h.badges = vec![Badge::Inbox(2)];
    with(&w, 0.0, |c| o.elements(c, &h));
    assert_eq!(o.dump()["rasters"], made + 1, "only the changed chip");
}

#[test]
fn no_wish_at_rest_toasts_wake_at_their_end() {
    let w = world();
    let mut o = ov();
    let h = hud(&w);
    with(&w, 0.0, |c| o.elements(c, &h));
    assert!(
        with(&w, 0.0, |c| o.wishes(c)).is_empty(),
        "at rest: nothing"
    );
    o.toast("salvo", ToastKind::Info, 0, 10.0);
    let wish = with(&w, 10.0, |c| o.wishes(c));
    assert!(wish.contains(&Wish::compose(Want::Once)));
    assert!(
        wish.contains(&Wish::compose(Want::Continuous)),
        "sliding in"
    );
    let els = with(&w, 10.0, |c| o.elements(c, &h));
    assert!(ids(&els).contains(&TOAST_ID));
    let wish = with(&w, 11.0, |c| o.wishes(c));
    assert_eq!(wish, [Wish::compose(Want::At(13.0))]);
    let wish = with(&w, 13.05, |c| o.wishes(c));
    assert_eq!(wish, [Wish::compose(Want::Continuous)], "sliding out");
    with(&w, 13.3, |c| o.tick(c));
    assert_eq!(o.dump()["toasts"], 0);
    for t in [0.0, 10.0, 11.0, 13.05] {
        assert!(
            with(&w, t, |c| o.wishes(c))
                .iter()
                .all(|x| !matches!(x.want, Want::Rate(_))),
            "never a Rate wish"
        );
    }
    // reduced motion: no slides
    o.toast("ok", ToastKind::Error, 0, 20.0);
    let pids = BTreeMap::new();
    let ctx = Ctx {
        world: &w,
        now: 20.0,
        frame: 0,
        reduced_motion: true,
        demo: true,
        scale: 1.0,
        size_px: [1280, 720],
        pids: &pids,
        visible_rooms: &[],
        line: None,
    };
    assert!(!o.wishes(&ctx).contains(&Wish::compose(Want::Continuous)));
}

#[test]
fn salto_labels_and_a_pick() {
    let mut w = world();
    let mut o = ov();
    let r = with(&w, 0.0, |c| {
        o.command("world.hint_focus", &BTreeMap::new(), c)
    })
    .unwrap();
    assert!(apply(&mut o, &mut w, r).is_empty());
    assert_eq!(w.mode().label(), "SALTO");
    let mut h = hud(&w);
    let pick = |s: u32| {
        let (r, d, k) = w.locate(ScreenId(s)).unwrap();
        Pickable {
            pick: Pick::Screen {
                station: Station::Desk {
                    room: RoomId(0),
                    desk: d,
                    slot: k,
                },
                screen: ScreenId(s),
            },
            pos: w.screen_place(r, d, k).center,
            radius: 0.3,
        }
    };
    h.pickables = vec![pick(1), pick(2)];
    let els = with(&w, 0.0, |c| o.elements(c, &h));
    assert_eq!(o.dump()["hints"]["count"], 2);
    assert_eq!(
        els.iter()
            .filter(|e| e.id >= HINT_ID && e.id < TAG_ID)
            .count(),
        2
    );
    let labels: Vec<String> = o.dump()["hints"]["labels"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    // a wrong letter clears, the right label picks
    assert!(with(&w, 0.0, |c| o.key(&key("z"), c)).is_empty());
    let first = labels[0].clone();
    let r = with(&w, 0.0, |c| o.key(&key(&first), c));
    assert!(matches!(
        r.as_slice(),
        [Req::Action(Action::Pick(Pick::Screen { .. }))]
    ));
    apply(&mut o, &mut w, r);
    assert_eq!(w.mode().label(), "FOCO");
    with(&w, 0.0, |c| o.tick(c));
    assert_eq!(o.dump()["hints"], Value::Null);
}

#[test]
fn palette_ranks_runs_and_asks_args() {
    let mut w = world();
    let mut o = ov();
    let r = with(&w, 0.0, |c| o.command("world.palette", &BTreeMap::new(), c)).unwrap();
    apply(&mut o, &mut w, r);
    assert_eq!(w.mode().label(), "COMANDOS");
    for ch in "world.room".chars() {
        with(&w, 0.0, |c| o.key(&key(&ch.to_string()), c));
    }
    assert_eq!(o.dump()["palette"]["top"], "world.room_next");
    assert_eq!(o.dump()["palette"]["query"], "world.room");
    let h = hud(&w);
    let els = with(&w, 0.0, |c| o.elements(c, &h));
    assert!(ids(&els).contains(&PALETTE_ID));
    // ctrl+u clears; backspace edits; a command with an arg asks it
    with(&w, 0.0, |c| {
        o.command("world.line_clear", &BTreeMap::new(), c)
    });
    assert_eq!(o.dump()["palette"]["query"], "");
    for ch in "sala novax".chars() {
        with(&w, 0.0, |c| o.key(&key(&ch.to_string()), c));
    }
    with(&w, 0.0, |c| o.key(&key("backspace"), c));
    assert_eq!(o.dump()["palette"]["top"], "world.room_new");
    let r = with(&w, 0.0, |c| {
        o.command("world.palette_run", &BTreeMap::new(), c)
    })
    .unwrap();
    assert!(r.is_empty());
    assert_eq!(o.dump()["palette"]["prompt"], "nome da sala");
    for ch in "bravo".chars() {
        with(&w, 0.0, |c| o.key(&key(&ch.to_string()), c));
    }
    let r = with(&w, 0.0, |c| {
        o.command("world.palette_run", &BTreeMap::new(), c)
    })
    .unwrap();
    assert_eq!(r[0], Req::Action(Action::PaletteDone));
    assert_eq!(
        r[1],
        Req::Command {
            name: "world.room_new".into(),
            args: BTreeMap::from([("name".to_string(), "bravo".to_string())])
        }
    );
    apply(&mut o, &mut w, r);
    assert_eq!(w.mode().label(), "NAVEGAR");
}

#[test]
fn palette_takes_items_handed_in() {
    let mut w = world();
    let mut o = ov();
    with(&w, 0.0, |c| {
        o.inject(
            &serde_json::json!({"palette_items": [{"name": "session.stop", "title": "parar a sessão", "source": "core"}]}),
            c,
        )
    });
    let r = with(&w, 0.0, |c| o.command("world.palette", &BTreeMap::new(), c)).unwrap();
    apply(&mut o, &mut w, r);
    for ch in "parar".chars() {
        with(&w, 0.0, |c| o.key(&key(&ch.to_string()), c));
    }
    assert_eq!(o.dump()["palette"]["top"], "session.stop");
}

#[test]
fn dialog_box_choices_and_blips() {
    let w = world();
    let mut o = ov();
    let mut h = hud(&w);
    h.dialog = Some(DialogView {
        name: "api-gateway".into(),
        rgb: [0x6a, 0x9f, 0xd8],
        text: "Apli".into(),
        more: false,
        choices: vec!["sim, aplicar agora".into(), "não, revisar antes".into()],
        selected: 0,
        line: None,
    });
    let els = with(&w, 0.0, |c| o.elements(c, &h));
    assert!(ids(&els).contains(&DIALOG_ID) && ids(&els).contains(&CHOICES_ID));
    let r = with(&w, 0.0, |c| o.tick(c));
    assert_eq!(r.len(), 3, "at most 3 blips a frame: {r:?}");
    assert!(matches!(
        r[0],
        Req::Cue {
            cue: nkw_objects::sound::Cue::Blip { .. },
            at: None
        }
    ));
    h.dialog.as_mut().unwrap().text = "Aplic".into();
    with(&w, 0.05, |c| o.elements(c, &h));
    assert_eq!(
        with(&w, 0.05, |c| o.tick(c)).len(),
        1,
        "one new char, one blip"
    );
    // the box rises while it appears, then rests
    assert!(with(&w, 0.05, |c| o.wishes(c)).contains(&Wish::compose(Want::Continuous)));
    with(&w, 1.0, |c| o.elements(c, &h));
    assert!(with(&w, 1.0, |c| o.wishes(c)).is_empty());
    // more, no choices: the ▼ blinks on a 2 Hz At wish
    let d = h.dialog.as_mut().unwrap();
    d.choices.clear();
    d.more = true;
    with(&w, 1.1, |c| o.elements(c, &h));
    assert_eq!(
        with(&w, 1.1, |c| o.wishes(c)),
        [Wish::compose(Want::At(1.5))]
    );
    h.dialog = None;
    let els = with(&w, 2.0, |c| o.elements(c, &h));
    assert!(!ids(&els).contains(&DIALOG_ID));
    assert_eq!(o.dump()["dialog"], Value::Null);
}

#[test]
fn injected_dialog_is_pixel_stable() {
    let w = world();
    let j = serde_json::json!({"dialog": {"name": "api-gateway", "text": "Aplicar a migração agora?", "choices": ["sim", "não"]}});
    let pixels = || {
        let mut o = ov();
        with(&w, 0.0, |c| o.inject(&j, c));
        let els = with(&w, 5.0, |c| o.elements(c, &hud(&w)));
        els.iter()
            .map(|e| format!("{:?}{:?}", e.id, e.pos))
            .collect::<Vec<_>>()
    };
    assert_eq!(pixels(), pixels());
    let mut o = ov();
    with(&w, 0.0, |c| o.inject(&j, c));
    let els = with(&w, 5.0, |c| o.elements(c, &hud(&w)));
    assert!(ids(&els).contains(&DIALOG_ID));
}

#[test]
fn the_reply_line_and_the_vault_mask() {
    let mut w = world();
    let mut o = ov();
    w.action(
        Action::Talk {
            cat: CatId(1),
            purpose: TalkPurpose::VaultUnlock,
        },
        0.0,
    );
    let r = with(&w, 0.0, |c| {
        o.command("world.talk_reply", &BTreeMap::new(), c)
    })
    .unwrap();
    assert_eq!(r, [Req::Action(Action::Reply)]);
    apply(&mut o, &mut w, r);
    with(&w, 0.0, |c| o.tick(c));
    for ch in "destrancar".chars() {
        with(&w, 0.0, |c| o.key(&key(&ch.to_string()), c));
    }
    with(&w, 0.0, |c| {
        o.command("world.line_home", &BTreeMap::new(), c)
    });
    with(&w, 0.0, |c| o.key(&key("x"), c));
    let d = o.dump().to_string();
    assert!(!d.contains("destrancar"), "never in the dump: {d}");
    assert_eq!(o.dump()["line"]["chars"], 11);
    let r = with(&w, 0.0, |c| {
        o.command("world.line_send", &BTreeMap::new(), c)
    })
    .unwrap();
    assert_eq!(r, [Req::Action(Action::Send)]);
    assert_eq!(o.take_line().as_deref(), Some("xdestrancar"));
    assert_eq!(o.dump()["line"]["chars"], 0);
}

#[test]
fn the_map_opens_moves_goes_and_closes() {
    let w = world();
    let mut o = ov();
    assert_eq!(o.submode(), None);
    with(&w, 0.0, |c| {
        o.command("world.overview", &BTreeMap::new(), c)
    });
    assert_eq!(o.submode(), Some("map"));
    let els = with(&w, 0.0, |c| o.elements(c, &hud(&w)));
    assert_eq!(ids(&els), [RING_ID, MAP_ID], "the map covers the HUD");
    let sel = o.dump()["map"]["sel"].as_u64().unwrap();
    with(&w, 0.0, |c| {
        o.key(&key(if sel == 0 { "l" } else { "h" }), c)
    });
    assert_ne!(o.dump()["map"]["sel"].as_u64().unwrap(), sel);
    with(&w, 0.0, |c| o.key(&key("o"), c));
    assert_eq!(o.dump()["map"]["floor"], true);
    with(&w, 0.0, |c| o.key(&key("o"), c));
    assert_eq!(o.dump()["map"]["floor"], false);
    let r = with(&w, 0.0, |c| o.key(&key("enter"), c));
    assert!(matches!(r.as_slice(), [Req::GlideTo(_)]));
    assert_eq!(o.submode(), None);
    with(&w, 0.0, |c| {
        o.command("world.overview", &BTreeMap::new(), c)
    });
    with(&w, 0.0, |c| o.key(&key("esc"), c));
    assert_eq!(o.submode(), None);
    // only from plain NAV
    let mut f = world();
    f.command(Cmd::Walk, 0.0);
    with(&f, 0.0, |c| {
        o.command("world.overview", &BTreeMap::new(), c)
    });
    assert_eq!(o.submode(), None);
}

#[test]
fn tags_show_near_and_hide_in_foco() {
    let w = world();
    let mut o = ov();
    let mut h = hud(&w);
    let at = w.camera.pos + crate::world::facing(w.camera.yaw) * 2.0;
    h.tags = vec![Tag {
        id: 7,
        pos: at,
        text: "acme".into(),
        rgb: [0x30, 0x60, 0x90],
        max_m: 10.0,
    }];
    let els = with(&w, 0.0, |c| o.elements(c, &h));
    assert!(ids(&els).contains(&(TAG_ID + 7)));
    h.focus_rect = Some([0, 0, 10, 10]);
    let els = with(&w, 0.0, |c| o.elements(c, &h));
    assert!(!ids(&els).contains(&(TAG_ID + 7)));
}

#[test]
fn commands_not_ours_are_not_taken() {
    let w = world();
    let mut o = ov();
    for n in [
        "world.left",
        "world.choice_1",
        "world.talk_sit",
        "session.stop",
        "world.summon",
    ] {
        assert!(
            with(&w, 0.0, |c| o.command(n, &BTreeMap::new(), c)).is_none(),
            "{n}"
        );
    }
    assert!(!o.dirty);
    for n in ["world.library", "world.grab"] {
        assert!(with(&w, 0.0, |c| o.command(n, &BTreeMap::new(), c)).is_some());
    }
    assert_eq!(o.dump()["toasts"], 2);
}
