//! The binary end to end. The host test needs a running compositor, so it is
//! `#[ignore]`d: `cargo test -p nkw-testclient -- --ignored` from a Wayland session.

use std::process::Command;

use serde_json::Value;

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_nkw-testclient"))
}

#[test]
fn bad_arguments_exit_2_with_usage() {
    let out = bin().arg("--bogus").output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("usage: nkw-testclient"));
}

#[test]
fn help_prints_usage() {
    let out = bin().arg("--help").output().unwrap();
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("--checker"));
}

#[test]
fn no_compositor_exits_1() {
    let dir = std::env::temp_dir().join(format!("nkw-testclient-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let out = bin()
        .env("XDG_RUNTIME_DIR", &dir)
        .env("WAYLAND_DISPLAY", "nkw-test-nobody-here")
        .args(["--exit-after-ms", "100"])
        .output()
        .unwrap();
    std::fs::remove_dir_all(&dir).ok();
    assert_eq!(out.status.code(), Some(1), "{out:?}");
}

#[test]
#[ignore = "needs a Wayland compositor (WAYLAND_DISPLAY)"]
fn host_compositor_shows_the_window_and_logs_configure_and_frames() {
    let log = std::env::temp_dir().join(format!("nkw-testclient-{}.json", std::process::id()));
    let status = bin()
        .args([
            "--checker",
            "--size",
            "200x100",
            "--app-id",
            "demo",
            "--exit-after-ms",
            "1500",
        ])
        .arg("--log")
        .arg(&log)
        .status()
        .unwrap();
    assert!(status.success());
    let text = std::fs::read_to_string(&log).unwrap();
    std::fs::remove_file(&log).ok();
    let lines: Vec<Value> = text
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let evs: Vec<&str> = lines.iter().map(|l| l["ev"].as_str().unwrap()).collect();
    assert_eq!(evs.first(), Some(&"configure"), "{evs:?}");
    assert!(evs.contains(&"frame"), "{evs:?}");
    // Keyboard focus depends on the host's focus policy; report it, don't gate on it.
    eprintln!("enter logged: {}", evs.contains(&"enter"));
    let ts: Vec<u64> = lines.iter().map(|l| l["t_us"].as_u64().unwrap()).collect();
    assert!(ts.windows(2).all(|w| w[0] <= w[1]));
}
