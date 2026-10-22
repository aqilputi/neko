//! Real apps on the headless backend (§9 "e2e real"): only with
//! `NKW_E2E=1` and the apps installed.

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

#[test]
fn foot_types_accents_through_dead_keys() {
    if !enabled(&["foot"]) {
        return;
    }
    // `´` then `e` is composed by the client from the keymap we hand it
    let r = common::run(
        r#"spawn foot -e sh -c "read x; printf '%s' \"$x\" > {dir}/typed"
wait 800
key Return
expect mode FOCO
wait 200
text "olá ção"
key dead_acute
key e
key Return
wait 500
quit
"#,
        &[],
        &[
            ("XKB_DEFAULT_LAYOUT", "us"),
            ("XKB_DEFAULT_VARIANT", "intl"),
        ],
    );
    assert_eq!(r.code(), 0, "{}", r.stderr());
    let typed = std::fs::read_to_string(r.path("typed")).unwrap_or_default();
    assert_eq!(typed, "olá çãoé");
}

#[test]
fn mpv_shows_through_dmabuf() {
    if !enabled(&["mpv"]) {
        return;
    }
    let r = common::run(
        "spawn sh -c \"WAYLAND_DEBUG=client mpv --no-config --vo=gpu --gpu-context=wayland --hwdec=no --length=2 av://lavfi:testsrc=size=320x240:rate=30 2> {dir}/mpv.log\"
wait 1500
dump {dir}/world.json
quit
",
        &[],
        &[],
    );
    assert_eq!(r.code(), 0, "{}", r.stderr());
    let log = std::fs::read_to_string(r.path("mpv.log")).unwrap_or_default();
    assert!(
        log.contains("zwp_linux_buffer_params_v1") && log.contains("wl_surface"),
        "mpv did not use linux-dmabuf:\n{}",
        &log[..log.len().min(2000)]
    );
    let d = r.json("world.json");
    assert_eq!(d["screens"][0]["app_id"], "mpv", "{d}");
}
