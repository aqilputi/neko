//! The Smithay dependency (wave 3, S14.6): the nkw manifest names the
//! upstream rev, the workspace `[patch]` swaps in the relative-pointer fork,
//! and the resolved package is that fork at the upstream rev's version.

use std::path::Path;
use std::process::Command;

use serde_json::Value;

/// The upstream rev nkw is pinned to (re-pin by its own PR only).
const UPSTREAM_REV: &str = "24647fc8d00131eb7f388efed26e35ea316b0018";
/// `smithay`'s version at that rev.
const UPSTREAM_VERSION: &str = "0.7.0";
/// The fork: the upstream rev + `patches/smithay-winit-relative-pointer.patch`.
const FORK: &str = "git+https://github.com/aqilputi/smithay.git";

fn office() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .unwrap()
}

/// `rev = "…"` of the workspace `[patch]` entry for smithay.
fn patch_rev(workspace_toml: &str) -> Option<String> {
    let section = workspace_toml
        .split("[patch.\"https://github.com/Smithay/smithay.git\"]")
        .nth(1)?;
    let line = section
        .lines()
        .find(|l| l.trim_start().starts_with("smithay"))?;
    if !line.contains("https://github.com/aqilputi/smithay.git") {
        return None;
    }
    let rest = line.split("rev = \"").nth(1)?;
    Some(rest.split('"').next()?.to_string())
}

#[test]
fn patch_rev_parser() {
    let t = "[x]\n[patch.\"https://github.com/Smithay/smithay.git\"]\nsmithay = { git = \"https://github.com/aqilputi/smithay.git\", rev = \"abc\" }\n";
    assert_eq!(patch_rev(t).as_deref(), Some("abc"));
    assert_eq!(patch_rev("[x]\n"), None);
    let other = t.replace("aqilputi", "someone");
    assert_eq!(patch_rev(&other), None, "only the fork");
}

#[test]
fn the_manifest_keeps_the_upstream_rev() {
    let m =
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml")).unwrap();
    assert!(
        m.contains("git = \"https://github.com/Smithay/smithay.git\""),
        "{m}"
    );
    assert!(m.contains(&format!("rev = \"{UPSTREAM_REV}\"")), "{m}");
}

#[test]
fn the_patch_file_is_in_the_repo_and_clean() {
    let p = office().join("patches/smithay-winit-relative-pointer.patch");
    let t = std::fs::read_to_string(&p).expect("patches/smithay-winit-relative-pointer.patch");
    assert!(t.contains("DeviceEvent::PointerMotion"));
    assert!(t.contains("type PointerMotionEvent = WinitMouseRelativeEvent;"));
    for bad in ["/home/", "/tmp/", "/Users/"] {
        assert!(!t.contains(bad), "no absolute paths in the patch ({bad})");
    }
}

#[test]
fn smithay_resolves_to_the_fork() {
    let ws = std::fs::read_to_string(office().join("Cargo.toml")).unwrap();
    let rev = patch_rev(&ws).expect("workspace [patch] for smithay → the fork");
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
    let smithay: Vec<&Value> = meta["packages"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|p| p["name"] == "smithay")
        .collect();
    assert_eq!(smithay.len(), 1, "one smithay in the graph");
    let p = smithay[0];
    let source = p["source"].as_str().unwrap();
    assert!(source.starts_with(FORK), "{source}");
    assert!(source.ends_with(&format!("#{rev}")), "{source} vs {rev}");
    assert_eq!(p["version"], UPSTREAM_VERSION);
}
