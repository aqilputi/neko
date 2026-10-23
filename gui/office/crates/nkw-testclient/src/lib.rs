//! nkw-testclient's pure parts: the CLI, the buffer contents and the JSON log
//! (contract S3 of the nkw work packages). The Wayland side lives in main.rs.
//!
//! Wave-2 additions to S3 (all optional, the S3 lines are unchanged):
//! - `--copy TEXT`: offers TEXT as the selection (`text/plain;charset=utf-8`,
//!   `text/plain`, `UTF8_STRING`) at the first keyboard `enter` after mapping
//!   (compositors refuse `set_selection` from unfocused clients); logs
//!   `{"ev":"selection_set"}`, and `{"ev":"selection_cancelled"}` when replaced.
//! - `--log-selection`: each selection offer is read as text and logged as
//!   `{"ev":"selection","mime":"text/plain;charset=utf-8","text":"…"}`.
//! - `--damage WxH`: every frame repaints and damages only the top-left `W`×`H`
//!   rect (alternating content), like a terminal while typing.
//! - `--fractional`: binds `wp_fractional_scale_v1` + `wp_viewporter`, renders the
//!   buffer at the preferred scale (`round(logical·scale/120)` px) with the viewport
//!   destination at the logical size; logs `{"ev":"scale","scale_120":150}`.
//! - `--fullscreen`: asks for fullscreen before the first commit.
//! - `--parent-of APP_ID`: the same process first maps a mother window
//!   (`app_id` = `title` = APP_ID, solid grey), then, after the mother's first
//!   configure, maps the main window with `xdg_toplevel.set_parent(mother)`
//!   (`set_parent` only takes toplevels of the same client). The mother's lines
//!   carry `"win":"parent"`; the main window's lines have no `win` key.
//!
//! Wave-3 additions (R1):
//! - `--activation-token`: reads `XDG_ACTIVATION_TOKEN` (and unsets it, as
//!   the protocol asks), and after the first configure calls
//!   `xdg_activation_v1.activate(token, surface)`; logs
//!   `{"ev":"activation","token":true}` (`false`: no token in the env).
//!   The token itself is never logged.
//! - `--spawn-child`: 300 ms after start, starts a second testclient as its
//!   own child (a pid descendant: placement intents, `world.open`), with the
//!   same arguments except `--spawn-child`/`--activation-token`/`--parent-of`,
//!   `--app-id`/`--title` + `.child` and the log at `<log>.child` (see
//!   [`child_argv`]); logs `{"ev":"spawned_child","pid":N}`. The child dies
//!   with its parent (`PR_SET_PDEATHSIG`).

use std::io::Write;

use serde::Serialize;

pub const USAGE: &str = "usage: nkw-testclient [--app-id ID] [--title T] [--color RRGGBB | --checker] \
[--size WxH] [--log PATH|-] [--popup-on-key] [--exit-after-ms N] [--no-frame-wait] [--copy TEXT] \
[--log-selection] [--damage WxH] [--fractional] [--fullscreen] [--parent-of APP_ID] \
[--activation-token] [--spawn-child]";

/// The mimes `--copy` offers and `--log-selection` prefers, best first.
pub const TEXT_MIMES: [&str; 3] = ["text/plain;charset=utf-8", "text/plain", "UTF8_STRING"];

/// Fill of the `--parent-of` mother window.
pub const MOTHER_FILL: Fill = Fill::Color(0x808080);

/// What the buffer shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fill {
    /// Opaque `0xRRGGBB`.
    Color(u32),
    /// 1 px black/white checkerboard, black at (0,0).
    Checker,
}

/// Where the JSON lines go.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LogTarget {
    Stdout,
    File(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Args {
    pub app_id: String,
    pub title: String,
    pub fill: Fill,
    /// Used until (and whenever) the compositor leaves the size to us.
    pub size: (u32, u32),
    pub log: LogTarget,
    pub popup_on_key: bool,
    pub exit_after_ms: Option<u64>,
    /// Draw on configure only and never ask for frame callbacks (an idle client).
    pub no_frame_wait: bool,
    /// `--copy`: the text offered as the selection.
    pub copy: Option<String>,
    /// `--log-selection`: read and log every selection offer.
    pub log_selection: bool,
    /// `--damage`: the top-left rect redrawn each frame.
    pub damage: Option<(u32, u32)>,
    /// `--fractional`: render at `wp_fractional_scale_v1`'s preferred scale.
    pub fractional: bool,
    pub fullscreen: bool,
    /// `--parent-of`: the mother window's app_id (and title).
    pub parent_of: Option<String>,
    /// `--activation-token`: activate with `XDG_ACTIVATION_TOKEN`.
    pub activation_token: bool,
    /// `--spawn-child`: start a child testclient after [`CHILD_AFTER_MS`].
    pub spawn_child: bool,
}

/// When `--spawn-child` starts its child.
pub const CHILD_AFTER_MS: u64 = 300;

/// The child's arguments for `--spawn-child` (from the parent's `argv`):
/// the flags that make a second parent (`--spawn-child`), use a one-shot
/// token (`--activation-token`) or a mother (`--parent-of X`) are dropped;
/// `--app-id`/`--title` get `.child` (added when missing); `--log P` becomes
/// `P.child` (stdout stays stdout).
pub fn child_argv<S: AsRef<str>>(argv: &[S]) -> Vec<String> {
    let mut out = Vec::new();
    let (mut app_id, mut title) = (false, false);
    let mut it = argv.iter().map(AsRef::as_ref);
    while let Some(a) = it.next() {
        match a {
            "--spawn-child" | "--activation-token" => {}
            "--parent-of" => {
                it.next();
            }
            "--app-id" | "--title" | "--log" => {
                let Some(v) = it.next() else { break };
                let v = match a {
                    "--log" if v == "-" => v.to_string(),
                    _ => format!("{v}.child"),
                };
                app_id |= a == "--app-id";
                title |= a == "--title";
                out.extend([a.to_string(), v]);
            }
            _ => out.push(a.to_string()),
        }
    }
    let d = Args::default();
    if !app_id {
        out.extend(["--app-id".into(), format!("{}.child", d.app_id)]);
    }
    if !title {
        out.extend(["--title".into(), format!("{}.child", d.title)]);
    }
    out
}

impl Default for Args {
    fn default() -> Self {
        Args {
            app_id: "nkw-testclient".into(),
            title: "nkw-testclient".into(),
            fill: Fill::Color(0x3366cc),
            size: (640, 480),
            log: LogTarget::Stdout,
            popup_on_key: false,
            exit_after_ms: None,
            no_frame_wait: false,
            copy: None,
            log_selection: false,
            damage: None,
            fractional: false,
            fullscreen: false,
            parent_of: None,
            activation_token: false,
            spawn_child: false,
        }
    }
}

/// Parses the arguments after the program name.
pub fn parse_args<S: AsRef<str>>(argv: &[S]) -> Result<Args, String> {
    let mut a = Args::default();
    let mut fill_set = false;
    let mut it = argv.iter().map(AsRef::as_ref);
    while let Some(flag) = it.next() {
        let mut value = || it.next().ok_or_else(|| format!("{flag} needs a value"));
        match flag {
            "--app-id" => a.app_id = value()?.to_string(),
            "--title" => a.title = value()?.to_string(),
            "--color" | "--checker" => {
                if fill_set {
                    return Err("--color and --checker are exclusive".into());
                }
                fill_set = true;
                a.fill = if flag == "--checker" {
                    Fill::Checker
                } else {
                    Fill::Color(parse_color(value()?)?)
                };
            }
            "--size" => a.size = parse_size(value()?)?,
            "--log" => {
                let v = value()?;
                a.log = if v == "-" {
                    LogTarget::Stdout
                } else {
                    LogTarget::File(v.to_string())
                };
            }
            "--popup-on-key" => a.popup_on_key = true,
            "--exit-after-ms" => {
                let v = value()?;
                a.exit_after_ms = Some(
                    v.parse()
                        .map_err(|_| format!("--exit-after-ms: not a number: {v}"))?,
                );
            }
            "--no-frame-wait" => a.no_frame_wait = true,
            "--copy" => a.copy = Some(value()?.to_string()),
            "--log-selection" => a.log_selection = true,
            "--damage" => {
                let v = value()?;
                a.damage = Some(parse_size(v).map_err(|_| format!("--damage wants WxH, got {v}"))?);
            }
            "--fractional" => a.fractional = true,
            "--fullscreen" => a.fullscreen = true,
            "--parent-of" => {
                let v = value()?;
                if v.is_empty() {
                    return Err("--parent-of needs an app_id".into());
                }
                a.parent_of = Some(v.to_string());
            }
            "--activation-token" => a.activation_token = true,
            "--spawn-child" => a.spawn_child = true,
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    Ok(a)
}

fn parse_color(s: &str) -> Result<u32, String> {
    let hex = s.strip_prefix('#').unwrap_or(s);
    if hex.len() != 6 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!("--color wants RRGGBB, got {s}"));
    }
    u32::from_str_radix(hex, 16).map_err(|e| e.to_string())
}

fn parse_size(s: &str) -> Result<(u32, u32), String> {
    let bad = || format!("--size wants WxH, got {s}");
    let (w, h) = s.split_once('x').ok_or_else(bad)?;
    let (w, h): (u32, u32) = (w.parse().map_err(|_| bad())?, h.parse().map_err(|_| bad())?);
    if w == 0 || h == 0 || w > 16384 || h > 16384 {
        return Err(bad());
    }
    Ok((w, h))
}

/// The ARGB8888 value of pixel (x, y).
pub fn pixel(fill: Fill, x: u32, y: u32) -> u32 {
    match fill {
        Fill::Color(rgb) => 0xff00_0000 | (rgb & 0x00ff_ffff),
        Fill::Checker if (x + y).is_multiple_of(2) => 0xff00_0000,
        Fill::Checker => 0xffff_ffff,
    }
}

/// Paints a `w`×`h` ARGB8888 (little-endian, stride `w*4`) canvas.
pub fn paint(canvas: &mut [u8], w: u32, h: u32, fill: Fill) {
    let w = w as usize;
    for (i, px) in canvas
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .take(w * h as usize)
        .enumerate()
    {
        let v = pixel(fill, (i % w) as u32, (i / w) as u32);
        px.copy_from_slice(&v.to_le_bytes());
    }
}

/// Paints the `rw`×`rh` rect at the origin of a canvas `w` px wide, clamped to
/// `w`×`h`. `odd` inverts the colours, so successive frames really differ.
pub fn paint_rect(canvas: &mut [u8], w: u32, h: u32, (rw, rh): (u32, u32), fill: Fill, odd: bool) {
    let (rw, rh) = (rw.min(w) as usize, rh.min(h) as usize);
    let w = w as usize;
    let px = canvas.as_chunks_mut::<4>().0;
    for y in 0..rh {
        for (x, p) in px[y * w..y * w + rw].iter_mut().enumerate() {
            let mut v = pixel(fill, x as u32, y as u32);
            if odd {
                v ^= 0x00ff_ffff;
            }
            p.copy_from_slice(&v.to_le_bytes());
        }
    }
}

/// The damage rect of `--damage` for a buffer of `size`: clamped, never empty.
pub fn damage_rect((rw, rh): (u32, u32), (w, h): (u32, u32)) -> (u32, u32) {
    (rw.clamp(1, w.max(1)), rh.clamp(1, h.max(1)))
}

/// Buffer size for a logical size at a `wp_fractional_scale_v1` scale (in 120ths):
/// `round(logical · scale / 120)`, half away from zero, as the protocol says.
pub fn physical_size((w, h): (u32, u32), scale_120: u32) -> (u32, u32) {
    let f = |v: u32| ((v as u64 * scale_120 as u64 + 60) / 120).max(1) as u32;
    (f(w), f(h))
}

/// The text mime `--log-selection` reads from an offer, best first.
pub fn pick_text_mime(offered: &[String]) -> Option<&'static str> {
    TEXT_MIMES
        .into_iter()
        .find(|m| offered.iter().any(|o| o == m))
}

/// `CLOCK_MONOTONIC` in µs: the clock `nkw`'s `NKW_STATS` uses too.
pub fn monotonic_us() -> u64 {
    let mut ts = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: clock_gettime only writes into the timespec we own.
    unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut ts) };
    ts.tv_sec as u64 * 1_000_000 + ts.tv_nsec as u64 / 1_000
}

/// xdg_toplevel states by protocol name, from SCTK's `WindowState` bits.
pub fn state_names(bits: u16) -> Vec<&'static str> {
    const NAMES: [(u16, &str); 9] = [
        (1 << 0, "maximized"),
        (1 << 1, "fullscreen"),
        (1 << 2, "resizing"),
        (1 << 3, "activated"),
        (1 << 4, "tiled_left"),
        (1 << 5, "tiled_right"),
        (1 << 6, "tiled_top"),
        (1 << 7, "tiled_bottom"),
        (1 << 8, "suspended"),
    ];
    NAMES
        .iter()
        .filter(|(b, _)| bits & b != 0)
        .map(|(_, n)| *n)
        .collect()
}

/// A keysym name as the `--script` language spells it (`XK_h` → `h`).
pub fn sym_name(debug_name: Option<&str>, raw: u32) -> String {
    match debug_name {
        Some(n) => n.strip_prefix("XK_").unwrap_or(n).to_string(),
        None => format!("0x{raw:x}"),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyState {
    Pressed,
    Released,
}

/// One log line. Every line also carries `t_us`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "ev", rename_all = "snake_case")]
pub enum Event {
    Configure {
        w: u32,
        h: u32,
        states: Vec<&'static str>,
    },
    Enter,
    Leave,
    Key {
        /// evdev code, as on the wire (xkb keycode − 8).
        keycode: u32,
        sym: String,
        state: KeyState,
        /// What xkb (with compose) produced; pressed only.
        #[serde(skip_serializing_if = "Option::is_none")]
        utf8: Option<String>,
    },
    Modifiers {
        ctrl: bool,
        alt: bool,
        shift: bool,
        logo: bool,
        caps_lock: bool,
        num_lock: bool,
    },
    Frame,
    PopupDone,
    Close,
    /// `--copy`: the selection was offered.
    SelectionSet,
    /// `--copy`: someone else took the selection.
    SelectionCancelled,
    /// `--log-selection`: an offer arrived and was read to the end.
    Selection {
        mime: String,
        text: String,
    },
    /// `--fractional`: the compositor's preferred scale, in 120ths.
    Scale {
        scale_120: u32,
    },
    /// `--activation-token`: `activate` was called (`token`: the env had one).
    Activation {
        token: bool,
    },
    /// `--spawn-child`: the child testclient's pid.
    SpawnedChild {
        pid: u32,
    },
}

#[derive(Serialize)]
struct Line<'a> {
    #[serde(flatten)]
    ev: &'a Event,
    t_us: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    win: Option<&'a str>,
}

/// The JSON line for `ev` at `t_us` (no trailing newline).
pub fn line(ev: &Event, t_us: u64) -> String {
    line_win(ev, t_us, None)
}

/// [`line`] for a secondary window: `win` tags it (`"parent"` for `--parent-of`).
pub fn line_win(ev: &Event, t_us: u64, win: Option<&str>) -> String {
    serde_json::to_string(&Line { ev, t_us, win }).expect("events always serialise")
}

/// Writes one flushed line per event, so a reader sees it immediately.
pub struct Log {
    out: Box<dyn Write>,
}

impl Log {
    pub fn new(out: Box<dyn Write>) -> Self {
        Log { out }
    }

    pub fn open(target: &LogTarget) -> std::io::Result<Self> {
        Ok(match target {
            LogTarget::Stdout => Log::new(Box::new(std::io::stdout())),
            LogTarget::File(p) => Log::new(Box::new(std::fs::File::create(p)?)),
        })
    }

    pub fn emit(&mut self, ev: Event) {
        self.emit_win(ev, None);
    }

    pub fn emit_win(&mut self, ev: Event, win: Option<&str>) {
        let l = line_win(&ev, monotonic_us(), win);
        // A test client must not die because its reader went away.
        let _ = writeln!(self.out, "{l}").and_then(|_| self.out.flush());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn ok(argv: &[&str]) -> Args {
        parse_args(argv).unwrap()
    }

    #[test]
    fn cli_table() {
        let d = Args::default();
        let cases: Vec<(&[&str], Args)> = vec![
            (&[], d.clone()),
            (
                &["--app-id", "alpha", "--title", "bravo"],
                Args {
                    app_id: "alpha".into(),
                    title: "bravo".into(),
                    ..d.clone()
                },
            ),
            (
                &["--color", "ff0080"],
                Args {
                    fill: Fill::Color(0xff0080),
                    ..d.clone()
                },
            ),
            (
                &["--color", "#00FF00"],
                Args {
                    fill: Fill::Color(0x00ff00),
                    ..d.clone()
                },
            ),
            (
                &["--checker", "--size", "320x180"],
                Args {
                    fill: Fill::Checker,
                    size: (320, 180),
                    ..d.clone()
                },
            ),
            (
                &["--log", "out.json"],
                Args {
                    log: LogTarget::File("out.json".into()),
                    ..d.clone()
                },
            ),
            (&["--log", "-"], d.clone()),
            (
                &[
                    "--popup-on-key",
                    "--exit-after-ms",
                    "1500",
                    "--no-frame-wait",
                ],
                Args {
                    popup_on_key: true,
                    exit_after_ms: Some(1500),
                    no_frame_wait: true,
                    ..d.clone()
                },
            ),
        ];
        for (argv, want) in cases {
            assert_eq!(ok(argv), want, "{argv:?}");
        }
    }

    #[test]
    fn cli_wave2_flags() {
        let d = Args::default();
        let cases: Vec<(&[&str], Args)> = vec![
            (
                &["--copy", "olá mundo"],
                Args {
                    copy: Some("olá mundo".into()),
                    ..d.clone()
                },
            ),
            (
                &["--log-selection"],
                Args {
                    log_selection: true,
                    ..d.clone()
                },
            ),
            (
                &["--damage", "80x16"],
                Args {
                    damage: Some((80, 16)),
                    ..d.clone()
                },
            ),
            (
                &["--fractional", "--checker"],
                Args {
                    fractional: true,
                    fill: Fill::Checker,
                    ..d.clone()
                },
            ),
            (
                &["--fullscreen"],
                Args {
                    fullscreen: true,
                    ..d.clone()
                },
            ),
            (
                &["--parent-of", "acme.mother", "--app-id", "acme.dialog"],
                Args {
                    parent_of: Some("acme.mother".into()),
                    app_id: "acme.dialog".into(),
                    ..d.clone()
                },
            ),
        ];
        for (argv, want) in cases {
            assert_eq!(ok(argv), want, "{argv:?}");
        }
        for bad in [
            &["--copy"][..],
            &["--damage", "8"],
            &["--damage", "0x4"],
            &["--damage"],
            &["--parent-of"],
            &["--parent-of", ""],
        ] {
            assert!(parse_args(bad).is_err(), "{bad:?} should fail");
        }
        assert!(USAGE.contains("--parent-of APP_ID") && USAGE.contains("--damage WxH"));
    }

    #[test]
    fn cli_wave3_flags() {
        let a = ok(&["--activation-token", "--spawn-child"]);
        assert!(a.activation_token && a.spawn_child);
        assert!(!Args::default().activation_token && !Args::default().spawn_child);
        assert!(USAGE.contains("--activation-token") && USAGE.contains("--spawn-child"));
    }

    #[test]
    fn the_childs_arguments() {
        let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        assert_eq!(
            child_argv(&[
                "--app-id",
                "alpha",
                "--spawn-child",
                "--color",
                "3366cc",
                "--log",
                "/r/client-alpha.json",
                "--activation-token",
                "--title",
                "a",
            ]),
            s(&[
                "--app-id",
                "alpha.child",
                "--color",
                "3366cc",
                "--log",
                "/r/client-alpha.json.child",
                "--title",
                "a.child",
            ])
        );
        assert_eq!(
            child_argv(&["--spawn-child", "--log", "-", "--parent-of", "m"]),
            s(&[
                "--log",
                "-",
                "--app-id",
                "nkw-testclient.child",
                "--title",
                "nkw-testclient.child"
            ])
        );
        let child = parse_args(&child_argv(&["--spawn-child", "--checker"])).unwrap();
        assert!(!child.spawn_child, "never a grandchild");
        assert_eq!(child.fill, Fill::Checker);
    }

    #[test]
    fn wave3_lines() {
        let v: Value = serde_json::from_str(&line(&Event::Activation { token: true }, 5)).unwrap();
        assert_eq!(v, json!({"ev": "activation", "token": true, "t_us": 5}));
        let v: Value = serde_json::from_str(&line(&Event::SpawnedChild { pid: 42 }, 6)).unwrap();
        assert_eq!(v, json!({"ev": "spawned_child", "pid": 42, "t_us": 6}));
    }

    #[test]
    fn damage_rect_paints_only_the_corner_and_alternates() {
        let (w, h) = (6, 4);
        let mut c = vec![0u8; (w * h * 4) as usize];
        paint(&mut c, w, h, Fill::Color(0x112233));
        let before = c.clone();
        paint_rect(&mut c, w, h, (2, 3), Fill::Color(0x112233), true);
        let px = |c: &[u8], x: u32, y: u32| {
            let i = ((y * w + x) * 4) as usize;
            u32::from_le_bytes(c[i..i + 4].try_into().unwrap())
        };
        for y in 0..h {
            for x in 0..w {
                let inside = x < 2 && y < 3;
                if inside {
                    assert_eq!(px(&c, x, y), 0xffee_ddcc, "({x},{y})");
                } else {
                    assert_eq!(px(&c, x, y), px(&before, x, y), "({x},{y})");
                }
            }
        }
        paint_rect(&mut c, w, h, (2, 3), Fill::Color(0x112233), false);
        assert_eq!(c, before, "an even frame restores the original");
        // Clamped to the canvas.
        paint_rect(&mut c, w, h, (100, 100), Fill::Checker, false);
        assert_eq!(px(&c, 5, 3), pixel(Fill::Checker, 5, 3));
        assert_eq!(damage_rect((100, 100), (6, 4)), (6, 4));
        assert_eq!(damage_rect((3, 2), (6, 4)), (3, 2));
        assert_eq!(damage_rect((0, 0), (6, 4)), (1, 1));
    }

    #[test]
    fn physical_size_rounds_like_the_protocol() {
        let cases = [
            ((640, 480), 120, (640, 480)),
            ((640, 480), 150, (800, 600)),
            ((641, 479), 150, (801, 599)), // 801.25 → 801, 598.75 → 599
            ((100, 1), 180, (150, 2)),     // 1.5 → 2 (half away from zero)
            ((1, 1), 60, (1, 1)),
        ];
        for (l, s, want) in cases {
            assert_eq!(physical_size(l, s), want, "{l:?}@{s}");
        }
    }

    #[test]
    fn text_mime_preference() {
        let v = |xs: &[&str]| xs.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(
            pick_text_mime(&v(&["UTF8_STRING", "text/plain;charset=utf-8"])),
            Some("text/plain;charset=utf-8")
        );
        assert_eq!(
            pick_text_mime(&v(&["text/plain", "UTF8_STRING"])),
            Some("text/plain")
        );
        assert_eq!(pick_text_mime(&v(&["image/png"])), None);
        assert_eq!(pick_text_mime(&[]), None);
    }

    #[test]
    fn wave2_log_lines() {
        let cases = vec![
            (Event::SelectionSet, json!({"ev":"selection_set","t_us":7})),
            (
                Event::SelectionCancelled,
                json!({"ev":"selection_cancelled","t_us":7}),
            ),
            (
                Event::Selection {
                    mime: "text/plain;charset=utf-8".into(),
                    text: "olá".into(),
                },
                json!({"ev":"selection","mime":"text/plain;charset=utf-8","text":"olá","t_us":7}),
            ),
            (
                Event::Scale { scale_120: 150 },
                json!({"ev":"scale","scale_120":150,"t_us":7}),
            ),
        ];
        for (ev, want) in cases {
            assert_eq!(parsed(ev.clone(), 7), want, "{ev:?}");
        }
        let tagged: Value =
            serde_json::from_str(&line_win(&Event::Enter, 7, Some("parent"))).unwrap();
        assert_eq!(tagged, json!({"ev":"enter","t_us":7,"win":"parent"}));
    }

    #[test]
    fn cli_errors() {
        let bad: &[&[&str]] = &[
            &["--color"],
            &["--color", "fff"],
            &["--color", "gggggg"],
            &["--checker", "--color", "000000"],
            &["--size", "100"],
            &["--size", "0x10"],
            &["--size", "axb"],
            &["--exit-after-ms", "soon"],
            &["--log"],
            &["--bogus"],
        ];
        for argv in bad {
            assert!(parse_args(argv).is_err(), "{argv:?} should fail");
        }
    }

    #[test]
    fn checker_has_1px_period_and_black_origin() {
        let (w, h) = (5, 3);
        let mut c = vec![0u8; (w * h * 4) as usize];
        paint(&mut c, w, h, Fill::Checker);
        let px = |x: u32, y: u32| {
            let i = ((y * w + x) * 4) as usize;
            u32::from_le_bytes(c[i..i + 4].try_into().unwrap())
        };
        assert_eq!(px(0, 0), 0xff00_0000);
        assert_eq!(px(1, 0), 0xffff_ffff);
        assert_eq!(px(0, 1), 0xffff_ffff);
        assert_eq!(px(1, 1), 0xff00_0000);
        assert_eq!(px(4, 2), 0xff00_0000);
        for y in 0..h {
            for x in 0..w - 1 {
                assert_ne!(px(x, y), px(x + 1, y), "({x},{y})");
            }
        }
        // ARGB8888 little-endian: B G R A in memory.
        assert_eq!(&c[4..8], &[0xff, 0xff, 0xff, 0xff]);
        assert_eq!(&c[0..4], &[0, 0, 0, 0xff]);
    }

    #[test]
    fn solid_colour_is_opaque_and_fills_exactly_w_by_h() {
        let mut c = vec![0u8; 2 * 2 * 4 + 4];
        paint(&mut c, 2, 2, Fill::Color(0x112233));
        assert_eq!(&c[0..4], &[0x33, 0x22, 0x11, 0xff]);
        assert_eq!(&c[12..16], &[0x33, 0x22, 0x11, 0xff]);
        assert_eq!(&c[16..20], &[0, 0, 0, 0], "past w*h stays untouched");
    }

    fn parsed(ev: Event, t: u64) -> Value {
        serde_json::from_str(&line(&ev, t)).unwrap()
    }

    #[test]
    fn log_lines_have_the_s3_keys() {
        let cases = vec![
            (
                Event::Configure {
                    w: 800,
                    h: 600,
                    states: vec!["activated"],
                },
                json!({"ev":"configure","w":800,"h":600,"states":["activated"],"t_us":7}),
            ),
            (Event::Enter, json!({"ev":"enter","t_us":7})),
            (Event::Leave, json!({"ev":"leave","t_us":7})),
            (
                Event::Key {
                    keycode: 35,
                    sym: "h".into(),
                    state: KeyState::Pressed,
                    utf8: Some("h".into()),
                },
                json!({"ev":"key","keycode":35,"sym":"h","state":"pressed","utf8":"h","t_us":7}),
            ),
            (
                Event::Key {
                    keycode: 35,
                    sym: "h".into(),
                    state: KeyState::Released,
                    utf8: None,
                },
                json!({"ev":"key","keycode":35,"sym":"h","state":"released","t_us":7}),
            ),
            (
                Event::Modifiers {
                    ctrl: true,
                    alt: false,
                    shift: false,
                    logo: false,
                    caps_lock: false,
                    num_lock: true,
                },
                json!({"ev":"modifiers","ctrl":true,"alt":false,"shift":false,"logo":false,
                       "caps_lock":false,"num_lock":true,"t_us":7}),
            ),
            (Event::Frame, json!({"ev":"frame","t_us":7})),
            (Event::PopupDone, json!({"ev":"popup_done","t_us":7})),
            (Event::Close, json!({"ev":"close","t_us":7})),
        ];
        for (ev, want) in cases {
            assert_eq!(parsed(ev.clone(), 7), want, "{ev:?}");
        }
        assert!(!line(&Event::Frame, 1).contains('\n'));
    }

    #[test]
    fn log_writes_one_line_per_event() {
        #[derive(Clone, Default)]
        struct Buf(std::rc::Rc<std::cell::RefCell<Vec<u8>>>);
        impl Write for Buf {
            fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
                self.0.borrow_mut().extend_from_slice(b);
                Ok(b.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let buf = Buf::default();
        let mut log = Log::new(Box::new(buf.clone()));
        log.emit(Event::Enter);
        log.emit(Event::Frame);
        let text = String::from_utf8(buf.0.borrow().clone()).unwrap();
        let lines: Vec<Value> = text
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0]["ev"], "enter");
        assert!(lines[1]["t_us"].as_u64().unwrap() >= lines[0]["t_us"].as_u64().unwrap());
    }

    #[test]
    fn t_us_is_monotonic_microseconds() {
        let a = monotonic_us();
        std::thread::sleep(std::time::Duration::from_millis(2));
        let b = monotonic_us();
        assert!(b > a);
        assert!((1_500..1_000_000).contains(&(b - a)), "{a} {b}");
        let mut last = 0;
        for _ in 0..1000 {
            let t = monotonic_us();
            assert!(t >= last);
            last = t;
        }
    }

    #[test]
    fn state_and_sym_names() {
        assert_eq!(state_names(0), Vec::<&str>::new());
        assert_eq!(state_names(0b1000), ["activated"]);
        assert_eq!(
            state_names(0b1_0000_0011),
            ["maximized", "fullscreen", "suspended"]
        );
        assert_eq!(sym_name(Some("XK_h"), 0x68), "h");
        assert_eq!(sym_name(Some("XK_dead_acute"), 0xfe51), "dead_acute");
        assert_eq!(sym_name(None, 0x1234), "0x1234");
    }
}
