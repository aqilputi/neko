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
            }
        }
    }

    fn blend(&mut self, x: i32, y: i32, c: Rgb, a: u8) {
        if a == 0 || x < 0 || y < 0 || x as usize >= self.w || y as usize >= self.h {
            return;
        }
        let i = (y as usize * self.w + x as usize) * 4;
        let a = a as u32;
        #[allow(clippy::needless_range_loop)] // three channels of two buffers
        for k in 0..3 {
            let d = self.px[i + k] as u32;
            self.px[i + k] = ((c[k] as u32 * a + d * (255 - a)) / 255) as u8;
        }
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

    #[test]
    fn rect_clips() {
        let mut cv = Canvas::new(4, 4, [0, 0, 0]);
        cv.rect(-2, -2, 100, 3, [9, 9, 9]);
        assert_eq!(cv.px[0], 9);
        assert_eq!(cv.px[(3 * 4) * 4], 0);
    }
}
