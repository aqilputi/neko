//! The overlay's rasters (§5.4): every 2D piece is a [`Spec`] (plain data,
//! hashed for the cache) turned into premultiplied RGBA with `nk-cats`'
//! fontdue text and anti-aliased rounded boxes. Sizes are logical px times
//! the raster scale `s` ([`Px`]: integer, or the output's fractional scale
//! so the text is drawn at physical px). Pure CPU, no Smithay.

use std::collections::HashMap;

use nk_cats::raster::{Canvas, Face};

pub type Rgb = [u8; 3];

/// The AC palette of the overlays.
pub const CREAM: Rgb = [0xf6, 0xec, 0xd8];
pub const INK: Rgb = [0x5a, 0x3e, 0x2b];
pub const SOFT: Rgb = [0x6e, 0x56, 0x44];
pub const ERROR_BG: Rgb = [0xf4, 0xcf, 0xc4];
pub const ERROR_INK: Rgb = [0x7a, 0x2a, 0x1e];
pub const HINT_BG: Rgb = [0xff, 0xd8, 0x6a];
pub const HINT_TYPED: Rgb = [0xc0, 0x8a, 0x2a];
pub const SELECT: Rgb = [0xe8, 0xd5, 0xb0];
pub const MAP_BG: Rgb = [0xe9, 0xe0, 0xc8];
pub const MAP_ROOM: Rgb = [0xfb, 0xf6, 0xea];
pub const MAP_DESK: Rgb = [0xc9, 0xa2, 0x7c];
pub const MAP_CAT: Rgb = [0xf0, 0xa8, 0x5a];
pub const MAP_SCREEN: Rgb = [0x7f, 0xa8, 0xc9];
pub const MAP_SHELF: Rgb = [0x9c, 0x6b, 0x45];
pub const RING: Rgb = [0xff, 0x9f, 0x43];

/// Font sizes, logical px.
pub const PX_SMALL: u32 = 14;
pub const PX_UI: u32 = 16;
pub const PX_DIALOG: u32 = 20;

/// The dialogue box's text lines.
pub const DIALOG_LINES: usize = 3;

/// The raster scale: buffer px per logical px. Integer at 1× and 2× (the
/// buffer carries that scale), the output's own at fractional scales (the
/// overlay rasters at physical px, `OverlayElem.buf_scale`), ¼ for the
/// vignette (smooth: the compositor upsamples it).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Px(pub f32);

impl Px {
    /// `n` logical px in buffer px.
    pub fn p(self, n: i32) -> i32 {
        (n as f32 * self.0).round() as i32
    }

    /// A font size in buffer px (`round(px·scale)`).
    pub fn font(self, px: u32) -> u32 {
        (px as f32 * self.0).round().max(1.0) as u32
    }

    /// The cache's key for it (two scales that round to the same 1/120
    /// are the same raster).
    pub fn key(self) -> i32 {
        (self.0 * 120.0).round() as i32
    }
}

/// The crosshair's box, logical px (the dot/ring is centred in it).
pub const CROSS_BOX: i32 = 20;
/// Dot → ring in this many raster steps (80 ms).
pub const CROSS_STEPS: u8 = 4;
/// The ring's radius and stroke, the dot's diameter, logical px.
pub const RING_R: f32 = 6.0;
pub const RING_W: f32 = 2.0;
pub const DOT_D: f32 = 5.0;
/// The vignette's raster scale and its darkest alpha at the corners.
pub const VIGNETTE_SCALE: f32 = 0.25;
pub const VIGNETTE_MAX: f32 = 0.7;
pub const VIGNETTE_INK: Rgb = [0x1c, 0x12, 0x0b];

/// A raster: size in buffer px and premultiplied RGBA.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Raster {
    pub w: u32,
    pub h: u32,
    pub px: Vec<u8>,
}

/// The faces the overlay draws with, by pixel size (JetBrains Mono, the
/// only font this wave; the rounded UI font is phase 5).
pub struct Fonts {
    /// makes a face of the loaded font (keeps fontdue out of nkw's deps)
    make: Box<dyn Fn(f32) -> Face>,
    faces: HashMap<u32, Face>,
}

impl Default for Fonts {
    fn default() -> Fonts {
        Fonts::new()
    }
}

impl Fonts {
    pub fn new() -> Fonts {
        let font = Face::load_font();
        Fonts {
            make: Box::new(move |px| Face::new(&font, px)),
            faces: HashMap::new(),
        }
    }

    pub fn face(&mut self, px: u32) -> &mut Face {
        let make = &self.make;
        self.faces.entry(px).or_insert_with(|| make(px as f32))
    }
}

/// One palette row: name, title, keys.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Row {
    pub name: String,
    pub title: String,
    pub keys: String,
}

/// A box of the map, logical px inside the map raster.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct MapBox {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub kind: MapKind,
    pub label: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MapKind {
    Hall,
    Room,
    /// the room you are in (floor view)
    Here,
    Desk,
    Screen,
    Cat,
    /// a library shelf against a wall
    Shelf,
}

/// What to rasterise. Equal specs give equal pixels (the cache's key).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Spec {
    /// the mode label (§3.1): cream badge, brown ink
    Label { text: String },
    /// a HUD badge: glyph + number chip
    Chip { text: String, bg: Rgb, fg: Rgb },
    /// a toast, bottom-left
    Toast { text: String, error: bool },
    /// the AC dialogue box: name plate, text, ▼, the reply line
    Dialog {
        width: i32,
        name: String,
        rgb: Rgb,
        lines: Vec<String>,
        arrow: bool,
        line: Option<(String, usize)>,
    },
    /// the choices box at the right of the dialogue
    Choices { items: Vec<String>, selected: usize },
    /// COMANDOS: the line and the ranked list under it
    Palette {
        width: i32,
        prompt: Option<String>,
        query: String,
        cursor: usize,
        rows: Vec<Row>,
        selected: usize,
    },
    /// a SALTO label; the first `typed` letters are dimmed
    Hint { label: String, typed: usize },
    /// a name tag in the world
    Tag { text: String, rgb: Rgb },
    /// the map's sheet
    Map {
        w: i32,
        h: i32,
        title: String,
        help: String,
        boxes: Vec<MapBox>,
    },
    /// the map's highlight ring
    Ring { w: i32, h: i32 },
    /// ANDAR's crosshair: `t` = 0 the dot … [`CROSS_STEPS`] the ring
    Crosshair { t: u8 },
    /// the comfort vignette over the whole output (logical px), alpha in
    /// sixteenths (1..=16)
    Vignette { w: i32, h: i32, a16: u8 },
    /// a cream hint bar (the "na mão" keys)
    Bar { text: String },
    /// `of` at `level`/[`FADE_STEPS`] opacity (tags fading in and out)
    Faded { level: u8, of: Box<Spec> },
    /// a toolkit tree (S21) inside `w`×`h` logical px, drawn in `theme`;
    /// the raster is the tree's measured size (cache key: the tree)
    Ui {
        node: crate::ui::Node,
        w: i32,
        h: i32,
        theme: crate::ui::Theme,
    },
}

/// A fade is drawn in this many opacity steps (rasters cached per step).
pub const FADE_STEPS: u8 = 4;

/// `spec` at `level` of [`FADE_STEPS`]: itself when opaque.
pub fn faded(spec: Spec, level: u8) -> Spec {
    if level >= FADE_STEPS {
        spec
    } else {
        Spec::Faded {
            level,
            of: Box::new(spec),
        }
    }
}

impl Spec {
    /// The raster scale this spec draws at, given the output's.
    pub fn scale(&self, s: Px) -> Px {
        match self {
            Spec::Vignette { .. } => Px(VIGNETTE_SCALE),
            Spec::Faded { of, .. } => of.scale(s),
            _ => s,
        }
    }
}

/// The text colour that reads on `bg` (contrast ≥ 4.5:1 for the palette).
pub fn ink_on(bg: Rgb) -> Rgb {
    let l = 0.2126 * bg[0] as f32 + 0.7152 * bg[1] as f32 + 0.0722 * bg[2] as f32;
    if l > 150.0 { INK } else { [0xff, 0xfb, 0xf2] }
}

/// Split `text` into lines of at most `cols` cells, at spaces when it can
/// (a longer word is cut), keeping explicit newlines.
pub fn wrap(text: &str, cols: usize) -> Vec<String> {
    let cols = cols.max(1);
    let mut out = Vec::new();
    for para in text.split('\n') {
        let mut line = String::new();
        let mut n = 0;
        for word in para.split(' ') {
            let w = word.chars().count();
            if n > 0 && n + 1 + w > cols {
                out.push(std::mem::take(&mut line));
                n = 0;
            }
            if n > 0 {
                line.push(' ');
                n += 1;
            }
            for c in word.chars() {
                if n >= cols {
                    out.push(std::mem::take(&mut line));
                    n = 0;
                }
                line.push(c);
                n += 1;
            }
        }
        out.push(line);
    }
    out
}

/// Cut `s` to `cols` cells with an ellipsis.
pub fn ellipsize(s: &str, cols: usize) -> String {
    if s.chars().count() <= cols {
        return s.to_string();
    }
    let mut t: String = s.chars().take(cols.saturating_sub(1)).collect();
    t.push('…');
    t
}

fn done(c: Canvas) -> Raster {
    let (w, h, px) = c.into_rgba();
    Raster {
        w: w as u32,
        h: h as u32,
        px,
    }
}

/// A pill of text: `pad` around, radius `r` (all logical px × `s`).
#[allow(clippy::too_many_arguments)]
fn pill(
    fonts: &mut Fonts,
    s: Px,
    text: &str,
    px: u32,
    bg: Rgb,
    fg: Rgb,
    pad: (i32, i32),
    r: i32,
) -> Raster {
    let face = fonts.face(s.font(px));
    let tw = Canvas::text_width(face, text);
    let (w, h) = (tw + 2 * s.p(pad.0), face.ch as i32 + 2 * s.p(pad.1));
    let mut c = Canvas::transparent(w.max(1) as usize, h.max(1) as usize);
    c.rounded_rect(0, 0, w, h, s.p(r), bg);
    c.text_px(face, s.p(pad.0), s.p(pad.1), text, fg, false);
    done(c)
}

/// Rasterise `spec` at integer scale `s`.
pub fn raster(spec: &Spec, fonts: &mut Fonts, s: Px) -> Raster {
    match spec {
        Spec::Label { text } => pill(fonts, s, text, PX_UI, CREAM, INK, (10, 4), 6),
        Spec::Chip { text, bg, fg } => pill(fonts, s, text, PX_SMALL, *bg, *fg, (8, 3), 10),
        Spec::Tag { text, rgb } => pill(fonts, s, text, PX_SMALL, *rgb, ink_on(*rgb), (7, 2), 9),
        Spec::Toast { text, error } => {
            let (bg, fg) = if *error {
                (ERROR_BG, ERROR_INK)
            } else {
                (CREAM, INK)
            };
            pill(fonts, s, &ellipsize(text, 60), PX_UI, bg, fg, (14, 8), 14)
        }
        Spec::Hint { label, typed } => {
            let face = fonts.face(s.font(PX_UI));
            let tw = Canvas::text_width(face, label);
            let (w, h) = (tw + s.p(8), face.ch as i32 + s.p(2));
            let mut c = Canvas::transparent(w as usize, h as usize);
            if *typed > 0 {
                // being typed: the one selection look (S21)
                c.rounded_rect(0, 0, w, h, s.p(5), SELECT);
                c.rounded_rect_ring(0, 0, w, h, s.p(5), s.p(2).max(2), RING);
            } else {
                c.rounded_rect(0, 0, w, h, s.p(5), INK);
                c.rounded_rect(s.p(1), s.p(1), w - s.p(2), h - s.p(2), s.p(4), HINT_BG);
            }
            let mut x = s.p(4);
            for (i, ch) in label.chars().enumerate() {
                let fg = if i < *typed { HINT_TYPED } else { INK };
                x += c.text_px(face, x, s.p(1), &ch.to_string(), fg, true);
            }
            done(c)
        }
        Spec::Dialog {
            width,
            name,
            rgb,
            lines,
            arrow,
            line,
        } => dialog(fonts, s, *width, name, *rgb, lines, *arrow, line.as_ref()),
        Spec::Choices { items, selected } => {
            let face = fonts.face(s.font(PX_DIALOG));
            let (cw, ch) = (face.cw as i32, face.ch as i32);
            let cols = items
                .iter()
                .map(|i| i.chars().count() + 4)
                .max()
                .unwrap_or(4) as i32;
            let (pad, row) = (s.p(14), ch + s.p(6));
            let w = cols * cw + 2 * pad;
            let h = items.len() as i32 * row + 2 * pad - s.p(6);
            let mut c = Canvas::transparent(w.max(1) as usize, h.max(1) as usize);
            c.rounded_rect(0, 0, w, h, s.p(18), CREAM);
            for (i, it) in items.iter().enumerate() {
                let y = pad + i as i32 * row;
                let sel = i == *selected;
                if sel {
                    c.rounded_rect(pad / 2, y - s.p(2), w - pad, ch + s.p(4), s.p(8), SELECT);
                    c.rounded_rect_ring(
                        pad / 2,
                        y - s.p(2),
                        w - pad,
                        ch + s.p(4),
                        s.p(8),
                        s.p(2).max(2),
                        RING,
                    );
                }
                let mark = if sel { "▶" } else { " " };
                let txt = format!("{mark} {} {it}", i + 1);
                c.text_px(face, pad, y, &txt, if sel { INK } else { SOFT }, sel);
            }
            done(c)
        }
        Spec::Palette {
            width,
            prompt,
            query,
            cursor,
            rows,
            selected,
        } => palette(
            fonts,
            s,
            *width,
            prompt.as_deref(),
            query,
            *cursor,
            rows,
            *selected,
        ),
        Spec::Map {
            w,
            h,
            title,
            help,
            boxes,
        } => map(fonts, s, *w, *h, title, help, boxes),
        Spec::Ring { w, h } => {
            // the one selection look on the map: the RING ring with the
            // SELECT colour as its halo (a fill would hide the target)
            let (w, h) = (s.p(*w), s.p(*h));
            let mut c = Canvas::transparent(w.max(1) as usize, h.max(1) as usize);
            let k = s.p(2).max(2);
            c.rounded_rect_ring(0, 0, w, h, s.p(9), k, SELECT);
            c.rounded_rect_ring(k, k, w - 2 * k, h - 2 * k, s.p(7), k, RING);
            done(c)
        }
        Spec::Crosshair { t } => {
            let side = s.p(CROSS_BOX).max(1);
            let k = (*t).min(CROSS_STEPS) as f32 / CROSS_STEPS as f32;
            let r = RING_R * k * s.0;
            let w = (DOT_D + (RING_W - DOT_D) * k) * s.0;
            let mut c = Canvas::transparent(side as usize, side as usize);
            let m = side as f32 / 2.0;
            // an ink halo so it reads on cream walls and on dark screens
            c.ring(m, m, r, w + 2.0 * s.0, INK);
            c.ring(m, m, r, w, CREAM);
            done(c)
        }
        Spec::Vignette { w, h, a16 } => {
            let (bw, bh) = (
                (*w as f32 * s.0).ceil().max(1.0) as usize,
                (*h as f32 * s.0).ceil().max(1.0) as usize,
            );
            let mut c = Canvas::transparent(bw, bh);
            let a = (*a16).min(16) as f32 / 16.0 * VIGNETTE_MAX * 255.0;
            c.radial_alpha(VIGNETTE_INK, a.round() as u8, 0.55, 1.35);
            done(c)
        }
        Spec::Bar { text } => pill(fonts, s, text, PX_UI, CREAM, INK, (14, 8), 14),
        Spec::Ui { node, w, h, theme } => {
            let avail = crate::ui::Size {
                w: s.p(*w),
                h: s.p(*h),
            };
            done(crate::ui::paint::render(node, avail, fonts, theme, s))
        }
        Spec::Faded { level, of } => {
            let mut r = raster(of, fonts, s);
            // premultiplied: opacity scales every channel alike
            let k = (*level).min(FADE_STEPS) as u32;
            let n = FADE_STEPS as u32;
            for v in &mut r.px {
                *v = ((*v as u32 * k + n / 2) / n) as u8;
            }
            r
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn dialog(
    fonts: &mut Fonts,
    s: Px,
    width: i32,
    name: &str,
    rgb: Rgb,
    lines: &[String],
    arrow: bool,
    line: Option<&(String, usize)>,
) -> Raster {
    let (pcw, pch) = {
        let f = fonts.face(s.font(PX_UI));
        (f.cw as i32, f.ch as i32)
    };
    let face = fonts.face(s.font(PX_DIALOG));
    let (ch, cw) = (face.ch as i32, face.cw as i32);
    let plate_h = pch + s.p(10);
    let over = plate_h / 2;
    let pad = s.p(24);
    let rows = DIALOG_LINES as i32 + i32::from(line.is_some());
    let w = s.p(width);
    let bh = rows * ch + 2 * pad;
    let h = bh + over;
    let mut c = Canvas::transparent(w.max(1) as usize, h.max(1) as usize);
    // a soft shadow, then the cream box
    c.rounded_rect_alpha(0, over + s.p(4), w, bh - s.p(4), s.p(28), INK, 40);
    c.rounded_rect(0, over, w, bh - s.p(4), s.p(28), CREAM);
    for (i, l) in lines.iter().take(DIALOG_LINES).enumerate() {
        c.text_px(face, pad, over + pad + i as i32 * ch, l, INK, false);
    }
    if let Some((text, cursor)) = line {
        let y = over + pad + DIALOG_LINES as i32 * ch;
        c.rounded_rect(
            pad - s.p(8),
            y - s.p(2),
            w - 2 * pad + s.p(16),
            ch + s.p(4),
            s.p(8),
            SELECT,
        );
        let cols = ((w - 2 * pad) / cw - 3).max(1) as usize;
        let shown: String = text
            .chars()
            .rev()
            .take(cols)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        let x = pad + c.text_px(face, pad, y, "› ", SOFT, false);
        c.text_px(face, x, y, &shown, INK, false);
        let skipped = text.chars().count() - shown.chars().count();
        let at = cursor.saturating_sub(skipped) as i32;
        c.rect(x + at * cw, y + s.p(2), s.p(2), ch - s.p(4), INK);
    }
    if arrow {
        let ax = w - pad - cw;
        let ay = over + bh - pad - ch / 2;
        c.text_px(face, ax, ay, "▼", [0xd0, 0x80, 0x40], false);
    }
    // the name plate, astride the box's top edge
    let pw = (name.chars().count() as i32 + 2) * pcw;
    let pf = fonts.face(s.font(PX_UI));
    c.rounded_rect(pad, 0, pw, plate_h, plate_h / 2, rgb);
    c.text_px(pf, pad + pcw, s.p(5), name, ink_on(rgb), true);
    done(c)
}

#[allow(clippy::too_many_arguments)]
fn palette(
    fonts: &mut Fonts,
    s: Px,
    width: i32,
    prompt: Option<&str>,
    query: &str,
    cursor: usize,
    rows: &[Row],
    selected: usize,
) -> Raster {
    let face = fonts.face(s.font(PX_UI));
    let (cw, ch) = (face.cw as i32, face.ch as i32);
    let pad = s.p(14);
    let row = ch + s.p(6);
    let w = s.p(width);
    let list = if rows.is_empty() {
        row
    } else {
        rows.len() as i32 * row
    };
    let h = pad + row + s.p(8) + list + pad;
    let mut c = Canvas::transparent(w.max(1) as usize, h.max(1) as usize);
    c.rounded_rect_alpha(0, s.p(4), w, h - s.p(4), s.p(18), INK, 40);
    c.rounded_rect(0, 0, w, h - s.p(4), s.p(18), CREAM);
    // the line
    c.rounded_rect(pad / 2, pad - s.p(3), w - pad, row, s.p(9), SELECT);
    let lead = match prompt {
        Some(p) => format!("{p}: "),
        None => ": ".to_string(),
    };
    let x = pad + c.text_px(face, pad, pad, &lead, SOFT, false);
    c.text_px(face, x, pad, query, INK, false);
    let at = query.chars().take(cursor).count() as i32;
    c.rect(x + at * cw, pad + s.p(1), s.p(2), ch - s.p(2), INK);
    let top = pad + row + s.p(8);
    if rows.is_empty() {
        c.text_px(face, pad, top, "nada encontrado", SOFT, false);
    }
    let cols = ((w - 2 * pad) / cw).max(10) as usize;
    for (i, r) in rows.iter().enumerate() {
        let y = top + i as i32 * row;
        let sel = i == selected;
        if sel {
            // the one selection look (S21): SELECT + a 2 px RING ring
            c.rounded_rect(pad / 2, y - s.p(3), w - pad, row, s.p(8), SELECT);
            c.rounded_rect_ring(
                pad / 2,
                y - s.p(3),
                w - pad,
                row,
                s.p(8),
                s.p(2).max(2),
                RING,
            );
        }
        let keys_cols = r.keys.chars().count();
        let name = ellipsize(&r.name, cols / 2);
        let nx = pad + c.text_px(face, pad, y, &name, INK, sel);
        let left = cols.saturating_sub(name.chars().count() + keys_cols + 3);
        if left > 3 && !r.title.is_empty() {
            c.text_px(
                face,
                nx + 2 * cw,
                y,
                &ellipsize(&r.title, left),
                SOFT,
                false,
            );
        }
        if keys_cols > 0 {
            let kx = w - pad - keys_cols as i32 * cw;
            c.text_px(face, kx, y, &r.keys, SOFT, false);
        }
    }
    done(c)
}

fn map(
    fonts: &mut Fonts,
    s: Px,
    w: i32,
    h: i32,
    title: &str,
    help: &str,
    boxes: &[MapBox],
) -> Raster {
    let (w, h) = (s.p(w), s.p(h));
    let mut c = Canvas::transparent(w.max(1) as usize, h.max(1) as usize);
    c.rounded_rect_alpha(0, 0, w, h, 0, MAP_BG, 240);
    for b in boxes {
        let (x, y, bw, bh) = (s.p(b.x), s.p(b.y), s.p(b.w), s.p(b.h));
        match b.kind {
            MapKind::Hall => c.rounded_rect(x, y, bw, bh, s.p(4), [0xd8, 0xcc, 0xb0]),
            MapKind::Room | MapKind::Here => {
                c.rounded_rect(x, y, bw, bh, s.p(12), INK);
                let fill = if b.kind == MapKind::Here {
                    [0xff, 0xf0, 0xc8]
                } else {
                    MAP_ROOM
                };
                c.rounded_rect(
                    x + s.p(2),
                    y + s.p(2),
                    bw - s.p(4),
                    bh - s.p(4),
                    s.p(10),
                    fill,
                );
            }
            MapKind::Desk => c.rounded_rect(x, y, bw, bh, s.p(6), MAP_DESK),
            MapKind::Screen => c.rounded_rect(x, y, bw, bh, s.p(2), MAP_SCREEN),
            MapKind::Cat => c.rounded_rect(x, y, bw, bh, bw.min(bh) / 2, MAP_CAT),
            MapKind::Shelf => c.rounded_rect(x, y, bw, bh, s.p(2), MAP_SHELF),
        }
    }
    let face = fonts.face(s.font(PX_SMALL));
    let (cw, ch) = (face.cw as i32, face.ch as i32);
    for b in boxes.iter().filter(|b| !b.label.is_empty()) {
        let cols = ((s.p(b.w)) / cw).max(4) as usize;
        let t = ellipsize(&b.label, cols.max(8));
        let tw = Canvas::text_width(face, &t);
        let (tx, ty) = match b.kind {
            MapKind::Room | MapKind::Here => (s.p(b.x) + s.p(10), s.p(b.y) + s.p(8)),
            _ => (s.p(b.x) + (s.p(b.w) - tw) / 2, s.p(b.y + b.h) + s.p(3)),
        };
        c.text_px(face, tx, ty, &t, INK, b.kind == MapKind::Here);
    }
    let big = fonts.face(s.font(PX_DIALOG));
    let tw = Canvas::text_width(big, title);
    c.text_px(big, (w - tw) / 2, s.p(24), title, INK, true);
    let small = fonts.face(s.font(PX_SMALL));
    let hw = Canvas::text_width(small, help);
    c.text_px(small, (w - hw) / 2, h - s.p(24) - ch, help, SOFT, false);
    done(c)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(r: &Raster, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * r.w + x) * 4) as usize;
        [r.px[i], r.px[i + 1], r.px[i + 2], r.px[i + 3]]
    }

    #[test]
    fn the_label_is_cream_with_rounded_corners() {
        let mut f = Fonts::new();
        let r = raster(
            &Spec::Label {
                text: "NAVEGAR".into(),
            },
            &mut f,
            Px(1.0),
        );
        assert!(r.w > 60 && r.h > 16, "{}x{}", r.w, r.h);
        assert_eq!(at(&r, 0, 0)[3], 0, "corner cut");
        assert_eq!(at(&r, 2, 2), [CREAM[0], CREAM[1], CREAM[2], 255]);
        assert_eq!(r.px.len(), (r.w * r.h * 4) as usize);
        // scale 2: twice the size, same look
        let r2 = raster(
            &Spec::Label {
                text: "NAVEGAR".into(),
            },
            &mut f,
            Px(2.0),
        );
        assert!(
            (r2.w as i32 - 2 * r.w as i32).abs() <= 16,
            "{} {}",
            r2.w,
            r.w
        );
    }

    #[test]
    fn equal_specs_give_equal_pixels() {
        let mut f = Fonts::new();
        let spec = Spec::Dialog {
            width: 600,
            name: "api-gateway".into(),
            rgb: [0x6a, 0x9f, 0xd8],
            lines: wrap("Aplicar a migração agora?", 40),
            arrow: true,
            line: Some(("sim".into(), 3)),
        };
        let a = raster(&spec, &mut f, Px(1.0));
        let b = raster(&spec, &mut Fonts::new(), Px(1.0));
        assert_eq!(a, b);
        assert_eq!(a.w, 600);
    }

    #[test]
    fn every_spec_rasterises() {
        let mut f = Fonts::new();
        let specs = [
            Spec::Chip {
                text: "✉ 3".into(),
                bg: [1, 2, 3],
                fg: [200, 200, 200],
            },
            Spec::Toast {
                text: "salvo".into(),
                error: false,
            },
            Spec::Toast {
                text: "x".repeat(200),
                error: true,
            },
            Spec::Choices {
                items: vec!["sim".into(), "não".into()],
                selected: 1,
            },
            Spec::Palette {
                width: 500,
                prompt: Some("nome da sala".into()),
                query: "bra".into(),
                cursor: 3,
                rows: vec![Row {
                    name: "world.room_new".into(),
                    title: "sala nova".into(),
                    keys: "".into(),
                }],
                selected: 0,
            },
            Spec::Palette {
                width: 500,
                prompt: None,
                query: "".into(),
                cursor: 0,
                rows: vec![],
                selected: 0,
            },
            Spec::Hint {
                label: "sd".into(),
                typed: 1,
            },
            Spec::Tag {
                text: "acme".into(),
                rgb: [0x30, 0x30, 0x60],
            },
            Spec::Map {
                w: 320,
                h: 200,
                title: "mapa".into(),
                help: "esc".into(),
                boxes: vec![MapBox {
                    x: 10,
                    y: 50,
                    w: 100,
                    h: 60,
                    kind: MapKind::Room,
                    label: "acme".into(),
                }],
            },
            Spec::Ring { w: 40, h: 30 },
            Spec::Crosshair { t: 1 },
            Spec::Vignette {
                w: 64,
                h: 36,
                a16: 3,
            },
            Spec::Bar {
                text: "g solta · [ ] distância · - = tamanho".into(),
            },
        ];
        for s in &specs {
            let r = raster(s, &mut f, Px(1.0));
            assert!(r.w > 0 && r.h > 0, "{s:?}");
            assert_eq!(r.px.len(), (r.w * r.h * 4) as usize);
            assert!(
                r.px.chunks(4)
                    .all(|p| p[0] <= p[3] && p[1] <= p[3] && p[2] <= p[3]),
                "premultiplied {s:?}"
            );
        }
        // the ring is hollow
        let ring = raster(&Spec::Ring { w: 40, h: 30 }, &mut f, Px(1.0));
        assert_eq!(at(&ring, 20, 15)[3], 0);
        assert_eq!(at(&ring, 20, 1)[3], 255);
    }

    fn label(t: &str) -> Spec {
        Spec::Label { text: t.into() }
    }

    /// At 1.25 the label is drawn at physical px: a 20 px font (round(16 ·
    /// 1.25)), the box 1.25× the 1× one, every glyph edge on the buffer's
    /// own grid (nothing to resample).
    #[test]
    fn fractional_scale_rasters_at_physical_px() {
        let mut f = Fonts::new();
        let one = raster(&label("NAVEGAR"), &mut f, Px(1.0));
        let q = raster(&label("NAVEGAR"), &mut f, Px(1.25));
        let face20 = f.face(20).cw as i32;
        assert_eq!(Px(1.25).font(PX_UI), 20);
        assert_eq!(Px(1.5).font(PX_SMALL), 21);
        assert_eq!(Px(1.25).p(10), 13, "12.5 rounds up");
        assert_eq!(q.w as i32, 7 * face20 + 2 * Px(1.25).p(10));
        assert!(
            (q.w as f32 - one.w as f32 * 1.25).abs() <= 8.0,
            "{} vs {}",
            q.w,
            one.w
        );
        assert!((q.h as f32 - one.h as f32 * 1.25).abs() <= 4.0);
        // the same input twice: the same pixels
        assert_eq!(q, raster(&label("NAVEGAR"), &mut Fonts::new(), Px(1.25)));
        assert_ne!(Px(1.25).key(), Px(1.0).key());
        assert_eq!(Px(1.25).key(), Px(1.250_001).key());
    }

    fn alpha(r: &Raster, x: u32, y: u32) -> u8 {
        at(r, x, y)[3]
    }

    #[test]
    fn faded_scales_opacity_and_keeps_size() {
        let mut f = Fonts::new();
        let tag = Spec::Tag {
            text: "notas.md".into(),
            rgb: [0x6a, 0x9f, 0xd8],
        };
        let full = raster(&tag, &mut f, Px(1.0));
        assert_eq!(faded(tag.clone(), FADE_STEPS), tag, "opaque: itself");
        assert_eq!(faded(tag.clone(), 9), tag);
        let half = raster(&faded(tag.clone(), 2), &mut f, Px(1.0));
        assert_eq!((half.w, half.h), (full.w, full.h));
        let (cx, cy) = (full.w / 2, full.h / 2);
        assert_eq!(alpha(&full, 3, cy), 255);
        assert_eq!(alpha(&half, 3, cy), 128);
        assert!(half.px.chunks(4).all(|p| p[0] <= p[3]), "premultiplied");
        let _ = cx;
        let none = raster(&faded(tag.clone(), 0), &mut f, Px(1.0));
        assert!(none.px.iter().all(|v| *v == 0));
        assert_eq!(faded(tag, 1).scale(Px(1.25)), Px(1.25));
    }

    #[test]
    fn crosshair_dot_grows_into_a_ring() {
        let mut f = Fonts::new();
        let dot = raster(&Spec::Crosshair { t: 0 }, &mut f, Px(1.0));
        let ring = raster(&Spec::Crosshair { t: CROSS_STEPS }, &mut f, Px(1.0));
        assert_eq!((dot.w, dot.h), (CROSS_BOX as u32, CROSS_BOX as u32));
        let m = CROSS_BOX as u32 / 2;
        assert_eq!(alpha(&dot, m, m), 255, "the dot is solid");
        assert_eq!(at(&dot, m, m)[..3], CREAM);
        assert_eq!(alpha(&dot, m + 5, m), 0, "and small");
        assert_eq!(alpha(&ring, m, m), 0, "the ring is hollow");
        assert_eq!(alpha(&ring, m + 6, m), 255, "6 px out");
        assert_eq!(alpha(&ring, m, m - 7), 255);
        assert_eq!(alpha(&ring, 0, 0), 0);
        // the steps between are distinct rasters, all premultiplied
        let mid = raster(&Spec::Crosshair { t: 2 }, &mut f, Px(1.0));
        assert_ne!(mid, dot);
        assert_ne!(mid, ring);
        assert!(mid.px.chunks(4).all(|p| p[0] <= p[3]));
        // at 2×: twice the box
        let r2 = raster(&Spec::Crosshair { t: CROSS_STEPS }, &mut f, Px(2.0));
        assert_eq!(r2.w, 2 * CROSS_BOX as u32);
    }

    #[test]
    fn vignette_is_quarter_res_clear_middle_dark_corners() {
        let mut f = Fonts::new();
        let spec = Spec::Vignette {
            w: 1280,
            h: 720,
            a16: 16,
        };
        let s = spec.scale(Px(1.25));
        assert_eq!(s, Px(VIGNETTE_SCALE), "whatever the output scale");
        let v = raster(&spec, &mut f, s);
        assert_eq!((v.w, v.h), (320, 180));
        assert_eq!(alpha(&v, 160, 90), 0);
        let corner = alpha(&v, 0, 0);
        let max = (VIGNETTE_MAX * 255.0).round() as u8;
        assert!(corner > max - 20 && corner <= max, "{corner}");
        let half = raster(
            &Spec::Vignette {
                w: 1280,
                h: 720,
                a16: 8,
            },
            &mut f,
            s,
        );
        let hc = alpha(&half, 0, 0);
        assert!((hc as i32 - corner as i32 / 2).abs() <= 2, "{hc} {corner}");
        // odd sizes round up (the element covers the whole output)
        let odd = raster(
            &Spec::Vignette {
                w: 1366,
                h: 767,
                a16: 4,
            },
            &mut f,
            s,
        );
        assert_eq!((odd.w, odd.h), (342, 192));
        assert_eq!(label("x").scale(Px(1.25)), Px(1.25));
    }

    #[test]
    fn wrap_and_ellipsize() {
        assert_eq!(wrap("a bb ccc", 4), ["a bb", "ccc"]);
        assert_eq!(wrap("abcdefgh", 3), ["abc", "def", "gh"]);
        assert_eq!(wrap("um\ndois", 10), ["um", "dois"]);
        assert_eq!(wrap("", 10), [""]);
        assert_eq!(wrap("ação é já", 6), ["ação é", "já"]);
        assert_eq!(ellipsize("abcdef", 4), "abc…");
        assert_eq!(ellipsize("abc", 4), "abc");
    }

    #[test]
    fn ink_reads_on_its_background() {
        assert_eq!(ink_on(CREAM), INK);
        assert_eq!(ink_on([0x20, 0x30, 0x60]), [0xff, 0xfb, 0xf2]);
    }
}
