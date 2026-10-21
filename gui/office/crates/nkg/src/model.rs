//! The office model: what the core says (snapshot + tree rows + theme),
//! reduced to what the scene draws — columns (one per session, in tree
//! order), contexts, poses, collars. Pure: no Bevy app, no I/O.

use std::collections::HashMap;

/// What a cat is doing (plan §4.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Pose {
    /// busy: paws on the keyboard, head bob
    Typing,
    /// blocked / needs you: sits up, "!" above
    Alert,
    /// done / returned: head on the desk, "zZ"
    Sleeping,
    /// idle: calm, tail sway
    Idle,
    /// error/ended: a one-shot fur puff, then a quiet sit
    Puff,
}

/// Attention values of the core (store.Attention).
pub const ATT_BLOCKED: i64 = 1;
pub const ATT_RETURNED: i64 = 2;

/// state/attention → pose. Precedence: puff, alert, typing, sleeping, idle.
pub fn pose_for(state: &str, attention: i64, ended: bool, finished: bool) -> Pose {
    if ended || state == "error" || state == "failed" {
        Pose::Puff
    } else if attention == ATT_BLOCKED || state == "blocked" {
        Pose::Alert
    } else if state == "busy" {
        Pose::Typing
    } else if state == "done" || finished || attention == ATT_RETURNED {
        Pose::Sleeping
    } else {
        Pose::Idle
    }
}

pub type Rgb = [u8; 3];

/// Parses `#rrggbb` (or `rrggbb`).
pub fn parse_hex(s: &str) -> Option<Rgb> {
    let h = s.trim().trim_start_matches('#');
    if h.len() != 6 {
        return None;
    }
    let v = u32::from_str_radix(h, 16).ok()?;
    Some([(v >> 16) as u8, (v >> 8) as u8, v as u8])
}

/// Linear mix of two colours (t = 0 → a, 1 → b).
pub fn mix(a: Rgb, b: Rgb, t: f32) -> Rgb {
    let f = |x: u8, y: u8| {
        (x as f32 + (y as f32 - x as f32) * t)
            .round()
            .clamp(0.0, 255.0) as u8
    };
    [f(a[0], b[0]), f(a[1], b[1]), f(a[2], b[2])]
}

/// The theme tokens the GUI paints with (fallbacks: catppuccin mocha).
#[derive(Clone, Debug, PartialEq)]
pub struct Palette {
    pub bg: Rgb,
    pub crust: Rgb,
    pub surface: Rgb,
    pub overlay: Rgb,
    pub border: Rgb,
    pub text: Rgb,
    pub subtle: Rgb,
    pub muted: Rgb,
    pub accent: Rgb,
    pub accent2: Rgb,
    pub busy: Rgb,
    pub blocked: Rgb,
    pub done: Rgb,
    pub idle: Rgb,
    pub error: Rgb,
    pub cat_body: Rgb,
    pub cat_eyes: Rgb,
    pub contexts: Vec<Rgb>,
}

impl Default for Palette {
    fn default() -> Self {
        Palette::from_tokens(&HashMap::new(), &[])
    }
}

impl Palette {
    pub fn from_tokens(tokens: &HashMap<String, String>, contexts: &[String]) -> Palette {
        let t = |k: &str, d: &str| {
            tokens
                .get(k)
                .and_then(|v| parse_hex(v))
                .unwrap_or_else(|| parse_hex(d).unwrap())
        };
        let bg = t("bg", "#1e1e2e");
        let cat_body = t("cat_body", "#11111b");
        let mut ctx: Vec<Rgb> = contexts.iter().filter_map(|c| parse_hex(c)).collect();
        if ctx.is_empty() {
            ctx = [
                "#f5c2e7", "#89b4fa", "#a6e3a1", "#fab387", "#94e2d5", "#f9e2af", "#b4befe",
            ]
            .iter()
            .map(|c| parse_hex(c).unwrap())
            .collect();
        }
        Palette {
            bg,
            crust: mix(bg, cat_body, 0.6),
            surface: t("surface", "#313244"),
            overlay: t("overlay", "#45475a"),
            border: t("border", "#585b70"),
            text: t("text", "#cdd6f4"),
            subtle: t("subtle", "#a6adc8"),
            muted: t("muted", "#7f849c"),
            accent: t("accent", "#cba6f7"),
            accent2: t("accent2", "#89b4fa"),
            busy: t("busy", "#89dceb"),
            blocked: t("blocked", "#f38ba8"),
            done: t("done", "#a6e3a1"),
            idle: t("idle", "#9399b2"),
            error: t("error", "#eba0ac"),
            cat_body,
            cat_eyes: t("cat_eyes", "#f9e2af"),
            contexts: ctx,
        }
    }

    /// Context / group colour for a palette index (`contexts[i % len]`).
    pub fn context(&self, i: i64) -> Rgb {
        let n = self.contexts.len() as i64;
        self.contexts[i.rem_euclid(n.max(1)) as usize]
    }

    /// Light theme (rose-pine-dawn): the scene keeps a dark office anyway,
    /// but text contrast decisions look at this.
    #[cfg(test)]
    pub fn is_light(&self) -> bool {
        let [r, g, b] = self.bg;
        (r as u32 + g as u32 + b as u32) > 3 * 160
    }
}

/// One session as the office needs it.
#[derive(Clone, Debug, PartialEq)]
pub struct Sess {
    pub id: String,
    pub short: String,
    pub label: String,
    pub ctx: String,
    pub state: String,
    pub attention: i64,
    pub last_at_ms: i64,
    pub needs: String,
    pub running_subs: usize,
    /// group tags with their colour index (core.GroupColorIndex)
    pub groups: Vec<(String, i64)>,
    pub dim: bool,
    pub archived: bool,
    pub ended: bool,
    pub finished: bool,
    /// cost so far ($)
    pub cost: f64,
}

impl Sess {
    pub fn pose(&self) -> Pose {
        pose_for(&self.state, self.attention, self.ended, self.finished)
    }
}

/// A row of the tree (only what the strip needs).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Row {
    pub kind: String,
    pub ctx: String,
    pub color: i64,
    pub session_id: String,
    pub groups: Vec<(String, i64)>,
}

/// One column of the strip.
#[derive(Clone, Debug, PartialEq)]
pub struct Column {
    pub sess: Sess,
    /// context palette index
    pub ctx_color: i64,
    /// first group's palette index (the collar), if any
    pub collar: Option<i64>,
}

/// The strip: columns in tree order (sessions tab).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Office {
    pub columns: Vec<Column>,
}

impl Office {
    /// Columns = the `sess` rows of the tree, in order; details from the snapshot.
    pub fn build(sessions: &[Sess], rows: &[Row]) -> Office {
        let by_id: HashMap<&str, &Sess> = sessions.iter().map(|s| (s.id.as_str(), s)).collect();
        let mut columns = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for r in rows {
            if r.kind != "sess" || r.session_id.is_empty() || !seen.insert(r.session_id.clone()) {
                continue;
            }
            let Some(s) = by_id.get(r.session_id.as_str()) else {
                continue;
            };
            let mut sess = (*s).clone();
            if !r.groups.is_empty() {
                sess.groups = r.groups.clone();
            }
            if sess.ctx.is_empty() {
                sess.ctx = r.ctx.clone();
            }
            columns.push(Column {
                collar: sess.groups.first().map(|g| g.1),
                ctx_color: r.color,
                sess,
            });
        }
        Office { columns }
    }

    pub fn index_of(&self, id: &str) -> Option<usize> {
        self.columns.iter().position(|c| c.sess.id == id)
    }

    pub fn ctxs(&self) -> Vec<&str> {
        self.columns.iter().map(|c| c.sess.ctx.as_str()).collect()
    }

    /// Counts for the top bar: (running, waiting on you, returned).
    pub fn counts(sessions: &[Sess]) -> (usize, usize, usize) {
        let live = sessions.iter().filter(|s| !s.archived);
        let mut r = (0, 0, 0);
        for s in live {
            if s.state == "busy" {
                r.0 += 1;
            }
            if s.attention == ATT_BLOCKED {
                r.1 += 1;
            }
            if s.attention == ATT_RETURNED {
                r.2 += 1;
            }
        }
        r
    }
}

// ---- time ----

/// Milliseconds since the epoch of an RFC 3339 timestamp
/// (`2026-10-21T11:12:15.711411658-03:00`, `…Z`); 0 if unparsable.
pub fn parse_rfc3339(s: &str) -> i64 {
    fn num(s: &str) -> Option<i64> {
        s.parse().ok()
    }
    let s = s.trim();
    if s.len() < 19 {
        return 0;
    }
    let b = s.as_bytes();
    if b[4] != b'-'
        || b[7] != b'-'
        || (b[10] != b'T' && b[10] != b' ')
        || b[13] != b':'
        || b[16] != b':'
    {
        return 0;
    }
    let (Some(y), Some(mo), Some(d), Some(h), Some(mi), Some(se)) = (
        num(&s[0..4]),
        num(&s[5..7]),
        num(&s[8..10]),
        num(&s[11..13]),
        num(&s[14..16]),
        num(&s[17..19]),
    ) else {
        return 0;
    };
    let mut rest = &s[19..];
    let mut ms = 0i64;
    if let Some(r) = rest.strip_prefix('.') {
        let digits: String = r.chars().take_while(|c| c.is_ascii_digit()).collect();
        let frac = format!("{:0<3}", &digits[..digits.len().min(3)]);
        ms = num(&frac).unwrap_or(0);
        rest = &r[digits.len()..];
    }
    let off_min = if rest.is_empty() || rest == "Z" || rest == "z" {
        0
    } else if rest.len() == 6 && (rest.starts_with('+') || rest.starts_with('-')) {
        let sign = if rest.starts_with('-') { -1 } else { 1 };
        match (num(&rest[1..3]), num(&rest[4..6])) {
            (Some(oh), Some(om)) => sign * (oh * 60 + om),
            _ => return 0,
        }
    } else {
        return 0;
    };
    let days = days_from_civil(y, mo, d);
    let secs = days * 86400 + h * 3600 + mi * 60 + se - off_min * 60;
    secs * 1000 + ms
}

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

/// ms epoch → `YYYY-MM-DDTHH:MM:SSZ` (for the stub and tests).
#[cfg(any(test, not(feature = "client")))]
pub fn format_rfc3339(ms: i64) -> String {
    let secs = ms.div_euclid(1000);
    let days = secs.div_euclid(86400);
    let sod = secs.rem_euclid(86400);
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        sod / 3600,
        (sod / 60) % 60,
        sod % 60
    )
}

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sess(id: &str, ctx: &str, state: &str, att: i64) -> Sess {
        Sess {
            id: id.into(),
            short: id.into(),
            label: id.into(),
            ctx: ctx.into(),
            state: state.into(),
            attention: att,
            last_at_ms: 0,
            needs: String::new(),
            running_subs: 0,
            groups: vec![],
            dim: false,
            archived: false,
            ended: false,
            finished: false,
            cost: 0.0,
        }
    }

    #[test]
    fn poses() {
        assert_eq!(pose_for("busy", 0, false, false), Pose::Typing);
        assert_eq!(pose_for("blocked", 0, false, false), Pose::Alert);
        assert_eq!(pose_for("busy", ATT_BLOCKED, false, false), Pose::Alert);
        assert_eq!(pose_for("done", 2, false, false), Pose::Sleeping);
        assert_eq!(pose_for("idle", 0, false, true), Pose::Sleeping);
        assert_eq!(pose_for("idle", 3, false, false), Pose::Idle);
        assert_eq!(pose_for("busy", 0, true, false), Pose::Puff);
        assert_eq!(pose_for("error", 1, false, false), Pose::Puff);
    }

    #[test]
    fn hex_and_palette() {
        assert_eq!(parse_hex("#1e1e2e"), Some([0x1e, 0x1e, 0x2e]));
        assert_eq!(parse_hex("zz"), None);
        let mut tok = HashMap::new();
        tok.insert("accent".to_string(), "#ff0000".to_string());
        let p = Palette::from_tokens(&tok, &["#000001".into(), "#000002".into()]);
        assert_eq!(p.accent, [255, 0, 0]);
        assert_eq!(p.context(3), [0, 0, 2]);
        assert_eq!(p.context(-1), [0, 0, 2]);
        assert!(!p.is_light());
    }

    #[test]
    fn office_follows_tree_order() {
        let ss = vec![
            sess("a", "x", "busy", 0),
            sess("b", "y", "idle", 0),
            sess("c", "x", "done", 2),
        ];
        let rows = vec![
            Row {
                kind: "ctx".into(),
                ctx: "x".into(),
                ..Default::default()
            },
            Row {
                kind: "sess".into(),
                ctx: "x".into(),
                session_id: "c".into(),
                color: 0,
                groups: vec![("t".into(), 5)],
            },
            Row {
                kind: "sub".into(),
                ctx: "x".into(),
                session_id: "c".into(),
                ..Default::default()
            },
            Row {
                kind: "sess".into(),
                ctx: "x".into(),
                session_id: "a".into(),
                color: 0,
                ..Default::default()
            },
            Row {
                kind: "sess".into(),
                ctx: "y".into(),
                session_id: "b".into(),
                color: 1,
                ..Default::default()
            },
            Row {
                kind: "sess".into(),
                ctx: "y".into(),
                session_id: "gone".into(),
                color: 1,
                ..Default::default()
            },
        ];
        let o = Office::build(&ss, &rows);
        let ids: Vec<_> = o.columns.iter().map(|c| c.sess.id.as_str()).collect();
        assert_eq!(ids, ["c", "a", "b"]);
        assert_eq!(o.columns[0].collar, Some(5));
        assert_eq!(o.columns[2].ctx_color, 1);
        assert_eq!(o.index_of("b"), Some(2));
        assert_eq!(Office::counts(&ss), (1, 0, 1));
    }

    #[test]
    fn rfc3339() {
        assert_eq!(parse_rfc3339("1970-01-01T00:00:00Z"), 0);
        assert_eq!(parse_rfc3339("1970-01-01T00:00:01.5Z"), 1500);
        assert_eq!(
            parse_rfc3339("2026-10-21T11:12:15.711411658-03:00"),
            parse_rfc3339("2026-10-21T14:12:15.711Z")
        );
        assert_eq!(parse_rfc3339("garbage"), 0);
        let t = parse_rfc3339("2026-10-21T14:12:15Z");
        assert_eq!(format_rfc3339(t), "2026-10-21T14:12:15Z");
        assert_eq!(parse_rfc3339("2000-02-29T00:00:00+00:00"), 951782400000);
    }
}
