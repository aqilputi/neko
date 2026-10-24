//! The cozy look of the HUD pieces as toolkit trees (S21): the mode label,
//! the status strip, the badge chips, the toasts and the name tags. Pure:
//! data in, a [`Node`] out; the overlay rasterises them through
//! `Spec::Ui` (cached by the tree).

use super::draw::{PX_SMALL, PX_UI, Rgb, ink_on, wrap};
use super::prefs::Strip;
use crate::ext::{Badge, Status, ToastKind};
use crate::ui::{Icon, Node, Theme, gap, text};

/// The pills' corner radius and padding (logical px).
pub const PILL_R: i32 = 9;
pub const PILL_PAD: (i32, i32) = (10, 4);
/// Icons inside a pill.
pub const PILL_ICON: i32 = 15;
/// A toast is at most this wide (logical px), wrapped.
pub const TOAST_MAX_W: i32 = 480;

const AMBER: Rgb = [0xff, 0xd8, 0x6a];
const SKY: Rgb = [0xcf, 0xe6, 0xf5];
const LEAF: Rgb = [0xd4, 0xec, 0xc4];
const RED: Rgb = [0xe0, 0x4a, 0x3a];
const WHITE: Rgb = [0xff, 0xfb, 0xf2];

/// A cream pill around `items` (a Row), the label's and the strip's shape.
pub fn pill(items: Vec<Node>, bg: Rgb, t: &Theme) -> Node {
    let _ = t;
    Node::Panel {
        radius: PILL_R,
        bg,
        alpha: 255,
        pad: PILL_PAD.1,
        child: Box::new(Node::Row {
            gap: 0,
            items: vec![
                gap(PILL_PAD.0 - PILL_PAD.1, 1),
                Node::Row { gap: 8, items },
                gap(PILL_PAD.0 - PILL_PAD.1, 1),
            ],
        }),
    }
}

/// The mode label (§3.1): cream pill, brown ink.
pub fn label(mode: &str, t: &Theme) -> Node {
    pill(vec![text(mode, t.px(PX_UI), t.ink, false)], t.cream, t)
}

fn icon(kind: Icon, rgb: Rgb) -> Node {
    Node::Icon {
        kind,
        px: PILL_ICON,
        rgb,
    }
}

/// `1234` cents → "12,34" (pt-BR).
pub fn money(cents: u32) -> String {
    format!("{},{:02}", cents / 100, cents % 100)
}

/// What the strip shows for `st` under `mode` (pure, the strip's table):
/// `None` when nothing (or `never`).
pub fn strip_items(st: &Status, mode: Strip, t: &Theme) -> Option<Vec<Node>> {
    let all = match mode {
        Strip::Never => return None,
        Strip::Always => true,
        Strip::Auto => false,
    };
    let ui = t.px(PX_UI);
    let mut v: Vec<Node> = Vec::new();
    if let Some((h, m)) = st.clock {
        v.push(text(format!("{h:02}:{m:02}"), ui, t.ink, true));
    }
    if let Some(n) = st.inbox
        && (n > 0 || all)
    {
        v.push(Node::Row {
            gap: 4,
            items: vec![
                icon(Icon::Envelope, t.ink),
                text(n.to_string(), ui, t.ink, false),
            ],
        });
    }
    if all && let Some((a, b)) = st.agents {
        v.push(Node::Row {
            gap: 4,
            items: vec![
                icon(Icon::Cat, t.ink),
                text(
                    if b > 0 {
                        format!("{a} · {b}!")
                    } else {
                        a.to_string()
                    },
                    ui,
                    t.ink,
                    false,
                ),
            ],
        });
    }
    if let Some(c) = st.cost_cents
        && (c > 0 || all)
    {
        v.push(text(format!("$ {}", money(c)), ui, t.ink, false));
    }
    if all && let Some(p) = st.usage_5h_pct {
        v.push(text(format!("5h {p}%"), ui, t.soft, false));
    }
    if st.playing == Some(true) {
        v.push(icon(Icon::Note, t.ink));
    }
    if all && st.vault_open {
        v.push(icon(Icon::LockOpen, t.error_ink));
    }
    if all && st.xwayland == Some(true) {
        v.push(icon(Icon::X11, t.soft));
    }
    if let Some((connected, bars)) = st.net
        && (all || !connected)
    {
        let rgb = if connected { t.ink } else { t.error_ink };
        v.push(if connected {
            Node::Bars {
                n: bars.min(4),
                of: 4,
                w: 16,
                h: 13,
                rgb,
            }
        } else {
            Node::Row {
                gap: 4,
                items: vec![icon(Icon::Wifi, rgb), text("sem rede", ui, rgb, false)],
            }
        });
    }
    if let Some((pct, charging)) = st.battery
        && (all || (!charging && pct < 30))
    {
        let low = !charging && pct < 30;
        let rgb = if low { t.error_ink } else { t.ink };
        let mut items = vec![Node::Gauge {
            permille: pct.min(100) as u16 * 10,
            w: 24,
            h: 12,
            rgb,
        }];
        if charging {
            items.push(icon(Icon::Plug, rgb));
        }
        items.push(text(format!("{pct}%"), ui, rgb, false));
        v.push(Node::Row { gap: 4, items });
    }
    (!v.is_empty()).then_some(v)
}

/// The strip: right-aligned cream pill, the label's height.
pub fn strip(st: &Status, mode: Strip, t: &Theme) -> Option<Node> {
    strip_items(st, mode, t).map(|items| pill(items, t.cream, t))
}

/// A badge as a chip: an icon where one says it better, the number.
pub fn chip(b: &Badge, t: &Theme) -> Node {
    let small = t.px(PX_SMALL);
    let row = |bg: Rgb, items: Vec<Node>| Node::Panel {
        radius: 11,
        bg,
        alpha: 255,
        pad: 3,
        child: Box::new(Node::Row {
            gap: 5,
            items: [vec![gap(3, 1)], items, vec![gap(3, 1)]].concat(),
        }),
    };
    let txt = |s: String, fg: Rgb| text(s, small, fg, false);
    let ink = t.ink;
    match b {
        Badge::Inbox(n) => row(
            AMBER,
            vec![icon(Icon::Envelope, ink), txt(n.to_string(), ink)],
        ),
        Badge::Confirm => row(
            AMBER,
            vec![icon(Icon::Question, ink), txt("confirmar".into(), ink)],
        ),
        Badge::BatteryLow(p) => row(
            t.error_bg,
            vec![
                Node::Gauge {
                    permille: (*p).min(100) as u16 * 10,
                    w: 22,
                    h: 11,
                    rgb: t.error_ink,
                },
                txt(format!("{p}%"), t.error_ink),
            ],
        ),
        Badge::NetDown => row(
            t.error_bg,
            vec![
                icon(Icon::Wifi, t.error_ink),
                txt("sem rede".into(), t.error_ink),
            ],
        ),
        Badge::VaultOpen => row(
            LEAF,
            vec![icon(Icon::LockOpen, ink), txt("cofre aberto".into(), ink)],
        ),
        Badge::Updates(n) => row(SKY, vec![txt(format!("{n} atualizações"), ink)]),
        Badge::OnAir => row(
            RED,
            vec![icon(Icon::Dot, WHITE), txt("ON AIR".into(), WHITE)],
        ),
        Badge::Layout(s) => row(t.cream, vec![txt(s.clone(), ink)]),
    }
}

/// A toast: icon by kind, wrapped to [`TOAST_MAX_W`], bottom-left.
pub fn toast(msg: &str, kind: ToastKind, t: &Theme, cell_w: i32) -> Node {
    let (bg, fg, ic) = match kind {
        ToastKind::Error => (t.error_bg, t.error_ink, Icon::Warning),
        ToastKind::Info => (t.cream, t.ink, Icon::Info),
    };
    let px = t.px(PX_UI);
    // the text's columns: the max width minus the icon and the padding
    let cols = ((TOAST_MAX_W - 80) / cell_w.max(1)).max(8) as usize;
    let lines: Vec<Node> = wrap(msg, cols)
        .into_iter()
        .take(4)
        .map(|l| text(l, px, fg, false))
        .collect();
    Node::Panel {
        radius: 16,
        bg,
        alpha: 255,
        pad: 10,
        child: Box::new(Node::Row {
            gap: 10,
            items: vec![
                gap(2, 1),
                Node::Icon {
                    kind: ic,
                    px: 20,
                    rgb: fg,
                },
                Node::Column {
                    gap: 2,
                    items: lines,
                },
                gap(2, 1),
            ],
        }),
    }
}

/// A name tag: the context's colour, 12 px from 4 m away, 14 px nearer;
/// `selected` (the library's selected book) wears the one selection look.
pub fn tag(s: &str, rgb: Rgb, metres: f32, selected: bool, t: &Theme) -> Node {
    let px = if metres >= 4.0 { 12 } else { PX_SMALL };
    let px = t.px(px);
    if selected {
        return Node::Selected {
            ring: true,
            child: Box::new(text(s, px, t.ink, true)),
        };
    }
    let fg = if t.high_contrast {
        contrast_ink(rgb, t)
    } else {
        ink_on(rgb)
    };
    Node::Panel {
        radius: 9,
        bg: rgb,
        alpha: 255,
        pad: 2,
        child: Box::new(Node::Row {
            gap: 0,
            items: vec![gap(5, 1), text(s, px, fg, false), gap(5, 1)],
        }),
    }
}

fn contrast_ink(bg: Rgb, t: &Theme) -> Rgb {
    let l = 0.2126 * bg[0] as f32 + 0.7152 * bg[1] as f32 + 0.0722 * bg[2] as f32;
    if l > 140.0 { t.ink } else { [0xff, 0xff, 0xff] }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn st() -> Status {
        Status {
            clock: Some((14, 5)),
            band: Some(4),
            battery: Some((42, false)),
            net: Some((true, 3)),
            playing: Some(false),
            vault_open: false,
            inbox: Some(0),
            agents: Some((3, 1)),
            cost_cents: Some(0),
            usage_5h_pct: Some(37),
            xwayland: None,
        }
    }

    fn n(st: &Status, mode: Strip) -> usize {
        strip_items(st, mode, &Theme::cozy()).map_or(0, |v| v.len())
    }

    /// The strip's table: what each mode shows for which fields.
    #[test]
    fn strip_contents_per_mode() {
        let t = Theme::cozy();
        let base = st();
        assert_eq!(n(&base, Strip::Never), 0);
        assert_eq!(n(&base, Strip::Auto), 1, "nominal: only the clock");
        // always: clock, inbox, agents, cost, usage, net, battery
        assert_eq!(n(&base, Strip::Always), 7);
        let busy = Status {
            inbox: Some(3),
            cost_cents: Some(1234),
            playing: Some(true),
            ..base.clone()
        };
        assert_eq!(n(&busy, Strip::Auto), 4, "clock, inbox, cost, ♪");
        let items = strip_items(&busy, Strip::Auto, &t).unwrap();
        assert!(matches!(&items[2], Node::Text { s, .. } if s == "$ 12,34"));
        let low = Status {
            battery: Some((12, false)),
            ..base.clone()
        };
        assert_eq!(n(&low, Strip::Auto), 2, "discharging and < 30 %: the gauge");
        let charging = Status {
            battery: Some((12, true)),
            ..base.clone()
        };
        assert_eq!(n(&charging, Strip::Auto), 1, "charging: nominal");
        let down = Status {
            net: Some((false, 0)),
            ..base.clone()
        };
        assert_eq!(n(&down, Strip::Auto), 2, "no network shows");
        assert_eq!(n(&Status::default(), Strip::Always), 0);
        assert!(strip(&Status::default(), Strip::Auto, &t).is_none());
        assert_eq!(money(5), "0,05");
        assert_eq!(money(100_050), "1000,50");
    }

    #[test]
    fn tags_by_distance_and_selection() {
        let t = Theme::cozy();
        let px = |n: &Node| match n {
            Node::Panel { child, .. } => match &**child {
                Node::Row { items, .. } => match &items[1] {
                    Node::Text { px, .. } => *px,
                    _ => 0,
                },
                _ => 0,
            },
            _ => 0,
        };
        assert_eq!(px(&tag("acme", [1, 2, 3], 5.0, false, &t)), 12);
        assert_eq!(px(&tag("acme", [1, 2, 3], 2.0, false, &t)), 14);
        assert!(matches!(
            tag("notas.md", [1, 2, 3], 1.0, true, &t),
            Node::Selected { .. }
        ));
    }

    #[test]
    fn chips_and_toasts_have_icons() {
        let t = Theme::cozy();
        let has_icon = |n: &Node| format!("{n:?}").contains("Icon {");
        assert!(has_icon(&chip(&Badge::Inbox(3), &t)));
        assert!(format!("{:?}", chip(&Badge::BatteryLow(12), &t)).contains("Gauge"));
        assert!(has_icon(&toast("salvo", ToastKind::Info, &t, 9)));
        let long = toast(&"palavra ".repeat(40), ToastKind::Error, &t, 9);
        assert!(format!("{long:?}").contains("Warning"));
        let mut f = crate::overlay::draw::Fonts::new();
        let big = crate::ui::Size { w: 2000, h: 2000 };
        let sz = crate::ui::measure(&long, big, &mut f, &t);
        assert!(sz.w <= TOAST_MAX_W, "{sz:?}");
        assert!(sz.h > 40, "wrapped: {sz:?}");
    }
}
