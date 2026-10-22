//! Command line (contract S2 of the wave plan):
//!
//! ```text
//! nkw [--demo] [--headless] [--script FILE|-] [--shot PATH.png] [--reduced-motion] [--preset64]
//!     [--size WxH] [--scale S] [--seconds N] [--socket NAME] [--gl-debug] [-- CMD ARGS...]
//! ```

use std::path::PathBuf;

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
[--gl-debug] [-- CMD ARGS...]";

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
        }
    }
}

impl Cli {
    /// The headless output size.
    pub fn size_or_default(&self) -> (u32, u32) {
        self.size.unwrap_or((1280, 720))
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
    fn exit_codes_are_the_contract() {
        assert_eq!(
            [exit::OK, exit::USAGE, exit::EXPECT, exit::GL, exit::SOCKET],
            [0, 2, 3, 4, 5]
        );
    }
}
