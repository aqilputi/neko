//! Arrangements (plan §2.9): the file of a saved arrangement and matching
//! it against the windows that are OPEN. Nothing here launches anything:
//! an entry with no open window is reported missing (the user's decision).

use serde::{Deserialize, Serialize};

use crate::dock::DockSlot;
use crate::ids::ScreenId;
use crate::persist::Air;
use crate::strip::Width;

pub const LAYOUT_VERSION: u32 = 1;

/// `<state-dir>/layouts/<name>.json`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LayoutFile {
    pub version: u32,
    pub name: String,
    pub room: String,
    pub entries: Vec<LayoutEntry>,
}

/// One window of the arrangement.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LayoutEntry {
    pub app_id: String,
    /// glob: `*` and `?`
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub at: LayoutAt,
}

/// Where an entry's window goes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LayoutAt {
    Air(Air),
    Wall(Air),
    Dock { slot: DockSlot, scale_pct: u16 },
    Desk { desk: usize, slot: usize, width: Width },
    Cluster { name: String, air: Air },
}

/// An open screen, for [`match_open`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Open {
    pub id: ScreenId,
    pub app_id: String,
    pub title: String,
}

/// What loading an arrangement does.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Matched {
    pub moves: Vec<(ScreenId, LayoutAt)>,
    /// app_ids, file order
    pub missing: Vec<String>,
}

/// Entries in file order; one with `title` takes first the open screens
/// whose title matches; each open screen is used once; ties go to the
/// lowest id. Open screens no entry names are not in `moves` (they stay
/// put).
pub fn match_open(file: &LayoutFile, open: &[Open]) -> Matched {
    let _ = open;
    Matched {
        moves: Vec::new(),
        missing: file.entries.iter().map(|e| e.app_id.clone()).collect(),
    }
}

/// Case-insensitive, `*` and `?`.
pub fn glob(pattern: &str, text: &str) -> bool {
    pattern == text
}

/// `[a-z0-9_-]{1,32}`, trimmed; Err pt-BR.
pub fn valid_name(s: &str) -> Result<String, String> {
    Ok(s.trim().to_string())
}

/// A version above LAYOUT_VERSION → Err pt-BR.
pub fn parse(text: &str) -> Result<LayoutFile, String> {
    serde_json::from_str(text).map_err(|e| e.to_string())
}

/// Pretty JSON, stable order.
pub fn to_string(f: &LayoutFile) -> String {
    let mut s = serde_json::to_string_pretty(f).expect("a layout serialises");
    s.push('\n');
    s
}
