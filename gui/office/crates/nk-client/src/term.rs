//! Terminal emulation for a pane: alacritty_terminal fed with the raw PTY
//! bytes, read back as a plain [`Grid`] snapshot for drawing.

use std::sync::{Arc, Mutex};

use alacritty_terminal::event::{Event as TermEvent, EventListener};
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line, Point};
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::{Config, TermMode};
use alacritty_terminal::vte::ansi::{Color, NamedColor, Processor, Rgb};

/// One cell of the grid. Colors are resolved to RGB but NOT swapped for
/// `inverse`: the renderer swaps fg/bg when `inverse` is set. A wide
/// character sits in its first cell (`wide: true`); the cell after it is a
/// spacer with `ch: ' '` and the same colors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cell {
    pub ch: char,
    pub fg: [u8; 3],
    pub bg: [u8; 3],
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub inverse: bool,
    pub wide: bool,
}

/// A snapshot of the visible screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Grid {
    pub cols: u16,
    pub rows: u16,
    /// Row-major: `cells[row * cols + col]`.
    pub cells: Vec<Cell>,
    /// `(col, row)` of the cursor; `None` while the program hides it.
    pub cursor: Option<(u16, u16)>,
}

impl Grid {
    pub fn cell(&self, col: u16, row: u16) -> Option<&Cell> {
        if col >= self.cols || row >= self.rows {
            return None;
        }
        self.cells.get(row as usize * self.cols as usize + col as usize)
    }

    /// A row as text (spacers after wide characters skipped, trailing
    /// blanks trimmed).
    pub fn row_text(&self, row: u16) -> String {
        let mut s = String::new();
        let mut skip = false;
        for c in 0..self.cols {
            let Some(cell) = self.cell(c, row) else { break };
            if skip {
                skip = false;
                continue;
            }
            s.push(cell.ch);
            skip = cell.wide;
        }
        s.trim_end().to_string()
    }

    /// The whole screen as text, one line per row.
    pub fn text(&self) -> String {
        (0..self.rows).map(|r| self.row_text(r)).collect::<Vec<_>>().join("\n")
    }
}

#[derive(Default)]
struct Shared {
    title: Option<String>,
    replies: Vec<u8>,
    bell: bool,
}

#[derive(Clone, Default)]
struct Listener(Arc<Mutex<Shared>>);

impl EventListener for Listener {
    fn send_event(&self, e: TermEvent) {
        let mut s = self.0.lock().unwrap_or_else(|p| p.into_inner());
        match e {
            TermEvent::Title(t) => s.title = Some(t),
            TermEvent::ResetTitle => s.title = None,
            TermEvent::PtyWrite(t) => s.replies.extend_from_slice(t.as_bytes()),
            TermEvent::Bell => s.bell = true,
            _ => {}
        }
    }
}

struct Size {
    cols: usize,
    rows: usize,
}

impl Dimensions for Size {
    fn total_lines(&self) -> usize {
        self.rows
    }
    fn screen_lines(&self) -> usize {
        self.rows
    }
    fn columns(&self) -> usize {
        self.cols
    }
}

/// A terminal emulator (xterm-like, via alacritty_terminal). `Send + Sync`.
pub struct Term {
    term: alacritty_terminal::Term<Listener>,
    parser: Processor,
    shared: Listener,
    changed: bool,
    cols: u16,
    rows: u16,
    palette: [[u8; 3]; 256],
    fg: [u8; 3],
    bg: [u8; 3],
}

const DEFAULT_FG: [u8; 3] = [0xcd, 0xd6, 0xf4];
const DEFAULT_BG: [u8; 3] = [0x1e, 0x1e, 0x2e];

impl Term {
    pub fn new(cols: u16, rows: u16) -> Term {
        let (cols, rows) = (cols.max(1), rows.max(1));
        let shared = Listener::default();
        let config = Config { scrolling_history: 2000, ..Config::default() };
        let term =
            alacritty_terminal::Term::new(config, &Size { cols: cols as usize, rows: rows as usize }, shared.clone());
        Term {
            term,
            parser: Processor::new(),
            shared,
            changed: true,
            cols,
            rows,
            palette: xterm_palette(),
            fg: DEFAULT_FG,
            bg: DEFAULT_BG,
        }
    }

    /// Feeds raw PTY output.
    pub fn feed(&mut self, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        self.parser.advance(&mut self.term, bytes);
        self.changed = true;
    }

    pub fn resize(&mut self, cols: u16, rows: u16) {
        let (cols, rows) = (cols.max(1), rows.max(1));
        if (cols, rows) == (self.cols, self.rows) {
            return;
        }
        self.cols = cols;
        self.rows = rows;
        self.term.resize(Size { cols: cols as usize, rows: rows as usize });
        self.changed = true;
    }

    pub fn size(&self) -> (u16, u16) {
        (self.cols, self.rows)
    }

    /// The colors of default text and background (e.g. the theme's text/bg).
    pub fn set_default_colors(&mut self, fg: [u8; 3], bg: [u8; 3]) {
        self.fg = fg;
        self.bg = bg;
        self.changed = true;
    }

    /// Replaces the 16 ANSI colors (indexes 0..16).
    pub fn set_ansi_colors(&mut self, colors: [[u8; 3]; 16]) {
        self.palette[..16].copy_from_slice(&colors);
        self.changed = true;
    }

    /// The last title the program set (OSC 0/2).
    pub fn title(&self) -> Option<String> {
        self.shared.0.lock().unwrap_or_else(|p| p.into_inner()).title.clone()
    }

    /// True once after new content (for redraw-on-demand).
    pub fn changed(&mut self) -> bool {
        std::mem::take(&mut self.changed)
    }

    /// True once after the program rang the bell.
    pub fn take_bell(&mut self) -> bool {
        std::mem::take(&mut self.shared.0.lock().unwrap_or_else(|p| p.into_inner()).bell)
    }

    /// Answers the terminal owes the program (cursor position reports,
    /// device attributes…): write them to the pane.
    pub fn take_replies(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.shared.0.lock().unwrap_or_else(|p| p.into_inner()).replies)
    }

    /// Whether the program asked for application cursor keys (DECCKM): pass
    /// it to `encode_key_mode`.
    pub fn app_cursor(&self) -> bool {
        self.term.mode().contains(TermMode::APP_CURSOR)
    }

    /// Whether the program enabled bracketed paste (wrap pastes in
    /// `ESC[200~ … ESC[201~`).
    pub fn bracketed_paste(&self) -> bool {
        self.term.mode().contains(TermMode::BRACKETED_PASTE)
    }

    /// A snapshot of the visible screen.
    pub fn grid(&self) -> Grid {
        let g = self.term.grid();
        let (cols, rows) = (self.cols as usize, self.rows as usize);
        let mut cells = Vec::with_capacity(cols * rows);
        for r in 0..rows {
            let row = &g[Line(r as i32)];
            for c in 0..cols {
                let cell = &row[Column(c)];
                let f = cell.flags;
                let mut fg = self.rgb(cell.fg, f.contains(Flags::BOLD), true);
                let bg = self.rgb(cell.bg, false, false);
                if f.contains(Flags::DIM) {
                    fg = dim(fg);
                }
                if f.contains(Flags::HIDDEN) {
                    fg = bg;
                }
                let spacer = f.intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER);
                let ch = if spacer || cell.c == '\0' || cell.c == '\t' { ' ' } else { cell.c };
                cells.push(Cell {
                    ch,
                    fg,
                    bg,
                    bold: f.contains(Flags::BOLD),
                    italic: f.contains(Flags::ITALIC),
                    underline: f.intersects(Flags::ALL_UNDERLINES),
                    inverse: f.contains(Flags::INVERSE),
                    wide: f.contains(Flags::WIDE_CHAR),
                });
            }
        }
        let cursor = if self.term.mode().contains(TermMode::SHOW_CURSOR) {
            let mut p: Point = g.cursor.point;
            if g[p].flags.contains(Flags::WIDE_CHAR_SPACER) && p.column.0 > 0 {
                p.column -= 1;
            }
            let (c, r) = (p.column.0.min(cols - 1), p.line.0.clamp(0, rows as i32 - 1));
            Some((c as u16, r as u16))
        } else {
            None
        };
        Grid { cols: self.cols, rows: self.rows, cells, cursor }
    }

    fn rgb(&self, c: Color, bold: bool, is_fg: bool) -> [u8; 3] {
        let over = |i: usize| self.term.colors()[i].map(|Rgb { r, g, b }| [r, g, b]);
        match c {
            Color::Spec(Rgb { r, g, b }) => [r, g, b],
            Color::Indexed(i) => {
                let i = if bold && is_fg && i < 8 { i + 8 } else { i };
                over(i as usize).unwrap_or(self.palette[i as usize])
            }
            Color::Named(n) => {
                let n = if bold && is_fg && (n as usize) < 8 { n.to_bright() } else { n };
                let i = n as usize;
                if let Some(c) = over(i) {
                    return c;
                }
                match n {
                    _ if i < 16 => self.palette[i],
                    NamedColor::Foreground | NamedColor::BrightForeground | NamedColor::Cursor => self.fg,
                    NamedColor::Background => self.bg,
                    NamedColor::DimForeground => dim(self.fg),
                    _ => {
                        // Dim* (259..267): the dim version of 0..8
                        let base = i.saturating_sub(NamedColor::DimBlack as usize).min(7);
                        dim(self.palette[base])
                    }
                }
            }
        }
    }
}

fn dim(c: [u8; 3]) -> [u8; 3] {
    c.map(|v| (v as u16 * 2 / 3) as u8)
}

/// xterm's 256-color palette.
fn xterm_palette() -> [[u8; 3]; 256] {
    let mut p = [[0u8; 3]; 256];
    let ansi: [[u8; 3]; 16] = [
        [0x00, 0x00, 0x00],
        [0xcd, 0x00, 0x00],
        [0x00, 0xcd, 0x00],
        [0xcd, 0xcd, 0x00],
        [0x00, 0x00, 0xee],
        [0xcd, 0x00, 0xcd],
        [0x00, 0xcd, 0xcd],
        [0xe5, 0xe5, 0xe5],
        [0x7f, 0x7f, 0x7f],
        [0xff, 0x00, 0x00],
        [0x00, 0xff, 0x00],
        [0xff, 0xff, 0x00],
        [0x5c, 0x5c, 0xff],
        [0xff, 0x00, 0xff],
        [0x00, 0xff, 0xff],
        [0xff, 0xff, 0xff],
    ];
    p[..16].copy_from_slice(&ansi);
    let level = |v: usize| if v == 0 { 0 } else { (55 + v * 40) as u8 };
    for i in 0..216 {
        p[16 + i] = [level(i / 36), level((i / 6) % 6), level(i % 6)];
    }
    for i in 0..24 {
        let v = (8 + i * 10) as u8;
        p[232 + i] = [v, v, v];
    }
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    fn is_send_sync<T: Send + Sync>() {}

    #[test]
    fn term_is_send_sync() {
        is_send_sync::<Term>();
    }

    #[test]
    fn plain_text_and_cursor() {
        let mut t = Term::new(10, 3);
        assert!(t.changed());
        assert!(!t.changed());
        t.feed(b"ola\r\nmundo");
        assert!(t.changed());
        let g = t.grid();
        assert_eq!((g.cols, g.rows, g.cells.len()), (10, 3, 30));
        assert_eq!(g.row_text(0), "ola");
        assert_eq!(g.row_text(1), "mundo");
        assert_eq!(g.cursor, Some((5, 1)));
        assert_eq!(g.cell(0, 0).unwrap().fg, DEFAULT_FG);
        assert_eq!(g.cell(0, 0).unwrap().bg, DEFAULT_BG);
        assert_eq!(g.cell(10, 0), None);
    }

    #[test]
    fn colors_and_attributes() {
        let mut t = Term::new(20, 2);
        t.set_default_colors([1, 2, 3], [4, 5, 6]);
        t.feed(b"\x1b[31mR\x1b[1;32mG\x1b[0m\x1b[38;5;196mI\x1b[48;2;10;20;30mT\x1b[0m\x1b[3;4;7mX\x1b[0m\x1b[38;5;244mg\x1b[2mD");
        let g = t.grid();
        let c = |i| *g.cell(i, 0).unwrap();
        assert_eq!(c(0).fg, [0xcd, 0, 0]);
        assert!(!c(0).bold);
        assert_eq!(c(1).fg, [0, 0xff, 0], "bold green is bright green");
        assert!(c(1).bold);
        assert_eq!(c(2).fg, [0xff, 0, 0], "256-color 196");
        assert_eq!(c(3).bg, [10, 20, 30]);
        assert_eq!(c(3).fg, [0xff, 0, 0], "fg carries over");
        assert_eq!(c(7).fg, [1, 2, 3], "default fg");
        let x = c(4);
        assert!(x.italic && x.underline && x.inverse);
        assert_eq!((x.fg, x.bg), ([1, 2, 3], [4, 5, 6]), "inverse is left to the renderer");
        assert_eq!(c(5).fg, [128, 128, 128], "grayscale ramp");
        assert_eq!(c(6).fg, dim([128, 128, 128]));
        assert_eq!(c(7).bg, [4, 5, 6]);
    }

    #[test]
    fn osc_palette_override() {
        let mut t = Term::new(5, 1);
        t.feed(b"\x1b]4;1;rgb:12/34/56\x07\x1b[31mx");
        assert_eq!(t.grid().cell(0, 0).unwrap().fg, [0x12, 0x34, 0x56]);
    }

    #[test]
    fn wide_chars() {
        let mut t = Term::new(10, 1);
        t.feed("日本a".as_bytes());
        let g = t.grid();
        assert!(g.cell(0, 0).unwrap().wide);
        assert_eq!(g.cell(0, 0).unwrap().ch, '日');
        assert_eq!(g.cell(1, 0).unwrap().ch, ' ');
        assert!(!g.cell(1, 0).unwrap().wide);
        assert_eq!(g.cell(2, 0).unwrap().ch, '本');
        assert_eq!(g.cell(4, 0).unwrap().ch, 'a');
        assert_eq!(g.row_text(0), "日本a");
        assert_eq!(g.cursor, Some((5, 0)));
    }

    #[test]
    fn cursor_moves_and_hides() {
        let mut t = Term::new(10, 5);
        t.feed(b"\x1b[3;4H");
        assert_eq!(t.grid().cursor, Some((3, 2)));
        t.feed(b"\x1b[?25l");
        assert_eq!(t.grid().cursor, None);
        t.feed(b"\x1b[?25h\x1b[2J\x1b[H");
        assert_eq!(t.grid().cursor, Some((0, 0)));
    }

    #[test]
    fn resize_keeps_content() {
        let mut t = Term::new(10, 3);
        t.feed(b"abc");
        t.changed();
        t.resize(10, 3);
        assert!(!t.changed(), "same size is no change");
        t.resize(20, 6);
        assert!(t.changed());
        let g = t.grid();
        assert_eq!((g.cols, g.rows, g.cells.len()), (20, 6, 120));
        assert_eq!(g.row_text(0), "abc");
        t.resize(2, 1);
        assert_eq!(t.grid().cells.len(), 2);
        t.resize(0, 0);
        assert_eq!(t.size(), (1, 1));
    }

    #[test]
    fn title_bell_and_replies() {
        let mut t = Term::new(10, 3);
        assert_eq!(t.title(), None);
        t.feed(b"\x1b]2;claude \xc2\xb7 api\x07");
        assert_eq!(t.title().as_deref(), Some("claude · api"));
        t.feed(b"\x07");
        assert!(t.take_bell());
        assert!(!t.take_bell());
        t.feed(b"ab\x1b[6n");
        assert_eq!(t.take_replies(), b"\x1b[1;3R");
        assert!(t.take_replies().is_empty());
    }

    #[test]
    fn modes() {
        let mut t = Term::new(10, 3);
        assert!(!t.app_cursor());
        t.feed(b"\x1b[?1h\x1b[?2004h");
        assert!(t.app_cursor());
        assert!(t.bracketed_paste());
    }

    #[test]
    fn scrolling_and_alt_screen() {
        let mut t = Term::new(5, 2);
        t.feed(b"1\r\n2\r\n3");
        let g = t.grid();
        assert_eq!(g.text(), "2\n3");
        t.feed(b"\x1b[?1049h\x1b[Halt");
        assert_eq!(t.grid().row_text(0), "alt");
        t.feed(b"\x1b[?1049l");
        assert_eq!(t.grid().text(), "2\n3");
    }

    #[test]
    fn split_utf8_and_escape_across_feeds() {
        let mut t = Term::new(10, 1);
        let bytes = "\x1b[32mç".as_bytes();
        for b in bytes {
            t.feed(std::slice::from_ref(b));
        }
        let c = *t.grid().cell(0, 0).unwrap();
        assert_eq!((c.ch, c.fg), ('ç', [0, 0xcd, 0]));
    }
}
