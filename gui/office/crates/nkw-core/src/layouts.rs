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
    Dock {
        slot: DockSlot,
        scale_pct: u16,
    },
    Desk {
        desk: usize,
        slot: usize,
        width: Width,
    },
    Cluster {
        name: String,
        air: Air,
    },
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

/// The open screen `app_id`/`title` matches best (lowest id on a tie),
/// among the ones `used` does not already claim.
fn pick(open: &[Open], used: &[bool], app_id: &str, title: Option<&str>) -> Option<usize> {
    open.iter()
        .enumerate()
        .filter(|(i, o)| !used[*i] && o.app_id == app_id)
        .filter(|(_, o)| title.is_none_or(|t| glob(t, &o.title)))
        .min_by_key(|(_, o)| o.id)
        .map(|(i, _)| i)
}

/// Entries in file order; one with `title` takes first the open screens
/// whose title matches; each open screen is used once; ties go to the
/// lowest id. Open screens no entry names are not in `moves` (they stay
/// put).
pub fn match_open(file: &LayoutFile, open: &[Open]) -> Matched {
    let mut used = vec![false; open.len()];
    let mut matched: Vec<Option<ScreenId>> = vec![None; file.entries.len()];
    // entries that name a title get first pick at the pool, regardless of
    // where they sit in the file, so a generic (title-less) entry never
    // steals the window a specific one is after.
    for (idx, e) in file.entries.iter().enumerate() {
        if e.title.is_some()
            && let Some(i) = pick(open, &used, &e.app_id, e.title.as_deref())
        {
            used[i] = true;
            matched[idx] = Some(open[i].id);
        }
    }
    for (idx, e) in file.entries.iter().enumerate() {
        if e.title.is_none()
            && let Some(i) = pick(open, &used, &e.app_id, None)
        {
            used[i] = true;
            matched[idx] = Some(open[i].id);
        }
    }
    let mut moves = Vec::new();
    let mut missing = Vec::new();
    for (idx, e) in file.entries.iter().enumerate() {
        match matched[idx] {
            Some(id) => moves.push((id, e.at.clone())),
            None => missing.push(e.app_id.clone()),
        }
    }
    Matched { moves, missing }
}

/// The classic greedy/backtracking wildcard match (no regex dependency):
/// `*` any run (even empty), `?` exactly one character.
fn glob_chars(pattern: &[char], text: &[char]) -> bool {
    let (mut pi, mut ti) = (0usize, 0usize);
    let mut star: Option<usize> = None;
    let mut resume = 0usize;
    while ti < text.len() {
        if pi < pattern.len() && (pattern[pi] == '?' || pattern[pi] == text[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < pattern.len() && pattern[pi] == '*' {
            star = Some(pi);
            resume = ti;
            pi += 1;
        } else if let Some(si) = star {
            pi = si + 1;
            resume += 1;
            ti = resume;
        } else {
            return false;
        }
    }
    while pi < pattern.len() && pattern[pi] == '*' {
        pi += 1;
    }
    pi == pattern.len()
}

/// Case-insensitive, `*` and `?`.
pub fn glob(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.chars().flat_map(char::to_lowercase).collect();
    let t: Vec<char> = text.chars().flat_map(char::to_lowercase).collect();
    glob_chars(&p, &t)
}

/// `[a-z0-9_-]{1,32}`, trimmed; Err pt-BR.
pub fn valid_name(s: &str) -> Result<String, String> {
    let name = s.trim();
    let len = name.chars().count();
    if !(1..=32).contains(&len) {
        return Err(format!(
            "nome do arranjo: precisa ter de 1 a 32 caracteres: {s:?}"
        ));
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
    {
        return Err(format!(
            "nome do arranjo: só letras minúsculas, números, \"_\" e \"-\": {s:?}"
        ));
    }
    Ok(name.to_string())
}

/// A version above LAYOUT_VERSION → Err pt-BR.
pub fn parse(text: &str) -> Result<LayoutFile, String> {
    let f: LayoutFile = serde_json::from_str(text).map_err(|e| e.to_string())?;
    if f.version > LAYOUT_VERSION {
        return Err(format!(
            "arranjo: versão {} desconhecida (até {} nesta build)",
            f.version, LAYOUT_VERSION
        ));
    }
    Ok(f)
}

/// Pretty JSON, stable order.
pub fn to_string(f: &LayoutFile) -> String {
    let mut s = serde_json::to_string_pretty(f).expect("a layout serialises");
    s.push('\n');
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glob_table() {
        let rows: [(&str, &str, bool); 14] = [
            ("firefox", "firefox", true),
            ("firefox", "Firefox", true),
            ("FIREFOX", "firefox", true),
            ("firefox", "chrome", false),
            ("*", "anything at all", true),
            ("*", "", true),
            ("fire*", "firefox", true),
            ("fire*", "fir", false),
            ("*fox", "firefox", true),
            ("fire*fox", "fire-and-fox", true),
            ("fire*fox", "firefox", true),
            ("fire?fox", "fireXfox", true),
            ("fire?fox", "firefox", false),
            ("a*b*c", "aXXbYYc", true),
        ];
        for (pattern, text, want) in rows {
            assert_eq!(glob(pattern, text), want, "{pattern:?} vs {text:?}");
        }
        // a trailing run of stars still matches, and an exact-length run
        // of '?' must cover every remaining character
        assert!(glob("abc***", "abc"));
        assert!(glob("???", "xyz"));
        assert!(!glob("???", "xy"));
    }

    #[test]
    fn valid_name_table() {
        let long_ok = "a".repeat(32);
        let ok = ["a", "arranjo-1", "dois_monitores", long_ok.as_str()];
        for n in ok {
            assert_eq!(valid_name(n).as_deref(), Ok(n), "{n:?}");
        }
        assert_eq!(valid_name("  padded  ").as_deref(), Ok("padded"));
        let too_long = "a".repeat(33);
        for bad in [
            "",
            "   ",
            too_long.as_str(),
            "Maiuscula",
            "tem espaço",
            "acentuação",
            "ponto.aqui",
        ] {
            assert!(valid_name(bad).is_err(), "{bad:?} should be rejected");
        }
    }

    fn at(x: f32) -> LayoutAt {
        LayoutAt::Air(Air::new([x, 1.5, -2.0], 0.0, false))
    }

    fn entry(app_id: &str, title: Option<&str>) -> LayoutEntry {
        LayoutEntry {
            app_id: app_id.into(),
            title: title.map(String::from),
            at: at(0.0),
        }
    }

    fn open(id: u32, app_id: &str, title: &str) -> Open {
        Open {
            id: ScreenId(id),
            app_id: app_id.into(),
            title: title.into(),
        }
    }

    #[test]
    fn match_open_pairs_by_app_id_in_file_order() {
        let file = LayoutFile {
            version: 1,
            name: "a".into(),
            room: "minha sala".into(),
            entries: vec![entry("firefox", None), entry("foot", None)],
        };
        let screens = [open(2, "foot", "term"), open(1, "firefox", "GitHub")];
        let m = match_open(&file, &screens);
        assert_eq!(
            m.moves,
            vec![(ScreenId(1), at(0.0)), (ScreenId(2), at(0.0))]
        );
        assert!(m.missing.is_empty());
    }

    #[test]
    fn match_open_reports_missing_without_launching_anything() {
        let file = LayoutFile {
            version: 1,
            name: "a".into(),
            room: "minha sala".into(),
            entries: vec![entry("firefox", None), entry("mpv", None)],
        };
        let screens = [open(1, "firefox", "GitHub")];
        let m = match_open(&file, &screens);
        assert_eq!(m.moves, vec![(ScreenId(1), at(0.0))]);
        assert_eq!(m.missing, vec!["mpv".to_string()]);
    }

    #[test]
    fn an_open_screen_no_entry_names_is_left_out_of_moves() {
        let file = LayoutFile {
            version: 1,
            name: "a".into(),
            room: "minha sala".into(),
            entries: vec![entry("firefox", None)],
        };
        let screens = [open(1, "firefox", "GitHub"), open(2, "foot", "shell")];
        let m = match_open(&file, &screens);
        assert_eq!(m.moves, vec![(ScreenId(1), at(0.0))]);
        assert!(m.missing.is_empty());
    }

    #[test]
    fn ties_go_to_the_lowest_id() {
        let file = LayoutFile {
            version: 1,
            name: "a".into(),
            room: "minha sala".into(),
            entries: vec![entry("firefox", None)],
        };
        let screens = [open(5, "firefox", "a"), open(2, "firefox", "b")];
        let m = match_open(&file, &screens);
        assert_eq!(m.moves, vec![(ScreenId(2), at(0.0))]);
    }

    #[test]
    fn a_titled_entry_claims_its_window_before_a_title_less_one_even_if_it_comes_later() {
        let file = LayoutFile {
            version: 1,
            name: "a".into(),
            room: "minha sala".into(),
            entries: vec![
                entry("firefox", None),            // comes first in the file...
                entry("firefox", Some("GitHub*")), // ...but this one picks first
            ],
        };
        let screens = [
            open(1, "firefox", "GitHub - pull request"),
            open(2, "firefox", "YouTube"),
        ];
        let m = match_open(&file, &screens);
        // entry 0 (no title) is left with whatever remains: screen 2
        // entry 1 (title) got its exact match: screen 1
        assert_eq!(
            m.moves,
            vec![(ScreenId(2), at(0.0)), (ScreenId(1), at(0.0))]
        );
    }

    #[test]
    fn title_glob_is_case_insensitive_with_wildcards() {
        let file = LayoutFile {
            version: 1,
            name: "a".into(),
            room: "minha sala".into(),
            entries: vec![entry("firefox", Some("*pull request*"))],
        };
        let screens = [open(1, "firefox", "GITHUB - Pull Request #9")];
        let m = match_open(&file, &screens);
        assert_eq!(m.moves, vec![(ScreenId(1), at(0.0))]);
    }

    #[test]
    fn parse_accepts_the_current_version_and_rejects_newer_ones() {
        let f = LayoutFile {
            version: LAYOUT_VERSION,
            name: "a".into(),
            room: "minha sala".into(),
            entries: vec![],
        };
        let text = to_string(&f);
        assert_eq!(parse(&text), Ok(f));
        let future = text.replacen(
            &format!("\"version\": {LAYOUT_VERSION}"),
            &format!("\"version\": {}", LAYOUT_VERSION + 1),
            1,
        );
        let err = parse(&future).unwrap_err();
        assert!(err.contains("versão"), "{err}");
        assert!(parse("not json").is_err());
    }
}
