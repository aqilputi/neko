//! A cat's monitor (§6.3): without a pane, the session's tail
//! (`nk_cats::monitor::Feed`), coloured by kind; with one, the live
//! terminal (`nk_client::Term`). Either becomes a grid of cells rasterised
//! by `nk_cats::raster::draw_grid` into an RGBA canvas, and only the text
//! rows that changed are drawn again ([`Raster::paint`] returns the dirty
//! pixel rows, which is what goes to the GL texture or the 2D buffer).

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use nk_cats::model::Palette;
use nk_cats::monitor::{Feed, Line, Role, clip, role_of};
use nk_cats::raster::{Canvas, Face, TCell, draw_grid};
use nkw_gl::glow::{self, HasContext};

/// Font size of the monitor in NAV (texture px), and of the terminal in
/// FOCO (logical px).
pub const NAV_PX: f32 = 11.0;
pub const FOCUS_PX: f32 = 15.0;
/// The NAV texture is the monitor's logical size divided by this.
pub const NAV_DIV: f32 = 2.0;
/// Tail lines asked of the core.
pub const TAIL_N: usize = 40;
/// Padding around the text, texture px.
pub const PAD: usize = 6;

/// `(cols, rows)` of a terminal that fits `w`×`h` px with cells `cw`×`ch`
/// (at least 20×5, the core's minimum is lower).
pub fn fit_cells(w: f32, h: f32, cw: usize, ch: usize) -> (u16, u16) {
    let cols = ((w.max(0.0) as usize).saturating_sub(2 * PAD) / cw.max(1)).max(20);
    let rows = ((h.max(0.0) as usize).saturating_sub(2 * PAD) / ch.max(1)).max(5);
    (cols.min(500) as u16, rows.min(300) as u16)
}

/// The colours of a tail line's role.
pub fn role_rgb(p: &Palette, r: Role) -> [u8; 3] {
    match r {
        Role::Tool => p.accent,
        Role::Text => p.text,
        Role::User => p.subtle,
        Role::Result => p.muted,
        Role::Sub => p.accent2,
        Role::Add => p.done,
        Role::Del => p.error,
    }
}

/// A core tail line as a monitor line.
pub fn line_of(l: &nk_client::TailLine) -> Line {
    Line {
        role: role_of(&l.kind, &l.text),
        at: l.at.clone(),
        text: l.text.clone(),
    }
}

fn blank(p: &Palette) -> TCell {
    TCell {
        ch: ' ',
        fg: p.text,
        bg: p.bg,
        bold: false,
        underline: false,
        inverse: false,
        wide: false,
    }
}

/// The tail as a `cols`×`rows` grid: a header row (the label in the
/// context's colour), then the newest lines that fit, oldest on top.
pub fn tail_cells(
    feed: &Feed,
    header: &str,
    header_rgb: [u8; 3],
    p: &Palette,
    cols: usize,
    rows: usize,
) -> Vec<TCell> {
    let mut cells = vec![blank(p); cols * rows];
    let mut put = |row: usize, text: &str, fg: [u8; 3], bold: bool| {
        let mut c = 0;
        for ch in text.chars() {
            let w = nk_cats::monitor::char_width(ch);
            if c + w > cols {
                break;
            }
            cells[row * cols + c] = TCell {
                ch,
                fg,
                bold,
                wide: w == 2,
                ..blank(p)
            };
            c += w;
        }
    };
    if rows == 0 {
        return cells;
    }
    put(0, &clip(header, cols), header_rgb, true);
    for (i, l) in feed.visible(rows.saturating_sub(1)).iter().enumerate() {
        put(
            i + 1,
            &clip(&l.text, cols),
            role_rgb(p, l.role),
            l.role == Role::Tool,
        );
    }
    cells
}

/// A terminal grid as cells.
pub fn grid_cells(g: &nk_client::Grid) -> Vec<TCell> {
    g.cells
        .iter()
        .map(|c| TCell {
            ch: c.ch,
            fg: c.fg,
            bg: c.bg,
            bold: c.bold,
            underline: c.underline,
            inverse: c.inverse,
            wide: c.wide,
        })
        .collect()
}

fn row_hash(cells: &[TCell], cursor: Option<usize>) -> u64 {
    let mut h = DefaultHasher::new();
    for c in cells {
        (c.ch, c.fg, c.bg, c.bold, c.underline, c.inverse, c.wide).hash(&mut h);
    }
    cursor.hash(&mut h);
    h.finish()
}

/// The text rows whose hash changed, merged into runs `(first, count)`.
pub fn dirty_runs(old: &[u64], new: &[u64]) -> Vec<(usize, usize)> {
    let mut runs: Vec<(usize, usize)> = Vec::new();
    for (i, h) in new.iter().enumerate() {
        if old.get(i) == Some(h) {
            continue;
        }
        match runs.last_mut() {
            Some((a, n)) if *a + *n == i => *n += 1,
            _ => runs.push((i, 1)),
        }
    }
    runs
}

/// A canvas and what is on it, by text row.
pub struct Raster {
    pub canvas: Canvas,
    pub cols: usize,
    pub rows: usize,
    hashes: Vec<u64>,
    bg: [u8; 3],
}

impl Raster {
    /// A `w`×`h` canvas for cells of `face`.
    pub fn new(w: usize, h: usize, face: &Face, bg: [u8; 3]) -> Raster {
        let cols = w.saturating_sub(2 * PAD) / face.cw.max(1);
        let rows = h.saturating_sub(2 * PAD) / face.ch.max(1);
        Raster {
            canvas: Canvas::new(w, h, bg),
            cols,
            rows,
            hashes: Vec::new(),
            bg,
        }
    }

    pub fn size(&self) -> (usize, usize) {
        (self.canvas.w, self.canvas.h)
    }

    /// Draw `cells` (`cols`×`rows` of the raster; a grid of another size is
    /// cut or padded) and return the dirty pixel rows `(y, height)`.
    pub fn paint(
        &mut self,
        face: &mut Face,
        cols: usize,
        cells: &[TCell],
        cursor: Option<(usize, usize)>,
        cursor_rgb: [u8; 3],
    ) -> Vec<(usize, usize)> {
        let rows = self.rows;
        let fill = TCell {
            ch: ' ',
            fg: self.bg,
            bg: self.bg,
            bold: false,
            underline: false,
            inverse: false,
            wide: false,
        };
        // the grid at our size
        let mut grid = vec![fill; self.cols * rows];
        for r in 0..rows {
            for c in 0..self.cols.min(cols) {
                if let Some(cell) = cells.get(r * cols + c) {
                    grid[r * self.cols + c] = cell.clone();
                }
            }
        }
        let hashes: Vec<u64> = (0..rows)
            .map(|r| {
                let cur = cursor.filter(|(_, y)| *y == r).map(|(x, _)| x);
                row_hash(&grid[r * self.cols..(r + 1) * self.cols], cur)
            })
            .collect();
        let first = self.hashes.is_empty();
        let runs = if first {
            vec![(0, rows)]
        } else {
            dirty_runs(&self.hashes, &hashes)
        };
        self.hashes = hashes;
        let mut out = Vec::new();
        if first {
            self.canvas.fill(self.bg);
            out.push((0, self.canvas.h));
        }
        let (w, ch) = (self.canvas.w, face.ch);
        for (r0, n) in runs {
            if n == 0 {
                continue;
            }
            let band_cells = &grid[r0 * self.cols..(r0 + n) * self.cols];
            let cur = cursor
                .filter(|(_, y)| *y >= r0 && *y < r0 + n)
                .map(|(x, y)| (x, y - r0));
            let mut band = Canvas::new(w, n * ch, self.bg);
            // the grid starts PAD px in: draw into a band offset by PAD
            let mut inner = Canvas::new(w.saturating_sub(PAD), n * ch, self.bg);
            draw_grid(
                &mut inner, face, self.cols, n, band_cells, cur, cursor_rgb, true,
            );
            for y in 0..n * ch {
                let src = &inner.px[y * inner.w * 4..(y + 1) * inner.w * 4];
                band.px[(y * w + PAD) * 4..(y * w + PAD) * 4 + src.len()].copy_from_slice(src);
            }
            let y0 = PAD + r0 * ch;
            for y in 0..n * ch {
                if y0 + y >= self.canvas.h {
                    break;
                }
                let dst = (y0 + y) * w * 4;
                self.canvas.px[dst..dst + w * 4]
                    .copy_from_slice(&band.px[y * w * 4..(y + 1) * w * 4]);
            }
            if !first {
                out.push((y0, (n * ch).min(self.canvas.h.saturating_sub(y0))));
            }
        }
        out
    }
}

/// A GL texture holding a raster (the monitor in NAV).
#[derive(Debug)]
pub struct Tex {
    pub id: glow::NativeTexture,
    pub size: [u32; 2],
}

/// Upload `raster` to `tex` (created or re-created at its size): the whole
/// canvas the first time, else only `dirty` rows.
///
/// # Safety
/// `gl` is current on this thread.
pub unsafe fn upload(
    gl: &glow::Context,
    tex: &mut Option<Tex>,
    raster: &Raster,
    dirty: &[(usize, usize)],
) -> Result<(), String> {
    let (w, h) = raster.size();
    let size = [w as u32, h as u32];
    unsafe {
        let fresh = tex.as_ref().is_none_or(|t| t.size != size);
        if fresh {
            if let Some(t) = tex.take() {
                gl.delete_texture(t.id);
            }
            let id = gl.create_texture()?;
            *tex = Some(Tex { id, size });
        }
        let t = tex.as_ref().expect("just made");
        gl.bind_texture(glow::TEXTURE_2D, Some(t.id));
        gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 4);
        gl.pixel_store_i32(glow::UNPACK_ROW_LENGTH, 0);
        if fresh {
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MIN_FILTER,
                glow::LINEAR as i32,
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MAG_FILTER,
                glow::LINEAR as i32,
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_WRAP_S,
                glow::CLAMP_TO_EDGE as i32,
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_WRAP_T,
                glow::CLAMP_TO_EDGE as i32,
            );
            gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                glow::RGBA8 as i32,
                w as i32,
                h as i32,
                0,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(Some(&raster.canvas.px)),
            );
        } else {
            for &(y, n) in dirty {
                if n == 0 || y >= h {
                    continue;
                }
                let n = n.min(h - y);
                gl.tex_sub_image_2d(
                    glow::TEXTURE_2D,
                    0,
                    0,
                    y as i32,
                    w as i32,
                    n as i32,
                    glow::RGBA,
                    glow::UNSIGNED_BYTE,
                    glow::PixelUnpackData::Slice(Some(
                        &raster.canvas.px[y * w * 4..(y + n) * w * 4],
                    )),
                );
            }
        }
        gl.bind_texture(glow::TEXTURE_2D, None);
    }
    Ok(())
}

/// Free a texture.
///
/// # Safety
/// `gl` is current on this thread.
pub unsafe fn free(gl: &glow::Context, tex: &mut Option<Tex>) {
    if let Some(t) = tex.take() {
        unsafe { gl.delete_texture(t.id) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn face(px: f32) -> Face {
        Face::new(&Face::load_font(), px)
    }

    fn line(role: Role, t: &str) -> Line {
        Line {
            at: String::new(),
            role,
            text: t.into(),
        }
    }

    #[test]
    fn fit_cells_fills_the_rect_with_whole_cells() {
        assert_eq!(fit_cells(812.0, 412.0, 8, 20), (100, 20));
        assert_eq!(fit_cells(10.0, 10.0, 8, 20), (20, 5), "the minimum");
        assert_eq!(fit_cells(1e6, 1e6, 1, 1), (500, 300), "the maximum");
        let f = face(FOCUS_PX);
        let (c, r) = fit_cells(640.0, 700.0, f.cw, f.ch);
        assert!(c as usize * f.cw <= 640 - 2 * PAD && r as usize * f.ch <= 700 - 2 * PAD);
        assert!((c as usize + 1) * f.cw > 640 - 2 * PAD);
    }

    #[test]
    fn dirty_runs_merge_neighbours() {
        assert_eq!(dirty_runs(&[], &[1, 2, 3]), [(0, 3)]);
        assert_eq!(
            dirty_runs(&[1, 2, 3, 4, 5], &[1, 9, 9, 4, 9]),
            [(1, 2), (4, 1)]
        );
        assert!(dirty_runs(&[1, 2], &[1, 2]).is_empty());
        assert_eq!(dirty_runs(&[1], &[1, 2]), [(1, 1)], "a new row");
    }

    #[test]
    fn the_tail_as_cells() {
        let p = Palette::default();
        let mut feed = Feed::default();
        feed.update(
            vec![
                line(Role::Tool, "● Edit(a.go)"),
                line(Role::Add, "+ x"),
                line(Role::Text, "uma linha bem comprida que não cabe"),
            ],
            false,
        );
        let cells = tail_cells(&feed, "acme", [1, 2, 3], &p, 12, 3);
        let row =
            |r: usize| -> String { cells[r * 12..(r + 1) * 12].iter().map(|c| c.ch).collect() };
        assert_eq!(row(0).trim_end(), "acme");
        assert_eq!(cells[0].fg, [1, 2, 3]);
        assert!(cells[0].bold);
        // two rows left: the newest two lines, clipped
        assert_eq!(row(1).trim_end(), "+ x");
        assert_eq!(cells[12].fg, p.done);
        assert_eq!(row(2), "uma linha b…");
        assert_eq!(cells[24].fg, p.text);
    }

    #[test]
    fn painting_twice_redraws_only_what_changed() {
        let p = Palette::default();
        let mut f = face(NAV_PX);
        let mut r = Raster::new(320, 200, &f, p.bg);
        assert!(r.rows >= 5 && r.cols >= 20);
        let (cols, rows) = (r.cols, r.rows);
        let mut feed = Feed::default();
        feed.update(vec![line(Role::Text, "olá")], false);
        let cells = tail_cells(&feed, "acme", p.accent, &p, cols, rows);
        let first = r.paint(&mut f, cols, &cells, None, p.text);
        assert_eq!(first, [(0, 200)], "everything the first time");
        let again = r.paint(&mut f, cols, &cells, None, p.text);
        assert!(again.is_empty(), "nothing changed");
        let before = r.canvas.px.clone();
        feed.update(
            vec![line(Role::Text, "olá"), line(Role::Tool, "● Bash(ls)")],
            false,
        );
        let cells = tail_cells(&feed, "acme", p.accent, &p, cols, rows);
        let d = r.paint(&mut f, cols, &cells, None, p.text);
        assert_eq!(d, [(PAD + 2 * f.ch, f.ch)], "the third text row only");
        // pixels changed only inside that band
        let w = r.canvas.w * 4;
        for y in 0..r.canvas.h {
            let same = before[y * w..(y + 1) * w] == r.canvas.px[y * w..(y + 1) * w];
            let inside = y >= PAD + 2 * f.ch && y < PAD + 3 * f.ch;
            assert!(same || inside, "row {y} changed outside the band");
        }
        assert!(before != r.canvas.px);
        // the text is really there: some pixel differs from the background
        assert!(r.canvas.px.chunks(4).any(|c| c[..3] != p.bg));
    }

    #[test]
    fn a_terminal_grid_and_its_cursor() {
        let mut t = nk_client::Term::new(20, 5);
        t.feed(b"oi\r\n\x1b[1mtudo\x1b[0m bem");
        let g = t.grid();
        let cells = grid_cells(&g);
        assert_eq!(cells.len(), 100);
        assert_eq!(cells[0].ch, 'o');
        assert!(cells[20].bold);
        let p = Palette::default();
        let mut f = face(FOCUS_PX);
        let mut r = Raster::new(400, 200, &f, p.bg);
        let cur = g.cursor.map(|(x, y)| (x as usize, y as usize));
        r.paint(&mut f, 20, &cells, cur, p.text);
        // moving the cursor dirties its rows only
        let d = r.paint(&mut f, 20, &cells, Some((0, 3)), p.text);
        let rows: Vec<usize> = d.iter().map(|(y, _)| (y - PAD) / f.ch).collect();
        assert_eq!(rows, [1, 3]);
    }

    #[test]
    fn roles_have_their_colours() {
        let p = Palette::default();
        assert_eq!(role_rgb(&p, Role::Del), p.error);
        assert_eq!(role_rgb(&p, Role::Sub), p.accent2);
        let l = line_of(&nk_client::TailLine {
            at: "t".into(),
            kind: "tool".into(),
            text: "● X".into(),
        });
        assert_eq!(l.role, Role::Tool);
    }
}
