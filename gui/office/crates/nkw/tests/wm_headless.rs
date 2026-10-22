//! The window manager end to end (P2) on the headless backend with real
//! clients (`nkw-testclient`): configure sizes per preset, dialogs on the
//! mother's desk, fullscreen in FOCO, the selection between two clients,
//! `world.close`, and `world.json` across a restart.

mod common;

use std::path::Path;
use std::process::Command;

use common::{evs, read_log, run, testclient};

/// `inject wm` of a command by name.
fn cmd(name: &str) -> String {
    format!("inject wm {{\"command\": \"{name}\"}}")
}

#[test]
fn configure_sizes_follow_the_presets() {
    if testclient().is_none() {
        return;
    }
    let r = run(
        &format!(
            "spawn testclient --app-id acme.alpha --log {{dir}}/a.json
wait json /screens/0/size [624.0,720.0] 3000
{wider}
wait json /screens/0/size [837.0,720.0] 3000
expect json /desks/1/width \"two_thirds\"
expect json /ext/wm/rooms/0/kind \"reception\"
{wider}
wait json /screens/0/size [1264.0,720.0] 3000
{narrower}
{narrower}
{narrower}
wait json /screens/0/size [410.0,720.0] 3000
expect json /desks/1/width \"third\"
quit
",
            wider = cmd("world.wider"),
            narrower = cmd("world.narrower"),
        ),
        &[],
        &[],
    );
    assert_eq!(r.code(), 0, "{}", r.stderr());
    let log = r.log("a.json");
    let sizes: Vec<(u64, u64)> = evs(&log, "configure")
        .iter()
        .map(|e| (e["w"].as_u64().unwrap_or(0), e["h"].as_u64().unwrap_or(0)))
        .collect();
    assert!(sizes.contains(&(837, 720)), "{sizes:?}");
    assert!(sizes.contains(&(410, 720)), "{sizes:?}");
}

#[test]
fn a_dialog_lands_on_its_mothers_desk() {
    if testclient().is_none() {
        return;
    }
    let r = run(
        "spawn testclient --parent-of acme.mother --app-id acme.dialog --size 300x200
wait json /screens/1/parent 1 3000
expect json /screens/0/app_id \"acme.mother\"
expect json /screens/1/placement/desk 1
expect json /screens/0/placement/desk 1
wait json /screens/1/size [300.0,200.0] 3000
key Return
expect mode FOCO
quit
",
        &[],
        &[],
    );
    assert_eq!(r.code(), 0, "{}", r.stderr());
}

#[test]
fn fullscreen_in_foco_runs_no_world_pass() {
    if testclient().is_none() {
        return;
    }
    // the client redraws every frame; in FOCO that only composes
    let script = |ms: u32| {
        format!(
            "spawn testclient --app-id acme.tv --damage 64x16 --fullscreen --log {{dir}}/a.json
wait json /screens/0/fullscreen true 3000
wait json /screens/0/size [1280.0,720.0] 3000
key Return
expect mode FOCO
wait 600
expect json /focus/rect [0.0,0.0,1280.0,720.0]
wait {ms}
quit
"
        )
    };
    let short = run(&script(100), &[], &[("NKW_STATS", "1")]);
    assert_eq!(short.code(), 0, "{}", short.stderr());
    let long = run(&script(1500), &[], &[("NKW_STATS", "1")]);
    assert_eq!(long.code(), 0, "{}", long.stderr());
    let passes = |r: &common::Run| r.summary()["world_passes"].as_u64().unwrap();
    let frames = |r: &common::Run| evs(&r.log("a.json"), "frame").len();
    assert!(
        frames(&long) > frames(&short) + 30,
        "the client kept drawing"
    );
    assert!(
        passes(&long) <= passes(&short) + 1,
        "world passes flat in FOCO: {} vs {}",
        passes(&long),
        passes(&short)
    );
}

#[test]
fn the_selection_goes_from_one_client_to_another() {
    if testclient().is_none() {
        return;
    }
    let r = run(
        "spawn testclient --app-id acme.alpha --copy olá --log {dir}/a.json
key Return
wait 200
key ctrl+bracketright
spawn testclient --app-id acme.bravo --log-selection --log {dir}/b.json
key Return
wait 400
dump {dir}/dump.json
quit
",
        &[],
        &[],
    );
    assert_eq!(r.code(), 0, "{}", r.stderr());
    let a = r.log("a.json");
    assert_eq!(evs(&a, "selection_set").len(), 1, "{a:?}");
    let b = r.log("b.json");
    let sel = evs(&b, "selection");
    assert!(sel.iter().any(|e| e["text"] == "olá"), "{b:?}");
    let dump = std::fs::read_to_string(r.path("dump.json")).unwrap();
    assert!(!dump.contains("olá"), "no clipboard content in the dump");
}

#[test]
fn world_close_asks_the_client() {
    if testclient().is_none() {
        return;
    }
    let r = run(
        &format!(
            "spawn testclient --app-id acme.alpha --log {{dir}}/a.json
{}
wait 300
quit
",
            cmd("world.close")
        ),
        &[],
        &[],
    );
    assert_eq!(r.code(), 0, "{}", r.stderr());
    assert_eq!(evs(&r.log("a.json"), "close").len(), 1);
}

/// `nkw --headless` without `--demo`, with `--state-dir`.
fn run_state(script: &str, state: &Path) -> common::Run {
    let dir = tempfile::tempdir().unwrap();
    let xdg = dir.path().join("xdg");
    std::fs::create_dir(&xdg).unwrap();
    let script_path = dir.path().join("t.nks");
    std::fs::write(
        &script_path,
        script.replace("{dir}", dir.path().to_str().unwrap()),
    )
    .unwrap();
    let out = Command::new(common::nkw_bin())
        .args([
            "--headless",
            "--core",
            "none",
            "--no-sys",
            "--socket",
            "nkw-wm",
        ])
        .arg("--state-dir")
        .arg(state)
        .arg("--script")
        .arg(&script_path)
        .env("XDG_RUNTIME_DIR", &xdg)
        .env("HOME", dir.path())
        .env("XDG_CONFIG_HOME", dir.path().join("config"))
        .env_remove("WAYLAND_DISPLAY")
        .env_remove("NKW_STATS")
        .env("XKB_DEFAULT_LAYOUT", "us")
        .output()
        .unwrap();
    common::Run { dir, out }
}

#[test]
fn a_restart_restores_rooms_desks_and_rules() {
    if testclient().is_none() {
        return;
    }
    let state = tempfile::tempdir().unwrap();
    std::fs::write(
        state.path().join("world.toml"),
        "[[rules]]\napp_id = \"acme.beta\"\nroom = \"bravo\"\nwidth = \"third\"\n",
    )
    .unwrap();
    let first = run_state(
        &format!(
            "inject wm {{\"command\": \"world.room_new\", \"args\": {{\"name\": \"acme\"}}}}
expect json /rooms/0/name \"acme\"
spawn testclient --app-id acme.alpha
{}
wait json /desks/0/width \"two_thirds\" 2000
inject wm {{\"save\": true}}
quit
",
            cmd("world.wider")
        ),
        state.path(),
    );
    assert_eq!(first.code(), 0, "{}", first.stderr());
    let saved = std::fs::read_to_string(state.path().join("world.json")).unwrap();
    assert!(saved.contains("\"acme.alpha\""), "{saved}");

    let second = run_state(
        "wait 100
expect json /rooms/0/name \"acme\"
expect json /rooms/0/kind \"free\"
spawn testclient --app-id acme.alpha
wait json /screens/0/size [837.0,720.0] 3000
expect json /desks/0/width \"two_thirds\"
expect json /screens/0/placement/room 1
spawn testclient --app-id acme.beta
wait json /ext/wm/places/acme.beta/0 \"bravo\" 3000
quit
",
        state.path(),
    );
    assert_eq!(second.code(), 0, "{}", second.stderr());
    // a broken file: moved aside, the world starts fresh
    std::fs::write(state.path().join("world.json"), "{ broken").unwrap();
    let third = run_state(
        "wait 100\nexpect json /rooms/0/name \"minha sala\"\nquit\n",
        state.path(),
    );
    assert_eq!(third.code(), 0, "{}", third.stderr());
    assert!(state.path().join("world.json.bak").exists());
    let _ = read_log;
}
