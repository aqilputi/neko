//! The cats against the `--demo` fixture (`--core none`), headless: the
//! office, CONVERSA with api-gateway, FOCO on a cat's terminal, the inbox
//! and reduced motion. No core, no `$HOME`.

mod common;

use common::run;
use serde_json::Value;

fn ok(r: &common::Run) {
    assert_eq!(r.code(), 0, "{}", r.stderr());
}

#[test]
fn the_fixture_office_sits_at_its_desks() {
    let r = run(
        "wait 300\n\
         expect json /ext/cats/core \"none\"\n\
         expect json /ext/cats/cats/1/label \"api-gateway\"\n\
         expect json /ext/cats/cats/1/pose \"Alert\"\n\
         expect json /rooms/1/name \"acme\"\n\
         expect json /rooms/2/name \"demo\"\n\
         key J\n\
         wait 600\n\
         dump {dir}/d.json\n\
         quit\n",
        &[],
        &[],
    );
    ok(&r);
    let d = r.json("d.json");
    let cats: Vec<Value> = d["desks"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|x| x["cat"].as_u64().map(|c| serde_json::json!([x["room"], c])))
        .collect();
    // the receptionist at the reception; acme 2, api-gateway 3 in acme; 4–6 in demo
    assert_eq!(
        cats,
        [
            serde_json::json!([0, 1]),
            serde_json::json!([1, 2]),
            serde_json::json!([1, 3]),
            serde_json::json!([2, 4]),
            serde_json::json!([2, 5]),
            serde_json::json!([2, 6]),
        ]
    );
    let ext = &d["ext"]["cats"];
    assert_eq!(
        ext["inbox"]["visible"], 0,
        "the demo starts with everyone seated"
    );
    assert!(
        ext["cats"]
            .as_array()
            .unwrap()
            .iter()
            .all(|c| c["walking"] == false)
    );
    assert!(
        ext["monitors"].as_u64().unwrap() >= 2,
        "acme's monitors painted"
    );
}

#[test]
fn the_office_shot_is_stable_under_reduced_motion() {
    let script = "key J\nwait 800\nshot {dir}/a.png\nquit\n";
    let a = run(script, &["--reduced-motion"], &[]);
    ok(&a);
    let b = run(script, &["--reduced-motion"], &[]);
    ok(&b);
    let pa = std::fs::read(a.path("a.png")).unwrap();
    let pb = std::fs::read(b.path("a.png")).unwrap();
    assert!(pa.len() > 10_000);
    assert!(pa == pb, "same input, same picture");
}

#[test]
fn talking_to_api_gateway_answers_its_question() {
    let r = run(
        "key J\n\
         wait 500\n\
         key l\n\
         wait 500\n\
         inject cats {\"cmd\":\"world.talk\"}\n\
         wait 100\n\
         expect mode CONVERSA\n\
         expect json /ext/cats/dialog/name \"api-gateway\"\n\
         wait json /ext/cats/dialog/choices/0 \"sim, aplicar agora\" 3000\n\
         expect json /ext/cats/dialog/choices/1 \"não, revisar antes\"\n\
         shot {dir}/talk.png\n\
         inject cats {\"cmd\":\"world.choice_1\"}\n\
         wait 400\n\
         expect mode NAV\n\
         expect json /ext/cats/dialog null\n\
         expect json /ext/cats/calls/0/reply \"1\"\n\
         wait json /ext/cats/by_label/api-gateway/screen/4 \"> sim, aplicar agora\" 3000\n\
         quit\n",
        &[],
        &[],
    );
    ok(&r);
}

#[test]
fn stopping_a_session_asks_first() {
    let r = run(
        "key J\n\
         wait 500\n\
         inject cats {\"cmd\":\"world.talk\"}\n\
         wait 100\n\
         expect json /ext/cats/dialog/name \"acme\"\n\
         inject cats {\"cmd\":\"world.choice_2\"}\n\
         wait 100\n\
         expect mode CONVERSA\n\
         expect json /ext/cats/calls/0/name \"session.stop\"\n\
         wait json /ext/cats/dialog/choices/0 \"Sim\" 2000\n\
         inject cats {\"cmd\":\"world.choice_1\"}\n\
         wait 300\n\
         expect mode NAV\n\
         expect json /ext/cats/calls/1/call \"confirm\"\n\
         expect json /ext/cats/calls/1/yes true\n\
         quit\n",
        &[],
        &[],
    );
    ok(&r);
}

#[test]
fn focus_on_a_cats_terminal_and_back() {
    let r = run(
        "key J\n\
         wait 500\n\
         inject cats {\"cmd\":\"world.talk\"}\n\
         wait 300\n\
         inject cats {\"cmd\":\"world.talk_sit\"}\n\
         wait 800\n\
         expect mode FOCO\n\
         expect json /ext/cats/pane/cat 2\n\
         key o\n\
         wait 200\n\
         wait json /ext/cats/by_label/acme/screen/9 \"❯ o\" 2000\n\
         shot {dir}/focus.png\n\
         dump {dir}/d.json\n\
         key ctrl+bracketright\n\
         wait 600\n\
         expect mode NAV\n\
         expect json /ext/cats/pane null\n\
         quit\n",
        &[],
        &[],
    );
    ok(&r);
    let d = r.json("d.json");
    let cols = d["ext"]["cats"]["pane"]["cols"].as_u64().unwrap();
    let rows = d["ext"]["cats"]["pane"]["rows"].as_u64().unwrap();
    assert!(cols >= 40 && rows >= 20, "{cols}x{rows}");
}

#[test]
fn the_busy_fixture_walks_three_cats_to_you() {
    let r = run(
        "inject cats {\"fixture\":\"busy\"}\n\
         wait json /ext/cats/inbox/visible 3 4000\n\
         expect json /ext/cats/inbox/queue 3\n\
         wait 3000\n\
         dump {dir}/d.json\n\
         quit\n",
        &[],
        &[],
    );
    ok(&r);
    let d = r.json("d.json");
    let walking: Vec<&str> = d["ext"]["cats"]["cats"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["walking"] == true)
        .map(|c| c["label"].as_str().unwrap())
        .collect();
    assert_eq!(walking, ["api-gateway", "alpha"], "and the postman");
}

#[test]
fn reduced_motion_cats_ask_for_no_rate() {
    let r = run(
        "key J\nwait 3000\nquit\n",
        &["--reduced-motion"],
        &[("NKW_STATS", "1")],
    );
    ok(&r);
    let s = r.summary();
    assert_eq!(s["rate_wishes_max"], 0, "{s}");
    assert!(s["world_passes"].as_u64().unwrap() <= 8, "{s}");
}

#[test]
fn moving_cats_ask_for_frames_only_where_you_look() {
    // at the reception (the receptionist sits still): idle
    let r = run("wait 3000\nquit\n", &[], &[("NKW_STATS", "1")]);
    ok(&r);
    let s = r.summary();
    assert!(s["world_passes"].as_u64().unwrap() <= 3, "{s}");
    // in acme the typing cat asks for 15 fps (how many frames that makes
    // depends on the machine's load: only "more than at rest" is checked)
    let r = run("key J\nwait 2000\nquit\n", &[], &[("NKW_STATS", "1")]);
    ok(&r);
    let s = r.summary();
    assert_eq!(s["rate_wishes_max"], 1, "{s}");
    assert!(s["world_passes"].as_u64().unwrap() >= 5, "{s}");
    // the cats' share of the hooks stays small (all plugins: a few ms at most)
    assert!(s["ext_tick_us_p95"].as_u64().unwrap() < 8_000, "{s}");
}

#[test]
fn reduced_motion_busy_office_holds_still() {
    // S3r: five active cats and the inbox, reduced motion: fixed poses, the
    // walkers cut to their spots, no Rate wish from the cats
    let r = run(
        "key J\n\
         wait 300\n\
         inject cats {\"fixture\":\"busy\"}\n\
         wait json /ext/cats/inbox/visible 3 3000\n\
         wait 4000\n\
         quit\n",
        &["--reduced-motion"],
        &[("NKW_STATS", "1")],
    );
    ok(&r);
    let s = r.summary();
    eprintln!("S3r-like: {s}");
    assert_eq!(s["rate_wishes_max"], 0, "{s}");
    assert!(s["world_passes"].as_u64().unwrap() <= 15, "{s}");
}

#[test]
fn busy_office_measure() {
    // the same without reduced motion, for the numbers (S3-like)
    let r = run(
        "key J\nwait 300\ninject cats {\"fixture\":\"busy\"}\nwait 4300\nquit\n",
        &[],
        &[("NKW_STATS", "1")],
    );
    ok(&r);
    let s = r.summary();
    eprintln!("S3-like: {s}");
    assert!(s["ext_tick_us_p95"].as_u64().unwrap() < 8_000, "{s}");
}
