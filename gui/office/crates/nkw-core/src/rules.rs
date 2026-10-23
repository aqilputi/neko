//! Window rules and the world's own config file, `world.toml` (§4.4).
//!
//! ```toml
//! [[rules]]
//! app_id = "acme.term"      # exact, or a prefix ending in `*` ("acme.*")
//! room = "acme"             # by name
//! desk = "last"             # or an index: a new desk there
//! width = "two_thirds"      # third | half | two_thirds | full
//!
//! [launch]
//! terminal = "foot"         # or ["foot", "-e", "tmux"]
//!
//! [keys]
//! focus_escape = "ctrl+]"
//! world_mod = "ctrl+alt"    # or "super"
//!
//! [walk]                    # ANDAR (§3.5); out-of-range values are clamped
//! speed = 2.8               # m/s, 1..5
//! run_mul = 2.0             # 1..3
//! turn = "smooth"           # or "snap" (the default under reduced motion)
//! turn_rate_dps = 110       # 45..240
//! snap_deg = 30             # 15 | 30 | 45
//! mouse_look = true
//! mouse_deg_per_count = 0.12
//! comfort_vignette = false
//! on_exit = "auto"          # auto | snap | stay
//!
//! [camera]
//! eye_height = 1.5          # 1.2..1.8
//! pitch_max_deg = 55
//! ```
//!
//! Pure: the text is parsed here, the file is read by `nkw`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::collide::TurnMode;
use crate::persist::{DeskAt, Place};
use crate::strip::Width;
use crate::walk_exit::ExitRule;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    pub app_id: String,
    #[serde(default)]
    pub room: Option<String>,
    #[serde(default)]
    pub desk: Option<DeskAt>,
    #[serde(default)]
    pub width: Option<Width>,
}

impl Rule {
    pub fn matches(&self, app_id: &str) -> bool {
        match self.app_id.strip_suffix('*') {
            Some(prefix) => app_id.starts_with(prefix),
            None => self.app_id == app_id,
        }
    }

    fn place(&self) -> Place {
        Place {
            room: self.room.clone(),
            desk: self.desk,
            width: self.width,
            air: None,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Launch {
    /// the terminal of `world.terminal`
    #[serde(default, deserialize_with = "argv")]
    pub terminal: Option<Vec<String>>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Keys {
    #[serde(default)]
    pub focus_escape: Option<String>,
    #[serde(default)]
    pub world_mod: Option<String>,
}

/// `[walk]` + `[camera]` of `world.toml` (plan §3.5 table), clamped.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WalkConfig {
    /// m/s, 1..5
    pub speed: f32,
    /// 1..3
    pub run_mul: f32,
    pub turn: TurnMode,
    /// 45..240
    pub turn_rate_dps: f32,
    /// 15 | 30 | 45
    pub snap_deg: f32,
    pub pitch_max_deg: f32,
    pub mouse_look: bool,
    pub mouse_deg_per_count: f32,
    /// metres, 1.2..1.8
    pub eye_height: f32,
    pub comfort_vignette: bool,
    pub on_exit: ExitRule,
}

impl Default for WalkConfig {
    fn default() -> WalkConfig {
        WalkConfig {
            speed: 2.8,
            run_mul: 2.0,
            turn: TurnMode::Smooth,
            turn_rate_dps: 110.0,
            snap_deg: 30.0,
            pitch_max_deg: 55.0,
            mouse_look: true,
            mouse_deg_per_count: 0.12,
            eye_height: 1.5,
            comfort_vignette: false,
            on_exit: ExitRule::Auto,
        }
    }
}

/// `world.toml`. Unknown top-level tables are kept for the other parts of
/// the world (`[objects]`, …) and ignored here.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub rules: Vec<Rule>,
    #[serde(default)]
    pub launch: Launch,
    #[serde(default)]
    pub keys: Keys,
    /// `[walk]` + `[camera]`, clamped (filled by [`parse`])
    #[serde(skip)]
    pub walk: WalkConfig,
    /// `[walk] turn` was given (else reduced motion picks `snap`)
    #[serde(skip)]
    pub walk_turn_set: bool,
    /// what [`parse`] clamped, for the caller to log
    #[serde(skip)]
    pub notes: Vec<String>,
}

/// `"foot -e tmux"` or `["foot", "-e", "tmux"]`.
fn argv<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<Vec<String>>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Raw {
        Line(String),
        Argv(Vec<String>),
    }
    let v = match Raw::deserialize(d)? {
        Raw::Line(s) => s.split_whitespace().map(String::from).collect::<Vec<_>>(),
        Raw::Argv(v) => v,
    };
    if v.is_empty() {
        return Err(serde::de::Error::custom("terminal: empty"));
    }
    Ok(Some(v))
}

/// Parse `world.toml`'s text.
pub fn parse(text: &str) -> Result<Config, String> {
    let err = |e: toml::de::Error| format!("world.toml: {e}");
    let mut c: Config = toml::from_str(text).map_err(err)?;
    let t: WalkTables = toml::from_str(text).map_err(err)?;
    c.walk_turn_set = t.walk.turn.is_some();
    (c.walk, c.notes) = t.config();
    Ok(c)
}

/// `[walk]` as written (every key optional; unknown keys such as
/// `run_key` are left to the other readers).
#[derive(Clone, Copy, Debug, Default, Deserialize)]
struct WalkRaw {
    speed: Option<f32>,
    run_mul: Option<f32>,
    turn: Option<TurnMode>,
    turn_rate_dps: Option<f32>,
    snap_deg: Option<f32>,
    mouse_look: Option<bool>,
    mouse_deg_per_count: Option<f32>,
    comfort_vignette: Option<bool>,
    on_exit: Option<ExitRule>,
    // also accepted here, the camera's own table wins
    eye_height: Option<f32>,
    pitch_max_deg: Option<f32>,
}

/// `[camera]` as written.
#[derive(Clone, Copy, Debug, Default, Deserialize)]
struct CameraRaw {
    eye_height: Option<f32>,
    pitch_max_deg: Option<f32>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize)]
struct WalkTables {
    #[serde(default)]
    walk: WalkRaw,
    #[serde(default)]
    camera: CameraRaw,
}

impl WalkTables {
    /// The clamped config and a note per clamped value.
    fn config(&self) -> (WalkConfig, Vec<String>) {
        let d = WalkConfig::default();
        let mut notes = Vec::new();
        fn clamp(
            notes: &mut Vec<String>,
            key: &str,
            v: Option<f32>,
            r: (f32, f32),
            def: f32,
        ) -> f32 {
            let (lo, hi) = r;
            match v {
                None => def,
                Some(x) if x.is_nan() || x < lo || x > hi => {
                    let got = if x.is_nan() { def } else { x.clamp(lo, hi) };
                    notes.push(format!(
                        "world.toml: {key} = {x} fora de {lo}..{hi}; usando {got}"
                    ));
                    got
                }
                Some(x) => x,
            }
        }
        let n = &mut notes;
        let w = &self.walk;
        let speed = clamp(n, "walk.speed", w.speed, (1.0, 5.0), d.speed);
        let run_mul = clamp(n, "walk.run_mul", w.run_mul, (1.0, 3.0), d.run_mul);
        let turn_rate_dps = clamp(
            n,
            "walk.turn_rate_dps",
            w.turn_rate_dps,
            (45.0, 240.0),
            d.turn_rate_dps,
        );
        let snap_deg = match w.snap_deg {
            None => d.snap_deg,
            Some(x) if [15.0, 30.0, 45.0].contains(&x) => x,
            Some(x) => {
                let got = [15.0f32, 30.0, 45.0]
                    .into_iter()
                    .min_by(|a, b| (a - x).abs().total_cmp(&(b - x).abs()))
                    .filter(|_| !x.is_nan())
                    .unwrap_or(d.snap_deg);
                n.push(format!(
                    "world.toml: walk.snap_deg = {x}: 15, 30 ou 45; usando {got}"
                ));
                got
            }
        };
        let mouse = clamp(
            n,
            "walk.mouse_deg_per_count",
            w.mouse_deg_per_count,
            (0.01, 2.0),
            d.mouse_deg_per_count,
        );
        let eye_height = clamp(
            n,
            "camera.eye_height",
            self.camera.eye_height.or(w.eye_height),
            (1.2, 1.8),
            d.eye_height,
        );
        let pitch_max_deg = clamp(
            n,
            "camera.pitch_max_deg",
            self.camera.pitch_max_deg.or(w.pitch_max_deg),
            (10.0, 85.0),
            d.pitch_max_deg,
        );
        let c = WalkConfig {
            speed,
            run_mul,
            turn: w.turn.unwrap_or(d.turn),
            turn_rate_dps,
            snap_deg,
            pitch_max_deg,
            mouse_look: w.mouse_look.unwrap_or(d.mouse_look),
            mouse_deg_per_count: mouse,
            eye_height,
            comfort_vignette: w.comfort_vignette.unwrap_or(d.comfort_vignette),
            on_exit: w.on_exit.unwrap_or(d.on_exit),
        };
        (c, notes)
    }
}

impl Config {
    /// Under reduced motion the turn snaps unless `[walk] turn` says
    /// otherwise (§3.11).
    pub fn reduced_motion(&mut self) {
        if !self.walk_turn_set {
            self.walk.turn = TurnMode::Snap;
        }
    }
}

/// The terminal `world.terminal` runs without `[launch] terminal`.
pub const DEFAULT_TERMINAL: &str = "foot";

impl Config {
    pub fn terminal(&self) -> Vec<String> {
        self.launch
            .terminal
            .clone()
            .unwrap_or_else(|| vec![DEFAULT_TERMINAL.to_string()])
    }
}

/// Where a new window of `app_id` goes, field by field: an explicit rule
/// (the first that matches) > the place memory > the default (`None`).
pub fn place_for(rules: &[Rule], memory: &BTreeMap<String, Place>, app_id: &str) -> Option<Place> {
    let rule = rules
        .iter()
        .find(|r| r.matches(app_id))
        .map(Rule::place)
        .unwrap_or_default();
    let mem = memory.get(app_id).cloned().unwrap_or_default();
    let p = rule.or(mem);
    (!p.is_empty()).then_some(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOML: &str = r#"
[[rules]]
app_id = "acme.term"
room = "acme"
desk = "last"
width = "two_thirds"

[[rules]]
app_id = "demo.*"
desk = 0

[launch]
terminal = "foot -e tmux"

[keys]
focus_escape = "ctrl+g"
world_mod = "super"

[objects]
turntable = true
"#;

    #[test]
    fn parses_the_file() {
        let c = parse(TOML).unwrap();
        assert_eq!(c.rules.len(), 2);
        assert_eq!(c.rules[0].desk, Some(DeskAt::Last));
        assert_eq!(c.rules[0].width, Some(Width::TwoThirds));
        assert_eq!(c.rules[1].desk, Some(DeskAt::Index(0)));
        assert_eq!(c.terminal(), ["foot", "-e", "tmux"]);
        assert_eq!(c.keys.focus_escape.as_deref(), Some("ctrl+g"));
        assert_eq!(c.keys.world_mod.as_deref(), Some("super"));
        let c = parse("[launch]\nterminal = [\"alacritty\", \"-e\", \"sh\"]\n").unwrap();
        assert_eq!(c.terminal(), ["alacritty", "-e", "sh"]);
        assert_eq!(parse("").unwrap(), Config::default());
        assert_eq!(Config::default().terminal(), [DEFAULT_TERMINAL]);
    }

    #[test]
    fn walk_and_camera_tables() {
        let c = parse("").unwrap();
        assert_eq!(c.walk, WalkConfig::default());
        assert!(!c.walk_turn_set && c.notes.is_empty());
        let c = parse(
            "[walk]\nspeed = 3.5\nrun_mul = 1.5\nturn = \"snap\"\nsnap_deg = 45\nmouse_look = false\n\
             mouse_deg_per_count = 0.2\ncomfort_vignette = true\non_exit = \"stay\"\nturn_rate_dps = 90\n\
             run_key = \"shift\"\n\n[camera]\neye_height = 1.7\npitch_max_deg = 40\n",
        )
        .unwrap();
        let w = c.walk;
        assert_eq!(
            (w.speed, w.run_mul, w.snap_deg, w.turn_rate_dps),
            (3.5, 1.5, 45.0, 90.0)
        );
        assert_eq!(w.turn, TurnMode::Snap);
        assert!(c.walk_turn_set);
        assert!(!w.mouse_look && w.comfort_vignette);
        assert_eq!(w.mouse_deg_per_count, 0.2);
        assert_eq!(w.on_exit, ExitRule::Stay);
        assert_eq!((w.eye_height, w.pitch_max_deg), (1.7, 40.0));
        assert!(c.notes.is_empty(), "{:?}", c.notes);
        // out of range: clamped, with a note each
        let c = parse(
            "[walk]\nspeed = 9\nrun_mul = 0\nturn_rate_dps = 1000\nsnap_deg = 20\n\
             mouse_deg_per_count = 7\n[camera]\neye_height = 0.5\npitch_max_deg = 120\n",
        )
        .unwrap();
        let w = c.walk;
        assert_eq!((w.speed, w.run_mul, w.turn_rate_dps), (5.0, 1.0, 240.0));
        assert_eq!(w.snap_deg, 15.0, "the nearest of 15/30/45");
        assert_eq!(w.mouse_deg_per_count, 2.0);
        assert_eq!((w.eye_height, w.pitch_max_deg), (1.2, 85.0));
        assert_eq!(c.notes.len(), 7, "{:?}", c.notes);
        assert!(
            c.notes[0].starts_with("world.toml: walk.speed"),
            "{:?}",
            c.notes
        );
        // a wrong type is an error like any other
        assert!(parse("[walk]\nspeed = \"rápido\"\n").is_err());
        assert!(parse("[walk]\non_exit = \"jump\"\n").is_err());
    }

    #[test]
    fn bad_files_say_why() {
        for bad in [
            "[[rules]]\nroom = \"acme\"\n", // no app_id
            "[[rules]]\napp_id = \"a\"\ndesk = \"first\"\n",
            "[[rules]]\napp_id = \"a\"\nwidth = \"wide\"\n",
            "[[rules]]\napp_id = \"a\"\ncolour = 1\n",
            "[launch]\nterminal = \"\"\n",
            "[launch]\nterminal = 3\n",
            "rules = 3",
            "[keys\n",
        ] {
            let e = parse(bad).unwrap_err();
            assert!(e.starts_with("world.toml:"), "{bad}: {e}");
        }
    }

    #[test]
    fn precedence_rule_then_memory_then_default() {
        let rules = parse(TOML).unwrap().rules;
        let mem = BTreeMap::from([
            (
                "acme.term".to_string(),
                Place {
                    room: Some("bravo".into()),
                    desk: Some(DeskAt::Index(3)),
                    width: Some(Width::Third),
                    air: None,
                },
            ),
            (
                "demo.viewer".to_string(),
                Place {
                    room: Some("bravo".into()),
                    desk: Some(DeskAt::Index(3)),
                    width: None,
                    air: None,
                },
            ),
            (
                "alpha".to_string(),
                Place {
                    room: Some("alpha".into()),
                    ..Place::default()
                },
            ),
        ]);
        let rows: [(&str, Option<Place>); 4] = [
            // the explicit rule wins every field it sets
            (
                "acme.term",
                Some(Place {
                    room: Some("acme".into()),
                    desk: Some(DeskAt::Last),
                    width: Some(Width::TwoThirds),
                    air: None,
                }),
            ),
            // a prefix rule with only a desk: the rest from memory
            (
                "demo.viewer",
                Some(Place {
                    room: Some("bravo".into()),
                    desk: Some(DeskAt::Index(0)),
                    width: None,
                    air: None,
                }),
            ),
            // memory only
            (
                "alpha",
                Some(Place {
                    room: Some("alpha".into()),
                    ..Place::default()
                }),
            ),
            // nothing: the default
            ("bravo", None),
        ];
        for (app, want) in rows {
            assert_eq!(place_for(&rules, &mem, app), want, "{app}");
        }
        assert!(rules[1].matches("demo."));
        assert!(!rules[1].matches("dem"));
        assert!(!rules[0].matches("acme.term2"));
    }
}
