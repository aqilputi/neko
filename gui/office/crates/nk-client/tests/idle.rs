//! Idle cost (plan §6.10): with a core, an events stream and an open pane
//! and nothing happening, nk-client's own threads must not wake up. Its own
//! test binary (one process, one test) so no other test's threads count.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use nk_client::*;

const GATEWAY: &str = "d3e0a001-0000-4000-8000-000000000001";

fn nk_bin() -> PathBuf {
    std::env::var_os("NK_BIN")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(std::env::var("HOME").unwrap()).join(".local/bin/nk"))
}

fn sandbox_env(dir: &std::path::Path) -> Vec<(String, String)> {
    let mut env = Vec::new();
    for (k, sub) in
        [("HOME", "home"), ("XDG_STATE_HOME", "state"), ("XDG_CONFIG_HOME", "config"), ("XDG_CACHE_HOME", "cache")]
    {
        let p = dir.join(sub);
        std::fs::create_dir_all(&p).unwrap();
        env.push((k.to_string(), p.display().to_string()));
    }
    env.push(("NEKO_CLAUDE".into(), "/bin/false".into()));
    env
}

/// Context switches (voluntary + not) per thread name, of this process's
/// threads whose name starts with `prefix`.
fn ctx_switches(prefix: &str) -> BTreeMap<String, u64> {
    let mut out = BTreeMap::new();
    for t in std::fs::read_dir("/proc/self/task").unwrap().flatten() {
        let Ok(name) = std::fs::read_to_string(t.path().join("comm")) else { continue };
        let name = name.trim().to_string();
        if !name.starts_with(prefix) {
            continue;
        }
        let Ok(status) = std::fs::read_to_string(t.path().join("status")) else { continue };
        let n: u64 = status
            .lines()
            .filter(|l| l.starts_with("voluntary_ctxt_switches") || l.starts_with("nonvoluntary_ctxt_switches"))
            .filter_map(|l| l.split_whitespace().last()?.parse::<u64>().ok())
            .sum();
        *out.entry(name).or_default() += n;
    }
    out
}

fn delta(before: &BTreeMap<String, u64>, after: &BTreeMap<String, u64>) -> BTreeMap<String, u64> {
    after.iter().map(|(k, v)| (k.clone(), v - before.get(k).copied().unwrap_or(0))).collect()
}

#[test]
fn idle_threads_do_not_wake_up() {
    let dir = tempfile::tempdir().unwrap();
    let core = Core::start(StartOpts {
        demo: true,
        nk_bin: Some(nk_bin()),
        env: sandbox_env(dir.path()),
        ..StartOpts::default()
    })
    .expect("nk serve --demo");
    let events = core.events();
    let pane = core.open_pane(GATEWAY, 80, 24).unwrap();
    // let the pane paint and the core settle
    let settle = Instant::now() + Duration::from_millis(1500);
    while Instant::now() < settle {
        while pane.rx().try_recv().is_ok() {}
        while events.try_recv().is_ok() {}
        std::thread::sleep(Duration::from_millis(100));
    }
    let names = ctx_switches("nk-");
    assert!(names.contains_key("nk-core-keeper"), "{names:?}");
    assert!(names.keys().any(|k| k.starts_with("nk-pane-")), "{names:?}");

    let window = Duration::from_secs(2);
    let before = ctx_switches("nk-");
    std::thread::sleep(window);
    let after = ctx_switches("nk-");
    let d = delta(&before, &after);

    // what the core legitimately sent meanwhile (each may cost a wakeup)
    let mut pane_msgs = 0;
    while pane.rx().try_recv().is_ok() {
        pane_msgs += 1;
    }
    let mut ev_msgs = 0;
    while events.try_recv().is_ok() {
        ev_msgs += 1;
    }
    eprintln!("idle {window:?}: {d:?} (pane msgs {pane_msgs}, events {ev_msgs})");
    let keeper = d.get("nk-core-keeper").copied().unwrap_or(0);
    let panes: u64 = d.iter().filter(|(k, _)| k.starts_with("nk-pane-")).map(|(_, v)| v).sum();
    let all: u64 = d.values().sum();
    assert!(keeper <= 1, "keeper woke {keeper} times in {window:?}");
    assert!(panes <= 1 + 2 * pane_msgs, "pane thread woke {panes} times in {window:?} ({pane_msgs} msgs)");
    // < 2 wakeups/s in total, beyond what the core actually sent
    let budget = 2 * window.as_secs() + 2 * (pane_msgs + ev_msgs);
    assert!(all < budget.max(4), "nk-* threads woke {all} times in {window:?}: {d:?}");
}
