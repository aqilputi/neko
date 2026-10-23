//! `--script`: synthetic input and checks, one command per line, `#`
//! comments (contract S2, S12). Keys are xkb keysym names (or a single
//! character), with modifiers `ctrl+ shift+ alt+ world+`.
//!
//! S12 adds `expect json <pointer> <json>` and `wait json <pointer> <json>
//! <ms>` (RFC 6901 pointers into `world.dump()`) and `inject <plugin>
//! <json>`; the JSON is the rest of the line, verbatim.
//!
//! Wave 3 (S14) adds `pointer <dx> <dy>` (a relative motion, as the host's
//! relative pointer delivers it) and `button left|right down|up`.

use std::path::PathBuf;

use serde_json::Value;

/// Modifiers of a chord. `world` resolves to `ctrl+alt` nested.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Mods {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub world: bool,
}

/// A key with its modifiers, e.g. `ctrl+bracketright`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chord {
    pub mods: Mods,
    /// keysym name (`h`, `Return`, `bracketright`) or one character
    pub key: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Cmd {
    Key(Chord),
    Down(Chord),
    Up(Chord),
    Text(String),
    Wait(u64),
    Spawn(Vec<String>),
    Shot(PathBuf),
    Dump(PathBuf),
    ExpectMode(String),
    ExpectFocus(String),
    /// the dump's value at `pointer` equals `value`
    ExpectJson {
        pointer: String,
        value: Value,
    },
    /// poll each frame until the dump's value at `pointer` equals `value`;
    /// after `ms`, fail
    WaitJson {
        pointer: String,
        value: Value,
        ms: u64,
    },
    /// `ext.<plugin>.inject(json)`
    Inject {
        plugin: String,
        json: Value,
    },
    /// a relative pointer motion (counts), through `process_input`
    Pointer(f64, f64),
    /// a pointer button press (`true`) or release
    Button(PointerButton, bool),
    Quit,
}

/// `button left|right`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointerButton {
    Left,
    Right,
}

impl PointerButton {
    /// The evdev code (`BTN_LEFT`, `BTN_RIGHT`).
    pub fn code(self) -> u32 {
        match self {
            PointerButton::Left => 0x110,
            PointerButton::Right => 0x111,
        }
    }
}

/// A command and the line it came from (1-based), for error messages.
#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    pub no: usize,
    pub cmd: Cmd,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ScriptError {
    pub line: usize,
    pub msg: String,
}

impl std::fmt::Display for ScriptError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "script line {}: {}", self.line, self.msg)
    }
}

/// The mode names `expect mode` accepts.
pub const MODES: &[&str] = &["NAV", "ANDAR", "FOCO", "CONVERSA", "SALTO", "COMANDOS"];

/// `expect mode` name for a `Mode::label()` (the HUD says NAVEGAR).
pub fn mode_name(label: &str) -> &str {
    if label == "NAVEGAR" { "NAV" } else { label }
}

pub fn parse_chord(s: &str) -> Result<Chord, String> {
    let mut mods = Mods::default();
    let mut rest = s;
    // a trailing "+" is the key itself (`ctrl++` is ctrl and plus)
    while let Some((m, tail)) = rest.split_once('+') {
        if tail.is_empty() {
            break;
        }
        match m {
            "ctrl" => mods.ctrl = true,
            "shift" => mods.shift = true,
            "alt" => mods.alt = true,
            "world" => mods.world = true,
            _ => return Err(format!("unknown modifier {m:?} in {s:?}")),
        }
        rest = tail;
    }
    if rest.is_empty() {
        return Err(format!("empty key in {s:?}"));
    }
    Ok(Chord {
        mods,
        key: rest.to_string(),
    })
}

/// Split a line into words; double quotes group, `\"` `\\` `\n` `\t`
/// escape inside them.
fn words(s: &str) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_word = false;
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                in_word = true;
                loop {
                    match chars.next() {
                        None => return Err("unterminated quote".into()),
                        Some('"') => break,
                        Some('\\') => match chars.next() {
                            Some('n') => cur.push('\n'),
                            Some('t') => cur.push('\t'),
                            Some(e @ ('"' | '\\')) => cur.push(e),
                            Some(e) => return Err(format!("bad escape \\{e}")),
                            None => return Err("unterminated quote".into()),
                        },
                        Some(c) => cur.push(c),
                    }
                }
            }
            c if c.is_whitespace() => {
                if in_word {
                    out.push(std::mem::take(&mut cur));
                    in_word = false;
                }
            }
            c => {
                in_word = true;
                cur.push(c);
            }
        }
    }
    if in_word {
        out.push(cur);
    }
    Ok(out)
}

/// Split off the first whitespace-separated word of `s`.
fn first_word(s: &str) -> (&str, &str) {
    let s = s.trim();
    s.split_once(char::is_whitespace)
        .map_or((s, ""), |(a, b)| (a, b.trim_start()))
}

fn pointer(p: &str) -> Result<String, String> {
    if p.starts_with('/') {
        Ok(p.to_string())
    } else {
        Err(format!("bad pointer {p:?} (RFC 6901: starts with /)"))
    }
}

fn json(src: &str) -> Result<Value, String> {
    if src.trim().is_empty() {
        return Err("needs a JSON value".into());
    }
    serde_json::from_str(src.trim()).map_err(|e| format!("bad JSON {src:?}: {e}"))
}

/// `<pointer> <json>`
fn pointer_json(tail: &str, what: &str) -> Result<(String, Value), String> {
    let (p, rest) = first_word(tail);
    if p.is_empty() {
        return Err(format!("{what} <pointer> <json>"));
    }
    Ok((pointer(p)?, json(rest)?))
}

/// Is the dump's value at `pointer` `want`? `Err`: the message of an
/// `expect json` / `wait json` that failed.
pub fn json_check(doc: &Value, pointer: &str, want: &Value) -> Result<(), String> {
    match doc.pointer(pointer) {
        Some(v) if v == want => Ok(()),
        Some(v) => Err(format!("expected json {pointer} = {want}, actual {v}")),
        None => Err(format!(
            "expected json {pointer} = {want}, actual <missing>"
        )),
    }
}

/// The plugins `inject` takes.
pub const PLUGINS: [&str; 4] = ["wm", "overlay", "cats", "objects"];

fn one<'a>(args: &'a [String], what: &str) -> Result<&'a str, String> {
    match args {
        [a] => Ok(a),
        _ => Err(format!("{what} takes exactly one argument")),
    }
}

fn parse_line(line: &str) -> Result<Option<Cmd>, String> {
    let t = line.trim();
    if t.is_empty() || t.starts_with('#') {
        return Ok(None);
    }
    let (head, tail) = t.split_once(char::is_whitespace).unwrap_or((t, ""));
    // the JSON commands take the rest of the line verbatim
    match (head, first_word(tail)) {
        ("expect", ("json", rest)) => {
            let (pointer, value) = pointer_json(rest, "expect json")?;
            return Ok(Some(Cmd::ExpectJson { pointer, value }));
        }
        ("wait", ("json", rest)) => {
            let (body, ms) = rest
                .trim_end()
                .rsplit_once(char::is_whitespace)
                .ok_or("wait json <pointer> <json> <ms>")?;
            let ms = ms.parse().map_err(|_| format!("bad wait {ms:?} (ms)"))?;
            let (pointer, value) = pointer_json(body, "wait json")?;
            return Ok(Some(Cmd::WaitJson { pointer, value, ms }));
        }
        ("inject", (plugin, rest)) => {
            if !PLUGINS.contains(&plugin) {
                return Err(format!(
                    "inject: unknown plugin {plugin:?} (wm overlay cats objects)"
                ));
            }
            return Ok(Some(Cmd::Inject {
                plugin: plugin.to_string(),
                json: json(rest)?,
            }));
        }
        _ => {}
    }
    let args = words(tail)?;
    let cmd = match head {
        "key" => Cmd::Key(parse_chord(one(&args, "key")?)?),
        "down" => Cmd::Down(parse_chord(one(&args, "down")?)?),
        "up" => Cmd::Up(parse_chord(one(&args, "up")?)?),
        "text" => {
            if !tail.trim_start().starts_with('"') {
                return Err("text needs a quoted string".into());
            }
            Cmd::Text(one(&args, "text")?.to_string())
        }
        "wait" => {
            let v = one(&args, "wait")?;
            Cmd::Wait(v.parse().map_err(|_| format!("bad wait {v:?} (ms)"))?)
        }
        "spawn" => {
            if args.is_empty() {
                return Err("spawn needs a command".into());
            }
            Cmd::Spawn(args)
        }
        "shot" => Cmd::Shot(one(&args, "shot")?.into()),
        "dump" => Cmd::Dump(one(&args, "dump")?.into()),
        "expect" => match args.as_slice() {
            [what, v] if what == "mode" => {
                if !MODES.contains(&v.as_str()) {
                    return Err(format!("unknown mode {v:?}"));
                }
                Cmd::ExpectMode(v.clone())
            }
            [what, v] if what == "focus" => Cmd::ExpectFocus(v.clone()),
            _ => {
                return Err(
                    "expect mode <M> | expect focus <app_id> | expect json <pointer> <json>".into(),
                );
            }
        },
        "pointer" => match args.as_slice() {
            [dx, dy] => {
                let n = |v: &str| {
                    v.parse::<f64>()
                        .ok()
                        .filter(|x| x.is_finite())
                        .ok_or_else(|| format!("bad pointer delta {v:?}"))
                };
                Cmd::Pointer(n(dx)?, n(dy)?)
            }
            _ => return Err("pointer <dx> <dy>".into()),
        },
        "button" => {
            const USE: &str = "button left|right down|up";
            let b = match args.first().map(String::as_str) {
                Some("left") => PointerButton::Left,
                Some("right") => PointerButton::Right,
                _ => return Err(USE.into()),
            };
            let down = match args.get(1).map(String::as_str) {
                Some("down") => true,
                Some("up") => false,
                _ => return Err(USE.into()),
            };
            if args.len() != 2 {
                return Err(USE.into());
            }
            Cmd::Button(b, down)
        }
        "quit" => {
            if !args.is_empty() {
                return Err("quit takes no arguments".into());
            }
            Cmd::Quit
        }
        other => return Err(format!("unknown command {other:?}")),
    };
    Ok(Some(cmd))
}

pub fn parse(src: &str) -> Result<Vec<Line>, ScriptError> {
    let mut out = Vec::new();
    for (i, line) in src.lines().enumerate() {
        match parse_line(line) {
            Ok(Some(cmd)) => out.push(Line { no: i + 1, cmd }),
            Ok(None) => {}
            Err(msg) => return Err(ScriptError { line: i + 1, msg }),
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chord(mods: Mods, key: &str) -> Chord {
        Chord {
            mods,
            key: key.into(),
        }
    }

    const NONE: Mods = Mods {
        ctrl: false,
        shift: false,
        alt: false,
        world: false,
    };
    const CTRL: Mods = Mods { ctrl: true, ..NONE };

    #[test]
    fn every_command() {
        let src = r#"
# a comment
key h
key ctrl+bracketright
down shift+a
up shift+a
text "olá \"mundo\"\n"
wait 250
spawn testclient --checker --log "/tmp/a b.json"
shot out.png
dump world.json
expect mode FOCO
expect focus acme.demo
expect json /mode "FOCO"
expect json /ext/cats {"stub": true, "n": [1, 2]}
wait json /station/desk/desk 2 1500
inject cats {"postman": {"to": "alpha"}}
pointer 100 -2.5
button left down
button right up
   quit
"#;
        let got: Vec<Cmd> = parse(src).unwrap().into_iter().map(|l| l.cmd).collect();
        let shift = Mods {
            shift: true,
            ..NONE
        };
        assert_eq!(
            got,
            vec![
                Cmd::Key(chord(NONE, "h")),
                Cmd::Key(chord(CTRL, "bracketright")),
                Cmd::Down(chord(shift, "a")),
                Cmd::Up(chord(shift, "a")),
                Cmd::Text("olá \"mundo\"\n".into()),
                Cmd::Wait(250),
                Cmd::Spawn(vec![
                    "testclient".into(),
                    "--checker".into(),
                    "--log".into(),
                    "/tmp/a b.json".into()
                ]),
                Cmd::Shot("out.png".into()),
                Cmd::Dump("world.json".into()),
                Cmd::ExpectMode("FOCO".into()),
                Cmd::ExpectFocus("acme.demo".into()),
                Cmd::ExpectJson {
                    pointer: "/mode".into(),
                    value: serde_json::json!("FOCO"),
                },
                Cmd::ExpectJson {
                    pointer: "/ext/cats".into(),
                    value: serde_json::json!({"stub": true, "n": [1, 2]}),
                },
                Cmd::WaitJson {
                    pointer: "/station/desk/desk".into(),
                    value: serde_json::json!(2),
                    ms: 1500,
                },
                Cmd::Inject {
                    plugin: "cats".into(),
                    json: serde_json::json!({"postman": {"to": "alpha"}}),
                },
                Cmd::Pointer(100.0, -2.5),
                Cmd::Button(PointerButton::Left, true),
                Cmd::Button(PointerButton::Right, false),
                Cmd::Quit,
            ]
        );
    }

    #[test]
    fn json_checks_say_what_they_saw() {
        let doc = serde_json::json!({"mode": "NAV", "ext": {"wm": {"stub": true}}});
        assert!(json_check(&doc, "/mode", &serde_json::json!("NAV")).is_ok());
        assert!(json_check(&doc, "/ext/wm/stub", &serde_json::json!(true)).is_ok());
        assert_eq!(
            json_check(&doc, "/mode", &serde_json::json!("FOCO")).unwrap_err(),
            "expected json /mode = \"FOCO\", actual \"NAV\""
        );
        assert_eq!(
            json_check(&doc, "/nope", &serde_json::json!(1)).unwrap_err(),
            "expected json /nope = 1, actual <missing>"
        );
    }

    #[test]
    fn line_numbers_are_kept() {
        let l = parse("\n# x\nkey h\n\nwait 1\n").unwrap();
        assert_eq!(l.iter().map(|l| l.no).collect::<Vec<_>>(), [3, 5]);
    }

    #[test]
    fn chords() {
        let all = Mods {
            ctrl: true,
            shift: true,
            alt: true,
            world: true,
        };
        assert_eq!(
            parse_chord("ctrl+shift+alt+world+x").unwrap(),
            chord(all, "x")
        );
        assert_eq!(parse_chord("ctrl++").unwrap(), chord(CTRL, "+"));
        assert_eq!(parse_chord("+").unwrap(), chord(NONE, "+"));
        assert_eq!(parse_chord("Return").unwrap(), chord(NONE, "Return"));
        assert!(parse_chord("hyper+x").is_err());
    }

    #[test]
    fn errors_carry_line_numbers() {
        let cases = [
            ("key", 1, "exactly one"),
            ("\nkey a b", 2, "exactly one"),
            ("\n\nbogus", 3, "unknown command"),
            ("wait soon", 1, "bad wait"),
            ("text hello", 1, "quoted"),
            ("text \"open", 1, "unterminated"),
            ("text \"a\\q\"", 1, "bad escape"),
            ("spawn", 1, "needs a command"),
            ("expect mode WALK", 1, "unknown mode"),
            ("expect color red", 1, "expect mode"),
            ("expect json", 1, "expect json <pointer>"),
            ("expect json mode \"NAV\"", 1, "bad pointer"),
            ("expect json /mode", 1, "needs a JSON"),
            ("expect json /mode {nope", 1, "bad JSON"),
            ("wait json /mode \"NAV\"", 1, "bad wait"),
            ("wait json 5", 1, "wait json <pointer>"),
            ("wait json /a 1 soon", 1, "bad wait"),
            ("inject router {}", 1, "unknown plugin"),
            ("inject cats", 1, "needs a JSON"),
            ("inject cats [1,", 1, "bad JSON"),
            ("quit now", 1, "no arguments"),
            ("key hyper+x", 1, "unknown modifier"),
            ("pointer 1", 1, "pointer <dx> <dy>"),
            ("pointer 1 2 3", 1, "pointer <dx> <dy>"),
            ("pointer a 2", 1, "bad pointer delta"),
            ("pointer 1 inf", 1, "bad pointer delta"),
            ("button middle down", 1, "button left|right"),
            ("button left", 1, "button left|right"),
            ("button left sideways", 1, "button left|right"),
            ("button left down now", 1, "button left|right"),
        ];
        for (src, line, want) in cases {
            let e = parse(src).unwrap_err();
            assert_eq!(e.line, line, "{src:?}");
            assert!(e.msg.contains(want), "{src:?} → {e}");
            assert!(e.to_string().starts_with(&format!("script line {line}:")));
        }
    }

    #[test]
    fn button_codes_are_evdev() {
        assert_eq!(PointerButton::Left.code(), 0x110);
        assert_eq!(PointerButton::Right.code(), 0x111);
    }

    #[test]
    fn mode_names_follow_the_hud() {
        assert_eq!(mode_name("NAVEGAR"), "NAV");
        assert_eq!(mode_name("FOCO"), "FOCO");
    }
}
