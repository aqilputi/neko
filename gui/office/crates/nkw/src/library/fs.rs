//! The library's `std::fs` port: `readdir` + `lstat` only (and the
//! lectern's bounded read). It never follows a symlink out of its roots,
//! reports mount boundaries by `st_dev`, and never puts a path in an error
//! (errors reach toasts and logs).

use std::io::Read;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use super::model::{Entry, FsError, FsPort, norm};

/// The roots of `[library] roots`: `~` is the home, `~/x` and relative
/// entries are under it, absolute ones stay. No home → only absolute roots.
pub fn resolve_roots(roots: &[String], home: Option<&Path>) -> Vec<PathBuf> {
    roots
        .iter()
        .filter_map(|r| {
            let r = r.trim();
            if r == "~" {
                home.map(Path::to_path_buf)
            } else if let Some(rest) = r.strip_prefix("~/") {
                home.map(|h| h.join(rest))
            } else if r.starts_with('/') {
                Some(PathBuf::from(r))
            } else if r.is_empty() {
                None
            } else {
                home.map(|h| h.join(r))
            }
        })
        .collect()
}

pub struct StdFs {
    roots: Vec<PathBuf>,
    /// the roots canonicalised once (the outside-root check)
    canon: Vec<Option<PathBuf>>,
}

fn err(e: std::io::Error) -> FsError {
    match e.kind() {
        std::io::ErrorKind::NotFound => FsError::NotFound,
        std::io::ErrorKind::PermissionDenied => FsError::Denied,
        // the kind only: never the path
        k => FsError::Io(format!("{k:?}")),
    }
}

impl StdFs {
    pub fn new(roots: Vec<PathBuf>) -> StdFs {
        let canon = roots.iter().map(|r| r.canonicalize().ok()).collect();
        StdFs { roots, canon }
    }

    pub fn roots(&self) -> &[PathBuf] {
        &self.roots
    }

    /// The absolute path of `rel` (normalised: never above the root).
    pub fn path(&self, root: usize, rel: &str) -> Option<PathBuf> {
        let base = self.roots.get(root)?;
        let rel = norm(rel);
        Some(if rel.is_empty() {
            base.clone()
        } else {
            base.join(rel)
        })
    }
}

impl FsPort for StdFs {
    fn read_dir(&self, root: usize, rel: &str) -> Result<Vec<Entry>, FsError> {
        let p = self.path(root, rel).ok_or(FsError::NotFound)?;
        let base = norm(rel);
        let mut out = Vec::new();
        for d in std::fs::read_dir(&p).map_err(err)? {
            let Ok(d) = d else { continue };
            let name = d.file_name().to_string_lossy().into_owned();
            let Ok(lm) = d.path().symlink_metadata() else {
                continue;
            };
            // a symlink inside the roots counts as its target; a broken one
            // is skipped
            let m = if lm.file_type().is_symlink() {
                match d.path().metadata() {
                    Ok(m) => m,
                    Err(_) => continue,
                }
            } else {
                lm
            };
            out.push(Entry {
                rel: if base.is_empty() {
                    name.clone()
                } else {
                    format!("{base}/{name}")
                },
                is_dir: m.is_dir(),
                size: m.len(),
                mtime: m.mtime(),
                hidden: name.starts_with('.'),
            });
        }
        Ok(out)
    }

    fn dev(&self, root: usize, rel: &str) -> Option<u64> {
        self.path(root, rel)?.metadata().ok().map(|m| m.dev())
    }

    fn resolves_outside(&self, root: usize, rel: &str) -> bool {
        let Some(p) = self.path(root, rel) else {
            return true;
        };
        // only a symlink can lead out: `rel` itself or a directory above it
        // (one lstat per segment; canonicalised only when one is a link)
        let segs = super::model::normalize(rel);
        let base = &self.roots[root];
        let linked = (1..=segs.len()).any(|k| {
            base.join(segs[..k].join("/"))
                .symlink_metadata()
                .is_ok_and(|m| m.file_type().is_symlink())
        });
        if !linked {
            return false;
        }
        match (p.canonicalize(), self.canon.get(root).cloned().flatten()) {
            (Ok(t), Some(c)) => !t.starts_with(c),
            _ => true,
        }
    }

    fn read_text(&self, root: usize, rel: &str, max: usize) -> Option<String> {
        if self.resolves_outside(root, rel) {
            return None;
        }
        let p = self.path(root, rel)?;
        if !p.symlink_metadata().ok()?.is_file() && !p.metadata().ok()?.is_file() {
            return None;
        }
        let mut buf = Vec::with_capacity(max.min(64 * 1024));
        std::fs::File::open(&p)
            .ok()?
            .take(max as u64)
            .read_to_end(&mut buf)
            .ok()?;
        text_of(buf)
    }
}

/// Bytes → text for the lectern: valid UTF-8 (a char cut at the end is
/// dropped), no NUL (binary).
pub fn text_of(mut buf: Vec<u8>) -> Option<String> {
    if buf.contains(&0) {
        return None;
    }
    match std::str::from_utf8(&buf) {
        Ok(_) => {}
        Err(e) if e.error_len().is_none() => buf.truncate(e.valid_up_to()),
        Err(_) => return None,
    }
    String::from_utf8(buf).ok()
}

#[cfg(test)]
mod tests {
    use super::super::model::{Config, FsError, read_shelf};
    use super::*;

    /// A temp tree (never the user's home).
    fn tree() -> tempfile::TempDir {
        let t = tempfile::tempdir().unwrap();
        let p = t.path();
        std::fs::create_dir_all(p.join("projeto/src")).unwrap();
        std::fs::write(p.join("projeto/src/lib.rs"), "pub fn a() {}\n").unwrap();
        std::fs::write(p.join("notas.md"), "# notas\nolá\n").unwrap();
        std::fs::write(p.join("bin.dat"), [0u8, 1, 2, 3]).unwrap();
        std::fs::create_dir_all(p.join(".ssh")).unwrap();
        std::fs::write(p.join(".ssh/id_demo"), "x").unwrap();
        t
    }

    #[test]
    fn roots_resolve_against_the_given_home() {
        let h = Path::new("/casa/demo");
        let r = resolve_roots(
            &[
                "~".into(),
                "~/projeto".into(),
                "docs".into(),
                "/srv/x".into(),
                "".into(),
            ],
            Some(h),
        );
        assert_eq!(
            r,
            [
                PathBuf::from("/casa/demo"),
                PathBuf::from("/casa/demo/projeto"),
                PathBuf::from("/casa/demo/docs"),
                PathBuf::from("/srv/x"),
            ]
        );
        assert_eq!(
            resolve_roots(&["~".into(), "/srv".into()], None),
            [PathBuf::from("/srv")]
        );
    }

    #[test]
    fn reads_names_and_metadata_of_a_temp_tree() {
        let t = tree();
        let fs = StdFs::new(vec![t.path().to_path_buf()]);
        let mut e = fs.read_dir(0, "").unwrap();
        e.sort_by(|a, b| a.rel.cmp(&b.rel));
        let names: Vec<&str> = e.iter().map(|e| e.rel.as_str()).collect();
        assert_eq!(names, [".ssh", "bin.dat", "notas.md", "projeto"]);
        assert!(e[0].hidden && e[0].is_dir);
        assert_eq!(e[1].size, 4);
        let src = fs.read_dir(0, "projeto/src").unwrap();
        assert_eq!(src[0].rel, "projeto/src/lib.rs");
        assert_eq!(fs.read_dir(0, "nada"), Err(FsError::NotFound));
        assert_eq!(
            fs.read_dir(0, "../.."),
            fs.read_dir(0, ""),
            "never above the root"
        );
        let s = read_shelf(&Config::default(), &fs, 0, "").unwrap();
        assert_eq!(s.vault_count, 1);
        assert_eq!(s.entries.len(), 3);
    }

    #[test]
    fn symlinks_out_of_the_root_are_detected_inside_ones_are_not() {
        let t = tree();
        let out = tempfile::tempdir().unwrap();
        std::fs::write(out.path().join("fora.txt"), "x").unwrap();
        let p = t.path();
        std::os::unix::fs::symlink(out.path(), p.join("atalho")).unwrap();
        std::os::unix::fs::symlink(p.join("projeto"), p.join("perto")).unwrap();
        let fs = StdFs::new(vec![p.to_path_buf()]);
        assert!(fs.resolves_outside(0, "atalho"));
        assert!(fs.resolves_outside(0, "atalho/fora.txt"));
        assert!(!fs.resolves_outside(0, "perto"));
        assert!(!fs.resolves_outside(0, "notas.md"));
        let s = read_shelf(&Config::default(), &fs, 0, "").unwrap();
        let names: Vec<&str> = s.entries.iter().map(|e| e.name()).collect();
        assert!(!names.contains(&"atalho"));
        assert!(names.contains(&"perto"));
        assert_eq!(
            read_shelf(&Config::default(), &fs, 0, "atalho"),
            Err(FsError::Boundary)
        );
        assert!(fs.read_text(0, "atalho/fora.txt", 100).is_none());
    }

    #[test]
    fn the_preview_is_bounded_utf8_and_refuses_binary() {
        let t = tree();
        let fs = StdFs::new(vec![t.path().to_path_buf()]);
        assert_eq!(fs.read_text(0, "notas.md", 1024).unwrap(), "# notas\nolá\n");
        // "olá": the á is two bytes; a cut inside it drops it
        assert_eq!(fs.read_text(0, "notas.md", 11).unwrap(), "# notas\nol");
        assert!(fs.read_text(0, "bin.dat", 1024).is_none());
        assert!(fs.read_text(0, "projeto", 1024).is_none());
        assert_eq!(text_of(vec![0xff, 0x41]), None);
    }

    #[test]
    fn dev_is_the_same_inside_one_file_system() {
        let t = tree();
        let fs = StdFs::new(vec![t.path().to_path_buf()]);
        assert!(fs.dev(0, "").is_some());
        assert_eq!(fs.dev(0, ""), fs.dev(0, "projeto/src"));
        assert_eq!(fs.dev(0, "nada"), None);
    }
}
