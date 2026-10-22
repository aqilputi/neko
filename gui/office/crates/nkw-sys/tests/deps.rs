//! The dependency rule of docs/plano-mundo.md §2.11 for nkw-sys: only zbus, async-io,
//! crossbeam-channel, libc (and pipewire) — never the compositor, the event loop, Tokio, libdbus or
//! libpulse. Same checker as `nkw-core/tests/deps.rs`.

use std::collections::{BTreeSet, HashMap};
use std::process::Command;

use serde_json::Value;

/// Crate names (or name prefixes ending in `-`) nkw-sys must never pull in.
const FORBIDDEN: &[&str] = &[
    "smithay",
    "glow",
    "wayland-",
    "calloop",
    "nk-client",
    "bevy",
    "tokio",
    "dbus",
    "libdbus-sys",
    "libpulse",
];

fn forbidden<'a>(names: impl IntoIterator<Item = &'a str>) -> Vec<&'a str> {
    names
        .into_iter()
        .filter(|n| {
            FORBIDDEN.iter().any(|f| {
                if f.ends_with('-') {
                    n.starts_with(f)
                } else {
                    *n == *f || n.starts_with(&format!("{f}_")) || n.starts_with(&format!("{f}-"))
                }
            })
        })
        .collect()
}

fn normal_closure(meta: &Value, root: &str) -> BTreeSet<String> {
    let names: HashMap<&str, &str> = meta["packages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| (p["id"].as_str().unwrap(), p["name"].as_str().unwrap()))
        .collect();
    let nodes: HashMap<&str, &Value> = meta["resolve"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| (n["id"].as_str().unwrap(), n))
        .collect();
    let root_id = names
        .iter()
        .find(|(_, n)| **n == root)
        .map(|(id, _)| *id)
        .expect("root package in metadata");
    let mut seen = BTreeSet::new();
    let mut todo = vec![root_id];
    while let Some(id) = todo.pop() {
        if !seen.insert(id) {
            continue;
        }
        for dep in nodes[id]["deps"].as_array().unwrap() {
            let normal = dep["dep_kinds"]
                .as_array()
                .unwrap()
                .iter()
                .any(|k| k["kind"].is_null());
            if normal {
                todo.push(dep["pkg"].as_str().unwrap());
            }
        }
    }
    seen.iter()
        .filter(|id| **id != root_id)
        .map(|id| names[id].to_string())
        .collect()
}

#[test]
fn checker_catches_forbidden_names() {
    let found = forbidden([
        "zbus",
        "dbus",
        "libdbus-sys",
        "tokio",
        "tokio-util",
        "libpulse-binding",
        "calloop",
        "pipewire",
    ]);
    assert_eq!(
        found,
        [
            "dbus",
            "libdbus-sys",
            "tokio",
            "tokio-util",
            "libpulse-binding",
            "calloop"
        ]
    );
    assert!(
        forbidden([
            "zbus",
            "zbus_names",
            "async-io",
            "crossbeam-channel",
            "libc",
            "pipewire",
            "libspa"
        ])
        .is_empty()
    );
}

#[test]
fn normal_dependency_closure_is_clean() {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let out = Command::new(cargo)
        .args(["metadata", "--format-version", "1"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("run cargo metadata");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let meta: Value = serde_json::from_slice(&out.stdout).unwrap();
    let closure = normal_closure(&meta, "nkw-sys");
    assert!(closure.contains("zbus"), "closure: {closure:?}");
    let bad = forbidden(closure.iter().map(String::as_str));
    assert!(bad.is_empty(), "nkw-sys pulls in forbidden crates: {bad:?}");
}
