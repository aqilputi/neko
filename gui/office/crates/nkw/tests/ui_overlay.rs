//! P3 end to end (headless `--demo`): the router's keys open SALTO,
//! COMANDOS and the map; the overlay's state shows in `dump`; the dialogue
//! box is pixel-stable.

mod common;

use common::{run, testclient};

#[test]
fn salto_labels_two_windows_and_a_label_goes_to_foco() {
    if testclient().is_none() {
        return;
    }
    // the WM's pickables (P2) are the real source; the screens are injected
    // too (the same target twice counts once), so this holds with or
    // without them
    let script = |then: &str| {
        format!(
            "key J
spawn testclient --app-id alpha --no-frame-wait
spawn testclient --app-id bravo --no-frame-wait
wait 900
inject overlay {{\"pickables\": [{{\"app_id\": \"alpha\"}}, {{\"app_id\": \"bravo\"}}]}}
key F
wait 200
expect mode SALTO
dump {{dir}}/hints.json
{then}
quit
"
        )
    };
    let r = run(&script(""), &[], &[]);
    assert_eq!(r.code(), 0, "{}", r.stderr());
    let h = &r.json("hints.json")["ext"]["overlay"]["hints"];
    let screens: Vec<&str> = h["targets"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|t| t["kind"] == "screen")
        .map(|t| t["label"].as_str().unwrap())
        .collect();
    assert_eq!(screens.len(), 2, "{h}");
    assert!(h["count"].as_u64().unwrap() >= 2);
    // same input, same labels: typing a screen's label goes to FOCO
    let r = run(
        &script(&format!(
            "text \"{}\"\nwait 300\nexpect mode FOCO",
            screens[0]
        )),
        &[],
        &[],
    );
    assert_eq!(r.code(), 0, "{}", r.stderr());
}

#[test]
fn esc_cancels_salto() {
    let r = run(
        "key f
expect mode SALTO
key Escape
expect mode NAV
quit
",
        &[],
        &[],
    );
    assert_eq!(r.code(), 0, "{}", r.stderr());
}

#[test]
fn palette_filters_and_runs() {
    let r = run(
        "key colon
expect mode COMANDOS
text \"world.room\"
wait 100
expect json /ext/overlay/palette/top \"world.room_next\"
key Return
wait 300
expect mode NAV
dump {dir}/after.json
quit
",
        &[],
        &[],
    );
    assert_eq!(r.code(), 0, "{}", r.stderr());
    // the palette ran world.room_next: the station moved to the next room
    let d = r.json("after.json");
    assert_eq!(d["station"]["desk"]["room"], 1, "{}", d["station"]);
}

#[test]
fn the_map_opens_toggles_and_closes() {
    let r = run(
        "key o
wait 100
expect json /ext/overlay/map/floor false
key o
wait 100
expect json /ext/overlay/map/floor true
key Escape
wait 100
expect json /ext/overlay/map null
expect mode NAV
quit
",
        &[],
        &[],
    );
    assert_eq!(r.code(), 0, "{}", r.stderr());
}

#[test]
fn dialog_shot_is_pixel_stable() {
    let script = "inject overlay {\"dialog\": {\"name\": \"api-gateway\", \"text\": \"Aplicar a migração agora?\", \"choices\": [\"sim, aplicar agora\", \"não, revisar antes\"]}}
wait 400
expect json /ext/overlay/dialog/chars 25
shot {dir}/d.png
quit
";
    let a = run(script, &[], &[]);
    assert_eq!(a.code(), 0, "{}", a.stderr());
    let b = run(script, &[], &[]);
    assert_eq!(b.code(), 0, "{}", b.stderr());
    let pa = std::fs::read(a.path("d.png")).unwrap();
    let pb = std::fs::read(b.path("d.png")).unwrap();
    assert!(pa == pb, "the same dialogue twice gives the same PNG");
}

#[test]
fn hud_rasters_only_on_change_and_idles() {
    let r = run(
        "wait 300
dump {dir}/a.json
wait 2000
dump {dir}/b.json
quit
",
        &[],
        &[],
    );
    assert_eq!(r.code(), 0, "{}", r.stderr());
    let (a, b) = (r.json("a.json"), r.json("b.json"));
    assert_eq!(
        a["ext"]["overlay"]["rasters"], b["ext"]["overlay"]["rasters"],
        "nothing changed, nothing re-rasterised"
    );
}
