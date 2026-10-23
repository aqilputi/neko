//! The library model (docs/plano-mundo.md §7, contract S18): roots are walls,
//! directories are shelves, files are books — names and metadata only.
//!
//! Pure like the rest of the crate: the file system comes in through
//! [`FsPort`] (the `nkw` side's `std::fs` port, or [`MemFs`] for the demo
//! and the tests), time through a clock closure. Nothing here keeps an index
//! beyond the shelves in memory, and vault paths ([`Policy`]) never enter a
//! shelf, a search or a breadcrumb.
//!
//! - [`read_shelf`]: one directory → a [`Shelf`] (folders first, by name,
//!   ≤ `per_shelf`, the rest counted in `more`), vault/hidden/excluded out;
//! - [`read_vault`]: the vault's cards (only after an unlock, `nkw`'s call);
//! - [`search_tree`]: fuzzy search over a subtree within the budget;
//! - [`Library`]: the cursor, the LRU of shelves, the breadcrumb.
//!
//! This file is written so it also compiles when included by `#[path]`
//! from another crate: its only crate-local import is `super::Dir`.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::atomic::{AtomicUsize, Ordering};

use nucleo_matcher::pattern::{AtomKind, CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Matcher, Utf32Str};
use serde::{Deserialize, Serialize};

use super::Dir;

/// What the file system looks like to the library: names and metadata
/// only; no method returns content except the lectern's bounded preview.
pub trait FsPort {
    /// `readdir` + `stat` of `rel` under root `root`, unsorted.
    fn read_dir(&self, root: usize, rel: &str) -> Result<Vec<Entry>, FsError>;
    /// `st_dev` of `rel` (a change is a mount boundary).
    fn dev(&self, root: usize, rel: &str) -> Option<u64>;
    /// A symlink whose target leaves the root.
    fn resolves_outside(&self, root: usize, rel: &str) -> bool;
    /// The preview only: at most `max` bytes, valid UTF-8, else `None`.
    fn read_text(&self, root: usize, rel: &str, max: usize) -> Option<String>;
}

/// A directory entry. There is, by construction, no field for content.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    /// relative to the root, `/`-separated, normalised
    pub rel: String,
    pub is_dir: bool,
    pub size: u64,
    pub mtime: i64,
    /// the name starts with a dot
    pub hidden: bool,
}

impl Entry {
    /// The last segment of `rel`.
    pub fn name(&self) -> &str {
        self.rel.rsplit('/').next().unwrap_or("")
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FsError {
    NotFound,
    Denied,
    /// a mount boundary or a symlink out of the roots: shown, never entered
    Boundary,
    Io(String),
}

/// The spine colour family of a book.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BookKind {
    Code,
    Text,
    Image,
    Media,
    Archive,
    Other,
    Folder,
}

const EXT: &[(&str, BookKind)] = {
    use BookKind::*;
    &[
        ("rs", Code),
        ("go", Code),
        ("c", Code),
        ("h", Code),
        ("cpp", Code),
        ("hpp", Code),
        ("cc", Code),
        ("py", Code),
        ("js", Code),
        ("ts", Code),
        ("tsx", Code),
        ("jsx", Code),
        ("java", Code),
        ("kt", Code),
        ("rb", Code),
        ("lua", Code),
        ("sh", Code),
        ("fish", Code),
        ("zig", Code),
        ("hs", Code),
        ("swift", Code),
        ("cs", Code),
        ("php", Code),
        ("sql", Code),
        ("html", Code),
        ("css", Code),
        ("toml", Code),
        ("yaml", Code),
        ("yml", Code),
        ("json", Code),
        ("xml", Code),
        ("md", Text),
        ("txt", Text),
        ("rst", Text),
        ("org", Text),
        ("tex", Text),
        ("pdf", Text),
        ("epub", Text),
        ("odt", Text),
        ("docx", Text),
        ("csv", Text),
        ("log", Text),
        ("png", Image),
        ("jpg", Image),
        ("jpeg", Image),
        ("gif", Image),
        ("webp", Image),
        ("svg", Image),
        ("bmp", Image),
        ("tiff", Image),
        ("heic", Image),
        ("avif", Image),
        ("mp3", Media),
        ("flac", Media),
        ("ogg", Media),
        ("opus", Media),
        ("wav", Media),
        ("m4a", Media),
        ("mp4", Media),
        ("mkv", Media),
        ("webm", Media),
        ("mov", Media),
        ("avi", Media),
        ("zip", Archive),
        ("tar", Archive),
        ("gz", Archive),
        ("tgz", Archive),
        ("xz", Archive),
        ("zst", Archive),
        ("bz2", Archive),
        ("7z", Archive),
        ("rar", Archive),
        ("deb", Archive),
        ("rpm", Archive),
        ("iso", Archive),
    ]
};

/// The kind of a book by its extension (case-insensitive); folders are
/// `Folder`, unknown or no extension `Other`.
pub fn kind_of(name: &str, is_dir: bool) -> BookKind {
    if is_dir {
        return BookKind::Folder;
    }
    let Some((stem, ext)) = name.rsplit_once('.') else {
        return BookKind::Other;
    };
    if stem.is_empty() {
        return BookKind::Other;
    }
    let ext = ext.to_ascii_lowercase();
    EXT.iter()
        .find(|(e, _)| *e == ext)
        .map(|(_, k)| *k)
        .unwrap_or(BookKind::Other)
}

/// How many extensions [`kind_of`] knows.
pub fn known_extensions() -> usize {
    EXT.len()
}

/// A book's spine: colour by kind, height and thickness by size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spine {
    pub rgb: [u8; 3],
    /// metres
    pub height: f32,
    /// metres
    pub thick: f32,
}

pub const SPINE_MIN_H: f32 = 0.14;
pub const SPINE_MAX_H: f32 = 0.32;
pub const SPINE_MIN_T: f32 = 0.02;
pub const SPINE_MAX_T: f32 = 0.05;

/// The cosy palette of the spines (plan §5.2).
pub fn kind_rgb(k: BookKind) -> [u8; 3] {
    match k {
        BookKind::Code => [0x5f, 0x9e, 0x96],
        BookKind::Text => [0xe8, 0xd8, 0xb0],
        BookKind::Image => [0xd9, 0x8c, 0x8c],
        BookKind::Media => [0xa8, 0x8c, 0xc8],
        BookKind::Archive => [0x9a, 0x70, 0x50],
        BookKind::Other => [0x8a, 0x9a, 0xaa],
        BookKind::Folder => [0x7a, 0xa0, 0x62],
    }
}

pub fn spine(e: &Entry) -> Spine {
    let k = kind_of(e.name(), e.is_dir);
    if e.is_dir {
        // a box file: tall and thick, whatever the directory's own size
        return Spine {
            rgb: kind_rgb(k),
            height: 0.28,
            thick: SPINE_MAX_T,
        };
    }
    let l = ((e.size as f64) + 1.0).log10() as f32;
    Spine {
        rgb: kind_rgb(k),
        height: (SPINE_MIN_H + 0.03 * l).clamp(SPINE_MIN_H, SPINE_MAX_H),
        thick: (SPINE_MIN_T + 0.004 * l).clamp(SPINE_MIN_T, SPINE_MAX_T),
    }
}

// ---------------------------------------------------------------- policy

/// What goes in the vault (addendum §5.1), on normalised paths relative to
/// a library root. The defaults can only grow.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Policy {
    pub roots: Vec<String>,
    pub globs: Vec<String>,
}

impl Default for Policy {
    fn default() -> Policy {
        Policy::default_policy()
    }
}

impl Policy {
    pub fn default_policy() -> Policy {
        Policy {
            roots: [
                ".ssh",
                ".gnupg",
                ".password-store",
                ".local/share/keyrings",
                ".config/gcloud",
                ".aws",
                ".kube",
                ".docker/config.json",
            ]
            .map(String::from)
            .to_vec(),
            globs: ["**/.env", "**/.env.*", "**/*.pem", "**/id_*"]
                .map(String::from)
                .to_vec(),
        }
    }

    /// Adds `extra`'s roots and globs; never removes anything.
    pub fn grow(&mut self, extra: &Policy) {
        for r in &extra.roots {
            let r = normalize(r).join("/");
            if !r.is_empty() && !self.roots.contains(&r) {
                self.roots.push(r);
            }
        }
        for g in &extra.globs {
            if !g.is_empty() && !self.globs.contains(g) {
                self.globs.push(g.clone());
            }
        }
    }

    /// Whether `rel` (or any directory above it) is a vault path.
    pub fn is_vault(&self, rel: &str) -> bool {
        let segs = normalize(rel);
        (1..=segs.len()).any(|n| self.is_vault_exact(&segs[..n]))
    }

    fn is_vault_exact(&self, prefix: &[&str]) -> bool {
        self.roots.iter().any(|r| {
            let r = normalize(r);
            !r.is_empty() && r.as_slice() == prefix
        }) || self.globs.iter().any(|g| {
            let g: Vec<&str> = g.split('/').filter(|s| !s.is_empty()).collect();
            glob_match(&g, prefix)
        })
    }
}

/// Splits a relative path into segments, resolving `.` and `..` (never
/// above the root) and ignoring leading or doubled slashes.
pub fn normalize(rel: &str) -> Vec<&str> {
    let mut out: Vec<&str> = Vec::new();
    for s in rel.split('/') {
        match s {
            "" | "." => {}
            ".." => {
                out.pop();
            }
            s => out.push(s),
        }
    }
    out
}

/// `rel` normalised back to a string (`""` = the root).
pub fn norm(rel: &str) -> String {
    normalize(rel).join("/")
}

fn join(dir: &str, name: &str) -> String {
    if dir.is_empty() {
        name.to_string()
    } else {
        format!("{dir}/{name}")
    }
}

fn parent(rel: &str) -> Option<String> {
    let segs = normalize(rel);
    if segs.is_empty() {
        return None;
    }
    Some(segs[..segs.len() - 1].join("/"))
}

/// `**` matches any number of segments; `*` within a segment any run of
/// characters.
fn glob_match(pat: &[&str], path: &[&str]) -> bool {
    match pat.split_first() {
        None => path.is_empty(),
        Some((&"**", rest)) => (0..=path.len()).any(|k| glob_match(rest, &path[k..])),
        Some((p, rest)) => match path.split_first() {
            Some((s, prest)) => seg_match(p.as_bytes(), s.as_bytes()) && glob_match(rest, prest),
            None => false,
        },
    }
}

fn seg_match(p: &[u8], s: &[u8]) -> bool {
    match p.split_first() {
        None => s.is_empty(),
        Some((b'*', rest)) => (0..=s.len()).any(|k| seg_match(rest, &s[k..])),
        Some((c, rest)) => s.first() == Some(c) && seg_match(rest, &s[1..]),
    }
}

/// Shelf and vault: disjoint, and together every entry.
pub fn partition<'a>(p: &Policy, entries: &'a [Entry]) -> (Vec<&'a Entry>, Vec<&'a Entry>) {
    entries.iter().partition(|e| !p.is_vault(&e.rel))
}

// ---------------------------------------------------------------- config

/// Where `enter` on a file puts the window it opens.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OpenIn {
    /// the room you came to the library from
    Origin,
    /// the library's reading desk
    Library,
}

/// `[vault]` besides the policy (addendum §5.8).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VaultConf {
    pub phrase: String,
    pub auto_lock_after_s: f64,
    pub cooldown_s: f64,
    pub max_fails: u8,
    pub lockout_s: f64,
}

impl Default for VaultConf {
    fn default() -> VaultConf {
        VaultConf {
            phrase: "destrancar".into(),
            auto_lock_after_s: 20.0,
            cooldown_s: 2.0,
            max_fails: 5,
            lockout_s: 60.0,
        }
    }
}

/// The shortest phrase `[vault] phrase` accepts.
pub const PHRASE_MIN: usize = 6;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Config {
    /// `~` = the home; relative entries are under the home
    pub roots: Vec<String>,
    pub show_hidden: bool,
    /// directory names never shown, never descended
    pub exclude_dirs: Vec<String>,
    pub per_shelf: usize,
    pub search_entries: usize,
    pub search_depth: usize,
    pub search_ms: u32,
    /// the vault's, moved here from `nkw_objects::vault`
    pub policy: Policy,
    pub open_in: OpenIn,
    pub vault: VaultConf,
}

impl Default for Config {
    fn default() -> Config {
        Config {
            roots: vec!["~".into()],
            show_hidden: false,
            exclude_dirs: [".git", "node_modules", "target", ".cache"]
                .map(String::from)
                .to_vec(),
            per_shelf: 400,
            search_entries: 100_000,
            search_depth: 12,
            search_ms: 2000,
            policy: Policy::default_policy(),
            open_in: OpenIn::Origin,
            vault: VaultConf::default(),
        }
    }
}

impl Config {
    fn excluded(&self, name: &str) -> bool {
        self.exclude_dirs.iter().any(|d| d == name)
    }

    /// Whether any directory of `rel` is excluded.
    fn excluded_path(&self, rel: &str) -> bool {
        normalize(rel).iter().any(|s| self.excluded(s))
    }
}

/// `[library]` + `[vault]` of `world.toml` (other tables are someone
/// else's). Missing keys keep their defaults; the policy and the excluded
/// directories only grow.
pub fn parse_config(toml_text: &str) -> Result<Config, String> {
    let t: toml::Table = toml::from_str(toml_text).map_err(|e| e.to_string())?;
    let mut c = Config::default();
    let strs = |v: &toml::Value, key: &str| -> Result<Vec<String>, String> {
        v.as_array()
            .ok_or(format!("{key}: esperava uma lista"))?
            .iter()
            .map(|s| {
                s.as_str()
                    .map(String::from)
                    .ok_or(format!("{key}: esperava textos"))
            })
            .collect()
    };
    let uint = |v: &toml::Value, key: &str| -> Result<u64, String> {
        v.as_integer()
            .filter(|n| *n >= 0)
            .map(|n| n as u64)
            .ok_or(format!("{key}: esperava um inteiro ≥ 0"))
    };
    let float = |v: &toml::Value, key: &str| -> Result<f64, String> {
        v.as_float()
            .or(v.as_integer().map(|n| n as f64))
            .filter(|x| *x >= 0.0)
            .ok_or(format!("{key}: esperava um número ≥ 0"))
    };
    if let Some(lib) = t.get("library") {
        let lib = lib.as_table().ok_or("[library]: esperava uma tabela")?;
        for (k, v) in lib {
            match k.as_str() {
                "roots" => {
                    let r = strs(v, "library.roots")?;
                    if !r.is_empty() {
                        c.roots = r;
                    }
                }
                "show_hidden" => {
                    c.show_hidden = v
                        .as_bool()
                        .ok_or("library.show_hidden: esperava true/false")?
                }
                "exclude_dirs" => {
                    for d in strs(v, "library.exclude_dirs")? {
                        if !d.is_empty() && !c.excluded(&d) {
                            c.exclude_dirs.push(d);
                        }
                    }
                }
                "per_shelf" => c.per_shelf = uint(v, k)?.clamp(1, 400) as usize,
                "search_entries" => c.search_entries = uint(v, k)?.clamp(1, 100_000) as usize,
                "search_depth" => c.search_depth = uint(v, k)?.clamp(1, 12) as usize,
                "search_ms" => c.search_ms = uint(v, k)?.clamp(1, 2000) as u32,
                "open_in" => {
                    c.open_in = match v.as_str() {
                        Some("origin") => OpenIn::Origin,
                        Some("library") => OpenIn::Library,
                        _ => return Err("library.open_in: \"origin\" ou \"library\"".into()),
                    }
                }
                _ => {}
            }
        }
    }
    if let Some(vault) = t.get("vault") {
        let vault = vault.as_table().ok_or("[vault]: esperava uma tabela")?;
        let mut extra = Policy {
            roots: Vec::new(),
            globs: Vec::new(),
        };
        for (k, v) in vault {
            match k.as_str() {
                "phrase" => {
                    let p = v.as_str().ok_or("vault.phrase: esperava um texto")?;
                    if p.chars().count() < PHRASE_MIN {
                        return Err(format!("vault.phrase: mínimo {PHRASE_MIN} caracteres"));
                    }
                    c.vault.phrase = p.into();
                }
                "auto_lock_after_s" => c.vault.auto_lock_after_s = float(v, k)?.clamp(1.0, 600.0),
                "cooldown_s" => c.vault.cooldown_s = float(v, k)?.min(60.0),
                "max_fails" => c.vault.max_fails = uint(v, k)?.clamp(1, 20) as u8,
                "lockout_s" => c.vault.lockout_s = float(v, k)?.min(3600.0),
                "extra_roots" => extra.roots = strs(v, "vault.extra_roots")?,
                "extra_globs" => extra.globs = strs(v, "vault.extra_globs")?,
                _ => {}
            }
        }
        c.policy.grow(&extra);
    }
    Ok(c)
}

// ---------------------------------------------------------------- shelves

/// Boards per bookcase (a board per 0.35 m of height).
pub const BOARDS: usize = 6;
/// The LRU of shelves the library keeps.
pub const CACHE: usize = 32;
/// The most search hits brought to the search table.
pub const HITS: usize = 60;
/// The lectern's preview limit, bytes.
pub const PREVIEW_MAX: usize = 64 * 1024;

#[derive(Clone, Debug, PartialEq)]
pub struct Shelf {
    pub root: usize,
    /// `""` = the root
    pub rel: String,
    /// folders first, then by name; ≤ `per_shelf`; vault entries removed,
    /// hidden removed unless `show_hidden`
    pub entries: Vec<Entry>,
    /// the "+N" box
    pub more: usize,
    /// vault entries in this directory (counted, never listed)
    pub vault_count: usize,
}

impl Shelf {
    /// Books per board.
    pub fn cols(&self) -> usize {
        cols_for(self.entries.len())
    }

    /// Boards in use.
    pub fn rows(&self) -> usize {
        let n = self.entries.len();
        if n == 0 { 0 } else { n.div_ceil(self.cols()) }
    }
}

/// A board takes at least this many books before the next one is used (a
/// small shelf reads as a row, not a column).
pub const MIN_COLS: usize = 12;

/// Books per board for `n` books on [`BOARDS`] boards.
pub fn cols_for(n: usize) -> usize {
    n.div_ceil(BOARDS).max(n.min(MIN_COLS)).max(1)
}

/// Folders first, then by name (bytes: locale-free and stable).
fn order(entries: &mut [Entry]) {
    entries.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then_with(|| a.name().cmp(b.name())));
}

fn check_dir(c: &Config, fs: &dyn FsPort, root: usize, rel: &str) -> Result<(), FsError> {
    if c.excluded_path(rel) {
        return Err(FsError::Denied);
    }
    if !rel.is_empty() {
        if fs.resolves_outside(root, rel) {
            return Err(FsError::Boundary);
        }
        if let (Some(a), Some(b)) = (fs.dev(root, ""), fs.dev(root, rel))
            && a != b
        {
            return Err(FsError::Boundary);
        }
    }
    Ok(())
}

/// One directory as a shelf. Vault directories are refused (`Denied`):
/// their names only ever come from [`read_vault`].
pub fn read_shelf(c: &Config, fs: &dyn FsPort, root: usize, rel: &str) -> Result<Shelf, FsError> {
    let rel = norm(rel);
    if !rel.is_empty() && c.policy.is_vault(&rel) {
        return Err(FsError::Denied);
    }
    check_dir(c, fs, root, &rel)?;
    let all = fs.read_dir(root, &rel)?;
    let mut vault_count = 0;
    let mut entries = Vec::with_capacity(all.len().min(c.per_shelf + 1));
    for e in all {
        if c.policy.is_vault(&e.rel) {
            vault_count += 1;
            continue;
        }
        if e.hidden && !c.show_hidden {
            continue;
        }
        if e.is_dir && c.excluded(e.name()) {
            continue;
        }
        if fs.resolves_outside(root, &e.rel) {
            continue;
        }
        entries.push(e);
    }
    order(&mut entries);
    let more = entries.len().saturating_sub(c.per_shelf);
    entries.truncate(c.per_shelf);
    Ok(Shelf {
        root,
        rel,
        entries,
        more,
        vault_count,
    })
}

/// The vault's cards for `rel` (`""` = the vault's top: every vault entry
/// of the root directory and of the policy's nested roots, a vault folder
/// opened one level so its files are cards). Hidden names are shown here.
pub fn read_vault(c: &Config, fs: &dyn FsPort, root: usize, rel: &str) -> Result<Shelf, FsError> {
    let rel = norm(rel);
    let mut cards: Vec<Entry> = Vec::new();
    if rel.is_empty() {
        let mut dirs: BTreeSet<String> = BTreeSet::from([String::new()]);
        for r in &c.policy.roots {
            if let Some(p) = parent(r) {
                dirs.insert(p);
            }
        }
        let mut seen = BTreeSet::new();
        for d in dirs {
            if (!d.is_empty() && c.policy.is_vault(&d)) || check_dir(c, fs, root, &d).is_err() {
                continue;
            }
            let Ok(list) = fs.read_dir(root, &d) else {
                continue;
            };
            for e in list.into_iter().filter(|e| c.policy.is_vault(&e.rel)) {
                if e.is_dir && check_dir(c, fs, root, &e.rel).is_ok() {
                    if let Ok(inner) = fs.read_dir(root, &e.rel) {
                        for i in inner {
                            if seen.insert(i.rel.clone()) {
                                cards.push(i);
                            }
                        }
                    }
                } else if seen.insert(e.rel.clone()) {
                    cards.push(e);
                }
            }
        }
    } else {
        if !c.policy.is_vault(&rel) {
            return Err(FsError::Denied);
        }
        check_dir(c, fs, root, &rel)?;
        cards = fs.read_dir(root, &rel)?;
    }
    cards.retain(|e| !fs.resolves_outside(root, &e.rel));
    order(&mut cards);
    let more = cards.len().saturating_sub(c.per_shelf);
    cards.truncate(c.per_shelf);
    Ok(Shelf {
        root,
        rel,
        entries: cards,
        more,
        vault_count: 0,
    })
}

// ---------------------------------------------------------------- search

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Search {
    /// ≤ [`HITS`], best first (ties by path)
    pub hits: Vec<Entry>,
    pub scanned: usize,
    /// a budget (entries, depth, time) stopped it
    pub truncated: bool,
}

/// Fuzzy search of `q` over the subtree at `rel`, breadth first, within
/// `search_entries`, `search_depth` and `search_ms` (`clock`: ms, any
/// epoch). Vault paths, hidden names (unless shown), excluded directories,
/// mount boundaries and outside symlinks never enter it.
pub fn search_tree(
    c: &Config,
    fs: &dyn FsPort,
    root: usize,
    rel: &str,
    q: &str,
    clock: &mut dyn FnMut() -> u32,
) -> Search {
    let base = norm(rel);
    let mut out = Search::default();
    let q = q.trim();
    if q.is_empty() || (!base.is_empty() && c.policy.is_vault(&base)) {
        return out;
    }
    if check_dir(c, fs, root, &base).is_err() {
        return out;
    }
    let start = clock();
    let pat = Pattern::new(
        q,
        CaseMatching::Ignore,
        Normalization::Smart,
        AtomKind::Fuzzy,
    );
    let mut m = Matcher::new(nucleo_matcher::Config::DEFAULT.match_paths());
    let mut buf = Vec::new();
    let root_dev = fs.dev(root, "");
    let mut scored: Vec<(u32, Entry)> = Vec::new();
    let mut queue: VecDeque<(String, usize)> = VecDeque::from([(base.clone(), 0)]);
    let strip = |r: &str| -> String {
        if base.is_empty() {
            r.to_string()
        } else {
            r.strip_prefix(&base)
                .map(|s| s.trim_start_matches('/').to_string())
                .unwrap_or_else(|| r.to_string())
        }
    };
    'walk: while let Some((dir, depth)) = queue.pop_front() {
        if clock().wrapping_sub(start) >= c.search_ms {
            out.truncated = true;
            break;
        }
        let Ok(list) = fs.read_dir(root, &dir) else {
            continue;
        };
        for e in list {
            if out.scanned >= c.search_entries {
                out.truncated = true;
                break 'walk;
            }
            if c.policy.is_vault(&e.rel) || (e.hidden && !c.show_hidden) {
                continue;
            }
            if e.is_dir && c.excluded(e.name()) {
                continue;
            }
            if fs.resolves_outside(root, &e.rel) {
                continue;
            }
            out.scanned += 1;
            if out.scanned % 512 == 0 && clock().wrapping_sub(start) >= c.search_ms {
                out.truncated = true;
                break 'walk;
            }
            let hay = strip(&e.rel);
            if let Some(s) = pat.score(Utf32Str::new(&hay, &mut buf), &mut m) {
                scored.push((s, e.clone()));
            }
            if e.is_dir {
                if depth + 1 >= c.search_depth {
                    out.truncated = true;
                } else if root_dev.is_none() || fs.dev(root, &e.rel) == root_dev {
                    queue.push_back((e.rel, depth + 1));
                }
            }
        }
    }
    scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.rel.cmp(&b.1.rel)));
    out.hits = scored.into_iter().take(HITS).map(|(_, e)| e).collect();
    out
}

// ---------------------------------------------------------------- library

/// The browsing state: the shelf you look at, the cursor, the LRU of the
/// shelves seen, the last search.
#[derive(Clone, Debug)]
pub struct Library {
    config: Config,
    /// most recent last; the current shelf is `cache.back()` once opened
    cache: VecDeque<Shelf>,
    current: Option<(usize, String)>,
    cursor: (usize, usize),
    search: Option<Search>,
}

impl Library {
    pub fn new(c: Config) -> Library {
        Library {
            config: c,
            cache: VecDeque::new(),
            current: None,
            cursor: (0, 0),
            search: None,
        }
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Reads `rel` and makes it the current shelf.
    pub fn open(&mut self, root: usize, rel: &str, fs: &dyn FsPort) -> Result<&Shelf, FsError> {
        let s = read_shelf(&self.config, fs, root, rel)?;
        Ok(self.show(s))
    }

    /// Makes a shelf read elsewhere (the scan thread) the current one; the
    /// cursor goes to the first book, or to the folder you came up from.
    pub fn show(&mut self, s: Shelf) -> &Shelf {
        let from = self.current.clone();
        self.search = None;
        self.cache.retain(|x| !(x.root == s.root && x.rel == s.rel));
        self.current = Some((s.root, s.rel.clone()));
        self.cache.push_back(s);
        while self.cache.len() > CACHE {
            self.cache.pop_front();
        }
        self.cursor = (0, 0);
        let cur = self.cache.back().expect("just pushed");
        if let Some((r, came)) = from
            && r == cur.root
            && let Some(i) = cur.entries.iter().position(|e| e.rel == came)
        {
            let cols = cur.cols();
            self.cursor = (i / cols, i % cols);
        }
        self.cache.back().expect("just pushed")
    }

    /// Keeps a shelf in the LRU without making it current (another wall's).
    pub fn remember(&mut self, s: Shelf) {
        if self
            .current
            .as_ref()
            .is_some_and(|(r, rel)| *r == s.root && *rel == s.rel)
        {
            self.show(s);
            return;
        }
        self.cache.retain(|x| !(x.root == s.root && x.rel == s.rel));
        self.cache.push_back(s);
        while self.cache.len() > CACHE {
            let cur = self.current.clone();
            let i = self
                .cache
                .iter()
                .position(|x| cur.as_ref() != Some(&(x.root, x.rel.clone())))
                .unwrap_or(0);
            self.cache.remove(i);
        }
    }

    /// A shelf seen recently, without reading.
    pub fn cached(&self, root: usize, rel: &str) -> Option<&Shelf> {
        let rel = norm(rel);
        self.cache.iter().find(|s| s.root == root && s.rel == rel)
    }

    /// One level up (the cache first); `None` at the root.
    pub fn up(&mut self, fs: &dyn FsPort) -> Option<&Shelf> {
        let (root, rel) = self.current.clone()?;
        let p = parent(&rel)?;
        let s = match self.cached(root, &p) {
            Some(s) => s.clone(),
            None => read_shelf(&self.config, fs, root, &p).ok()?,
        };
        Some(self.show(s))
    }

    pub fn current(&self) -> Option<&Shelf> {
        let (root, rel) = self.current.as_ref()?;
        self.cached(*root, rel)
    }

    /// Leaves the shelf (the cache stays).
    pub fn close(&mut self) {
        self.current = None;
        self.search = None;
        self.cursor = (0, 0);
    }

    /// Forget every shelf (the vault's cards are never cached here).
    pub fn clear(&mut self) {
        self.close();
        self.cache.clear();
    }

    /// (row = board, from the top; col = book, from the left).
    pub fn cursor(&self) -> (usize, usize) {
        self.cursor
    }

    /// Moves the cursor; `false` at an edge (nothing moved).
    pub fn step(&mut self, dir: Dir) -> bool {
        let Some(s) = self.current() else {
            return false;
        };
        let n = s.entries.len();
        if n == 0 {
            return false;
        }
        let cols = s.cols();
        let rows = s.rows();
        let (r, c) = self.cursor;
        let len_of = |row: usize| (n - row * cols).min(cols);
        let next = match dir {
            Dir::Left => (c > 0).then(|| (r, c - 1)),
            Dir::Right => (c + 1 < len_of(r)).then(|| (r, c + 1)),
            Dir::Up => (r > 0).then(|| (r - 1, c.min(len_of(r - 1) - 1))),
            Dir::Down => (r + 1 < rows).then(|| (r + 1, c.min(len_of(r + 1) - 1))),
        };
        match next {
            Some(p) => {
                self.cursor = p;
                true
            }
            None => false,
        }
    }

    /// Puts the cursor on book `i` of the shelf (a pick).
    pub fn select(&mut self, i: usize) -> bool {
        let Some(s) = self.current() else {
            return false;
        };
        if i >= s.entries.len() {
            return false;
        }
        let cols = s.cols();
        self.cursor = (i / cols, i % cols);
        true
    }

    /// The index of the selected book.
    pub fn index(&self) -> Option<usize> {
        let s = self.current()?;
        let i = self.cursor.0 * s.cols() + self.cursor.1;
        (i < s.entries.len()).then_some(i)
    }

    pub fn selected(&self) -> Option<&Entry> {
        let i = self.index()?;
        self.current()?.entries.get(i)
    }

    /// "projeto › src › lib" (≤ 48 chars, middle-ellipsised); the root's
    /// label at the root.
    pub fn breadcrumb(&self) -> String {
        let Some((root, rel)) = &self.current else {
            return String::new();
        };
        if rel.is_empty() {
            return root_label(
                self.config
                    .roots
                    .get(*root)
                    .map(String::as_str)
                    .unwrap_or("~"),
            );
        }
        crumb(&normalize(rel), BREADCRUMB_MAX)
    }

    /// Fuzzy search over the current subtree (the root when no shelf is
    /// open), within the budget; kept as [`Library::last_search`].
    pub fn search(&mut self, q: &str, fs: &dyn FsPort, clock: &mut dyn FnMut() -> u32) -> Search {
        let (root, rel) = self.current.clone().unwrap_or((0, String::new()));
        let s = search_tree(&self.config, fs, root, &rel, q, clock);
        self.search = Some(s.clone());
        s
    }

    /// Keeps a search run elsewhere (the scan thread).
    pub fn set_search(&mut self, s: Option<Search>) {
        self.search = s;
    }

    pub fn last_search(&self) -> Option<&Search> {
        self.search.as_ref()
    }

    pub fn is_vault(&self, rel: &str) -> bool {
        self.config.policy.is_vault(rel)
    }
}

pub const BREADCRUMB_MAX: usize = 48;
const SEP: &str = " › ";

/// The label of a root path: its last segment, `~` for the home.
pub fn root_label(root: &str) -> String {
    let t = root.trim_end_matches('/');
    if t.is_empty() || t == "~" {
        return "~".into();
    }
    t.rsplit('/').next().unwrap_or("~").to_string()
}

/// Segments joined with " › ", the middle replaced by "…" to fit `max`
/// characters (the first and the last segments stay when they fit).
pub fn crumb(segs: &[&str], max: usize) -> String {
    let full = segs.join(SEP);
    if full.chars().count() <= max {
        return full;
    }
    let first = segs[0];
    let mut tail: Vec<&str> = Vec::new();
    let room = |tail: &[&str]| {
        let mut parts = vec![first, "…"];
        parts.extend(tail.iter().rev());
        parts.join(SEP).chars().count()
    };
    for s in segs[1..].iter().rev() {
        let mut t = tail.clone();
        t.push(s);
        if room(&t) > max {
            break;
        }
        tail = t;
    }
    if tail.is_empty() {
        // even "first › … › last" is too long: cut characters in the middle
        let last = segs[segs.len() - 1];
        let s = format!("{first}{SEP}…{SEP}{last}");
        return ellipsize(&s, max);
    }
    let mut parts = vec![first, "…"];
    parts.extend(tail.iter().rev());
    parts.join(SEP)
}

fn ellipsize(s: &str, max: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max {
        return s.to_string();
    }
    let keep = max.saturating_sub(1);
    let head = keep.div_ceil(2);
    let tail = keep - head;
    let mut out: String = chars[..head].iter().collect();
    out.push('…');
    out.extend(&chars[chars.len() - tail..]);
    out
}

// ---------------------------------------------------------------- MemFs

#[derive(Clone, Debug, Default)]
struct MemNode {
    entry: Option<Entry>,
    children: BTreeSet<String>,
    dev: u64,
    outside: bool,
    text: Option<String>,
}

/// An in-memory file system: the demo's fixture library and the tests'
/// fake. Every call is counted (`calls`), so a test can prove nothing was
/// read.
#[derive(Debug, Default)]
pub struct MemFs {
    roots: Vec<BTreeMap<String, MemNode>>,
    calls: AtomicUsize,
}

impl MemFs {
    pub fn new(roots: usize) -> MemFs {
        let mut m = MemFs::default();
        for _ in 0..roots.max(1) {
            m.roots
                .push(BTreeMap::from([(String::new(), MemNode::default())]));
        }
        m
    }

    fn node(&mut self, root: usize, rel: &str, is_dir: bool, size: u64, mtime: i64) {
        let rel = norm(rel);
        if rel.is_empty() {
            return;
        }
        if let Some(p) = parent(&rel) {
            if !self.roots[root].contains_key(&p) {
                self.node(root, &p, true, 4096, mtime);
            }
            let name = rel.rsplit('/').next().unwrap_or("").to_string();
            let dev = self.roots[root].get(&p).map(|n| n.dev).unwrap_or(0);
            let pn = self.roots[root].get_mut(&p).expect("made above");
            pn.children.insert(name.clone());
            let n = self.roots[root].entry(rel.clone()).or_default();
            n.dev = dev;
            n.entry = Some(Entry {
                hidden: name.starts_with('.'),
                rel,
                is_dir,
                size,
                mtime,
            });
        }
    }

    pub fn dir(&mut self, root: usize, rel: &str) -> &mut MemFs {
        self.node(root, rel, true, 4096, 0);
        self
    }

    pub fn file(&mut self, root: usize, rel: &str, size: u64) -> &mut MemFs {
        self.node(root, rel, false, size, 0);
        self
    }

    /// A file with text content (the lectern's preview).
    pub fn text(&mut self, root: usize, rel: &str, text: &str) -> &mut MemFs {
        self.node(root, rel, false, text.len() as u64, 0);
        if let Some(n) = self.roots[root].get_mut(&norm(rel)) {
            n.text = Some(text.into());
        }
        self
    }

    /// `rel` (a directory) is on another device, and so is all below.
    pub fn mount(&mut self, root: usize, rel: &str, dev: u64) -> &mut MemFs {
        self.dir(root, rel);
        let rel = norm(rel);
        let below: Vec<String> = self.roots[root]
            .keys()
            .filter(|k| **k == rel || k.starts_with(&format!("{rel}/")))
            .cloned()
            .collect();
        for k in below {
            if let Some(n) = self.roots[root].get_mut(&k) {
                n.dev = dev;
            }
        }
        self
    }

    /// `rel` is a symlink out of the root.
    pub fn link_out(&mut self, root: usize, rel: &str, is_dir: bool) -> &mut MemFs {
        self.node(root, rel, is_dir, 0, 0);
        if let Some(n) = self.roots[root].get_mut(&norm(rel)) {
            n.outside = true;
        }
        self
    }

    /// Port calls so far.
    pub fn calls(&self) -> usize {
        self.calls.load(Ordering::Relaxed)
    }

    /// Entries in all roots.
    pub fn len(&self) -> usize {
        self.roots.iter().map(|r| r.len() - 1).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn get(&self, root: usize, rel: &str) -> Option<&MemNode> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        self.roots.get(root)?.get(&norm(rel))
    }
}

impl FsPort for MemFs {
    fn read_dir(&self, root: usize, rel: &str) -> Result<Vec<Entry>, FsError> {
        let n = self.get(root, rel).ok_or(FsError::NotFound)?;
        if n.entry.as_ref().is_some_and(|e| !e.is_dir) {
            return Err(FsError::Io("não é uma pasta".into()));
        }
        let rel = norm(rel);
        Ok(n.children
            .iter()
            .filter_map(|c| {
                self.roots[root]
                    .get(&join(&rel, c))
                    .and_then(|n| n.entry.clone())
            })
            .collect())
    }

    fn dev(&self, root: usize, rel: &str) -> Option<u64> {
        self.get(root, rel).map(|n| n.dev)
    }

    fn resolves_outside(&self, root: usize, rel: &str) -> bool {
        let segs = normalize(rel);
        (1..=segs.len()).any(|k| {
            self.roots
                .get(root)
                .and_then(|r| r.get(&segs[..k].join("/")))
                .is_some_and(|n| n.outside)
        })
    }

    fn read_text(&self, root: usize, rel: &str, max: usize) -> Option<String> {
        let t = self.get(root, rel)?.text.as_ref()?;
        if t.len() <= max {
            return Some(t.clone());
        }
        let mut end = max;
        while !t.is_char_boundary(end) {
            end -= 1;
        }
        Some(t[..end].to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(rel: &str, is_dir: bool, size: u64) -> Entry {
        Entry {
            rel: rel.into(),
            is_dir,
            size,
            mtime: 0,
            hidden: rel.rsplit('/').next().unwrap().starts_with('.'),
        }
    }

    /// The fixture tree of the plan (`projeto/`, `notas.md`, `fotos/`,
    /// a fictional `.ssh/`).
    fn fs() -> MemFs {
        let mut m = MemFs::new(1);
        m.dir(0, "projeto/src")
            .file(0, "projeto/src/lib.rs", 1200)
            .file(0, "projeto/src/main.rs", 300)
            .dir(0, "projeto/docs")
            .dir(0, "projeto/testes")
            .file(0, "projeto/.env", 40)
            .file(0, "projeto/Cargo.toml", 200)
            .dir(0, "projeto/target/debug")
            .dir(0, "projeto/.git")
            .text(0, "notas.md", "# notas\nolá")
            .dir(0, "fotos")
            .file(0, "fotos/praia.jpg", 2_000_000)
            .file(0, ".ssh/id_demo", 411)
            .file(0, ".ssh/config", 80)
            .file(0, ".ssh/known_hosts", 900)
            .file(0, ".bashrc", 10)
            .file(0, "chave.pem", 10);
        m
    }

    fn names(s: &Shelf) -> Vec<&str> {
        s.entries.iter().map(|e| e.name()).collect()
    }

    #[test]
    fn kind_of_table() {
        assert!(known_extensions() >= 40);
        use BookKind::*;
        for (n, d, k) in [
            ("lib.rs", false, Code),
            ("main.GO", false, Code),
            ("x.py", false, Code),
            ("Cargo.toml", false, Code),
            ("notas.md", false, Text),
            ("a.txt", false, Text),
            ("livro.pdf", false, Text),
            ("praia.jpg", false, Image),
            ("x.PNG", false, Image),
            ("a.svg", false, Image),
            ("song.flac", false, Media),
            ("v.mkv", false, Media),
            ("a.tar", false, Archive),
            ("a.tar.gz", false, Archive),
            ("x.zip", false, Archive),
            ("Makefile", false, Other),
            (".bashrc", false, Other),
            ("x.unknown", false, Other),
            ("noext.", false, Other),
            ("src", true, Folder),
            ("a.rs", true, Folder),
        ] {
            assert_eq!(kind_of(n, d), k, "{n}");
        }
    }

    #[test]
    fn spine_is_monotone_in_size_and_clamped() {
        let mut last = 0.0;
        let mut last_t = 0.0;
        for size in [0, 1, 10, 100, 1_000, 10_000, 1 << 20, 1 << 30, u64::MAX] {
            let s = spine(&e("a.txt", false, size));
            assert!(s.height >= last && s.thick >= last_t, "{size}");
            assert!((SPINE_MIN_H..=SPINE_MAX_H).contains(&s.height));
            assert!((SPINE_MIN_T..=SPINE_MAX_T).contains(&s.thick));
            last = s.height;
            last_t = s.thick;
        }
        assert_eq!(spine(&e("a.txt", false, 0)).height, SPINE_MIN_H);
        assert_eq!(spine(&e("a.txt", false, u64::MAX)).height, SPINE_MAX_H);
        assert_eq!(spine(&e("a.rs", false, 5)).rgb, kind_rgb(BookKind::Code));
        assert_ne!(kind_rgb(BookKind::Code), kind_rgb(BookKind::Text));
        assert_eq!(spine(&e("src", true, 4096)).rgb, kind_rgb(BookKind::Folder));
    }

    #[test]
    fn shelf_folders_first_then_by_name_vault_and_hidden_out() {
        let c = Config::default();
        let s = read_shelf(&c, &fs(), 0, "").unwrap();
        assert_eq!(names(&s), ["fotos", "projeto", "notas.md"]);
        // .ssh and chave.pem are vault; .bashrc hidden
        assert_eq!(s.vault_count, 2);
        assert_eq!(s.more, 0);
        let p = read_shelf(&c, &fs(), 0, "projeto").unwrap();
        // .env is vault (and hidden); target and .git excluded
        assert_eq!(names(&p), ["docs", "src", "testes", "Cargo.toml"]);
        assert_eq!(p.vault_count, 1);
        assert_eq!(p.rel, "projeto");
    }

    #[test]
    fn ordering_is_locale_free_bytes() {
        let mut m = MemFs::new(1);
        for n in ["b", "B", "a", "á", "_x", "Z"] {
            m.file(0, n, 1);
        }
        m.dir(0, "zz");
        let s = read_shelf(&Config::default(), &m, 0, "").unwrap();
        assert_eq!(names(&s), ["zz", "B", "Z", "_x", "a", "b", "á"]);
    }

    #[test]
    fn show_hidden_shows_dotfiles_but_never_the_vault() {
        let c = Config {
            show_hidden: true,
            ..Config::default()
        };
        let s = read_shelf(&c, &fs(), 0, "").unwrap();
        assert_eq!(names(&s), ["fotos", "projeto", ".bashrc", "notas.md"]);
        let p = read_shelf(&c, &fs(), 0, "projeto").unwrap();
        assert!(!names(&p).contains(&".env"));
        assert!(!names(&p).contains(&".git"), "excluded even when shown");
    }

    #[test]
    fn per_shelf_cut_and_more() {
        let mut m = MemFs::new(1);
        for i in 0..450 {
            m.file(0, &format!("f{i:03}.txt"), 1);
        }
        let s = read_shelf(&Config::default(), &m, 0, "").unwrap();
        assert_eq!(s.entries.len(), 400);
        assert_eq!(s.more, 50);
        assert_eq!(s.entries[399].name(), "f399.txt");
        let c = Config {
            per_shelf: 10,
            ..Config::default()
        };
        let s = read_shelf(&c, &m, 0, "").unwrap();
        assert_eq!((s.entries.len(), s.more), (10, 440));
        assert_eq!((s.cols(), s.rows()), (10, 1), "a short shelf is one row");
        assert_eq!((cols_for(0), cols_for(12), cols_for(30)), (1, 12, 12));
        assert_eq!((cols_for(400), cols_for(72), cols_for(73)), (67, 12, 13));
    }

    #[test]
    fn excluded_dirs_are_never_descended() {
        let mut m = fs();
        m.file(0, "projeto/node_modules/x/index.js", 1);
        let c = Config::default();
        let before = m.calls();
        assert_eq!(
            read_shelf(&c, &m, 0, "projeto/target"),
            Err(FsError::Denied)
        );
        assert_eq!(
            read_shelf(&c, &m, 0, "projeto/node_modules/x"),
            Err(FsError::Denied)
        );
        assert_eq!(m.calls(), before, "refused before any read");
        let q = |_: &str| search_tree(&c, &m, 0, "", "index", &mut || 0).hits;
        assert!(q("x").is_empty());
    }

    #[test]
    fn mount_boundaries_and_outside_symlinks_are_leaves() {
        let mut m = fs();
        m.mount(0, "rede", 7)
            .file(0, "rede/longe.txt", 1)
            .link_out(0, "atalho", true)
            .link_out(0, "fora.txt", false);
        let c = Config::default();
        let s = read_shelf(&c, &m, 0, "").unwrap();
        assert!(names(&s).contains(&"rede"), "the mount point is shown");
        assert!(!names(&s).contains(&"atalho"), "symlinks out are not");
        assert!(!names(&s).contains(&"fora.txt"));
        assert_eq!(read_shelf(&c, &m, 0, "rede"), Err(FsError::Boundary));
        assert_eq!(read_shelf(&c, &m, 0, "atalho"), Err(FsError::Boundary));
        let hits = search_tree(&c, &m, 0, "", "longe", &mut || 0).hits;
        assert!(hits.is_empty(), "search stops at the boundary");
    }

    #[test]
    fn vault_dirs_are_refused_as_shelves() {
        let c = Config::default();
        assert_eq!(read_shelf(&c, &fs(), 0, ".ssh"), Err(FsError::Denied));
        assert_eq!(
            read_shelf(&c, &fs(), 0, "projeto/../.ssh"),
            Err(FsError::Denied)
        );
        assert_eq!(read_shelf(&c, &fs(), 0, "nada"), Err(FsError::NotFound));
    }

    #[test]
    fn the_vault_cards_are_names_of_the_vault_partition() {
        let mut m = fs();
        m.file(0, ".config/gcloud/credentials.db", 5)
            .file(0, ".config/acme.toml", 5);
        let c = Config::default();
        let v = read_vault(&c, &m, 0, "").unwrap();
        let mut got: Vec<&str> = v.entries.iter().map(|e| e.rel.as_str()).collect();
        got.sort();
        assert_eq!(
            got,
            [
                ".config/gcloud/credentials.db",
                ".ssh/config",
                ".ssh/id_demo",
                ".ssh/known_hosts",
                "chave.pem"
            ]
        );
        let ssh = read_vault(&c, &m, 0, ".ssh").unwrap();
        assert_eq!(ssh.entries.len(), 3);
        assert_eq!(read_vault(&c, &m, 0, "projeto"), Err(FsError::Denied));
    }

    #[test]
    fn partition_is_disjoint_and_complete() {
        let pol = Policy::default_policy();
        let entries: Vec<Entry> = [
            ".ssh",
            ".ssh/id_demo",
            "src",
            "src/lib.rs",
            ".env",
            "README.md",
            "k/id_demo",
        ]
        .map(|r| e(r, false, 1))
        .to_vec();
        let (shelf, vault) = partition(&pol, &entries);
        assert_eq!(shelf.len() + vault.len(), entries.len());
        for x in &entries {
            let s = shelf.iter().any(|y| std::ptr::eq(*y, x));
            let v = vault.iter().any(|y| std::ptr::eq(*y, x));
            assert!(s ^ v, "{} in exactly one", x.rel);
        }
        let names: Vec<&str> = vault.iter().map(|e| e.rel.as_str()).collect();
        assert_eq!(names, [".ssh", ".ssh/id_demo", ".env", "k/id_demo"]);
    }

    #[test]
    fn policy_default_roots_and_globs_with_normalisation() {
        let pol = Policy::default_policy();
        for rel in [
            ".ssh",
            ".ssh/id_ed25519",
            ".gnupg/x",
            ".password-store/acme/site.gpg",
            ".local/share/keyrings/login.keyring",
            ".config/gcloud/credentials.db",
            ".aws/credentials",
            ".kube/config",
            ".docker/config.json",
            ".env",
            "a/b/.env.local",
            "demo/.env",
            "cert.pem",
            "proj/tls/server.pem",
            "backup/id_rsa",
            "backup/id_rsa.pub",
            "a/../.ssh/known_hosts",
            "./.ssh",
            "/.aws//config",
        ] {
            assert!(pol.is_vault(rel), "{rel} is vault");
        }
        for rel in [
            "",
            "src/main.rs",
            ".sshx",
            "notes/.ssh-tips.md",
            ".docker/daemon.json",
            "env",
            ".envrc",
            "pem.txt",
            "my_id_card.png",
            ".config/acme",
            ".ssh/../notes.txt",
            "../.bashrc",
        ] {
            assert!(!pol.is_vault(rel), "{rel} is shelf");
        }
        assert_eq!(Policy::default(), pol);
    }

    #[test]
    fn grow_only_adds() {
        let mut pol = Policy::default_policy();
        let before = pol.clone();
        pol.grow(&Policy {
            roots: vec![],
            globs: vec![],
        });
        assert_eq!(pol, before);
        pol.grow(&Policy {
            roots: vec![
                ".config/acme-secrets".into(),
                ".ssh".into(),
                "./x/../.vault/".into(),
            ],
            globs: vec!["**/*.key".into(), "**/.env".into()],
        });
        for r in &before.roots {
            assert!(pol.roots.contains(r));
        }
        for g in &before.globs {
            assert!(pol.globs.contains(g));
        }
        assert_eq!(pol.roots.len(), before.roots.len() + 2, "no duplicates");
        assert_eq!(pol.globs.len(), before.globs.len() + 1);
        assert!(pol.is_vault(".config/acme-secrets/token"));
        assert!(pol.is_vault(".vault/a"));
        assert!(pol.is_vault("deploy/site.key"));
        assert!(pol.is_vault(".ssh/config"), "defaults still hold");
    }

    #[test]
    fn an_entry_has_no_content_fields() {
        // exhaustive destructuring: adding a field (say, bytes) breaks this
        let Entry {
            rel,
            is_dir,
            size,
            mtime,
            hidden,
        } = e("id_demo", false, 411);
        assert_eq!(
            (rel.as_str(), is_dir, size, mtime, hidden),
            ("id_demo", false, 411, 0, false)
        );
    }

    #[test]
    fn config_defaults_and_parse() {
        let d = Config::default();
        assert_eq!(
            (d.per_shelf, d.search_entries, d.search_depth, d.search_ms),
            (400, 100_000, 12, 2000)
        );
        assert_eq!(d.exclude_dirs, [".git", "node_modules", "target", ".cache"]);
        assert!(!d.show_hidden);
        assert_eq!(parse_config("").unwrap(), d);
        assert_eq!(parse_config("[walk]\nspeed = 3.0\n").unwrap(), d);
        let c = parse_config(
            r#"
[library]
roots = ["~", "~/projeto"]
show_hidden = true
exclude_dirs = ["dist"]
per_shelf = 900
search_ms = 500
open_in = "library"

[vault]
phrase = "abre-te sésamo"
auto_lock_after_s = 30
max_fails = 3
extra_roots = [".config/acme-secrets"]
extra_globs = ["**/*.key"]
"#,
        )
        .unwrap();
        assert_eq!(c.roots, ["~", "~/projeto"]);
        assert!(c.show_hidden);
        assert!(c.exclude_dirs.contains(&"dist".to_string()));
        assert!(c.exclude_dirs.contains(&".git".to_string()), "only grows");
        assert_eq!(c.per_shelf, 400, "clamped");
        assert_eq!(c.search_ms, 500);
        assert_eq!(c.open_in, OpenIn::Library);
        assert_eq!(c.vault.phrase, "abre-te sésamo");
        assert_eq!(c.vault.auto_lock_after_s, 30.0);
        assert_eq!(c.vault.max_fails, 3);
        assert!(c.policy.is_vault(".config/acme-secrets/x"));
        assert!(c.policy.is_vault("a/b.key"));
        assert!(c.policy.is_vault(".ssh"), "the default policy only grows");
        assert!(parse_config("[vault]\nphrase = \"curta\"\n").is_err());
        assert!(parse_config("[library]\nroots = 3\n").is_err());
        assert!(parse_config("[library]\nopen_in = \"sala\"\n").is_err());
        assert!(parse_config("not toml = = =").is_err());
    }

    #[test]
    fn breadcrumb_and_its_ellipsis() {
        assert_eq!(crumb(&["projeto", "src", "lib"], 48), "projeto › src › lib");
        let long = [
            "projeto",
            "aaaaaaaaaa",
            "bbbbbbbbbb",
            "cccccccccc",
            "dddddddddd",
            "fim",
        ];
        let c = crumb(&long, 48);
        assert!(c.chars().count() <= 48, "{c}");
        assert!(c.starts_with("projeto › … › "), "{c}");
        assert!(c.ends_with("dddddddddd › fim"), "{c}");
        let huge = ["p", &"x".repeat(80)];
        let c = crumb(&huge, 48);
        assert_eq!(c.chars().count(), 48);
        assert!(c.contains('…'));
        assert_eq!(root_label("~"), "~");
        assert_eq!(root_label("/srv/demo/"), "demo");
        let mut l = Library::new(Config::default());
        assert_eq!(l.breadcrumb(), "");
        l.open(0, "", &fs()).unwrap();
        assert_eq!(l.breadcrumb(), "~");
        l.open(0, "projeto/src", &fs()).unwrap();
        assert_eq!(l.breadcrumb(), "projeto › src");
    }

    #[test]
    fn cursor_moves_within_the_boards_and_up_returns_to_the_folder() {
        let mut m = MemFs::new(1);
        for i in 0..28 {
            m.file(0, &format!("d/f{i:02}"), 1);
        }
        m.dir(0, "d/sub");
        let fs = m;
        let mut l = Library::new(Config::default());
        assert!(!l.step(Dir::Right), "no shelf");
        let s = l.open(0, "d", &fs).unwrap();
        assert_eq!(s.entries.len(), 29);
        assert_eq!(s.cols(), MIN_COLS);
        assert_eq!(s.rows(), 3);
        assert_eq!(l.cursor(), (0, 0));
        assert_eq!(l.selected().unwrap().name(), "sub");
        assert!(!l.step(Dir::Left));
        assert!(!l.step(Dir::Up));
        for _ in 0..11 {
            assert!(l.step(Dir::Right));
        }
        assert!(!l.step(Dir::Right), "12 per board");
        assert!(l.step(Dir::Down));
        assert!(l.step(Dir::Down), "onto the short last board");
        assert!(!l.step(Dir::Down));
        assert_eq!(l.cursor(), (2, 4), "clamped to its last book");
        assert_eq!(l.selected().unwrap().name(), "f27");
        assert!(l.select(1));
        assert_eq!(l.cursor(), (0, 1));
        assert!(!l.select(99));
        // into "sub" and back: the cursor lands on "sub"
        l.select(0);
        l.open(0, "d/sub", &fs).unwrap();
        assert_eq!(l.current().unwrap().entries.len(), 0);
        assert!(l.selected().is_none());
        let calls = fs.calls();
        let up = l.up(&fs).unwrap();
        assert_eq!(up.rel, "d");
        assert_eq!(fs.calls(), calls, "from the cache");
        assert_eq!(l.selected().unwrap().name(), "sub");
        let top = l.up(&fs).unwrap();
        assert_eq!(top.rel, "");
        assert_eq!(l.selected().unwrap().name(), "d");
        assert!(l.up(&fs).is_none(), "at the root");
    }

    #[test]
    fn the_shelf_cache_is_an_lru_of_32() {
        let mut m = MemFs::new(1);
        for i in 0..40 {
            m.dir(0, &format!("d{i:02}"));
        }
        let mut l = Library::new(Config::default());
        for i in 0..40 {
            l.open(0, &format!("d{i:02}"), &m).unwrap();
        }
        assert!(l.cached(0, "d39").is_some());
        assert!(l.cached(0, "d08").is_some());
        assert!(l.cached(0, "d07").is_none());
        l.clear();
        assert!(l.current().is_none());
        assert!(l.cached(0, "d39").is_none());
    }

    fn big() -> MemFs {
        let mut m = MemFs::new(1);
        for a in 0..50 {
            for b in 0..50 {
                for f in 0..20 {
                    m.file(0, &format!("a{a:02}/b{b:02}/arquivo{f:02}.txt"), 10);
                }
            }
        }
        m
    }

    #[test]
    fn search_ranks_stably_and_never_finds_the_vault() {
        let c = Config::default();
        let m = fs();
        let s = search_tree(&c, &m, 0, "", "lib", &mut || 0);
        assert_eq!(s.hits[0].rel, "projeto/src/lib.rs");
        assert!(!s.truncated);
        for q in ["id_demo", "known", "ssh", "env", "pem", "config"] {
            let s = search_tree(&c, &m, 0, "", q, &mut || 0);
            for h in &s.hits {
                assert!(!c.policy.is_vault(&h.rel), "{q}: {}", h.rel);
            }
        }
        assert!(
            search_tree(&c, &m, 0, ".ssh", "id", &mut || 0)
                .hits
                .is_empty()
        );
        assert!(search_tree(&c, &m, 0, "", "  ", &mut || 0).hits.is_empty());
        // ties broken by path: the same twice
        let a = search_tree(&c, &m, 0, "", "s", &mut || 0);
        let b = search_tree(&c, &m, 0, "", "s", &mut || 0);
        assert_eq!(a, b);
        // within the subtree, scored on the path below it
        let s = search_tree(&c, &m, 0, "projeto", "main", &mut || 0);
        assert_eq!(s.hits[0].rel, "projeto/src/main.rs");
        assert!(
            search_tree(&c, &m, 0, "fotos", "main", &mut || 0)
                .hits
                .is_empty()
        );
    }

    #[test]
    fn search_budgets_entries_depth_and_time() {
        let m = big();
        let c = Config::default();
        let s = search_tree(&c, &m, 0, "", "arquivo07", &mut || 0);
        assert_eq!(s.scanned, 50 + 50 * 50 + 50 * 50 * 20);
        assert!(!s.truncated);
        assert_eq!(s.hits.len(), HITS);
        let c2 = Config {
            search_entries: 1000,
            ..Config::default()
        };
        let s = search_tree(&c2, &m, 0, "", "arquivo", &mut || 0);
        assert_eq!(s.scanned, 1000);
        assert!(s.truncated);
        let c3 = Config {
            search_depth: 2,
            ..Config::default()
        };
        let s = search_tree(&c3, &m, 0, "", "arquivo", &mut || 0);
        assert!(s.hits.is_empty(), "files are at depth 3");
        assert!(s.truncated);
        // a fake clock: 1 ms per call, 50 ms budget
        let c4 = Config {
            search_ms: 50,
            ..Config::default()
        };
        let mut t = 0u32;
        let s = search_tree(&c4, &m, 0, "", "arquivo", &mut || {
            t += 1;
            t
        });
        assert!(s.truncated);
        assert!(s.scanned < 50 * 50 * 20, "{}", s.scanned);
    }

    #[test]
    fn library_search_runs_over_the_current_subtree() {
        let m = fs();
        let mut l = Library::new(Config::default());
        let s = l.search("praia", &m, &mut || 0);
        assert_eq!(s.hits[0].rel, "fotos/praia.jpg");
        l.open(0, "projeto", &m).unwrap();
        assert!(l.last_search().is_none(), "a new shelf forgets it");
        let s = l.search("praia", &m, &mut || 0);
        assert!(s.hits.is_empty());
        assert_eq!(l.last_search(), Some(&s));
        assert!(l.is_vault(".ssh/x"));
    }

    #[test]
    fn mem_fs_reads_names_and_bounded_text() {
        let m = fs();
        assert_eq!(m.read_text(0, "notas.md", 64).unwrap(), "# notas\nolá");
        assert_eq!(
            m.read_text(0, "notas.md", 11).unwrap(),
            "# notas\nol",
            "char boundary"
        );
        assert!(m.read_text(0, "fotos/praia.jpg", 64).is_none());
        assert!(m.read_dir(0, "notas.md").is_err());
        assert_eq!(m.dev(0, ""), Some(0));
        assert!(!m.is_empty());
        assert!(m.calls() > 0);
    }
}
