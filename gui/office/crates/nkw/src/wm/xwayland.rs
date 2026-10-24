//! X11 apps through `xwayland-satellite`, a child of the hub started on
//! demand (S22): the first `world.launch*`/`world.terminal` spawns it on
//! the first free display of 7..17 (by the absence of its socket); children
//! launched meanwhile wait (≤ 2 s) for the socket, then every child gets
//! `DISPLAY`. No XWM, no X11 code here: satellite windows arrive as plain
//! toplevels (`app_id` = `WM_CLASS`) and `[[rules]]` apply unchanged.
//!
//! The state machine is pure but for `stat`s of the socket directory (a
//! parameter: tests use a temp dir and a fake satellite) and, once up, one
//! scan of a `/proc`-shaped tree for the satellite's pid.

use std::path::{Path, PathBuf};

use nkw_core::Time;
use nkw_core::rules::XwaylandMode;

/// The satellite's binary name (looked up on `PATH`).
pub const BINARY: &str = "xwayland-satellite";
/// The displays tried, in order.
pub const DISPLAYS: std::ops::RangeInclusive<u32> = 7..=17;
/// How long children wait for the socket before the satellite is given up.
pub const START_TIMEOUT_S: Time = 2.0;
/// While up, the socket is checked at most this often (from `tick`, which
/// runs anyway: no wakeup of its own).
pub const ALIVE_EVERY_S: Time = 1.0;
/// The X11 socket directory of the host.
pub const SOCKET_DIR: &str = "/tmp/.X11-unix";

/// The satellite's state.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum XState {
    #[default]
    Off,
    Starting {
        since: Time,
    },
    Up,
    Failed(String),
}

impl XState {
    /// `"off" | "starting" | "up" | "failed"` (the dump's).
    pub fn label(&self) -> &'static str {
        match self {
            XState::Off => "off",
            XState::Starting { .. } => "starting",
            XState::Up => "up",
            XState::Failed(_) => "failed",
        }
    }
}

/// What a launch should do about X11.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ensure {
    /// spawn now; `DISPLAY` = this when `Some`
    Ready(Option<String>),
    /// the satellite is starting: queue the child until `poll` says
    Wait,
    /// spawn the satellite (this argv) and queue the child
    Start(Vec<String>),
    /// `--xwayland on` without the binary: toast this once, spawn without X11
    Missing(String),
}

/// What `poll` saw.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Poll {
    Nothing,
    /// the socket appeared: release the queue with `DISPLAY`
    Up(String),
    /// gave up: toast, release the queue without X11
    Failed(String),
    /// the socket went away while up (the satellite died)
    Gone,
}

/// The toast when the satellite does not come up.
pub const FAILED_TOAST: &str = "xwayland-satellite não subiu";
/// The toast of `--xwayland on` without the binary.
pub const MISSING_TOAST: &str = "xwayland-satellite não está no PATH: apps X11 não abrem no mundo";

/// The satellite as the WM tracks it.
#[derive(Clone, Debug, Default)]
pub struct Xwayland {
    /// `Auto`/`On`/`Off`, as resolved (`Auto` under `--demo` is `Off`)
    pub mode: XwaylandMode,
    /// the binary, when on `PATH`
    pub binary: Option<PathBuf>,
    pub socket_dir: PathBuf,
    pub state: XState,
    /// `N` of `:N` once chosen
    pub display: Option<u32>,
    /// the satellite's pid, once up (found by its argv)
    pub pid: Option<u32>,
    warned: bool,
    alive_at: Time,
}

impl Xwayland {
    pub fn new(mode: XwaylandMode, binary: Option<PathBuf>, socket_dir: PathBuf) -> Xwayland {
        Xwayland {
            mode,
            binary,
            socket_dir,
            ..Xwayland::default()
        }
    }

    /// The mode for this run: the CLI's, else the file's; `Auto` under
    /// `--demo` is `Off` (fixtures only; `--xwayland on` still works).
    pub fn resolve(cli: Option<XwaylandMode>, file: XwaylandMode, demo: bool) -> XwaylandMode {
        match cli.unwrap_or(file) {
            XwaylandMode::Auto if demo => XwaylandMode::Off,
            m => m,
        }
    }

    /// Whether X11 can happen at all this run.
    pub fn enabled(&self) -> bool {
        self.mode != XwaylandMode::Off && self.binary.is_some()
    }

    /// `":N"` while up.
    pub fn display(&self) -> Option<String> {
        match (&self.state, self.display) {
            (XState::Up, Some(n)) => Some(format!(":{n}")),
            _ => None,
        }
    }

    fn socket(&self, n: u32) -> PathBuf {
        self.socket_dir.join(format!("X{n}"))
    }

    /// The first display of [`DISPLAYS`] without a socket (or a lock file
    /// next to the socket directory).
    pub fn free_display(&self) -> Option<u32> {
        let locks = self.socket_dir.parent().map(Path::to_path_buf);
        DISPLAYS.into_iter().find(|n| {
            !self.socket(*n).exists()
                && !locks
                    .as_ref()
                    .is_some_and(|d| d.join(format!(".X{n}-lock")).exists())
        })
    }

    /// A child is about to be launched.
    pub fn ensure(&mut self, now: Time) -> Ensure {
        if self.mode == XwaylandMode::Off {
            return Ensure::Ready(None);
        }
        if self.binary.is_none() {
            if self.mode == XwaylandMode::On && !self.warned {
                self.warned = true;
                return Ensure::Missing(MISSING_TOAST.into());
            }
            return Ensure::Ready(None);
        }
        match &self.state {
            XState::Up => Ensure::Ready(self.display()),
            XState::Starting { .. } => Ensure::Wait,
            XState::Failed(_) => Ensure::Ready(None),
            XState::Off => match self.start(now) {
                Some(argv) => Ensure::Start(argv),
                None => Ensure::Ready(None),
            },
        }
    }

    /// Start now (also `world.xwayland {on: true}`): the satellite's argv,
    /// `None` when disabled, already starting/up, or no display is free
    /// (then `Failed`).
    pub fn start(&mut self, now: Time) -> Option<Vec<String>> {
        let bin = self.binary.clone()?;
        if self.mode == XwaylandMode::Off
            || matches!(self.state, XState::Up | XState::Starting { .. })
        {
            return None;
        }
        let Some(n) = self.free_display() else {
            self.state = XState::Failed("nenhum display X livre (7..17)".into());
            return None;
        };
        self.display = Some(n);
        self.pid = None;
        self.state = XState::Starting { since: now };
        Some(vec![bin.to_string_lossy().into_owned(), format!(":{n}")])
    }

    /// While starting: the socket appeared, or the time is up. While up:
    /// the socket is still there (at most every [`ALIVE_EVERY_S`]).
    pub fn poll(&mut self, now: Time) -> Poll {
        match self.state {
            XState::Starting { since } => {
                let n = self.display.unwrap_or(0);
                if self.socket(n).exists() {
                    self.state = XState::Up;
                    self.alive_at = now;
                    return Poll::Up(format!(":{n}"));
                }
                if now - since >= START_TIMEOUT_S {
                    self.state = XState::Failed(FAILED_TOAST.into());
                    return Poll::Failed(FAILED_TOAST.into());
                }
                Poll::Nothing
            }
            XState::Up if now - self.alive_at >= ALIVE_EVERY_S => {
                self.alive_at = now;
                if self.socket(self.display.unwrap_or(0)).exists() {
                    return Poll::Nothing;
                }
                self.state = XState::Off;
                self.display = None;
                self.pid = None;
                Poll::Gone
            }
            _ => Poll::Nothing,
        }
    }

    /// The next instant `poll` has something to decide (`Starting` only).
    pub fn next_poll(&self, now: Time) -> Option<Time> {
        match self.state {
            XState::Starting { since } => Some((now + 0.05).min(since + START_TIMEOUT_S)),
            _ => None,
        }
    }

    /// `world.xwayland {on: false}`: back to `Off`; the pid to stop (the
    /// satellite is our own child), if known.
    pub fn stop(&mut self) -> Option<u32> {
        self.state = XState::Off;
        self.display = None;
        self.pid.take()
    }
}

/// Look `name` up on a `PATH`-like list.
pub fn which(name: &str, path: Option<&std::ffi::OsStr>) -> Option<PathBuf> {
    std::env::split_paths(path?)
        .map(|d| d.join(name))
        .find(|p| is_executable(p))
}

fn is_executable(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(p).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

/// The pid of the satellite on `:n` that serves `our_display`: an argv
/// with `xwayland-satellite` then `:n`, and our `WAYLAND_DISPLAY`.
pub fn find_pid(proc_root: &Path, n: u32, our_display: &str) -> Option<u32> {
    let want = format!(":{n}");
    let ours = |env: &[u8]| {
        env.split(|b| *b == 0)
            .find_map(|kv| kv.strip_prefix(b"WAYLAND_DISPLAY="))
            .is_some_and(|d| {
                let d = String::from_utf8_lossy(d);
                d == our_display || d.rsplit('/').next() == our_display.rsplit('/').next()
            })
    };
    for e in std::fs::read_dir(proc_root).ok()?.flatten() {
        let Some(pid) = e.file_name().to_str().and_then(|s| s.parse::<u32>().ok()) else {
            continue;
        };
        let Ok(cmd) = std::fs::read(e.path().join("cmdline")) else {
            continue;
        };
        let args: Vec<String> = cmd
            .split(|b| *b == 0)
            .map(|a| String::from_utf8_lossy(a).into_owned())
            .collect();
        let hit = args
            .windows(2)
            .any(|w| w[0].rsplit('/').next() == Some(BINARY) && w[1] == want);
        if hit && std::fs::read(e.path().join("environ")).is_ok_and(|env| ours(&env)) {
            return Some(pid);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn up_dir() -> (tempfile::TempDir, PathBuf) {
        let t = tempfile::tempdir().unwrap();
        let d = t.path().join(".X11-unix");
        std::fs::create_dir(&d).unwrap();
        (t, d)
    }

    fn x(mode: XwaylandMode, dir: &Path) -> Xwayland {
        Xwayland::new(
            mode,
            Some(PathBuf::from("/usr/bin/xwayland-satellite")),
            dir.into(),
        )
    }

    #[test]
    fn the_mode_of_the_run() {
        use XwaylandMode::*;
        for (cli, file, demo, want) in [
            (None, Auto, false, Auto),
            (None, Auto, true, Off),
            (Some(On), Auto, true, On),
            (Some(Off), On, false, Off),
            (None, Off, false, Off),
            (Some(Auto), Off, false, Auto),
        ] {
            assert_eq!(
                Xwayland::resolve(cli, file, demo),
                want,
                "{cli:?} {file:?} {demo}"
            );
        }
    }

    #[test]
    fn off_or_missing_never_waits() {
        let (_t, d) = up_dir();
        let mut off = x(XwaylandMode::Off, &d);
        assert_eq!(off.ensure(0.0), Ensure::Ready(None));
        assert!(!off.enabled());
        let mut auto = Xwayland::new(XwaylandMode::Auto, None, d.clone());
        assert_eq!(auto.ensure(0.0), Ensure::Ready(None), "auto: quietly none");
        let mut on = Xwayland::new(XwaylandMode::On, None, d.clone());
        assert!(matches!(on.ensure(0.0), Ensure::Missing(_)));
        assert_eq!(on.ensure(1.0), Ensure::Ready(None), "the toast once");
        assert_eq!(on.state, XState::Off);
    }

    #[test]
    fn starts_waits_and_comes_up() {
        let (_t, d) = up_dir();
        std::fs::write(d.join("X7"), "").unwrap(); // :7 is taken
        let mut s = x(XwaylandMode::Auto, &d);
        assert_eq!(
            s.ensure(10.0),
            Ensure::Start(vec!["/usr/bin/xwayland-satellite".into(), ":8".into()])
        );
        assert_eq!(s.state, XState::Starting { since: 10.0 });
        assert_eq!(s.ensure(10.1), Ensure::Wait, "the next child waits too");
        assert_eq!(s.display(), None, "no DISPLAY before the socket");
        assert_eq!(s.poll(10.2), Poll::Nothing);
        assert!(s.next_poll(10.2).is_some_and(|t| (t - 10.25).abs() < 1e-9));
        std::fs::write(d.join("X8"), "").unwrap(); // the satellite is up
        assert_eq!(s.poll(10.3), Poll::Up(":8".into()));
        assert_eq!(s.state, XState::Up);
        assert_eq!(s.ensure(11.0), Ensure::Ready(Some(":8".into())));
        assert_eq!(s.next_poll(11.0), None, "no wakeups while up");
        assert_eq!(s.start(11.0), None, "already up");
        // it dies: noticed at the next check, then a launch starts it again
        std::fs::remove_file(d.join("X8")).unwrap();
        assert_eq!(s.poll(11.2), Poll::Nothing, "checked at most every second");
        assert_eq!(s.poll(11.4), Poll::Gone);
        assert_eq!(s.state, XState::Off);
        assert!(matches!(s.ensure(12.0), Ensure::Start(a) if a[1] == ":8"));
    }

    #[test]
    fn gives_up_after_two_seconds() {
        let (_t, d) = up_dir();
        let mut s = x(XwaylandMode::On, &d);
        assert!(matches!(s.ensure(0.0), Ensure::Start(_)));
        assert_eq!(s.poll(1.9), Poll::Nothing);
        assert_eq!(s.poll(2.0), Poll::Failed(FAILED_TOAST.into()));
        assert_eq!(s.state.label(), "failed");
        assert_eq!(s.ensure(3.0), Ensure::Ready(None), "no retry by itself");
        // `world.xwayland {on}` tries again after a stop
        assert_eq!(s.stop(), None);
        assert!(s.start(4.0).is_some());
    }

    #[test]
    fn no_free_display_fails() {
        let (t, d) = up_dir();
        for n in DISPLAYS {
            if n % 2 == 0 {
                std::fs::write(d.join(format!("X{n}")), "").unwrap();
            } else {
                std::fs::write(t.path().join(format!(".X{n}-lock")), "").unwrap();
            }
        }
        let mut s = x(XwaylandMode::Auto, &d);
        assert_eq!(s.free_display(), None);
        assert_eq!(s.ensure(0.0), Ensure::Ready(None));
        assert!(matches!(s.state, XState::Failed(_)));
    }

    #[test]
    fn labels_and_which() {
        assert_eq!(XState::Off.label(), "off");
        assert_eq!(XState::Starting { since: 0.0 }.label(), "starting");
        assert_eq!(XState::Up.label(), "up");
        assert_eq!(XState::Failed("x".into()).label(), "failed");
        let t = tempfile::tempdir().unwrap();
        let bin = t.path().join(BINARY);
        std::fs::write(&bin, "#!/bin/sh\n").unwrap();
        assert_eq!(
            which(BINARY, Some(t.path().as_os_str())),
            None,
            "not executable"
        );
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        let path = std::env::join_paths(["/nonexistent", t.path().to_str().unwrap()]).unwrap();
        assert_eq!(which(BINARY, Some(&path)), Some(bin));
        assert_eq!(which(BINARY, None), None);
    }

    #[test]
    fn the_satellites_pid_by_argv_and_display() {
        let t = tempfile::tempdir().unwrap();
        for (pid, argv, wd) in [
            ("20", "/usr/bin/xwayland-satellite\0:7\0", "wayland-1"),
            ("21", "/bin/sh\0/srv/bin/xwayland-satellite\0:7\0", "nkw-0"),
            ("22", "xwayland-satellite\0:8\0", "nkw-0"),
        ] {
            let p = t.path().join(pid);
            std::fs::create_dir(&p).unwrap();
            std::fs::write(p.join("cmdline"), argv).unwrap();
            std::fs::write(p.join("environ"), format!("WAYLAND_DISPLAY={wd}\0")).unwrap();
        }
        assert_eq!(
            find_pid(t.path(), 7, "nkw-0"),
            Some(21),
            "ours, not the host's"
        );
        assert_eq!(find_pid(t.path(), 8, "nkw-0"), Some(22));
        assert_eq!(find_pid(t.path(), 9, "nkw-0"), None);
    }
}
