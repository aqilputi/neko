//! The overlay's own preferences (S21): what the phone's Ajustes toggles
//! live — reduced motion (overlay-local override), high contrast, font
//! scale, the status strip — persisted in `<state-dir>/ui.toml` (`[prefs]`;
//! never under `--demo`), and the strip's default from `world.toml`
//! (`[hud] strip`). Unknown keys are ignored in both files.

use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::ui::Theme;

/// When the status strip shows (S21).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Strip {
    /// clock, inbox, cost, playing; battery/net only when not nominal
    #[default]
    Auto,
    /// every field it has
    Always,
    Never,
}

impl Strip {
    pub fn parse(s: &str) -> Option<Strip> {
        match s {
            "auto" => Some(Strip::Auto),
            "always" | "sempre" => Some(Strip::Always),
            "never" | "nunca" => Some(Strip::Never),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Strip::Auto => "auto",
            Strip::Always => "always",
            Strip::Never => "never",
        }
    }

    /// pt-BR, for Ajustes.
    pub fn label(self) -> &'static str {
        match self {
            Strip::Auto => "auto",
            Strip::Always => "sempre",
            Strip::Never => "nunca",
        }
    }

    /// auto → sempre → nunca → auto
    pub fn next(self) -> Strip {
        match self {
            Strip::Auto => Strip::Always,
            Strip::Always => Strip::Never,
            Strip::Never => Strip::Auto,
        }
    }
}

/// The font scale's range and step, percent.
pub const FONT_MIN: u16 = 80;
pub const FONT_MAX: u16 = 160;
pub const FONT_STEP: u16 = 10;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Prefs {
    /// `Some`: overrides the hub's reduced motion for the overlay's own
    /// animations
    pub reduced_motion: Option<bool>,
    pub high_contrast: bool,
    /// 80..=160
    pub font_scale_pct: u16,
    /// `Some`: overrides `[hud] strip`
    pub strip: Option<Strip>,
}

impl Default for Prefs {
    fn default() -> Prefs {
        Prefs {
            reduced_motion: None,
            high_contrast: false,
            font_scale_pct: 100,
            strip: None,
        }
    }
}

fn pct(scale: f64) -> u16 {
    ((scale * 100.0).round() as i64).clamp(FONT_MIN as i64, FONT_MAX as i64) as u16
}

impl Prefs {
    /// The theme these prefs ask for.
    pub fn theme(&self) -> Theme {
        let mut t = if self.high_contrast {
            Theme::high_contrast()
        } else {
            Theme::cozy()
        };
        t.font_scale_pct = self.font_scale_pct.clamp(FONT_MIN, FONT_MAX);
        t
    }

    /// `[prefs]` of `ui.toml`; a broken file or field is the default.
    pub fn parse(text: &str) -> Prefs {
        let mut p = Prefs::default();
        let Ok(v) = text.parse::<toml::Table>() else {
            return p;
        };
        let Some(t) = v.get("prefs").and_then(|t| t.as_table()) else {
            return p;
        };
        p.reduced_motion = t.get("reduced_motion").and_then(|v| v.as_bool());
        p.high_contrast = t.get("contrast").and_then(|v| v.as_str()) == Some("high");
        if let Some(f) = t
            .get("font_scale")
            .and_then(|v| v.as_float().or_else(|| v.as_integer().map(|i| i as f64)))
        {
            p.font_scale_pct = pct(f);
        }
        p.strip = t
            .get("strip")
            .and_then(|v| v.as_str())
            .and_then(Strip::parse);
        p
    }

    /// The file's text (only what is set; the rest stays default).
    pub fn to_toml(&self) -> String {
        let mut s = String::from("# nkw: preferências do HUD e do telefone (Ajustes)\n[prefs]\n");
        if let Some(r) = self.reduced_motion {
            s += &format!("reduced_motion = {r}\n");
        }
        s += &format!(
            "contrast = \"{}\"\n",
            if self.high_contrast { "high" } else { "normal" }
        );
        s += &format!("font_scale = {:.1}\n", self.font_scale_pct as f64 / 100.0);
        if let Some(st) = self.strip {
            s += &format!("strip = \"{}\"\n", st.as_str());
        }
        s
    }

    /// `inject overlay {"prefs": {...}}`: `contrast` "high"|"normal",
    /// `font_scale` 0.8..1.6, `strip`, `reduced_motion` bool|null.
    pub fn apply_json(&mut self, v: &Value) {
        if let Some(c) = v.get("contrast").and_then(Value::as_str) {
            self.high_contrast = c == "high";
        }
        if let Some(f) = v.get("font_scale").and_then(Value::as_f64) {
            self.font_scale_pct = pct(f);
        }
        if let Some(s) = v.get("strip") {
            self.strip = s.as_str().and_then(Strip::parse);
        }
        if let Some(r) = v.get("reduced_motion") {
            self.reduced_motion = r.as_bool();
        }
    }

    /// One font step up (wraps back to 80 % after 160 %) or down (stops).
    pub fn font_step(&mut self, up: bool) {
        self.font_scale_pct = if up {
            if self.font_scale_pct >= FONT_MAX {
                FONT_MIN
            } else {
                self.font_scale_pct + FONT_STEP
            }
        } else {
            self.font_scale_pct.saturating_sub(FONT_STEP).max(FONT_MIN)
        }
        .clamp(FONT_MIN, FONT_MAX);
    }
}

/// `[hud] strip` of a `world.toml` text (default auto).
pub fn hud_strip(world_toml: &str) -> Strip {
    world_toml
        .parse::<toml::Table>()
        .ok()
        .and_then(|v| {
            v.get("hud")?
                .as_table()?
                .get("strip")?
                .as_str()
                .and_then(Strip::parse)
        })
        .unwrap_or_default()
}

/// Where `ui.toml` lives: `<state root>/ui.toml`, never under `--demo`.
pub fn path(cli: &crate::cli::Cli) -> Option<PathBuf> {
    // unit tests never touch the real state dir: only an explicit one
    if cli.demo || (cfg!(test) && cli.state_dir.is_none()) {
        return None;
    }
    cli.state_root().map(|d| d.join("ui.toml"))
}

/// The `world.toml` the `[hud]` table comes from (the same file the WM
/// reads; none under `--demo`, nor in unit tests without a state dir).
pub fn world_toml(cli: &crate::cli::Cli) -> Option<PathBuf> {
    if cfg!(test) && cli.state_dir.is_none() {
        return None;
    }
    cli.world_toml()
}

/// Read the prefs (a missing file: the defaults).
pub fn load(path: Option<&Path>) -> Prefs {
    path.and_then(|p| std::fs::read_to_string(p).ok())
        .map(|t| Prefs::parse(&t))
        .unwrap_or_default()
}

/// Write the prefs (atomically: a temp file renamed over); `None` (the
/// demo) writes nothing.
pub fn save(path: Option<&Path>, p: &Prefs) -> std::io::Result<()> {
    let Some(path) = path else { return Ok(()) };
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("toml.tmp");
    std::fs::write(&tmp, p.to_toml())?;
    std::fs::rename(tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_defaults_and_fields() {
        assert_eq!(Prefs::parse(""), Prefs::default());
        assert_eq!(Prefs::parse("not toml ["), Prefs::default());
        let p = Prefs::parse(
            "[prefs]\nreduced_motion = true\ncontrast = \"high\"\nfont_scale = 1.3\nstrip = \"never\"\nwhat = 1\n",
        );
        assert_eq!(p.reduced_motion, Some(true));
        assert!(p.high_contrast);
        assert_eq!(p.font_scale_pct, 130);
        assert_eq!(p.strip, Some(Strip::Never));
        assert_eq!(
            Prefs::parse("[prefs]\nfont_scale = 9.0").font_scale_pct,
            160
        );
        assert_eq!(Prefs::parse("[prefs]\nfont_scale = 1").font_scale_pct, 100);
        assert_eq!(Prefs::parse("[prefs]\nstrip = \"x\"").strip, None);
    }

    #[test]
    fn round_trip_through_a_tempdir() {
        let d = tempfile::tempdir().unwrap();
        let path = d.path().join("sub/ui.toml");
        assert_eq!(load(Some(&path)), Prefs::default(), "missing file");
        let p = Prefs {
            reduced_motion: Some(false),
            high_contrast: true,
            font_scale_pct: 120,
            strip: Some(Strip::Always),
        };
        save(Some(&path), &p).unwrap();
        assert_eq!(load(Some(&path)), p);
        let q = Prefs::default();
        save(Some(&path), &q).unwrap();
        assert_eq!(load(Some(&path)), q);
        assert!(!path.with_extension("toml.tmp").exists());
        // no path: nothing written, no error
        save(None, &p).unwrap();
    }

    #[test]
    fn demo_has_no_path() {
        let cli = crate::cli::Cli {
            demo: true,
            state_dir: Some(PathBuf::from("/tmp/x")),
            ..Default::default()
        };
        assert_eq!(path(&cli), None);
        let cli = crate::cli::Cli {
            state_dir: Some(PathBuf::from("/tmp/x")),
            ..Default::default()
        };
        assert_eq!(path(&cli), Some(PathBuf::from("/tmp/x/ui.toml")));
    }

    #[test]
    fn hud_strip_from_world_toml() {
        assert_eq!(hud_strip(""), Strip::Auto);
        assert_eq!(
            hud_strip("[hud]\nstrip = \"always\"\nother = 2"),
            Strip::Always
        );
        assert_eq!(hud_strip("[hud]\nstrip = \"never\""), Strip::Never);
        assert_eq!(hud_strip("[hud]\nstrip = 3"), Strip::Auto);
        assert_eq!(hud_strip("[walk]\nspeed = 2"), Strip::Auto);
    }

    #[test]
    fn json_prefs_steps_and_theme() {
        let mut p = Prefs::default();
        p.apply_json(&json!({"contrast": "high", "font_scale": 1.2, "strip": "always"}));
        assert!(p.high_contrast);
        assert_eq!(p.font_scale_pct, 120);
        assert_eq!(p.strip, Some(Strip::Always));
        let t = p.theme();
        assert!(t.high_contrast);
        assert_eq!(t.font_scale_pct, 120);
        p.apply_json(&json!({"strip": null, "reduced_motion": true}));
        assert_eq!((p.strip, p.reduced_motion), (None, Some(true)));
        let mut f = Prefs {
            font_scale_pct: 150,
            ..Default::default()
        };
        f.font_step(true);
        assert_eq!(f.font_scale_pct, 160);
        f.font_step(true);
        assert_eq!(f.font_scale_pct, 80, "wraps");
        f.font_step(false);
        assert_eq!(f.font_scale_pct, 80, "stops at 80");
        assert_eq!(Strip::Auto.next().next().next(), Strip::Auto);
        assert_eq!(Strip::Always.label(), "sempre");
    }
}
