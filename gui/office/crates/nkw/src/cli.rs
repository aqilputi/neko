//! Command line (contract S2 of the wave plan):
//!
//! ```text
//! nkw [--demo] [--headless] [--script FILE|-] [--shot PATH.png] [--reduced-motion] [--preset64]
//!     [--size WxH] [--scale S] [--seconds N] [--socket NAME] [--gl-debug]
//!     [--core auto|none|demo|URL] [--world-mod ctrl+alt|super] [--focus-escape KEY]
//!     [--mouse-look on|off] [--state-dir DIR] [--no-sys] [--xwayland auto|on|off]
//!     [--text-input] [-- CMD ARGS...]
//! ```

use std::path::{Path, PathBuf};

use crate::router::WorldMod;

/// Exit codes of the binary.
pub mod exit {
    pub const OK: i32 = 0;
    pub const USAGE: i32 = 2;
    pub const EXPECT: i32 = 3;
    pub const GL: i32 = 4;
    pub const SOCKET: i32 = 5;
}

pub const USAGE: &str = "usage: nkw [--demo] [--headless] [--script FILE|-] [--shot PATH.png] \
[--reduced-motion] [--preset64] [--size WxH] [--scale S] [--seconds N] [--socket NAME] \
[--gl-debug] [--core auto|none|demo|URL] [--world-mod ctrl+alt|super] [--focus-escape KEY] \
[--mouse-look on|off] [--state-dir DIR] [--no-sys] [--xwayland auto|on|off] [--text-input] \
[-- CMD ARGS...]";

/// `--xwayland` / `[launch] xwayland` (S20/S22): one type, the config's.
pub use nkw_core::rules::XwaylandMode;

/// `--core`: where the cats come from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Core {
    /// connect like `nkg` (`nk_client::StartOpts::default()`)
    Auto,
    /// no core: the cats of the fixture
    None,
    /// spawn `nk serve --demo`
    Demo,
    /// connect there (`tcp://…`, `unix://…`)
    Url(String),
}

impl Core {
    fn parse(v: &str) -> Option<Core> {
        Some(match v {
            "auto" => Core::Auto,
            "none" => Core::None,
            "demo" => Core::Demo,
            u if u.contains("://") => Core::Url(u.to_string()),
            _ => return None,
        })
    }
}

/// Where the `--script` comes from.
#[derive(Clone, Debug, PartialEq)]
pub enum ScriptSrc {
    File(PathBuf),
    Stdin,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Cli {
    pub demo: bool,
    pub headless: bool,
    pub script: Option<ScriptSrc>,
    pub shot: Option<PathBuf>,
    pub reduced_motion: bool,
    pub preset64: bool,
    /// physical px; `None`: 1280x720 headless, the host's choice nested
    pub size: Option<(u32, u32)>,
    /// `None`: 1 headless, the host's scale nested
    pub scale: Option<f64>,
    pub seconds: Option<f64>,
    pub socket: String,
    pub gl_debug: bool,
    /// spawned with the world's `WAYLAND_DISPLAY`
    pub cmd: Vec<String>,
    pub help: bool,
    /// `--core`; `None`: the default (`auto`, `none` under `--demo`)
    pub core: Option<Core>,
    /// what `world+` is
    pub world_mod: WorldMod,
    /// `--world-mod` was given (it wins over `[keys] world_mod`)
    pub world_mod_flag: bool,
    /// `--focus-escape KEY` (wins over `[keys] focus_escape`)
    pub focus_escape: Option<String>,
    /// `--mouse-look on|off` (wins over `[walk] mouse_look`)
    pub mouse_look: Option<bool>,
    /// `--state-dir`: the persistence root
    pub state_dir: Option<PathBuf>,
    /// `--no-sys`: no D-Bus/PipeWire adapters (implied by `--demo`)
    pub no_sys: bool,
    /// `--xwayland auto|on|off` (wins over `[launch] xwayland`; W3 reads it)
    pub xwayland: Option<XwaylandMode>,
    /// `--text-input`: advertise text-input-v3 (an evaluation flag, W4)
    pub text_input: bool,
}

impl Default for Cli {
    fn default() -> Cli {
        Cli {
            demo: false,
            headless: false,
            script: None,
            shot: None,
            reduced_motion: false,
            preset64: false,
            size: None,
            scale: None,
            seconds: None,
            socket: "nkw-0".into(),
            gl_debug: false,
            cmd: Vec::new(),
            help: false,
            core: None,
            world_mod: WorldMod::CtrlAlt,
            world_mod_flag: false,
            focus_escape: None,
            mouse_look: None,
            state_dir: None,
            no_sys: false,
            xwayland: None,
            text_input: false,
        }
    }
}

impl Cli {
    /// The headless output size.
    pub fn size_or_default(&self) -> (u32, u32) {
        self.size.unwrap_or((1280, 720))
    }

    /// The core to use: `--core`, else `auto` (`none` under `--demo`).
    pub fn core(&self) -> Core {
        self.core
            .clone()
            .unwrap_or(if self.demo { Core::None } else { Core::Auto })
    }

    /// Whether the system adapters run (`--no-sys` and `--demo` say no:
    /// `nkw_sys::demo::fixture()` instead).
    pub fn sys(&self) -> bool {
        !self.no_sys && !self.demo
    }

    /// The persistence root: `--state-dir`, else (never under `--demo`)
    /// `$XDG_STATE_HOME/nekuroddo` or `~/.local/state/nekuroddo`.
    pub fn state_root(&self) -> Option<PathBuf> {
        let xdg = std::env::var_os("XDG_STATE_HOME");
        let home = std::env::var_os("HOME");
        self.state_root_from(
            xdg.as_deref().map(Path::new),
            home.as_deref().map(Path::new),
        )
    }

    /// `world.toml`: next to `world.json` with `--state-dir`, else
    /// `$XDG_CONFIG_HOME/nekuroddo/` (`~/.config/nekuroddo/`); `None` under
    /// `--demo` (the same file `Wm::new` reads).
    pub fn world_toml(&self) -> Option<PathBuf> {
        let env = |k: &str| std::env::var_os(k).map(PathBuf::from);
        self.world_toml_from(env("XDG_CONFIG_HOME").as_deref(), env("HOME").as_deref())
    }

    fn world_toml_from(&self, config_home: Option<&Path>, home: Option<&Path>) -> Option<PathBuf> {
        if self.demo {
            return None;
        }
        crate::wm::persist_io::config_path(self.state_dir.as_deref(), config_home, home)
    }

    /// What `world+` is: `--world-mod`, else `[keys] world_mod`, else
    /// ctrl+alt.
    pub fn world_mod_or(&self, cfg: Option<&str>) -> WorldMod {
        if self.world_mod_flag {
            return self.world_mod;
        }
        match cfg {
            Some("super") => WorldMod::Super,
            Some("ctrl+alt") => WorldMod::CtrlAlt,
            _ => self.world_mod,
        }
    }

    /// FOCO's escape chord: `--focus-escape`, else `[keys] focus_escape`,
    /// else `ctrl+]`.
    pub fn focus_escape_or(&self, cfg: Option<&str>) -> String {
        self.focus_escape
            .as_deref()
            .or(cfg.filter(|k| !k.trim().is_empty()))
            .unwrap_or(crate::ext::FOCUS_ESCAPE)
            .to_string()
    }

    /// The satellite's mode: `--xwayland`, else the config's.
    pub fn xwayland_or(&self, cfg: XwaylandMode) -> XwaylandMode {
        self.xwayland.unwrap_or(cfg)
    }

    /// ANDAR's mouse-look: `--mouse-look`, else `[walk] mouse_look`.
    pub fn mouse_look_or(&self, cfg: bool) -> bool {
        self.mouse_look.unwrap_or(cfg)
    }

    fn state_root_from(&self, xdg: Option<&Path>, home: Option<&Path>) -> Option<PathBuf> {
        if let Some(d) = &self.state_dir {
            return Some(d.clone());
        }
        if self.demo {
            return None;
        }
        match (xdg.filter(|p| p.is_absolute()), home) {
            (Some(x), _) => Some(x.join("nekuroddo")),
            (None, Some(h)) => Some(h.join(".local/state/nekuroddo")),
            (None, None) => None,
        }
    }
}

/// A usage error; the binary prints it with `USAGE` and exits 2.
#[derive(Clone, Debug, PartialEq)]
pub struct UsageError(pub String);

impl std::fmt::Display for UsageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "nkw: {}\n{USAGE}", self.0)
    }
}

fn parse_size(s: &str) -> Option<(u32, u32)> {
    let (w, h) = s.split_once(['x', 'X'])?;
    let (w, h) = (w.parse().ok()?, h.parse().ok()?);
    (w > 0 && h > 0 && w <= 16384 && h <= 16384).then_some((w, h))
}

/// Parse the arguments (without the program name). `env_reduced`:
/// `NKW_REDUCED_MOTION=1` is set.
pub fn parse<S: AsRef<str>>(args: &[S], env_reduced: bool) -> Result<Cli, UsageError> {
    let mut cli = Cli {
        reduced_motion: env_reduced,
        ..Cli::default()
    };
    let mut it = args.iter().map(AsRef::as_ref);
    let err = |m: String| Err(UsageError(m));
    while let Some(a) = it.next() {
        let mut value = |name: &str| {
            it.next()
                .map(str::to_string)
                .ok_or_else(|| UsageError(format!("{name} needs a value")))
        };
        match a {
            "--" => {
                cli.cmd = it.by_ref().map(str::to_string).collect();
                if cli.cmd.is_empty() {
                    return err("nothing after --".into());
                }
            }
            "--demo" => cli.demo = true,
            "--headless" => cli.headless = true,
            "--reduced-motion" => cli.reduced_motion = true,
            "--preset64" => cli.preset64 = true,
            "--gl-debug" => cli.gl_debug = true,
            "--no-sys" => cli.no_sys = true,
            "--core" => {
                let v = value(a)?;
                cli.core =
                    Some(Core::parse(&v).ok_or_else(|| {
                        UsageError(format!("bad --core {v:?} (auto|none|demo|URL)"))
                    })?);
            }
            "--world-mod" => {
                let v = value(a)?;
                cli.world_mod = match v.as_str() {
                    "ctrl+alt" => WorldMod::CtrlAlt,
                    "super" => WorldMod::Super,
                    _ => return err(format!("bad --world-mod {v:?} (ctrl+alt|super)")),
                };
                cli.world_mod_flag = true;
            }
            "--focus-escape" => {
                let v = value(a)?;
                if v.trim().is_empty() {
                    return err("bad --focus-escape \"\"".into());
                }
                cli.focus_escape = Some(v);
            }
            "--mouse-look" => {
                let v = value(a)?;
                cli.mouse_look = Some(match v.as_str() {
                    "on" => true,
                    "off" => false,
                    _ => return err(format!("bad --mouse-look {v:?} (on|off)")),
                });
            }
            "--xwayland" => {
                let v = value(a)?;
                cli.xwayland = Some(match v.as_str() {
                    "auto" => XwaylandMode::Auto,
                    "on" => XwaylandMode::On,
                    "off" => XwaylandMode::Off,
                    _ => return err(format!("bad --xwayland {v:?} (auto|on|off)")),
                });
            }
            "--text-input" => cli.text_input = true,
            "--state-dir" => {
                let v = value(a)?;
                if v.is_empty() {
                    return err("bad --state-dir \"\"".into());
                }
                cli.state_dir = Some(v.into());
            }
            "-h" | "--help" => cli.help = true,
            "--script" => {
                let v = value(a)?;
                cli.script = Some(if v == "-" {
                    ScriptSrc::Stdin
                } else {
                    ScriptSrc::File(v.into())
                });
            }
            "--shot" => cli.shot = Some(value(a)?.into()),
            "--socket" => {
                let v = value(a)?;
                if v.is_empty() || v.contains('/') {
                    return err(format!("bad --socket {v:?}"));
                }
                cli.socket = v;
            }
            "--size" => {
                let v = value(a)?;
                cli.size = Some(
                    parse_size(&v).ok_or_else(|| UsageError(format!("bad --size {v:?} (WxH)")))?,
                );
            }
            "--scale" => {
                let v = value(a)?;
                match v.parse::<f64>() {
                    Ok(s) if (0.5..=4.0).contains(&s) => cli.scale = Some(s),
                    _ => return err(format!("bad --scale {v:?} (0.5..4)")),
                }
            }
            "--seconds" => {
                let v = value(a)?;
                match v.parse::<f64>() {
                    Ok(s) if s > 0.0 && s.is_finite() => cli.seconds = Some(s),
                    _ => return err(format!("bad --seconds {v:?}")),
                }
            }
            other => return err(format!("unknown argument {other:?}")),
        }
    }
    Ok(cli)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(args: &[&str]) -> Cli {
        parse(args, false).unwrap()
    }

    #[test]
    fn defaults() {
        let c = ok(&[]);
        assert_eq!(c, Cli::default());
        assert_eq!(c.socket, "nkw-0");
        assert_eq!(c.size_or_default(), (1280, 720));
    }

    #[test]
    fn table_of_good_args() {
        type Check = Box<dyn Fn(&Cli) -> bool>;
        let cases: Vec<(Vec<&str>, Check)> = vec![
            (vec!["--demo"], Box::new(|c| c.demo)),
            (vec!["--headless"], Box::new(|c| c.headless)),
            (vec!["--reduced-motion"], Box::new(|c| c.reduced_motion)),
            (vec!["--preset64"], Box::new(|c| c.preset64)),
            (vec!["--gl-debug"], Box::new(|c| c.gl_debug)),
            (vec!["--help"], Box::new(|c| c.help)),
            (
                vec!["--script", "t.nks"],
                Box::new(|c| c.script == Some(ScriptSrc::File("t.nks".into()))),
            ),
            (
                vec!["--script", "-"],
                Box::new(|c| c.script == Some(ScriptSrc::Stdin)),
            ),
            (
                vec!["--shot", "a.png"],
                Box::new(|c| c.shot.as_deref() == Some("a.png".as_ref())),
            ),
            (
                vec!["--size", "1920x1080"],
                Box::new(|c| c.size == Some((1920, 1080))),
            ),
            (vec!["--scale", "1.25"], Box::new(|c| c.scale == Some(1.25))),
            (
                vec!["--seconds", "60"],
                Box::new(|c| c.seconds == Some(60.0)),
            ),
            (
                vec!["--socket", "nkw-test"],
                Box::new(|c| c.socket == "nkw-test"),
            ),
            (
                vec!["--demo", "--", "foot", "-e", "--demo"],
                Box::new(|c| c.demo && c.cmd == ["foot", "-e", "--demo"]),
            ),
            (
                vec!["--core", "none"],
                Box::new(|c| c.core == Some(Core::None)),
            ),
            (vec!["--core", "demo"], Box::new(|c| c.core() == Core::Demo)),
            (vec!["--core", "auto"], Box::new(|c| c.core() == Core::Auto)),
            (
                vec!["--core", "tcp://127.0.0.1:7777"],
                Box::new(|c| c.core() == Core::Url("tcp://127.0.0.1:7777".into())),
            ),
            (
                vec!["--demo", "--core", "demo"],
                Box::new(|c| c.demo && c.core() == Core::Demo),
            ),
            (
                vec!["--demo"],
                Box::new(|c| c.core() == Core::None && !c.sys()),
            ),
            (vec![], Box::new(|c| c.core() == Core::Auto && c.sys())),
            (
                vec!["--world-mod", "super"],
                Box::new(|c| c.world_mod == WorldMod::Super),
            ),
            (
                vec!["--world-mod", "ctrl+alt"],
                Box::new(|c| c.world_mod == WorldMod::CtrlAlt),
            ),
            (
                vec!["--state-dir", "/tmp/s"],
                Box::new(|c| c.state_dir.as_deref() == Some("/tmp/s".as_ref())),
            ),
            (vec!["--no-sys"], Box::new(|c| c.no_sys && !c.sys())),
            (
                vec!["--focus-escape", "ctrl+g"],
                Box::new(|c| c.focus_escape.as_deref() == Some("ctrl+g")),
            ),
            (
                vec!["--mouse-look", "off"],
                Box::new(|c| c.mouse_look == Some(false)),
            ),
            (
                vec!["--mouse-look", "on"],
                Box::new(|c| c.mouse_look == Some(true)),
            ),
            (
                vec!["--xwayland", "off"],
                Box::new(|c| {
                    c.xwayland == Some(XwaylandMode::Off)
                        && c.xwayland_or(XwaylandMode::On) == XwaylandMode::Off
                }),
            ),
            (
                vec!["--xwayland", "on"],
                Box::new(|c| c.xwayland == Some(XwaylandMode::On)),
            ),
            (
                vec!["--xwayland", "auto"],
                Box::new(|c| c.xwayland == Some(XwaylandMode::Auto)),
            ),
            (
                vec![],
                Box::new(|c| {
                    c.xwayland.is_none()
                        && !c.text_input
                        && c.xwayland_or(XwaylandMode::Off) == XwaylandMode::Off
                }),
            ),
            (vec!["--text-input"], Box::new(|c| c.text_input)),
        ];
        for (args, check) in cases {
            let c = ok(&args);
            assert!(check(&c), "{args:?} → {c:?}");
        }
    }

    #[test]
    fn env_reduced_motion() {
        assert!(parse::<&str>(&[], true).unwrap().reduced_motion);
    }

    #[test]
    fn table_of_bad_args() {
        let cases: &[(&[&str], &str)] = &[
            (&["--bogus"], "unknown argument"),
            (&["--size"], "needs a value"),
            (&["--size", "12"], "bad --size"),
            (&["--size", "0x10"], "bad --size"),
            (&["--size", "axb"], "bad --size"),
            (&["--scale", "9"], "bad --scale"),
            (&["--scale", "x"], "bad --scale"),
            (&["--seconds", "-1"], "bad --seconds"),
            (&["--seconds", "nan"], "bad --seconds"),
            (&["--socket", "a/b"], "bad --socket"),
            (&["--script"], "needs a value"),
            (&["--"], "nothing after --"),
            (&["--core"], "needs a value"),
            (&["--core", "maybe"], "bad --core"),
            (&["--world-mod", "hyper"], "bad --world-mod"),
            (&["--state-dir", ""], "bad --state-dir"),
            (&["--focus-escape", " "], "bad --focus-escape"),
            (&["--mouse-look", "maybe"], "bad --mouse-look"),
            (&["--mouse-look"], "needs a value"),
            (&["--xwayland", "maybe"], "bad --xwayland"),
            (&["--xwayland"], "needs a value"),
        ];
        for (args, want) in cases {
            let e = parse(args, false).unwrap_err();
            assert!(e.0.contains(want), "{args:?} → {e:?}");
            let text = e.to_string();
            assert!(
                text.starts_with("nkw: ") && text.contains("usage: nkw"),
                "{text}"
            );
        }
    }

    #[test]
    fn state_root_defaults() {
        let x = Some(Path::new("/x/state"));
        let h = Some(Path::new("/home/demo"));
        let c = ok(&[]);
        assert_eq!(
            c.state_root_from(x, h),
            Some(PathBuf::from("/x/state/nekuroddo"))
        );
        assert_eq!(
            c.state_root_from(None, h),
            Some(PathBuf::from("/home/demo/.local/state/nekuroddo"))
        );
        assert_eq!(
            c.state_root_from(Some(Path::new("rel")), h),
            Some(PathBuf::from("/home/demo/.local/state/nekuroddo")),
            "a relative XDG_STATE_HOME is ignored"
        );
        assert_eq!(
            ok(&["--demo"]).state_root_from(x, h),
            None,
            "never under --demo"
        );
        assert_eq!(
            ok(&["--demo", "--state-dir", "/t"]).state_root_from(x, h),
            Some(PathBuf::from("/t")),
            "unless given"
        );
    }

    #[test]
    fn world_toml_follows_the_wm() {
        let c = Some(Path::new("/c"));
        let h = Some(Path::new("/home/demo"));
        assert_eq!(
            ok(&["--state-dir", "/s"]).world_toml_from(c, h),
            Some(PathBuf::from("/s/world.toml"))
        );
        assert_eq!(
            ok(&[]).world_toml_from(c, h),
            Some(PathBuf::from("/c/nekuroddo/world.toml"))
        );
        assert_eq!(
            ok(&[]).world_toml_from(None, h),
            Some(PathBuf::from("/home/demo/.config/nekuroddo/world.toml"))
        );
        assert_eq!(ok(&["--demo"]).world_toml_from(c, h), None);
        assert_eq!(
            ok(&["--demo", "--state-dir", "/s"]).world_toml_from(c, h),
            None,
            "the demo never reads a world.toml"
        );
    }

    #[test]
    fn the_flags_win_over_world_toml() {
        let plain = ok(&[]);
        assert_eq!(plain.world_mod_or(Some("super")), WorldMod::Super);
        assert_eq!(plain.world_mod_or(Some("hyper")), WorldMod::CtrlAlt);
        assert_eq!(plain.world_mod_or(None), WorldMod::CtrlAlt);
        let flag = ok(&["--world-mod", "ctrl+alt"]);
        assert_eq!(flag.world_mod_or(Some("super")), WorldMod::CtrlAlt);
        assert_eq!(plain.focus_escape_or(Some("ctrl+g")), "ctrl+g");
        assert_eq!(plain.focus_escape_or(Some(" ")), "ctrl+]");
        assert_eq!(plain.focus_escape_or(None), "ctrl+]");
        let esc = ok(&["--focus-escape", "ctrl+q"]);
        assert_eq!(esc.focus_escape_or(Some("ctrl+g")), "ctrl+q");
        assert!(plain.mouse_look_or(true));
        assert!(!ok(&["--mouse-look", "off"]).mouse_look_or(true));
        assert!(ok(&["--mouse-look", "on"]).mouse_look_or(false));
    }

    #[test]
    fn exit_codes_are_the_contract() {
        assert_eq!(
            [exit::OK, exit::USAGE, exit::EXPECT, exit::GL, exit::SOCKET],
            [0, 2, 3, 4, 5]
        );
    }
}
