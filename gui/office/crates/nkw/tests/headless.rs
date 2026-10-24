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
    focus_pixel_exact("", &[]);
}

/// Plan §5.5 at a fractional scale: the client renders at the preferred
/// scale (`wp_fractional_scale_v1` + viewport), FOCO places it at integer
/// physical px, 1:1.
#[test]
fn shot_focus_is_pixel_exact_at_1_25() {
    let log = focus_pixel_exact("--fractional", &["--scale", "1.25"]);
    let scale: Vec<_> = common::evs(&log, "scale");
    assert_eq!(
        scale.last().map(|e| e["scale_120"].clone()),
        Some(150.into()),
        "{log:?}"
    );
}

/// FOCO on a checker testclient (`flags` added to it), nkw run with `args`:
/// the shot's crop at the dump's focus rect equals the checker, 0 px off.
/// Returns the client's log.
fn focus_pixel_exact(flags: &str, args: &[&str]) -> Vec<serde_json::Value> {
    if testclient().is_none() {
        return Vec::new();
    }
    let r = run(
        &format!(
            "spawn testclient --app-id acme.check --checker {flags} --log {{dir}}/a.json
wait 200
key Return
wait 200
shot {{dir}}/focus.png
dump {{dir}}/world.json
quit
"
        ),
        args,
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
    r.log("a.json")
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
    // the label's cream badge (2D, MemoryRenderBuffer) inside its rounded
    // corner (wave 4: the corner radius grew) …
    assert_eq!(at(24, 22), [0xf6, 0xec, 0xd8]);
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
    // S1 is measured under reduced motion (plan §1.3): no ambient cat animation
    let r = run(
        "wait 5000\nquit\n",
        &["--reduced-motion"],
        &[("NKW_STATS", "1")],
    );
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
fn mouse_look_turns_the_walker_and_locks_the_pointer() {
    // S14: `pointer` goes through `process_input` like the patched winit
    // backend's relative motion; the headless host records the lock
    let r = run(
        "pointer 50 0
expect json /host/look_events 0
key w
expect mode ANDAR
expect json /host/pointer_locked true
dump {dir}/a.json
pointer 100 0
dump {dir}/b.json
key Escape
expect mode NAV
expect json /host/pointer_locked false
pointer 100 0
expect json /host/look_events 1
quit
",
        &[],
        &[],
    );
    assert_eq!(r.code(), 0, "{}", r.stderr());
    let yaw = |d: &serde_json::Value| d["camera"]["yaw"].as_f64().unwrap();
    let (a, b) = (r.json("a.json"), r.json("b.json"));
    let want = (100.0f64 * 0.12).to_radians();
    assert!(
        (yaw(&b) - yaw(&a) - want).abs() < 1e-3,
        "yaw {} → {} (want +{want})",
        yaw(&a),
        yaw(&b)
    );
    assert_eq!(b["host"]["look_events"], 1);
}

#[test]
fn mouse_look_off_neither_locks_nor_looks() {
    let r = run(
        "key w
expect mode ANDAR
expect json /host/pointer_locked false
pointer 100 0
expect json /host/look_events 0
quit
",
        &["--mouse-look", "off"],
        &[],
    );
    assert_eq!(r.code(), 0, "{}", r.stderr());
}

#[test]
fn buttons_in_nav_reach_no_world_command() {
    // NAV: the world takes no pointer (no client either: nothing happens)
    let r = run(
        "button left down
button left up
button right down
button right up
expect mode NAV
quit
",
        &[],
        &[],
    );
    assert_eq!(r.code(), 0, "{}", r.stderr());
}

#[test]
fn every_plugins_palette_items_reach_comandos() {
    // the objects' media items are no S9 world command: only the hub's
    // `set_palette_items` on `OpenPalette` brings them
    let r = run(
        "key colon
wait 200
expect mode COMANDOS
text \"media.play\"
wait 200
expect json /ext/overlay/palette/top \"media.play_pause\"
quit
",
        &[],
        &[],
    );
    assert_eq!(r.code(), 0, "{}", r.stderr());
}

#[test]
fn a_shot_locks_the_vault_first() {
    // the hub injects {"Shot":true} into the objects before every shot
    let r = run(
        "shot {dir}/s.png
quit
",
        &[],
        &[],
    );
    assert_eq!(r.code(), 0, "{}", r.stderr());
    assert!(r.path("s.png").exists());
}

#[test]
fn the_stats_summary_has_the_wave3_fields() {
    let r = run(
        "wait 300\nquit\n",
        &[],
        &[("NKW_STATS", "slow,ext"), ("NKW_STATS_WARMUP_MS", "100")],
    );
    assert_eq!(r.code(), 0, "{}", r.stderr());
    let s = r.summary();
    for k in ["compose_hud_changes", "look_events"] {
        assert!(s[k].is_u64(), "{k}: {s}");
    }
    for p in ["wm", "objects", "cats", "overlay"] {
        assert!(s["ext_plugin_us_p95"][p].is_u64(), "{p}: {s}");
    }
}

/// The overlay's own raster of the NAV mode label at `scale` (the buffer
/// the hub composes), RGBA.
fn label_buffer(scale: f64) -> (Vec<u8>, f64) {
    use nkw::ext::{Ctx, HudInput};
    let w = nkw::demo::world(
        nkw_core::Output {
            width_px: 1280,
            height_px: 720,
            scale: scale as f32,
        },
        nkw::world::Opts {
            reduced_motion: false,
        },
    );
    let mut o = nkw::overlay::Overlay::new(&nkw::cli::Cli {
        demo: true,
        ..Default::default()
    });
    let pids = std::collections::BTreeMap::new();
    let ctx = Ctx {
        world: &w,
        now: 0.0,
        frame: 0,
        reduced_motion: false,
        demo: true,
        scale,
        size_px: [1280, 720],
        pids: &pids,
        visible_rooms: &[],
        line: None,
    };
    let hud = HudInput {
        mode_label: w.mode().label(),
        badges: Vec::new(),
        dialog: None,
        focus_rect: None,
        tags: Vec::new(),
        pickables: Vec::new(),
        aimed: false,
        status: nkw::ext::Status::default(),
        phone: None,
    };
    let mut els = o.elements(&ctx, &hud);
    let i = els
        .iter()
        .position(|e| e.id == nkw::overlay::LABEL_ID)
        .expect("the mode label");
    let mut e = els.swap_remove(i);
    let mut px = Vec::new();
    let _ = e.buf.render().draw(|b| {
        px = b.to_vec();
        Ok::<_, ()>(Vec::new())
    });
    (px, e.buf_scale)
}

#[test]
fn the_mode_label_is_crisp_at_1_25() {
    // a fractional scale: the overlay rasters at physical px (buf_scale =
    // 1.25) and the hub composes it 1:1, so the shot's label is exactly
    // the overlay's buffer (no 2-px resampling blur)
    let r = run(
        "shot {dir}/l.png\ndump {dir}/l.json\nquit\n",
        &["--scale", "1.25"],
        &[],
    );
    assert_eq!(r.code(), 0, "{}", r.stderr());
    let d = r.json("l.json");
    let placed = d["host"]["hud"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == nkw::overlay::LABEL_ID)
        .unwrap_or_else(|| panic!("no label in {}", d["host"]))
        .clone();
    assert_eq!(placed["buf_scale"], 1.25, "{placed}");
    let at = |i| placed["at"][i].as_f64().unwrap();
    let (x0, y0) = (at(0), at(1));
    assert_eq!(
        (x0.fract(), y0.fract()),
        (0.0, 0.0),
        "whole pixels: {placed}"
    );
    let bw = placed["px"][0].as_f64().unwrap() as u32;
    let bh = placed["px"][1].as_f64().unwrap() as u32;
    let (buf, scale) = label_buffer(1.25);
    assert_eq!(scale, 1.25);
    assert_eq!(buf.len() as u32, bw * bh * 4, "same buffer as the frame's");
    let (w, _, shot) = read_png(&r.path("l.png"));
    let (mut opaque, mut bad) = (0, 0);
    for y in 0..bh {
        for x in 0..bw {
            let b = ((y * bw + x) * 4) as usize;
            if buf[b + 3] != 255 {
                continue; // blended with the world below
            }
            opaque += 1;
            let s = (((y0 as u32 + y) * w + x0 as u32 + x) * 4) as usize;
            if shot[s..s + 3] != buf[b..b + 3] {
                bad += 1;
            }
        }
    }
    assert!(opaque > 100, "{opaque} opaque px");
    assert_eq!(bad, 0, "{bad} of {opaque} label px differ from the buffer");
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

#[test]
fn spawn_with_env_reaches_the_child() {
    // S20: `env K=V` lines (→ `Nkw::spawn_with`) after the hub's own env; a
    // `DISPLAY` given is kept, the host's never leaks
    let r = run(
        "env NKW_T=olá
env DISPLAY=:77
spawn sh -c \"echo $NKW_T $DISPLAY ${WAYLAND_DISPLAY:-none} > {dir}/env.txt\"
quit
",
        &[],
        &[("DISPLAY", ":0")],
    );
    assert_eq!(r.code(), 0, "{}", r.stderr());
    let got = std::fs::read_to_string(r.path("env.txt")).unwrap();
    assert_eq!(got.trim(), "olá :77 nkw-test");
    let r = run(
        "spawn sh -c \"echo ${DISPLAY:-none} > {dir}/env.txt\"\nquit\n",
        &[],
        &[("DISPLAY", ":0")],
    );
    assert_eq!(r.code(), 0, "{}", r.stderr());
    let got = std::fs::read_to_string(r.path("env.txt")).unwrap();
    assert_eq!(got.trim(), "none", "no DISPLAY unless given");
}

#[test]
fn the_protocol_hooks_are_called_at_their_sites() {
    // S20/S23: the hub calls `protos_*` (W1's stub counts them): a relative
    // motion, every presented frame, the mode leaving FOCO
    let r = run(
        "expect json /protocols/calls/relative_motion 0
pointer 10 0
expect json /protocols/calls/relative_motion 1
expect json /protocols/calls/mode_left_focus 0
key J
wait 400
inject wm {\"command\": \"world.focus\", \"args\": {}}
wait json /focus/target \"CatTerm\" 3000
expect mode FOCO
expect json /protocols/calls/mode_left_focus 0
key ctrl+bracketright
expect mode NAV
expect json /protocols/calls/mode_left_focus 1
dump {dir}/dump.json
quit
",
        &[],
        &[],
    );
    assert_eq!(r.code(), 0, "{}", r.stderr());
    let d = r.json("dump.json");
    assert!(
        d["protocols"]["calls"]["presented"].as_u64().unwrap() >= 1,
        "{}",
        d["protocols"]
    );
}

#[test]
fn the_dump_has_status_host_cursor_and_lock_owner() {
    let r = run(
        "expect json /host/cursor \"default\"
expect json /host/pointer_locked_by null
key w
expect mode ANDAR
expect json /host/pointer_locked_by \"walk\"
key Escape
expect json /host/pointer_locked_by null
dump {dir}/dump.json
quit
",
        &[],
        &[],
    );
    assert_eq!(r.code(), 0, "{}", r.stderr());
    let d = r.json("dump.json");
    let s = &d["status"];
    for k in [
        "clock",
        "band",
        "battery",
        "net",
        "playing",
        "vault_open",
        "inbox",
        "agents",
        "cost_cents",
        "usage_5h_pct",
        "xwayland",
    ] {
        assert!(s.get(k).is_some(), "status.{k}: {s}");
    }
    assert!(d["protocols"].is_object());
}

#[test]
fn the_stats_split_import_from_cull() {
    let r = run(
        "wait 300\nquit\n",
        &[],
        &[("NKW_STATS", "1"), ("NKW_STATS_WARMUP_MS", "0")],
    );
    assert_eq!(r.code(), 0, "{}", r.stderr());
    let s = r.summary();
    for k in [
        "import_us_p50",
        "import_us_p95",
        "compose_us_p50",
        "compose_us_p95",
    ] {
        assert!(s[k].is_u64(), "{k}: {s}");
    }
    let frames: Vec<serde_json::Value> = r
        .stderr()
        .lines()
        .filter_map(|l| l.strip_prefix("nkw-stats "))
        .filter_map(|l| serde_json::from_str(l).ok())
        .filter(|v: &serde_json::Value| v.get("pass").is_some())
        .collect();
    assert!(!frames.is_empty());
    for f in &frames {
        assert!(f["import_us"].is_u64(), "{f}");
    }
}
