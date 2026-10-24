//! The phone (S21): a NookPhone-like panel over COMANDOS. This file is the
//! shell — its state machine (home grid → app → sub-page → back → close;
//! `1..9`; the search line, `live` fuzzy or `run`; an item's `on_keys`
//! before the mode table; a `close` act emits `Back` first) and the tree it
//! draws. Pure: the overlay feeds it the last [`PhoneView`] the hub
//! gathered and hands its `Req`s on.

use std::collections::BTreeMap;

use nkw_core::line::Line;
use nkw_core::palette::fuzzy_rank;
use nkw_core::{Action, Dir};

use super::draw::{Fonts, PX_SMALL, PX_UI, Rgb};
use super::look;
use crate::ext::{Req, Status};
use crate::ui::phone::{
    ACT_APP, ACT_BACK, ACT_CLOSE, ACT_SUB, Act, Item, PhoneApp, PhonePage, PhoneState, PhoneView,
};
use crate::ui::{Icon, Node, Size, Theme, gap, measure, selectable, text};

/// The panel, logical px (`min(out.h − 80)` tall).
pub const W: i32 = 400;
pub const H: i32 = 620;
pub const RADIUS: i32 = 24;
pub const PAD: i32 = 16;
/// The home grid's columns.
pub const COLS: usize = 3;
/// Acts the overlay applies itself (its prefs), never dispatched.
pub const ACT_PREF: &str = "phone.pref";
/// The app that is an action, not a page: it closes the phone and opens
/// the map (the NAV sub-view it is).
pub const DIRECT: &[(&str, &str)] = &[("mapa", "world.overview")];

/// The app tiles' colours, by home order (warm pastels).
pub const TINTS: [Rgb; 10] = [
    [0xf0, 0x9a, 0x6a],
    [0xe8, 0xb4, 0x3c],
    [0x6c, 0xb8, 0x84],
    [0x6a, 0xa4, 0xdc],
    [0xa4, 0x8a, 0xd8],
    [0xe2, 0x80, 0x9c],
    [0x8a, 0xb8, 0x58],
    [0xc4, 0x96, 0x66],
    [0x4c, 0xb0, 0xa6],
    [0xdc, 0x70, 0x60],
];
const WHITE: Rgb = [0xff, 0xfb, 0xf2];

#[derive(Clone, Debug, Default)]
pub struct Phone {
    pub st: PhoneState,
    /// the search line has the keys
    pub typing: bool,
    pub line: Line,
    /// `inject overlay {"phone_page": …}`: drawn instead of the hub's page
    pub forced: Option<PhonePage>,
    /// what the hub gathered last (apps, the page) — what keys act on
    pub view: Option<PhoneView>,
}

fn cmd(a: &Act) -> Req {
    Req::Command {
        name: a.name.clone(),
        args: a.args.clone(),
    }
}

impl Phone {
    pub fn new(app: Option<String>) -> Phone {
        Phone {
            st: PhoneState {
                app,
                ..PhoneState::default()
            },
            ..Phone::default()
        }
    }

    /// The home grid's apps, sorted by order.
    pub fn apps(&self) -> Vec<PhoneApp> {
        let mut a = self
            .view
            .as_ref()
            .map(|v| v.apps.clone())
            .unwrap_or_default();
        a.sort_by_key(|x| x.order);
        a
    }

    /// The page shown (the injected one wins).
    pub fn page(&self) -> Option<&PhonePage> {
        self.st.app.as_ref()?;
        self.forced
            .as_ref()
            .or_else(|| self.view.as_ref().and_then(|v| v.page.as_ref()))
    }

    /// The page's items as listed: filtered by the query when its search
    /// is `live`.
    pub fn shown(&self) -> Vec<&Item> {
        let Some(p) = self.page() else {
            return Vec::new();
        };
        let live = p.search.as_ref().is_some_and(|s| s.live);
        if !live || self.st.query.trim().is_empty() {
            return p.items.iter().collect();
        }
        let texts: Vec<String> = p
            .items
            .iter()
            .map(|i| {
                let mut s = i.title.clone();
                for x in [&i.detail, &i.badge].into_iter().flatten() {
                    s.push(' ');
                    s.push_str(x);
                }
                s
            })
            .collect();
        let refs: Vec<&str> = texts.iter().map(String::as_str).collect();
        fuzzy_rank(&self.st.query, &refs)
            .into_iter()
            .map(|i| &p.items[i])
            .collect()
    }

    /// How many things the selection moves over (apps at home, items).
    pub fn count(&self) -> usize {
        match self.st.app {
            None => self.apps().len(),
            Some(_) => self.shown().len(),
        }
    }

    /// The selected item of a list page.
    pub fn selected(&self) -> Option<&Item> {
        self.shown().get(self.st.sel).copied()
    }

    /// Keep `sel` inside what is listed.
    pub fn clamp(&mut self) {
        let n = self.count();
        self.st.sel = self.st.sel.min(n.saturating_sub(1));
    }

    fn set_query(&mut self) {
        self.st.query = self.line.text.clone();
        self.st.sel = 0;
        self.st.scroll = 0;
    }

    /// Open an app (home grid, `1..9`, `phone.app`).
    pub fn open_app(&mut self, id: &str) -> Vec<Req> {
        if let Some((_, c)) = DIRECT.iter().find(|d| d.0 == id) {
            return vec![
                Req::Action(Action::Back),
                Req::Command {
                    name: c.to_string(),
                    args: BTreeMap::new(),
                },
            ];
        }
        self.st = PhoneState {
            app: Some(id.to_string()),
            ..PhoneState::default()
        };
        self.typing = false;
        self.line.clear();
        self.forced = None;
        Vec::new()
    }

    /// `h/j/k/l`: the grid moves in 3 columns; a list up and down; `h`/`l`
    /// on a list item are its own `on_keys` ("h"/"l") when it has them.
    pub fn nav(&mut self, d: Dir) -> Vec<Req> {
        let n = self.count();
        if n == 0 {
            return Vec::new();
        }
        let s = self.st.sel.min(n - 1);
        if self.st.app.is_none() {
            self.st.sel = match d {
                Dir::Left => s.saturating_sub(1),
                Dir::Right => (s + 1).min(n - 1),
                Dir::Up => s.checked_sub(COLS).unwrap_or(s),
                Dir::Down => {
                    if s + COLS < n {
                        s + COLS
                    } else {
                        s
                    }
                }
            };
            return Vec::new();
        }
        match d {
            Dir::Up => self.st.sel = s.saturating_sub(1),
            Dir::Down => self.st.sel = (s + 1).min(n - 1),
            Dir::Left | Dir::Right => {
                let k = if d == Dir::Left { "h" } else { "l" };
                if let Some(a) = self
                    .selected()
                    .and_then(|i| i.on_keys.iter().find(|(key, _)| key == k))
                    .map(|(_, a)| a.clone())
                {
                    return self.act(&a);
                }
            }
        }
        Vec::new()
    }

    /// `enter`/`space`: open the app, run the item's act, or (typing) end
    /// the search line (`run` sends the query).
    pub fn enter(&mut self) -> Vec<Req> {
        if self.typing {
            self.typing = false;
            if let Some(run) = self
                .page()
                .and_then(|p| p.search.as_ref())
                .and_then(|s| s.run.clone())
            {
                return vec![Req::Command {
                    name: run,
                    args: BTreeMap::from([("q".to_string(), self.st.query.clone())]),
                }];
            }
            return Vec::new();
        }
        match &self.st.app {
            None => {
                let apps = self.apps();
                match apps.get(self.st.sel) {
                    Some(a) => self.open_app(a.id),
                    None => Vec::new(),
                }
            }
            Some(_) => match self.selected().and_then(|i| i.on_enter.clone()) {
                Some(a) => self.act(&a),
                None => Vec::new(),
            },
        }
    }

    /// An act: the phone's own (sub, app, back, close) here; any other is
    /// a command (`close`: `Back` first, then the command).
    pub fn act(&mut self, a: &Act) -> Vec<Req> {
        match a.name.as_str() {
            ACT_SUB => {
                self.st.sub = a.args.get("sub").cloned();
                self.st.sel = 0;
                self.st.scroll = 0;
                self.st.query.clear();
                self.line.clear();
                self.typing = false;
                Vec::new()
            }
            ACT_APP => match a.args.get("app") {
                Some(id) => self.open_app(&id.clone()),
                None => Vec::new(),
            },
            ACT_BACK => self.back(),
            ACT_CLOSE => self.close(),
            _ if a.close => vec![Req::Action(Action::Back), cmd(a)],
            _ => vec![cmd(a)],
        }
    }

    /// `backspace`: the search line edits; else sub → app → home → close.
    pub fn back(&mut self) -> Vec<Req> {
        if self.typing {
            if self.line.text.is_empty() {
                self.typing = false;
            } else {
                self.line.backspace();
                self.set_query();
            }
            return Vec::new();
        }
        if self.st.sub.is_some() {
            self.st.sub = None;
            self.st.sel = 0;
            self.st.scroll = 0;
            return Vec::new();
        }
        if let Some(app) = self.st.app.take() {
            let at = self.apps().iter().position(|a| a.id == app).unwrap_or(0);
            self.st = PhoneState {
                sel: at,
                ..PhoneState::default()
            };
            self.line.clear();
            self.forced = None;
            return Vec::new();
        }
        self.close()
    }

    /// `esc`/`tab`: the search line lets go first; else the phone closes.
    pub fn close(&mut self) -> Vec<Req> {
        if self.typing {
            self.typing = false;
            return Vec::new();
        }
        vec![Req::Action(Action::Back)]
    }

    /// `1..9`: app N of the home order.
    pub fn app_n(&mut self, n: usize) -> Vec<Req> {
        let apps = self.apps();
        match n.checked_sub(1).and_then(|i| apps.get(i)) {
            Some(a) => self.open_app(a.id),
            None => Vec::new(),
        }
    }

    /// `/`: the search line takes the keys (when the page has one).
    pub fn search(&mut self) -> Vec<Req> {
        if self.page().is_some_and(|p| p.search.is_some()) {
            self.typing = true;
            self.line.end();
        }
        Vec::new()
    }

    /// A printable key (`Route::Text`): into the search line while typing;
    /// else the selected item's `on_keys`. `None`: not taken (the overlay
    /// then tries the mode's table).
    pub fn text(&mut self, key: &str, utf8: Option<&str>) -> Option<Vec<Req>> {
        if self.typing {
            match (key, utf8) {
                ("left", _) => self.line.left(),
                ("right", _) => self.line.right(),
                ("home", _) => self.line.home(),
                ("end", _) => self.line.end(),
                (_, Some(t)) => {
                    self.line.insert(t);
                    self.set_query();
                }
                _ => return None,
            }
            return Some(Vec::new());
        }
        self.st.app.as_ref()?;
        let a = self
            .selected()
            .and_then(|i| i.on_keys.iter().find(|(k, _)| k == key))
            .map(|(_, a)| a.clone())?;
        Some(self.act(&a))
    }

    /// Make the selection visible in a list of `fit` rows.
    pub fn scroll_to(&mut self, fit: usize) {
        let fit = fit.max(1);
        if self.st.sel < self.st.scroll {
            self.st.scroll = self.st.sel;
        }
        if self.st.sel >= self.st.scroll + fit {
            self.st.scroll = self.st.sel + 1 - fit;
        }
        let n = self.count();
        self.st.scroll = self.st.scroll.min(n.saturating_sub(fit));
    }

    /// `dump()["ext"]["overlay"]["phone"]` (rule 15): counts, ids, the app
    /// and page title — never an item's title nor the page's header (the
    /// library's file names, the player's track); the typed query only
    /// under `--demo` (else its length).
    pub fn dump(&self, demo: bool) -> serde_json::Value {
        let query = if demo {
            serde_json::json!(self.st.query)
        } else {
            serde_json::json!({"chars": self.st.query.chars().count()})
        };
        serde_json::json!({
            "app": self.st.app,
            "sub": self.st.sub,
            "sel": self.st.sel,
            "scroll": self.st.scroll,
            "items": self.count(),
            "query": query,
            "typing": self.typing,
            "title": self.page().map(|p| p.title.clone()),
            "selected": self.selected().map(|i| i.id.clone()),
        })
    }
}

// ---- drawing ----

fn icon(kind: Icon, px: i32, rgb: Rgb) -> Node {
    Node::Icon { kind, px, rgb }
}

fn centred(n: Node) -> Node {
    Node::Row {
        gap: 0,
        items: vec![Node::Spacer(0), n, Node::Spacer(0)],
    }
}

/// A round tile with a white icon: the app's face.
fn tile(kind: Icon, tint: Rgb, px: i32) -> Node {
    Node::Panel {
        radius: px,
        bg: tint,
        alpha: 255,
        pad: px / 5,
        child: Box::new(icon(kind, px - 2 * (px / 5), WHITE)),
    }
}

/// A badge: white on coral (the error ink in high contrast).
/// An item's badge: an alert ("!", a count) is the coral chip; anything
/// else (keys, "zZ", "100 %") a quiet cream keycap.
fn badge(s: String, t: &Theme) -> Node {
    let alert = s == "!" || (!s.is_empty() && s.chars().all(|c| c.is_ascii_digit()));
    if alert {
        return chip(s, t);
    }
    Node::Chip {
        text: s,
        bg: t.select,
        fg: t.ink,
    }
}

fn chip(s: String, t: &Theme) -> Node {
    Node::Chip {
        text: s,
        bg: if t.high_contrast {
            t.error_ink
        } else {
            [0xd8, 0x4a, 0x3a]
        },
        fg: WHITE,
    }
}

/// The phone's header: the clock big, then what the status knows.
pub fn header(st: &Status, t: &Theme) -> Node {
    let ui = t.px(PX_UI);
    let mut right: Vec<Node> = Vec::new();
    if let Some(n) = st.inbox {
        right.push(Node::Row {
            gap: 3,
            items: vec![
                icon(Icon::Envelope, 15, t.ink),
                text(n.to_string(), ui, t.ink, false),
            ],
        });
    }
    if let Some(c) = st.cost_cents {
        right.push(text(format!("$ {}", look::money(c)), ui, t.ink, false));
    }
    if let Some((up, bars)) = st.net {
        right.push(if up {
            Node::Bars {
                n: bars.min(4),
                of: 4,
                w: 16,
                h: 13,
                rgb: t.ink,
            }
        } else {
            icon(Icon::Wifi, 15, t.error_ink)
        });
    }
    if let Some((pct, charging)) = st.battery {
        let rgb = if !charging && pct < 30 {
            t.error_ink
        } else {
            t.ink
        };
        right.push(Node::Gauge {
            permille: pct.min(100) as u16 * 10,
            w: 26,
            h: 13,
            rgb,
        });
    }
    let clock = match st.clock {
        Some((h, m)) => format!("{h:02}:{m:02}"),
        None => "--:--".into(),
    };
    let mut items = vec![text(clock, t.px(26), t.ink, true), Node::Spacer(0)];
    items.push(Node::Row {
        gap: 10,
        items: right,
    });
    Node::Row { gap: 8, items }
}

/// The keys at the bottom, wrapped between pairs to the panel's width.
fn footer(keys: &[(String, String)], t: &Theme, cols: usize) -> Node {
    let mut lines: Vec<String> = vec![String::new()];
    for (k, w) in keys {
        let part = format!("{k} {w}");
        let cur = lines.last_mut().expect("one line");
        if cur.is_empty() {
            *cur = part;
        } else if cur.chars().count() + 3 + part.chars().count() <= cols {
            *cur += " · ";
            *cur += &part;
        } else {
            lines.push(part);
        }
    }
    Node::Column {
        gap: 2,
        items: lines
            .into_iter()
            .map(|l| centred(text(l, t.px(PX_SMALL - 1), t.soft, false)))
            .collect(),
    }
}

/// The home grid: tiles in 3 columns, the number small, the badge.
fn home(ph: &Phone, inner_w: i32, t: &Theme) -> Node {
    let apps = ph.apps();
    let cell_w = (inner_w - 2 * 6) / COLS as i32;
    let mut rows = Vec::new();
    for (r, chunk) in apps.chunks(COLS).enumerate() {
        let mut cells = Vec::new();
        for (c, a) in chunk.iter().enumerate() {
            let i = r * COLS + c;
            let tint = TINTS[(a.order.max(1) as usize - 1) % TINTS.len()];
            let num = if i < 9 {
                text((i + 1).to_string(), t.px(12), t.soft, false)
            } else {
                gap(1, 1)
            };
            let badge = match a.badge {
                Some(n) if n > 0 => chip(n.to_string(), t),
                _ => gap(1, 1),
            };
            let cell = Node::Fixed {
                w: cell_w - 2 * crate::ui::layout::SEL_PAD.0,
                h: 0,
                child: Box::new(Node::Column {
                    gap: 4,
                    items: vec![
                        // a fixed height: a badge does not push its tile down
                        Node::Fixed {
                            w: 0,
                            h: 22,
                            child: Box::new(Node::Row {
                                gap: 0,
                                items: vec![num, Node::Spacer(0), badge],
                            }),
                        },
                        centred(tile(a.icon, tint, 54)),
                        centred(text(a.title, t.px(PX_SMALL), t.ink, i == ph.st.sel)),
                    ],
                }),
            };
            cells.push(selectable(i == ph.st.sel, cell));
        }
        rows.push(Node::Row {
            gap: 6,
            items: cells,
        });
    }
    if rows.is_empty() {
        rows.push(centred(text("nenhum app", t.px(PX_UI), t.soft, false)));
    }
    Node::Column {
        gap: 8,
        items: rows,
    }
}

/// A switch: a pill with the knob at its end.
fn switch(on: bool, t: &Theme) -> Node {
    let bg = if on { [0x6c, 0xb8, 0x84] } else { t.select };
    let knob = Node::Panel {
        radius: 8,
        bg: WHITE,
        alpha: 255,
        pad: 0,
        child: Box::new(gap(14, 14)),
    };
    let items = if on {
        vec![Node::Spacer(0), knob]
    } else {
        vec![knob, Node::Spacer(0)]
    };
    Node::Fixed {
        w: 38,
        h: 0,
        child: Box::new(Node::Panel {
            radius: 10,
            bg,
            alpha: 255,
            pad: 3,
            child: Box::new(Node::Row { gap: 0, items }),
        }),
    }
}

/// One list row: icon, title, detail under it, badge or switch at the end.
fn row(it: &Item, sel: bool, t: &Theme) -> Node {
    let mut items = Vec::new();
    match (it.icon, it.rgb) {
        (Some(k), Some(rgb)) => items.push(tile(k, rgb, 34)),
        (Some(k), None) => items.push(icon(k, 20, t.ink)),
        (None, Some(rgb)) => items.push(Node::Panel {
            radius: 6,
            bg: rgb,
            alpha: 255,
            pad: 0,
            child: Box::new(gap(12, 12)),
        }),
        (None, None) => {}
    }
    let mut col = vec![text(it.title.clone(), t.px(PX_UI), t.ink, sel)];
    if let Some(d) = &it.detail {
        col.push(text(d.clone(), t.px(PX_SMALL - 1), t.soft, false));
    }
    // the text stretches: measured last, so a long detail is cut and the
    // badge/switch at the end always fit
    items.push(Node::Row {
        gap: 0,
        items: vec![Node::Column { gap: 1, items: col }, Node::Spacer(0)],
    });
    if let Some(b) = &it.badge {
        items.push(badge(b.clone(), t));
    }
    if let Some(on) = it.toggle {
        items.push(switch(on, t));
    }
    selectable(sel, Node::Row { gap: 10, items })
}

/// The search line: glyph, the query (or its placeholder), the cursor.
fn search_line(ph: &Phone, placeholder: &str, t: &Theme) -> Node {
    let ui = t.px(PX_UI);
    let q = &ph.line.text;
    let body = if q.is_empty() && !ph.typing {
        text(placeholder, ui, t.soft, false)
    } else if ph.typing {
        let c = ph.line.cursor.min(q.len());
        text(format!("{}▏{}", &q[..c], &q[c..]), ui, t.ink, false)
    } else {
        text(q.clone(), ui, t.ink, false)
    };
    let field = Node::Panel {
        radius: 12,
        bg: t.select,
        alpha: 255,
        pad: 6,
        child: Box::new(Node::Row {
            gap: 8,
            items: vec![
                gap(2, 1),
                icon(Icon::Search, 16, t.soft),
                body,
                Node::Spacer(0),
            ],
        }),
    };
    if ph.typing {
        Node::Selected {
            ring: true,
            child: Box::new(field),
        }
    } else {
        crate::ui::plain(field)
    }
}

/// The whole phone at the output's logical size `out` (w, h). Measures
/// its parts with `fonts` to fit the list, and moves `ph.st.scroll`.
pub fn node(
    ph: &mut Phone,
    status: &Status,
    out: (i32, i32),
    t: &Theme,
    fonts: &mut Fonts,
) -> Node {
    ph.clamp();
    let h = H.min(out.1 - 80).max(240);
    let shadow = crate::ui::layout::SHADOW;
    let inner = Size {
        w: W - 2 * PAD,
        h: h - 2 * PAD - shadow,
    };
    let big = Size {
        w: inner.w,
        h: 4000,
    };
    let head = header(status, t);
    let cols = {
        let cw = fonts.face(t.px(PX_SMALL - 1)).cw as i32;
        (inner.w / cw.max(1)).max(10) as usize
    };
    let mut keys: Vec<(String, String)> = Vec::new();
    let mut body: Vec<Node> = Vec::new();
    let mut title_bar: Option<Node> = None;
    match ph.st.app.clone() {
        None => {
            body.push(home(ph, inner.w, t));
            keys.push(("1–9".into(), "app".into()));
            keys.push(("enter".into(), "abre".into()));
        }
        Some(app) => {
            let page = ph.page().cloned().unwrap_or_else(|| PhonePage {
                title: app.clone(),
                empty: Some("carregando…".into()),
                ..PhonePage::default()
            });
            title_bar = Some(Node::Row {
                gap: 8,
                items: vec![
                    icon(Icon::Arrow, 18, t.soft),
                    text(page.title.clone(), t.px(PX_UI + 2), t.ink, true),
                    Node::Spacer(0),
                    if page.search.is_some() {
                        icon(Icon::Search, 18, t.soft)
                    } else {
                        gap(1, 1)
                    },
                ],
            });
            if let Some(hd) = &page.header {
                body.push(hd.clone());
            }
            if let Some(s) = &page.search {
                body.push(search_line(ph, &s.placeholder, t));
            }
            keys.extend(page.keys.iter().cloned());
            if page.search.is_some() {
                keys.push(("/".into(), "busca".into()));
            }
            keys.push(("⌫".into(), "volta".into()));
            // the list fits what is left
            let fixed: i32 = {
                let mut parts = vec![head.clone(), footer(&keys, t, cols)];
                parts.extend(title_bar.clone());
                parts.extend(body.iter().cloned());
                parts.iter().map(|p| measure(p, big, fonts, t).h + 10).sum()
            };
            let list_h = (inner.h - fixed - 12).max(40);
            let items: Vec<Item> = ph.shown().into_iter().cloned().collect();
            if items.is_empty() {
                let msg = page.empty.clone().unwrap_or_else(|| "nada aqui".into());
                body.push(gap(1, 12));
                body.push(centred(text(msg, t.px(PX_UI), t.soft, false)));
            } else {
                let row_h = items
                    .iter()
                    .map(|i| measure(&row(i, false, t), big, fonts, t).h)
                    .max()
                    .unwrap_or(30)
                    + 4;
                let fit = (list_h / row_h.max(1)).max(1) as usize;
                ph.scroll_to(fit);
                let (sel, scroll) = (ph.st.sel, ph.st.scroll);
                let rows: Vec<Node> = items
                    .iter()
                    .enumerate()
                    .skip(scroll)
                    .take(fit)
                    .map(|(i, it)| row(it, i == sel, t))
                    .collect();
                let list = Node::Column {
                    gap: 4,
                    items: rows,
                };
                if items.len() > fit {
                    // a thin indicator at the right
                    let track = list_h.min(fit as i32 * row_h);
                    let len = (track * fit as i32 / items.len() as i32).max(16);
                    let top = (track - len) * scroll as i32 / (items.len() - fit).max(1) as i32;
                    let bar = Node::Column {
                        gap: 0,
                        items: vec![
                            gap(1, top.max(0)),
                            Node::Panel {
                                radius: 2,
                                bg: t.soft,
                                alpha: 160,
                                pad: 0,
                                child: Box::new(gap(4, len)),
                            },
                        ],
                    };
                    body.push(Node::Row {
                        gap: 4,
                        items: vec![
                            Node::Fixed {
                                w: inner.w - 8,
                                h: 0,
                                child: Box::new(list),
                            },
                            bar,
                        ],
                    });
                } else {
                    body.push(list);
                }
            }
        }
    }
    keys.push(("esc".into(), "fecha".into()));
    let mut col = vec![head];
    col.push(Node::Rule { rgb: t.select });
    col.extend(title_bar);
    col.extend(body);
    col.push(Node::Spacer(0));
    col.push(footer(&keys, t, cols));
    Node::Panel {
        radius: RADIUS,
        bg: t.cream,
        alpha: 255,
        pad: PAD,
        child: Box::new(Node::Fixed {
            w: inner.w,
            h: inner.h,
            child: Box::new(Node::Column {
                gap: 10,
                items: col,
            }),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::phone::SearchSpec;

    fn app(id: &'static str, order: u8) -> PhoneApp {
        PhoneApp {
            id,
            title: id,
            icon: Icon::Cat,
            order,
            badge: None,
        }
    }

    fn item(id: &str, on_enter: Option<Act>) -> Item {
        Item {
            id: id.into(),
            title: id.into(),
            on_enter,
            ..Item::default()
        }
    }

    fn view(page: Option<PhonePage>) -> PhoneView {
        PhoneView {
            state: PhoneState::default(),
            apps: vec![
                app("ajustes", 9),
                app("gatos", 1),
                app("inbox", 2),
                app("mapa", 3),
                app("musica", 6),
            ],
            page,
        }
    }

    fn page() -> PhonePage {
        PhonePage {
            title: "Gatos".into(),
            items: vec![
                item("acme", Some(Act::new(ACT_SUB, &[("sub", "7")], false))),
                item("bravo", Some(Act::new("world.talk", &[("cat", "8")], true))),
                Item {
                    on_keys: vec![("x".into(), Act::new("session.stop", &[], false))],
                    ..item("demo", None)
                },
            ],
            search: Some(SearchSpec {
                placeholder: "buscar".into(),
                live: true,
                run: None,
            }),
            ..PhonePage::default()
        }
    }

    fn cmd_names(r: &[Req]) -> Vec<String> {
        r.iter()
            .map(|r| match r {
                Req::Command { name, .. } => name.clone(),
                Req::Action(a) => format!("{a:?}"),
                _ => "?".into(),
            })
            .collect()
    }

    /// home → app → sub → back → back → close; numbers; the grid.
    #[test]
    fn the_state_machine() {
        let mut p = Phone::new(None);
        p.view = Some(view(None));
        assert_eq!(
            p.apps().iter().map(|a| a.id).collect::<Vec<_>>(),
            ["gatos", "inbox", "mapa", "musica", "ajustes"]
        );
        assert_eq!(p.count(), 5);
        p.nav(Dir::Right);
        p.nav(Dir::Down);
        assert_eq!(p.st.sel, 4, "3 columns: right then down");
        p.nav(Dir::Down);
        assert_eq!(p.st.sel, 4, "no row below");
        p.nav(Dir::Up);
        p.nav(Dir::Left);
        assert_eq!(p.st.sel, 0);
        p.nav(Dir::Left);
        assert_eq!(p.st.sel, 0);
        assert!(p.enter().is_empty());
        assert_eq!(p.st.app.as_deref(), Some("gatos"));
        p.view = Some(view(Some(page())));
        assert_eq!(p.count(), 3);
        assert!(p.enter().is_empty(), "phone.sub is the phone's own");
        assert_eq!(p.st.sub.as_deref(), Some("7"));
        assert!(p.back().is_empty());
        assert_eq!(
            (p.st.app.as_deref(), p.st.sub.as_deref()),
            (Some("gatos"), None)
        );
        assert!(p.back().is_empty());
        assert_eq!(p.st.app, None);
        assert_eq!(p.st.sel, 0, "home, on the app it came from");
        assert_eq!(cmd_names(&p.back()), ["Back"], "home: back closes");
        // numbers: 4 = musica; 3 = mapa (an action: closes, opens the map)
        assert!(p.app_n(4).is_empty());
        assert_eq!(p.st.app.as_deref(), Some("musica"));
        assert_eq!(cmd_names(&p.app_n(3)), ["Back", "world.overview"]);
        assert!(p.app_n(99).is_empty());
        assert_eq!(cmd_names(&p.close()), ["Back"]);
    }

    #[test]
    fn acts_close_emits_back_first_and_on_keys_win() {
        let mut p = Phone::new(Some("gatos".into()));
        p.view = Some(view(Some(page())));
        p.nav(Dir::Down);
        let r = p.enter();
        assert_eq!(cmd_names(&r), ["Back", "world.talk"]);
        assert!(matches!(&r[1], Req::Command { args, .. } if args["cat"] == "8"));
        p.nav(Dir::Down);
        assert_eq!(p.selected().unwrap().id, "demo");
        assert_eq!(
            cmd_names(&p.text("x", Some("x")).unwrap()),
            ["session.stop"]
        );
        assert!(p.text("q", Some("q")).is_none(), "not its key: the table's");
        assert!(p.enter().is_empty(), "no on_enter");
    }

    #[test]
    fn live_search_filters_and_run_sends_the_query() {
        let mut p = Phone::new(Some("gatos".into()));
        p.view = Some(view(Some(page())));
        p.nav(Dir::Down);
        p.search();
        assert!(p.typing);
        for c in ["b", "r", "a"] {
            assert_eq!(p.text(c, Some(c)), Some(vec![]));
        }
        assert_eq!(p.st.query, "bra");
        assert_eq!(p.count(), 1);
        assert_eq!(p.selected().unwrap().id, "bravo");
        assert_eq!(p.st.sel, 0, "the selection restarts");
        p.back();
        assert_eq!(p.st.query, "br");
        assert!(p.close().is_empty(), "esc lets the line go first");
        assert!(!p.typing);
        assert_eq!(cmd_names(&p.close()), ["Back"]);
        // run: enter in the line sends the query
        let mut q = Phone::new(Some("biblioteca".into()));
        q.forced = Some(PhonePage {
            title: "Biblioteca".into(),
            search: Some(SearchSpec {
                placeholder: String::new(),
                live: false,
                run: Some("world.library_search".into()),
            }),
            ..PhonePage::default()
        });
        q.view = Some(view(None));
        q.search();
        q.text("n", Some("n"));
        q.text("o", Some("o"));
        let r = q.enter();
        assert!(
            matches!(&r[0], Req::Command { name, args } if name == "world.library_search" && args["q"] == "no")
        );
        assert!(!q.typing);
        // a page without search: `/` does nothing
        let mut z = Phone::new(Some("musica".into()));
        z.forced = Some(PhonePage::default());
        z.search();
        assert!(!z.typing);
    }

    #[test]
    fn selection_follows_sel_and_scroll() {
        let mut p = Phone::new(Some("gatos".into()));
        let many = PhonePage {
            items: (0..30).map(|i| item(&format!("c{i}"), None)).collect(),
            ..PhonePage::default()
        };
        p.view = Some(view(Some(many)));
        for _ in 0..12 {
            p.nav(Dir::Down);
        }
        p.scroll_to(8);
        assert_eq!((p.st.sel, p.st.scroll), (12, 5));
        for _ in 0..12 {
            p.nav(Dir::Up);
        }
        p.scroll_to(8);
        assert_eq!((p.st.sel, p.st.scroll), (0, 0));
        // the drawn tree has exactly one Selected row, the selected one
        let t = Theme::cozy();
        let mut f = Fonts::new();
        p.st.sel = 3;
        let n = node(&mut p, &Status::default(), (1280, 720), &t, &mut f);
        let dbg = format!("{n:?}");
        assert_eq!(dbg.matches("Selected").count(), 1);
        let sel_at = dbg.find("Selected").unwrap();
        assert!(dbg[sel_at..].contains("\"c3\""));
        // the tree is the phone's size
        let sz = measure(&n, Size { w: 1280, h: 720 }, &mut f, &t);
        assert_eq!(sz.w, W);
        assert_eq!(sz.h, H.min(720 - 80));
    }

    /// Rule 15 (W6's ask): the dump never carries an item's title nor the
    /// page's header; the query only under `--demo`.
    #[test]
    fn the_dump_never_prints_titles_or_the_header() {
        let mut p = Phone::new(Some("biblioteca".into()));
        p.forced = Some(PhonePage {
            title: "Biblioteca".into(),
            header: Some(text("faixa-secreta", 16, [0; 3], false)),
            items: vec![Item {
                id: "hit:0".into(),
                title: "arquivo-secreto.md".into(),
                detail: Some("pasta-secreta".into()),
                badge: Some("selo-secreto".into()),
                ..Item::default()
            }],
            search: Some(SearchSpec {
                placeholder: String::new(),
                live: true,
                run: None,
            }),
            ..PhonePage::default()
        });
        p.st.query = "segredo".into();
        p.st.query.clear();
        p.view = Some(view(None));
        let real = p.dump(false).to_string();
        assert!(!real.contains("secret"), "{real}");
        assert!(real.contains("hit:0") && real.contains("Biblioteca"));
        p.search();
        for c in ["a", "r", "q"] {
            p.text(c, Some(c));
        }
        let real = p.dump(false);
        assert_eq!(real["query"]["chars"], 3);
        assert!(!real.to_string().contains("\"arq\""));
        assert_eq!(p.dump(true)["query"], "arq", "--demo: the fixture's query");
        assert!(!p.dump(true).to_string().contains("secret"));
    }

    #[test]
    fn a_page_change_changes_the_tree() {
        let t = Theme::cozy();
        let mut f = Fonts::new();
        let mut p = Phone::new(Some("gatos".into()));
        p.view = Some(view(Some(page())));
        let a = node(&mut p, &Status::default(), (1280, 720), &t, &mut f);
        let mut pg = page();
        pg.items[0].detail = Some("dormindo · acme".into());
        p.view = Some(view(Some(pg)));
        let b = node(&mut p, &Status::default(), (1280, 720), &t, &mut f);
        assert_ne!(a, b);
    }
}
