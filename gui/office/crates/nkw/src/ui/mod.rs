//! A tiny retained-less 2D toolkit (contract S21): a [`Node`] tree is measured
//! and painted onto an [`nk_cats::raster::Canvas`]. Pure: no Smithay, no GL,
//! no time. Equal trees give equal pixels (the overlay caches rasters by
//! `Spec::Ui { node, w, h }`), so every field is `Hash + Eq` (no f32).

pub mod icon;
pub mod layout;
pub mod paint;
pub mod phone;
pub mod theme;

pub use nk_cats::raster::Rgb;
pub use theme::Theme;

use crate::overlay::draw::Fonts;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Node {
    Panel {
        radius: i32,
        bg: Rgb,
        alpha: u8,
        pad: i32,
        child: Box<Node>,
    },
    Column {
        gap: i32,
        items: Vec<Node>,
    },
    /// `Spacer(0)` takes the remaining space
    Row {
        gap: i32,
        items: Vec<Node>,
    },
    Text {
        s: String,
        px: u32,
        rgb: Rgb,
        bold: bool,
    },
    Icon {
        kind: Icon,
        px: i32,
        rgb: Rgb,
    },
    /// battery, usage
    Gauge {
        permille: u16,
        w: i32,
        h: i32,
        rgb: Rgb,
    },
    /// network
    Bars {
        n: u8,
        of: u8,
        w: i32,
        h: i32,
        rgb: Rgb,
    },
    Chip {
        text: String,
        bg: Rgb,
        fg: Rgb,
    },
    Spacer(i32),
    Rule {
        rgb: Rgb,
    },
    Fixed {
        w: i32,
        h: i32,
        child: Box<Node>,
    },
    /// the one selection look (the same everywhere): SELECT background, 2 px
    /// RING ring when `ring`
    Selected {
        ring: bool,
        child: Box<Node>,
    },
    /// `level` 0..=FADE_STEPS
    Faded {
        level: u8,
        child: Box<Node>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Icon {
    Cat,
    Envelope,
    Map,
    Grid,
    Window,
    Note,
    Book,
    Lock,
    LockOpen,
    Gear,
    Question,
    Battery,
    Plug,
    Wifi,
    Clock,
    Play,
    Pause,
    Check,
    Dot,
    Arrow,
    Search,
    Home,
    Phone,
    Terminal,
    X11,
    Warning,
    Info,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Size {
    pub w: i32,
    pub h: i32,
}

/// The size `n` wants inside `avail` (logical px; never more than `avail`).
pub fn measure(n: &Node, avail: Size, fonts: &mut Fonts, t: &Theme) -> Size {
    layout::measure(n, avail, fonts, t)
}

/// The unselected twin of [`Node::Selected`]: the same padding, no
/// background (so a list's rows keep their size when the selection moves).
pub fn plain(child: Node) -> Node {
    pad_xy(child, layout::SEL_PAD)
}

/// An empty box of `w`×`h` logical px.
pub fn gap(w: i32, h: i32) -> Node {
    Node::Fixed {
        w: w.max(1),
        h: h.max(1),
        child: Box::new(Node::Spacer(0)),
    }
}

/// `child` with its own padding (x, y), nothing drawn.
pub fn pad_xy(child: Node, (px, py): (i32, i32)) -> Node {
    Node::Column {
        gap: 0,
        items: vec![
            gap(1, py),
            Node::Row {
                gap: 0,
                items: vec![gap(px, 1), child, gap(px, 1)],
            },
            gap(1, py),
        ],
    }
}

/// `child` selected (the one look: SELECT background + RING ring) or
/// [`plain`].
pub fn selectable(sel: bool, child: Node) -> Node {
    if sel {
        Node::Selected {
            ring: true,
            child: Box::new(child),
        }
    } else {
        plain(child)
    }
}

/// A text node.
pub fn text(s: impl Into<String>, px: u32, rgb: Rgb, bold: bool) -> Node {
    Node::Text {
        s: s.into(),
        px,
        rgb,
        bold,
    }
}

/// Paints `n` at `at` inside `avail` (never outside it).
pub fn paint(
    n: &Node,
    c: &mut nk_cats::raster::Canvas,
    at: (i32, i32),
    avail: Size,
    fonts: &mut Fonts,
    t: &Theme,
) {
    paint::paint(n, c, at, avail, fonts, t)
}

#[cfg(test)]
mod tests;
