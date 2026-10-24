//! CPU text rasterizer for the monitors and the zoomed terminal: a monospace
//! glyph cache (fontdue) blitted into an RGBA8 (sRGB) buffer that becomes a
//! Bevy `Image`. One code path for both, pixel-exact when shown 1:1.

use std::collections::HashMap;

pub use crate::model::Rgb;

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

    /// An anti-aliased circle outline: centre (`cx`, `cy`) in pixel
    /// coordinates (pixel centres at +0.5), `r` to the middle of the
    /// stroke, `w` wide; blended over what is there.
    pub fn ring(&mut self, cx: f32, cy: f32, r: f32, w: f32, c: Rgb) {
        if w <= 0.0 || r < 0.0 {
            return;
        }
        let reach = r + w / 2.0 + 1.0;
        let x0 = (cx - reach).floor().max(0.0) as i32;
        let y0 = (cy - reach).floor().max(0.0) as i32;
        let x1 = ((cx + reach).ceil() as i32).min(self.w as i32);
        let y1 = ((cy + reach).ceil() as i32).min(self.h as i32);
        for yy in y0..y1 {
            for xx in x0..x1 {
                let d = ((xx as f32 + 0.5 - cx).powi(2) + (yy as f32 + 0.5 - cy).powi(2)).sqrt();
                let cover = (w / 2.0 + 0.5 - (d - r).abs()).clamp(0.0, 1.0);
                let k = (cover * 255.0).round() as u8;
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

    /// Calls `cover(px, py)` (pixel centre, 0..1) over the box `[x0, x1) ×
    /// [y0, y1)` clipped to the canvas; fills with `c` (blended) or, with
    /// `None`, cuts the alpha (a punch: `a · (1 − cover)`).
    fn shade(&mut self, b: (f32, f32, f32, f32), c: Option<Rgb>, cover: impl Fn(f32, f32) -> f32) {
        let x0 = b.0.floor().max(0.0) as i32;
        let y0 = b.1.floor().max(0.0) as i32;
        let x1 = (b.2.ceil() as i32).min(self.w as i32);
        let y1 = (b.3.ceil() as i32).min(self.h as i32);
        for yy in y0..y1 {
            for xx in x0..x1 {
                let k = cover(xx as f32 + 0.5, yy as f32 + 0.5).clamp(0.0, 1.0);
                if k <= 0.0 {
                    continue;
                }
                let i = (yy as usize * self.w + xx as usize) * 4;
                match c {
                    Some(c) => {
                        let a = (k * 255.0).round() as u8;
                        if a == 255 {
                            self.px[i..i + 3].copy_from_slice(&c);
                            self.px[i + 3] = 255;
                        } else {
                            self.blend(xx, yy, c, a);
                        }
                    }
                    None => {
                        let a = self.px[i + 3] as f32 * (1.0 - k);
                        self.px[i + 3] = a.round() as u8;
                    }
                }
            }
        }
    }

    /// A filled anti-aliased disc, centre in pixel coordinates.
    pub fn disc(&mut self, cx: f32, cy: f32, r: f32, c: Rgb) {
        self.disc_op(cx, cy, r, Some(c));
    }

    /// Cuts a disc out of what is there (alpha).
    pub fn punch_disc(&mut self, cx: f32, cy: f32, r: f32) {
        self.disc_op(cx, cy, r, None);
    }

    fn disc_op(&mut self, cx: f32, cy: f32, r: f32, c: Option<Rgb>) {
        if r <= 0.0 {
            return;
        }
        let b = (cx - r - 1.0, cy - r - 1.0, cx + r + 1.0, cy + r + 1.0);
        self.shade(b, c, |x, y| {
            r + 0.5 - ((x - cx).powi(2) + (y - cy).powi(2)).sqrt()
        });
    }

    /// A stroke `w` wide from (x0, y0) to (x1, y1) with round caps.
    pub fn line(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, w: f32, c: Rgb) {
        self.line_op(x0, y0, x1, y1, w, Some(c));
    }

    /// Cuts a round-capped stroke out of what is there.
    pub fn punch_line(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, w: f32) {
        self.line_op(x0, y0, x1, y1, w, None);
    }

    fn line_op(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, w: f32, c: Option<Rgb>) {
        if w <= 0.0 {
            return;
        }
        let h = w / 2.0;
        let b = (
            x0.min(x1) - h - 1.0,
            y0.min(y1) - h - 1.0,
            x0.max(x1) + h + 1.0,
            y0.max(y1) + h + 1.0,
        );
        let (dx, dy) = (x1 - x0, y1 - y0);
        let len2 = (dx * dx + dy * dy).max(1e-6);
        self.shade(b, c, |x, y| {
            let t = (((x - x0) * dx + (y - y0) * dy) / len2).clamp(0.0, 1.0);
            let (px, py) = (x0 + dx * t - x, y0 + dy * t - y);
            h + 0.5 - (px * px + py * py).sqrt()
        });
    }

    /// A filled anti-aliased triangle (either winding).
    pub fn tri(&mut self, p0: (f32, f32), p1: (f32, f32), p2: (f32, f32), c: Rgb) {
        self.tri_op(p0, p1, p2, Some(c));
    }

    /// Cuts a triangle out of what is there.
    pub fn punch_tri(&mut self, p0: (f32, f32), p1: (f32, f32), p2: (f32, f32)) {
        self.tri_op(p0, p1, p2, None);
    }

    fn tri_op(&mut self, p0: (f32, f32), p1: (f32, f32), p2: (f32, f32), c: Option<Rgb>) {
        let area = (p1.0 - p0.0) * (p2.1 - p0.1) - (p1.1 - p0.1) * (p2.0 - p0.0);
        if area.abs() < 1e-6 {
            return;
        }
        // counter-clockwise in screen terms, so "inside" is positive
        let (p1, p2) = if area < 0.0 { (p2, p1) } else { (p1, p2) };
        let edges = [(p0, p1), (p1, p2), (p2, p0)];
        let b = (
            p0.0.min(p1.0).min(p2.0) - 1.0,
            p0.1.min(p1.1).min(p2.1) - 1.0,
            p0.0.max(p1.0).max(p2.0) + 1.0,
            p0.1.max(p1.1).max(p2.1) + 1.0,
        );
        self.shade(b, c, |x, y| {
            // signed distance to each edge's line; the minimum is the
            // distance to the triangle's boundary (inside: positive)
            edges
                .iter()
                .map(|(a, b)| {
                    let (ex, ey) = (b.0 - a.0, b.1 - a.1);
                    let l = (ex * ex + ey * ey).sqrt().max(1e-6);
                    ((y - a.1) * ex - (x - a.0) * ey) / l
                })
                .fold(f32::INFINITY, f32::min)
                + 0.5
        });
    }

    /// Cuts a rectangle out of what is there (pixel coordinates, AA edges).
    pub fn punch_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        let b = (x - 1.0, y - 1.0, x + w + 1.0, y + h + 1.0);
        self.shade(b, None, |px, py| {
            let cx = (px - x + 0.5).min(x + w - px + 0.5).clamp(0.0, 1.0);
            let cy = (py - y + 0.5).min(y + h - py + 0.5).clamp(0.0, 1.0);
            cx * cy
        });
    }

    /// The outline of a rounded rectangle, `ring_w` wide, inside the box.
    #[allow(clippy::too_many_arguments)]
    pub fn rounded_rect_ring(
        &mut self,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        r: i32,
        ring_w: i32,
        c: Rgb,
    ) {
        if w <= 0 || h <= 0 || ring_w <= 0 {
            return;
        }
        let r = r.clamp(0, w.min(h) / 2) as f32;
        let (hw, hh) = (w as f32 / 2.0, h as f32 / 2.0);
        let (cx, cy) = (x as f32 + hw, y as f32 + hh);
        let rw = ring_w as f32;
        let b = (x as f32, y as f32, (x + w) as f32, (y + h) as f32);
        self.shade(b, Some(c), |px, py| {
            let qx = (px - cx).abs() - (hw - r);
            let qy = (py - cy).abs() - (hh - r);
            let out = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt();
            let d = out + qx.max(qy).min(0.0) - r;
            // inside the outer edge and outside the inner one
            (0.5 - d)
                .clamp(0.0, 1.0)
                .min((d + rw + 0.5).clamp(0.0, 1.0))
        });
    }

    /// A rounded rectangle at sub-pixel coordinates (icons), filled, or
    /// only its outline `ring_w` wide inside the box when `ring_w > 0`.
    #[allow(clippy::too_many_arguments)]
    pub fn rounded_rect_f(&mut self, x: f32, y: f32, w: f32, h: f32, r: f32, ring_w: f32, c: Rgb) {
        if w <= 0.0 || h <= 0.0 {
            return;
        }
        let r = r.clamp(0.0, w.min(h) / 2.0);
        let (hw, hh) = (w / 2.0, h / 2.0);
        let (cx, cy) = (x + hw, y + hh);
        let b = (x - 1.0, y - 1.0, x + w + 1.0, y + h + 1.0);
        self.shade(b, Some(c), |px, py| {
            let qx = (px - cx).abs() - (hw - r);
            let qy = (py - cy).abs() - (hh - r);
            let out = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt();
            let d = out + qx.max(qy).min(0.0) - r;
            let fill = (0.5 - d).clamp(0.0, 1.0);
            if ring_w > 0.0 {
                fill.min((d + ring_w + 0.5).clamp(0.0, 1.0))
            } else {
                fill
            }
        });
    }

    /// Composites `src` (straight alpha) at (x, y) at `opacity`, clipped.
    pub fn blit(&mut self, src: &Canvas, x: i32, y: i32, opacity: u8) {
        if opacity == 0 {
            return;
        }
        for sy in 0..src.h as i32 {
            let dy = y + sy;
            if dy < 0 || dy >= self.h as i32 {
                continue;
            }
            for sx in 0..src.w as i32 {
                let dx = x + sx;
                if dx < 0 || dx >= self.w as i32 {
                    continue;
                }
                let i = (sy as usize * src.w + sx as usize) * 4;
                let a = (src.px[i + 3] as u32 * opacity as u32 + 127) / 255;
                if a == 0 {
                    continue;
                }
                let c = [src.px[i], src.px[i + 1], src.px[i + 2]];
                if a == 255 {
                    let j = (dy as usize * self.w + dx as usize) * 4;
                    self.px[j..j + 3].copy_from_slice(&c);
                    self.px[j + 3] = 255;
                } else {
                    self.blend(dx, dy, c, a as u8);
                }
            }
        }
    }

    /// A radial fade over the whole canvas (a vignette): clear inside the
    /// normalised radius `inner`, `a` at `outer` and beyond, smoothstep in
    /// between. The radius is 1 where the ellipse touches the edges' middles
    /// (the corners are at √2).
    pub fn radial_alpha(&mut self, c: Rgb, a: u8, inner: f32, outer: f32) {
        if a == 0 || self.w == 0 || self.h == 0 {
            return;
        }
        let (hw, hh) = (self.w as f32 / 2.0, self.h as f32 / 2.0);
        let span = (outer - inner).max(1e-6);
        for yy in 0..self.h {
            let ny = (yy as f32 + 0.5 - hh) / hh;
            for xx in 0..self.w {
                let nx = (xx as f32 + 0.5 - hw) / hw;
                let d = (nx * nx + ny * ny).sqrt();
                let t = ((d - inner) / span).clamp(0.0, 1.0);
                let k = (t * t * (3.0 - 2.0 * t) * a as f32).round() as u8;
                self.blend(xx as i32, yy as i32, c, k);
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
    fn ring_is_an_aa_annulus() {
        let mut t = Canvas::transparent(21, 21);
        t.ring(10.5, 10.5, 6.0, 2.0, [255, 255, 255]);
        // the centre and the far corners stay empty; the stroke is solid
        assert_eq!(alpha(&t, 10, 10), 0, "hollow");
        assert_eq!(alpha(&t, 0, 0), 0);
        assert_eq!(alpha(&t, 16, 10), 255, "on the stroke (r = 6)");
        assert_eq!(alpha(&t, 10, 4), 255);
        // along a ray the coverage rises then falls, with AA on both sides
        let ray: Vec<u8> = (10..21).map(|x| alpha(&t, x, 10)).collect();
        let peak = ray.iter().position(|a| *a == 255).unwrap();
        assert!(ray[..=peak].windows(2).all(|w| w[0] <= w[1]), "{ray:?}");
        assert!(ray[peak..].windows(2).all(|w| w[0] >= w[1]), "{ray:?}");
        // symmetric: the four quadrants mirror each other
        for y in 0..21 {
            for x in 0..21 {
                assert_eq!(alpha(&t, x, y), alpha(&t, 20 - x, y), "{x},{y}");
                assert_eq!(alpha(&t, x, y), alpha(&t, x, 20 - y), "{x},{y}");
            }
        }
        let partial =
            t.px.as_chunks::<4>()
                .0
                .iter()
                .filter(|p| p[3] > 0 && p[3] < 255);
        assert!(partial.count() > 8, "anti-aliased");
        // off-canvas parts clip; a zero width draws nothing
        let mut c = Canvas::transparent(8, 8);
        c.ring(0.0, 0.0, 6.0, 2.0, [1, 2, 3]);
        c.ring(4.0, 4.0, 3.0, 0.0, [1, 2, 3]);
        assert!(alpha(&c, 6, 0) > 0);
    }

    #[test]
    fn radial_alpha_darkens_the_edges_only() {
        let mut t = Canvas::transparent(64, 36);
        t.radial_alpha([10, 20, 30], 200, 0.5, 1.4);
        assert_eq!(alpha(&t, 32, 18), 0, "the centre is clear");
        assert_eq!(alpha(&t, 40, 18), 0, "inside `inner`");
        let corner = alpha(&t, 0, 0);
        assert!(corner > 150 && corner <= 200, "{corner}");
        // monotone from the centre outwards, along x and along the diagonal
        let row: Vec<u8> = (32..64).map(|x| alpha(&t, x, 18)).collect();
        assert!(row.windows(2).all(|w| w[0] <= w[1]), "{row:?}");
        let diag: Vec<u8> = (0..18)
            .map(|k| alpha(&t, 32 + k * 32 / 18, 18 + k))
            .collect();
        assert!(diag.windows(2).all(|w| w[0] <= w[1]), "{diag:?}");
        // mirror symmetric, colour kept
        assert_eq!(alpha(&t, 1, 5), alpha(&t, 62, 30));
        assert_eq!(&t.px[..3], &[10, 20, 30]);
        // alpha 0: untouched
        let mut z = Canvas::transparent(4, 4);
        z.radial_alpha([9, 9, 9], 0, 0.0, 1.0);
        assert!(z.px.iter().all(|v| *v == 0));
    }

    #[test]
    fn rect_clips() {
        let mut cv = Canvas::new(4, 4, [0, 0, 0]);
        cv.rect(-2, -2, 100, 3, [9, 9, 9]);
        assert_eq!(cv.px[0], 9);
        assert_eq!(cv.px[(3 * 4) * 4], 0);
    }

    // ---- wave 4 (W2): the toolkit's primitives ----

    fn rgb_at(cv: &Canvas, x: usize, y: usize) -> [u8; 3] {
        let i = (y * cv.w + x) * 4;
        [cv.px[i], cv.px[i + 1], cv.px[i + 2]]
    }

    #[test]
    fn disc_is_filled_aa_and_symmetric() {
        let mut cv = Canvas::transparent(20, 20);
        cv.disc(10.0, 10.0, 6.0, [200, 100, 50]);
        assert_eq!(alpha(&cv, 10, 10), 255);
        assert_eq!(rgb_at(&cv, 10, 10), [200, 100, 50]);
        assert_eq!(alpha(&cv, 10, 2), 0, "outside");
        let edge = alpha(&cv, 15, 10);
        assert!(edge > 0, "{edge}");
        for y in 0..20 {
            for x in 0..20 {
                assert_eq!(alpha(&cv, x, y), alpha(&cv, 19 - x, y), "mirror x {x},{y}");
                assert_eq!(alpha(&cv, x, y), alpha(&cv, x, 19 - y), "mirror y {x},{y}");
            }
        }
        // clipped at the canvas edge, no panic
        cv.disc(-3.0, 25.0, 8.0, [1, 2, 3]);
    }

    #[test]
    fn line_is_a_round_capped_stroke() {
        let mut cv = Canvas::transparent(30, 10);
        cv.line(5.0, 5.0, 25.0, 5.0, 2.0, [10, 20, 30]);
        assert_eq!(alpha(&cv, 15, 4), 255);
        assert_eq!(alpha(&cv, 15, 5), 255);
        assert_eq!(alpha(&cv, 15, 1), 0);
        assert_eq!(alpha(&cv, 1, 4), 0, "beyond the cap");
        assert!(alpha(&cv, 4, 4) > 0, "the cap is round");
        // a diagonal one touches its end points
        let mut d = Canvas::transparent(20, 20);
        d.line(2.0, 2.0, 18.0, 18.0, 2.0, [1, 1, 1]);
        assert_eq!(alpha(&d, 10, 10), 255);
        assert_eq!(alpha(&d, 17, 2), 0);
    }

    #[test]
    fn tri_fills_inside_its_edges_either_winding() {
        let mut a = Canvas::transparent(20, 20);
        a.tri((2.0, 18.0), (10.0, 2.0), (18.0, 18.0), [9, 9, 9]);
        let mut b = Canvas::transparent(20, 20);
        b.tri((18.0, 18.0), (10.0, 2.0), (2.0, 18.0), [9, 9, 9]);
        assert_eq!(a.px, b.px, "winding does not matter");
        assert_eq!(alpha(&a, 10, 14), 255);
        assert_eq!(alpha(&a, 2, 3), 0);
        for y in 0..20 {
            for x in 0..20 {
                assert_eq!(alpha(&a, x, y), alpha(&a, 19 - x, y), "{x},{y}");
            }
        }
    }

    #[test]
    fn rounded_rect_ring_is_hollow_with_its_width() {
        let mut cv = Canvas::transparent(40, 30);
        cv.rounded_rect_ring(0, 0, 40, 30, 8, 2, [255, 159, 67]);
        assert_eq!(alpha(&cv, 20, 15), 0, "hollow");
        assert_eq!(alpha(&cv, 20, 0), 255);
        assert_eq!(alpha(&cv, 20, 1), 255);
        assert_eq!(alpha(&cv, 20, 3), 0, "2 px wide");
        assert_eq!(alpha(&cv, 0, 0), 0, "rounded corner");
        assert_eq!(alpha(&cv, 39, 15), 255);
    }

    #[test]
    fn punches_cut_alpha_and_blit_composites() {
        let mut cv = Canvas::transparent(20, 20);
        cv.rect(0, 0, 20, 20, [50, 50, 50]);
        cv.punch_disc(10.0, 10.0, 3.0);
        assert_eq!(alpha(&cv, 10, 10), 0);
        assert_eq!(alpha(&cv, 1, 1), 255);
        cv.punch_rect(0.0, 0.0, 4.0, 4.0);
        assert_eq!(alpha(&cv, 1, 1), 0);
        cv.punch_line(0.0, 18.0, 19.0, 18.0, 2.0);
        assert_eq!(alpha(&cv, 10, 18), 0);
        cv.punch_tri((14.0, 0.0), (20.0, 0.0), (20.0, 6.0));
        assert_eq!(alpha(&cv, 19, 1), 0);
        // blit at half opacity over white
        let mut dst = Canvas::new(30, 30, [255, 255, 255]);
        dst.blit(&cv, 5, 5, 128);
        assert_eq!(rgb_at(&dst, 0, 0), [255, 255, 255]);
        let v = rgb_at(&dst, 5 + 8, 5 + 6);
        assert!(v[0] > 140 && v[0] < 160, "{v:?}");
        assert_eq!(rgb_at(&dst, 15, 15), [255, 255, 255], "the punched hole");
        // clipped
        dst.blit(&cv, 25, -5, 255);
    }
}
