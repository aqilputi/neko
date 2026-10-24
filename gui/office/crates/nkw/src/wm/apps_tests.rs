//! Tests of `wm::apps` (the launcher's policy, X11 on demand, the tour
//! flag, the phone's Apps/Janelas).

use super::*;
use crate::cli::Cli;
use crate::ext::Plugin;
use crate::wm::launcher::App;
use crate::world::{Opts, Room, Screen, World};
use glam::Vec2;
use nkw_core::strip::{Desk, Width};
use nkw_core::{Output, RoomId};
use serde_json::json;

fn world() -> World {
    let out = Output {
        width_px: 1280,
        height_px: 720,
        scale: 1.0,
    };
    let rooms = vec![Room {
        id: RoomId(0),
        name: "acme".into(),
        desks: vec![Desk {
            width: Width::Half,
            screens: vec![ScreenId(1), ScreenId(2)],
            cat: None,
        }],
    }];
    let mut w = World::with_rooms(
        out,
        Opts {
            reduced_motion: false,
        },
        rooms,
    );
    for (id, app, title) in [(1, "acme.term", "shell"), (2, "demo.viewer", "")] {
        w.screens.insert(
            ScreenId(id),
            Screen {
                id: ScreenId(id),
                app_id: app.into(),
                title: title.into(),
                size: Vec2::new(624.0, 360.0),
            },
        );
    }
    w
}

fn with_ctx<R>(w: &World, now: Time, f: impl FnOnce(&Ctx) -> R) -> R {
    let pids = BTreeMap::new();
    let visible = w.visible_rooms(&[]);
    let ctx = Ctx {
        world: w,
        now,
        frame: 0,
        reduced_motion: false,
        demo: true,
        scale: 1.0,
        size_px: [1280, 720],
        pids: &pids,
        visible_rooms: &visible,
        line: None,
    };
    f(&ctx)
}

fn demo() -> Wm {
    Wm::new(&Cli {
        demo: true,
        socket: "nkw-test".into(),
        ..Cli::default()
    })
}

fn real(dir: &Path) -> Wm {
    Wm::new(&Cli {
        state_dir: Some(dir.to_path_buf()),
        headless: true,
        socket: "nkw-test".into(),
        xwayland: Some(crate::cli::XwaylandMode::Off),
        ..Cli::default()
    })
}

fn args(kv: &[(&str, &str)]) -> BTreeMap<String, String> {
    kv.iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

type Spawned = Vec<(Vec<String>, Vec<(String, String)>)>;

fn spawned(r: &[Req]) -> Spawned {
    r.iter()
        .filter_map(|q| match q {
            Req::SpawnWith { argv, env } => Some((argv.clone(), env.clone())),
            _ => None,
        })
        .collect()
}

fn toasts(r: &[Req]) -> Vec<String> {
    r.iter()
        .filter_map(|q| match q {
            Req::Toast { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect()
}

fn has(env: &[(String, String)], k: &str, v: &str) -> bool {
    env.iter().any(|(a, b)| a == k && b == v)
}

/// A fake `/proc`: a firefox outside (another display), a foot of ours.
fn fake_proc() -> tempfile::TempDir {
    let t = tempfile::tempdir().unwrap();
    for (pid, comm, wd) in [("40", "firefox", "wayland-1"), ("41", "foot", "nkw-test")] {
        let p = t.path().join(pid);
        std::fs::create_dir(&p).unwrap();
        std::fs::write(p.join("comm"), format!("{comm}\n")).unwrap();
        std::fs::write(p.join("cmdline"), format!("{comm}\0")).unwrap();
        std::fs::write(p.join("environ"), format!("WAYLAND_DISPLAY={wd}\0")).unwrap();
    }
    t
}

#[test]
fn the_outside_warning_and_the_spawn_still_happens() {
    let mut wm = demo();
    let w = world();
    let fx = args(&[("exec", "firefox about:blank")]);
    // under --demo no /proc is read at all
    let r = with_ctx(&w, 0.0, |c| wm.command("world.launch", &fx, c)).unwrap();
    assert!(toasts(&r).is_empty(), "{r:?}");
    let p = fake_proc();
    let inj = json!({"proc_root": p.path().to_str().unwrap()});
    with_ctx(&w, 0.0, |c| wm.inject(&inj, c));
    let r = with_ctx(&w, 0.0, |c| wm.command("world.launch", &fx, c)).unwrap();
    assert_eq!(toasts(&r), [outside_toast("firefox")], "{r:?}");
    assert!(toasts(&r)[0].contains("abrir isolado"));
    let s = spawned(&r);
    assert_eq!(s.len(), 1, "the user asked for it");
    assert_eq!(s[0].0, ["firefox", "about:blank"]);
    assert!(has(&s[0].1, "GTK_USE_PORTAL", "0"));
    // not an isolatable app: no warning even when it runs outside
    let foot = args(&[("exec", "foot")]);
    let r = with_ctx(&w, 0.0, |c| wm.command("world.launch", &foot, c)).unwrap();
    assert!(toasts(&r).is_empty());
}

#[test]
fn launch_isolated_makes_a_0700_profile_never_under_demo() {
    use std::os::unix::fs::PermissionsExt;
    let t = tempfile::tempdir().unwrap();
    let mut wm = real(t.path());
    wm.proc_root = None;
    let ff = App {
        id: "firefox".into(),
        name: "Firefox".into(),
        argv: vec!["firefox".into()],
    };
    let mpv = App {
        id: "mpv".into(),
        name: "mpv".into(),
        argv: vec!["mpv".into()],
    };
    let _ = wm.apps.set(vec![ff.clone(), mpv]);
    let w = world();
    let id = args(&[("id", "firefox")]);
    let r = with_ctx(&w, 0.0, |c| wm.command(LAUNCH_ISOLATED, &id, c)).unwrap();
    let dir = t.path().join("profiles").join("firefox");
    assert!(dir.is_dir());
    for d in [dir.clone(), t.path().join("profiles")] {
        let mode = std::fs::metadata(&d).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o700, "{}", d.display());
    }
    let s = spawned(&r);
    assert_eq!(
        s[0].0,
        [
            "firefox",
            "--new-instance",
            "--profile",
            dir.to_str().unwrap()
        ]
    );
    // no switch: a toast, nothing spawned
    let r = with_ctx(&w, 0.0, |c| {
        wm.command(LAUNCH_ISOLATED, &args(&[("id", "mpv")]), c)
    })
    .unwrap();
    assert!(spawned(&r).is_empty());
    assert_eq!(toasts(&r), ["mpv não tem perfil isolado"]);
    // unknown app / no id: not ours
    let x = args(&[("id", "x")]);
    assert_eq!(
        with_ctx(&w, 0.0, |c| wm.command(LAUNCH_ISOLATED, &x, c)),
        None
    );
    let none = BTreeMap::new();
    assert_eq!(
        with_ctx(&w, 0.0, |c| wm.command(LAUNCH_ISOLATED, &none, c)),
        None
    );
    // --demo: never a profile
    let mut d = demo();
    let _ = d.apps.set(vec![ff]);
    let r = with_ctx(&w, 0.0, |c| d.command(LAUNCH_ISOLATED, &id, c)).unwrap();
    assert!(spawned(&r).is_empty());
    assert_eq!(toasts(&r).len(), 1);
}

#[test]
fn profile_names_are_plain() {
    for (id, want) in [
        ("firefox", Some("firefox")),
        ("org.acme.Browser", Some("org.acme.Browser")),
        ("../x", Some(".._x")),
        ("a/b c", Some("a_b_c")),
        ("..", None),
        ("", None),
    ] {
        assert_eq!(profile_name(id).as_deref(), want, "{id}");
    }
}

#[test]
fn the_tour_is_asked_once_on_a_fresh_state_dir() {
    let t = tempfile::tempdir().unwrap();
    let mut wm = real(t.path());
    let w = world();
    let tours = |r: &[Req]| {
        r.iter()
            .filter(|q| matches!(q, Req::Command { name, .. } if name == "world.tour"))
            .count()
    };
    assert_eq!(tours(&with_ctx(&w, 10.0, |c| wm.tick(c))), 0, "not at once");
    let wish = nkw_core::Wish::compose(nkw_core::Want::At(10.0 + TOUR_AFTER_S));
    assert!(with_ctx(&w, 10.0, |c| wm.wishes(c)).contains(&wish));
    assert_eq!(tours(&with_ctx(&w, 11.0, |c| wm.tick(c))), 0);
    assert_eq!(tours(&with_ctx(&w, 11.5, |c| wm.tick(c))), 1);
    assert_eq!(tours(&with_ctx(&w, 12.0, |c| wm.tick(c))), 0, "once");
    // the cats say it was seen: saved with the usual debounce
    let none = BTreeMap::new();
    assert_eq!(
        with_ctx(&w, 12.0, |c| wm.command(TOUR_DONE, &none, c)),
        Some(vec![])
    );
    assert_eq!(wm.dump()["tour_done"], true);
    with_ctx(&w, 12.1, |c| wm.tick(c));
    with_ctx(&w, 20.0, |c| wm.tick(c));
    let text = std::fs::read_to_string(t.path().join("world.json")).unwrap();
    assert!(nkw_core::persist::parse(&text).unwrap().tour_done);
    // the next run: no tour
    let mut again = real(t.path());
    for now in [0.0, 1.0, 2.0, 5.0] {
        assert_eq!(tours(&with_ctx(&w, now, |c| again.tick(c))), 0);
    }
    assert_eq!(again.dump()["tour_done"], true);
    // --demo: never
    let mut d = demo();
    for now in [0.0, 2.0, 5.0] {
        assert_eq!(tours(&with_ctx(&w, now, |c| d.tick(c))), 0);
    }
}

#[test]
fn launch_env_from_the_file_reaches_every_child() {
    let t = tempfile::tempdir().unwrap();
    std::fs::write(
        t.path().join("world.toml"),
        "[launch]\nterminal = \"foot\"\n[launch.env]\nNKW_T = \"1\"\nGDK_BACKEND = \"x11\"\n",
    )
    .unwrap();
    let mut wm = real(t.path());
    let w = world();
    for (name, a) in [
        ("world.terminal", BTreeMap::new()),
        ("world.launch", args(&[("exec", "acme --x")])),
    ] {
        let r = with_ctx(&w, 0.0, |c| wm.command(name, &a, c)).unwrap();
        let (_, env) = &spawned(&r)[0];
        assert!(has(env, "NKW_T", "1"), "{name}: {env:?}");
        assert!(has(env, "GDK_BACKEND", "x11"), "{name}");
        assert_eq!(env.iter().filter(|(k, _)| k == "GDK_BACKEND").count(), 1);
        assert!(
            !env.iter().any(|(k, _)| k == "DISPLAY"),
            "{name}: no X11 (off)"
        );
    }
}

/// A Wm with the satellite on and a fake binary and socket dir.
fn with_x(t: &Path) -> Wm {
    let mut wm = Wm::new(&Cli {
        demo: true,
        socket: "nkw-test".into(),
        xwayland: Some(crate::cli::XwaylandMode::On),
        ..Cli::default()
    });
    let x11 = t.join(".X11-unix");
    std::fs::create_dir_all(&x11).unwrap();
    wm.xwayland.binary = Some(t.join("xwayland-satellite"));
    wm.xwayland.socket_dir = x11;
    wm.proc_root = None;
    wm
}

#[test]
fn x11_on_demand_children_wait_for_the_socket() {
    let t = tempfile::tempdir().unwrap();
    let mut wm = with_x(t.path());
    let w = world();
    assert_eq!(wm.dump()["xwayland"]["state"], "off");
    let one = args(&[("exec", "acme-x11 --one")]);
    let r = with_ctx(&w, 1.0, |c| wm.command("world.launch", &one, c)).unwrap();
    let s = spawned(&r);
    assert_eq!(s.len(), 1, "only the satellite: the child waits");
    assert_eq!(s[0].0[1], ":7");
    assert!(s[0].0[0].ends_with("xwayland-satellite"));
    assert!(s[0].1.is_empty(), "the satellite gets the hub's env only");
    assert_eq!(wm.dump()["xwayland"]["state"], "starting");
    assert_eq!(with_ctx(&w, 1.0, |c| wm.status(c)).xwayland, Some(false));
    let two = args(&[("exec", "acme-x11 --two")]);
    let r = with_ctx(&w, 1.1, |c| wm.command("world.launch", &two, c)).unwrap();
    assert!(spawned(&r).is_empty(), "waits too");
    assert!(with_ctx(&w, 1.2, |c| wm.tick(c)).is_empty());
    assert!(
        !with_ctx(&w, 1.2, |c| wm.wishes(c)).is_empty(),
        "polls while starting"
    );
    std::fs::write(t.path().join(".X11-unix").join("X7"), "").unwrap();
    let r = with_ctx(&w, 1.3, |c| wm.tick(c));
    let s = spawned(&r);
    assert_eq!(s.len(), 2, "{r:?}");
    assert_eq!(s[0].0, ["acme-x11", "--one"]);
    for (_, env) in &s {
        assert!(has(env, "DISPLAY", ":7"));
    }
    assert_eq!(wm.dump()["xwayland"]["state"], "up");
    assert_eq!(wm.dump()["xwayland"]["display"], ":7");
    assert_eq!(wm.xwayland_display().as_deref(), Some(":7"));
    assert_eq!(with_ctx(&w, 1.3, |c| wm.status(c)).xwayland, Some(true));
    // later children get DISPLAY at once, terminals too
    let none = BTreeMap::new();
    let r = with_ctx(&w, 2.0, |c| wm.command("world.terminal", &none, c)).unwrap();
    assert!(has(&spawned(&r)[0].1, "DISPLAY", ":7"));
    // off: back to no X11
    let off = args(&[("on", "false")]);
    let r = with_ctx(&w, 3.0, |c| wm.command(XWAYLAND, &off, c)).unwrap();
    assert_eq!(toasts(&r), ["xwayland desligado"]);
    assert_eq!(*wm.xwayland_state(), xwayland::XState::Off);
    let bad = args(&[("on", "talvez")]);
    assert_eq!(with_ctx(&w, 3.0, |c| wm.command(XWAYLAND, &bad, c)), None);
}

#[test]
fn x11_gives_up_and_releases_the_children() {
    let t = tempfile::tempdir().unwrap();
    let mut wm = with_x(t.path());
    let w = world();
    let acme = args(&[("exec", "acme")]);
    with_ctx(&w, 0.0, |c| wm.command("world.launch", &acme, c));
    let r = with_ctx(&w, 2.5, |c| wm.tick(c));
    assert_eq!(toasts(&r), [xwayland::FAILED_TOAST]);
    let s = spawned(&r);
    assert_eq!(s.len(), 1);
    assert!(!s[0].1.iter().any(|(k, _)| k == "DISPLAY"));
    // world.xwayland {on} tries again
    let on = args(&[("on", "true")]);
    let r = with_ctx(&w, 3.0, |c| wm.command(XWAYLAND, &on, c)).unwrap();
    assert_eq!(spawned(&r).len(), 1, "the satellite again");
    // without the binary: a toast once per run, the child spawns anyway
    let mut nob = with_x(t.path());
    nob.xwayland.binary = None;
    let r = with_ctx(&w, 0.0, |c| nob.command("world.launch", &acme, c)).unwrap();
    assert_eq!(toasts(&r), [xwayland::MISSING_TOAST]);
    assert_eq!(spawned(&r).len(), 1);
    let r = with_ctx(&w, 0.0, |c| nob.command("world.launch", &acme, c)).unwrap();
    assert!(toasts(&r).is_empty());
    assert_eq!(nob.dump()["xwayland"], serde_json::Value::Null, "disabled");
}

#[test]
fn the_phone_apps_and_their_pages() {
    let mut wm = demo();
    let w = world();
    let apps = wm.phone_apps_of();
    let ids: Vec<(&str, u8)> = apps.iter().map(|a| (a.id, a.order)).collect();
    assert_eq!(ids, [("apps", 4), ("janelas", 5)]);
    let st = PhoneState::default();
    let page = with_ctx(&w, 0.0, |c| wm.phone_page_of(APP_APPS, &st, c)).unwrap();
    assert_eq!(page.title, "Apps");
    assert_eq!(page.items.len(), 3, "the terminal + the two demo apps");
    assert_eq!(
        page.items[0].on_enter.as_ref().unwrap().name,
        "world.terminal"
    );
    let a = &page.items[1];
    assert_eq!(a.title, "Alpha (demo)");
    assert_eq!(
        a.on_enter,
        Some(Act::new("world.launch:demo.alpha", &[], true))
    );
    assert!(a.on_keys.is_empty(), "not isolatable");
    assert!(page.search.as_ref().unwrap().live);
    // an isolatable app open outside: the detail says so, `i` opens it isolated
    let p = fake_proc();
    wm.proc_root = Some(p.path().to_path_buf());
    let _ = wm.apps.take();
    let _ = wm.apps.set(vec![App {
        id: "firefox".into(),
        name: "Firefox".into(),
        argv: vec!["/usr/lib/firefox/firefox".into()],
    }]);
    let page = with_ctx(&w, 0.0, |c| wm.phone_page_of(APP_APPS, &st, c)).unwrap();
    let f = &page.items[1];
    assert_eq!(f.detail.as_deref(), Some("aberto lá fora · i abre isolado"));
    assert_eq!(
        f.on_keys,
        [(
            "i".to_string(),
            Act::new(LAUNCH_ISOLATED, &[("id", "firefox")], true)
        )]
    );
    // Janelas: every screen, room · desk · app_id; enter goes there
    let page = with_ctx(&w, 0.0, |c| wm.phone_page_of(APP_JANELAS, &st, c)).unwrap();
    assert_eq!(page.items.len(), 2);
    assert_eq!(page.items[0].title, "shell");
    assert_eq!(
        page.items[0].detail.as_deref(),
        Some("acme · mesa 1 · acme.term")
    );
    assert_eq!(page.items[1].title, "demo.viewer", "no title: the app_id");
    assert_eq!(
        page.items[0].on_enter,
        Some(Act::new("world.goto_screen:1", &[], true))
    );
    assert_eq!(
        page.items[0].on_keys[0],
        (
            "Q".to_string(),
            Act::new(CLOSE_SCREEN, &[("screen", "1")], false)
        )
    );
    assert_eq!(
        with_ctx(&w, 0.0, |c| wm.phone_page_of("gatos", &st, c)),
        None,
        "not mine"
    );
    // Q and M on a screen
    let r = with_ctx(&w, 0.0, |c| {
        wm.command(CLOSE_SCREEN, &args(&[("screen", "2")]), c)
    });
    assert_eq!(r, Some(vec![Req::Close(ScreenId(2))]));
    let r = with_ctx(&w, 0.0, |c| {
        wm.command(SEND_SCREEN, &args(&[("screen", "1")]), c)
    })
    .unwrap();
    assert_eq!(r[0], Req::Cmd(crate::world::Cmd::GotoScreen(ScreenId(1))));
    assert!(matches!(&r[1], Req::Command { name, args }
        if name == "world.palette_run" && args["name"] == "world.send_room"));
    let r = with_ctx(&w, 0.0, |c| {
        wm.command(CLOSE_SCREEN, &args(&[("screen", "9")]), c)
    });
    assert_eq!(r, Some(vec![]));
}
