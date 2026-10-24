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
        aimed: false,
        status: crate::ext::Status::default(),
        phone: None,
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
    // a wide second chip (wider than the label it moves into in FOCO)
    h.badges = vec![Badge::Inbox(3), Badge::Updates(12)];
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
        // the World's since S15 (no more "fase 3/4" toasts)
        "world.library",
        "world.grab",
        "world.nearer",
        "world.farther",
        // the library's (objects plugin)
        "world.book_next",
        "world.library_search",
        "lua.key:normal.world:n",
    ] {
        assert!(
            with(&w, 0.0, |c| o.command(n, &BTreeMap::new(), c)).is_none(),
            "{n}"
        );
    }
    assert!(!o.dirty);
    assert_eq!(o.dump()["toasts"], 0);
}

fn with_scale<R>(w: &World, now: Time, scale: f64, f: impl FnOnce(&Ctx) -> R) -> R {
    let pids = BTreeMap::new();
    let vis = [RoomId(0)];
    let ctx = Ctx {
        world: w,
        now,
        frame: 0,
        reduced_motion: false,
        demo: true,
        scale,
        size_px: [1280, 720],
        pids: &pids,
        visible_rooms: &vis,
        line: None,
    };
    f(&ctx)
}

fn walking() -> World {
    let mut w = world();
    w.command(Cmd::Walk, 0.0);
    assert!(w.mode().walking());
    w
}

fn inject(o: &mut Overlay, w: &World, j: Value) {
    with(w, 0.0, |c| o.inject(&j, c));
}

fn el(els: &[OverlayElem], id: u64) -> Option<&OverlayElem> {
    els.iter().find(|e| e.id == id)
}

#[test]
fn crosshair_dot_then_ring_only_in_andar() {
    let w = walking();
    let mut o = ov();
    let mut h = hud(&w);
    let els = with(&w, 1.0, |c| o.elements(c, &h));
    let x = el(&els, CROSS_ID).expect("a crosshair in ANDAR");
    let c = draw::CROSS_BOX;
    assert_eq!(x.pos, ((1280 - c) / 2, (720 - c) / 2), "centred");
    assert_eq!(o.dump()["crosshair"], "dot");
    assert!(
        with(&w, 1.0, |c| o.wishes(c)).is_empty(),
        "a resting dot: no wish"
    );
    // something interactable in the aim: a ring in 80 ms
    h.aimed = true;
    with(&w, 2.0, |c| o.elements(c, &h));
    assert_eq!(o.dump()["crosshair"], "ring");
    assert!(with(&w, 2.0, |c| o.wishes(c)).contains(&Wish::compose(Want::Continuous)));
    let made = o.dump()["rasters"].as_u64().unwrap();
    with(&w, 2.04, |c| o.elements(c, &h));
    with(&w, 2.08, |c| o.elements(c, &h));
    assert!(o.dump()["rasters"].as_u64().unwrap() > made, "the steps");
    assert!(with(&w, 2.08, |c| o.wishes(c)).is_empty(), "then at rest");
    // the same through `inject overlay {"aimed": …}` (null gives it back)
    h.aimed = false;
    inject(&mut o, &w, serde_json::json!({"aimed": true}));
    with(&w, 3.0, |c| o.elements(c, &h));
    assert_eq!(o.dump()["crosshair"], "ring");
    inject(&mut o, &w, serde_json::json!({"aimed": null}));
    with(&w, 4.0, |c| o.elements(c, &h));
    assert_eq!(o.dump()["crosshair"], "dot");
    // NAV: none
    let n = world();
    let mut h = hud(&n);
    h.aimed = true;
    let els = with(&n, 5.0, |c| o.elements(c, &h));
    assert!(el(&els, CROSS_ID).is_none());
    assert_eq!(o.dump()["crosshair"], Value::Null);
}

#[test]
fn vignette_is_cached_by_quantised_alpha_and_wishes_nothing() {
    let w = walking();
    let mut o = ov();
    let h = hud(&w);
    inject(&mut o, &w, serde_json::json!({"vignette": 0.5}));
    let els = with(&w, 1.0, |c| o.elements(c, &h));
    let v = els.last().unwrap();
    assert_eq!(v.id, VIGNETTE_ID, "under everything");
    assert_eq!(v.pos, (0, 0));
    assert_eq!(v.buf_scale, 0.25, "quarter-res, the hub upsamples");
    assert_eq!(o.dump()["vignette"], 0.5);
    let made = o.dump()["rasters"].as_u64().unwrap();
    // 0.51 is the same sixteenth: no raster
    inject(&mut o, &w, serde_json::json!({"vignette": 0.51}));
    with(&w, 1.1, |c| o.elements(c, &h));
    assert_eq!(o.dump()["rasters"].as_u64().unwrap(), made);
    inject(&mut o, &w, serde_json::json!({"vignette": 0.8}));
    with(&w, 1.2, |c| o.elements(c, &h));
    assert_eq!(o.dump()["rasters"].as_u64().unwrap(), made + 1);
    assert_eq!(o.dump()["vignette"], 0.8125);
    for t in [1.0, 1.1, 1.2] {
        assert!(
            with(&w, t, |c| o.wishes(c))
                .iter()
                .all(|x| !matches!(x.want, Want::Rate(_))),
            "never a Rate"
        );
    }
    // off (the World's own value, 0 with the option off) and outside ANDAR
    inject(&mut o, &w, serde_json::json!({"vignette": null}));
    let els = with(&w, 2.0, |c| o.elements(c, &h));
    assert!(el(&els, VIGNETTE_ID).is_none());
    assert_eq!(o.dump()["vignette"], 0.0);
    inject(&mut o, &w, serde_json::json!({"vignette": 1.0}));
    let n = world();
    let els = with(&n, 3.0, |c| o.elements(c, &hud(&n)));
    assert!(el(&els, VIGNETTE_ID).is_none());
}

#[test]
fn the_hand_hint_while_carrying() {
    let w = walking();
    let mut o = ov();
    let h = hud(&w);
    let els = with(&w, 1.0, |c| o.elements(c, &h));
    assert!(el(&els, HAND_ID).is_none());
    assert_eq!(o.dump()["hand"], false);
    inject(&mut o, &w, serde_json::json!({"carrying": true}));
    let els = with(&w, 1.0, |c| o.elements(c, &h));
    let e = el(&els, HAND_ID).expect("the keys");
    assert_eq!(o.dump()["hand"], true);
    let size = {
        let mut f = Fonts::new();
        cache::Cache::new()
            .get(
                &Spec::Bar {
                    text: HAND_HINT.into(),
                },
                Px(1.0),
                &mut f,
            )
            .size
    };
    assert_eq!(e.pos.0, (1280 - size.0) / 2, "bottom-centre");
    assert!(e.pos.1 > 600);
    let n = world();
    let els = with(&n, 1.0, |c| o.elements(c, &hud(&n)));
    assert!(el(&els, HAND_ID).is_none(), "only in ANDAR");
}

fn dialog(text: &str, choices: &[&str], more: bool) -> DialogView {
    DialogView {
        name: "api-gateway".into(),
        rgb: [0x6a, 0x9f, 0xd8],
        text: text.into(),
        more,
        choices: choices.iter().map(|c| c.to_string()).collect(),
        selected: 0,
        line: None,
    }
}

/// S16 counters: what the box drew, not what the view had.
#[test]
fn dialog_counters_are_what_is_drawn() {
    let w = world();
    let mut o = ov();
    let mut h = hud(&w);
    let q = "Aplicar a migração agora?";
    assert_eq!(q.chars().count(), 25);
    h.dialog = Some(dialog(
        q,
        &["sim, aplicar agora", "não, revisar antes"],
        false,
    ));
    let els = with(&w, 0.0, |c| o.elements(c, &h));
    assert!(el(&els, CHOICES_ID).is_some());
    assert_eq!(
        o.dump()["dialog"],
        serde_json::json!({"chars": 25, "choices": 2})
    );
    // still revealing: no choices yet
    h.dialog = Some(dialog("Aplicar a migra", &[], false));
    with(&w, 0.1, |c| o.elements(c, &h));
    assert_eq!(
        o.dump()["dialog"],
        serde_json::json!({"chars": 15, "choices": 0})
    );
    // a long text: the box shows its last three lines only
    let long = "um dois três quatro cinco seis sete oito nove dez ".repeat(8);
    h.dialog = Some(dialog(&long, &[], true));
    with(&w, 0.2, |c| o.elements(c, &h));
    let chars = o.dump()["dialog"]["chars"].as_u64().unwrap() as usize;
    assert!(chars < long.chars().count() && chars > 60, "{chars}");
    // the choices box would cover the FOCO window: not drawn, not counted
    h.dialog = Some(dialog(q, &["sim", "não"], false));
    h.focus_rect = Some([0, 0, 1280, 720]);
    with(&w, 0.3, |c| o.elements(c, &h));
    assert_eq!(o.dump()["dialog"]["choices"], 0);
    h.dialog = None;
    with(&w, 0.4, |c| o.elements(c, &h));
    assert_eq!(o.dump()["dialog"], Value::Null);
}

#[test]
fn toasts_stack_above_an_open_dialogue() {
    let w = world();
    let mut o = ov();
    let mut h = hud(&w);
    o.toast("salvo", ToastKind::Info, 10_000, 0.0);
    let els = with(&w, 1.0, |c| o.elements(c, &h));
    let alone = el(&els, TOAST_ID).unwrap().pos.1;
    assert!(alone > 650, "bottom-left: {alone}");
    h.dialog = Some(dialog("Aplicar a migração agora?", &[], true));
    let els = with(&w, 2.0, |c| o.elements(c, &h));
    let box_top = el(&els, DIALOG_ID).unwrap().pos.1;
    let t = el(&els, TOAST_ID).unwrap();
    let th = with(&w, 2.0, |_| {
        cache::Cache::new()
            .get(
                &Spec::Toast {
                    text: "salvo".into(),
                    error: false,
                },
                Px(1.0),
                &mut Fonts::new(),
            )
            .size
            .1
    });
    assert!(t.pos.1 + th <= box_top, "{} + {th} vs {box_top}", t.pos.1);
}

#[test]
fn drawn_chars_counts_the_kept_lines() {
    let t = "a bb ccc dddd";
    let all = draw::wrap(t, 4);
    assert_eq!(all, ["a bb", "ccc", "dddd"]);
    assert_eq!(drawn_chars(t, &all, 0), 13);
    assert_eq!(drawn_chars(t, &all, 1), 8, "\"a bb \" gone");
    assert_eq!(drawn_chars(t, &all, 2), 4);
    assert_eq!(drawn_chars("", &[String::new()], 0), 0);
}

/// Fractional scale: every element at physical px with `buf_scale` = the
/// scale; integer scales keep today's integer buffers; one raster per
/// scale, hits after.
#[test]
fn fractional_scale_elements_and_cache() {
    let w = world();
    let mut o = ov();
    let h = hud(&w);
    let one = with_scale(&w, 0.0, 1.0, |c| o.elements(c, &h));
    assert_eq!(one[0].buf_scale, 1.0);
    let r1 = o.dump()["rasters"].as_u64().unwrap();
    let q = with_scale(&w, 0.0, 1.25, |c| o.elements(c, &h));
    assert_eq!(q[0].id, LABEL_ID);
    assert_eq!(q[0].buf_scale, 1.25);
    assert_eq!(q[0].pos, LABEL_POS, "positions stay logical");
    assert_eq!(o.dump()["rasters"].as_u64().unwrap(), r1 + 1, "a miss");
    with_scale(&w, 0.0, 1.25, |c| o.elements(c, &h));
    assert_eq!(o.dump()["rasters"].as_u64().unwrap(), r1 + 1, "then hits");
    let two = with_scale(&w, 0.0, 2.0, |c| o.elements(c, &h));
    assert_eq!(two[0].buf_scale, 1.0, "2×: integer buffer");
    for (s, want) in [
        (1.0, 1.0),
        (2.0, 2.0),
        (1.25, 1.25),
        (1.5, 1.5),
        (1.75, 1.75),
        (0.0, 1.0),
    ] {
        assert_eq!(raster_scale(s), Px(want), "{s}");
    }
}

/// SALTO over 20 targets with books mixed in: each gets a label, none
/// covers another, the dump names their kinds.
#[test]
fn hint_labels_on_twenty_targets_with_books() {
    let mut w = world();
    let mut o = ov();
    let r = with(&w, 0.0, |c| o.command("world.hint", &BTreeMap::new(), c)).unwrap();
    apply(&mut o, &mut w, r);
    let cam = w.camera;
    let fwd = crate::world::facing(cam.yaw);
    let right = glam::Vec3::new(-fwd.z, 0.0, fwd.x);
    let picks: Vec<Value> = (0..20)
        .map(|i| {
            let p = cam.pos
                + fwd * 3.0
                + right * ((i % 5) as f32 - 2.0) * 0.5
                + glam::Vec3::Y * ((i / 5) as f32 * 0.3 - 0.5);
            let pos = [p.x, p.y, p.z];
            if i % 2 == 0 {
                serde_json::json!({"book": i, "pos": pos})
            } else {
                serde_json::json!({"cat": i, "pos": pos})
            }
        })
        .collect();
    inject(&mut o, &w, serde_json::json!({ "pickables": picks }));
    let els = with(&w, 0.0, |c| o.elements(c, &hud(&w)));
    assert_eq!(o.dump()["hints"]["count"], 20);
    let boxes: Vec<_> = els
        .iter()
        .filter(|e| e.id >= HINT_ID && e.id < TAG_ID)
        .collect();
    assert_eq!(boxes.len(), 20);
    let kinds = o.dump()["hints"]["targets"].clone();
    let books = kinds
        .as_array()
        .unwrap()
        .iter()
        .filter(|t| t["kind"] == "book")
        .count();
    assert_eq!(books, 10);
    let labels: std::collections::BTreeSet<String> = o.dump()["hints"]["labels"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    assert_eq!(labels.len(), 20, "unique labels");
    for (i, a) in boxes.iter().enumerate() {
        for b in &boxes[i + 1..] {
            assert_ne!(a.pos, b.pos);
        }
    }
}

#[test]
fn tags_fade_in_and_out_on_at_wishes() {
    let w = world();
    let mut o = ov();
    let mut h = hud(&w);
    let at = w.camera.pos + crate::world::facing(w.camera.yaw) * 1.5;
    h.tags = vec![Tag {
        id: 7,
        pos: at,
        text: "notas.md".into(),
        rgb: [0x6a, 0x9f, 0xd8],
        max_m: 1.8,
    }];
    let els = with(&w, 1.0, |c| o.elements(c, &h));
    assert!(el(&els, TAG_ID + 7).is_some(), "from its first frame");
    assert_eq!(
        with(&w, 1.0, |c| o.wishes(c)),
        [Wish::compose(Want::At(1.02))]
    );
    with(&w, 1.1, |c| o.elements(c, &h));
    assert!(with(&w, 1.1, |c| o.wishes(c)).is_empty(), "opaque: at rest");
    // out of its radius: fades for 120 ms, then gone
    h.tags.clear();
    let els = with(&w, 2.0, |c| o.elements(c, &h));
    assert!(el(&els, TAG_ID + 7).is_some(), "fading");
    assert_eq!(o.dump()["tags"], 0, "not counted as shown");
    let wish = with(&w, 2.0, |c| o.wishes(c));
    assert!(matches!(wish.as_slice(), [Wish { want: Want::At(t), .. }] if *t > 2.0 && *t <= 2.12));
    let els = with(&w, 2.12, |c| o.elements(c, &h));
    assert!(el(&els, TAG_ID + 7).is_none());
    assert!(with(&w, 2.12, |c| o.wishes(c)).is_empty());
}

mod w2;
