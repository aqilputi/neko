//! Phase 2 acceptance (plan §10): `nkw --demo --core demo --script
//! talk-gateway` answers api-gateway's question with option 1 in CONVERSA
//! and its monitor shows `> sim, aplicar agora`; stopping a session asks
//! first. Needs `nk` on PATH (it spawns its own `nk serve --demo`, never
//! the user's core): `NKW_E2E=1 cargo test -p nkw --test cats_e2e -- --ignored`.

mod common;

/// The talk-gateway script (the demo's api-gateway is blocked every other
/// 15 s step of its timeline).
pub const TALK_GATEWAY: &str = "\
wait json /ext/cats/core \"ok\" 15000
wait json /ext/cats/by_label/api-gateway/state \"blocked\" 34000
inject cats {\"talk\":\"api-gateway\"}
wait json /ext/cats/dialog/choices/0 \"sim, aplicar agora\" 5000
expect mode CONVERSA
inject cats {\"cmd\":\"world.choice_1\"}
wait 300
expect mode NAV
wait 5000
dump {dir}/d.json
quit
";

fn enabled() -> bool {
    let on = std::env::var("NKW_E2E").as_deref() == Ok("1");
    let nk = std::process::Command::new("nk")
        .arg("--help")
        .output()
        .is_ok();
    if on && !nk {
        eprintln!("SKIP: nk not on PATH");
    }
    on && nk
}

#[test]
#[ignore = "NKW_E2E=1 and nk on PATH"]
fn talk_gateway_answers_with_option_1() {
    if !enabled() {
        return;
    }
    let r = common::run(TALK_GATEWAY, &["--core", "demo"], &[]);
    assert_eq!(r.code(), 0, "{}", r.stderr());
    let d = r.json("d.json");
    let screen = &d["ext"]["cats"]["by_label"]["api-gateway"]["screen"];
    let lines: Vec<&str> = screen
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|l| l.as_str())
        .collect();
    assert!(
        lines.iter().any(|l| l.contains("> sim, aplicar agora")),
        "the monitor: {lines:#?}"
    );
}

#[test]
#[ignore = "NKW_E2E=1 and nk on PATH"]
fn stopping_a_session_asks_and_works() {
    if !enabled() {
        return;
    }
    let r = common::run(
        "wait json /ext/cats/core \"ok\" 15000\n\
         wait json /ext/cats/by_label/indexador/state \"busy\" 34000\n\
         inject cats {\"talk\":\"indexador\"}\n\
         wait json /ext/cats/dialog/choices/1 \"Parar\" 5000\n\
         inject cats {\"cmd\":\"world.choice_2\"}\n\
         wait json /ext/cats/dialog/choices/0 \"Sim\" 5000\n\
         dump {dir}/ask.json\n\
         inject cats {\"cmd\":\"world.choice_1\"}\n\
         wait 300\n\
         expect mode NAV\n\
         wait 2000\n\
         dump {dir}/d.json\n\
         quit\n",
        &["--core", "demo"],
        &[],
    );
    assert_eq!(r.code(), 0, "{}", r.stderr());
    let ask = r.json("ask.json");
    assert!(
        ask["ext"]["cats"]["dialog"]["pending"].is_string(),
        "a confirmation token"
    );
    let d = r.json("d.json");
    let state = d["ext"]["cats"]["by_label"]["indexador"]["state"]
        .as_str()
        .unwrap_or("gone");
    assert_ne!(state, "busy", "stopped");
}
