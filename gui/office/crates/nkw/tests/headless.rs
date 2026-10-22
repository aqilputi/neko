//! End to end on the headless backend: real Wayland clients
//! (`nkw-testclient`), input through `--script`, asserts on the client's
//! log, `dump` and `shot`.

mod common;

use common::{evs, keys, run, t_us, testclient};

const SPAWN_A: &str = "spawn testclient --app-id acme.alpha --log {dir}/a.json";

#[test]
fn focus_receives_keys_nav_does_not() {
    if testclient().is_none() {
        return;
    }
    let r = run(
        &format!(
            "{SPAWN_A}
key k
key x
key Escape
expect mode NAV
key Return
expect mode FOCO
expect focus acme.alpha
key a
wait 100
quit
"
        ),
        &[],
        &[],
    );
    assert_eq!(r.code(), 0, "{}", r.stderr());
    let log = r.log("a.json");
    let k = keys(&log);
    assert_eq!(
        k,
        [
            ("a".to_string(), "pressed".to_string()),
            ("a".into(), "released".into())
        ],
        "only the FOCO key reaches the client: {k:?}"
    );
    assert_eq!(evs(&log, "enter").len(), 1);
}

#[test]
fn ctrl_bracket_releases_held_keys() {
    if testclient().is_none() {
        return;
    }
    let r = run(
        &format!(
            "{SPAWN_A}
key Return
down a
key ctrl+bracketright
expect mode NAV
up a
wait 100
quit
"
        ),
        &[],
        &[],
    );
    assert_eq!(r.code(), 0, "{}", r.stderr());
    let log = r.log("a.json");
    let leave = evs(&log, "leave");
    assert_eq!(leave.len(), 1, "{log:?}");
    let t_leave = t_us(leave[0]);
    let k = keys(&log);
    assert!(k.contains(&("a".into(), "pressed".into())), "{k:?}");
    // the held `a` ends with the leave: no release arrives afterwards, and
    // the escape chord itself never reaches the client
    for e in evs(&log, "key") {
        assert!(t_us(e) < t_leave || e["state"] != "released", "{e}");
        assert_ne!(e["sym"], "bracketright", "{e}");
    }
}

#[test]
fn popup_done_on_focus_exit() {
    if testclient().is_none() {
        return;
    }
    let r = run(
        "spawn testclient --app-id acme.alpha --popup-on-key --log {dir}/a.json
key Return
key p
wait 200
key ctrl+bracketright
wait 200
quit
",
        &[],
        &[],
    );
    assert_eq!(r.code(), 0, "{}", r.stderr());
    let log = r.log("a.json");
    assert_eq!(evs(&log, "popup_done").len(), 1, "{log:?}");
}

#[test]
fn frame_callbacks_out_of_view_le_1_2_per_s_over_3s() {
    if testclient().is_none() {
        return;
    }
    let r = run(
        "spawn testclient --app-id acme.alpha --log {dir}/a.json
spawn testclient --app-id acme.bravo --log {dir}/b.json
key Return
expect focus acme.bravo
wait 3300
quit
",
        &[],
        &[],
    );
    assert_eq!(r.code(), 0, "{}", r.stderr());
    let a = r.log("a.json");
    let b = r.log("b.json");
    let t0 = t_us(evs(&b, "enter")[0]) + 300_000;
    let t1 = t0 + 3_000_000;
    let n = evs(&a, "frame")
        .iter()
        .filter(|e| (t0..t1).contains(&t_us(e)))
        .count();
    assert!(n as f64 <= 1.2 * 3.0, "{n} callbacks in 3 s behind FOCO");
    // the focused one runs at the frame rate
    let nb = evs(&b, "frame")
        .iter()
        .filter(|e| (t0..t1).contains(&t_us(e)))
        .count();
    assert!(nb > 120, "focused client got {nb} frames in 3 s");
}

#[test]
fn shot_focus_is_pixel_exact() {
    if testclient().is_none() {
        return;
    }
    let r = run(
        "spawn testclient --app-id acme.check --checker --log {dir}/a.json
key Return
shot {dir}/focus.png
dump {dir}/world.json
quit
",
        &[],
        &[],
    );
    assert_eq!(r.code(), 0, "{}", r.stderr());
    let d = r.json("world.json");
    let rect: Vec<u32> = d["focus"]["rect"]
        .as_array()
        .unwrap_or_else(|| panic!("no focus in {d}"))
        .iter()
        .map(|v| v.as_f64().unwrap() as u32)
        .collect();
    let (w, _h, px) = read_png(&r.path("focus.png"));
    let at = |x: u32, y: u32| {
        let i = ((y * w + x) * 4) as usize;
        [px[i], px[i + 1], px[i + 2]]
    };
    let first = at(rect[0], rect[1]);
    assert!(first == [0, 0, 0] || first == [255, 255, 255], "{first:?}");
    let other = if first == [0, 0, 0] { [255; 3] } else { [0; 3] };
    let mut bad = 0;
    for y in 0..rect[3] {
        for x in 0..rect[2] {
            let want = if (x + y) % 2 == 0 { first } else { other };
            if at(rect[0] + x, rect[1] + y) != want {
                bad += 1;
            }
        }
    }
    assert_eq!(bad, 0, "{bad} pixels differ in the FOCO crop {rect:?}");
}

#[test]
fn nav_shot_composes_the_hud_over_the_world() {
    let r = run("shot {dir}/nav.png\nquit\n", &[], &[]);
    assert_eq!(r.code(), 0, "{}", r.stderr());
    let (w, h, px) = read_png(&r.path("nav.png"));
    assert_eq!((w, h), (1280, 720));
    let at = |x: u32, y: u32| {
        let i = ((y * w + x) * 4) as usize;
        [px[i], px[i + 1], px[i + 2]]
    };
    // the label's cream badge (2D, MemoryRenderBuffer) at its corner …
    assert_eq!(at(18, 18), [0xf6, 0xec, 0xd8]);
    // … over a world pass that drew something other than the clear colour
    let sky = at(640, 5);
    let floor = at(640, 715);
    assert_ne!(sky, floor, "the world pass drew nothing");
}

#[test]
fn no_gl_error_with_gl_debug() {
    if testclient().is_none() {
        return;
    }
    let r = run(
        "spawn testclient --app-id acme.alpha --log {dir}/a.json
key l
key h
key Return
wait 300
key ctrl+bracketright
wait 300
quit
",
        &["--gl-debug"],
        &[],
    );
    assert_eq!(r.code(), 0, "{}", r.stderr());
    assert!(!r.stderr().contains("GL error"), "{}", r.stderr());
}

#[test]
fn idle_demo_takes_no_world_passes() {
    let r = run("wait 5000\nquit\n", &[], &[("NKW_STATS", "1")]);
    assert_eq!(r.code(), 0, "{}", r.stderr());
    let s = r.summary();
    assert!(s["world_passes"].as_u64().unwrap() <= 3, "{s}");
    assert!(s["frames"].as_u64().unwrap() <= 3, "{s}");
    assert!(s["ext_tick_us_p95"].is_u64(), "{s}");
    assert_eq!(s["rate_wishes_max"], 0, "{s}");
}

#[test]
fn andar_walks_with_held_keys_and_exits_to_nav() {
    let r = run(
        "dump {dir}/nav.json
key w
expect mode ANDAR
down w
wait 400
up w
down e
wait 300
up e
wait 400
dump {dir}/walk.json
key Escape
expect mode NAV
quit
",
        &[],
        &[],
    );
    assert_eq!(r.code(), 0, "{}", r.stderr());
    let (a, b) = (r.json("nav.json"), r.json("walk.json"));
    let z = |d: &serde_json::Value| d["camera"]["pos"][2].as_f64().unwrap();
    assert!(z(&b) < z(&a) - 0.3, "walked forward: {} → {}", z(&a), z(&b));
    assert!(b["camera"]["yaw"].as_f64().unwrap() > 0.2, "turned right");
    assert_eq!(b["mode"], "ANDAR");
}

#[test]
fn expect_mismatch_exits_3() {
    let r = run("expect mode FOCO\n", &[], &[]);
    assert_eq!(r.code(), 3);
    assert!(
        r.stderr().contains("expected mode FOCO, actual NAV"),
        "{}",
        r.stderr()
    );
}

#[test]
fn dump_has_the_ext_section() {
    let r = run(
        "dump {dir}/world.json
expect json /version 1
expect json /mode \"NAV\"
expect json /ext/router/world_mod \"ctrl+alt\"
inject cats {\"postman\": {\"to\": \"alpha\"}}
wait json /mode \"NAV\" 500
quit
",
        &[],
        &[],
    );
    assert_eq!(r.code(), 0, "{}", r.stderr());
    let d = r.json("world.json");
    assert_eq!(d["version"], 1);
    for k in ["wm", "overlay", "cats", "objects", "router"] {
        assert!(d["ext"].get(k).is_some(), "ext.{k} missing: {}", d["ext"]);
    }
}

#[test]
fn expect_json_mismatch_exits_3() {
    let r = run("expect json /mode \"FOCO\"\n", &[], &[]);
    assert_eq!(r.code(), 3);
    assert!(
        r.stderr()
            .contains("script line 1: expected json /mode = \"FOCO\", actual \"NAV\""),
        "{}",
        r.stderr()
    );
}

#[test]
fn wait_json_times_out_with_exit_3() {
    let r = run("key l\nwait json /mode \"FOCO\" 300\nquit\n", &[], &[]);
    assert_eq!(r.code(), 3);
    assert!(
        r.stderr().contains("wait json: expected json /mode"),
        "{}",
        r.stderr()
    );
}

#[test]
fn wait_json_sees_the_world_move() {
    let r = run(
        "key J\nwait json /station/desk/room 1 2000\nexpect json /station/desk/desk 0\nquit\n",
        &[],
        &[],
    );
    assert_eq!(r.code(), 0, "{}", r.stderr());
}

#[test]
fn script_errors_exit_2() {
    let r = run("bogus\n", &[], &[]);
    assert_eq!(r.code(), 2);
    assert!(r.stderr().contains("script line 1"), "{}", r.stderr());
}

#[test]
fn no_runtime_dir_exits_5() {
    let out = std::process::Command::new(common::nkw_bin())
        .args(["--headless", "--seconds", "1"])
        .env_remove("XDG_RUNTIME_DIR")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(5));
}

fn read_png(p: &std::path::Path) -> (u32, u32, Vec<u8>) {
    let dec = png::Decoder::new(std::fs::File::open(p).unwrap());
    let mut r = dec.read_info().unwrap();
    let mut buf = vec![0; r.output_buffer_size()];
    let info = r.next_frame(&mut buf).unwrap();
    buf.truncate(info.buffer_size());
    assert_eq!(info.color_type, png::ColorType::Rgba);
    (info.width, info.height, buf)
}
