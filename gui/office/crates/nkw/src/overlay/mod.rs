//! The 2D overlay (§3.7–3.10, §5.4): HUD (mode label + badge chips),
//! toasts, the AC dialogue box drawn from a [`DialogView`], SALTO labels,
//! COMANDOS, name tags anchored in the world, and the map. Every piece is a
//! [`draw::Spec`] rasterised only when it changes ([`cache::Cache`]); the
//! overlay asks for a compose frame only when something of its own changed
//! (`Want::Once`), toasts end on `Want::At`, and only short slides use
//! `Want::Continuous` (none under reduced motion). No `Rate` wish, ever.

pub mod andar;
pub mod apps;
pub mod cache;
pub mod draw;
pub mod hud;
pub mod inject;
pub mod look;
pub mod map;
pub mod phone;
pub mod prefs;
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
use crate::router::{KeyEv, names, table};
use crate::ui::phone::{PhoneApp, PhonePage, PhoneState, PhoneView};
use crate::ui::{Node, Theme};
use cache::Cache;
use draw::{Fonts, Px, Row, Spec};
use hud::Toast;
use map::MapState;
use nk_cats::keys::Bind;
use phone::Phone;
use prefs::{Prefs, Strip};
use project::View;

/// Where the mode label sits, logical px from the top-left.
pub const LABEL_POS: (i32, i32) = (16, 16);
/// The status strip's right/top margin, logical px.
pub const STRIP_MARGIN: i32 = 16;
/// Element ids (stable per role).
pub const LABEL_ID: u64 = 1;
pub const STRIP_ID: u64 = 2;
pub const PHONE_ID: u64 = 600;
/// The library's selected-book tags wear the selection look.
pub const SELECTED_TAGS: std::ops::RangeInclusive<u64> = 0x11b0_0100..=0x11b0_01ff;
const CHIP_ID: u64 = 10;
const TOAST_ID: u64 = 100;
const DIALOG_ID: u64 = 200;
const CHOICES_ID: u64 = 201;
const PALETTE_ID: u64 = 300;
const MAP_ID: u64 = 400;
const RING_ID: u64 = 401;
const HAND_ID: u64 = 500;
const CROSS_ID: u64 = 501;
const VIGNETTE_ID: u64 = 502;
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
    /// ANDAR's crosshair (dot ↔ ring)
    cross: andar::Cross,
    /// the 2D tags' fades
    fades: andar::Fades,
    /// `inject overlay {"aimed"|"carrying"|"vignette": …}` (tests, shots)
    forced: Forced,
    stats: Stats,
    /// the phone, while open (it rides COMANDOS)
    phone: Option<Phone>,
    /// `--demo`: the phone's dump may show the typed query
    demo: bool,
    /// the keymap mode the phone was opened from (Ajuda's first section)
    phone_origin: &'static str,
    prefs: Prefs,
    /// `<state-dir>/ui.toml`; `None` under `--demo`
    prefs_path: Option<std::path::PathBuf>,
    /// `[hud] strip` of `world.toml`
    hud_strip: Strip,
    /// `world.hud` (`z`): the label, the strip and the chips hidden
    hud_hidden: bool,
    /// `inject overlay {"los": bool|null}`: the tags' line of sight forced
    los: Option<bool>,
    /// the bindings in effect (Ajuda; the phone's printable keys)
    binds: Vec<Bind>,
    world_mod: &'static str,
    focus_escape: String,
    /// `world.palette_run {name, …}`: the next palette opens on it
    palette_ask: Option<(String, BTreeMap<String, String>)>,
}

/// What a script forces over the hub's/World's values (`null` releases).
#[derive(Default, Clone, Copy, Debug, PartialEq)]
struct Forced {
    aimed: Option<bool>,
    carrying: Option<bool>,
    vignette: Option<f32>,
}

#[derive(Default)]
struct Stats {
    elements: usize,
    label: Option<&'static str>,
    badges: usize,
    tags: usize,
    hints: BTreeMap<u64, String>,
    /// the dialogue's chars and choices as drawn (S16 counters)
    dialog_chars: usize,
    dialog_choices: usize,
    crosshair: Option<&'static str>,
    vignette: f32,
    hand: bool,
    /// the map's library shelves
    shelves: usize,
    /// the status strip is drawn
    strip: bool,
    /// tags left out this frame because a wall is in the way
    tags_hidden: usize,
    /// the tags' rects as placed (logical px)
    tag_rects: Vec<(i32, i32, i32, i32)>,
}

/// The keys while a screen is in hand (ANDAR), bottom-centre.
pub const HAND_HINT: &str = "g solta · [ ] distância · - = tamanho";

impl Overlay {
    pub fn new(cli: &crate::cli::Cli) -> Overlay {
        let focus_escape = cli
            .focus_escape
            .clone()
            .unwrap_or_else(|| crate::ext::FOCUS_ESCAPE.to_string());
        let prefs_path = prefs::path(cli);
        let hud_strip = prefs::world_toml(cli)
            .and_then(|p| std::fs::read_to_string(p).ok())
            .map(|t| prefs::hud_strip(&t))
            .unwrap_or_default();
        Overlay {
            phone: None,
            demo: cli.demo,
            phone_origin: "normal.world",
            prefs: prefs::load(prefs_path.as_deref()),
            prefs_path,
            hud_strip,
            hud_hidden: false,
            los: None,
            binds: crate::router::fallback_binds(cli.world_mod, &focus_escape),
            world_mod: cli.world_mod.name(),
            focus_escape,
            palette_ask: None,
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
            cross: andar::Cross::default(),
            fades: andar::Fades::default(),
            forced: Forced::default(),
            stats: Stats::default(),
        }
    }

    /// A screen is in hand (S15 `World::carrying`), or a script says so.
    fn carrying(&self, ctx: &Ctx) -> bool {
        self.forced
            .carrying
            .unwrap_or_else(|| ctx.world.carrying().is_some())
    }

    /// The comfort vignette's alpha 0..1 (S15 `World::comfort_vignette`).
    fn vignette(&self, ctx: &Ctx) -> f32 {
        self.forced
            .vignette
            .unwrap_or_else(|| ctx.world.comfort_vignette(ctx.now))
    }

    /// The palette's items besides the world's own (the hub's Σ
    /// `palette_items`; see the seam proposal). Replaces the previous set.
    pub fn set_palette_items(&mut self, items: Vec<PaletteItem>) {
        self.extra_items = items;
    }

    /// The phone's state while it is open (S21, for the hub's `hud_input`).
    pub fn phone_state(&self) -> Option<PhoneState> {
        self.phone.as_ref().map(|p| p.st.clone())
    }

    /// The bindings in effect (`Router::binds`, after every keymap
    /// change): Ajuda lists them and the phone's printable keys resolve in
    /// them. Until the hub hands them in, the S9 table.
    pub fn set_help(&mut self, binds: Vec<Bind>) {
        self.binds = binds;
        self.dirty = true;
    }

    /// The theme the prefs ask for.
    pub fn theme(&self) -> Theme {
        self.prefs.theme()
    }

    /// Reduced motion for the overlay's own animations: the Ajustes
    /// override, else the hub's.
    fn reduced(&self, ctx: &Ctx) -> bool {
        self.prefs.reduced_motion.unwrap_or(ctx.reduced_motion)
    }

    /// The strip's mode: Ajustes, else `[hud] strip`.
    fn strip_mode(&self) -> Strip {
        self.prefs.strip.unwrap_or(self.hud_strip)
    }

    /// Apply the phone's own acts (`phone.pref`) and pass the rest on.
    fn local(&mut self, reqs: Vec<Req>, ctx: &Ctx) -> Vec<Req> {
        let mut out = Vec::new();
        for r in reqs {
            match r {
                Req::Command { name, args } if name == phone::ACT_PREF => self.pref(&args, ctx),
                other => out.push(other),
            }
        }
        out
    }

    /// A pref toggled in Ajustes: applied now, saved (never under `--demo`).
    fn pref(&mut self, args: &BTreeMap<String, String>, ctx: &Ctx) {
        let p = &mut self.prefs;
        match args.get("key").map(String::as_str) {
            Some("reduced_motion") => {
                p.reduced_motion = Some(!p.reduced_motion.unwrap_or(ctx.reduced_motion));
            }
            Some("contrast") => p.high_contrast = !p.high_contrast,
            Some("font_scale") => p.font_step(args.get("dir").map(String::as_str) != Some("down")),
            Some("strip") => p.strip = Some(p.strip.unwrap_or(self.hud_strip).next()),
            _ => return,
        }
        self.dirty = true;
        if let Err(e) = prefs::save(self.prefs_path.as_deref(), &self.prefs) {
            tracing::warn!("ui.toml: {e}");
        }
    }

    /// A printable key while the phone is up: the search line or the
    /// selected item's keys, else the phone's table (the live keymap).
    fn phone_key(&mut self, ev: &KeyEv, ctx: &Ctx) -> Vec<Req> {
        let text = ev
            .utf8
            .as_deref()
            .filter(|_| !ev.mods.ctrl && !ev.mods.alt && !ev.mods.logo);
        let Some(ph) = self.phone.as_mut() else {
            return Vec::new();
        };
        if let Some(r) = ph.text(&ev.name, text) {
            return self.local(r, ctx);
        }
        let cmd = self
            .binds
            .iter()
            .find(|b| b.mode == "world.phone" && b.keys.iter().any(|k| k == &ev.name))
            .map(|b| b.command.clone());
        match cmd {
            Some(c) => self.command(&c, &BTreeMap::new(), ctx).unwrap_or_default(),
            None => Vec::new(),
        }
    }

    /// Open the phone on `app` (home when `None`) over COMANDOS.
    fn open_phone(&mut self, app: Option<String>, ctx: &Ctx) -> Vec<Req> {
        let mode = ctx.world.mode();
        if self.phone.is_none() {
            self.phone_origin = names::mode_of(mode, self.submode());
        }
        self.map = None;
        self.palette = None;
        self.palette_ask = None;
        let mut ph = Phone::new(None);
        if let Some(a) = app {
            let r = ph.open_app(&a);
            if !r.is_empty() {
                // an action app (the map): nothing to open
                return r;
            }
        }
        self.phone = Some(ph);
        if mode.kind() == Some(Kind::Palette) {
            Vec::new()
        } else {
            vec![Req::Action(Action::Palette)]
        }
    }

    /// The phone's commands (S26 `world.phone_*`); `None` when closed.
    fn phone_command(&mut self, name: &str, ctx: &Ctx) -> Option<Vec<Req>> {
        let ph = self.phone.as_mut()?;
        let r = match name {
            "world.phone_left" => ph.nav(Dir::Left),
            "world.phone_right" => ph.nav(Dir::Right),
            "world.phone_up" => ph.nav(Dir::Up),
            "world.phone_down" => ph.nav(Dir::Down),
            "world.phone_open" => ph.enter(),
            "world.phone_back" => ph.back(),
            "world.phone_close" => ph.close(),
            "world.phone_search" => ph.search(),
            n => ph.app_n(n.strip_prefix("world.phone_app_")?.parse().ok()?),
        };
        Some(self.local(r, ctx))
    }

    /// The overlay's own apps' pages (Mapa, Ajustes, Ajuda).
    fn own_page(&self, app: &str, ctx: &Ctx) -> Option<PhonePage> {
        match app {
            apps::MAPA => Some(apps::mapa()),
            apps::AJUSTES => Some(apps::ajustes(
                &self.prefs,
                &apps::Facts {
                    walk: ctx.world.walk_config(),
                    world_mod: self.world_mod,
                    focus_escape: &self.focus_escape,
                    hub_reduced: ctx.reduced_motion,
                    persisted: self.prefs_path.is_some(),
                },
            )),
            apps::AJUDA => Some(apps::ajuda(&self.binds, self.phone_origin)),
            _ => None,
        }
    }

    /// `Route::Text`: a key typed into the overlay (palette, line, SALTO,
    /// the map, the phone).
    pub fn key(&mut self, ev: &KeyEv, ctx: &Ctx) -> Vec<Req> {
        if !ev.pressed {
            return Vec::new();
        }
        self.dirty = true;
        if self.phone.is_some() && ctx.world.mode().kind() == Some(Kind::Palette) {
            return self.phone_key(ev, ctx);
        }
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

    /// `Some("phone")` while the phone is open, `Some("map")` while the map
    /// is.
    pub fn submode(&self) -> Option<&'static str> {
        if self.phone.is_some() {
            return Some("phone");
        }
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

    /// COMANDOS opens: the list, or straight on the asked command's prompt.
    fn open_palette(&mut self) {
        let items = self.items();
        self.palette = Some(match self.palette_ask.take() {
            Some((name, preset)) => {
                let specs = self.arg_specs.get(&name).cloned().unwrap_or_default();
                Palette::ask(items, &name, specs, preset)
            }
            None => Palette::new(items, ""),
        });
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
        let reduced = self.reduced(ctx);
        let theme = self.theme();
        let strip_mode = self.strip_mode();
        // the phone's apps and page: the hub's gathering, else (no hub,
        // unit tests) the overlay's own apps
        let own = self.phone.as_ref().map(|ph| PhoneView {
            state: ph.st.clone(),
            apps: apps::apps(),
            page: ph.st.app.as_deref().and_then(|a| self.own_page(a, ctx)),
        });
        if let Some(ph) = self.phone.as_mut() {
            ph.view = hud.phone.clone().or(own);
        }
        let walking = ctx.world.mode().walking();
        let carrying = walking && self.carrying(ctx);
        let vignette = if walking { self.vignette(ctx) } else { 0.0 };
        let mut fonts = self.fonts.take().unwrap_or_default();
        let mut stats = Stats::default();
        let mut f = Frame {
            cache: &mut self.cache,
            fonts: &mut fonts,
            s: raster_scale(ctx.scale),
            out,
            focus: hud.focus_rect,
            els: Vec::new(),
        };

        // the map (a sub-view of NAV: it covers the world)
        if let Some(m) = self.map {
            // the library's shelves, from its books (pickables)
            let books: Vec<glam::Vec3> = hud
                .pickables
                .iter()
                .chain(&self.extra_pickables)
                .filter(|p| matches!(p.pick, Pick::Book(_)))
                .map(|p| p.pos)
                .collect();
            let (spec, targets) = map::layout_with(ctx.world, m.floor, out, &books);
            if let Spec::Map { boxes, .. } = &spec {
                stats.shelves = boxes
                    .iter()
                    .filter(|b| b.kind == draw::MapKind::Shelf)
                    .count();
            }
            if let Some(t) = targets.get(m.sel) {
                let ring = Spec::Ring {
                    w: t.rect.2,
                    h: t.rect.3,
                };
                f.put(RING_ID, &ring, Placer::At(t.rect.0, t.rect.1));
            }
            f.put(MAP_ID, &spec, Placer::Raw(0, 0));
        }

        // the phone (over COMANDOS, centred)
        if let Some(ph) = self.phone.as_mut() {
            let o = (out.x as i32, out.y as i32);
            let node = phone::node(ph, &hud.status, o, &theme, f.fonts);
            let spec = Spec::Ui {
                node,
                w: o.0,
                h: o.1,
                theme: theme.clone(),
            };
            let size = f.size(&spec);
            f.put_over(
                PHONE_ID,
                &spec,
                Placer::At((o.0 - size.0) / 2, (o.1 - size.1) / 2),
                true,
            );
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
        let mut dialog_top: Option<i32> = None;
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
                let rise = match self.dialog_since {
                    Some(t0) if !reduced && now - t0 < RISE_S => {
                        let p = ((now - t0) / RISE_S).clamp(0.0, 1.0);
                        (RISE_PX * (1.0 - p).powi(3)).round() as i32
                    }
                    _ => 0,
                };
                let width = (out.x as i32 - 128).min(760);
                // columns in the raster's own px (the face is the scaled one)
                let cw = f.fonts.face(f.s.font(draw::PX_DIALOG)).cw as i32;
                let cols = ((f.s.p(width) - f.s.p(48)) / cw.max(1)).max(10) as usize;
                let all = draw::wrap(&d.text, cols);
                let skip = all.len().saturating_sub(draw::DIALOG_LINES);
                let lines: Vec<String> = all.iter().skip(skip).cloned().collect();
                stats.dialog_chars = drawn_chars(&d.text, &all, skip);
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
                dialog_top = Some(y);
                if !d.choices.is_empty() {
                    let ch = Spec::Choices {
                        items: d.choices.clone(),
                        selected: d.selected,
                    };
                    let cs = f.size(&ch);
                    let r = f.put(
                        CHOICES_ID,
                        &ch,
                        Placer::At(x + width - cs.0 - 24, y - cs.1 + 4),
                    );
                    if r.2 > 0 {
                        stats.dialog_choices = d.choices.len();
                    }
                }
            }
            None => {
                self.last_dialog = None;
                self.dialog_since = None;
                self.blink_at = None;
            }
        }

        // toasts, bottom-left, the newest at the bottom
        // (above the dialogue box while one is open)
        let mut y = dialog_top.map_or(out.y as i32 - 16, |t| t - 8);
        let cell = f.fonts.face(theme.px(draw::PX_UI)).cw as i32;
        for (i, t) in self.toasts.iter().enumerate().rev() {
            let spec = Spec::Ui {
                node: look::toast(&t.text, t.kind, &theme, cell),
                w: look::TOAST_MAX_W,
                h: out.y as i32,
                theme: theme.clone(),
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

        // HUD: the mode label (not in FOCO, not over the map), the badges,
        // the status strip at the right; `world.hud` hides all three
        let ui = |node: Node, w: i32, h: i32| Spec::Ui {
            node,
            w,
            h,
            theme: theme.clone(),
        };
        let (ow, oh) = (out.x as i32, out.y as i32);
        let mut x = LABEL_POS.0;
        if !self.hud_hidden && f.focus.is_none() && self.map.is_none() {
            let label = if self.phone.is_some() {
                "TELEFONE"
            } else {
                hud.mode_label
            };
            let spec = ui(look::label(label, &theme), ow, oh);
            let r = f.put(LABEL_ID, &spec, Placer::Raw(LABEL_POS.0, LABEL_POS.1));
            x += r.2 + 8;
            stats.label = Some(label);
        }
        if !self.hud_hidden {
            for (i, b) in hud.badges.iter().enumerate() {
                let spec = ui(look::chip(b, &theme), ow, oh);
                let (w, h) = f.size(&spec);
                let lh = f.size(&ui(look::label("A", &theme), ow, oh)).1;
                let y = LABEL_POS.1 + (lh - h) / 2;
                let r = f.put(CHIP_ID + i as u64, &spec, Placer::Raw(x, y));
                if r.2 > 0 {
                    stats.badges += 1;
                }
                x += w + 6;
            }
        }
        if !self.hud_hidden
            && self.map.is_none()
            && self.phone.is_none()
            && let Some(node) = look::strip(&hud.status, strip_mode, &theme)
        {
            let spec = ui(node, ow, oh);
            let (w, _) = f.size(&spec);
            let r = f.put(
                STRIP_ID,
                &spec,
                Placer::Raw(ow - STRIP_MARGIN - w, STRIP_MARGIN),
            );
            stats.strip = r.2 > 0;
        }

        // name tags (not in FOCO, not over the map, not behind a wall): 80
        // ms in, 120 ms out once they leave their reading radius (addendum
        // §3.1); placed so none covers another, the nearest keeps its spot
        if f.focus.is_none() && self.map.is_none() {
            let mut vis = project::visible_tags(&view, &hud.tags);
            let before = vis.len();
            let forced = self.los;
            vis.retain(|(i, _, _)| {
                let t = &hud.tags[*i];
                forced
                    .unwrap_or_else(|| ctx.world.line_of_sight(view.cam, t.pos, ctx.visible_rooms))
            });
            stats.tags_hidden = before - vis.len();
            stats.tags = vis.len();
            let dist: BTreeMap<u64, f32> =
                vis.iter().map(|(i, _, d)| (hud.tags[*i].id, *d)).collect();
            let mut shown = self.fades.update(
                now,
                reduced,
                vis.iter().map(|(i, q, _)| {
                    let t = &hud.tags[*i];
                    (t.id, t.text.clone(), t.rgb, *q)
                }),
            );
            // nearest first (fading-out ones last)
            let d = |id: u64| dist.get(&id).copied().unwrap_or(f32::MAX);
            shown.sort_by(|a, b| d(a.id).total_cmp(&d(b.id)).then(a.id.cmp(&b.id)));
            let mut specs = Vec::new();
            for s in shown {
                let metres = dist.get(&s.id).copied().unwrap_or(10.0);
                let sel = SELECTED_TAGS.contains(&s.id);
                let tag = ui(look::tag(&s.text, s.rgb, metres, sel, &theme), ow, oh);
                let size = f.size(&tag);
                // the box sits above its anchor: centre it there
                let anchor = Vec2::new(s.at.x, s.at.y - 6.0 - size.1 as f32 / 2.0);
                specs.push((s.id, anchor, size, draw::faded(tag, s.level)));
            }
            let items: Vec<(u64, Vec2, (i32, i32))> =
                specs.iter().map(|t| (t.0, t.1, t.2)).collect();
            let placed = project::place_labels(&items, out);
            for ((id, _, _, spec), (_, at)) in specs.iter().zip(placed) {
                let r = f.put(TAG_ID + (id & 0xf_ffff), spec, Placer::Raw(at.0, at.1));
                stats.tag_rects.push(r);
            }
        } else {
            // they fade in again afterwards
            self.fades = andar::Fades::default();
        }

        // ANDAR: the keys while carrying, the crosshair, and under
        // everything the comfort vignette
        if walking && carrying {
            let spec = Spec::Bar {
                text: HAND_HINT.into(),
            };
            let size = f.size(&spec);
            let at = ((out.x as i32 - size.0) / 2, out.y as i32 - size.1 - 28);
            stats.hand = f.put(HAND_ID, &spec, Placer::At(at.0, at.1)).2 > 0;
        }
        let aimed = self.forced.aimed.unwrap_or(hud.aimed);
        self.cross.update(walking && aimed, now, reduced);
        if walking {
            let c = draw::CROSS_BOX;
            let spec = Spec::Crosshair {
                t: self.cross.step(now, reduced),
            };
            let at = ((out.x as i32 - c) / 2, (out.y as i32 - c) / 2);
            f.put(CROSS_ID, &spec, Placer::Raw(at.0, at.1));
            stats.crosshair = Some(if self.cross.aimed() { "ring" } else { "dot" });
        }
        let a16 = if walking {
            andar::vignette_a16(vignette)
        } else {
            0
        };
        if a16 > 0 {
            let spec = Spec::Vignette {
                w: out.x.ceil() as i32,
                h: out.y.ceil() as i32,
                a16,
            };
            f.put(VIGNETTE_ID, &spec, Placer::Raw(0, 0));
        }
        stats.vignette = a16 as f32 / 16.0;

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
    /// the raster scale ([`raster_scale`])
    s: Px,
    /// logical px
    out: Vec2,
    /// FOCO: nothing may cover the window
    focus: Option<[i32; 4]>,
    els: Vec<OverlayElem>,
}

impl Frame<'_> {
    /// The logical size `spec` draws at (rasterised once, then cached).
    fn size(&mut self, spec: &Spec) -> (i32, i32) {
        self.cache.get(spec, self.s, self.fonts).size
    }

    /// Add `spec` as element `id`; returns its rect (w, h 0 when it would
    /// cover the FOCO window and was left out).
    fn put(&mut self, id: u64, spec: &Spec, at: Placer) -> (i32, i32, i32, i32) {
        self.put_over(id, spec, at, false)
    }

    /// [`Frame::put`]; `over_focus`: drawn even over the FOCO window (the
    /// phone, which the user opened over it on purpose).
    fn put_over(
        &mut self,
        id: u64,
        spec: &Spec,
        at: Placer,
        over_focus: bool,
    ) -> (i32, i32, i32, i32) {
        let g = self.cache.get(spec, self.s, self.fonts);
        let size = g.size;
        let pos = at.place(size, self.out);
        if let Some(r) = self.focus
            && !over_focus
            && intersects(r, (pos.0, pos.1, size.0, size.1))
        {
            return (pos.0, pos.1, 0, 0);
        }
        self.els.push(OverlayElem {
            id,
            pos,
            buf: g.buf,
            buf_scale: g.buf_scale,
        });
        (pos.0, pos.1, size.0, size.1)
    }
}

/// The overlay's raster scale for an output scale: the integer scale at 1×,
/// 2× …; the scale itself when it is fractional (physical px, S16:
/// `(scale·120).round() % 120 != 0`).
pub fn raster_scale(scale: f64) -> Px {
    let k = (scale * 120.0).round() as i64;
    if k % 120 != 0 && k > 0 {
        Px(scale as f32)
    } else {
        Px(scale.round().max(1.0) as f32)
    }
}

/// How many of `text`'s chars the dialogue box draws when its wrapped
/// lines `all` lose their first `skip` (the box shows the last
/// [`draw::DIALOG_LINES`]): each skipped line and the space or newline it
/// was cut at.
pub fn drawn_chars(text: &str, all: &[String], skip: usize) -> usize {
    let total = text.chars().count();
    let gone: usize = all.iter().take(skip).map(|l| l.chars().count() + 1).sum();
    total.saturating_sub(gone)
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
        let reduced = self.reduced(ctx);
        self.toasts.retain(|t| !t.gone(now, reduced));
        if self.toasts.len() != n {
            self.dirty = true;
        }
        let mode = ctx.world.mode();
        if self.phone.is_some() && mode.kind() != Some(Kind::Palette) {
            self.phone = None;
            self.dirty = true;
        }
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
        let reduced = self.reduced(ctx);
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
        // the crosshair's 80 ms (a short slide); the tags' fade steps
        if self.cross.moving(now, reduced) {
            w.push(Wish::compose(Want::Continuous));
        }
        if let Some(t) = self.fades.next_at(now, reduced) {
            w.push(Wish::compose(Want::At(t)));
        }
        w
    }

    fn submode(&self) -> Option<&'static str> {
        Overlay::submode(self)
    }

    fn phone_apps(&self, ctx: &Ctx) -> Vec<PhoneApp> {
        let _ = ctx;
        apps::apps()
    }

    fn phone_page(&self, app: &str, st: &PhoneState, ctx: &Ctx) -> Option<PhonePage> {
        let _ = st;
        self.own_page(app, ctx)
    }

    fn command(
        &mut self,
        name: &str,
        args: &BTreeMap<String, String>,
        ctx: &Ctx,
    ) -> Option<Vec<Req>> {
        self.dirty = true;
        let act = |a: Action| Some(vec![Req::Action(a)]);
        if name.starts_with("world.phone_")
            && let Some(r) = self.phone_command(name, ctx)
        {
            return Some(r);
        }
        match name {
            // the phone (S21): open, or close when open (`world+n` again)
            "world.phone" if self.phone.is_some() && args.get("app").is_none() => {
                Some(vec![Req::Action(Action::Back)])
            }
            "world.phone" => Some(self.open_phone(args.get("app").cloned(), ctx)),
            "world.help" => Some(self.open_phone(Some(apps::AJUDA.into()), ctx)),
            "world.hud" => {
                self.hud_hidden = !self.hud_hidden;
                Some(Vec::new())
            }
            n if n == phone::ACT_PREF => {
                self.pref(args, ctx);
                Some(Vec::new())
            }
            // the palette already on a command's first argument prompt
            // (W5's session keys); the other args ride along
            "world.palette_run" if args.contains_key("name") => {
                let mut rest = args.clone();
                let target = rest.remove("name").unwrap_or_default();
                self.phone = None;
                self.palette_ask = Some((target, rest));
                if ctx.world.mode().kind() == Some(Kind::Palette) {
                    self.open_palette();
                    Some(Vec::new())
                } else {
                    act(Action::Palette)
                }
            }
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
            // `world.library`, `grab`, `nearer`, `farther`: the World's now
            // (S15), not toasts
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
            // the phone rides COMANDOS: no list while it is up
            Effect::OpenPalette if self.phone.is_some() => {}
            Effect::OpenPalette => {
                self.open_palette();
                // an asked command with nothing left to ask runs now
                if matches!(
                    self.palette.as_ref().map(|p| p.step()),
                    Some(Step::Run { .. })
                ) {
                    return self.palette_run();
                }
            }
            Effect::ClosePalette => {
                self.palette = None;
                self.apps_only = false;
                self.phone = None;
                self.palette_ask = None;
            }
            _ => {}
        }
        Vec::new()
    }

    fn inject(&mut self, json: &Value, ctx: &Ctx) -> Vec<Req> {
        self.dirty = true;
        let mut out = Vec::new();
        if let Some(p) = json.get("prefs") {
            // runtime only (shots): never saved
            self.prefs.apply_json(p);
        }
        if let Some(v) = json.get("los") {
            self.los = v.as_bool();
        }
        if let Some(v) = json.get("hud_hidden") {
            self.hud_hidden = v.as_bool().unwrap_or(false);
        }
        if let Some(p) = json.get("phone") {
            if p.is_null() {
                if self.phone.is_some() {
                    out.push(Req::Action(Action::Back));
                }
            } else {
                let app = p.get("app").and_then(Value::as_str).map(String::from);
                out.extend(self.open_phone(app, ctx));
                if let (Some(ph), Some(sub)) =
                    (self.phone.as_mut(), p.get("sub").and_then(Value::as_str))
                {
                    ph.st.sub = Some(sub.to_string());
                }
            }
        }
        if let Some(pg) = json.get("phone_page") {
            let page = inject::phone_page(pg);
            if let Some(ph) = self.phone.as_mut() {
                ph.forced = page;
            }
        }
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
        if let Some(v) = json.get("aimed") {
            self.forced.aimed = v.as_bool();
        }
        if let Some(v) = json.get("carrying") {
            self.forced.carrying = v.as_bool();
        }
        if let Some(v) = json.get("vignette") {
            self.forced.vignette = v.as_f64().map(|a| a as f32);
        }
        out
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
            // as drawn (S16): R4's regression test waits on these
            "dialog": self.last_dialog.as_ref().map(|_| serde_json::json!({
                "chars": self.stats.dialog_chars,
                "choices": self.stats.dialog_choices,
            })),
            "crosshair": self.stats.crosshair,
            "vignette": self.stats.vignette,
            "hand": self.stats.hand,
            "blips": self.blips_total,
            // never the text: the vault's phrase goes through this line
            "line": {"open": self.line_open, "chars": self.line.text.chars().count()},
            "hints": hints,
            "palette": palette,
            "map": self.map.map(|m| serde_json::json!({"floor": m.floor, "sel": m.sel, "shelves": self.stats.shelves})),
            "submode": self.submode(),
            // S21: the phone's truth (`dump().mode` says COMANDOS)
            "phone": self.phone.as_ref().map(|p| p.dump(self.demo)),
            "strip": self.stats.strip,
            "hud_hidden": self.hud_hidden,
            "prefs": {
                "contrast": if self.prefs.high_contrast { "high" } else { "normal" },
                "font_scale_pct": self.prefs.font_scale_pct,
                "strip": self.strip_mode().as_str(),
                "reduced_motion": self.prefs.reduced_motion,
                "saved": self.prefs_path.is_some(),
            },
            "tags_dropped_los": self.stats.tags_hidden,
        })
    }
}

#[cfg(test)]
mod tests;
