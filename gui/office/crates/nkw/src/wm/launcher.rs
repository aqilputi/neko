//! The apps of the palette (`world+d`): `.desktop` files from
//! `$XDG_DATA_HOME/applications` and each `$XDG_DATA_DIRS/applications`
//! (desktop entry spec: the first file of an id wins), read once on the
//! first open. Under `--demo` a fixture instead (nothing from the system
//! shows in shots).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// One launchable app.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct App {
    /// the desktop file id (`org.example.App` for `org.example.App.desktop`)
    pub id: String,
    pub name: String,
    /// the command line, field codes stripped (`Terminal=true`: already
    /// wrapped in the terminal)
    pub argv: Vec<String>,
}

/// The fields of a `[Desktop Entry]` this launcher reads.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub exec: String,
    pub app: bool,
    pub no_display: bool,
    pub hidden: bool,
    pub terminal: bool,
    pub only_show_in: Vec<String>,
    pub not_show_in: Vec<String>,
}

/// Parse a `.desktop` file's text (only the `[Desktop Entry]` group; a
/// `Name[pt_BR]`/`Name[pt]` beats `Name`).
pub fn parse(text: &str) -> Option<Entry> {
    let mut e = Entry::default();
    let mut in_entry = false;
    let mut name_rank = 0u8;
    let mut typed = false;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
            continue;
        }
        if !in_entry {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let (k, v) = (k.trim(), v.trim());
        let yes = v == "true";
        let list = || {
            v.split(';')
                .filter(|s| !s.is_empty())
                .map(String::from)
                .collect::<Vec<_>>()
        };
        match k {
            "Type" => {
                typed = true;
                e.app = v == "Application";
            }
            "Name" if name_rank < 1 => {
                e.name = v.into();
                name_rank = 1;
            }
            "Name[pt]" if name_rank < 2 => {
                e.name = v.into();
                name_rank = 2;
            }
            "Name[pt_BR]" => {
                e.name = v.into();
                name_rank = 3;
            }
            "Exec" => e.exec = v.into(),
            "NoDisplay" => e.no_display = yes,
            "Hidden" => e.hidden = yes,
            "Terminal" => e.terminal = yes,
            "OnlyShowIn" => e.only_show_in = list(),
            "NotShowIn" => e.not_show_in = list(),
            _ => {}
        }
    }
    (typed && !e.name.is_empty()).then_some(e)
}

impl Entry {
    /// Whether the launcher lists it under the desktops `current`
    /// (`$XDG_CURRENT_DESKTOP`, `:`-separated).
    pub fn shown(&self, current: &[String]) -> bool {
        if !self.app || self.no_display || self.hidden || self.exec.is_empty() {
            return false;
        }
        if !self.only_show_in.is_empty() && !self.only_show_in.iter().any(|d| current.contains(d)) {
            return false;
        }
        !self.not_show_in.iter().any(|d| current.contains(d))
    }

    /// The argv to run: the `Exec` line split (quotes and escapes of the
    /// spec), field codes dropped; `Terminal=true` inside `terminal`.
    pub fn argv(&self, terminal: &[String]) -> Vec<String> {
        let cmd = split_exec(&self.exec);
        if self.terminal && !cmd.is_empty() {
            let mut v = terminal.to_vec();
            v.extend(cmd);
            return v;
        }
        cmd
    }
}

/// Split an `Exec` value into argv: double quotes group, `\` escapes inside
/// them, field codes (`%f %F %u %U %d %D %n %N %i %c %k %v %m`) go, `%%` is `%`.
pub fn split_exec(exec: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut any = false;
    let mut chars = exec.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '"' => {
                quoted = !quoted;
                any = true;
            }
            '\\' if quoted => {
                if let Some(n) = chars.next() {
                    cur.push(n);
                }
            }
            // `%%` is a `%`; any other field code is dropped
            '%' => {
                if chars.next() == Some('%') {
                    cur.push('%');
                }
            }
            c if c.is_whitespace() && !quoted => {
                if !cur.is_empty() || any {
                    out.push(std::mem::take(&mut cur));
                }
                any = false;
            }
            c => cur.push(c),
        }
    }
    if !cur.is_empty() || any {
        out.push(cur);
    }
    out.retain(|a| !a.is_empty());
    out
}

/// The application dirs in priority order.
pub fn dirs(
    data_home: Option<&Path>,
    home: Option<&Path>,
    data_dirs: Option<&str>,
) -> Vec<PathBuf> {
    let mut v = Vec::new();
    match (data_home, home) {
        (Some(d), _) => v.push(d.join("applications")),
        (None, Some(h)) => v.push(h.join(".local/share/applications")),
        _ => {}
    }
    let dd = data_dirs
        .filter(|s| !s.is_empty())
        .unwrap_or("/usr/local/share:/usr/share");
    v.extend(
        dd.split(':')
            .filter(|s| !s.is_empty())
            .map(|d| Path::new(d).join("applications")),
    );
    v
}

/// The desktop file id of `path` under `dir` (`a/b.desktop` → `a-b`).
fn file_id(dir: &Path, path: &Path) -> Option<String> {
    let rel = path.strip_prefix(dir).ok()?.to_str()?;
    let id = rel.strip_suffix(".desktop")?;
    Some(id.replace('/', "-"))
}

fn walk(dir: &Path, depth: u8, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    let mut items: Vec<_> = rd.flatten().map(|e| e.path()).collect();
    items.sort();
    for p in items {
        if p.is_dir() && depth < 3 {
            walk(&p, depth + 1, out);
        } else if p.extension().is_some_and(|e| e == "desktop") {
            out.push(p);
        }
    }
}

/// Every shown app of `dirs` (the first file of an id wins, hidden ones
/// included in that rule), sorted by name.
pub fn scan(dirs: &[PathBuf], current: &[String], terminal: &[String]) -> Vec<App> {
    let mut seen = BTreeSet::new();
    let mut apps = Vec::new();
    for d in dirs {
        let mut files = Vec::new();
        walk(d, 0, &mut files);
        for f in files {
            let Some(id) = file_id(d, &f) else { continue };
            if !seen.insert(id.clone()) {
                continue;
            }
            let Some(e) = std::fs::read_to_string(&f).ok().and_then(|t| parse(&t)) else {
                continue;
            };
            if !e.shown(current) {
                continue;
            }
            let argv = e.argv(terminal);
            if argv.is_empty() {
                continue;
            }
            apps.push(App {
                id,
                name: e.name,
                argv,
            });
        }
    }
    apps.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then(a.id.cmp(&b.id))
    });
    apps
}

/// The system's apps (environment of this process).
pub fn system(terminal: &[String]) -> Vec<App> {
    let env = |k: &str| std::env::var_os(k).map(PathBuf::from);
    let data_dirs = std::env::var("XDG_DATA_DIRS").ok();
    let current: Vec<String> = std::env::var("XDG_CURRENT_DESKTOP")
        .unwrap_or_default()
        .split(':')
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect();
    let dirs = dirs(
        env("XDG_DATA_HOME").as_deref(),
        env("HOME").as_deref(),
        data_dirs.as_deref(),
    );
    scan(&dirs, &current, terminal)
}

/// `--demo`: the fixture apps (testclients, so a run from the palette
/// opens something real in the demo world).
pub fn demo() -> Vec<App> {
    let tc = |id: &str, name: &str, app_id: &str| App {
        id: id.into(),
        name: name.into(),
        argv: vec!["testclient".into(), "--app-id".into(), app_id.into()],
    };
    vec![
        tc("demo.alpha", "Alpha (demo)", "demo.alpha"),
        tc("demo.bravo", "Bravo (demo)", "demo.bravo"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn field_codes_and_quotes() {
        let rows: &[(&str, &[&str])] = &[
            ("foot", &["foot"]),
            ("firefox %u", &["firefox"]),
            ("app --open %F --flag", &["app", "--open", "--flag"]),
            ("app %U%i", &["app"]),
            ("\"/opt/my app/bin\" --x", &["/opt/my app/bin", "--x"]),
            (
                "sh -c \"echo \\\"oi\\\" \\$HOME\"",
                &["sh", "-c", "echo \"oi\" $HOME"],
            ),
            ("printf 100%%", &["printf", "100%"]),
            ("app \"\"", &["app"]),
            ("  spaced   out  ", &["spaced", "out"]),
            ("", &[]),
        ];
        for (exec, want) in rows {
            assert_eq!(split_exec(exec), *want, "{exec}");
        }
    }

    const FILE: &str = "\
# comment
[Desktop Entry]
Type=Application
Name=Editor
Name[pt_BR]=Editor de texto
Name[de]=Texteditor
Exec=editor %F
Terminal=false

[Desktop Action new]
Name=New window
Exec=editor --new
";

    #[test]
    fn parses_an_entry() {
        let e = parse(FILE).unwrap();
        assert_eq!(e.name, "Editor de texto");
        assert_eq!(e.exec, "editor %F", "not the action's Exec");
        assert!(e.app && !e.terminal && !e.no_display);
        assert!(e.shown(&[]));
        assert_eq!(e.argv(&["foot".into()]), ["editor"]);
        assert_eq!(parse("[Desktop Entry]\nName=x\n"), None, "no Type");
        assert_eq!(parse("[Other]\nType=Application\nName=x\n"), None);
    }

    #[test]
    fn what_is_shown() {
        let base = "[Desktop Entry]\nType=Application\nName=X\nExec=x\n";
        let rows: &[(&str, &[&str], bool)] = &[
            ("", &[], true),
            ("NoDisplay=true\n", &[], false),
            ("Hidden=true\n", &[], false),
            ("OnlyShowIn=GNOME;KDE;\n", &["niri"], false),
            ("OnlyShowIn=GNOME;niri;\n", &["niri"], true),
            ("NotShowIn=niri;\n", &["niri"], false),
            ("NotShowIn=GNOME;\n", &["niri"], true),
        ];
        for (extra, cur, want) in rows {
            let e = parse(&format!("{base}{extra}")).unwrap();
            let cur: Vec<String> = cur.iter().map(|s| s.to_string()).collect();
            assert_eq!(e.shown(&cur), *want, "{extra}");
        }
        let link = parse("[Desktop Entry]\nType=Link\nName=X\nURL=https://acme.example\n").unwrap();
        assert!(!link.shown(&[]));
    }

    #[test]
    fn terminal_apps_run_in_the_terminal() {
        let e =
            parse("[Desktop Entry]\nType=Application\nName=Top\nExec=htop -d 10\nTerminal=true\n")
                .unwrap();
        let term: Vec<String> = vec!["foot".into()];
        assert_eq!(e.argv(&term), ["foot", "htop", "-d", "10"]);
        let term: Vec<String> = vec!["alacritty".into(), "-e".into()];
        assert_eq!(e.argv(&term), ["alacritty", "-e", "htop", "-d", "10"]);
    }

    #[test]
    fn scans_dirs_first_id_wins() {
        let t = tempfile::tempdir().unwrap();
        let home = t.path().join("home/applications");
        let sys = t.path().join("sys/applications");
        std::fs::create_dir_all(home.join("sub")).unwrap();
        std::fs::create_dir_all(&sys).unwrap();
        let w = |p: PathBuf, name: &str, extra: &str| {
            std::fs::write(
                p,
                format!(
                    "[Desktop Entry]\nType=Application\nName={name}\nExec={}\n{extra}",
                    name.to_lowercase()
                ),
            )
            .unwrap()
        };
        w(home.join("bravo.desktop"), "Bravo", "");
        w(sys.join("bravo.desktop"), "Bravo System", "");
        w(home.join("gone.desktop"), "Gone", "Hidden=true\n");
        w(sys.join("gone.desktop"), "Gone System", "");
        w(sys.join("alpha.desktop"), "alpha", "");
        w(home.join("sub/tool.desktop"), "Tool", "");
        std::fs::write(sys.join("notes.txt"), "x").unwrap();
        let apps = scan(&[home, sys], &[], &["foot".into()]);
        let got: Vec<(&str, &str)> = apps
            .iter()
            .map(|a| (a.id.as_str(), a.name.as_str()))
            .collect();
        assert_eq!(
            got,
            [("alpha", "alpha"), ("bravo", "Bravo"), ("sub-tool", "Tool")]
        );
        assert_eq!(apps[1].argv, ["bravo"]);
    }

    #[test]
    fn dirs_follow_the_spec() {
        let d = dirs(None, Some(Path::new("/h")), None);
        assert_eq!(
            d,
            [
                PathBuf::from("/h/.local/share/applications"),
                PathBuf::from("/usr/local/share/applications"),
                PathBuf::from("/usr/share/applications"),
            ]
        );
        let d = dirs(Some(Path::new("/d")), Some(Path::new("/h")), Some("/a:/b:"));
        assert_eq!(
            d,
            [
                PathBuf::from("/d/applications"),
                PathBuf::from("/a/applications"),
                PathBuf::from("/b/applications"),
            ]
        );
        assert_eq!(demo().len(), 2);
        assert!(demo().iter().all(|a| a.argv[0] == "testclient"));
    }
}
