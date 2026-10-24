use nk_cats::raster::Canvas;

use super::layout::{SEL_PAD, SHADOW};
use super::*;
use crate::overlay::draw::{FADE_STEPS, Fonts, Px};

const BIG: Size = Size { w: 1000, h: 1000 };

fn fixed(w: i32, h: i32) -> Node {
    gap(w, h)
}

fn m(n: &Node) -> Size {
    measure(n, BIG, &mut Fonts::new(), &Theme::cozy())
}

fn sz(w: i32, h: i32) -> Size {
    Size { w, h }
}

#[test]
fn measure_table() {
    let t = |s: &str| text(s, 16, [0, 0, 0], false);
    let cw = Fonts::new().face(16).cw as i32;
    let ch = Fonts::new().face(16).ch as i32;
    let rows: Vec<(Node, Size)> = vec![
        (fixed(30, 20), sz(30, 20)),
        (Node::Spacer(7), sz(7, 7)),
        (Node::Spacer(0), sz(0, 0)),
        (t("abcd"), sz(4 * cw, ch)),
        (
            Node::Column {
                gap: 5,
                items: vec![fixed(30, 20), fixed(50, 10)],
            },
            sz(50, 35),
        ),
        (
            Node::Row {
                gap: 5,
                items: vec![fixed(30, 20), fixed(50, 10)],
            },
            sz(85, 20),
        ),
        // Spacer(0) takes the remaining width (and the Row the whole box)
        (
            Node::Row {
                gap: 0,
                items: vec![fixed(30, 20), Node::Spacer(0), fixed(50, 10)],
            },
            sz(1000, 20),
        ),
        // ... and in a Column the remaining height
        (
            Node::Column {
                gap: 0,
                items: vec![fixed(30, 20), Node::Spacer(0)],
            },
            sz(30, 1000),
        ),
        (
            Node::Fixed {
                w: 40,
                h: 0,
                child: Box::new(fixed(10, 33)),
            },
            sz(40, 33),
        ),
        (
            Node::Panel {
                radius: 6,
                bg: [1, 1, 1],
                alpha: 255,
                pad: 4,
                child: Box::new(fixed(10, 10)),
            },
            sz(18, 18),
        ),
        (
            Node::Panel {
                radius: 24,
                bg: [1, 1, 1],
                alpha: 255,
                pad: 4,
                child: Box::new(fixed(10, 10)),
            },
            sz(18, 18 + SHADOW),
        ),
        (
            Node::Selected {
                ring: true,
                child: Box::new(fixed(10, 10)),
            },
            sz(10 + 2 * SEL_PAD.0, 10 + 2 * SEL_PAD.1),
        ),
        (
            plain(fixed(10, 10)),
            sz(10 + 2 * SEL_PAD.0, 10 + 2 * SEL_PAD.1),
        ),
        (
            Node::Faded {
                level: 2,
                child: Box::new(fixed(12, 9)),
            },
            sz(12, 9),
        ),
        (
            Node::Gauge {
                permille: 500,
                w: 24,
                h: 12,
                rgb: [0; 3],
            },
            sz(24, 12),
        ),
        (Node::Rule { rgb: [0; 3] }, sz(0, 1)),
    ];
    for (n, want) in rows {
        assert_eq!(m(&n), want, "{n:?}");
    }
}

#[test]
fn measure_never_exceeds_avail_and_text_is_cut() {
    let long = text("x".repeat(400), 16, [0; 3], false);
    let a = sz(100, 50);
    let got = measure(&long, a, &mut Fonts::new(), &Theme::cozy());
    assert!(got.w <= 100 && got.h <= 50, "{got:?}");
    // a stretching row beside a fixed item keeps the fixed one whole
    let r = Node::Row {
        gap: 0,
        items: vec![
            plain(Node::Row {
                gap: 0,
                items: vec![fixed(10, 10), Node::Spacer(0)],
            }),
            fixed(40, 10),
        ],
    };
    let mut f = Fonts::new();
    let mut cx = layout::Cx {
        fonts: &mut f,
        t: &Theme::cozy(),
        s: Px(1.0),
    };
    let sizes = layout::row_sizes(
        match &r {
            Node::Row { items, .. } => items,
            _ => unreachable!(),
        },
        0,
        sz(200, 40),
        &mut cx,
    );
    assert_eq!(sizes[1], Some(sz(40, 10)));
    assert_eq!(sizes[0].unwrap().w, 160);
}

#[test]
fn scale_two_doubles_sizes() {
    let n = Node::Column {
        gap: 4,
        items: vec![fixed(30, 20), fixed(50, 10)],
    };
    let one = layout::measure_at(&n, BIG, &mut Fonts::new(), &Theme::cozy(), Px(1.0));
    let two = layout::measure_at(
        &n,
        sz(2000, 2000),
        &mut Fonts::new(),
        &Theme::cozy(),
        Px(2.0),
    );
    assert_eq!((two.w, two.h), (2 * one.w, 2 * one.h));
}

fn alpha(c: &Canvas, x: usize, y: usize) -> u8 {
    c.px[(y * c.w + x) * 4 + 3]
}

/// Paint stays inside the box it is handed, whatever the tree.
#[test]
fn paint_stays_inside_avail() {
    let tree = Node::Panel {
        radius: 24,
        bg: [200, 180, 150],
        alpha: 255,
        pad: 12,
        child: Box::new(Node::Column {
            gap: 6,
            items: vec![
                text("um texto bem comprido que não cabe", 16, [0; 3], true),
                Node::Row {
                    gap: 4,
                    items: vec![
                        Node::Icon {
                            kind: Icon::Cat,
                            px: 24,
                            rgb: [0; 3],
                        },
                        Node::Spacer(0),
                        Node::Chip {
                            text: "3".into(),
                            bg: [255, 200, 100],
                            fg: [0; 3],
                        },
                    ],
                },
                Node::Rule { rgb: [0; 3] },
                Node::Selected {
                    ring: true,
                    child: Box::new(fixed(300, 30)),
                },
            ],
        }),
    };
    let mut c = Canvas::transparent(200, 160);
    let avail = sz(120, 80);
    paint(
        &tree,
        &mut c,
        (40, 30),
        avail,
        &mut Fonts::new(),
        &Theme::cozy(),
    );
    let mut inside = 0;
    for y in 0..160 {
        for x in 0..200 {
            let a = alpha(&c, x, y);
            let within = (40..160).contains(&x) && (30..110).contains(&y);
            if !within {
                assert_eq!(a, 0, "painted outside at {x},{y}");
            } else if a > 0 {
                inside += 1;
            }
        }
    }
    assert!(inside > 1000, "{inside}");
}

#[test]
fn selected_draws_the_one_look() {
    let t = Theme::cozy();
    let n = Node::Selected {
        ring: true,
        child: Box::new(fixed(40, 20)),
    };
    let c = paint::render(&n, BIG, &mut Fonts::new(), &t, Px(1.0));
    let at = |x: usize, y: usize| {
        let i = (y * c.w + x) * 4;
        [c.px[i], c.px[i + 1], c.px[i + 2]]
    };
    let (w, h) = (c.w, c.h);
    assert_eq!(at(w / 2, h / 2), t.select, "the cream background");
    assert_eq!(at(w / 2, 0), t.ring, "the ring on the edge");
    assert_eq!(at(w / 2, 1), t.ring, "2 px wide");
    // no ring: the background reaches the edge
    let n2 = Node::Selected {
        ring: false,
        child: Box::new(fixed(40, 20)),
    };
    let c2 = paint::render(&n2, BIG, &mut Fonts::new(), &t, Px(1.0));
    let i = (c2.w / 2) * 4;
    assert_eq!(&c2.px[i..i + 3], &t.select);
}

#[test]
fn faded_scales_opacity() {
    let n = |level| Node::Faded {
        level,
        child: Box::new(Node::Panel {
            radius: 0,
            bg: [100, 100, 100],
            alpha: 255,
            pad: 5,
            child: Box::new(fixed(10, 10)),
        }),
    };
    let full = paint::render(
        &n(FADE_STEPS),
        BIG,
        &mut Fonts::new(),
        &Theme::cozy(),
        Px(1.0),
    );
    let half = paint::render(&n(2), BIG, &mut Fonts::new(), &Theme::cozy(), Px(1.0));
    let none = paint::render(&n(0), BIG, &mut Fonts::new(), &Theme::cozy(), Px(1.0));
    assert_eq!(alpha(&full, 10, 10), 255);
    assert!((alpha(&half, 10, 10) as i32 - 128).abs() <= 1);
    assert!(none.px.iter().all(|v| *v == 0));
}

#[test]
fn gauge_and_bars_fill_by_value() {
    let g = |p| Node::Gauge {
        permille: p,
        w: 40,
        h: 16,
        rgb: [0, 0, 0],
    };
    let ink = |c: &Canvas| c.px.chunks(4).filter(|p| p[3] > 128).count();
    let r = |n: &Node| paint::render(n, BIG, &mut Fonts::new(), &Theme::cozy(), Px(1.0));
    let (empty, half, full) = (ink(&r(&g(0))), ink(&r(&g(500))), ink(&r(&g(1000))));
    assert!(empty < half && half < full, "{empty} {half} {full}");
    let b = |n| Node::Bars {
        n,
        of: 4,
        w: 20,
        h: 16,
        rgb: [0, 0, 0],
    };
    let solid = |c: &Canvas| c.px.chunks(4).filter(|p| p[3] == 255).count();
    assert!(solid(&r(&b(1))) < solid(&r(&b(3))));
    assert_eq!(solid(&r(&b(0))), 0);
}

#[test]
fn every_icon_is_small_non_empty_inside_and_symmetric_where_meant() {
    for kind in icon::ALL {
        assert!(icon::prims(kind).len() <= icon::MAX_PRIMS, "{kind:?}");
        for px in [16, 24, 32] {
            let mut c = Canvas::transparent(px as usize + 8, px as usize + 8);
            icon::draw(kind, &mut c, 4, 4, px, [10, 20, 30]);
            let mut ink = 0;
            for y in 0..c.h {
                for x in 0..c.w {
                    let a = alpha(&c, x, y);
                    let inside =
                        (4..4 + px as usize).contains(&x) && (4..4 + px as usize).contains(&y);
                    if a > 0 {
                        assert!(inside, "{kind:?} {px}: ink outside at {x},{y}");
                        ink += 1;
                    }
                }
            }
            let area = (px * px) as usize;
            assert!(ink > area / 20, "{kind:?} {px}: almost empty ({ink})");
            assert!(ink < area, "{kind:?} {px}: a full square");
        }
    }
    // left/right mirror images
    for kind in [
        Icon::Cat,
        Icon::Envelope,
        Icon::Grid,
        Icon::Home,
        Icon::Warning,
        Icon::Pause,
        Icon::X11,
        Icon::Dot,
        Icon::Info,
        Icon::Gear,
        Icon::Wifi,
        Icon::Lock,
    ] {
        let mut c = Canvas::transparent(32, 32);
        icon::draw(kind, &mut c, 0, 0, 32, [0; 3]);
        let mut diff = 0;
        for y in 0..32 {
            for x in 0..32 {
                diff += (alpha(&c, x, y) as i32 - alpha(&c, 31 - x, y) as i32).unsigned_abs();
            }
        }
        assert!(diff < 32 * 255, "{kind:?} is lopsided ({diff})");
    }
}

#[test]
fn theme_px_scales_and_clamps() {
    let mut t = Theme::cozy();
    assert_eq!(t.px(16), 16);
    t.font_scale_pct = 130;
    assert_eq!(t.px(16), 21);
    t.font_scale_pct = 400;
    assert_eq!(t.px(16), 26, "160 % at most");
    t.font_scale_pct = 10;
    assert_eq!(t.px(16), 13, "80 % at least");
    assert_eq!(t.px(8), 10, "never below 10 px");
}

/// WCAG 2 relative luminance of an sRGB colour.
fn lum(c: Rgb) -> f64 {
    let ch = |v: u8| {
        let s = v as f64 / 255.0;
        if s <= 0.039_28 {
            s / 12.92
        } else {
            ((s + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * ch(c[0]) + 0.7152 * ch(c[1]) + 0.0722 * ch(c[2])
}

fn ratio(a: Rgb, b: Rgb) -> f64 {
    let (x, y) = (lum(a), lum(b));
    (x.max(y) + 0.05) / (x.min(y) + 0.05)
}

/// Every text/background pair the toolkit draws: ≥ 4.5:1 cozy, ≥ 7:1 high
/// contrast.
#[test]
fn theme_text_pairs_meet_wcag() {
    for (t, min) in [(Theme::cozy(), 4.5), (Theme::high_contrast(), 7.0)] {
        let pairs = theme::text_pairs(&t);
        assert!(pairs.len() >= 6);
        for (name, fg, bg) in pairs {
            let r = ratio(fg, bg);
            assert!(r >= min, "{name}: {r:.2} < {min} (hc {})", t.high_contrast);
        }
    }
}
