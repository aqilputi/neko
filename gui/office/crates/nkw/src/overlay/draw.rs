//! The overlay's rasters (§5.4): every 2D piece is a [`Spec`] (plain data,
//! hashed for the cache) turned into premultiplied RGBA with `nk-cats`'
//! fontdue text and anti-aliased rounded boxes. Sizes are logical px times
//! the integer buffer scale `s`. Pure CPU, no Smithay.

use std::collections::HashMap;

use nk_cats::raster::{Canvas, Face};

pub type Rgb = [u8; 3];

/// The AC palette of the overlays.
pub const CREAM: Rgb = [0xf6, 0xec, 0xd8];
pub const INK: Rgb = [0x5a, 0x3e, 0x2b];
pub const SOFT: Rgb = [0x9a, 0x82, 0x6c];
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
pub const RING: Rgb = [0xff, 0x9f, 0x43];

/// Font sizes, logical px.
pub const PX_SMALL: u32 = 14;
pub const PX_UI: u32 = 16;
pub const PX_DIALOG: u32 = 20;

/// The dialogue box's text lines.
pub const DIALOG_LINES: usize = 3;

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
    s: i32,
    text: &str,
    px: u32,
    bg: Rgb,
    fg: Rgb,
    pad: (i32, i32),
    r: i32,
) -> Raster {
    let face = fonts.face(px * s as u32);
    let tw = Canvas::text_width(face, text);
    let (w, h) = (tw + 2 * pad.0 * s, face.ch as i32 + 2 * pad.1 * s);
    let mut c = Canvas::transparent(w.max(1) as usize, h.max(1) as usize);
    c.rounded_rect(0, 0, w, h, r * s, bg);
    c.text_px(face, pad.0 * s, pad.1 * s, text, fg, false);
    done(c)
}

/// Rasterise `spec` at integer scale `s`.
pub fn raster(spec: &Spec, fonts: &mut Fonts, s: i32) -> Raster {
    let s = s.max(1);
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
            let face = fonts.face(PX_UI * s as u32);
            let tw = Canvas::text_width(face, label);
            let (w, h) = (tw + 8 * s, face.ch as i32 + 2 * s);
            let mut c = Canvas::transparent(w as usize, h as usize);
            c.rounded_rect(0, 0, w, h, 5 * s, INK);
            c.rounded_rect(s, s, w - 2 * s, h - 2 * s, 4 * s, HINT_BG);
            let mut x = 4 * s;
            for (i, ch) in label.chars().enumerate() {
                let fg = if i < *typed { HINT_TYPED } else { INK };
                x += c.text_px(face, x, s, &ch.to_string(), fg, true);
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
            let face = fonts.face(PX_DIALOG * s as u32);
            let (cw, ch) = (face.cw as i32, face.ch as i32);
            let cols = items
                .iter()
                .map(|i| i.chars().count() + 4)
                .max()
                .unwrap_or(4) as i32;
            let (pad, row) = (14 * s, ch + 6 * s);
            let w = cols * cw + 2 * pad;
            let h = items.len() as i32 * row + 2 * pad - 6 * s;
            let mut c = Canvas::transparent(w.max(1) as usize, h.max(1) as usize);
            c.rounded_rect(0, 0, w, h, 18 * s, CREAM);
            for (i, it) in items.iter().enumerate() {
                let y = pad + i as i32 * row;
                let sel = i == *selected;
                if sel {
                    c.rounded_rect(pad / 2, y - 2 * s, w - pad, ch + 4 * s, 8 * s, SELECT);
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
            let (w, h) = (w * s, h * s);
            let mut c = Canvas::transparent(w.max(1) as usize, h.max(1) as usize);
            c.rounded_rect(0, 0, w, h, 8 * s, RING);
            // hollow it: clear the inside
            let t = 3 * s;
            for y in t..(h - t).max(t) {
                for x in t..(w - t).max(t) {
                    let i = ((y * w + x) * 4) as usize;
                    c.px[i..i + 4].copy_from_slice(&[0, 0, 0, 0]);
                }
            }
            done(c)
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn dialog(
    fonts: &mut Fonts,
    s: i32,
    width: i32,
    name: &str,
    rgb: Rgb,
    lines: &[String],
    arrow: bool,
    line: Option<&(String, usize)>,
) -> Raster {
    let (pcw, pch) = {
        let f = fonts.face(PX_UI * s as u32);
        (f.cw as i32, f.ch as i32)
    };
    let face = fonts.face(PX_DIALOG * s as u32);
    let (ch, cw) = (face.ch as i32, face.cw as i32);
    let plate_h = pch + 10 * s;
    let over = plate_h / 2;
    let pad = 24 * s;
    let rows = DIALOG_LINES as i32 + i32::from(line.is_some());
    let w = width * s;
    let bh = rows * ch + 2 * pad;
    let h = bh + over;
    let mut c = Canvas::transparent(w.max(1) as usize, h.max(1) as usize);
    // a soft shadow, then the cream box
    c.rounded_rect_alpha(0, over + 4 * s, w, bh - 4 * s, 28 * s, INK, 40);
    c.rounded_rect(0, over, w, bh - 4 * s, 28 * s, CREAM);
    for (i, l) in lines.iter().take(DIALOG_LINES).enumerate() {
        c.text_px(face, pad, over + pad + i as i32 * ch, l, INK, false);
    }
    if let Some((text, cursor)) = line {
        let y = over + pad + DIALOG_LINES as i32 * ch;
        c.rounded_rect(
            pad - 8 * s,
            y - 2 * s,
            w - 2 * pad + 16 * s,
            ch + 4 * s,
            8 * s,
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
        c.rect(x + at * cw, y + 2 * s, 2 * s, ch - 4 * s, INK);
    }
    if arrow {
        let ax = w - pad - cw;
        let ay = over + bh - pad - ch / 2;
        c.text_px(face, ax, ay, "▼", [0xd0, 0x80, 0x40], false);
    }
    // the name plate, astride the box's top edge
    let pw = (name.chars().count() as i32 + 2) * pcw;
    let pf = fonts.face(PX_UI * s as u32);
    c.rounded_rect(pad, 0, pw, plate_h, plate_h / 2, rgb);
    c.text_px(pf, pad + pcw, 5 * s, name, ink_on(rgb), true);
    done(c)
}

#[allow(clippy::too_many_arguments)]
fn palette(
    fonts: &mut Fonts,
    s: i32,
    width: i32,
    prompt: Option<&str>,
    query: &str,
    cursor: usize,
    rows: &[Row],
    selected: usize,
) -> Raster {
    let face = fonts.face(PX_UI * s as u32);
    let (cw, ch) = (face.cw as i32, face.ch as i32);
    let pad = 14 * s;
    let row = ch + 6 * s;
    let w = width * s;
    let list = if rows.is_empty() {
        row
    } else {
        rows.len() as i32 * row
    };
    let h = pad + row + 8 * s + list + pad;
    let mut c = Canvas::transparent(w.max(1) as usize, h.max(1) as usize);
    c.rounded_rect_alpha(0, 4 * s, w, h - 4 * s, 18 * s, INK, 40);
    c.rounded_rect(0, 0, w, h - 4 * s, 18 * s, CREAM);
    // the line
    c.rounded_rect(pad / 2, pad - 3 * s, w - pad, row, 9 * s, SELECT);
    let lead = match prompt {
        Some(p) => format!("{p}: "),
        None => ": ".to_string(),
    };
    let x = pad + c.text_px(face, pad, pad, &lead, SOFT, false);
    c.text_px(face, x, pad, query, INK, false);
    let at = query.chars().take(cursor).count() as i32;
    c.rect(x + at * cw, pad + s, 2 * s, ch - 2 * s, INK);
    let top = pad + row + 8 * s;
    if rows.is_empty() {
        c.text_px(face, pad, top, "nada encontrado", SOFT, false);
    }
    let cols = ((w - 2 * pad) / cw).max(10) as usize;
    for (i, r) in rows.iter().enumerate() {
        let y = top + i as i32 * row;
        let sel = i == selected;
        if sel {
            c.rounded_rect(pad / 2, y - 3 * s, w - pad, row, 8 * s, SELECT);
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
    s: i32,
    w: i32,
    h: i32,
    title: &str,
    help: &str,
    boxes: &[MapBox],
) -> Raster {
    let (w, h) = (w * s, h * s);
    let mut c = Canvas::transparent(w.max(1) as usize, h.max(1) as usize);
    c.rounded_rect_alpha(0, 0, w, h, 0, MAP_BG, 240);
    for b in boxes {
        let (x, y, bw, bh) = (b.x * s, b.y * s, b.w * s, b.h * s);
        match b.kind {
            MapKind::Hall => c.rounded_rect(x, y, bw, bh, 4 * s, [0xd8, 0xcc, 0xb0]),
            MapKind::Room | MapKind::Here => {
                c.rounded_rect(x, y, bw, bh, 12 * s, INK);
                let fill = if b.kind == MapKind::Here {
                    [0xff, 0xf0, 0xc8]
                } else {
                    MAP_ROOM
                };
                c.rounded_rect(x + 2 * s, y + 2 * s, bw - 4 * s, bh - 4 * s, 10 * s, fill);
            }
            MapKind::Desk => c.rounded_rect(x, y, bw, bh, 6 * s, MAP_DESK),
            MapKind::Screen => c.rounded_rect(x, y, bw, bh, 2 * s, MAP_SCREEN),
            MapKind::Cat => c.rounded_rect(x, y, bw, bh, bw.min(bh) / 2, MAP_CAT),
        }
    }
    let face = fonts.face(PX_SMALL * s as u32);
    let (cw, ch) = (face.cw as i32, face.ch as i32);
    for b in boxes.iter().filter(|b| !b.label.is_empty()) {
        let cols = ((b.w * s) / cw).max(4) as usize;
        let t = ellipsize(&b.label, cols.max(8));
        let tw = Canvas::text_width(face, &t);
        let (tx, ty) = match b.kind {
            MapKind::Room | MapKind::Here => (b.x * s + 10 * s, b.y * s + 8 * s),
            _ => (b.x * s + (b.w * s - tw) / 2, (b.y + b.h) * s + 3 * s),
        };
        c.text_px(face, tx, ty, &t, INK, b.kind == MapKind::Here);
    }
    let big = fonts.face(PX_DIALOG * s as u32);
    let tw = Canvas::text_width(big, title);
    c.text_px(big, (w - tw) / 2, 24 * s, title, INK, true);
    let small = fonts.face(PX_SMALL * s as u32);
    let hw = Canvas::text_width(small, help);
    c.text_px(small, (w - hw) / 2, h - 24 * s - ch, help, SOFT, false);
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
            1,
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
            2,
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
        let a = raster(&spec, &mut f, 1);
        let b = raster(&spec, &mut Fonts::new(), 1);
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
        ];
        for s in &specs {
            let r = raster(s, &mut f, 1);
            assert!(r.w > 0 && r.h > 0, "{s:?}");
            assert_eq!(r.px.len(), (r.w * r.h * 4) as usize);
            assert!(
                r.px.chunks(4)
                    .all(|p| p[0] <= p[3] && p[1] <= p[3] && p[2] <= p[3]),
                "premultiplied {s:?}"
            );
        }
        // the ring is hollow
        let ring = raster(&Spec::Ring { w: 40, h: 30 }, &mut f, 1);
        assert_eq!(at(&ring, 20, 15)[3], 0);
        assert_eq!(at(&ring, 20, 1)[3], 255);
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
