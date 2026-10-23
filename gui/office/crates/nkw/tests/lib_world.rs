//! R6 end to end on the headless backend (`--demo`: the fixture library in
//! memory; nothing reads the home — one test proves it with a canary).

mod common;

use common::run;

fn cmd(name: &str, args: &str) -> String {
    format!("inject objects {{\"Command\":{{\"name\":\"{name}\",\"args\":{{{args}}}}}}}")
}

#[test]
fn b_walks_into_the_library_and_the_shelves_browse_one_level() {
    let r = run(
        &format!(
            "wait 100
expect json /ext/objects/library/reads 0
key b
wait json /ext/objects/library/browsing true 2000
expect json /ext/objects/library/rel \"\"
expect json /ext/objects/library/entries/3 \"projeto\"
expect json /ext/objects/library/vault_count 1
{next}
{next}
{next}
expect json /ext/objects/library/selected \"projeto\"
{open}
wait json /ext/objects/library/rel \"projeto\" 2000
expect json /ext/objects/library/selected \"docs\"
{back}
wait json /ext/objects/library/rel \"\" 2000
{leave}
expect json /ext/objects/library/browsing false
quit
",
            next = cmd("world.book_next", ""),
            open = cmd("world.book_open", ""),
            back = cmd("world.shelf_back", ""),
            leave = cmd("world.library_leave", ""),
        ),
        &[],
        &[],
    );
    assert_eq!(r.code(), 0, "{}", r.stderr());
}

#[test]
fn the_library_keys_go_through_the_keymap_while_browsing() {
    let r = run(
        "wait 100
key b
wait json /ext/objects/library/browsing true 2000
key l
key l
key l
expect json /ext/objects/library/cursor [0,3]
key Return
wait json /ext/objects/library/rel \"projeto\" 2000
key BackSpace
wait json /ext/objects/library/rel \"\" 2000
key Escape
expect json /ext/objects/library/browsing false
quit
",
        &[],
        &[],
    );
    assert_eq!(r.code(), 0, "{}", r.stderr());
}

#[test]
fn world_open_on_a_vault_path_toasts_and_spawns_nothing() {
    let r = run(
        &format!(
            "wait 100
key b
wait json /ext/objects/library/browsing true 2000
{vault}
expect json /ext/objects/library/last_open null
{env}
expect json /ext/objects/library/last_open null
{fine}
expect json /ext/objects/library/last_open \"notas.md\"
quit
",
            vault = cmd("world.open", "\"path\":\"~/.ssh/id_demo\""),
            env = cmd("world.open", "\"path\":\"projeto/.env\""),
            fine = cmd("world.open", "\"path\":\"~/notas.md\""),
        ),
        &[],
        &[],
    );
    assert_eq!(r.code(), 0, "{}", r.stderr());
}

#[test]
fn search_lectern_and_the_big_fixture() {
    let r = run(
        &format!(
            "wait 100
key b
wait json /ext/objects/library/browsing true 2000
{search}
wait json /ext/objects/library/mode \"search\" 3000
expect json /ext/objects/library/search/names/0 \"projeto/src/lib.rs\"
expect json /ext/objects/library/search/truncated false
key Escape
expect json /ext/objects/library/mode \"shelf\"
{d}
{d}
{d}
{d}
{d}
{d}
expect json /ext/objects/library/selected \"notas.md\"
{preview}
wait json /ext/objects/library/lectern true 2000
dump {{dir}}/lib.json
inject objects {{\"library\":{{\"fixture\":\"big\"}}}}
key Escape
key b
wait 300
{search7}
wait json /ext/objects/library/mode \"search\" 5000
dump {{dir}}/big.json
quit
",
            search = cmd("world.library_search", "\"q\":\"lib\""),
            search7 = cmd("world.library_search", "\"q\":\"arquivo07\""),
            d = cmd("world.book_next", ""),
            preview = cmd("world.book_preview", ""),
        ),
        &[],
        &[],
    );
    assert_eq!(r.code(), 0, "{}", r.stderr());
    let big = r.json("big.json");
    let lib = &big["ext"]["objects"]["library"];
    assert_eq!(lib["source"], "fixture:big");
    let scanned = lib["search"]["scanned"].as_u64().unwrap();
    assert!(scanned > 40_000, "{scanned}");
    let ms = lib["timings"]["search_ms"].as_f64().unwrap();
    // plan §10 fase 4: 50k entries ≤ 1 s (debug build: ≤ 5×)
    assert!(ms < 5000.0, "search took {ms} ms");
    let first = lib["timings"]["first_shelf_ms"].as_f64().unwrap();
    assert!(first < 500.0, "first shelf {first} ms");
}

#[test]
fn demo_never_touches_the_home_a_canary_proves_it() {
    let home = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(home.path().join(".ssh")).unwrap();
    std::fs::write(home.path().join(".ssh/canario-ssh"), "x").unwrap();
    std::fs::write(home.path().join("canario.txt"), "x").unwrap();
    std::fs::create_dir_all(home.path().join("canario-pasta")).unwrap();
    let h = home.path().to_str().unwrap();
    let r = run(
        &format!(
            "wait 100
key b
wait json /ext/objects/library/browsing true 2000
{s}
wait json /ext/objects/library/search/hits 0 3000
dump {{dir}}/d.json
quit
",
            s = cmd("world.library_search", "\"q\":\"canario\""),
        ),
        &[],
        &[("HOME", h), ("XDG_STATE_HOME", h), ("XDG_CONFIG_HOME", h)],
    );
    assert_eq!(r.code(), 0, "{}", r.stderr());
    let d = std::fs::read_to_string(r.path("d.json")).unwrap();
    assert!(!d.contains("canario"), "the home leaked into the dump");
    let v: serde_json::Value = serde_json::from_str(&d).unwrap();
    let lib = &v["ext"]["objects"]["library"];
    assert_eq!(lib["source"], "fixture:demo");
    assert_eq!(
        lib["entries"][3], "projeto",
        "the fixture's shelf, not the home"
    );
    // nothing was written into the fake home either (no vault.log, no state)
    assert!(!home.path().join("nekuroddo").exists());
}
