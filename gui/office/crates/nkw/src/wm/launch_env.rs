//! The launcher's environment policy (S22): what every child the world
//! spawns gets on top of the hub's own (`WAYLAND_DISPLAY`, a fresh
//! `XDG_ACTIVATION_TOKEN`), the "already open outside" check for
//! single-instance apps, and the isolated-profile argv. Pure except for
//! `running_outside`, which reads a `/proc`-shaped tree given as a path
//! (and only `comm`, `cmdline` and the `WAYLAND_DISPLAY` of `environ`;
//! nothing is kept or logged).

use std::collections::BTreeSet;
use std::path::Path;

use nkw_core::rules::Launch;

/// The toolkit variables (Wayland first, X11 as the fallback where the
/// toolkit has one), `GTK_USE_PORTAL=0` (GTK's own file chooser opens
/// inside the world, not the host's portal dialog outside), and `DISPLAY`
/// when the satellite is up. Never `XDG_CURRENT_DESKTOP` (the portals pick
/// their backend by it) and never `DBUS_*` (the session bus stays the
/// host's).
pub fn default_env(x_display: Option<&str>) -> Vec<(String, String)> {
    let mut v: Vec<(String, String)> = [
        ("GDK_BACKEND", "wayland,x11"),
        ("QT_QPA_PLATFORM", "wayland;xcb"),
        ("QT_WAYLAND_DISABLE_WINDOWDECORATION", "1"),
        ("SDL_VIDEODRIVER", "wayland"),
        ("CLUTTER_BACKEND", "wayland"),
        ("MOZ_ENABLE_WAYLAND", "1"),
        ("ELECTRON_OZONE_PLATFORM_HINT", "auto"),
        ("_JAVA_AWT_WM_NONREPARENTING", "1"),
        ("GTK_USE_PORTAL", "0"),
    ]
    .iter()
    .map(|(k, v)| (k.to_string(), v.to_string()))
    .collect();
    if let Some(d) = x_display {
        v.push(("DISPLAY".into(), d.into()));
    }
    v
}

/// [`default_env`] then `[launch] env` (the file wins on a shared key; its
/// own keys follow in the file's sorted order).
pub fn env_for(cfg: &Launch, x_display: Option<&str>) -> Vec<(String, String)> {
    let mut v = default_env(x_display);
    v.retain(|(k, _)| !cfg.env.contains_key(k));
    v.extend(cfg.env.iter().map(|(k, v)| (k.clone(), v.clone())));
    v
}

/// The program of an argv: past a leading `env` and its `K=V` words.
pub fn program(argv: &[String]) -> Option<&str> {
    let mut it = argv.iter().map(String::as_str).peekable();
    if it.peek().is_some_and(|a| base(a) == "env") {
        it.next();
        while it
            .peek()
            .is_some_and(|a| a.contains('=') || a.starts_with('-'))
        {
            it.next();
        }
    }
    it.next()
}

fn base(p: &str) -> &str {
    p.rsplit('/').next().unwrap_or(p)
}

/// `/proc/<pid>/comm` holds at most 15 bytes of the name.
const COMM_MAX: usize = 15;

/// Whether a process named like `exec`'s basename runs with a
/// `WAYLAND_DISPLAY` other than `our_display` (or none: an X11 instance)
/// — the app is open outside the world, and a single-instance app would
/// hand our launch over to it. Processes whose `environ` cannot be read
/// (another user's) do not count.
pub fn running_outside(exec: &str, our_display: &str, proc_root: &Path) -> bool {
    !outside(&[exec], our_display, proc_root).is_empty()
}

/// [`running_outside`] for several programs in one scan of `proc_root`:
/// the basenames (of `execs`) found outside.
pub fn outside(execs: &[&str], our_display: &str, proc_root: &Path) -> BTreeSet<String> {
    let names: Vec<&str> = execs
        .iter()
        .map(|e| base(e.trim()))
        .filter(|n| !n.is_empty())
        .collect();
    let mut found = BTreeSet::new();
    if names.is_empty() {
        return found;
    }
    let Ok(dir) = std::fs::read_dir(proc_root) else {
        return found;
    };
    for e in dir.flatten() {
        let fname = e.file_name();
        let Some(pid) = fname.to_str() else { continue };
        if pid.is_empty() || !pid.bytes().all(|b| b.is_ascii_digit()) {
            continue;
        }
        let p = e.path();
        let comm = std::fs::read(p.join("comm")).unwrap_or_default();
        let comm = comm.strip_suffix(b"\n").unwrap_or(&comm);
        let mut argv0 = None;
        let mut hit = None;
        for n in &names {
            if found.contains(*n) {
                continue;
            }
            let short = &n.as_bytes()[..n.len().min(COMM_MAX)];
            let named = comm == short || {
                let a0 = argv0.get_or_insert_with(|| {
                    let c = std::fs::read(p.join("cmdline")).unwrap_or_default();
                    let a = c.split(|b| *b == 0).next().unwrap_or_default();
                    base(&String::from_utf8_lossy(a)).to_string()
                });
                a0.as_str() == *n
            };
            if named {
                hit = Some(*n);
                break;
            }
        }
        let Some(n) = hit else { continue };
        let Ok(env) = std::fs::read(p.join("environ")) else {
            continue;
        };
        let display = env
            .split(|b| *b == 0)
            .find_map(|kv| kv.strip_prefix(b"WAYLAND_DISPLAY="));
        if !display.is_some_and(|d| same_display(d, our_display)) {
            found.insert(n.to_string());
            if found.len() == names.len() {
                break;
            }
        }
    }
    found
}

/// `nkw-0` and `/run/user/1000/nkw-0` name the same socket.
fn same_display(d: &[u8], ours: &str) -> bool {
    let d = String::from_utf8_lossy(d);
    d == ours || base(&d) == base(ours)
}

/// Firefox-family programs: `--new-instance --profile DIR`.
const GECKO: [&str; 6] = [
    "firefox",
    "firefox-esr",
    "firefox-developer-edition",
    "librewolf",
    "zen-browser",
    "zen",
];
/// Chromium-family programs: `--user-data-dir=DIR --no-first-run`.
const BLINK: [&str; 6] = [
    "chromium",
    "chromium-browser",
    "google-chrome",
    "google-chrome-stable",
    "brave",
    "brave-browser",
];

/// The argv of `argv` with a profile of its own at `profile_dir` (the
/// flags right after the program, before any URL); `None` for an app
/// that has no such switch.
pub fn isolated_argv(argv: &[String], profile_dir: &Path) -> Option<Vec<String>> {
    let prog = program(argv)?;
    let at = argv.iter().position(|a| a == prog)? + 1;
    let dir = profile_dir.to_str()?;
    let flags: Vec<String> = match base(prog) {
        p if GECKO.contains(&p) => vec!["--new-instance".into(), "--profile".into(), dir.into()],
        p if BLINK.contains(&p) => {
            vec![format!("--user-data-dir={dir}"), "--no-first-run".into()]
        }
        _ => return None,
    };
    let mut v = argv[..at].to_vec();
    v.extend(flags);
    v.extend_from_slice(&argv[at..]);
    Some(v)
}

/// Whether an app (by desktop id or program) gets the "abrir isolado"
/// choice: `[launch] isolate` names it.
pub fn isolatable(cfg: &Launch, id: &str, argv: &[String]) -> bool {
    let prog = program(argv).map(base).unwrap_or("");
    cfg.isolate
        .iter()
        .any(|i| i == id || (!prog.is_empty() && i == prog))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|a| a.to_string()).collect()
    }

    #[test]
    fn the_defaults() {
        let v = default_env(None);
        let get = |v: &[(String, String)], k: &str| {
            v.iter().find(|(a, _)| a == k).map(|(_, b)| b.clone())
        };
        for (k, want) in [
            ("GDK_BACKEND", "wayland,x11"),
            ("QT_QPA_PLATFORM", "wayland;xcb"),
            ("QT_WAYLAND_DISABLE_WINDOWDECORATION", "1"),
            ("SDL_VIDEODRIVER", "wayland"),
            ("CLUTTER_BACKEND", "wayland"),
            ("MOZ_ENABLE_WAYLAND", "1"),
            ("ELECTRON_OZONE_PLATFORM_HINT", "auto"),
            ("_JAVA_AWT_WM_NONREPARENTING", "1"),
            ("GTK_USE_PORTAL", "0"),
        ] {
            assert_eq!(get(&v, k).as_deref(), Some(want), "{k}");
        }
        assert_eq!(get(&v, "DISPLAY"), None, "no X11 until the satellite is up");
        for (k, _) in &v {
            assert!(
                k != "XDG_CURRENT_DESKTOP" && !k.starts_with("DBUS_") && k != "WAYLAND_DISPLAY",
                "{k}"
            );
        }
        let x = default_env(Some(":7"));
        assert_eq!(get(&x, "DISPLAY").as_deref(), Some(":7"));
        assert_eq!(x.len(), v.len() + 1);
    }

    #[test]
    fn env_for_the_file_wins() {
        let cfg = Launch {
            env: BTreeMap::from([
                ("GDK_BACKEND".to_string(), "x11".to_string()),
                ("ACME_DEBUG".to_string(), "1".to_string()),
                ("DISPLAY".to_string(), ":3".to_string()),
            ]),
            ..Launch::default()
        };
        let v = env_for(&cfg, Some(":7"));
        let all = |k: &str| v.iter().filter(|(a, _)| a == k).count();
        assert_eq!(all("GDK_BACKEND"), 1, "once");
        assert!(v.contains(&("GDK_BACKEND".into(), "x11".into())));
        assert!(v.contains(&("ACME_DEBUG".into(), "1".into())));
        assert!(
            v.contains(&("DISPLAY".into(), ":3".into())),
            "the file's own DISPLAY"
        );
        assert_eq!(all("DISPLAY"), 1);
        // the file's keys come last: applied after, they win anyway
        assert_eq!(v.last().unwrap().0, "GDK_BACKEND");
        assert_eq!(env_for(&Launch::default(), None), default_env(None));
    }

    #[test]
    fn the_program_of_an_argv() {
        for (argv, want) in [
            (s(&["firefox", "%u"]), Some("firefox")),
            (s(&["/usr/bin/chromium"]), Some("/usr/bin/chromium")),
            (
                s(&["env", "A=1", "B=2", "zen-browser"]),
                Some("zen-browser"),
            ),
            (s(&["/usr/bin/env", "-i", "acme"]), Some("acme")),
            (s(&[]), None),
        ] {
            assert_eq!(program(&argv), want, "{argv:?}");
        }
    }

    #[test]
    fn isolated_argv_by_family() {
        let d = PathBuf::from("/srv/state/profiles/firefox");
        assert_eq!(
            isolated_argv(&s(&["firefox", "about:blank"]), &d).unwrap(),
            s(&[
                "firefox",
                "--new-instance",
                "--profile",
                "/srv/state/profiles/firefox",
                "about:blank"
            ])
        );
        assert_eq!(
            isolated_argv(&s(&["/opt/zen/zen-browser"]), &d).unwrap()[1..3],
            s(&["--new-instance", "--profile"])
        );
        assert_eq!(
            isolated_argv(&s(&["env", "A=1", "chromium", "--incognito"]), &d).unwrap(),
            s(&[
                "env",
                "A=1",
                "chromium",
                "--user-data-dir=/srv/state/profiles/firefox",
                "--no-first-run",
                "--incognito"
            ])
        );
        assert_eq!(isolated_argv(&s(&["mpv", "a.mkv"]), &d), None);
        assert_eq!(isolated_argv(&s(&[]), &d), None);
    }

    #[test]
    fn isolatable_by_id_or_program() {
        let cfg = Launch::default();
        assert!(isolatable(
            &cfg,
            "firefox",
            &s(&["/usr/lib/firefox/firefox"])
        ));
        assert!(isolatable(&cfg, "org.acme.Browser", &s(&["chromium"])));
        assert!(!isolatable(&cfg, "acme.term", &s(&["foot"])));
        let none = Launch {
            isolate: Vec::new(),
            ..Launch::default()
        };
        assert!(!isolatable(&none, "firefox", &s(&["firefox"])));
    }

    /// A fake `/proc`: (pid, comm, argv0, WAYLAND_DISPLAY or none, environ readable).
    fn fake_proc(rows: &[(&str, &str, &str, Option<&str>, bool)]) -> tempfile::TempDir {
        let t = tempfile::tempdir().unwrap();
        for (pid, comm, argv0, wd, readable) in rows {
            let p = t.path().join(pid);
            std::fs::create_dir(&p).unwrap();
            std::fs::write(p.join("comm"), format!("{comm}\n")).unwrap();
            std::fs::write(p.join("cmdline"), format!("{argv0}\0--flag\0")).unwrap();
            if *readable {
                let mut env = String::from("HOME=/srv/acme\0");
                if let Some(d) = wd {
                    env.push_str(&format!("WAYLAND_DISPLAY={d}\0"));
                }
                env.push_str("LANG=C\0");
                std::fs::write(p.join("environ"), env).unwrap();
            }
        }
        std::fs::create_dir(t.path().join("self")).unwrap();
        std::fs::write(t.path().join("uptime"), "1 1\n").unwrap();
        t
    }

    #[test]
    fn running_outside_reads_comm_cmdline_and_the_display() {
        let t = fake_proc(&[
            (
                "10",
                "firefox",
                "/usr/lib/firefox/firefox",
                Some("wayland-1"),
                true,
            ),
            ("11", "foot", "foot", Some("nkw-0"), true),
            (
                "12",
                "chromium",
                "/usr/lib/chromium/chromium",
                Some("/run/user/1/nkw-0"),
                true,
            ),
            ("13", "zen-bin", "/opt/zen/zen-browser", None, true),
            ("14", "mpv", "mpv", Some("wayland-1"), false),
            (
                "15",
                "a-very-long-pro",
                "/opt/a-very-long-program-name",
                Some("wayland-1"),
                true,
            ),
        ]);
        let root = t.path();
        let rows = [
            ("firefox", true, "another display"),
            ("/usr/bin/firefox", true, "by basename"),
            ("foot", false, "ours"),
            ("chromium", false, "ours, as a path"),
            (
                "zen-browser",
                true,
                "by argv0; no WAYLAND_DISPLAY = X11, outside",
            ),
            ("mpv", false, "environ unreadable: not counted"),
            ("a-very-long-program-name", true, "comm is truncated to 15"),
            ("kitty", false, "not running"),
            ("", false, "no name"),
        ];
        for (exec, want, why) in rows {
            assert_eq!(running_outside(exec, "nkw-0", root), want, "{exec}: {why}");
        }
        assert!(!running_outside(
            "firefox",
            "nkw-0",
            Path::new("/nonexistent/proc")
        ));
        let all = outside(
            &["firefox", "foot", "zen-browser", "mpv", "kitty"],
            "nkw-0",
            root,
        );
        assert_eq!(
            all.into_iter().collect::<Vec<_>>(),
            ["firefox", "zen-browser"],
            "one scan, every name"
        );
        assert!(outside(&[], "nkw-0", root).is_empty());
    }
}
