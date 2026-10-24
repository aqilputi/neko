//! The wave-4 protocols (S23), headless, with the testclient's flags:
//! cursor shape → the host cursor in FOCO, primary selection between two
//! clients, presentation feedback, a client's pointer lock in FOCO, content
//! type, single-pixel buffers, xdg-foreign handles, idle inhibitors.

mod common;

use common::{evs, run, testclient};
use serde_json::Value;

const NEW_GLOBALS: [&str; 10] = [
    "wp_cursor_shape_manager_v1",
    "zwp_primary_selection_device_manager_v1",
    "wp_presentation",
    "zwp_pointer_constraints_v1",
    "zwp_relative_pointer_manager_v1",
    "wp_single_pixel_buffer_manager_v1",
    "wp_content_type_manager_v1",
    "zxdg_exporter_v2",
    "zxdg_importer_v2",
    "zwp_idle_inhibit_manager_v1",
];

fn names(log: &[Value]) -> Vec<String> {
    evs(log, "global")
        .iter()
        .map(|g| g["name"].as_str().unwrap_or("").to_string())
        .collect()
}

#[test]
fn the_new_globals_are_advertised() {
    let Some(tc) = testclient() else { return };
    let tc = tc.display().to_string();
    let r = run(
        &format!("spawn {tc} --list-globals --log {{dir}}/g.json\ndump {{dir}}/d.json\nquit\n"),
        &[],
        &[],
    );
    assert_eq!(r.code(), 0, "{}", r.stderr());
    let got = names(&r.log("g.json"));
    for g in NEW_GLOBALS {
        assert!(got.iter().any(|n| n == g), "{g} missing from {got:?}");
    }
    assert!(
        !got.iter().any(|n| n == "zwp_text_input_manager_v3"),
        "text-input only with --text-input"
    );
    // the module says what it advertised
    let d = r.json("d.json");
    let listed: Vec<&str> = d["protocols"]["globals"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .collect();
    for g in NEW_GLOBALS {
        assert!(listed.contains(&g), "{g} not in dump {listed:?}");
    }
}

#[test]
fn text_input_only_behind_its_flag() {
    let Some(tc) = testclient() else { return };
    let tc = tc.display().to_string();
    let r = run(
        &format!("spawn {tc} --list-globals --log {{dir}}/g.json\nquit\n"),
        &["--text-input"],
        &[],
    );
    assert_eq!(r.code(), 0, "{}", r.stderr());
    assert!(
        names(&r.log("g.json"))
            .iter()
            .any(|n| n == "zwp_text_input_manager_v3")
    );
}

#[test]
fn a_cursor_shape_reaches_the_host_in_foco_and_resets_on_leaving() {
    if testclient().is_none() {
        return;
    }
    let r = run(
        "spawn testclient --app-id alpha --cursor-shape text --log {dir}/a.json
wait 200
expect json /host/cursor \"default\"
key Return
expect mode FOCO
wait json /host/cursor \"text\" 2000
key ctrl+bracketright
expect mode NAV
expect json /host/cursor \"default\"
quit
",
        &[],
        &[],
    );
    assert_eq!(r.code(), 0, "{}", r.stderr());
    let log = r.log("a.json");
    assert!(!evs(&log, "pointer_enter").is_empty(), "{log:?}");
    assert_eq!(evs(&log, "cursor_shape_set")[0]["shape"], 9);
}

#[test]
fn primary_selection_goes_from_one_client_to_another() {
    if testclient().is_none() {
        return;
    }
    let r = run(
        "spawn testclient --app-id alpha --primary \"olá primária\" --log {dir}/a.json
wait 200
key Return
wait 200
key ctrl+bracketright
wait json /protocols/primary true 2000
spawn testclient --app-id bravo --log-primary --log {dir}/b.json
wait 200
key Return
wait 400
quit
",
        &[],
        &[],
    );
    assert_eq!(r.code(), 0, "{}", r.stderr());
    assert_eq!(evs(&r.log("a.json"), "primary_set").len(), 1);
    let got = evs(&r.log("b.json"), "primary")
        .iter()
        .map(|e| e["text"].as_str().unwrap_or("").to_string())
        .collect::<Vec<_>>();
    assert!(got.iter().any(|t| t == "olá primária"), "{got:?}");
}

#[test]
fn presentation_feedback_comes_at_the_hosts_period() {
    if testclient().is_none() {
        return;
    }
    let r = run(
        "spawn testclient --app-id alpha --presentation --log {dir}/a.json
wait 200
key Return
wait 300
quit
",
        &[],
        &[],
    );
    assert_eq!(r.code(), 0, "{}", r.stderr());
    let log = r.log("a.json");
    let p = evs(&log, "presented");
    assert!(p.len() >= 3, "{} presented in 300 ms of FOCO", p.len());
    for e in &p {
        assert_eq!(e["flags"].as_u64().unwrap() & 1, 1, "vsync flag: {e}");
        let refresh = e["refresh_ns"].as_u64().unwrap();
        assert!((16_000_000..17_500_000).contains(&refresh), "{e}");
    }
    let seqs: Vec<u64> = p.iter().map(|e| e["seq"].as_u64().unwrap()).collect();
    assert!(seqs.windows(2).all(|w| w[1] > w[0]), "{seqs:?}");
    // CLOCK_MONOTONIC: the same clock as the client's t_us
    let last = p.last().unwrap();
    let at_us =
        last["tv_sec"].as_u64().unwrap() * 1_000_000 + last["tv_nsec"].as_u64().unwrap() / 1000;
    let logged = last["t_us"].as_u64().unwrap();
    assert!(
        at_us <= logged && logged - at_us < 1_000_000,
        "{at_us} vs {logged}"
    );
}

#[test]
fn a_client_lock_in_foco_gets_relative_motion_until_ctrl_bracket() {
    if testclient().is_none() {
        return;
    }
    let r = run(
        "spawn testclient --app-id alpha --lock-pointer --log {dir}/a.json
wait 200
key Return
expect mode FOCO
wait json /host/pointer_locked_by \"client\" 2000
pointer 10 0
pointer 0 -4
wait 200
expect json /host/look_events 0
key ctrl+bracketright
expect mode NAV
expect json /host/pointer_locked_by null
expect json /protocols/locked_by null
wait 200
quit
",
        &[],
        &[],
    );
    assert_eq!(r.code(), 0, "{}", r.stderr());
    let log = r.log("a.json");
    assert_eq!(evs(&log, "locked").len(), 1, "{log:?}");
    let m: Vec<(f64, f64)> = evs(&log, "relative_motion")
        .iter()
        .map(|e| (e["dx"].as_f64().unwrap(), e["dy"].as_f64().unwrap()))
        .collect();
    assert_eq!(m, [(10.0, 0.0), (0.0, -4.0)]);
    assert_eq!(evs(&log, "unlocked").len(), 1, "{log:?}");
}

#[test]
fn content_type_is_read_on_commit() {
    if testclient().is_none() {
        return;
    }
    let r = run(
        "spawn testclient --app-id alpha --content-type video
wait 200
dump {dir}/d.json
quit
",
        &[],
        &[],
    );
    assert_eq!(r.code(), 0, "{}", r.stderr());
    let d = r.json("d.json");
    let id = d["screens"][0]["id"].as_u64().unwrap().to_string();
    assert_eq!(
        d["protocols"]["content_types"][&id], "video",
        "{}",
        d["protocols"]
    );
}

#[test]
fn a_single_pixel_window_is_its_colour_in_foco() {
    if testclient().is_none() {
        return;
    }
    let r = run(
        "spawn testclient --app-id alpha --single-pixel ff4000ff --size 320x200
wait 300
key Return
wait 300
shot {dir}/s.png
dump {dir}/d.json
quit
",
        &[],
        &[],
    );
    assert_eq!(r.code(), 0, "{}", r.stderr());
    let d = r.json("d.json");
    let rect: Vec<u32> = d["focus"]["rect"]
        .as_array()
        .unwrap_or_else(|| panic!("no focus in {d}"))
        .iter()
        .map(|v| v.as_f64().unwrap() as u32)
        .collect();
    assert!(rect[2] > 100 && rect[3] > 100, "{rect:?}");
    let (w, px) = read_png(&r.path("s.png"));
    let mut bad = 0;
    for y in (rect[1]..rect[1] + rect[3]).step_by(7) {
        for x in (rect[0]..rect[0] + rect[2]).step_by(7) {
            let i = ((y * w + x) * 4) as usize;
            if px[i..i + 3] != [0xff, 0x40, 0x00] {
                bad += 1;
            }
        }
    }
    assert_eq!(
        bad, 0,
        "{bad} sampled pixels are not the colour in {rect:?}"
    );
}

#[test]
fn an_exported_toplevel_gets_a_handle() {
    if testclient().is_none() {
        return;
    }
    let r = run(
        "spawn testclient --app-id alpha --export --log {dir}/a.json
wait 300
quit
",
        &[],
        &[],
    );
    assert_eq!(r.code(), 0, "{}", r.stderr());
    let log = r.log("a.json");
    let h = evs(&log, "exported");
    assert_eq!(h.len(), 1, "{log:?}");
    assert!(!h[0]["handle"].as_str().unwrap().is_empty());
}

#[test]
fn idle_inhibitors_are_counted_and_released_with_their_client() {
    if testclient().is_none() {
        return;
    }
    let r = run(
        "spawn testclient --app-id alpha --idle-inhibit --exit-after-ms 1200 --log {dir}/a.json
wait json /protocols/inhibitors 1 2000
wait json /protocols/inhibitors 0 4000
quit
",
        &[],
        &[],
    );
    assert_eq!(r.code(), 0, "{}", r.stderr());
    assert_eq!(evs(&r.log("a.json"), "idle_inhibit").len(), 1);
}

fn read_png(p: &std::path::Path) -> (u32, Vec<u8>) {
    let dec = png::Decoder::new(std::fs::File::open(p).unwrap());
    let mut r = dec.read_info().unwrap();
    let mut buf = vec![0; r.output_buffer_size()];
    let info = r.next_frame(&mut buf).unwrap();
    buf.truncate(info.buffer_size());
    assert_eq!(info.color_type, png::ColorType::Rgba);
    (info.width, buf)
}
