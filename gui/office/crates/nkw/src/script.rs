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
//!
//! Wave 5 (S29) adds `pointer_to <x> <y>` (absolute, logical px), `middle`
//! buttons, `click`/`dclick`/`drag` (expanded here into the primitive lines,
//! modifiers through the seat keyboard), `scroll <steps>`, `expect near` and
//! `expect pose`, and the mode `ARRUMAR`.

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
    /// `env K=V`: every later `spawn` of the script carries it (S20)
    Env(String, String),
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
    /// an absolute pointer motion, logical px (`PointerMotionAbsolute`)
    PointerTo(f64, f64),
    /// a vertical wheel of `steps` notches, `+` = away (v120 = steps·120)
    Scroll(i32),
    /// the number at `pointer` is within `tol` of `value`
    ExpectNear {
        pointer: String,
        value: f64,
        tol: f64,
    },
    /// the first screen with `app_id`: its `pose.pos_mm` within `tol_mm`
    /// and, when given, `yaw_mdeg` within 500
    ExpectPose {
        app_id: String,
        pos_mm: [f64; 3],
        yaw_mdeg: Option<f64>,
        tol_mm: f64,
    },
    Quit,
}

/// `button left|middle|right`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointerButton {
    Left,
    Middle,
    Right,
}

impl PointerButton {
    /// The evdev code (`BTN_LEFT`, `BTN_RIGHT`, `BTN_MIDDLE`).
    pub fn code(self) -> u32 {
        match self {
            PointerButton::Left => 0x110,
            PointerButton::Right => 0x111,
            PointerButton::Middle => 0x112,
        }
    }

    fn parse(s: &str) -> Option<PointerButton> {
        match s {
            "left" => Some(PointerButton::Left),
            "middle" => Some(PointerButton::Middle),
            "right" => Some(PointerButton::Right),
            _ => None,
        }
    }
}

/// `dclick`: the two clicks are this far apart.
pub const DCLICK_GAP_MS: u64 = 80;
/// `drag`: the default duration and the number of motions.
pub const DRAG_MS: u64 = 200;
pub const DRAG_STEPS: u32 = 10;
/// `expect pose`: the default position tolerance and the yaw one.
pub const POSE_TOL_MM: f64 = 5.0;
pub const POSE_YAW_TOL_MDEG: f64 = 500.0;

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
pub const MODES: &[&str] = &[
    "NAV", "ANDAR", "FOCO", "CONVERSA", "SALTO", "COMANDOS", "ARRUMAR",
];

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

/// `ctrl`, `shift`, `alt`, `world`, joined by `+` (the pointer verbs).
pub fn parse_mods(s: &str) -> Result<Mods, String> {
    let mut m = Mods::default();
    for w in s.split('+') {
        match w {
            "ctrl" => m.ctrl = true,
            "shift" => m.shift = true,
            "alt" => m.alt = true,
            "world" => m.world = true,
            _ => return Err(format!("unknown modifier {w:?} in {s:?}")),
        }
    }
    Ok(m)
}

/// The modifier keys of `m` (keysym names) in press order: `world` is
/// `Control_L` + `Alt_L` nested.
fn mod_keys(m: Mods) -> Vec<&'static str> {
    let mut v = Vec::new();
    if m.ctrl || m.world {
        v.push("Control_L");
    }
    if m.alt || m.world {
        v.push("Alt_L");
    }
    if m.shift {
        v.push("Shift_L");
    }
    v
}

fn bare(key: &str) -> Chord {
    Chord {
        mods: Mods::default(),
        key: key.to_string(),
    }
}

fn mods_down(m: Mods) -> Vec<Cmd> {
    mod_keys(m).into_iter().map(|k| Cmd::Down(bare(k))).collect()
}

fn mods_up(m: Mods) -> Vec<Cmd> {
    mod_keys(m)
        .into_iter()
        .rev()
        .map(|k| Cmd::Up(bare(k)))
        .collect()
}

/// `click` / `dclick`: modifiers down, press+release (twice, 80 ms apart),
/// modifiers up.
fn click(b: PointerButton, m: Mods, double: bool) -> Vec<Cmd> {
    let mut v = mods_down(m);
    v.extend([Cmd::Button(b, true), Cmd::Button(b, false)]);
    if double {
        v.extend([
            Cmd::Wait(DCLICK_GAP_MS),
            Cmd::Button(b, true),
            Cmd::Button(b, false),
        ]);
    }
    v.extend(mods_up(m));
    v
}

/// `drag`: `pointer_to` the start, modifiers down, press, ten motions over
/// `ms`, release, modifiers up.
fn drag(from: (f64, f64), to: (f64, f64), ms: u64, m: Mods) -> Vec<Cmd> {
    let mut v = vec![Cmd::PointerTo(from.0, from.1)];
    v.extend(mods_down(m));
    v.push(Cmd::Button(PointerButton::Left, true));
    let step = ms / DRAG_STEPS as u64;
    for i in 1..=DRAG_STEPS {
        let t = i as f64 / DRAG_STEPS as f64;
        if step > 0 {
            v.push(Cmd::Wait(step));
        }
        v.push(Cmd::PointerTo(
            from.0 + (to.0 - from.0) * t,
            from.1 + (to.1 - from.1) * t,
        ));
    }
    v.push(Cmd::Button(PointerButton::Left, false));
    v.extend(mods_up(m));
    v
}

fn number(v: &str, what: &str) -> Result<f64, String> {
    v.parse::<f64>()
        .ok()
        .filter(|x| x.is_finite())
        .ok_or_else(|| format!("bad {what} {v:?}"))
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
pub const PLUGINS: [&str; 5] = ["wm", "overlay", "cats", "objects", "pointer"];

fn one<'a>(args: &'a [String], what: &str) -> Result<&'a str, String> {
    match args {
        [a] => Ok(a),
        _ => Err(format!("{what} takes exactly one argument")),
    }
}

fn parse_line(line: &str) -> Result<Vec<Cmd>, String> {
    let t = line.trim();
    if t.is_empty() || t.starts_with('#') {
        return Ok(Vec::new());
    }
    let (head, tail) = t.split_once(char::is_whitespace).unwrap_or((t, ""));
    // the JSON commands take the rest of the line verbatim
    match (head, first_word(tail)) {
        ("expect", ("json", rest)) => {
            let (pointer, value) = pointer_json(rest, "expect json")?;
            return Ok(vec![Cmd::ExpectJson { pointer, value }]);
        }
        ("wait", ("json", rest)) => {
            let (body, ms) = rest
                .trim_end()
                .rsplit_once(char::is_whitespace)
                .ok_or("wait json <pointer> <json> <ms>")?;
            let ms = ms.parse().map_err(|_| format!("bad wait {ms:?} (ms)"))?;
            let (pointer, value) = pointer_json(body, "wait json")?;
            return Ok(vec![Cmd::WaitJson { pointer, value, ms }]);
        }
        ("inject", (plugin, rest)) => {
            if !PLUGINS.contains(&plugin) {
                return Err(format!(
                    "inject: unknown plugin {plugin:?} (wm overlay cats objects pointer)"
                ));
            }
            return Ok(vec![Cmd::Inject {
                plugin: plugin.to_string(),
                json: json(rest)?,
            }]);
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
        "env" => {
            let kv = match args.as_slice() {
                [kv] => kv.split_once('='),
                _ => None,
            };
            match kv {
                Some((k, v)) if !k.is_empty() => Cmd::Env(k.into(), v.into()),
                _ => return Err("env K=V (one variable per line)".into()),
            }
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
            [what, p, v, tol] if what == "near" => Cmd::ExpectNear {
                pointer: pointer(p)?,
                value: number(v, "number")?,
                tol: number(tol, "tolerance")?,
            },
            [what, app, rest @ ..] if what == "pose" => expect_pose(app, rest)?,
            _ => {
                return Err("expect mode <M> | expect focus <app_id> | expect json <pointer> \
                     <json> | expect near <pointer> <n> <tol> | expect pose <app_id> <x> <y> \
                     <z> [<yaw>] [<tol>]"
                    .into());
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
        "pointer_to" => match args.as_slice() {
            [x, y] => Cmd::PointerTo(
                number(x, "pointer position")?,
                number(y, "pointer position")?,
            ),
            _ => return Err("pointer_to <x> <y>".into()),
        },
        "scroll" => match args.as_slice() {
            [n] => Cmd::Scroll(
                n.trim_start_matches('+')
                    .parse()
                    .map_err(|_| format!("bad scroll {n:?} (steps)"))?,
            ),
            _ => return Err("scroll <steps>".into()),
        },
        "click" | "dclick" => {
            const USE: &str = "click left|middle|right [mods] | dclick left [mods]";
            let b = args
                .first()
                .and_then(|b| PointerButton::parse(b))
                .ok_or(USE)?;
            let m = match args.get(1) {
                Some(m) => parse_mods(m)?,
                None => Mods::default(),
            };
            if args.len() > 2 || (head == "dclick" && b != PointerButton::Left) {
                return Err(USE.into());
            }
            return Ok(click(b, m, head == "dclick"));
        }
        "drag" => {
            const USE: &str = "drag <x0> <y0> <x1> <y1> [ms] [mods]";
            if !(4..=6).contains(&args.len()) {
                return Err(USE.into());
            }
            let n: Vec<f64> = args[..4]
                .iter()
                .map(|v| number(v, "drag position"))
                .collect::<Result<_, _>>()?;
            let (mut ms, mut m) = (DRAG_MS, Mods::default());
            for a in &args[4..] {
                let a = a.strip_prefix("ms=").unwrap_or(a);
                if let Ok(v) = a.parse::<u64>() {
                    ms = v;
                } else {
                    m = parse_mods(a)?;
                }
            }
            return Ok(drag((n[0], n[1]), (n[2], n[3]), ms, m));
        }
        "button" => {
            const USE: &str = "button left|middle|right down|up";
            let b = args
                .first()
                .and_then(|b| PointerButton::parse(b))
                .ok_or(USE)?;
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
    Ok(vec![cmd])
}

/// `expect pose <app_id> <x_mm> <y_mm> <z_mm> [<yaw_mdeg>] [<tol_mm>]`
/// (also `yaw=N`, `tol=N`).
fn expect_pose(app: &str, rest: &[String]) -> Result<Cmd, String> {
    const USE: &str = "expect pose <app_id> <x_mm> <y_mm> <z_mm> [<yaw_mdeg>] [<tol_mm>]";
    if !(3..=5).contains(&rest.len()) {
        return Err(USE.into());
    }
    let mut pos = [0.0; 3];
    for (p, v) in pos.iter_mut().zip(rest) {
        *p = number(v, "pose mm")?;
    }
    let (mut yaw, mut tol) = (None, POSE_TOL_MM);
    for (i, a) in rest[3..].iter().enumerate() {
        if let Some(v) = a.strip_prefix("yaw=") {
            yaw = Some(number(v, "pose yaw")?);
        } else if let Some(v) = a.strip_prefix("tol=") {
            tol = number(v, "pose tolerance")?;
        } else if i == 0 {
            yaw = Some(number(a, "pose yaw")?);
        } else {
            tol = number(a, "pose tolerance")?;
        }
    }
    Ok(Cmd::ExpectPose {
        app_id: app.to_string(),
        pos_mm: pos,
        yaw_mdeg: yaw,
        tol_mm: tol,
    })
}

/// `expect near`: the number at `pointer` within `tol` of `want`.
pub fn near_check(doc: &Value, pointer: &str, want: f64, tol: f64) -> Result<(), String> {
    match doc.pointer(pointer).map(|v| (v, v.as_f64())) {
        Some((_, Some(n))) if (n - want).abs() <= tol => Ok(()),
        Some((v, _)) => Err(format!(
            "expected near {pointer} = {want} ± {tol}, actual {v}"
        )),
        None => Err(format!(
            "expected near {pointer} = {want} ± {tol}, actual <missing>"
        )),
    }
}

/// `expect pose`: the first screen with `app_id` in `doc["screens"]`, its
/// `pose.pos_mm` (and `yaw_mdeg`) against the wanted ones.
pub fn pose_check(
    doc: &Value,
    app_id: &str,
    pos_mm: [f64; 3],
    yaw_mdeg: Option<f64>,
    tol_mm: f64,
) -> Result<(), String> {
    let screens = doc["screens"].as_array().map(Vec::as_slice).unwrap_or(&[]);
    let Some(s) = screens.iter().find(|s| s["app_id"] == app_id) else {
        return Err(format!("expected pose of {app_id}: no such screen"));
    };
    let pose = &s["pose"];
    let got: Option<Vec<f64>> = pose["pos_mm"]
        .as_array()
        .map(|a| a.iter().filter_map(Value::as_f64).collect());
    let Some(got) = got.filter(|g| g.len() == 3) else {
        return Err(format!("expected pose of {app_id}: no pose in the dump"));
    };
    let far = got.iter().zip(pos_mm).any(|(g, w)| (g - w).abs() > tol_mm);
    let yaw = pose["yaw_mdeg"].as_f64();
    let yaw_off = match (yaw_mdeg, yaw) {
        (Some(w), Some(g)) => (g - w).abs() > POSE_YAW_TOL_MDEG,
        (Some(_), None) => true,
        (None, _) => false,
    };
    if far || yaw_off {
        return Err(format!(
            "expected pose of {app_id} {pos_mm:?} ± {tol_mm} mm{}, actual {got:?} yaw {}",
            yaw_mdeg.map(|y| format!(" yaw {y}")).unwrap_or_default(),
            yaw.map(|y| y.to_string()).unwrap_or_else(|| "?".into()),
        ));
    }
    Ok(())
}

pub fn parse(src: &str) -> Result<Vec<Line>, ScriptError> {
    let mut out = Vec::new();
    for (i, line) in src.lines().enumerate() {
        match parse_line(line) {
            Ok(cmds) => out.extend(cmds.into_iter().map(|cmd| Line { no: i + 1, cmd })),
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
env GDK_BACKEND=wayland
env "NKW_T=a b=c"
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
                Cmd::Env("GDK_BACKEND".into(), "wayland".into()),
                Cmd::Env("NKW_T".into(), "a b=c".into()),
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
            ("env", 1, "env K=V"),
            ("env FOO", 1, "env K=V"),
            ("env =x", 1, "env K=V"),
            ("env A=1 B=2", 1, "env K=V"),
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
            ("button up down", 1, "button left|middle|right"),
            ("button left", 1, "button left|middle|right"),
            ("button left sideways", 1, "button left|middle|right"),
            ("button left down now", 1, "button left|middle|right"),
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
        assert_eq!(PointerButton::Middle.code(), 0x112);
    }

    fn cmds(src: &str) -> Vec<Cmd> {
        parse(src).unwrap().into_iter().map(|l| l.cmd).collect()
    }

    fn down(k: &str) -> Cmd {
        Cmd::Down(chord(NONE, k))
    }

    fn up(k: &str) -> Cmd {
        Cmd::Up(chord(NONE, k))
    }

    #[test]
    fn pointer_verbs_s29() {
        use PointerButton::*;
        assert_eq!(cmds("pointer_to 640 400.5"), [Cmd::PointerTo(640.0, 400.5)]);
        assert_eq!(cmds("button middle down"), [Cmd::Button(Middle, true)]);
        assert_eq!(cmds("scroll 2"), [Cmd::Scroll(2)]);
        assert_eq!(cmds("scroll +3"), [Cmd::Scroll(3)]);
        assert_eq!(cmds("scroll -1"), [Cmd::Scroll(-1)]);
        assert_eq!(
            cmds("click right"),
            [Cmd::Button(Right, true), Cmd::Button(Right, false)]
        );
        assert_eq!(
            cmds("click left ctrl+shift"),
            [
                down("Control_L"),
                down("Shift_L"),
                Cmd::Button(Left, true),
                Cmd::Button(Left, false),
                up("Shift_L"),
                up("Control_L"),
            ]
        );
        assert_eq!(
            cmds("dclick left"),
            [
                Cmd::Button(Left, true),
                Cmd::Button(Left, false),
                Cmd::Wait(DCLICK_GAP_MS),
                Cmd::Button(Left, true),
                Cmd::Button(Left, false),
            ]
        );
        // world = Control_L + Alt_L nested
        let c = cmds("click left world");
        assert_eq!(&c[..2], [down("Control_L"), down("Alt_L")]);
        assert_eq!(&c[4..], [up("Alt_L"), up("Control_L")]);
        assert_eq!(cmds("expect mode ARRUMAR"), [Cmd::ExpectMode("ARRUMAR".into())]);
    }

    #[test]
    fn drag_expands_to_ten_motions() {
        let c = cmds("drag 100 100 200 300");
        assert_eq!(c[0], Cmd::PointerTo(100.0, 100.0));
        assert_eq!(c[1], Cmd::Button(PointerButton::Left, true));
        let motions: Vec<_> = c
            .iter()
            .filter_map(|c| match c {
                Cmd::PointerTo(x, y) => Some((*x, *y)),
                _ => None,
            })
            .collect();
        assert_eq!(motions.len(), 1 + DRAG_STEPS as usize);
        assert_eq!(*motions.last().unwrap(), (200.0, 300.0));
        assert_eq!(motions[5], (150.0, 200.0));
        let waits: u64 = c
            .iter()
            .filter_map(|c| match c {
                Cmd::Wait(ms) => Some(*ms),
                _ => None,
            })
            .sum();
        assert_eq!(waits, DRAG_MS);
        assert_eq!(*c.last().unwrap(), Cmd::Button(PointerButton::Left, false));
        // ms and mods in either form and order; lines keep their number
        let c = cmds("drag 0 0 10 10 ms=500 ctrl");
        assert_eq!(c[1], down("Control_L"));
        assert_eq!(*c.last().unwrap(), up("Control_L"));
        assert!(c.contains(&Cmd::Wait(50)));
        let c = cmds("drag 0 0 10 10 world 0");
        assert!(!c.iter().any(|c| matches!(c, Cmd::Wait(_))));
        assert_eq!(&c[1..3], [down("Control_L"), down("Alt_L")]);
        let l = parse("\ndrag 0 0 1 1").unwrap();
        assert!(l.iter().all(|l| l.no == 2));
    }

    #[test]
    fn expect_near_and_pose() {
        assert_eq!(
            cmds("expect near /camera/pos/0 1.5 0.01"),
            [Cmd::ExpectNear {
                pointer: "/camera/pos/0".into(),
                value: 1.5,
                tol: 0.01
            }]
        );
        let pose = |yaw, tol| Cmd::ExpectPose {
            app_id: "acme.1".into(),
            pos_mm: [10.0, 1200.0, -800.0],
            yaw_mdeg: yaw,
            tol_mm: tol,
        };
        assert_eq!(
            cmds("expect pose acme.1 10 1200 -800"),
            [pose(None, POSE_TOL_MM)]
        );
        assert_eq!(
            cmds("expect pose acme.1 10 1200 -800 -15000"),
            [pose(Some(-15000.0), POSE_TOL_MM)]
        );
        assert_eq!(
            cmds("expect pose acme.1 10 1200 -800 0 30"),
            [pose(Some(0.0), 30.0)]
        );
        assert_eq!(
            cmds("expect pose acme.1 10 1200 -800 tol=30"),
            [pose(None, 30.0)]
        );
        let doc = serde_json::json!({"n": 1.004, "s": "x", "screens": [
            {"app_id": "foot", "pose": {"pos_mm": [0, 0, 0], "yaw_mdeg": 0}},
            {"app_id": "acme.1", "pose": {"pos_mm": [12, 1197, -800], "yaw_mdeg": -14800}},
            {"app_id": "acme.1", "pose": {"pos_mm": [0, 0, 0], "yaw_mdeg": 0}},
        ]});
        assert!(near_check(&doc, "/n", 1.0, 0.01).is_ok());
        assert!(near_check(&doc, "/n", 1.0, 0.001).unwrap_err().contains("actual 1.004"));
        assert!(near_check(&doc, "/s", 1.0, 1.0).is_err());
        assert!(near_check(&doc, "/nope", 1.0, 1.0).unwrap_err().contains("<missing>"));
        let p = [10.0, 1200.0, -800.0];
        assert!(pose_check(&doc, "acme.1", p, None, 5.0).is_ok());
        assert!(pose_check(&doc, "acme.1", p, Some(-15000.0), 5.0).is_ok());
        assert!(pose_check(&doc, "acme.1", p, Some(-16000.0), 5.0).is_err());
        assert!(pose_check(&doc, "acme.1", p, None, 1.0).is_err());
        assert!(pose_check(&doc, "mpv", p, None, 5.0).unwrap_err().contains("no such"));
        let bare = serde_json::json!({"screens": [{"app_id": "foot"}]});
        assert!(pose_check(&bare, "foot", p, None, 5.0).unwrap_err().contains("no pose"));
    }

    #[test]
    fn pointer_verb_errors() {
        let cases = [
            ("pointer_to 1", "pointer_to <x> <y>"),
            ("pointer_to a 1", "bad pointer position"),
            ("scroll", "scroll <steps>"),
            ("scroll up", "bad scroll"),
            ("click", "click left|middle|right"),
            ("click up", "click left|middle|right"),
            ("click left hyper", "unknown modifier"),
            ("click left ctrl extra", "click left|middle|right"),
            ("dclick right", "dclick left"),
            ("drag 1 2 3", "drag <x0>"),
            ("drag 1 2 3 x", "bad drag position"),
            ("drag 1 2 3 4 5 ctrl 7", "drag <x0>"),
            ("drag 1 2 3 4 slow", "unknown modifier"),
            ("expect near /a 1", "expect mode"),
            ("expect near a 1 2", "bad pointer"),
            ("expect near /a x 2", "bad number"),
            ("expect near /a 1 x", "bad tolerance"),
            ("expect pose foot 1 2", "expect pose <app_id>"),
            ("expect pose foot 1 2 3 4 5 6", "expect pose <app_id>"),
            ("expect pose foot 1 2 z", "bad pose mm"),
            ("expect pose foot 1 2 3 yaw=x", "bad pose yaw"),
            ("expect pose foot 1 2 3 0 x", "bad pose tolerance"),
        ];
        for (src, want) in cases {
            let e = parse(src).unwrap_err();
            assert!(e.msg.contains(want), "{src:?} → {e}");
        }
    }

    #[test]
    fn mode_names_follow_the_hud() {
        assert_eq!(mode_name("NAVEGAR"), "NAV");
        assert_eq!(mode_name("FOCO"), "FOCO");
    }
}
