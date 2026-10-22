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
//! ```
//!
//! Pure: the text is parsed here, the file is read by `nkw`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::persist::{DeskAt, Place};
use crate::strip::Width;

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

/// `world.toml`. Unknown top-level tables are kept for the other parts of
/// the world (`[objects]`, …) and ignored here.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub rules: Vec<Rule>,
    #[serde(default)]
    pub launch: Launch,
    #[serde(default)]
    pub keys: Keys,
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
    toml::from_str(text).map_err(|e| format!("world.toml: {e}"))
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
                },
            ),
            (
                "demo.viewer".to_string(),
                Place {
                    room: Some("bravo".into()),
                    desk: Some(DeskAt::Index(3)),
                    width: None,
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
                }),
            ),
            // a prefix rule with only a desk: the rest from memory
            (
                "demo.viewer",
                Some(Place {
                    room: Some("bravo".into()),
                    desk: Some(DeskAt::Index(0)),
                    width: None,
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
