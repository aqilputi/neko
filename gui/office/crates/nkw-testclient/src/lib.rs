//! nkw-testclient's pure parts: the CLI, the buffer contents and the JSON log
//! (contract S3 of the nkw work packages). The Wayland side lives in main.rs.

use std::io::Write;

use serde::Serialize;

pub const USAGE: &str = "usage: nkw-testclient [--app-id ID] [--title T] [--color RRGGBB | --checker] \
[--size WxH] [--log PATH|-] [--popup-on-key] [--exit-after-ms N] [--no-frame-wait]";

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
}

#[derive(Serialize)]
struct Line<'a> {
    #[serde(flatten)]
    ev: &'a Event,
    t_us: u64,
}

/// The JSON line for `ev` at `t_us` (no trailing newline).
pub fn line(ev: &Event, t_us: u64) -> String {
    serde_json::to_string(&Line { ev, t_us }).expect("events always serialise")
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
        let l = line(&ev, monotonic_us());
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
