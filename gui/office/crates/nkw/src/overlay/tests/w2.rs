//! Wave 4 (W2): the phone, the strip, prefs, tags behind walls.

use super::*;

fn names_of(r: &[Req]) -> Vec<String> {
    r.iter()
        .map(|r| match r {
            Req::Command { name, .. } => name.clone(),
            Req::Action(a) => format!("{a:?}"),
            _ => "?".into(),
        })
        .collect()
}

/// A command through the overlay, its machine actions applied (the hub's
/// `apply_reqs`); returns the rest.
fn cmd(o: &mut Overlay, w: &mut World, name: &str, args: &[(&str, &str)]) -> Vec<Req> {
    let a: BTreeMap<String, String> = args
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    let r = with(w, 0.0, |c| o.command(name, &a, c)).unwrap_or_default();
    apply(o, w, r)
}

fn draw(o: &mut Overlay, w: &World) -> Vec<OverlayElem> {
    with(w, 0.0, |c| o.elements(c, &hud(w)))
}

/// The phone rides COMANDOS: `world.phone` asks for the palette mode, the
/// machine opens it, no list is made, the label says TELEFONE; `esc`
/// (`world.phone_close`) goes back.
#[test]
fn the_phone_opens_over_comandos_and_closes() {
    let mut w = world();
    let mut o = ov();
    let rest = cmd(&mut o, &mut w, "world.phone", &[]);
    assert!(rest.is_empty(), "{:?}", names_of(&rest));
    assert_eq!(w.mode().kind(), Some(Kind::Palette));
    assert!(o.palette.is_none(), "no list under the phone");
    assert_eq!(o.submode(), Some("phone"));
    assert_eq!(o.phone_state().unwrap().app, None);
    let els = draw(&mut o, &w);
    assert!(ids(&els).contains(&PHONE_ID));
    assert_eq!(o.dump()["label"], "TELEFONE");
    assert_eq!(
        o.dump()["phone"]["items"],
        3,
        "home: the overlay's own apps"
    );
    // `3` is a printable (Route::Text): app 3 of the overlay's own three
    // (mapa 3, ajustes 9, ajuda 10) is ajuda
    let r = with(&w, 0.0, |c| o.key(&key("3"), c));
    assert!(r.is_empty());
    assert_eq!(o.phone_state().unwrap().app.as_deref(), Some("ajuda"));
    draw(&mut o, &w);
    assert!(
        o.dump()["phone"]["items"].as_u64().unwrap() > 20,
        "the keys"
    );
    assert_eq!(o.dump()["phone"]["title"], "Ajuda");
    // backspace → home, esc → closed, back to NAV
    cmd(&mut o, &mut w, "world.phone_back", &[]);
    assert_eq!(o.phone_state().unwrap().app, None);
    cmd(&mut o, &mut w, "world.phone_close", &[]);
    assert_eq!(w.mode().kind(), None);
    assert!(o.phone.is_none());
    assert_eq!(o.submode(), None);
    // world+n twice: open, close
    cmd(&mut o, &mut w, "world.phone", &[]);
    cmd(&mut o, &mut w, "world.phone", &[]);
    assert!(o.phone.is_none());
    assert_eq!(w.mode().kind(), None);
}

#[test]
fn mapa_closes_the_phone_and_opens_the_map() {
    let mut w = world();
    let mut o = ov();
    cmd(&mut o, &mut w, "world.phone", &[]);
    draw(&mut o, &w);
    let r = with(&w, 0.0, |c| o.key(&key("1"), c));
    assert_eq!(names_of(&r), ["Back", "world.overview"]);
    let rest = apply(&mut o, &mut w, r);
    assert_eq!(w.mode().kind(), None);
    for r in rest {
        if let Req::Command { name, args } = r {
            with(&w, 0.0, |c| o.command(&name, &args, c));
        }
    }
    assert_eq!(o.submode(), Some("map"));
}

#[test]
fn help_opens_on_ajuda_with_the_origin_mode_first() {
    let mut w = walking();
    let mut o = ov();
    o.demo = true; // the query shows in the dump only under --demo
    cmd(&mut o, &mut w, "world.help", &[]);
    assert_eq!(o.phone_state().unwrap().app.as_deref(), Some("ajuda"));
    draw(&mut o, &w);
    assert_eq!(o.phone.as_ref().unwrap().shown()[0].id, "sec:world.walk");
    // live search: `/`, type, the list shrinks
    with(&w, 0.0, |c| o.key(&key("/"), c));
    assert!(o.phone.as_ref().unwrap().typing);
    for k in ["p", "e", "g", "a", "r"] {
        with(&w, 0.0, |c| o.key(&key(k), c));
    }
    assert_eq!(o.dump()["phone"]["query"], "pegar");
    let n = o.dump()["phone"]["items"].as_u64().unwrap();
    assert!((1..20).contains(&n), "fewer than all: {n}");
    let first = o.phone.as_ref().unwrap().shown()[0].title.clone();
    assert!(first.starts_with("pegar"), "best first: {first}");
}

#[test]
fn ajustes_toggles_prefs_live_and_walk_options_by_command() {
    let mut w = world();
    let mut o = ov();
    cmd(&mut o, &mut w, "world.phone", &[("app", "ajustes")]);
    draw(&mut o, &w);
    assert_eq!(o.dump()["phone"]["selected"], "reduced_motion");
    // contrast: down once, enter → high contrast now (not saved: no path)
    cmd(&mut o, &mut w, "world.phone_down", &[]);
    let r = cmd(&mut o, &mut w, "world.phone_open", &[]);
    assert!(r.is_empty());
    assert_eq!(o.dump()["prefs"]["contrast"], "high");
    assert_eq!(o.dump()["prefs"]["saved"], false);
    assert!(o.theme().high_contrast);
    // font scale: l/h on the item are its own keys
    cmd(&mut o, &mut w, "world.phone_down", &[]);
    draw(&mut o, &w);
    with(&w, 0.0, |c| o.key(&key("l"), c));
    assert_eq!(o.dump()["prefs"]["font_scale_pct"], 110);
    with(&w, 0.0, |c| o.key(&key("h"), c));
    with(&w, 0.0, |c| o.key(&key("h"), c));
    assert_eq!(o.dump()["prefs"]["font_scale_pct"], 90);
    // the strip cycles
    cmd(&mut o, &mut w, "world.phone_down", &[]);
    cmd(&mut o, &mut w, "world.phone_open", &[]);
    assert_eq!(o.dump()["prefs"]["strip"], "always");
    // turn steps: a world.walk_set command for the World
    cmd(&mut o, &mut w, "world.phone_down", &[]);
    let r = with(&w, 0.0, |c| {
        o.command("world.phone_open", &BTreeMap::new(), c)
    })
    .unwrap();
    assert!(
        matches!(&r[..], [Req::Command { name, args }] if name == "world.walk_set" && args["key"] == "turn")
    );
    // reduced motion: the override is the overlay's
    let mut x = ov();
    let mut w2 = world();
    cmd(&mut x, &mut w2, "world.phone", &[("app", "ajustes")]);
    draw(&mut x, &w2);
    cmd(&mut x, &mut w2, "world.phone_open", &[]);
    assert_eq!(x.dump()["prefs"]["reduced_motion"], true);
}

#[test]
fn a_page_handed_in_acts_and_closes_with_back_first() {
    let mut w = world();
    let mut o = ov();
    let r = with(&w, 0.0, |c| {
        o.inject(&serde_json::json!({"phone": {"app": "gatos"}}), c)
    });
    apply(&mut o, &mut w, r);
    assert_eq!(w.mode().kind(), Some(Kind::Palette));
    inject(
        &mut o,
        &w,
        serde_json::json!({"phone_page": {"title": "Gatos", "items": [
            {"id": "1", "title": "acme", "icon": "Cat", "rgb": [200, 100, 50],
             "on_enter": {"name": "phone.sub", "args": {"sub": "1"}}},
            {"id": "2", "title": "bravo", "detail": "dormindo · bravo", "badge": "zZ",
             "on_enter": {"name": "world.talk", "args": {"cat": "2"}, "close": true},
             "on_keys": [["x", {"name": "session.stop", "args": {"cat": "2"}}]]}
        ], "search": {"placeholder": "buscar", "live": true}, "keys": [["x", "parar"]]}}),
    );
    draw(&mut o, &w);
    assert_eq!(o.dump()["phone"]["items"], 2);
    cmd(&mut o, &mut w, "world.phone_down", &[]);
    let r = with(&w, 0.0, |c| o.key(&key("x"), c));
    assert_eq!(names_of(&r), ["session.stop"]);
    let r = with(&w, 0.0, |c| {
        o.command("world.phone_open", &BTreeMap::new(), c)
    })
    .unwrap();
    assert_eq!(names_of(&r), ["Back", "world.talk"], "Back first (W5)");
    let rest = apply(&mut o, &mut w, r);
    assert_eq!(names_of(&rest), ["world.talk"]);
    assert!(o.phone.is_none());
    // `{"phone": null}` closes an open phone
    let r = with(&w, 0.0, |c| o.inject(&serde_json::json!({"phone": {}}), c));
    apply(&mut o, &mut w, r);
    assert!(o.phone.is_some());
    let r = with(&w, 0.0, |c| {
        o.inject(&serde_json::json!({"phone": null}), c)
    });
    apply(&mut o, &mut w, r);
    assert!(o.phone.is_none());
}

#[test]
fn palette_run_with_a_name_asks_its_argument_and_carries_the_rest() {
    let mut w = world();
    let mut o = ov();
    let r = cmd(
        &mut o,
        &mut w,
        "world.palette_run",
        &[("name", "world.room_new"), ("cat", "7")],
    );
    assert!(r.is_empty());
    assert_eq!(o.dump()["palette"]["step"], "args");
    assert_eq!(o.dump()["palette"]["prompt"], "nome da sala");
    with(&w, 0.0, |c| {
        for k in ["b", "r", "a", "v", "o"] {
            o.key(&key(k), c);
        }
    });
    let r = with(&w, 0.0, |c| {
        o.command("world.palette_run", &BTreeMap::new(), c)
    })
    .unwrap();
    let Req::Command { name, args } = &r[1] else {
        panic!("{r:?}")
    };
    assert_eq!(name, "world.room_new");
    assert_eq!(args["name"], "bravo");
    assert_eq!(args["cat"], "7");
}

fn status() -> crate::ext::Status {
    crate::ext::Status {
        clock: Some((14, 5)),
        inbox: Some(3),
        cost_cents: Some(1234),
        battery: Some((42, false)),
        net: Some((true, 3)),
        ..Default::default()
    }
}

#[test]
fn the_strip_sits_right_and_world_hud_hides_the_hud() {
    let w = world();
    let mut o = ov();
    let mut h = hud(&w);
    let els = with(&w, 0.0, |c| o.elements(c, &h));
    assert!(el(&els, STRIP_ID).is_none(), "no status: no strip");
    h.status = status();
    h.badges = vec![Badge::Inbox(3)];
    let els = with(&w, 0.0, |c| o.elements(c, &h));
    let s = el(&els, STRIP_ID).expect("the strip");
    assert_eq!(s.pos.1, STRIP_MARGIN);
    assert!(s.pos.0 > 900, "right-aligned: {:?}", s.pos);
    assert_eq!(o.dump()["strip"], true);
    // never: no strip
    inject(&mut o, &w, serde_json::json!({"prefs": {"strip": "never"}}));
    let els = with(&w, 0.0, |c| o.elements(c, &h));
    assert!(el(&els, STRIP_ID).is_none());
    // always is wider than auto
    inject(&mut o, &w, serde_json::json!({"prefs": {"strip": "auto"}}));
    let auto = el(&with(&w, 0.0, |c| o.elements(c, &h)), STRIP_ID)
        .unwrap()
        .pos
        .0;
    inject(
        &mut o,
        &w,
        serde_json::json!({"prefs": {"strip": "always"}}),
    );
    let always = el(&with(&w, 0.0, |c| o.elements(c, &h)), STRIP_ID)
        .unwrap()
        .pos
        .0;
    assert!(always < auto, "{always} {auto}");
    // z: label, strip and chips go; again: back
    let mut wm = world();
    cmd(&mut o, &mut wm, "world.hud", &[]);
    let els = with(&w, 0.0, |c| o.elements(c, &h));
    for id in [LABEL_ID, STRIP_ID, CHIP_ID] {
        assert!(el(&els, id).is_none(), "{id}");
    }
    assert_eq!(o.dump()["hud_hidden"], true);
    cmd(&mut o, &mut wm, "world.hud", &[]);
    let els = with(&w, 0.0, |c| o.elements(c, &h));
    assert!(el(&els, LABEL_ID).is_some() && el(&els, STRIP_ID).is_some());
}

#[test]
fn contrast_and_font_scale_change_the_rasters_once() {
    let w = world();
    let mut o = ov();
    let h = hud(&w);
    let first = with(&w, 0.0, |c| o.elements(c, &h));
    let a = el(&first, LABEL_ID).unwrap();
    inject(
        &mut o,
        &w,
        serde_json::json!({"prefs": {"contrast": "high", "font_scale": 1.3}}),
    );
    let made = o.dump()["rasters"].as_u64().unwrap();
    let second = with(&w, 0.0, |c| o.elements(c, &h));
    let b = el(&second, LABEL_ID).unwrap();
    assert_eq!(o.dump()["rasters"], made + 1, "the label again, bigger");
    with(&w, 0.0, |c| o.elements(c, &h));
    assert_eq!(
        o.dump()["rasters"],
        made + 1,
        "cached: no new raster at rest"
    );
    assert_eq!(a.pos, b.pos);
    assert_ne!(
        format!("{:?}", a.buf),
        format!("{:?}", b.buf),
        "a new buffer"
    );
}

#[test]
fn tags_behind_a_wall_are_dropped_and_the_rest_never_overlap() {
    let w = world();
    let mut o = ov();
    let mut h = hud(&w);
    let ahead = w.camera.pos + crate::world::facing(w.camera.yaw) * 2.5;
    h.tags = (0..12)
        .map(|i| Tag {
            id: 30 + i,
            pos: ahead + glam::Vec3::new(0.0, 0.0, i as f32 * 0.001),
            text: format!("gato-{i}"),
            rgb: [0x6a, 0x9f, 0xd8],
            max_m: 10.0,
        })
        .collect();
    let els = with(&w, 0.0, |c| o.elements(c, &h));
    assert_eq!(els.iter().filter(|e| e.id >= TAG_ID).count(), 12);
    let rects = o.stats.tag_rects.clone();
    assert_eq!(rects.len(), 12);
    for (i, a) in rects.iter().enumerate() {
        for b in &rects[i + 1..] {
            let hit = a.0 < b.0 + b.2 && b.0 < a.0 + a.2 && a.1 < b.1 + b.3 && b.1 < a.1 + a.3;
            assert!(!hit, "{a:?} {b:?}");
        }
    }
    // a wall in the way (the World says so): none shows, they fade out
    inject(&mut o, &w, serde_json::json!({"los": false}));
    with(&w, 1.0, |c| o.elements(c, &h));
    assert_eq!(o.dump()["tags"], 0);
    assert_eq!(o.dump()["tags_dropped_los"], 12);
    let els = with(&w, 2.0, |c| o.elements(c, &h));
    assert!(els.iter().all(|e| e.id < TAG_ID), "faded out");
    // the library's selected book wears the selection look
    inject(&mut o, &w, serde_json::json!({"los": null}));
    h.tags = vec![Tag {
        id: 0x11b0_0100,
        pos: ahead,
        text: "notas.md".into(),
        rgb: [0xf2, 0xe8, 0xd5],
        max_m: 10.0,
    }];
    let els = with(&w, 3.0, |c| o.elements(c, &h));
    assert!(el(&els, TAG_ID + (0x11b0_0100 & 0xf_ffff)).is_some());
}

/// Opened over FOCO on purpose, the phone covers the window (other HUD
/// pieces still keep off it).
#[test]
fn the_phone_draws_over_the_foco_window() {
    let mut w = world();
    let mut o = ov();
    cmd(&mut o, &mut w, "world.phone", &[]);
    let mut h = hud(&w);
    h.focus_rect = Some([0, 0, 1280, 720]);
    let els = with(&w, 0.0, |c| o.elements(c, &h));
    assert!(el(&els, PHONE_ID).is_some());
    assert!(el(&els, LABEL_ID).is_none());
}

#[test]
fn the_phone_wishes_nothing_at_rest() {
    let mut w = world();
    let mut o = ov();
    cmd(&mut o, &mut w, "world.phone", &[]);
    draw(&mut o, &w);
    assert!(with(&w, 0.0, |c| o.wishes(c)).is_empty());
    let made = o.dump()["rasters"].as_u64().unwrap();
    draw(&mut o, &w);
    assert_eq!(o.dump()["rasters"], made, "an unchanged phone: no raster");
    cmd(&mut o, &mut w, "world.phone_right", &[]);
    assert!(
        !with(&w, 0.0, |c| o.wishes(c)).is_empty(),
        "a key: one compose"
    );
    draw(&mut o, &w);
    assert_eq!(o.dump()["rasters"], made + 1);
}
