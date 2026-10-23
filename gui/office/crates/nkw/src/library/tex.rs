//! Text on a quad in the world: the lectern's preview and the vault's
//! cards. Rastered on the CPU with `nk_cats::raster` (the shipped mono
//! font), uploaded in `upload()` as an RGBA texture, drawn as a
//! `ScreenQuad`. The cards' names only ever reach this texture: never a
//! tag, a dump or a log.

use glam::{Mat4, Quat, Vec3};
use nk_cats::raster::{Canvas, Face};
use nkw_gl::draw::{ClientTex, ScreenQuad};
use nkw_gl::glow::{self, HasContext};

pub const PAPER: [u8; 3] = [0xf6, 0xee, 0xdc];
pub const INK: [u8; 3] = [0x3a, 0x30, 0x2a];
pub const MARK: [u8; 3] = [0xe8, 0xc0, 0x80];
pub const CARD: [u8; 3] = [0xe6, 0xe0, 0xf0];

/// What to raster: lines of text, an optional highlighted line.
#[derive(Clone, Debug, PartialEq)]
pub struct Page {
    pub title: String,
    pub lines: Vec<String>,
    pub mark: Option<usize>,
    pub bg: [u8; 3],
}

/// Lines of `text` wrapped at `cols` characters (tabs as 4 spaces), at most
/// `rows`.
pub fn wrap(text: &str, cols: usize, rows: usize) -> Vec<String> {
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.replace('\t', "    ");
        let chars: Vec<char> = line.chars().filter(|c| !c.is_control()).collect();
        if chars.is_empty() {
            out.push(String::new());
        }
        for chunk in chars.chunks(cols.max(1)) {
            out.push(chunk.iter().collect());
        }
        if out.len() >= rows {
            out.truncate(rows);
            break;
        }
    }
    out
}

/// The page as RGBA px (`w`×`h`), title on top, a marked line behind a
/// warm band.
pub fn raster(face: &mut Face, page: &Page, w: usize, h: usize) -> Canvas {
    let mut c = Canvas::new(w, h, page.bg);
    let pad = 12;
    let lh = face.ch as i32 + 2;
    c.text_px(face, pad, pad, &page.title, INK, true);
    let mut y = pad + lh + 6;
    c.rect(pad, y - 4, w as i32 - 2 * pad, 1, INK);
    for (i, l) in page.lines.iter().enumerate() {
        if y + lh > h as i32 - pad {
            break;
        }
        if page.mark == Some(i) {
            c.rect(pad - 4, y - 1, w as i32 - 2 * pad + 8, lh, MARK);
        }
        c.text_px(face, pad, y, l, INK, false);
        y += lh;
    }
    c
}

/// The model of a `w`×`h` m text quad at `center`, looked at with `yaw`
/// and `pitch` (the `ScreenPlace` convention of `scene::screen_model`).
pub fn quad_model(center: Vec3, yaw: f32, pitch: f32, w: f32, h: f32) -> Mat4 {
    let rot = Quat::from_rotation_y(-yaw) * Quat::from_rotation_x(pitch);
    Mat4::from_scale_rotation_translation(Vec3::new(w, h, 1.0), rot, center)
}

/// A page and its texture.
#[derive(Default)]
pub struct TextTex {
    want: Option<(Page, usize, usize)>,
    /// rastered, waiting for `upload`
    canvas: Option<Canvas>,
    tex: Option<(glow::NativeTexture, [u32; 2])>,
    /// drop the texture at the next upload
    free: bool,
    face: Option<Face>,
}

impl TextTex {
    /// Show `page` (re-rastered only when it changed).
    pub fn set(&mut self, page: Page, w: usize, h: usize) {
        if self
            .want
            .as_ref()
            .is_some_and(|(p, pw, ph)| *p == page && *pw == w && *ph == h)
        {
            return;
        }
        let face = self
            .face
            .get_or_insert_with(|| Face::new(&Face::load_font(), 15.0));
        self.canvas = Some(raster(face, &page, w, h));
        self.want = Some((page, w, h));
        self.free = false;
    }

    /// Forget the page and its pixels (the texture goes at the next upload).
    pub fn clear(&mut self) {
        self.want = None;
        self.canvas = None;
        self.free = self.tex.is_some();
    }

    pub fn shown(&self) -> bool {
        self.want.is_some()
    }

    pub fn ready(&self) -> bool {
        self.want.is_some() && self.tex.is_some() && self.canvas.is_none()
    }

    /// Upload a fresh raster (or delete the texture after `clear`).
    ///
    /// # Safety
    /// `gl` is current on this thread.
    pub unsafe fn upload(&mut self, gl: &glow::Context) {
        if self.free {
            if let Some((t, _)) = self.tex.take() {
                unsafe { gl.delete_texture(t) };
            }
            self.free = false;
        }
        let Some(c) = self.canvas.take() else { return };
        let size = [c.w as u32, c.h as u32];
        unsafe {
            if self.tex.is_some_and(|(_, s)| s != size)
                && let Some((t, _)) = self.tex.take()
            {
                gl.delete_texture(t);
            }
            let fresh = self.tex.is_none();
            if fresh {
                match gl.create_texture() {
                    Ok(t) => self.tex = Some((t, size)),
                    Err(e) => {
                        tracing::warn!("library: texture: {e}");
                        return;
                    }
                }
            }
            let (t, _) = self.tex.expect("made above");
            gl.bind_texture(glow::TEXTURE_2D, Some(t));
            gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 4);
            gl.pixel_store_i32(glow::UNPACK_ROW_LENGTH, 0);
            if fresh {
                for (k, v) in [
                    (glow::TEXTURE_MIN_FILTER, glow::LINEAR),
                    (glow::TEXTURE_MAG_FILTER, glow::LINEAR),
                    (glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE),
                    (glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE),
                ] {
                    gl.tex_parameter_i32(glow::TEXTURE_2D, k, v as i32);
                }
            }
            gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                glow::RGBA8 as i32,
                c.w as i32,
                c.h as i32,
                0,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(Some(&c.px)),
            );
            gl.bind_texture(glow::TEXTURE_2D, None);
        }
    }

    /// The quad to draw, once uploaded.
    pub fn quad(&self, model: Mat4) -> Option<ScreenQuad> {
        self.want.as_ref()?;
        let (t, size) = self.tex?;
        Some(ScreenQuad {
            model,
            tex: ClientTex {
                id: t.0.get(),
                external: false,
                size,
                y_flip: false,
                opaque: true,
            },
            dim: 0.0,
            selected: false,
        })
    }

    /// The rastered pixels not yet uploaded (tests).
    pub fn pending(&self) -> Option<&Canvas> {
        self.canvas.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_cuts_long_lines_and_stops_at_rows() {
        let w = wrap("abcdef\n\n\tx\nlast", 4, 10);
        assert_eq!(w, ["abcd", "ef", "", "    ", "x", "last"]);
        assert_eq!(wrap("a\nb\nc\nd", 10, 2), ["a", "b"]);
        assert!(wrap("", 10, 5).is_empty());
    }

    #[test]
    fn a_page_rasters_ink_on_paper_and_only_when_it_changed() {
        let mut t = TextTex::default();
        let page = Page {
            title: "notas.md".into(),
            lines: vec!["olá".into(), "mundo".into()],
            mark: Some(1),
            bg: PAPER,
        };
        t.set(page.clone(), 200, 120);
        let c = t.pending().unwrap();
        assert_eq!((c.w, c.h), (200, 120));
        // something darker than the paper was drawn, and the mark band
        let px = c.px.as_chunks::<4>().0;
        assert!(px.iter().any(|p| p[0] < 0x80));
        assert!(px.iter().any(|p| p[..3] == MARK));
        assert!(t.shown() && !t.ready());
        t.canvas = None;
        t.set(page, 200, 120);
        assert!(t.pending().is_none(), "same page: no new raster");
        t.clear();
        assert!(!t.shown());
        assert!(t.quad(Mat4::IDENTITY).is_none());
    }

    #[test]
    fn the_quad_model_faces_its_viewer() {
        let m = quad_model(Vec3::new(1.0, 1.5, 2.0), 0.0, 0.0, 0.4, 0.5);
        assert_eq!(m.transform_point3(Vec3::ZERO), Vec3::new(1.0, 1.5, 2.0));
        let x = m.transform_vector3(Vec3::X);
        assert!((x.length() - 0.4).abs() < 1e-5);
    }
}
