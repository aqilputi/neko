//! ANDAR and the WM's wave-3 pieces end to end (R2) on the headless backend
//! under `--demo`: held walking keys by name, carrying a screen and dropping
//! it in the air, `world.focus` on a cat's desk, `world.library`, the
//! placement intent and `world.copy`.

mod common;

use common::{evs, run, testclient};

/// `inject wm` of a command by name with string args.
fn cmd(name: &str, args: &[(&str, &str)]) -> String {
    let a: Vec<String> = args
        .iter()
        .map(|(k, v)| format!("\"{k}\": \"{v}\""))
        .collect();
    format!(
        "inject wm {{\"command\": \"{name}\", \"args\": {{{}}}}}",
        a.join(", ")
    )
}

fn dump(r: &common::Run) -> serde_json::Value {
    r.json("dump.json")
}

#[test]
fn held_keys_by_name_walk_and_stop() {
    let r = run(
        &format!(
            "key J
wait 400
key w
wait 100
expect mode ANDAR
dump {{dir}}/before.json
{back}
wait 600
{stop}
wait 400
dump {{dir}}/dump.json
quit
",
            back = cmd("world.back", &[("pressed", "true")]),
            stop = cmd("world.back", &[("pressed", "false")]),
        ),
        &[],
        &[],
    );
    assert_eq!(r.code(), 0, "{}", r.stderr());
    let d = dump(&r);
    assert_eq!(d["mode"], "ANDAR", "world.back released does not leave");
    let before = r.json("before.json");
    let pos = |v: &serde_json::Value| -> Vec<f64> {
        v["camera"]["pos"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x.as_f64().unwrap())
            .collect()
    };
    let (a, b) = (pos(&before), pos(&d));
    let moved = ((a[0] - b[0]).powi(2) + (a[2] - b[2]).powi(2)).sqrt();
    assert!(moved > 0.5, "walked back {moved} m");
    let wants = d["wants"].as_array().unwrap();
    assert!(
        !wants.iter().any(|w| w == "world:continuous"),
        "released: stopped ({wants:?})"
    );
}

#[test]
fn held_keys_through_the_router_stop_on_release() {
    let r = run(
        "key J
wait 400
key w
wait 100
expect mode ANDAR
dump {dir}/before.json
down w
wait 600
up w
wait 400
dump {dir}/dump.json
quit
",
        &[],
        &[],
    );
    assert_eq!(r.code(), 0, "{}", r.stderr());
    let (a, b) = (r.json("before.json"), dump(&r));
    let z = |v: &serde_json::Value| v["camera"]["pos"][2].as_f64().unwrap();
    assert!((z(&a) - z(&b)).abs() > 0.5, "walked: {} → {}", z(&a), z(&b));
    let wants = b["wants"].as_array().unwrap();
    assert!(
        !wants.iter().any(|w| w == "world:continuous"),
        "released: stopped ({wants:?})"
    );
}

#[test]
fn world_focus_on_a_cats_desk_is_its_terminal() {
    let r = run(
        &format!(
            "key J
wait 400
expect json /station/desk/desk 0
{focus}
wait json /focus/target \"CatTerm\" 3000
expect mode FOCO
dump {{dir}}/dump.json
quit
",
            focus = cmd("world.focus", &[]),
        ),
        &[],
        &[],
    );
    assert_eq!(r.code(), 0, "{}", r.stderr());
    assert!(dump(&r)["focus"]["cat"].is_u64());
}

#[test]
fn world_library_makes_the_room_after_the_reception() {
    let r = run(
        &format!(
            "{lib}
wait 400
expect json /rooms/1/kind \"library\"
expect json /rooms/1/name \"biblioteca\"
{lib}
wait 200
dump {{dir}}/dump.json
quit
",
            lib = cmd("world.library", &[]),
        ),
        &[],
        &[],
    );
    assert_eq!(r.code(), 0, "{}", r.stderr());
    let d = dump(&r);
    let libs = d["rooms"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["kind"] == "library")
        .count();
    assert_eq!(libs, 1, "once");
    assert_eq!(
        d["station"]["desk"]["room"], d["rooms"][1]["id"],
        "went there"
    );
    assert_eq!(d["station"]["desk"]["desk"], 0, "the counter");
    let counter = d["desks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|k| k["room"] == d["rooms"][1]["id"])
        .unwrap();
    assert_eq!(counter["cat"], 999_000, "the librarian's");
}

#[test]
fn grab_walk_back_and_drop_in_the_air() {
    if testclient().is_none() {
        return;
    }
    let r = run(
        &format!(
            "key J
spawn testclient --app-id acme.alpha --log {{dir}}/a.json
wait json /screens/0/size [624.0,720.0] 3000
wait 300
key w
wait 200
wait json /aim/Screen 1 2000
{grab}
expect json /carrying 1
expect json /screens/0/hand true
down s
wait 1000
up s
wait 500
{grab}
wait 200
expect json /carrying null
expect json /screens/0/air true
dump {{dir}}/dump.json
quit
",
            grab = cmd("world.grab", &[]),
        ),
        &[],
        &[],
    );
    assert_eq!(r.code(), 0, "{}", r.stderr());
    let d = dump(&r);
    assert_eq!(d["screens"][0]["wall"], false);
}

#[test]
fn the_placement_intent_puts_the_next_window_on_its_desk() {
    if testclient().is_none() {
        return;
    }
    let r = run(
        &format!(
            "{next}
spawn testclient --app-id acme.alpha --log {{dir}}/a.json
wait json /screens/0/placement/room 2 3000
expect json /screens/0/placement/desk 0
spawn testclient --app-id acme.bravo --log {{dir}}/b.json
wait json /screens/1/size [624.0,720.0] 3000
dump {{dir}}/dump.json
quit
",
            next = cmd("world.place_next", &[("room", "demo"), ("desk", "0")]),
        ),
        &[],
        &[],
    );
    assert_eq!(r.code(), 0, "{}", r.stderr());
    let d = dump(&r);
    let at = |app: &str| {
        let s = d["screens"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["app_id"] == app)
            .unwrap_or_else(|| panic!("{app}: {d}"));
        (
            s["placement"]["room"].as_u64().unwrap(),
            s["placement"]["desk"].as_u64().unwrap(),
        )
    };
    assert_eq!(at("acme.alpha"), (2, 0), "the intent");
    assert_eq!(at("acme.bravo"), (2, 1), "only once: right of the station");
}

#[test]
fn world_copy_reaches_a_client_and_never_the_dump() {
    if testclient().is_none() {
        return;
    }
    let r = run(
        &format!(
            "{copy}
wait 200
spawn testclient --app-id acme.bravo --log-selection --log {{dir}}/b.json
wait json /screens/0/size [624.0,720.0] 3000
key Return
wait 500
dump {{dir}}/dump.json
quit
",
            copy = cmd("world.copy", &[("text", "projeto/notas.md")]),
        ),
        &[],
        &[],
    );
    assert_eq!(r.code(), 0, "{}", r.stderr());
    let b = r.log("b.json");
    let sel = evs(&b, "selection");
    assert!(sel.iter().any(|e| e["text"] == "projeto/notas.md"), "{b:?}");
    let text = std::fs::read_to_string(r.path("dump.json")).unwrap();
    assert!(!text.contains("notas"), "no clipboard content in the dump");
    assert!(!r.stderr().contains("notas"), "nor in the log");
}
