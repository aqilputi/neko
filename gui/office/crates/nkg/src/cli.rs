//! `nkg [--demo] [--connect tcp://…|unix://…] [--nk <path>] [--reduced-motion]
//!      [--shot <png> [--frames N] [--shot-zoom] [--shot-inbox] [--focus N]]`

use std::path::PathBuf;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Cli {
    pub demo: bool,
    pub connect: Option<String>,
    pub nk_bin: Option<PathBuf>,
    pub reduced_motion: bool,
    /// render, save a screenshot here and exit
    pub shot: Option<PathBuf>,
    /// frames to wait (after the office is loaded) before the screenshot
    pub frames: u32,
    /// zoom onto the focused monitor (pane open) before the shot
    pub shot_zoom: bool,
    /// shoot while an inbox cat walks
    pub shot_inbox: bool,
    /// focus this column first
    pub focus: Option<usize>,
}

pub const USAGE: &str = "uso: nkg [--demo] [--connect tcp://127.0.0.1:PORTA|unix:///caminho] [--nk <binário>] [--reduced-motion]
          [--shot <arquivo.png> [--frames N] [--shot-zoom] [--shot-inbox] [--focus N]]";

pub fn parse(args: impl IntoIterator<Item = String>, env_reduced: bool) -> Result<Cli, String> {
    let mut c = Cli {
        frames: 90,
        reduced_motion: env_reduced,
        ..Default::default()
    };
    let mut it = args.into_iter();
    while let Some(a) = it.next() {
        let mut val = |name: &str| {
            it.next()
                .ok_or_else(|| format!("{name} precisa de um valor\n{USAGE}"))
        };
        match a.as_str() {
            "--demo" => c.demo = true,
            "--connect" => c.connect = Some(val("--connect")?),
            "--nk" => c.nk_bin = Some(PathBuf::from(val("--nk")?)),
            "--reduced-motion" => c.reduced_motion = true,
            "--shot" => c.shot = Some(PathBuf::from(val("--shot")?)),
            "--frames" => {
                c.frames = val("--frames")?
                    .parse()
                    .map_err(|_| format!("--frames: número\n{USAGE}"))?
            }
            "--shot-zoom" => c.shot_zoom = true,
            "--shot-inbox" => c.shot_inbox = true,
            "--focus" => {
                c.focus = Some(
                    val("--focus")?
                        .parse()
                        .map_err(|_| format!("--focus: número\n{USAGE}"))?,
                )
            }
            "-h" | "--help" => return Err(USAGE.to_string()),
            other => {
                if let Some(v) = other.strip_prefix("--connect=") {
                    c.connect = Some(v.to_string());
                } else {
                    return Err(format!("argumento desconhecido: {other}\n{USAGE}"));
                }
            }
        }
    }
    if let Some(addr) = &c.connect
        && !(addr.starts_with("tcp://") || addr.starts_with("unix://"))
    {
        return Err(format!("--connect: tcp://… ou unix://…\n{USAGE}"));
    }
    Ok(c)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(a: &[&str]) -> Result<Cli, String> {
        parse(a.iter().map(|s| s.to_string()), false)
    }

    #[test]
    fn flags() {
        let c = p(&[
            "--demo",
            "--shot",
            "/tmp/x.png",
            "--frames",
            "12",
            "--shot-zoom",
            "--focus",
            "2",
        ])
        .unwrap();
        assert!(c.demo && c.shot_zoom);
        assert_eq!(c.shot, Some(PathBuf::from("/tmp/x.png")));
        assert_eq!(c.frames, 12);
        assert_eq!(c.focus, Some(2));
        assert_eq!(p(&[]).unwrap().frames, 90);
        assert_eq!(
            p(&["--connect=unix:///run/x.sock"])
                .unwrap()
                .connect
                .as_deref(),
            Some("unix:///run/x.sock")
        );
        assert!(parse(Vec::new(), true).unwrap().reduced_motion);
    }

    #[test]
    fn errors() {
        assert!(p(&["--frames"]).is_err());
        assert!(p(&["--frames", "x"]).is_err());
        assert!(p(&["--connect", "http://x"]).is_err());
        assert!(p(&["--bogus"]).is_err());
        assert!(p(&["--help"]).is_err());
    }
}
