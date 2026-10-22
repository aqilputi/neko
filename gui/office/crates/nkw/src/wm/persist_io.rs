//! `world.json` and `world.toml` on disk: read at start, written atomically
//! (a temp file in the same dir, then `rename`) 2 s after the last change
//! and at exit. A corrupted `world.json` is moved to `world.json.bak` and
//! the world starts fresh. Never used under `--demo`.

use std::io::Write;
use std::path::{Path, PathBuf};

use nkw_core::persist::{self, PersistError, WorldFile};
use nkw_core::rules::{self, Config};

/// Save this long after the last change.
pub const SAVE_DELAY_S: f64 = 2.0;
pub const WORLD_JSON: &str = "world.json";
pub const WORLD_TOML: &str = "world.toml";

/// What reading `world.json` gave.
#[derive(Debug, PartialEq)]
pub enum Load {
    Missing,
    Ok(WorldFile),
    /// unreadable: moved aside to `.bak` (the message for the toast)
    Corrupt(String),
}

pub struct Store {
    dir: PathBuf,
}

impl Store {
    pub fn new(dir: PathBuf) -> Store {
        Store { dir }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn path(&self) -> PathBuf {
        self.dir.join(WORLD_JSON)
    }

    pub fn load(&self) -> Load {
        let p = self.path();
        let text = match std::fs::read_to_string(&p) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Load::Missing,
            Err(e) => return Load::Corrupt(format!("world.json: {e}")),
        };
        match persist::parse(&text) {
            Ok(f) => Load::Ok(f),
            Err(e) => {
                let bak = self.dir.join(format!("{WORLD_JSON}.bak"));
                let moved = std::fs::rename(&p, &bak).is_ok();
                let tail = if moved {
                    " (guardado em world.json.bak)"
                } else {
                    ""
                };
                Load::Corrupt(format!("{e}{tail}"))
            }
        }
    }

    /// Write `file` atomically.
    pub fn save(&self, file: &WorldFile) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.dir)?;
        let tmp = self.dir.join(format!(".{WORLD_JSON}.tmp"));
        {
            let mut f = std::fs::File::create(&tmp)?;
            f.write_all(persist::to_string(file).as_bytes())?;
            f.sync_all()?;
        }
        std::fs::rename(&tmp, self.path())
    }
}

/// Read `world.toml` from `path`: missing = the defaults; broken = the
/// defaults and the message.
pub fn read_config(path: &Path) -> (Config, Option<String>) {
    match std::fs::read_to_string(path) {
        Ok(t) => match rules::parse(&t) {
            Ok(c) => (c, None),
            Err(e) => (Config::default(), Some(e)),
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (Config::default(), None),
        Err(e) => (Config::default(), Some(format!("world.toml: {e}"))),
    }
}

/// Where `world.toml` is: next to `world.json` with `--state-dir`, else
/// `$XDG_CONFIG_HOME/nekuroddo/` (`~/.config/nekuroddo/`).
pub fn config_path(
    state_dir: Option<&Path>,
    config_home: Option<&Path>,
    home: Option<&Path>,
) -> Option<PathBuf> {
    if let Some(d) = state_dir {
        return Some(d.join(WORLD_TOML));
    }
    let base = match (config_home, home) {
        (Some(c), _) => c.to_path_buf(),
        (None, Some(h)) => h.join(".config"),
        _ => return None,
    };
    Some(base.join("nekuroddo").join(WORLD_TOML))
}

/// `PersistError` for messages.
pub fn describe(e: &PersistError) -> String {
    e.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use nkw_core::persist::{RoomKind, RoomRec};

    fn file() -> WorldFile {
        WorldFile {
            rooms: vec![RoomRec {
                id: 1,
                name: "acme".into(),
                kind: RoomKind::Free,
            }],
            ..WorldFile::default()
        }
    }

    #[test]
    fn save_then_load() {
        let t = tempfile::tempdir().unwrap();
        let s = Store::new(t.path().join("state"));
        assert_eq!(s.load(), Load::Missing);
        s.save(&file()).unwrap();
        assert_eq!(s.load(), Load::Ok(file()));
        // no temp file left behind
        let names: Vec<_> = std::fs::read_dir(s.dir())
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        assert_eq!(names, [WORLD_JSON]);
    }

    #[test]
    fn corrupted_goes_to_bak() {
        let t = tempfile::tempdir().unwrap();
        let s = Store::new(t.path().to_path_buf());
        std::fs::write(s.path(), "{ not json").unwrap();
        let Load::Corrupt(msg) = s.load() else {
            panic!("not corrupt")
        };
        assert!(msg.contains("world.json.bak"), "{msg}");
        assert!(!s.path().exists());
        assert!(t.path().join("world.json.bak").exists());
        assert_eq!(s.load(), Load::Missing, "then a fresh start");
    }

    #[test]
    fn config_files() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join(WORLD_TOML);
        assert_eq!(read_config(&p), (Config::default(), None));
        std::fs::write(&p, "[launch]\nterminal = \"foot\"\n").unwrap();
        assert_eq!(read_config(&p).0.terminal(), ["foot"]);
        std::fs::write(&p, "[[rules]]\n").unwrap();
        assert!(read_config(&p).1.unwrap().starts_with("world.toml"));
        assert_eq!(
            config_path(Some(Path::new("/s")), None, None),
            Some(PathBuf::from("/s/world.toml"))
        );
        assert_eq!(
            config_path(None, None, Some(Path::new("/h"))),
            Some(PathBuf::from("/h/.config/nekuroddo/world.toml"))
        );
        assert_eq!(
            config_path(None, Some(Path::new("/c")), Some(Path::new("/h"))),
            Some(PathBuf::from("/c/nekuroddo/world.toml"))
        );
        assert_eq!(config_path(None, None, None), None);
    }
}
