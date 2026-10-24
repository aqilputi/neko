//! The core's session keys in NAV (plan §3.4), the cats' phone commands and
//! the receptionist's tour (S24), headless against the `--demo` fixture
//! (`--core none`: core calls are recorded in `dump()["ext"]["cats"]["calls"]`).
//! The fixture serves the core's `normal` session rows as its keymap.
//!
//! Desks of the fixture: the reception's receptionist (cat 1); acme: acme
//! (2), api-gateway (3, blocked); demo: alpha (4), bravo (5), demo (6).

mod common;

use common::run;
use serde_json::Value;

fn ok(r: &common::Run) {
    assert_eq!(r.code(), 0, "{}", r.stderr());
}

fn calls(r: &common::Run, file: &str) -> Vec<Value> {
    r.json(file)["ext"]["cats"]["calls"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}

#[test]
fn session_keys_run_on_the_stations_cat() {
    let r = run(
        "wait 300\n\
         expect json /ext/router/source \"keymap\"\n\
         key J\n\
         wait 600\n\
         key x\n\
         key X\n\
         key d\n\
         key d\n\
         wait 100\n\
         expect mode NAV\n\
         dump {dir}/d.json\n\
         quit\n",
        &[],
        &[],
    );
    ok(&r);
    let c = calls(&r, "d.json");
    let got: Vec<(&str, &str, &str)> = c
        .iter()
        .map(|v| {
            (
                v["call"].as_str().unwrap(),
                v["name"].as_str().unwrap_or(""),
                v["session"].as_str().unwrap_or(""),
            )
        })
        .collect();
    assert_eq!(
        got,
        [
            ("run", "session.stop", "demo-acme"),
            ("run", "session.archive", "demo-acme"),
            ("run", "session.remove", "demo-acme"),
        ]
    );
}

#[test]
fn a_desk_without_a_cat_runs_nothing() {
    // demo's first desk is the old empty one
    let r = run(
        "wait 300\n\
         key J\n\
         key J\n\
         wait 900\n\
         key x\n\
         wait 100\n\
         expect json /ext/overlay/toasts 1\n\
         dump {dir}/d.json\n\
         quit\n",
        &[],
        &[],
    );
    ok(&r);
    assert!(calls(&r, "d.json").is_empty());
}

#[test]
fn a_goes_to_the_next_cat_that_waits_and_x_stops_it() {
    let r = run(
        "wait 300\n\
         key a\n\
         wait 900\n\
         key x\n\
         wait 100\n\
         dump {dir}/d.json\n\
         quit\n",
        &[],
        &[],
    );
    ok(&r);
    let c = calls(&r, "d.json");
    // api-gateway (blocked) and alpha (returned) wait; from the reception
    // the first in the office's order is api-gateway
    assert_eq!(c[0]["name"], "session.stop");
    assert_eq!(c[0]["session"], "demo-api-gateway");
}

#[test]
fn r_opens_the_conversation_on_the_reply_line() {
    let r = run(
        "wait 300\n\
         key J\n\
         wait 600\n\
         key r\n\
         wait 400\n\
         expect mode CONVERSA\n\
         expect json /ext/cats/dialog/name \"acme\"\n\
         expect json /ext/cats/dialog/line true\n\
         quit\n",
        &[],
        &[],
    );
    ok(&r);
}

#[test]
fn n_asks_comandos_for_the_new_name() {
    let r = run(
        "wait 300\n\
         key J\n\
         wait 600\n\
         key n\n\
         wait 200\n\
         expect mode COMANDOS\n\
         expect json /ext/overlay/palette/prompt \"nome\"\n\
         text \"zeta\"\n\
         key Return\n\
         wait 200\n\
         dump {dir}/d.json\n\
         quit\n",
        &[],
        &[],
    );
    ok(&r);
    let c = calls(&r, "d.json");
    assert_eq!(c[0]["name"], "session.rename");
    assert_eq!(c[0]["session"], "demo-acme");
    assert_eq!(c[0]["args"]["name"], "zeta");
}

#[test]
fn goto_cat_then_the_station_is_that_cats() {
    let r = run(
        "wait 300\n\
         inject cats {\"cmd\":\"world.goto_cat\",\"args\":{\"cat\":\"5\"}}\n\
         wait 900\n\
         expect mode NAV\n\
         key x\n\
         wait 100\n\
         dump {dir}/d.json\n\
         quit\n",
        &[],
        &[],
    );
    ok(&r);
    let c = calls(&r, "d.json");
    assert_eq!(
        (c[0]["name"].as_str(), c[0]["session"].as_str()),
        (Some("session.stop"), Some("demo-bravo"))
    );
}

#[test]
fn talk_with_a_cat_by_id_wherever_you_are() {
    let r = run(
        "wait 300\n\
         inject cats {\"cmd\":\"world.talk\",\"args\":{\"cat\":\"5\"}}\n\
         wait 600\n\
         expect mode CONVERSA\n\
         expect json /ext/cats/dialog/name \"bravo\"\n\
         quit\n",
        &[],
        &[],
    );
    ok(&r);
}

#[test]
fn the_tour_starts_at_the_receptionist_and_ends_once() {
    let r = run(
        "wait 300\n\
         key J\n\
         wait 600\n\
         inject cats {\"tour\":true}\n\
         wait 600\n\
         expect mode CONVERSA\n\
         expect json /ext/cats/tour 0\n\
         expect json /ext/cats/dialog/row \"Tour\"\n\
         expect json /ext/cats/dialog/cat 1\n\
         key 1\n\
         wait 300\n\
         expect json /ext/cats/tour 1\n\
         key 2\n\
         wait 300\n\
         expect mode NAV\n\
         expect json /ext/cats/tour null\n\
         quit\n",
        &[],
        &[],
    );
    ok(&r);
}

#[test]
fn the_phone_lists_the_cats() {
    let r = run(
        "wait 300\n\
         inject overlay {\"phone\":{\"app\":\"gatos\"}}\n\
         wait 300\n\
         expect mode COMANDOS\n\
         expect json /ext/overlay/phone/app \"gatos\"\n\
         expect json /ext/overlay/phone/items 5\n\
         quit\n",
        &[],
        &[],
    );
    ok(&r);
}

#[test]
fn help_and_switcher_open_the_phone() {
    let r = run(
        "wait 300\n\
         key ?\n\
         wait 300\n\
         expect json /ext/overlay/phone/app \"ajuda\"\n\
         key Escape\n\
         wait 300\n\
         key ctrl+k\n\
         wait 300\n\
         expect json /ext/overlay/phone/app \"gatos\"\n\
         quit\n",
        &[],
        &[],
    );
    ok(&r);
}

#[test]
fn the_phones_cat_actions_land_on_that_cat() {
    // Gatos › bravo (4th) › renomear (6th) → COMANDOS asks the name → the
    // rename goes to bravo, not to the station's cat (the receptionist)
    let r = run(
        "wait 300\n\
         inject overlay {\"phone\":{\"app\":\"gatos\"}}\n\
         wait 200\n\
         key j\nkey j\nkey j\nkey Return\n\
         wait 200\n\
         expect json /ext/overlay/phone/sub \"5\"\n\
         key j\nkey j\nkey j\nkey j\nkey j\nkey Return\n\
         wait 300\n\
         expect mode COMANDOS\n\
         expect json /ext/overlay/palette/prompt \"nome\"\n\
         text \"zeta\"\n\
         key Return\n\
         wait 300\n\
         expect mode NAV\n\
         dump {dir}/d.json\n\
         quit\n",
        &[],
        &[],
    );
    ok(&r);
    let c = calls(&r, "d.json");
    assert_eq!(c[0]["name"], "session.rename");
    assert_eq!(c[0]["session"], "demo-bravo");
    assert_eq!(c[0]["args"]["name"], "zeta");
    assert!(c[0]["args"].get("cat").is_none(), "ours, never the core's");
}

#[test]
fn the_phones_conversar_talks_to_that_cat() {
    let r = run(
        "wait 300\n\
         inject overlay {\"phone\":{\"app\":\"gatos\"}}\n\
         wait 200\n\
         key j\nkey j\nkey j\nkey Return\n\
         wait 200\n\
         key j\nkey Return\n\
         wait 600\n\
         expect mode CONVERSA\n\
         expect json /ext/overlay/phone null\n\
         expect json /ext/cats/dialog/name \"bravo\"\n\
         quit\n",
        &[],
        &[],
    );
    ok(&r);
}

#[test]
fn the_strip_knows_what_the_cats_know() {
    let r = run(
        "wait 300\n\
         expect json /status/agents [1,1]\n\
         expect json /status/cost_cents 450\n\
         expect json /status/usage_5h_pct 27\n\
         quit\n",
        &[],
        &[],
    );
    ok(&r);
}
