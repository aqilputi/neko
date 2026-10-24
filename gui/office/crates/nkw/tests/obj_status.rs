//! W6 end to end on the headless backend: the objects' status reaches
//! `dump()["status"]` (the fixture under `--demo`, `inject objects {"status"}`
//! over it), the vault's state with it, and the idle-inhibit forward is only
//! recorded (no portal under `--demo`).

mod common;

use common::run;

#[test]
fn the_demo_status_and_its_overrides_reach_the_dump() {
    let r = run(
        "wait 100
expect json /status/clock [14,5]
expect json /status/band 4
expect json /status/battery [42,false]
expect json /status/net [true,3]
expect json /status/playing false
expect json /status/vault_open false
inject objects {\"status\":{\"battery\":[12,false],\"playing\":true,\"net\":null}}
wait 100
expect json /status/battery [12,false]
expect json /status/playing true
expect json /status/net null
expect json /status/clock [14,5]
dump {dir}/d.json
quit
",
        &[],
        &[],
    );
    assert_eq!(r.code(), 0, "{}", r.stderr());
    let d = r.json("d.json");
    let s = d["status"].to_string();
    assert!(
        !s.contains("Lo-fi") && !s.contains("acme"),
        "no media title: {s}"
    );
}

#[test]
fn the_open_vault_shows_in_the_status_until_a_shot_locks_it() {
    let r = run(
        "wait 100
key b
wait json /ext/objects/library/browsing true 2000
inject objects {\"vault\":{\"phrase\":\"destrancar\"}}
wait json /ext/objects/vault/state \"unlocked\" 1000
wait json /status/vault_open true 500
inject objects {\"Shot\":true}
wait json /status/vault_open false 500
inject objects {\"IdleInhibit\":true}
expect json /ext/objects/idle_inhibit/wanted true
expect json /ext/objects/idle_inhibit/host \"none\"
quit
",
        &[],
        &[],
    );
    assert_eq!(r.code(), 0, "{}", r.stderr());
}
