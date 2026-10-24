//! The library (plan §7, phase 4), owned by the objects plugin: a room
//! whose bookcases are directories and whose books are files — names and
//! metadata only, read lazily on a scan thread, bounded — with fuzzy
//! search on a budget, `xdg-open` into a screen placed by intent, the
//! editor, the lectern's preview and the copy of a path. The vault's cards
//! (addendum §5) are browsed here too, with the same keys, only while the
//! vault is open.
//!
//! - [`model`]: the pure model (`nkw_core::library`, S18);
//! - [`fs`]: the `std::fs` port; [`fixture`]: the in-memory demo and `big`;
//! - [`scan`]: the scan thread; [`layout`]: bookcases and books in the room;
//! - [`tex`]: text on quads (lectern, cards).
//!
//! Privacy (§7.5): nothing here leaves the process except an `xdg-open`
//! or an editor argv and a copied path the user asked for; the dump shows
//! entry names only under `--demo`; vault names never reach a tag, a
//! pickable, a dump, a log, a search or an intent.

pub mod fixture;
pub mod fs;
pub mod layout;
pub mod scan;
pub mod tex;

use nkw_core::Dir;
pub use nkw_core::library as model;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use glam::{Mat4, Quat, Vec3};
use nkw_core::persist::RoomKind;
use nkw_core::strip::RoomSlot;
use nkw_core::{BookId, Pick, Pose, RoomId, Time, Want};
use nkw_gl::draw::{DrawList, Instance, MeshId};
use serde_json::{Value, json};

use crate::ext::{Ctx, Pickable, Req, Tag, ToastKind};
use layout::Run;
use model::{Config, Entry, FsError, Library, Search, Shelf};
use scan::{Port, Request, Response, Scanner, Wake};
use tex::{Page, TextTex};

/// `BookId`s: a book is `root · ROOT_STRIDE + index`; a board
/// `BOARD_BASE + root · 10 + board`; a search hit `HIT_BASE + i`.
pub const ROOT_STRIDE: u32 = 100_000;
pub const HIT_BASE: u32 = 900_000;
pub const BOARD_BASE: u32 = 1_000_000;
/// The library room's name (`world.library`, S15).
pub const ROOM_NAME: &str = "biblioteca";
/// Name tags show within this: the browse stand is `STAND_M` (2.4 m) from
/// the case, so up to ≈ 2.7 m to a bottom-board book (the plan's 1.8 m was
/// for the old 1.5 m stand).
pub const TAG_M: f32 = 2.9;
/// The cursor's slide, seconds (≤ 200 ms).
pub const SLIDE_S: f64 = 0.16;
/// The librarian "brings" the search table this long.
pub const BRING_S: f64 = 1.5;
/// How far off-centre the selected book may be before the camera follows.
const FOLLOW_M: f32 = 0.9;
/// The lectern's quad and its raster.
const LECTERN_M: (f32, f32) = (0.42, 0.55);
const LECTERN_PX: (usize, usize) = (420, 550);
const CARDS_PX: (usize, usize) = (360, 400);

/// The phone's Biblioteca lists at most this many hits (S25).
pub const MAX_PHONE_HITS: usize = 60;

/// One search hit as the phone's Biblioteca lists it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hit {
    /// what `world.open`/`world.book_edit`/`world.copy` take
    pub path: String,
    pub name: String,
    pub is_dir: bool,
    /// the folder it is in (relative to its root)
    pub folder: String,
}

/// What `h/l/j/k`, `enter` … act on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Browse {
    Off,
    /// a wall's bookcase
    Shelf,
    /// the search table
    Search,
    /// the vault's cards (only while it is open)
    Vault,
}

/// Where the port comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    /// `--demo` or `inject objects {"library":{"fixture":…}}`
    Fixture(&'static str),
    /// the roots of `[library]` under the home
    Disk,
}

pub struct LibGlue {
    pub demo: bool,
    pub cfg: Config,
    pub lib: Library,
    /// the search table's hits, as a shelf of their own
    hits: Library,
    /// the vault's cards: only while the vault is open
    cards: Option<Library>,
    source: Source,
    port: Option<Port>,
    /// the resolved roots on disk (empty for a fixture)
    roots: Vec<PathBuf>,
    home: Option<PathBuf>,
    scanner: Option<Scanner>,
    wake: Option<Wake>,
    pub browsing: Browse,
    /// the camera was in the library room at the last tick
    in_room: bool,
    /// the last room you were in before the library (`open_in = "origin"`)
    origin: Option<RoomId>,
    /// requests made to the port (the "no scan at rest" test)
    pub reads: usize,
    /// when the cursor last moved (the slide)
    moved_at: Time,
    /// the search table: where, since when, how many brought
    table: Option<(Run, Time)>,
    searching: bool,
    lectern: Option<(String, Pose)>,
    lectern_tex: TextTex,
    cards_tex: TextTex,
    /// where the cards are drawn: the vault's front and yaw
    pub cards_at: Option<(Vec3, f32)>,
    /// the last file opened (rel under `--demo`, else "…")
    pub last_open: Option<String>,
    /// the last first-shelf / search times, ms (the phase-4 numbers)
    pub timings: BTreeMap<&'static str, f64>,
    now: Time,
}

fn toast(text: impl Into<String>) -> Req {
    Req::Toast {
        text: text.into(),
        kind: ToastKind::Info,
        ms: 1800,
    }
}

fn redraw() -> Req {
    Req::Wish(nkw_core::Wish::world(Want::Once))
}

impl LibGlue {
    /// `demo`: the fixture library, never the disk; `cfg`: `[library]` +
    /// `[vault]`; `home`: where `~` is (ignored under `--demo`).
    pub fn new(demo: bool, cfg: Config, home: Option<PathBuf>) -> LibGlue {
        let source = if demo {
            Source::Fixture("demo")
        } else {
            Source::Disk
        };
        LibGlue {
            demo,
            lib: Library::new(cfg.clone()),
            hits: Library::new(cfg.clone()),
            cards: None,
            source,
            port: None,
            roots: Vec::new(),
            home: if demo { None } else { home },
            scanner: None,
            wake: None,
            browsing: Browse::Off,
            in_room: false,
            origin: None,
            reads: 0,
            moved_at: f64::NEG_INFINITY,
            table: None,
            searching: false,
            lectern: None,
            lectern_tex: TextTex::default(),
            cards_tex: TextTex::default(),
            cards_at: None,
            last_open: None,
            timings: BTreeMap::new(),
            now: 0.0,
            cfg,
        }
    }

    /// The scan thread wakes the loop through this (from `Plugin::start`).
    pub fn set_wake(&mut self, w: Wake) {
        self.wake = Some(w);
    }

    /// A fixture port by name (`inject objects {"library":{"fixture":…}}`):
    /// the library starts over on it.
    pub fn use_fixture(&mut self, name: &str) -> Result<(), String> {
        let (n, port): (&'static str, Port) = match name {
            "demo" => ("demo", Arc::new(fixture::demo())),
            "big" => ("big", Arc::new(fixture::big())),
            _ => return Err(format!("library: fixture desconhecida {name:?}")),
        };
        self.source = Source::Fixture(n);
        self.port = Some(port);
        self.scanner = None;
        self.lib.clear();
        self.hits.clear();
        self.table = None;
        self.browsing = Browse::Off;
        self.in_room = false;
        Ok(())
    }

    /// A port given by a test (a `MemFs` with vault entries …).
    pub fn use_port(&mut self, port: Port) {
        self.source = Source::Fixture("test");
        self.port = Some(port);
        self.scanner = None;
        self.lib.clear();
    }

    fn port(&mut self) -> Port {
        if let Some(p) = &self.port {
            return p.clone();
        }
        let p: Port = match self.source {
            Source::Fixture(_) => Arc::new(fixture::demo()),
            Source::Disk => {
                self.roots = fs::resolve_roots(&self.cfg.roots, self.home.as_deref());
                Arc::new(fs::StdFs::new(self.roots.clone()))
            }
        };
        self.port = Some(p.clone());
        p
    }

    fn request(&mut self, r: Request) {
        self.reads += 1;
        if self.scanner.is_none() {
            let port = self.port();
            let cfg = self.cfg.clone();
            self.scanner = Some(match &self.wake {
                Some(w) => Scanner::thread(port, cfg, w.clone()),
                None => Scanner::inline(port, cfg),
            });
        }
        if let Some(s) = &mut self.scanner {
            s.request(r);
        }
    }

    /// The library room: index, id and slot. The `RoomKind::Library` room
    /// (`World::ensure_room`, S15), or a room by the library's name (a world
    /// whose kind was not recorded).
    pub fn room(ctx: &Ctx) -> Option<(usize, RoomId, RoomSlot)> {
        let w = ctx.world;
        let i = w
            .rooms
            .iter()
            .position(|r| w.room_kind(r.id) == RoomKind::Library)
            .or_else(|| w.rooms.iter().position(|r| r.name == ROOM_NAME))?;
        Some((i, w.rooms[i].id, *w.slots().get(i)?))
    }

    /// The roots that have a wall in the room.
    fn walls(&self) -> usize {
        self.cfg.roots.len().clamp(1, 2)
    }

    /// The shelf shown on root `root`'s wall.
    fn wall_shelf(&self, root: usize) -> Option<&Shelf> {
        match self.lib.current() {
            Some(s) if s.root == root => Some(s),
            _ => self.lib.cached(root, ""),
        }
    }

    /// The library being browsed.
    fn active(&self) -> Option<&Library> {
        match self.browsing {
            Browse::Shelf => Some(&self.lib),
            Browse::Search => Some(&self.hits),
            Browse::Vault => self.cards.as_ref(),
            Browse::Off => None,
        }
    }

    fn active_mut(&mut self) -> Option<&mut Library> {
        match self.browsing {
            Browse::Shelf => Some(&mut self.lib),
            Browse::Search => Some(&mut self.hits),
            Browse::Vault => self.cards.as_mut(),
            Browse::Off => None,
        }
    }

    /// `Plugin::submode`: "library" while browsing.
    pub fn submode(&self) -> Option<&'static str> {
        (self.browsing != Browse::Off).then_some("library")
    }

    // ------------------------------------------------------------ hooks

    /// The scan thread's answers.
    pub fn drain(&mut self, ctx: &Ctx) -> Vec<Req> {
        let Some(s) = &mut self.scanner else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for r in s.poll() {
            out.extend(self.on_response(r, ctx));
        }
        out
    }

    fn on_response(&mut self, r: Response, ctx: &Ctx) -> Vec<Req> {
        let mut out = vec![redraw()];
        match r {
            Response::Shelf {
                root,
                rel,
                current,
                result,
                ms,
            } => match result {
                Ok(s) => {
                    self.timings.insert(
                        if rel.is_empty() {
                            "first_shelf_ms"
                        } else {
                            "shelf_ms"
                        },
                        ms,
                    );
                    if current {
                        self.lib.show(s);
                        self.moved_at = ctx.now;
                        if self.browsing == Browse::Off
                            && self.in_room
                            && ctx.world.walking.is_none()
                        {
                            self.browsing = Browse::Shelf;
                        }
                    } else {
                        self.lib.remember(s);
                    }
                }
                Err(e) => {
                    let _ = (root, rel);
                    out.push(toast(match e {
                        FsError::NotFound => "essa pasta sumiu",
                        FsError::Denied => "essa pasta não abre",
                        FsError::Boundary => "fica em outro disco: não entro",
                        FsError::Io(_) => "não consegui ler a pasta",
                    }));
                }
            },
            Response::Search { search, ms } => {
                self.searching = false;
                self.timings.insert("search_ms", ms);
                out.extend(self.show_search(search, ctx));
            }
            Response::Vault { rel, result } => {
                // dropped if the vault locked meanwhile
                if let (Some(c), Ok(s)) = (&mut self.cards, result) {
                    let _ = rel;
                    c.show(s);
                    self.browsing = Browse::Vault;
                    self.refresh_cards();
                }
            }
            Response::Preview { rel, text } => match text {
                Some(t) => {
                    let name = rel.rsplit('/').next().unwrap_or("").to_string();
                    let cam = ctx.world.camera;
                    let fwd = cam.forward();
                    let flat = Vec3::new(fwd.x, 0.0, fwd.z).normalize_or_zero();
                    let at = cam.pos + flat * 0.9 - Vec3::Y * 0.12;
                    let pose = Pose::new(at, cam.yaw, -0.35);
                    let (cols, rows) = (44, 30);
                    self.lectern_tex.set(
                        Page {
                            title: name,
                            lines: tex::wrap(&t, cols, rows),
                            mark: None,
                            bg: tex::PAPER,
                        },
                        LECTERN_PX.0,
                        LECTERN_PX.1,
                    );
                    self.lectern = Some((rel, pose));
                }
                None => out.push(toast("sem prévia")),
            },
        }
        out
    }

    fn show_search(&mut self, s: Search, ctx: &Ctx) -> Vec<Req> {
        let mut out = Vec::new();
        if s.truncated {
            out.push(toast("busca parou no orçamento"));
        }
        if s.hits.is_empty() {
            out.push(toast("nada encontrado"));
            self.lib.set_search(Some(s));
            return out;
        }
        let n = s.hits.len();
        let (root, rel) = self
            .lib
            .current()
            .map(|c| (c.root, c.rel.clone()))
            .unwrap_or((0, String::new()));
        self.hits.show(Shelf {
            root,
            rel,
            entries: s.hits.clone(),
            more: 0,
            vault_count: 0,
        });
        self.lib.set_search(Some(s));
        // the table stands before you
        let cam = ctx.world.camera;
        let f = Vec3::new(cam.forward().x, 0.0, cam.forward().z).normalize_or_zero();
        let centre = Vec3::new(cam.pos.x, 0.0, cam.pos.z) + f * 1.3;
        let normal = -f;
        let run = Run {
            centre,
            normal,
            along: Vec3::Y.cross(normal),
            len: 2.0 * layout::UNIT_W,
        };
        let at = if ctx.reduced_motion {
            ctx.now
        } else {
            ctx.now + BRING_S
        };
        self.table = Some((run, at));
        self.browsing = Browse::Search;
        let _ = n;
        out
    }

    /// Every frame (after `drain`): arriving and leaving the room.
    pub fn tick(&mut self, ctx: &Ctx) -> Vec<Req> {
        self.now = ctx.now;
        let mut out = Vec::new();
        let room = Self::room(ctx);
        let here = room.is_some_and(|(_, id, _)| ctx.world.current_room_id() == id);
        if !here {
            let cur = ctx.world.current_room_id();
            if room.is_none_or(|(_, id, _)| id != cur) {
                self.origin = Some(cur);
            }
        }
        if here && !self.in_room {
            self.in_room = true;
            out.extend(self.arrive(ctx, room.expect("here").2));
        } else if !here && self.in_room {
            self.in_room = false;
            self.leave();
            out.push(redraw());
        }
        if self.browsing != Browse::Off && ctx.world.mode().overlay.is_some() {
            // FOCO/SALTO/… on top: the keys are theirs; browsing resumes after
            let _ = ();
        }
        out
    }

    fn arrive(&mut self, ctx: &Ctx, slot: RoomSlot) -> Vec<Req> {
        let mut out = Vec::new();
        for root in (0..self.walls()).rev() {
            self.request(Request::Open {
                root,
                rel: String::new(),
                current: root == 0,
            });
        }
        // in NAV the camera goes to the bookcase and you browse it
        if ctx.world.walking.is_none()
            && let Some(r) = layout::run(&slot, 0)
        {
            // a middle board: the whole height of the case is in the frame
            out.push(Req::GlideTo(stand_at(&r, layout::board_centre(&r, 2))));
        }
        out
    }

    fn leave(&mut self) {
        self.browsing = Browse::Off;
        self.lib.clear();
        self.hits.clear();
        self.table = None;
        self.lectern = None;
        self.lectern_tex.clear();
    }

    /// Wants: the slide, the search table's arrival.
    pub fn wants(&self, now: Time, reduced: bool) -> Vec<Want> {
        let mut v = Vec::new();
        if !reduced && now - self.moved_at < SLIDE_S {
            v.push(Want::Continuous);
        }
        if let Some((_, at)) = self.table
            && at > now
        {
            v.push(Want::At(at));
        }
        v
    }

    // ------------------------------------------------------------ vault

    /// The vault just opened: read its cards (names only).
    pub fn on_unlock(&mut self, at: Option<(Vec3, f32)>) {
        self.cards = Some(Library::new(self.cfg.clone()));
        self.cards_at = at;
        let root = self.lib.current().map(|s| s.root).unwrap_or(0);
        self.request(Request::Vault {
            root,
            rel: String::new(),
        });
    }

    /// The vault locked: the cards leave memory.
    pub fn on_lock(&mut self) {
        self.cards = None;
        self.cards_tex.clear();
        if self.browsing == Browse::Vault {
            self.browsing = Browse::Off;
        }
    }

    /// The last search's hits for the phone's Biblioteca (`None`: nothing
    /// searched yet). The search never enters the vault; a vault path is
    /// dropped here once more all the same.
    pub fn phone_hits(&self) -> Option<Vec<Hit>> {
        let s = self.lib.last_search()?;
        let root = self.hits.current().map(|c| c.root).unwrap_or(0);
        Some(
            s.hits
                .iter()
                .filter(|e| !self.cfg.policy.is_vault(&e.rel))
                .take(MAX_PHONE_HITS)
                .map(|e| Hit {
                    path: self.abs(root, &e.rel),
                    name: e.name().to_string(),
                    is_dir: e.is_dir,
                    folder: parent_of(&e.rel),
                })
                .collect(),
        )
    }

    /// A search is running (the phone says "procurando…").
    pub fn searching(&self) -> bool {
        self.searching
    }

    pub fn cards(&self) -> usize {
        self.cards
            .as_ref()
            .and_then(|c| c.current())
            .map(|s| s.entries.len())
            .unwrap_or(0)
    }

    fn refresh_cards(&mut self) {
        let Some(c) = &self.cards else { return };
        let Some(s) = c.current() else { return };
        let lines: Vec<String> = s
            .entries
            .iter()
            .map(|e| {
                let kind = if e.is_dir { "pasta" } else { "arquivo" };
                format!("{}  · {kind} · {}", e.name(), size_label(e.size))
            })
            .collect();
        let mark = c.index();
        self.cards_tex.set(
            Page {
                title: "cofre".into(),
                lines,
                mark,
                bg: tex::CARD,
            },
            CARDS_PX.0,
            CARDS_PX.1,
        );
    }

    // ------------------------------------------------------------ commands

    /// The library's commands (S19). `vault_touch` is set when the vault's
    /// auto-lock should renew (a card read, a path copied).
    pub fn command(
        &mut self,
        name: &str,
        args: &BTreeMap<String, String>,
        ctx: &Ctx,
        vault_touch: &mut bool,
    ) -> Option<Vec<Req>> {
        let browsing = self.browsing != Browse::Off;
        let dir = match name {
            "world.book_prev" => Some(Dir::Left),
            "world.book_next" => Some(Dir::Right),
            "world.shelf_up" => Some(Dir::Up),
            "world.shelf_down" => Some(Dir::Down),
            _ => None,
        };
        if let Some(d) = dir {
            if !browsing {
                return None;
            }
            *vault_touch |= self.browsing == Browse::Vault;
            return Some(self.step(d, ctx));
        }
        Some(match name {
            "world.book_open" if browsing => {
                *vault_touch |= self.browsing == Browse::Vault;
                self.open_selected(ctx)
            }
            "world.shelf_back" if browsing => {
                *vault_touch |= self.browsing == Browse::Vault;
                self.back()
            }
            "world.library_leave" if browsing => {
                self.lectern = None;
                self.lectern_tex.clear();
                self.browsing = match self.browsing {
                    Browse::Search => {
                        self.table = None;
                        Browse::Shelf
                    }
                    _ => Browse::Off,
                };
                vec![redraw()]
            }
            "world.library_search" => {
                let q = args.get("q").map(|s| s.trim()).unwrap_or("");
                if q.is_empty() {
                    vec![toast("busca: diga o que procurar")]
                } else if !self.in_room && Self::room(ctx).is_none() {
                    vec![toast("a busca é na biblioteca (b)")]
                } else {
                    let (root, rel) = self
                        .lib
                        .current()
                        .map(|c| (c.root, c.rel.clone()))
                        .unwrap_or((0, String::new()));
                    self.searching = true;
                    self.request(Request::Search {
                        root,
                        rel,
                        q: q.to_string(),
                    });
                    vec![]
                }
            }
            // the phone's Biblioteca ("e" on a hit): by path, browsing or not
            "world.book_edit" if args.contains_key("path") => {
                let p = args.get("path").map(|s| s.trim()).unwrap_or("");
                if self.path_is_vault(p) {
                    vec![toast("esse caminho fica no cofre")]
                } else {
                    match self.locate(p) {
                        Some((root, rel)) => self.edit(root, &rel),
                        None => vec![toast("fora da biblioteca")],
                    }
                }
            }
            "world.book_edit" if browsing => {
                *vault_touch |= self.browsing == Browse::Vault;
                match self.selected_path() {
                    Some((root, rel, false)) => self.edit(root, &rel),
                    Some(_) => vec![toast("é uma pasta")],
                    None => vec![],
                }
            }
            "world.book_preview" if browsing => match (self.browsing, self.selected_path()) {
                (Browse::Vault, _) => vec![toast("no cofre não há prévia")],
                (_, Some((root, rel, false))) => {
                    self.request(Request::Preview { root, rel });
                    vec![]
                }
                (_, Some(_)) => vec![toast("é uma pasta")],
                _ => vec![],
            },
            "world.book_copy_path" if browsing => {
                *vault_touch |= self.browsing == Browse::Vault;
                match self.selected_path() {
                    Some((root, rel, _)) => vec![Req::Command {
                        name: "world.copy".into(),
                        args: BTreeMap::from([("text".into(), self.abs(root, &rel))]),
                    }],
                    None => vec![],
                }
            }
            "world.book_give" if browsing => vec![toast(if self.browsing == Browse::Vault {
                "do cofre, nada vai para um gato"
            } else {
                "entregar ao gato: fase 4b"
            })],
            "world.open" => {
                let p = args.get("path").map(String::as_str).unwrap_or("");
                self.open_path(p, ctx)
            }
            _ => return None,
        })
    }

    fn step(&mut self, d: Dir, ctx: &Ctx) -> Vec<Req> {
        let moved = self.active_mut().is_some_and(|l| l.step(d));
        if !moved {
            return Vec::new();
        }
        self.moved_at = ctx.now;
        let mut out = vec![redraw()];
        match self.browsing {
            Browse::Vault => self.refresh_cards(),
            Browse::Shelf => out.extend(self.follow(ctx)),
            _ => {}
        }
        out
    }

    /// The camera keeps the selected book in view (a glide, cut under
    /// reduced motion by the hub).
    fn follow(&self, ctx: &Ctx) -> Vec<Req> {
        let Some((_, _, slot)) = Self::room(ctx) else {
            return Vec::new();
        };
        let Some(s) = self.lib.current() else {
            return Vec::new();
        };
        let Some(r) = layout::run(&slot, s.root) else {
            return Vec::new();
        };
        let Some(i) = self.lib.index() else {
            return Vec::new();
        };
        let Some(b) = layout::books(s, &r, None, 0.0)
            .into_iter()
            .find(|b| b.index == i)
        else {
            return Vec::new();
        };
        let cam = ctx.world.camera;
        let off = (b.centre - cam.pos).dot(r.along).abs();
        let framed = stand_at(&r, b.centre);
        // another board → another pitch: reframe so it stays mid-frame
        if off > FOLLOW_M || (framed.pitch - cam.pitch).abs() > 0.05 {
            return vec![Req::GlideTo(framed)];
        }
        Vec::new()
    }

    fn back(&mut self) -> Vec<Req> {
        match self.browsing {
            Browse::Search => {
                self.browsing = Browse::Shelf;
                self.table = None;
                vec![redraw()]
            }
            Browse::Vault => {
                let Some(rel) = self
                    .cards
                    .as_ref()
                    .and_then(|c| c.current())
                    .map(|s| s.rel.clone())
                else {
                    return vec![];
                };
                if rel.is_empty() {
                    return vec![];
                }
                let p = parent_of(&rel);
                let p = if !p.is_empty() && self.cfg.policy.is_vault(&p) {
                    p
                } else {
                    String::new()
                };
                let root = self.cards_root();
                self.request(Request::Vault { root, rel: p });
                vec![]
            }
            Browse::Shelf => {
                let Some((root, rel)) = self.lib.current().map(|s| (s.root, s.rel.clone())) else {
                    return vec![];
                };
                if rel.is_empty() {
                    return vec![toast("já está na raiz")];
                }
                let p = parent_of(&rel);
                match self.lib.cached(root, &p).cloned() {
                    Some(s) => {
                        self.lib.show(s);
                        vec![redraw()]
                    }
                    None => {
                        self.request(Request::Open {
                            root,
                            rel: p,
                            current: true,
                        });
                        vec![]
                    }
                }
            }
            Browse::Off => vec![],
        }
    }

    fn cards_root(&self) -> usize {
        self.cards
            .as_ref()
            .and_then(|c| c.current())
            .map(|s| s.root)
            .unwrap_or(0)
    }

    /// (root, rel, is_dir) of the selected book of what you browse.
    fn selected_path(&self) -> Option<(usize, String, bool)> {
        let l = self.active()?;
        let e = l.selected()?;
        let root = l.current()?.root;
        Some((root, e.rel.clone(), e.is_dir))
    }

    fn open_selected(&mut self, ctx: &Ctx) -> Vec<Req> {
        let Some((root, rel, is_dir)) = self.selected_path() else {
            return vec![];
        };
        match (self.browsing, is_dir) {
            (Browse::Vault, true) => {
                self.request(Request::Vault { root, rel });
                vec![]
            }
            (Browse::Vault, false) => vec![toast("no cofre é só a lista")],
            (_, true) => {
                self.browsing = Browse::Shelf;
                self.table = None;
                self.request(Request::Open {
                    root,
                    rel,
                    current: true,
                });
                vec![]
            }
            (_, false) => self.open_file(root, &rel, ctx),
        }
    }

    /// The absolute path of a book: on disk, or the fixture's fiction.
    pub fn abs(&self, root: usize, rel: &str) -> String {
        match (self.source, self.roots.get(root)) {
            (Source::Disk, Some(r)) => r.join(model::norm(rel)).to_string_lossy().into_owned(),
            _ => format!("~/{}", model::norm(rel)),
        }
    }

    /// `enter` on a file: the placement intent, then `xdg-open` (never
    /// under `--demo`: the fixture's paths are fiction).
    fn open_file(&mut self, root: usize, rel: &str, ctx: &Ctx) -> Vec<Req> {
        if self.cfg.policy.is_vault(rel) {
            return vec![toast("esse caminho fica no cofre")];
        }
        let lib_room = Self::room(ctx).map(|(_, id, _)| id);
        let room = match self.cfg.open_in {
            model::OpenIn::Origin => self.origin.or(lib_room),
            model::OpenIn::Library => lib_room.or(self.origin),
        }
        .unwrap_or_else(|| ctx.world.current_room_id());
        let mut out = vec![Req::Command {
            name: "world.place_next".into(),
            args: BTreeMap::from([
                ("room".into(), room.0.to_string()),
                ("ttl_s".into(), "5".into()),
            ]),
        }];
        self.last_open = Some(if self.demo || matches!(self.source, Source::Fixture(_)) {
            model::norm(rel)
        } else {
            "…".into()
        });
        if self.demo || matches!(self.source, Source::Fixture(_)) {
            out.push(toast(format!(
                "abriria {}",
                rel.rsplit('/').next().unwrap_or("")
            )));
        } else {
            out.push(Req::Spawn(vec!["xdg-open".into(), self.abs(root, rel)]));
        }
        out
    }

    fn edit(&mut self, root: usize, rel: &str) -> Vec<Req> {
        if self.demo || matches!(self.source, Source::Fixture(_)) {
            return vec![toast("abriria no editor")];
        }
        let path = shell_quote(&self.abs(root, rel));
        vec![Req::Command {
            name: "world.terminal".into(),
            args: BTreeMap::from([("cmd".into(), format!("exec \"${{EDITOR:-vi}}\" {path}"))]),
        }]
    }

    /// `world.open {path}` (palette, Lua): vault paths are refused.
    fn open_path(&mut self, path: &str, ctx: &Ctx) -> Vec<Req> {
        let path = path.trim();
        if path.is_empty() {
            return vec![toast("world.open: falta o caminho")];
        }
        if self.path_is_vault(path) {
            return vec![toast("esse caminho fica no cofre")];
        }
        match self.locate(path) {
            Some((root, rel)) => self.open_file(root, &rel, ctx),
            None => {
                // outside the roots: still opened (it is the user's ask),
                // never under --demo
                if self.demo || matches!(self.source, Source::Fixture(_)) {
                    return vec![toast("fora da biblioteca")];
                }
                self.last_open = Some("…".into());
                let room = self.origin.unwrap_or_else(|| ctx.world.current_room_id());
                vec![
                    Req::Command {
                        name: "world.place_next".into(),
                        args: BTreeMap::from([
                            ("room".into(), room.0.to_string()),
                            ("ttl_s".into(), "5".into()),
                        ]),
                    },
                    Req::Spawn(vec!["xdg-open".into(), path.into()]),
                ]
            }
        }
    }

    /// Whether a path given by the user (absolute, `~/…` or relative to the
    /// roots) is a vault path: against every root, the home and as written.
    pub fn path_is_vault(&self, path: &str) -> bool {
        let pol = &self.cfg.policy;
        if pol.is_vault(path.trim_start_matches("~/")) {
            return true;
        }
        if let Some((_, rel)) = self.locate(path)
            && pol.is_vault(&rel)
        {
            return true;
        }
        if let Some(h) = &self.home
            && let Ok(rel) = Path::new(path).strip_prefix(h)
            && pol.is_vault(&rel.to_string_lossy())
        {
            return true;
        }
        false
    }

    /// (root, rel) of a path under the roots.
    fn locate(&self, path: &str) -> Option<(usize, String)> {
        if let Some(rel) = path.strip_prefix("~/") {
            return Some((0, model::norm(rel)));
        }
        if !path.starts_with('/') {
            return Some((0, model::norm(path)));
        }
        let roots = if self.roots.is_empty() {
            fs::resolve_roots(&self.cfg.roots, self.home.as_deref())
        } else {
            self.roots.clone()
        };
        roots.iter().enumerate().find_map(|(i, r)| {
            Path::new(path)
                .strip_prefix(r)
                .ok()
                .map(|rel| (i, model::norm(&rel.to_string_lossy())))
        })
    }

    /// `Effect::OpenBook` (a SALTO label): the cursor on that book (or
    /// board), browsing, the camera before it.
    pub fn open_book(&mut self, id: BookId, ctx: &Ctx) -> Vec<Req> {
        let id = id.0;
        if id >= BOARD_BASE {
            let root = ((id - BOARD_BASE) / 10) as usize;
            let board = ((id - BOARD_BASE) % 10) as usize;
            if self.lib.current().is_none_or(|s| s.root != root)
                && let Some(s) = self.lib.cached(root, "").cloned()
            {
                self.lib.show(s);
            }
            if let Some(s) = self.lib.current() {
                let i = board * s.cols();
                self.lib.select(i);
            }
            self.browsing = Browse::Shelf;
        } else if id >= HIT_BASE {
            self.hits.select((id - HIT_BASE) as usize);
            self.browsing = Browse::Search;
        } else {
            let root = (id / ROOT_STRIDE) as usize;
            let i = (id % ROOT_STRIDE) as usize;
            if self.lib.current().is_none_or(|s| s.root != root)
                && let Some(s) = self.lib.cached(root, "").cloned()
            {
                self.lib.show(s);
            }
            self.lib.select(i);
            self.browsing = Browse::Shelf;
        }
        self.moved_at = ctx.now;
        let mut out = vec![redraw()];
        if self.browsing == Browse::Shelf
            && let (Some((_, _, slot)), Some(s), Some(i)) =
                (Self::room(ctx), self.lib.current(), self.lib.index())
            && let Some(r) = layout::run(&slot, s.root)
            && let Some(b) = layout::books(s, &r, None, 0.0)
                .into_iter()
                .find(|b| b.index == i)
        {
            out.push(Req::GlideTo(stand_at(&r, b.centre)));
        }
        out
    }

    /// `world.interact` in the library: a bookcase in the aim → browse it.
    pub fn interact(&mut self, ctx: &Ctx) -> Option<Vec<Req>> {
        let root = self.aimed_run(ctx)?;
        if self.lib.current().is_none_or(|s| s.root != root) {
            match self.lib.cached(root, "").cloned() {
                Some(s) => {
                    self.lib.show(s);
                }
                None => self.request(Request::Open {
                    root,
                    rel: String::new(),
                    current: true,
                }),
            }
        }
        self.browsing = Browse::Shelf;
        self.moved_at = ctx.now;
        Some(vec![redraw()])
    }

    /// The root whose bookcase the camera's ray meets within reach.
    pub fn aimed_run(&self, ctx: &Ctx) -> Option<usize> {
        let (_, id, slot) = Self::room(ctx)?;
        if ctx.world.current_room_id() != id {
            return None;
        }
        let cam = ctx.world.camera;
        (0..self.walls())
            .filter_map(|root| {
                let r = layout::run(&slot, root)?;
                layout::hits_run(&r, cam.pos, cam.forward(), 4.0).map(|t| (t, root))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|x| x.1)
    }

    // ------------------------------------------------------------ drawing

    /// Bookcases, books, the "+N" box, the search table, the lectern, the
    /// cards (only for the library room in the culling set).
    pub fn draw(&self, ctx: &Ctx, list: &mut DrawList) {
        let cam = list.camera.pose.pos;
        if let Some((_, id, slot)) = Self::room(ctx)
            && ctx.visible_rooms.contains(&id)
        {
            for root in 0..self.walls() {
                let Some(r) = layout::run(&slot, root) else {
                    continue;
                };
                for m in layout::cases(&r) {
                    list.instances
                        .push(inst(MeshId::Shelf, m, [0xc8, 0x98, 0x68, 0xff], cam));
                }
                if let Some(s) = self.wall_shelf(root) {
                    let sel = (self.browsing == Browse::Shelf
                        && self.lib.current().is_some_and(|c| c.root == root))
                    .then(|| self.lib.index())
                    .flatten();
                    self.draw_books(s, &r, sel, ctx, cam, list);
                    if s.more > 0 {
                        let m = Mat4::from_rotation_translation(
                            Quat::from_rotation_y(layout::yaw_to(r.normal)),
                            layout::more_at(&r),
                        ) * Mat4::from_scale(Vec3::splat(0.7));
                        list.instances.push(inst(
                            MeshId::Package,
                            m,
                            [0xd8, 0xb8, 0x88, 0xff],
                            cam,
                        ));
                    }
                }
            }
        }
        if let Some((r, at)) = &self.table
            && ctx.now >= *at
        {
            for m in layout::cases(r) {
                list.instances
                    .push(inst(MeshId::Shelf, m, [0xb8, 0x88, 0x60, 0xff], cam));
            }
            if let Some(s) = self.hits.current() {
                let sel = (self.browsing == Browse::Search)
                    .then(|| self.hits.index())
                    .flatten();
                self.draw_books(s, r, sel, ctx, cam, list);
            }
        }
        if let Some((_, pose)) = &self.lectern {
            let m = tex::quad_model(pose.pos, pose.yaw, pose.pitch, LECTERN_M.0, LECTERN_M.1);
            if let Some(q) = self.lectern_tex.quad(m) {
                list.screens.push(q);
            }
            let stand = Mat4::from_translation(Vec3::new(pose.pos.x, 0.0, pose.pos.z))
                * Mat4::from_scale(Vec3::new(0.12, pose.pos.y - 0.2, 0.12));
            list.instances.push(inst(
                MeshId::RoundedBox,
                stand,
                [0x9a, 0x70, 0x50, 0xff],
                cam,
            ));
        }
        if let (Some((front, yaw)), true) = (self.cards_at, self.cards.is_some()) {
            let m = tex::quad_model(front, yaw, 0.0, 0.40, 0.44);
            if let Some(q) = self.cards_tex.quad(m) {
                list.screens.push(q);
            }
        }
    }

    fn draw_books(
        &self,
        s: &Shelf,
        r: &Run,
        sel: Option<usize>,
        ctx: &Ctx,
        cam: Vec3,
        list: &mut DrawList,
    ) {
        let pull = if ctx.reduced_motion {
            1.0
        } else {
            ((ctx.now - self.moved_at) / SLIDE_S).clamp(0.0, 1.0) as f32
        };
        for b in layout::books(s, r, sel, pull) {
            let c = b.spine.rgb;
            let lift = if sel == Some(b.index) { 0.15 } else { 0.0 };
            list.instances.push(Instance {
                emissive: lift,
                ..inst(MeshId::Book, b.model, [c[0], c[1], c[2], 0xff], cam)
            });
        }
    }

    /// # Safety
    /// `gl` is current on this thread.
    pub unsafe fn upload(&mut self, gl: &nkw_gl::glow::Context) {
        unsafe {
            self.lectern_tex.upload(gl);
            self.cards_tex.upload(gl);
        }
    }

    /// SALTO targets: every book on the walls, a board each, the hits.
    pub fn pickables(&self, ctx: &Ctx) -> Vec<Pickable> {
        let mut out = Vec::new();
        let Some((_, id, slot)) = Self::room(ctx) else {
            return out;
        };
        if !ctx.visible_rooms.contains(&id) {
            return out;
        }
        for root in 0..self.walls() {
            let (Some(r), Some(s)) = (layout::run(&slot, root), self.wall_shelf(root)) else {
                continue;
            };
            for b in layout::books(s, &r, None, 0.0) {
                out.push(Pickable {
                    pick: Pick::Book(BookId(root as u32 * ROOT_STRIDE + b.index as u32)),
                    pos: b.centre,
                    radius: 0.05,
                });
            }
            for board in 0..s.rows() {
                out.push(Pickable {
                    pick: Pick::Book(BookId(BOARD_BASE + root as u32 * 10 + board as u32)),
                    pos: layout::board_centre(&r, board + layout::first_board(s.rows())),
                    radius: 0.3,
                });
            }
        }
        if let (Some((r, at)), Some(s)) = (&self.table, self.hits.current())
            && ctx.now >= *at
        {
            for b in layout::books(s, r, None, 0.0) {
                out.push(Pickable {
                    pick: Pick::Book(BookId(HIT_BASE + b.index as u32)),
                    pos: b.centre,
                    radius: 0.05,
                });
            }
        }
        out
    }

    /// The breadcrumb plates, the selected book's name, the search
    /// table's note, the "+N" box. Never a vault name: the cards are not
    /// tagged.
    pub fn tags(&self, ctx: &Ctx) -> Vec<Tag> {
        let mut out = Vec::new();
        let Some((_, id, slot)) = Self::room(ctx) else {
            return out;
        };
        if !ctx.visible_rooms.contains(&id) {
            return out;
        }
        const CREAM: [u8; 3] = [0xf2, 0xe8, 0xd5];
        for root in 0..self.walls() {
            let (Some(r), Some(s)) = (layout::run(&slot, root), self.wall_shelf(root)) else {
                continue;
            };
            let crumb = if self.lib.current().is_some_and(|c| c.root == root) {
                self.lib.breadcrumb()
            } else {
                model::root_label(self.cfg.roots.get(root).map(String::as_str).unwrap_or("~"))
            };
            out.push(Tag {
                id: 0x11b0_0000 + root as u64,
                pos: layout::plate_at(&r),
                text: crumb,
                rgb: CREAM,
                max_m: 8.0,
            });
            if s.more > 0 {
                out.push(Tag {
                    id: 0x11b0_0010 + root as u64,
                    pos: layout::more_at(&r) + Vec3::Y * 0.25,
                    text: format!("+{}", s.more),
                    rgb: CREAM,
                    max_m: 4.0,
                });
            }
            if self.browsing == Browse::Shelf
                && self.lib.current().is_some_and(|c| c.root == root)
                && let (Some(i), Some(e)) = (self.lib.index(), self.lib.selected())
                && let Some(b) = layout::books(s, &r, None, 0.0)
                    .into_iter()
                    .find(|b| b.index == i)
            {
                out.push(Tag {
                    id: 0x11b0_0100,
                    pos: b.centre + r.normal * 0.06 + Vec3::Y * (b.spine.height / 2.0 + 0.04),
                    text: e.name().to_string(),
                    rgb: CREAM,
                    max_m: TAG_M,
                });
            }
        }
        if let Some((r, at)) = &self.table {
            if ctx.now < *at {
                out.push(Tag {
                    id: 0x11b0_0200,
                    pos: r.centre + Vec3::Y * 1.4,
                    text: format!(
                        "a bibliotecária traz {} livros",
                        self.hits.current().map(|s| s.entries.len()).unwrap_or(0)
                    ),
                    rgb: CREAM,
                    max_m: 6.0,
                });
            } else if self.browsing == Browse::Search
                && let (Some(i), Some(e), Some(s)) =
                    (self.hits.index(), self.hits.selected(), self.hits.current())
                && let Some(b) = layout::books(s, r, None, 0.0)
                    .into_iter()
                    .find(|b| b.index == i)
            {
                out.push(Tag {
                    id: 0x11b0_0101,
                    pos: b.centre + Vec3::Y * (b.spine.height / 2.0 + 0.04),
                    text: e.rel.clone(),
                    rgb: CREAM,
                    max_m: TAG_M,
                });
            }
        }
        out
    }

    /// `dump()["objects"]["library"]` (S18 shape): names only under
    /// `--demo`; never a vault path.
    pub fn dump(&self) -> Value {
        let demo = self.demo || matches!(self.source, Source::Fixture(_));
        let cur = self
            .active()
            .and_then(|l| l.current())
            .or(self.lib.current());
        let (rows, cols) = cur.map(|s| (s.rows(), s.cols())).unwrap_or((0, 0));
        let cursor = self
            .active()
            .or(Some(&self.lib))
            .map(|l| l.cursor())
            .unwrap_or((0, 0));
        let search = self.lib.last_search().map(|s| {
            json!({
                "hits": s.hits.len(),
                "scanned": s.scanned,
                "truncated": s.truncated,
                "names": if demo {
                    json!(s.hits.iter().map(|h| h.rel.clone()).collect::<Vec<_>>())
                } else {
                    Value::Null
                },
            })
        });
        let shelf = self.lib.current();
        json!({
            "source": match self.source {
                Source::Fixture(n) => format!("fixture:{n}"),
                Source::Disk => "disk".into(),
            },
            "root": shelf.map(|s| s.root),
            "rel": shelf.map(|s| if demo { json!(s.rel) } else { json!(model::normalize(&s.rel).len()) }),
            "rows": rows,
            "cols": cols,
            "cursor": [cursor.0, cursor.1],
            "more": shelf.map(|s| s.more).unwrap_or(0),
            "vault_count": shelf.map(|s| s.vault_count).unwrap_or(0),
            "entries": match shelf {
                Some(s) if demo && self.browsing != Browse::Vault => {
                    json!(s.entries.iter().map(|e| e.name().to_string()).collect::<Vec<_>>())
                }
                Some(s) => json!(s.entries.len()),
                None => Value::Null,
            },
            "selected": match (demo, self.browsing) {
                (true, Browse::Shelf | Browse::Search) => {
                    json!(self.active().and_then(|l| l.selected()).map(|e| e.name().to_string()))
                }
                _ => Value::Null,
            },
            "search": search,
            "browsing": self.browsing != Browse::Off,
            "mode": format!("{:?}", self.browsing).to_lowercase(),
            "searching": self.searching,
            "table": self.table.is_some(),
            "lectern": self.lectern.is_some(),
            "last_open": self.last_open,
            "reads": self.reads,
            "in_room": self.in_room,
            "timings": self.timings,
        })
    }
}

/// The stand before a book (wave 4's framing rule): `STAND_M` from the
/// bookcase at eye height, pitched so the centre of the board the book
/// stands on sits in the middle of the frame (pitch ≤ ±`STAND_PITCH`), so
/// the board's unit and half of each neighbour fit at 60° FOV.
pub fn stand_at(r: &Run, book: Vec3) -> Pose {
    let st = layout::stand(r, book);
    let board = layout::board_at(book.y);
    let target = layout::floors()[board] + layout::BOARD_MID;
    let horiz = layout::STAND_M - layout::BOARD_FRONT;
    let pitch = (target - st.pos.y)
        .atan2(horiz)
        .clamp(-layout::STAND_PITCH, layout::STAND_PITCH);
    Pose::new(st.pos, st.yaw, pitch)
}

fn parent_of(rel: &str) -> String {
    let segs = model::normalize(rel);
    segs[..segs.len().saturating_sub(1)].join("/")
}

fn size_label(n: u64) -> String {
    match n {
        0..1024 => format!("{n} B"),
        1024..1_048_576 => format!("{} KB", n / 1024),
        _ => format!("{} MB", n / 1_048_576),
    }
}

/// `'…'` with embedded quotes escaped (for `sh -c`).
pub fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

fn inst(mesh: MeshId, model: Mat4, tint: [u8; 4], cam: Vec3) -> Instance {
    let centre = model.transform_point3(Vec3::ZERO);
    Instance {
        mesh,
        model,
        tint,
        emissive: 0.0,
        fade: nkw_core::layout::fade_for(cam, centre),
    }
}

/// An entry's display name (tests, palette).
pub fn name_of(e: &Entry) -> &str {
    e.name()
}

#[cfg(test)]
mod tests;
