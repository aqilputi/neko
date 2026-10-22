//! The wave-2 testclient flags against the real compositor (headless): the
//! features other packages' tests build on (`--copy`, `--log-selection`,
//! `--damage`, `--parent-of`, `--fullscreen`).

mod common;

use common::{evs, run, testclient};

#[test]
fn selection_copies_between_two_testclients() {
    if testclient().is_none() {
        return;
    }
    let r = run(
        "spawn testclient --app-id acme.alpha --copy \"olá mundo\" --log {dir}/a.json
wait 300
key Return
expect focus acme.alpha
wait 200
key ctrl+bracketright
spawn testclient --app-id acme.bravo --log-selection --log {dir}/b.json
wait 300
key Return
expect focus acme.bravo
wait 300
quit
",
        &[],
        &[],
    );
    assert_eq!(r.code(), 0, "{}", r.stderr());
    let a = r.log("a.json");
    let b = r.log("b.json");
    assert_eq!(evs(&a, "selection_set").len(), 1, "{a:?}");
    let sel = evs(&b, "selection");
    assert_eq!(sel.len(), 1, "{b:?}");
    assert_eq!(sel[0]["mime"], "text/plain;charset=utf-8");
    assert_eq!(sel[0]["text"], "olá mundo");
}

#[test]
fn damage_client_keeps_framing() {
    if testclient().is_none() {
        return;
    }
    let r = run(
        "spawn testclient --app-id acme.alpha --damage 64x16 --log {dir}/a.json
key Return
expect focus acme.alpha
wait 500
quit
",
        &[],
        &[],
    );
    assert_eq!(r.code(), 0, "{}", r.stderr());
    let a = r.log("a.json");
    assert!(evs(&a, "frame").len() > 10, "{a:?}");
}

#[test]
fn parent_of_maps_mother_then_dialog() {
    if testclient().is_none() {
        return;
    }
    let r = run(
        "spawn testclient --app-id acme.dialog --parent-of acme.mother --size 320x200 --log {dir}/a.json
wait 500
dump {dir}/world.json
quit
",
        &[],
        &[],
    );
    assert_eq!(r.code(), 0, "{}", r.stderr());
    let a = r.log("a.json");
    let conf = evs(&a, "configure");
    assert!(conf.len() >= 2, "{a:?}");
    assert_eq!(
        conf[0]["win"], "parent",
        "the mother configures first: {a:?}"
    );
    assert!(conf.iter().any(|e| e.get("win").is_none()), "{a:?}");
    let d = r.json("world.json");
    let ids: Vec<&str> = d["screens"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|s| s["app_id"].as_str())
        .collect();
    assert!(
        ids.contains(&"acme.mother") && ids.contains(&"acme.dialog"),
        "{ids:?}"
    );
}

#[test]
fn fullscreen_flag_still_maps() {
    if testclient().is_none() {
        return;
    }
    let r = run(
        "spawn testclient --app-id acme.tv --fullscreen --log {dir}/a.json
wait 400
dump {dir}/world.json
quit
",
        &[],
        &[],
    );
    assert_eq!(r.code(), 0, "{}", r.stderr());
    assert!(!evs(&r.log("a.json"), "configure").is_empty());
}
