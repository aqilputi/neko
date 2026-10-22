//! Harness for the headless integration tests: runs the `nkw` binary with
//! a private `XDG_RUNTIME_DIR`, a script, and the sibling `nkw-testclient`.

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;

pub fn nkw_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_nkw"))
}

/// The testclient built next to `nkw` (`cargo build -p nkw-testclient`);
/// `None` (the test is skipped with a message) when it is not there.
pub fn testclient() -> Option<PathBuf> {
    let p = nkw_bin().parent()?.join("nkw-testclient");
    if p.exists() {
        Some(p)
    } else {
        eprintln!(
            "SKIP: {} missing; run `cargo build -p nkw-testclient` first",
            p.display()
        );
        None
    }
}

pub struct Run {
    pub dir: tempfile::TempDir,
    pub out: Output,
}

impl Run {
    pub fn path(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }

    pub fn stderr(&self) -> String {
        String::from_utf8_lossy(&self.out.stderr).into_owned()
    }

    pub fn code(&self) -> i32 {
        self.out.status.code().unwrap_or(-1)
    }

    /// The `nkw-stats` summary line.
    pub fn summary(&self) -> Value {
        self.stderr()
            .lines()
            .filter_map(|l| l.strip_prefix("nkw-stats "))
            .filter_map(|l| serde_json::from_str::<Value>(l).ok())
            .find(|v| v["summary"] == true)
            .unwrap_or_else(|| panic!("no summary in:\n{}", self.stderr()))
    }

    pub fn log(&self, name: &str) -> Vec<Value> {
        read_log(&self.path(name))
    }

    pub fn json(&self, name: &str) -> Value {
        serde_json::from_slice(&std::fs::read(self.path(name)).unwrap()).unwrap()
    }
}

pub fn read_log(p: &Path) -> Vec<Value> {
    std::fs::read_to_string(p)
        .unwrap_or_default()
        .lines()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect()
}

/// Run `nkw --demo --headless` with `script` (`{dir}` is replaced by the
/// run's temp dir) and extra args/env.
pub fn run(script: &str, args: &[&str], env: &[(&str, &str)]) -> Run {
    let dir = tempfile::tempdir().unwrap();
    let xdg = dir.path().join("xdg");
    std::fs::create_dir(&xdg).unwrap();
    let d = dir.path().to_str().unwrap();
    let script_path = dir.path().join("t.nks");
    std::fs::write(&script_path, script.replace("{dir}", d)).unwrap();
    let mut c = Command::new(nkw_bin());
    c.args(["--demo", "--headless", "--socket", "nkw-test", "--script"])
        .arg(&script_path)
        .args(args)
        .env("XDG_RUNTIME_DIR", &xdg)
        .env_remove("WAYLAND_DISPLAY")
        .env_remove("NKW_STATS")
        .env("XKB_DEFAULT_LAYOUT", "us")
        .env_remove("XKB_DEFAULT_VARIANT")
        .env_remove("XKB_DEFAULT_OPTIONS");
    for (k, v) in env {
        c.env(k, v);
    }
    let out = run_with_timeout(c, 40);
    Run { dir, out }
}

fn run_with_timeout(mut c: Command, secs: u64) -> Output {
    use std::time::{Duration, Instant};
    let mut child = c
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let t0 = Instant::now();
    loop {
        if child.try_wait().unwrap().is_some() {
            return child.wait_with_output().unwrap();
        }
        if t0.elapsed() > Duration::from_secs(secs) {
            let _ = child.kill();
            let out = child.wait_with_output().unwrap();
            panic!(
                "nkw timed out; stderr:\n{}",
                String::from_utf8_lossy(&out.stderr)
            );
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Events of one kind.
pub fn evs<'a>(log: &'a [Value], ev: &str) -> Vec<&'a Value> {
    log.iter().filter(|e| e["ev"] == ev).collect()
}

/// Key events as (sym, state).
pub fn keys(log: &[Value]) -> Vec<(String, String)> {
    evs(log, "key")
        .iter()
        .map(|e| {
            (
                e["sym"].as_str().unwrap_or("").to_string(),
                e["state"].as_str().unwrap_or("").to_string(),
            )
        })
        .collect()
}

pub fn t_us(e: &Value) -> u64 {
    e["t_us"].as_u64().unwrap_or(0)
}
