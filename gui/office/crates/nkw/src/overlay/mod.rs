//! The 2D overlay (§3.7–3.10, §5.4): HUD (mode label + badge chips),
//! toasts, the AC dialogue box drawn from a [`DialogView`], SALTO labels,
//! COMANDOS, name tags anchored in the world, and the map. Every piece is a
//! [`draw::Spec`] rasterised only when it changes ([`cache::Cache`]); the
//! overlay asks for a compose frame only when something of its own changed
//! (`Want::Once`), toasts end on `Want::At`, and only short slides use
//! `Want::Continuous` (none under reduced motion). No `Rate` wish, ever.

pub mod cache;
pub mod draw;
pub mod hud;
pub mod inject;
pub mod map;
pub mod project;

use std::collections::BTreeMap;

use glam::Vec2;
use nkw_core::hints::{ALPHABET, HintMatch, Hints};
use nkw_core::line::Line;
use nkw_core::palette::{Item, Palette, Step};
use nkw_core::{Action, Base, Dir, Effect, Kind, Pick, TalkPurpose, Time, Want, Wish};
use serde_json::Value;

use crate::ext::{
    Ctx, DialogView, HudInput, OverlayElem, PaletteItem, Pickable, Plugin, Req, Source, ToastKind,
};
use crate::router::{KeyEv, table};
use cache::Cache;
use draw::{Fonts, Row, Spec};
use hud::Toast;
use map::MapState;
use project::View;

/// Where the mode label sits, logical px from the top-left.
pub const LABEL_POS: (i32, i32) = (16, 16);
/// Element ids (stable per role).
pub const LABEL_ID: u64 = 1;
const CHIP_ID: u64 = 10;
const TOAST_ID: u64 = 100;
const DIALOG_ID: u64 = 200;
const CHOICES_ID: u64 = 201;
const PALETTE_ID: u64 = 300;
const MAP_ID: u64 = 400;
const RING_ID: u64 = 401;
const HINT_ID: u64 = 1 << 20;
const TAG_ID: u64 = 2 << 20;

/// The dialogue box rises 12 px while it appears.
const RISE_S: Time = 0.14;
const RISE_PX: f64 = 12.0;

/// SALTO while it is open.
struct Hinting {
    typed: String,
    hints: Hints,
    /// the targets of the last frame, by id
    picks: BTreeMap<u64, Pick>,
}

pub struct Overlay {
    fonts: Option<Fonts>,
    cache: Cache,
    hinting: Option<Hinting>,
    palette: Option<Palette>,
    /// the palette's args per item, (name, prompt)
    arg_specs: BTreeMap<String, Vec<(String, String)>>,
    /// items handed in (hub/inject): core, apps, "ir para" …
    extra_items: Vec<PaletteItem>,
    /// `world.launcher`: the next palette lists only apps
    apps_only: bool,
    /// the reply line of CONVERSA
    line: Line,
    line_open: bool,
    map: Option<MapState>,
    toasts: Vec<Toast>,
    injected_dialog: Option<DialogView>,
    extra_pickables: Vec<Pickable>,
    /// the last dialogue drawn (name, text): blips for what is new
    last_dialog: Option<(String, String)>,
    dialog_since: Option<Time>,
    blips: Vec<f32>,
    blips_total: u64,
    /// something of the overlay's changed: one compose frame
    dirty: bool,
    /// the ▼'s next blink, while a dialogue with more waits
    blink_at: Option<Time>,
    stats: Stats,
}

#[derive(Default)]
struct Stats {
    elements: usize,
    label: Option<&'static str>,
    badges: usize,
    tags: usize,
    hints: BTreeMap<u64, String>,
    dialog_chars: usize,
}

impl Overlay {
    pub fn new(cli: &crate::cli::Cli) -> Overlay {
        let _ = cli;
        Overlay {
            fonts: None,
            cache: Cache::new(),
            hinting: None,
            palette: None,
            arg_specs: BTreeMap::new(),
            extra_items: Vec::new(),
            apps_only: false,
            line: Line::default(),
            line_open: false,
            map: None,
            toasts: Vec::new(),
            injected_dialog: None,
            extra_pickables: Vec::new(),
            last_dialog: None,
            dialog_since: None,
            blips: Vec::new(),
            blips_total: 0,
            dirty: false,
            blink_at: None,
            stats: Stats::default(),
        }
    }

    /// The palette's items besides the world's own (the hub's Σ
    /// `palette_items`; see the seam proposal). Replaces the previous set.
    pub fn set_palette_items(&mut self, items: Vec<PaletteItem>) {
        self.extra_items = items;
    }

    /// `Route::Text`: a key typed into the overlay (palette, line, SALTO,
    /// the map).
    pub fn key(&mut self, ev: &KeyEv, ctx: &Ctx) -> Vec<Req> {
        if !ev.pressed {
            return Vec::new();
        }
        self.dirty = true;
        if self.map.is_some() {
            return self.map_key(ev, ctx);
        }
        let mode = ctx.world.mode();
        let text = ev
            .utf8
            .as_deref()
            .filter(|_| !ev.mods.ctrl && !ev.mods.alt && !ev.mods.logo);
        match mode.kind() {
            Some(Kind::Hint { .. }) => {
                let Some(t) = text else { return Vec::new() };
                self.hint_type(t)
            }
            Some(Kind::Palette) => {
                if let Some(p) = self.palette.as_mut() {
                    edit_key(ev, text, |f| p.edit(f));
                }
                Vec::new()
            }
            Some(Kind::Talk { line: true, .. }) => {
                self.sync_line(true);
                let l = &mut self.line;
                edit_key(ev, text, |f| f(l));
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    /// `Some("map")` while the map is open.
    pub fn submode(&self) -> Option<&'static str> {
        self.map.map(|_| "map")
    }

    pub fn toast(&mut self, text: &str, kind: ToastKind, ms: u32, now: Time) {
        hud::push(&mut self.toasts, Toast::new(text, kind, ms, now));
        self.dirty = true;
    }

    /// The hub fills `Ctx.line` from it on `SendLine`.
    pub fn take_line(&mut self) -> Option<String> {
        let t = std::mem::take(&mut self.line.text);
        self.line.cursor = 0;
        self.dirty = true;
        Some(t)
    }

    /// The reply line opened or closed (the machine says so through the
    /// mode): a fresh line each time it opens.
    fn sync_line(&mut self, open: bool) {
        if open && !self.line_open {
            self.line.clear();
        }
        self.line_open = open;
    }

    fn hint_type(&mut self, t: &str) -> Vec<Req> {
        let Some(h) = self.hinting.as_mut() else {
            return Vec::new();
        };
        h.typed.push_str(t);
        match h.hints.resolve(&h.typed) {
            HintMatch::Hit(id) => match h.picks.get(&id) {
                Some(p) => vec![Req::Action(Action::Pick(*p))],
                None => Vec::new(),
            },
            HintMatch::Partial => Vec::new(),
            HintMatch::None => {
                h.typed.clear();
                Vec::new()
            }
        }
    }

    fn map_targets(&self, ctx: &Ctx) -> Vec<map::Target> {
        let floor = self.map.is_some_and(|m| m.floor);
        map::layout(ctx.world, floor, View::of(ctx.world).size).1
    }

    fn open_map(&mut self, floor: bool, ctx: &Ctx) {
        let t = map::layout(ctx.world, floor, View::of(ctx.world).size).1;
        self.map = Some(MapState {
            floor,
            sel: map::initial(ctx.world, floor, &t),
        });
        self.dirty = true;
    }

    fn map_key(&mut self, ev: &KeyEv, ctx: &Ctx) -> Vec<Req> {
        let Some(m) = self.map else { return Vec::new() };
        let dir = match ev.name.as_str() {
            "h" | "left" => Some(Dir::Left),
            "l" | "right" => Some(Dir::Right),
            "k" | "up" => Some(Dir::Up),
            "j" | "down" => Some(Dir::Down),
            _ => None,
        };
        if let Some(d) = dir {
            let t = self.map_targets(ctx);
            self.map = Some(MapState {
                sel: map::step(&t, m.sel, d),
                ..m
            });
            return Vec::new();
        }
        match ev.name.as_str() {
            "o" => {
                self.open_map(!m.floor, ctx);
                Vec::new()
            }
            "esc" | "q" => {
                self.map = None;
                Vec::new()
            }
            "enter" | "space" => {
                let t = self.map_targets(ctx);
                self.map = None;
                t.get(m.sel)
                    .map(|t| vec![Req::GlideTo(t.go)])
                    .unwrap_or_default()
            }
            _ => Vec::new(),
        }
    }

    /// The palette's items: the world's commands (S9) + what was handed in.
    fn items(&mut self) -> Vec<Item> {
        let mut out: Vec<Item> = Vec::new();
        self.arg_specs.clear();
        if !self.apps_only {
            for (name, title) in table::TITLES {
                if !table::in_palette(name) {
                    continue;
                }
                let keys = table::keys(name).iter().map(|k| k.to_string()).collect();
                out.push(Item {
                    name: name.to_string(),
                    title: title.to_string(),
                    keys,
                    group: 0,
                });
                let args: Vec<(String, String)> = table::args(name)
                    .into_iter()
                    .map(|(a, p)| (a.to_string(), p.to_string()))
                    .collect();
                if !args.is_empty() {
                    self.arg_specs.insert(name.to_string(), args);
                }
            }
        }
        for it in &self.extra_items {
            let group = match it.source {
                Source::World => 0,
                Source::Core => 1,
                Source::App { .. } => 2,
                Source::Goto | Source::Object => 3,
            };
            if self.apps_only && group != 2 {
                continue;
            }
            out.retain(|o| o.name != it.name);
            out.push(Item {
                name: it.name.clone(),
                title: it.title.clone(),
                keys: it.keys.clone(),
                group,
            });
            let args: Vec<(String, String)> = it
                .args
                .iter()
                .filter(|a| a.required)
                .map(|a| (a.name.clone(), a.prompt.clone()))
                .collect();
            if !args.is_empty() {
                self.arg_specs.insert(it.name.clone(), args);
            }
        }
        out
    }

    fn palette_run(&mut self) -> Vec<Req> {
        let Some(p) = self.palette.as_mut() else {
            return Vec::new();
        };
        let specs = &self.arg_specs;
        match p.enter(&|n| specs.get(n).cloned().unwrap_or_default()) {
            Step::Run { name, args } => {
                self.palette = None;
                vec![
                    Req::Action(Action::PaletteDone),
                    Req::Command { name, args },
                ]
            }
            _ => Vec::new(),
        }
    }

    /// The line the editing commands act on: the palette's or the reply's.
    fn edit_line(&mut self, ctx: &Ctx, f: impl FnOnce(&mut Line)) -> bool {
        match ctx.world.mode().kind() {
            Some(Kind::Palette) => match self.palette.as_mut() {
                Some(p) => {
                    p.edit(f);
                    true
                }
                None => false,
            },
            Some(Kind::Talk { line: true, .. }) => {
                self.sync_line(true);
                f(&mut self.line);
                true
            }
            _ => false,
        }
    }

    /// The 2D elements of this frame, topmost first.
    pub fn elements(&mut self, ctx: &Ctx, hud: &HudInput) -> Vec<OverlayElem> {
        let view = View::of(ctx.world);
        let out = view.size;
        let now = ctx.now;
        let reduced = ctx.reduced_motion;
        let mut fonts = self.fonts.take().unwrap_or_default();
        let mut stats = Stats::default();
        let mut f = Frame {
            cache: &mut self.cache,
            fonts: &mut fonts,
            s: ctx.scale.round().max(1.0) as i32,
            out,
            focus: hud.focus_rect,
            els: Vec::new(),
        };

        // the map (a sub-view of NAV: it covers the world)
        if let Some(m) = self.map {
            let (spec, targets) = map::layout(ctx.world, m.floor, out);
            if let Some(t) = targets.get(m.sel) {
                let ring = Spec::Ring {
                    w: t.rect.2,
                    h: t.rect.3,
                };
                f.put(RING_ID, &ring, Placer::At(t.rect.0, t.rect.1));
            }
            f.put(MAP_ID, &spec, Placer::Raw(0, 0));
        }

        // COMANDOS
        if let Some(p) = &self.palette {
            let width = (out.x as i32 - 96).min(720);
            let rows: Vec<Row> = p
                .ranked()
                .iter()
                .map(|i| Row {
                    name: i.name.clone(),
                    title: i.title.clone(),
                    keys: i.keys.first().cloned().unwrap_or_default(),
                })
                .collect();
            let spec = Spec::Palette {
                width,
                prompt: p.prompt().map(String::from),
                query: p.line().text.clone(),
                cursor: p.line().cursor_chars(),
                rows,
                selected: p.cursor(),
            };
            f.put(
                PALETTE_ID,
                &spec,
                Placer::At((out.x as i32 - width) / 2, 72),
            );
        }

        // SALTO
        if let Some(h) = self.hinting.as_mut() {
            let mut seen: Vec<(u64, Vec2, f32)> = Vec::new();
            h.picks.clear();
            for p in hud.pickables.iter().chain(&self.extra_pickables) {
                let id = project::pick_id(&p.pick);
                if h.picks.contains_key(&id) {
                    continue;
                }
                if let Some(q) = view.project(p.pos) {
                    h.picks.insert(id, p.pick);
                    seen.push((id, q, view.from_centre(q)));
                }
            }
            let vis: Vec<(u64, f32)> = seen.iter().map(|t| (t.0, t.2)).collect();
            h.hints.update(&vis);
            seen.sort_by(|a, b| a.2.total_cmp(&b.2).then(a.0.cmp(&b.0)));
            let mut specs = Vec::new();
            for (id, q, _) in &seen {
                let Some(label) = h.hints.label(*id) else {
                    continue;
                };
                stats.hints.insert(*id, label.to_string());
                if !label.starts_with(h.typed.as_str()) {
                    continue;
                }
                let spec = Spec::Hint {
                    label: label.to_string(),
                    typed: h.typed.chars().count(),
                };
                let size = f.size(&spec);
                specs.push((*id, *q, size, spec));
            }
            let items: Vec<(u64, Vec2, (i32, i32))> =
                specs.iter().map(|t| (t.0, t.1, t.2)).collect();
            let placed = project::place_labels(&items, out);
            for (k, ((_, _, _, spec), (_, at))) in specs.iter().zip(placed).enumerate() {
                f.put(HINT_ID + k as u64, spec, Placer::Raw(at.0, at.1));
            }
        }

        // the dialogue
        let dialog = hud.dialog.clone().or_else(|| self.injected_dialog.clone());
        match &dialog {
            Some(d) => {
                if self.dialog_since.is_none() {
                    self.dialog_since = Some(now);
                }
                let before = match &self.last_dialog {
                    Some((n, t)) if n == &d.name => t.clone(),
                    _ => String::new(),
                };
                if before != d.text {
                    let b = hud::blips(&before, &d.text, &d.name);
                    self.blips_total += b.len() as u64;
                    self.blips.extend(b);
                }
                self.last_dialog = Some((d.name.clone(), d.text.clone()));
                stats.dialog_chars = d.text.chars().count();
                let rise = match self.dialog_since {
                    Some(t0) if !reduced && now - t0 < RISE_S => {
                        let p = ((now - t0) / RISE_S).clamp(0.0, 1.0);
                        (RISE_PX * (1.0 - p).powi(3)).round() as i32
                    }
                    _ => 0,
                };
                let width = (out.x as i32 - 128).min(760);
                let cw = f.fonts.face(draw::PX_DIALOG).cw as i32;
                let cols = ((width - 48) / cw.max(1)).max(10) as usize;
                let all = draw::wrap(&d.text, cols);
                let lines: Vec<String> = all
                    .iter()
                    .skip(all.len().saturating_sub(draw::DIALOG_LINES))
                    .cloned()
                    .collect();
                let mode = ctx.world.mode();
                let line = if matches!(mode.kind(), Some(Kind::Talk { line: true, .. })) {
                    let shown = if mode.purpose() == Some(TalkPurpose::VaultUnlock) {
                        "•".repeat(self.line.text.chars().count())
                    } else {
                        self.line.text.clone()
                    };
                    Some((shown, self.line.cursor_chars()))
                } else {
                    d.line.as_ref().map(|l| {
                        let c = l
                            .text
                            .get(..l.cursor.min(l.text.len()))
                            .map_or(0, |t| t.chars().count());
                        (l.text.clone(), c)
                    })
                };
                let waiting = d.more && d.choices.is_empty();
                let blink_on = reduced || ((now * 2.0).floor() as i64) % 2 == 0;
                self.blink_at = (waiting && !reduced).then(|| ((now * 2.0).floor() + 1.0) / 2.0);
                let spec = Spec::Dialog {
                    width,
                    name: d.name.clone(),
                    rgb: d.rgb,
                    lines,
                    arrow: waiting && blink_on,
                    line,
                };
                let x = (out.x as i32 - width) / 2;
                let size = f.size(&spec);
                let y = out.y as i32 - size.1 - 28 + rise;
                f.put(DIALOG_ID, &spec, Placer::Raw(x, y));
                if !d.choices.is_empty() {
                    let ch = Spec::Choices {
                        items: d.choices.clone(),
                        selected: d.selected,
                    };
                    let cs = f.size(&ch);
                    f.put(
                        CHOICES_ID,
                        &ch,
                        Placer::At(x + width - cs.0 - 24, y - cs.1 + 4),
                    );
                }
            }
            None => {
                self.last_dialog = None;
                self.dialog_since = None;
                self.blink_at = None;
            }
        }

        // toasts, bottom-left, the newest at the bottom
        let mut y = out.y as i32 - 16;
        for (i, t) in self.toasts.iter().enumerate().rev() {
            let spec = Spec::Toast {
                text: t.text.clone(),
                error: t.kind == ToastKind::Error,
            };
            let size = f.size(&spec);
            y -= size.1;
            f.put(
                TOAST_ID + i as u64,
                &spec,
                Placer::Raw(16 + t.offset(now, reduced), y),
            );
            y -= 8;
        }

        // HUD: the mode label (not in FOCO, not over the map), the badges
        let mut x = LABEL_POS.0;
        if f.focus.is_none() && self.map.is_none() {
            let spec = Spec::Label {
                text: hud.mode_label.to_string(),
            };
            let r = f.put(LABEL_ID, &spec, Placer::Raw(LABEL_POS.0, LABEL_POS.1));
            x += r.2 + 8;
            stats.label = Some(hud.mode_label);
        }
        for (i, b) in hud.badges.iter().enumerate() {
            let (text, bg, fg) = hud::chip(b);
            let spec = Spec::Chip { text, bg, fg };
            let w = f.size(&spec).0;
            let r = f.put(CHIP_ID + i as u64, &spec, Placer::Raw(x, LABEL_POS.1 + 2));
            if r.2 > 0 {
                stats.badges += 1;
            }
            x += w + 6;
        }

        // name tags (not in FOCO, not over the map)
        if f.focus.is_none() && self.map.is_none() {
            for (i, q, _) in project::visible_tags(&view, &hud.tags) {
                let t = &hud.tags[i];
                let spec = Spec::Tag {
                    text: t.text.clone(),
                    rgb: t.rgb,
                };
                let size = f.size(&spec);
                let at = (
                    q.x.round() as i32 - size.0 / 2,
                    q.y.round() as i32 - size.1 - 6,
                );
                f.put(TAG_ID + (t.id & 0xf_ffff), &spec, Placer::At(at.0, at.1));
                stats.tags += 1;
            }
        }

        let els = f.els;
        stats.elements = els.len();
        self.stats = stats;
        self.dirty = false;
        self.fonts = Some(fonts);
        els
    }
}

/// One frame's elements being built: the cache, the fonts, the output.
struct Frame<'a> {
    cache: &'a mut Cache,
    fonts: &'a mut Fonts,
    /// the integer buffer scale
    s: i32,
    /// logical px
    out: Vec2,
    /// FOCO: nothing may cover the window
    focus: Option<[i32; 4]>,
    els: Vec<OverlayElem>,
}

impl Frame<'_> {
    /// The logical size `spec` draws at (rasterised once, then cached).
    fn size(&mut self, spec: &Spec) -> (i32, i32) {
        self.cache.get(spec, self.s, self.fonts).1
    }

    /// Add `spec` as element `id`; returns its rect (w, h 0 when it would
    /// cover the FOCO window and was left out).
    fn put(&mut self, id: u64, spec: &Spec, at: Placer) -> (i32, i32, i32, i32) {
        let (buf, size) = self.cache.get(spec, self.s, self.fonts);
        let pos = at.place(size, self.out);
        if let Some(r) = self.focus
            && intersects(r, (pos.0, pos.1, size.0, size.1))
        {
            return (pos.0, pos.1, 0, 0);
        }
        self.els.push(OverlayElem { id, pos, buf });
        (pos.0, pos.1, size.0, size.1)
    }
}

/// How an element's top-left is chosen.
#[derive(Clone, Copy)]
enum Placer {
    /// clamped inside the output
    At(i32, i32),
    /// exactly there
    Raw(i32, i32),
}

impl Placer {
    fn place(self, size: (i32, i32), out: Vec2) -> (i32, i32) {
        match self {
            Placer::At(x, y) => project::clamp((x, y), size, out),
            Placer::Raw(x, y) => (x, y),
        }
    }
}

fn intersects(r: [i32; 4], b: (i32, i32, i32, i32)) -> bool {
    b.0 < r[0] + r[2] && r[0] < b.0 + b.2 && b.1 < r[1] + r[3] && r[1] < b.1 + b.3
}

/// A line-editing key from `Route::Text`: printable text is inserted, the
/// arrows/home/end/backspace/delete move or delete.
fn edit_key(ev: &KeyEv, text: Option<&str>, edit: impl FnOnce(&mut dyn FnMut(&mut Line))) {
    let name = ev.name.as_str();
    let mut f = |l: &mut Line| match (text, name) {
        (_, "backspace") => l.backspace(),
        (_, "delete") => {
            let c = l.cursor;
            l.right();
            if l.cursor != c {
                l.backspace();
            }
        }
        (_, "left") => l.left(),
        (_, "right") => l.right(),
        (_, "home") => l.home(),
        (_, "end") => l.end(),
        (Some(t), _) => l.insert(t),
        _ => {}
    };
    edit(&mut f);
}

impl Plugin for Overlay {
    fn name(&self) -> &'static str {
        "overlay"
    }

    fn tick(&mut self, ctx: &Ctx) -> Vec<Req> {
        let now = ctx.now;
        let n = self.toasts.len();
        self.toasts.retain(|t| !t.gone(now, ctx.reduced_motion));
        if self.toasts.len() != n {
            self.dirty = true;
        }
        let mode = ctx.world.mode();
        self.sync_line(matches!(mode.kind(), Some(Kind::Talk { line: true, .. })));
        let plain_nav = mode.overlay.is_none() && mode.base == Base::Nav;
        if self.map.is_some() && !plain_nav {
            self.map = None;
            self.dirty = true;
        }
        if self.hinting.is_some() && !matches!(mode.kind(), Some(Kind::Hint { .. })) {
            self.hinting = None;
            self.dirty = true;
        }
        if self.palette.is_some() && mode.kind() != Some(Kind::Palette) {
            self.palette = None;
            self.dirty = true;
        }
        self.blips
            .drain(..)
            .map(|pitch| Req::Cue {
                cue: nkw_objects::sound::Cue::Blip { pitch },
                at: None,
            })
            .collect()
    }

    fn wishes(&self, ctx: &Ctx) -> Vec<Wish> {
        let now = ctx.now;
        let reduced = ctx.reduced_motion;
        let mut w = Vec::new();
        if self.dirty {
            w.push(Wish::compose(Want::Once));
        }
        for t in &self.toasts {
            if t.moving(now, reduced) {
                w.push(Wish::compose(Want::Continuous));
            } else if now < t.until {
                w.push(Wish::compose(Want::At(t.until)));
            } else {
                w.push(Wish::compose(Want::At(t.until + hud::TOAST_OUT)));
            }
        }
        if let Some(t0) = self.dialog_since
            && !reduced
            && now - t0 < RISE_S
        {
            w.push(Wish::compose(Want::Continuous));
        }
        if let Some(t) = self.blink_at {
            w.push(Wish::compose(Want::At(t)));
        }
        w
    }

    fn command(
        &mut self,
        name: &str,
        args: &BTreeMap<String, String>,
        ctx: &Ctx,
    ) -> Option<Vec<Req>> {
        let _ = args;
        self.dirty = true;
        let act = |a: Action| Some(vec![Req::Action(a)]);
        match name {
            "world.hint" => act(Action::Hint { focus: false }),
            "world.hint_focus" => act(Action::Hint { focus: true }),
            "world.palette" => {
                self.apps_only = false;
                act(Action::Palette)
            }
            "world.launcher" => {
                self.apps_only = true;
                act(Action::Palette)
            }
            "world.hint_cancel"
            | "world.palette_cancel"
            | "world.line_cancel"
            | "world.talk_end" => act(Action::Back),
            "world.hint_backspace" => {
                if let Some(h) = self.hinting.as_mut() {
                    h.typed.pop();
                }
                Some(Vec::new())
            }
            "world.palette_run" => Some(self.palette_run()),
            "world.palette_complete" => {
                if let Some(p) = self.palette.as_mut() {
                    p.complete();
                }
                Some(Vec::new())
            }
            "world.palette_prev" => {
                if let Some(p) = self.palette.as_mut() {
                    p.up();
                }
                Some(Vec::new())
            }
            "world.palette_next" => {
                if let Some(p) = self.palette.as_mut() {
                    p.down();
                }
                Some(Vec::new())
            }
            "world.line_home" => self.edit_line(ctx, Line::home).then(Vec::new),
            "world.line_end" => self.edit_line(ctx, Line::end).then(Vec::new),
            "world.line_delete_word" => self.edit_line(ctx, Line::delete_word).then(Vec::new),
            "world.line_clear" => self.edit_line(ctx, Line::clear).then(Vec::new),
            "world.line_newline" => self.edit_line(ctx, |l| l.insert("\n")).then(Vec::new),
            "world.line_send" => act(Action::Send),
            "world.talk_next" => act(Action::TalkNext),
            "world.talk_reply" => act(Action::Reply),
            "world.overview" => {
                let m = ctx.world.mode();
                match self.map {
                    Some(s) => self.open_map(!s.floor, ctx),
                    None if m.overlay.is_none() && m.base == Base::Nav => self.open_map(false, ctx),
                    None => {}
                }
                Some(Vec::new())
            }
            "world.library" => {
                self.toast("a biblioteca chega na fase 4", ToastKind::Info, 0, ctx.now);
                Some(Vec::new())
            }
            "world.grab" | "world.nearer" | "world.farther" => {
                self.toast("pegar a tela chega na fase 3", ToastKind::Info, 0, ctx.now);
                Some(Vec::new())
            }
            _ => {
                self.dirty = false;
                None
            }
        }
    }

    fn effect(&mut self, e: &Effect, ctx: &Ctx) -> Vec<Req> {
        let _ = ctx;
        self.dirty = true;
        match e {
            Effect::ShowHints { .. } => {
                self.hinting = Some(Hinting {
                    typed: String::new(),
                    hints: Hints::new(ALPHABET),
                    picks: BTreeMap::new(),
                });
            }
            Effect::HideHints => self.hinting = None,
            Effect::OpenPalette => {
                let items = self.items();
                self.palette = Some(Palette::new(items, ""));
            }
            Effect::ClosePalette => {
                self.palette = None;
                self.apps_only = false;
            }
            _ => {}
        }
        Vec::new()
    }

    fn inject(&mut self, json: &Value, ctx: &Ctx) -> Vec<Req> {
        self.dirty = true;
        if let Some(d) = json.get("dialog") {
            self.injected_dialog = inject::dialog(d);
        }
        if let Some((text, kind, ms)) = json.get("toast").and_then(inject::toast) {
            self.toast(&text, kind, ms, ctx.now);
        }
        if let Some(p) = json.get("pickables") {
            self.extra_pickables = inject::pickables(p, ctx.world);
        }
        if let Some(p) = json.get("palette_items") {
            self.extra_items = inject::palette_items(p);
        }
        Vec::new()
    }

    fn dump(&self) -> Value {
        let palette = self.palette.as_ref().map(|p| {
            let ranked = p.ranked();
            serde_json::json!({
                "query": p.line().text,
                "top": ranked.first().map(|i| i.name.clone()),
                "selected": ranked.get(p.cursor()).map(|i| i.name.clone()),
                "count": ranked.len(),
                "prompt": p.prompt(),
                "step": match p.step() {
                    Step::Open => "open",
                    Step::Args { .. } => "args",
                    Step::Run { .. } => "run",
                    Step::Closed => "closed",
                },
            })
        });
        let hints = self.hinting.as_ref().map(|h| {
            let kind = |id: &u64| match id >> 56 {
                1 => "screen",
                2 => "cat",
                3 => "door",
                4 => "book",
                _ => "object",
            };
            serde_json::json!({
                "count": self.stats.hints.len(),
                "typed": h.typed,
                "labels": self.stats.hints.values().collect::<Vec<_>>(),
                "targets": self.stats.hints.iter()
                    .map(|(id, l)| serde_json::json!({"label": l, "kind": kind(id)}))
                    .collect::<Vec<_>>(),
            })
        });
        serde_json::json!({
            "rasters": self.cache.rasters,
            "hits": self.cache.hits,
            "cached": self.cache.len(),
            "elements": self.stats.elements,
            "label": self.stats.label,
            "badges": self.stats.badges,
            "tags": self.stats.tags,
            "toasts": self.toasts.len(),
            "dialog": self.last_dialog.as_ref().map(|_| serde_json::json!({"chars": self.stats.dialog_chars})),
            "blips": self.blips_total,
            // never the text: the vault's phrase goes through this line
            "line": {"open": self.line_open, "chars": self.line.text.chars().count()},
            "hints": hints,
            "palette": palette,
            "map": self.map.map(|m| serde_json::json!({"floor": m.floor, "sel": m.sel})),
        })
    }
}

#[cfg(test)]
mod tests;
