//! Real apps inside the world, headless (S23): only with `NKW_E2E=1`, each
//! test skips when its binary is missing. Throwaway profiles only, never the
//! user's (rule 4 of wave 4); every app runs under `timeout` and dies with the
//! world (the harness kills nkw, nkw kills its children). foot and mpv rows
//! live in `e2e.rs`.

mod common;

fn enabled(apps: &[&str]) -> bool {
    if std::env::var("NKW_E2E").as_deref() != Ok("1") {
        eprintln!("SKIP: set NKW_E2E=1");
        return false;
    }
    for a in apps {
        let found = std::env::var_os("PATH")
            .is_some_and(|p| std::env::split_paths(&p).any(|d| d.join(a).exists()));
        if !found {
            eprintln!("SKIP: {a} not installed");
            return false;
        }
    }
    true
}

/// A `WAYLAND_DEBUG` log without libwayland's colour escapes.
fn plain(log: &str) -> String {
    let mut out = String::with_capacity(log.len());
    let mut it = log.chars();
    while let Some(c) = it.next() {
        if c == '\x1b' {
            // ESC [ … m
            for d in it.by_ref() {
                if d == 'm' {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// The interfaces a `WAYLAND_DEBUG=client` log bound (names only).
fn bound(log: &str) -> Vec<String> {
    let log = plain(log);
    log.lines()
        .filter(|l| l.contains("wl_registry") && l.contains(".bind("))
        .filter_map(|l| l.split('"').nth(1).map(str::to_string))
        .collect()
}

#[test]
fn kitty_maps_and_binds_cursor_shape_and_primary() {
    if !enabled(&["kitty"]) {
        return;
    }
    let r = common::run(
        "spawn sh -c \"WAYLAND_DEBUG=client timeout 20 kitty --config NONE -o allow_remote_control=no -o linux_display_server=wayland sh -c 'sleep 15' 2> {dir}/kitty.log\"
wait 2500
key Return
wait 800
dump {dir}/d.json
quit
",
        &[],
        &[],
    );
    assert_eq!(r.code(), 0, "{}", r.stderr());
    let d = r.json("d.json");
    assert_eq!(d["screens"][0]["app_id"], "kitty", "{}", d["screens"]);
    let b = bound(&std::fs::read_to_string(r.path("kitty.log")).unwrap_or_default());
    for g in [
        "wp_cursor_shape_manager_v1",
        "zwp_primary_selection_device_manager_v1",
    ] {
        assert!(b.iter().any(|x| x == g), "{g} not bound: {b:?}");
    }
}

#[test]
fn gtk_widget_factory_opens_a_popover_in_foco() {
    if !enabled(&["gtk4-widget-factory"]) {
        return;
    }
    let r = common::run(
        "spawn sh -c \"WAYLAND_DEBUG=client timeout 25 gtk4-widget-factory 2> {dir}/gtk.log\"
wait json /screens/0/app_id \"org.gtk.WidgetFactory4\" 15000
wait 1500
key Return
wait 800
key shift+F10
wait 1200
key ctrl+bracketright
wait 600
quit
",
        &[],
        &[],
    );
    assert_eq!(r.code(), 0, "{}", r.stderr());
    let log = plain(&std::fs::read_to_string(r.path("gtk.log")).unwrap_or_default());
    assert!(log.contains(".get_popup("), "no popup");
    assert!(
        log.contains(".popup_done("),
        "ctrl+] did not close the popup"
    );
}

/// A browser with a throwaway profile under its own D-Bus maps `app_id` in 15 s.
fn browser_maps(argv: &str, app_id: &str) {
    let r = common::run(
        &format!(
            "spawn sh -c \"timeout 30 dbus-run-session -- {argv} >/dev/null 2>&1\"
wait json /screens/0/app_id \"{app_id}\" 15000
dump {{dir}}/d.json
quit
"
        ),
        &[],
        &[],
    );
    assert_eq!(r.code(), 0, "{}", r.stderr());
    assert_eq!(r.json("d.json")["screens"][0]["app_id"], app_id);
}

#[test]
fn firefox_throwaway_profile_maps() {
    if !enabled(&["firefox", "dbus-run-session"]) {
        return;
    }
    let p = tempfile::tempdir().unwrap();
    browser_maps(
        &format!(
            "firefox --new-instance --no-remote --profile {} about:blank",
            p.path().display()
        ),
        "firefox",
    );
}

#[test]
fn chromium_throwaway_profile_maps() {
    if !enabled(&["chromium", "dbus-run-session"]) {
        return;
    }
    let p = tempfile::tempdir().unwrap();
    browser_maps(
        &format!(
            "chromium --user-data-dir={} --no-first-run --no-default-browser-check --password-store=basic about:blank",
            p.path().display()
        ),
        "chromium",
    );
}

#[test]
fn chromium_x11_through_a_satellite() {
    if !enabled(&["chromium", "xwayland-satellite", "dbus-run-session"]) {
        return;
    }
    // the first free display of :90..:99 (never one somebody uses)
    let Some(n) = (90..100).find(|n| {
        !std::path::Path::new(&format!("/tmp/.X11-unix/X{n}")).exists()
            && !std::path::Path::new(&format!("/tmp/.X{n}-lock")).exists()
    }) else {
        eprintln!("SKIP: no free X display");
        return;
    };
    let p = tempfile::tempdir().unwrap();
    let r = common::run(
        &format!(
            "spawn sh -c \"timeout 30 xwayland-satellite :{n} >/dev/null 2>&1\"
wait 1500
spawn sh -c \"DISPLAY=:{n} timeout 25 dbus-run-session -- chromium --user-data-dir={} --no-first-run --no-default-browser-check --password-store=basic --ozone-platform=x11 about:blank >/dev/null 2>&1\"
wait json /screens/0/app_id \"Chromium\" 15000
key Return
wait 600
expect mode FOCO
quit
",
            p.path().display()
        ),
        &[],
        &[],
    );
    assert_eq!(r.code(), 0, "{}", r.stderr());
}

#[test]
fn plain_strips_colour_escapes() {
    assert_eq!(
        plain("\x1b[34mwl_registry\x1b[35m#2\x1b[36m.bind\x1b[0m(1, \"wl_seat\", 9)"),
        "wl_registry#2.bind(1, \"wl_seat\", 9)"
    );
    assert_eq!(
        bound("-> wl_registry#2.bind(1, \"wl_seat\", 9, new id [unknown]#3)"),
        ["wl_seat"]
    );
}
