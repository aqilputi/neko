//! CPU text rasterizer for the monitors and the zoomed terminal: a monospace
//! glyph cache (fontdue) blitted into an RGBA8 (sRGB) buffer that becomes a
//! Bevy `Image`. One code path for both, pixel-exact when shown 1:1.

use std::collections::HashMap;

use crate::model::Rgb;

/// The shipped font (JetBrains Mono NL, OFL-1.1; assets/fonts/OFL.txt).
pub static FONT_BYTES: &[u8] = include_bytes!("../assets/fonts/JetBrainsMonoNL-Regular.ttf");

pub struct Glyph {
    pub w: usize,
    pub h: usize,
    pub xmin: i32,
    pub ymin: i32,
    pub alpha: Vec<u8>,
}

/// A font at one pixel size, with its glyphs cached.
pub struct Face {
    font: fontdue::Font,
    pub px: f32,
    /// cell size in pixels
    pub cw: usize,
    pub ch: usize,
    ascent: i32,
    cache: HashMap<char, Glyph>,
}

impl Face {
    pub fn new(font: &fontdue::Font, px: f32) -> Face {
        let m = font.metrics('M', px);
        let lm = font.horizontal_line_metrics(px);
        let (ascent, descent, gap) =
            lm.map(|l| (l.ascent, l.descent, l.line_gap))
                .unwrap_or((px * 0.8, -px * 0.2, 0.0));
        let cw = m.advance_width.round().max(1.0) as usize;
        let ch = (ascent - descent + gap).ceil().max(1.0) as usize;
        Face {
            font: font.clone(),
            px,
            cw,
            ch,
            ascent: ascent.round() as i32,
            cache: HashMap::new(),
        }
    }

    pub fn load_font() -> fontdue::Font {
        fontdue::Font::from_bytes(FONT_BYTES, fontdue::FontSettings::default())
            .expect("embedded font")
    }

    fn glyph(&mut self, c: char) -> &Glyph {
        if !self.cache.contains_key(&c) {
            let c2 = if self.font.lookup_glyph_index(c) == 0 && c != ' ' {
                fallback(c)
            } else {
                c
            };
            let (m, alpha) = self.font.rasterize(c2, self.px);
            self.cache.insert(
                c,
                Glyph {
                    w: m.width,
                    h: m.height,
                    xmin: m.xmin,
                    ymin: m.ymin,
                    alpha,
                },
            );
        }
        &self.cache[&c]
    }

    #[cfg(test)]
    pub fn has(&self, c: char) -> bool {
        self.font.lookup_glyph_index(c) != 0
    }
}

/// A stand-in for glyphs the font lacks.
fn fallback(c: char) -> char {
    match c {
        '✻' | '✽' | '✶' | '✳' | '✢' => '*',
        '⏺' => '●',
        '✉' => '@',
        '❯' => '>',
        '⎿' => '└',
        _ => '?',
    }
}

/// An RGBA8 canvas (sRGB, alpha 255).
pub struct Canvas {
    pub w: usize,
    pub h: usize,
    pub px: Vec<u8>,
}

impl Canvas {
    /// A fully transparent canvas (alpha 0 everywhere), for overlays: draw
    /// on it, then take [`Canvas::into_rgba`].
    pub fn transparent(w: usize, h: usize) -> Canvas {
        Canvas {
            w,
            h,
            px: vec![0; w * h * 4],
        }
    }

    pub fn new(w: usize, h: usize, bg: Rgb) -> Canvas {
        let mut c = Canvas {
            w,
            h,
            px: vec![255; w * h * 4],
        };
        c.fill(bg);
        c
    }

    pub fn fill(&mut self, c: Rgb) {
        for p in self.px.as_chunks_mut::<4>().0 {
            p[0] = c[0];
            p[1] = c[1];
            p[2] = c[2];
            p[3] = 255;
        }
    }

    pub fn rect(&mut self, x: i32, y: i32, w: i32, h: i32, c: Rgb) {
        let x0 = x.max(0) as usize;
        let y0 = y.max(0) as usize;
        let x1 = ((x + w).max(0) as usize).min(self.w);
        let y1 = ((y + h).max(0) as usize).min(self.h);
        for yy in y0..y1 {
            let row = &mut self.px[(yy * self.w + x0) * 4..(yy * self.w + x1) * 4];
            for p in row.as_chunks_mut::<4>().0 {
                p[0] = c[0];
                p[1] = c[1];
                p[2] = c[2];
                p[3] = 255;
            }
        }
    }

    /// A filled rectangle with corners of radius `r`, anti-aliased edges.
    pub fn rounded_rect(&mut self, x: i32, y: i32, w: i32, h: i32, r: i32, c: Rgb) {
        self.rounded_rect_alpha(x, y, w, h, r, c, 255);
    }

    /// The same at opacity `a`, blended over what is there.
    #[allow(clippy::too_many_arguments)]
    pub fn rounded_rect_alpha(&mut self, x: i32, y: i32, w: i32, h: i32, r: i32, c: Rgb, a: u8) {
        if w <= 0 || h <= 0 || a == 0 {
            return;
        }
        let r = r.clamp(0, w.min(h) / 2) as f32;
        let (hw, hh) = (w as f32 / 2.0, h as f32 / 2.0);
        let (cx, cy) = (x as f32 + hw, y as f32 + hh);
        let x0 = x.max(0);
        let y0 = y.max(0);
        let x1 = (x + w).min(self.w as i32);
        let y1 = (y + h).min(self.h as i32);
        for yy in y0..y1 {
            for xx in x0..x1 {
                // signed distance from the pixel centre to the rounded box
                let qx = (xx as f32 + 0.5 - cx).abs() - (hw - r);
                let qy = (yy as f32 + 0.5 - cy).abs() - (hh - r);
                let out = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt();
                let d = out + qx.max(qy).min(0.0) - r;
                let cover = (0.5 - d).clamp(0.0, 1.0);
                if cover <= 0.0 {
                    continue;
                }
                let k = (cover * a as f32).round() as u8;
                if k == 255 {
                    let i = (yy as usize * self.w + xx as usize) * 4;
                    self.px[i..i + 3].copy_from_slice(&c);
                    self.px[i + 3] = 255;
                } else {
                    self.blend(xx, yy, c, k);
                }
            }
        }
    }

    /// The pixels as premultiplied RGBA (what a compositor's memory buffer
    /// wants); untouched pixels of a [`Canvas::transparent`] stay 0.
    pub fn into_rgba(self) -> (usize, usize, Vec<u8>) {
        let mut px = self.px;
        for p in px.as_chunks_mut::<4>().0 {
            let a = p[3] as u32;
            if a < 255 {
                for v in &mut p[..3] {
                    *v = ((*v as u32 * a + 127) / 255) as u8;
                }
            }
        }
        (self.w, self.h, px)
    }

    /// How wide `s` is in pixels in `face` (wide chars take two cells).
    pub fn text_width(face: &mut Face, s: &str) -> i32 {
        let cells: usize = s.chars().map(crate::monitor::char_width).sum();
        (cells * face.cw) as i32
    }

    /// Draws a string with its first cell's top-left at pixel (x, y);
    /// returns the width drawn.
    pub fn text_px(
        &mut self,
        face: &mut Face,
        x: i32,
        y: i32,
        s: &str,
        fg: Rgb,
        bold: bool,
    ) -> i32 {
        let mut cx = x;
        for c in s.chars() {
            self.glyph(face, cx, y, c, fg, bold);
            cx += (crate::monitor::char_width(c) * face.cw) as i32;
        }
        cx - x
    }

    fn blend(&mut self, x: i32, y: i32, c: Rgb, a: u8) {
        if a == 0 || x < 0 || y < 0 || x as usize >= self.w || y as usize >= self.h {
            return;
        }
        let i = (y as usize * self.w + x as usize) * 4;
        let a = a as u32;
        let da = self.px[i + 3] as u32;
        if da == 255 {
            #[allow(clippy::needless_range_loop)] // three channels of two buffers
            for k in 0..3 {
                let d = self.px[i + k] as u32;
                self.px[i + k] = ((c[k] as u32 * a + d * (255 - a)) / 255) as u8;
            }
            return;
        }
        // over a (partly) transparent pixel: straight-alpha "over"
        let under = da * (255 - a) / 255;
        let out = a + under;
        if out == 0 {
            return;
        }
        #[allow(clippy::needless_range_loop)]
        for k in 0..3 {
            let d = self.px[i + k] as u32;
            self.px[i + k] = ((c[k] as u32 * a + d * under) / out) as u8;
        }
        self.px[i + 3] = out as u8;
    }

    /// Draws one char with its cell's top-left at (x, y).
    pub fn glyph(&mut self, face: &mut Face, x: i32, y: i32, c: char, fg: Rgb, bold: bool) {
        if c == ' ' || c == '\0' {
            return;
        }
        let ascent = face.ascent;
        let g = face.glyph(c);
        let gx = x + g.xmin;
        let gy = y + ascent - g.ymin - g.h as i32;
        let (w, h) = (g.w, g.h);
        let alpha = &g.alpha;
        for yy in 0..h {
            for xx in 0..w {
                let a = alpha[yy * w + xx];
                self.blend(gx + xx as i32, gy + yy as i32, fg, a);
                if bold {
                    self.blend(gx + xx as i32 + 1, gy + yy as i32, fg, a);
                }
            }
        }
    }

    /// Draws a string from cell column `col`, row `row`; returns columns used.
    pub fn text(
        &mut self,
        face: &mut Face,
        col: usize,
        row: usize,
        s: &str,
        fg: Rgb,
        bold: bool,
    ) -> usize {
        let mut cx = col;
        for c in s.chars() {
            let x = (cx * face.cw) as i32;
            let y = (row * face.ch) as i32;
            self.glyph(face, x, y, c, fg, bold);
            cx += crate::monitor::char_width(c);
        }
        cx - col
    }
}

/// A terminal cell as the renderer needs it (from nk-client's Grid).
#[derive(Clone, Debug, PartialEq)]
pub struct TCell {
    pub ch: char,
    pub fg: Rgb,
    pub bg: Rgb,
    pub bold: bool,
    pub underline: bool,
    pub inverse: bool,
    pub wide: bool,
}

/// Draws a terminal grid (row-major cells) at cell size; cursor as a block
/// outline in `cursor` colour (filled when `focused`).
#[allow(clippy::too_many_arguments)]
pub fn draw_grid(
    canvas: &mut Canvas,
    face: &mut Face,
    cols: usize,
    rows: usize,
    cells: &[TCell],
    cursor: Option<(usize, usize)>,
    cursor_color: Rgb,
    focused: bool,
) {
    let (cw, ch) = (face.cw as i32, face.ch as i32);
    for r in 0..rows {
        for c in 0..cols {
            let Some(cell) = cells.get(r * cols + c) else {
                continue;
            };
            let bg = if cell.inverse { cell.fg } else { cell.bg };
            let x = c as i32 * cw;
            let y = r as i32 * ch;
            let w = if cell.wide { 2 * cw } else { cw };
            canvas.rect(x, y, w, ch, bg);
        }
    }
    for r in 0..rows {
        for c in 0..cols {
            let Some(cell) = cells.get(r * cols + c) else {
                continue;
            };
            let fg = if cell.inverse { cell.bg } else { cell.fg };
            let x = c as i32 * cw;
            let y = r as i32 * ch;
            let is_cursor = cursor == Some((c, r));
            if is_cursor && focused {
                canvas.rect(x, y, cw, ch, cursor_color);
                canvas.glyph(face, x, y, cell.ch, cell.bg, cell.bold);
                continue;
            }
            canvas.glyph(face, x, y, cell.ch, fg, cell.bold);
            if cell.underline {
                canvas.rect(x, y + ch - 2, cw, 1, fg);
            }
            if is_cursor {
                canvas.rect(x, y, cw, 1, cursor_color);
                canvas.rect(x, y + ch - 1, cw, 1, cursor_color);
                canvas.rect(x, y, 1, ch, cursor_color);
                canvas.rect(x + cw - 1, y, 1, ch, cursor_color);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn face_metrics_are_monospace() {
        let font = Face::load_font();
        let f = Face::new(&font, 16.0);
        assert!(f.cw >= 8 && f.cw <= 11, "cw={}", f.cw);
        assert!(f.ch >= 16 && f.ch <= 24, "ch={}", f.ch);
        for c in ['●', '⎿', '◆', '─', '│', '╭', '❯', 'ã', 'ç'] {
            assert!(
                f.has(c) || f.has(fallback(c)),
                "font lacks {c:?} and its fallback"
            );
        }
    }

    #[test]
    fn draws_text_and_grid() {
        let font = Face::load_font();
        let mut f = Face::new(&font, 14.0);
        let mut cv = Canvas::new(f.cw * 10, f.ch * 2, [0, 0, 0]);
        let n = cv.text(&mut f, 0, 0, "abc", [255, 255, 255], false);
        assert_eq!(n, 3);
        let lit = cv
            .px
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|p| p[0] > 128)
            .count();
        assert!(lit > 10, "some pixels lit");
        let cells = vec![
            TCell {
                ch: 'x',
                fg: [255, 0, 0],
                bg: [0, 0, 255],
                bold: true,
                underline: true,
                inverse: false,
                wide: false
            };
            20
        ];
        draw_grid(
            &mut cv,
            &mut f,
            10,
            2,
            &cells,
            Some((1, 1)),
            [0, 255, 0],
            true,
        );
        // background of the first cell is blue
        assert_eq!(&cv.px[0..3], &[0, 0, 255]);
        // the cursor cell is filled green at its top-left
        let i = ((f.ch) * cv.w + f.cw) * 4;
        assert_eq!(&cv.px[i..i + 3], &[0, 255, 0]);
    }

    fn alpha(cv: &Canvas, x: usize, y: usize) -> u8 {
        cv.px[(y * cv.w + x) * 4 + 3]
    }

    #[test]
    fn rounded_rect_has_aa_corners_monotone_to_the_inside() {
        let mut cv = Canvas::transparent(40, 30);
        cv.rounded_rect(5, 5, 30, 20, 8, [200, 100, 50]);
        // outside stays untouched, the middle is solid
        assert_eq!(alpha(&cv, 0, 0), 0);
        assert_eq!(alpha(&cv, 5, 5), 0, "the very corner is cut");
        assert_eq!(alpha(&cv, 20, 15), 255);
        assert_eq!(&cv.px[(15 * 40 + 20) * 4..][..3], &[200, 100, 50]);
        assert_eq!(alpha(&cv, 20, 5), 255, "a straight edge is solid");
        // along the corner's diagonal the coverage only grows inwards
        let diag: Vec<u8> = (0..8).map(|k| alpha(&cv, 5 + k, 5 + k)).collect();
        assert!(diag.windows(2).all(|w| w[0] <= w[1]), "{diag:?}");
        assert!(diag.iter().any(|a| *a > 0 && *a < 255), "AA: {diag:?}");
        // the four corners are mirror images
        for k in 0..8 {
            let a = alpha(&cv, 5 + k, 5 + k);
            assert_eq!(alpha(&cv, 34 - k, 5 + k), a);
            assert_eq!(alpha(&cv, 5 + k, 24 - k), a);
            assert_eq!(alpha(&cv, 34 - k, 24 - k), a);
        }
    }

    #[test]
    fn rounded_rect_alpha_blends_over_and_clips() {
        let mut cv = Canvas::new(10, 10, [0, 0, 0]);
        cv.rounded_rect_alpha(-5, -5, 30, 30, 0, [255, 255, 255], 128);
        assert_eq!(&cv.px[0..4], &[128, 128, 128, 255]);
        let mut t = Canvas::transparent(10, 10);
        t.rounded_rect_alpha(0, 0, 10, 10, 3, [255, 0, 0], 128);
        assert_eq!(alpha(&t, 5, 5), 128);
        // radius larger than the box is clamped (a pill), nothing panics
        t.rounded_rect(2, 2, 6, 4, 99, [0, 0, 255]);
        t.rounded_rect(0, 0, 0, 5, 2, [0, 0, 255]);
        assert_eq!(alpha(&t, 5, 4), 255);
    }

    #[test]
    fn into_rgba_is_premultiplied() {
        let mut t = Canvas::transparent(4, 1);
        t.rect(0, 0, 1, 1, [200, 100, 50]);
        t.rounded_rect_alpha(1, 0, 1, 1, 0, [200, 100, 50], 128);
        let (w, h, px) = t.into_rgba();
        assert_eq!((w, h), (4, 1));
        assert_eq!(&px[0..4], &[200, 100, 50, 255]);
        assert_eq!(&px[4..8], &[100, 50, 25, 128]);
        assert_eq!(&px[8..16], &[0; 8], "untouched");
        // an opaque canvas comes out unchanged
        let cv = Canvas::new(2, 2, [1, 2, 3]);
        let before = cv.px.clone();
        assert_eq!(cv.into_rgba().2, before);
    }

    #[test]
    fn text_width_and_text_px() {
        let font = Face::load_font();
        let mut f = Face::new(&font, 16.0);
        assert_eq!(Canvas::text_width(&mut f, ""), 0);
        assert_eq!(Canvas::text_width(&mut f, "ação"), 4 * f.cw as i32);
        assert_eq!(Canvas::text_width(&mut f, "猫a"), 3 * f.cw as i32);
        let mut t = Canvas::transparent(f.cw * 6, f.ch);
        let w = t.text_px(&mut f, 0, 0, "olá", [255, 255, 255], false);
        assert_eq!(w, Canvas::text_width(&mut f, "olá"));
        let lit = t.px.as_chunks::<4>().0.iter().filter(|p| p[3] > 0).count();
        assert!(lit > 10);
        let (_, _, px) = t.into_rgba();
        assert!(
            px.as_chunks::<4>().0.iter().all(|p| p[0] <= p[3]),
            "premultiplied"
        );
    }

    #[test]
    fn rect_clips() {
        let mut cv = Canvas::new(4, 4, [0, 0, 0]);
        cv.rect(-2, -2, 100, 3, [9, 9, 9]);
        assert_eq!(cv.px[0], 9);
        assert_eq!(cv.px[(3 * 4) * 4], 0);
    }
}
