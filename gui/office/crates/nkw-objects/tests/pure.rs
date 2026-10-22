//! nkw-objects is pure: no file system, network, processes, threads or
//! clocks anywhere in `src/` (time always comes in as `now`).

use std::path::Path;

const FORBIDDEN: &[&str] = &[
    "std::fs",
    "std::net",
    "std::process",
    "std::thread",
    "std::time::Instant",
    "SystemTime",
];

fn sources(dir: &Path, out: &mut Vec<(String, String)>) {
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            sources(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push((
                p.display().to_string(),
                std::fs::read_to_string(&p).unwrap(),
            ));
        }
    }
}

#[test]
fn src_has_no_io_threads_or_clocks() {
    let mut files = Vec::new();
    sources(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut files,
    );
    assert!(files.len() >= 8, "found {} files", files.len());
    let mut bad = Vec::new();
    for (path, text) in &files {
        for f in FORBIDDEN {
            if text.contains(f) {
                bad.push(format!("{path}: {f}"));
            }
        }
    }
    assert!(bad.is_empty(), "impure code in src/: {bad:?}");
}
