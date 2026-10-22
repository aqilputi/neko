//! The dependency rule of docs/plano-mundo.md §2.11: the normal-dependency
//! closure of nkw-objects must stay free of the compositor, GL, Wayland, the
//! event loop, the core client, Bevy, D-Bus and PipeWire; and its direct
//! dependencies are exactly nkw-core, glam and serde (so no libc either).
//!
//! libc is only checked as a direct dependency: `cargo metadata` shows it
//! reachable through glam → rand → getrandom because Bevy (in `nkg`) turns
//! on glam's `rand` feature and metadata unifies features across the
//! workspace; `cargo build -p nkw-objects` alone never builds it.

use std::collections::{BTreeSet, HashMap};
use std::process::Command;

use serde_json::Value;

/// Crate names (or name prefixes ending in `-`) nkw-objects must never pull in.
const FORBIDDEN: &[&str] = &[
    "smithay",
    "glow",
    "wayland-",
    "calloop",
    "nk-client",
    "bevy",
    "zbus",
    "pipewire",
];

/// The direct normal dependencies, exactly (§2.11).
const DIRECT: &[&str] = &["glam", "nkw-core", "serde"];

/// The forbidden crates found in `names`.
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

/// Names of every package reachable from `root` through normal (non-dev,
/// non-build) dependency edges, from `cargo metadata` JSON.
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
        "glam",
        "smithay",
        "wayland-client",
        "bevy_ecs",
        "nk-client",
        "serde",
        "glow",
        "calloop",
        "zbus",
        "pipewire-sys",
    ]);
    assert_eq!(
        found,
        [
            "smithay",
            "wayland-client",
            "bevy_ecs",
            "nk-client",
            "glow",
            "calloop",
            "zbus",
            "pipewire-sys"
        ]
    );
    assert!(forbidden(["glam", "serde", "nk-cats", "toml", "glowworm"]).is_empty());
}

fn metadata() -> Value {
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
    serde_json::from_slice(&out.stdout).unwrap()
}

#[test]
fn direct_dependencies_are_exactly_the_rule() {
    let meta = metadata();
    let pkg = meta["packages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == "nkw-objects")
        .unwrap();
    let mut names: Vec<&str> = pkg["dependencies"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|d| d["kind"].is_null())
        .map(|d| d["name"].as_str().unwrap())
        .collect();
    names.sort();
    assert_eq!(names, DIRECT, "no libc, no adapters: §2.11");
}

#[test]
fn normal_dependency_closure_is_clean() {
    let meta = metadata();
    let closure = normal_closure(&meta, "nkw-objects");
    assert!(closure.contains("glam"), "closure: {closure:?}");
    assert!(closure.contains("nkw-core"), "closure: {closure:?}");
    let bad = forbidden(closure.iter().map(String::as_str));
    assert!(
        bad.is_empty(),
        "nkw-objects pulls in forbidden crates: {bad:?}"
    );
}
