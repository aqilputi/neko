//! The cats' monitors and terminals (§6.3, D6): each visible cat's monitor
//! is a texture painted from its tail or its pane's terminal (dirty rows
//! only), drawn as a screen quad on its desk's monitor; FOCO on a cat opens
//! or attaches its pane, sized to the 1:1 rectangle, composed from a 2D
//! buffer, and the keys go to the pane through `encode_key`.

use std::collections::HashMap;

use glam::Vec3;
use nk_cats::monitor::Feed;
use nk_cats::raster::Face;
use nk_client::{Key, Mods, PaneConn, Term};
use nkw_core::onetoone::M_PER_PX;
use nkw_core::{CatId, Target};
use nkw_gl::draw::{ClientTex, DrawList, ScreenQuad};
use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::element::memory::MemoryRenderBuffer;
use smithay::utils::{Rectangle, Transform};

use super::Cats;
use super::monitor::{self, FOCUS_PX, NAV_DIV, NAV_PX, Raster, Tex};
use crate::ext::Ctx;
use crate::router::KeyEv;

/// A cat's monitor in the world.
#[derive(Default)]
pub struct Mon {
    pub feed: Feed,
    pub raster: Option<Raster>,
    pub tex: Option<Tex>,
    /// pixel rows painted but not uploaded yet
    pub dirty: Vec<(usize, usize)>,
    /// something changed since the last paint
    pub stale: bool,
}

/// A session's pane: the connection (none in the fixture) and its terminal.
pub struct Pane {
    pub session: String,
    pub conn: Option<PaneConn>,
    pub term: Term,
    pub cols: u16,
    pub rows: u16,
    pub exited: bool,
}

/// FOCO on a cat's terminal: the 2D buffer and what is on it.
pub struct Focus {
    pub cat: CatId,
    pub raster: Raster,
    pub buf: MemoryRenderBuffer,
    pub pos: (i32, i32),
}

/// The fonts, one face per pixel size (two sizes: NAV and FOCO).
#[derive(Default)]
pub struct Fonts {
    faces: HashMap<u32, Face>,
}

impl Fonts {
    pub fn face(&mut self, px: f32) -> &mut Face {
        self.faces
            .entry(px.to_bits())
            .or_insert_with(|| Face::new(&Face::load_font(), px))
    }

    /// Parse the font and rasterize the common glyphs of both sizes now
    /// (`start`), not in the first frame's tick.
    pub fn warm(&mut self) {
        const COMMON: &str = " !\"#$%&'()*+,-./0123456789:;<=>?@ABCDEFGHIJKLMNOPQRSTUVWXYZ[\\]^_`\
                              abcdefghijklmnopqrstuvwxyz{|}~áàâãéêíóôõúçÁÉÍÓÚÇ●⎿◆❯⏺…·└";
        for px in [NAV_PX, FOCUS_PX] {
            let face = self.face(px);
            let mut c = nk_cats::raster::Canvas::new(8, 8, [0; 3]);
            for ch in COMMON.chars() {
                c.glyph(face, 0, 0, ch, [255; 3], false);
            }
        }
    }
}

/// A key event as the PTY's key (`None`: nothing to send).
pub fn pty_key(ev: &KeyEv) -> Option<(Key, Mods)> {
    let mods = Mods {
        ctrl: ev.mods.ctrl,
        alt: ev.mods.alt,
        shift: ev.mods.shift,
    };
    let key = match ev.sym {
        0xff0d | 0xff8d => Key::Enter,
        0xff1b => Key::Esc,
        0xff09 | 0xfe20 => Key::Tab,
        0xff08 => Key::Backspace,
        0xff52 => Key::Up,
        0xff54 => Key::Down,
        0xff51 => Key::Left,
        0xff53 => Key::Right,
        0xff50 => Key::Home,
        0xff57 => Key::End,
        0xff55 => Key::PageUp,
        0xff56 => Key::PageDown,
        0xff63 => Key::Insert,
        0xffff => Key::Delete,
        s @ 0xffbe..=0xffc9 => Key::F((s - 0xffbe + 1) as u8),
        s @ 0x20..=0x7e => Key::Char(char::from_u32(s)?),
        _ => {
            let c = ev.utf8.as_deref()?.chars().next()?;
            if c.is_control() {
                return None;
            }
            Key::Char(c)
        }
    };
    // a shifted printable already is the shifted char
    let mods = match key {
        Key::Char(_) => Mods {
            shift: false,
            ..mods
        },
        _ => mods,
    };
    Some((key, mods))
}

impl Cats {
    /// The monitor of a cat (made on first use).
    pub(super) fn mon(&mut self, cat: CatId) -> &mut Mon {
        self.mons.entry(cat).or_default()
    }

    /// A tail arrived (or the fixture's): new lines scroll in.
    pub(super) fn feed_tail(&mut self, cat: CatId, lines: &[nk_client::TailLine], animate: bool) {
        let animate = animate && !self.reduced_motion;
        let m = self.mon(cat);
        m.feed
            .update(lines.iter().map(monitor::line_of).collect(), animate);
        m.stale = true;
    }

    /// Paint the visible cats' monitors that changed (tail reveal, pane
    /// output); returns whether anything needs uploading.
    pub(super) fn paint_monitors(&mut self, ctx: &Ctx) -> bool {
        let w = ctx.world;
        let now_ms = (ctx.now * 1000.0) as i64;
        let mut any = false;
        let mut seen = Vec::new();
        for room in w.rooms.iter().filter(|r| ctx.visible_rooms.contains(&r.id)) {
            for (d, desk) in room.desks.iter().enumerate() {
                let Some(cat) = desk.cat else { continue };
                let Some((_, logical)) = w.monitor_place(room.id, d) else {
                    continue;
                };
                seen.push((cat, logical));
            }
        }
        // a monitor seen for the first time is painted whole: one per tick
        let mut fresh_left = 1;
        for (cat, logical) in seen {
            if !self.mons.get(&cat).is_some_and(|m| m.raster.is_some()) {
                if fresh_left == 0 {
                    any = true; // come back next frame
                    continue;
                }
                fresh_left -= 1;
            }
            let (header, rgb) = match self.office.get(cat) {
                Some(v) => (format!("{} · {}", v.label(), v.sess.state), v.ctx_rgb),
                None if cat == super::office::RECEPTIONIST => {
                    ("recepção · avisos".to_string(), self.office.palette.accent)
                }
                None => (String::new(), self.office.palette.accent),
            };
            let palette = self.office.palette.clone();
            let size = (
                ((logical.x / NAV_DIV).round() as usize).max(64),
                ((logical.y / NAV_DIV).round() as usize).max(48),
            );
            let session = self.office.get(cat).map(|v| v.sess.session_id.clone());
            let pane_cells = session.as_ref().and_then(|s| self.panes.get(s)).map(|p| {
                let g = p.term.grid();
                (g.cols as usize, monitor::grid_cells(&g))
            });
            let fonts = &mut self.fonts;
            let m = self.mons.entry(cat).or_default();
            if m.feed.tick(now_ms) {
                m.stale = true;
            }
            if m.raster.as_ref().is_none_or(|r| r.size() != size) {
                let face = fonts.face(NAV_PX);
                m.raster = Some(Raster::new(size.0, size.1, face, palette.bg));
                m.stale = true;
            }
            if !m.stale {
                continue;
            }
            m.stale = false;
            let face = fonts.face(NAV_PX);
            let r = m.raster.as_mut().expect("made above");
            let (cols, rows) = (r.cols, r.rows);
            let dirty = match &pane_cells {
                Some((gc, cells)) => r.paint(face, *gc, cells, None, palette.text),
                None => {
                    let cells = monitor::tail_cells(&m.feed, &header, rgb, &palette, cols, rows);
                    r.paint(face, cols, &cells, None, palette.text)
                }
            };
            if !dirty.is_empty() {
                m.dirty.extend(dirty);
                any = true;
            }
        }
        any
    }

    /// The next tail reveal of a visible monitor (≤ 10 Hz).
    pub(super) fn monitor_wake(&self, ctx: &Ctx) -> Option<f64> {
        let now_ms = (ctx.now * 1000.0) as i64;
        self.mons
            .iter()
            .filter(|(c, _)| crate::ext::cat_visible(ctx, **c))
            .filter_map(|(_, m)| m.feed.next_reveal(now_ms))
            .min()
            .map(|t| t as f64 / 1000.0)
    }

    /// Upload the painted rows.
    ///
    /// # Safety
    /// `gl` is current on this thread.
    pub(super) unsafe fn upload_monitors(&mut self, gl: &nkw_gl::glow::Context) {
        for m in self.mons.values_mut() {
            let Some(r) = m.raster.as_ref() else { continue };
            if m.dirty.is_empty() && m.tex.is_some() {
                continue;
            }
            let dirty = std::mem::take(&mut m.dirty);
            if let Err(e) = unsafe { monitor::upload(gl, &mut m.tex, r, &dirty) } {
                tracing::warn!("cats: monitor texture: {e}");
            }
        }
    }

    /// The monitors of the visible cats as screen quads, a hair in front of
    /// the dark screen the scene draws for an empty monitor.
    pub(super) fn draw_monitors(&self, ctx: &Ctx, list: &mut DrawList) {
        let w = ctx.world;
        let hidden = w.focus_2d_target().map(|(t, _)| t);
        for room in w.rooms.iter().filter(|r| ctx.visible_rooms.contains(&r.id)) {
            for (d, desk) in room.desks.iter().enumerate() {
                let Some(cat) = desk.cat else { continue };
                if !desk.screens.is_empty() || hidden == Some(Target::CatTerm(cat)) {
                    continue;
                }
                let (Some(m), Some((place, logical))) =
                    (self.mons.get(&cat), w.monitor_place(room.id, d))
                else {
                    continue;
                };
                let Some(t) = &m.tex else { continue };
                let size = logical * M_PER_PX;
                let normal = nkw_core::Pose::new(Vec3::ZERO, place.yaw, place.pitch).forward();
                let mut p = place;
                p.center -= normal * 0.003;
                list.screens.push(ScreenQuad {
                    model: crate::scene::screen_model(&p, Vec3::new(size.x, size.y, 1.0)),
                    tex: ClientTex {
                        id: t.id.0.get(),
                        external: false,
                        size: t.size,
                        y_flip: false,
                        opaque: true,
                    },
                    dim: 0.0,
                    selected: false,
                });
            }
        }
    }

    /// `Effect::OpenPane`: open (or reuse) the cat's pane at the size of
    /// its 1:1 rectangle.
    pub(super) fn open_pane(&mut self, cat: CatId, ctx: &Ctx) {
        let Some(o) = ctx.world.one_to_one_target(Target::CatTerm(cat)) else {
            return;
        };
        let s = ctx.scale as f32;
        let (w, h) = (
            (o.size.x / s).round() as usize,
            (o.size.y / s).round() as usize,
        );
        let bg = self.office.palette.bg;
        let face = self.fonts.face(FOCUS_PX);
        let raster = Raster::new(w.max(64), h.max(48), face, bg);
        let (cols, rows) = (raster.cols.max(20) as u16, raster.rows.max(5) as u16);
        let pos = (
            (o.origin.x / s).round() as i32,
            (o.origin.y / s).round() as i32,
        );
        let buf = MemoryRenderBuffer::new(
            Fourcc::Abgr8888,
            (raster.canvas.w as i32, raster.canvas.h as i32),
            1,
            Transform::Normal,
            None,
        );
        self.focus = Some(Focus {
            cat,
            raster,
            buf,
            pos,
        });
        let Some(v) = self.office.get(cat) else {
            return;
        };
        let (session, label) = (v.sess.session_id.clone(), v.label().to_string());
        self.ensure_pane(&session, &label, cols, rows, None);
    }

    /// The pane of a session at `cols`×`rows` (opened, reused or resized);
    /// `reply`: text typed in once it is up (core mode).
    pub(super) fn ensure_pane(
        &mut self,
        session: &str,
        label: &str,
        cols: u16,
        rows: u16,
        reply: Option<String>,
    ) {
        if let Some(p) = self.panes.get_mut(session) {
            if (p.cols, p.rows) != (cols, rows) {
                p.term.resize(cols, rows);
                if let Some(c) = &p.conn {
                    c.resize(cols, rows);
                }
                p.cols = cols;
                p.rows = rows;
            }
            if let (Some(text), Some(link)) = (reply, &self.link) {
                // the core types it (bracketed paste + enter)
                let _ = link.tx.send(super::core_link::ToCore::Pane {
                    session: session.into(),
                    cols,
                    rows,
                    reply: Some(text),
                });
            }
            return;
        }
        let mut term = Term::new(cols, rows);
        let p = &self.office.palette;
        term.set_default_colors(p.text, p.bg);
        match &self.link {
            None => term.feed(&super::demo::pane_text(label)),
            Some(link) => {
                let _ = link.tx.send(super::core_link::ToCore::Pane {
                    session: session.into(),
                    cols,
                    rows,
                    reply,
                });
            }
        }
        self.panes.insert(
            session.to_string(),
            Pane {
                session: session.to_string(),
                conn: None,
                term,
                cols,
                rows,
                exited: false,
            },
        );
    }

    /// Bytes from a pane: into its terminal, the replies back.
    pub(super) fn pane_data(&mut self, session: &str, bytes: &[u8]) {
        let Some(p) = self.panes.get_mut(session) else {
            return;
        };
        p.term.feed(bytes);
        let replies = p.term.take_replies();
        if !replies.is_empty()
            && let Some(c) = &p.conn
        {
            c.write(&replies);
        }
        if let Some(cat) = self.office.by_session(session).map(|v| v.cat) {
            self.mon(cat).stale = true;
        }
    }

    /// FOCO(CatTerm): the terminal at 1:1, only its changed rows redrawn.
    pub fn focus_buffer(
        &mut self,
        cat: CatId,
        ctx: &Ctx,
    ) -> Option<(MemoryRenderBuffer, (i32, i32))> {
        let _ = ctx;
        let session = self.office.get(cat)?.sess.session_id.clone();
        let (cells, cols, cursor) = {
            let p = self.panes.get(&session)?;
            let g = p.term.grid();
            let cur = g.cursor.map(|(x, y)| (x as usize, y as usize));
            (monitor::grid_cells(&g), g.cols as usize, cur)
        };
        let text = self.office.palette.text;
        let face = self.fonts.face(FOCUS_PX);
        let f = self.focus.as_mut().filter(|f| f.cat == cat)?;
        let dirty = f.raster.paint(face, cols, &cells, cursor, text);
        if !dirty.is_empty() {
            let w = f.raster.canvas.w;
            let px = &f.raster.canvas.px;
            let mut rc = f.buf.render();
            let _ = rc.draw(|mem: &mut [u8]| {
                let mut damage = Vec::new();
                for &(y, n) in &dirty {
                    let (a, b) = (y * w * 4, ((y + n) * w * 4).min(px.len()));
                    if b <= mem.len() {
                        mem[a..b].copy_from_slice(&px[a..b]);
                    }
                    damage.push(Rectangle::new(
                        (0, y as i32).into(),
                        (w as i32, n as i32).into(),
                    ));
                }
                Ok::<_, ()>(damage)
            });
        }
        Some((f.buf.clone(), f.pos))
    }

    /// FOCO(CatTerm) + `Route::Forward`: a key for the cat's pane.
    pub fn key_to_pane(&mut self, cat: CatId, ev: &KeyEv) {
        if !ev.pressed {
            return;
        }
        let Some(session) = self.office.get(cat).map(|v| v.sess.session_id.clone()) else {
            return;
        };
        let Some(p) = self.panes.get_mut(&session) else {
            return;
        };
        let Some((key, mods)) = pty_key(ev) else {
            return;
        };
        let bytes = nk_client::encode_key_mode(key, mods, p.term.app_cursor());
        match &p.conn {
            Some(c) => c.write(&bytes),
            // the fixture: a still terminal that echoes what you type
            None => {
                let echo: Vec<u8> = match key {
                    Key::Enter => b"\r\n".to_vec(),
                    Key::Backspace => b"\x08 \x08".to_vec(),
                    Key::Char(_) if !mods.ctrl && !mods.alt => bytes,
                    _ => Vec::new(),
                };
                p.term.feed(&echo);
            }
        }
        self.mon(cat).stale = true;
    }

    /// What a cat's monitor shows, as text lines (its pane's screen, else
    /// its tail), trailing blanks trimmed: for the dump and the e2e.
    pub(super) fn screen_text(&self, cat: CatId) -> Vec<String> {
        let pane = self
            .office
            .get(cat)
            .and_then(|v| self.panes.get(&v.sess.session_id));
        match (pane, self.mons.get(&cat)) {
            (Some(p), _) => {
                let g = p.term.grid();
                let mut v: Vec<String> = (0..g.rows)
                    .map(|r| g.row_text(r).trim_end().to_string())
                    .collect();
                while v.last().is_some_and(|l| l.is_empty()) {
                    v.pop();
                }
                v
            }
            (None, Some(m)) => m
                .feed
                .visible(usize::MAX)
                .iter()
                .map(|l| l.text.clone())
                .collect(),
            _ => Vec::new(),
        }
    }

    /// `KeyboardLeave` from a cat's terminal: the pane stays, the 2D buffer
    /// goes.
    pub(super) fn leave_pane(&mut self) {
        self.focus = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::router::KeyMods;

    fn ev(sym: u32, utf8: Option<&str>, mods: KeyMods) -> KeyEv {
        KeyEv {
            code: 0,
            sym,
            name: String::new(),
            utf8: utf8.map(str::to_string),
            mods,
            pressed: true,
        }
    }

    #[test]
    fn keys_for_the_pty() {
        let none = KeyMods::default();
        let ctrl = KeyMods {
            ctrl: true,
            ..KeyMods::default()
        };
        let shift = KeyMods {
            shift: true,
            ..KeyMods::default()
        };
        assert_eq!(
            pty_key(&ev(0xff0d, Some("\r"), none)),
            Some((Key::Enter, Mods::NONE))
        );
        assert_eq!(
            pty_key(&ev(0x61, Some("a"), none)),
            Some((Key::Char('a'), Mods::NONE))
        );
        assert_eq!(
            pty_key(&ev(0x41, Some("A"), shift)),
            Some((Key::Char('A'), Mods::NONE))
        );
        assert_eq!(
            pty_key(&ev(0x63, Some("\u{3}"), ctrl)),
            Some((Key::Char('c'), Mods::CTRL))
        );
        assert_eq!(
            pty_key(&ev(0xff52, None, shift)),
            Some((Key::Up, Mods::SHIFT))
        );
        assert_eq!(
            pty_key(&ev(0xffbe, None, none)),
            Some((Key::F(1), Mods::NONE))
        );
        assert_eq!(
            pty_key(&ev(0x00e7, Some("ç"), none)),
            Some((Key::Char('ç'), Mods::NONE))
        );
        assert_eq!(pty_key(&ev(0xffe1, None, none)), None, "a bare Shift");
        let b = nk_client::encode_key(Key::Char('c'), Mods::CTRL);
        assert_eq!(b, [3]);
    }
}
