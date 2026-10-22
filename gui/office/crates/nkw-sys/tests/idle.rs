//! With every adapter started and nothing happening, our threads sleep: ≤ 2 context switches/s.
//! Its own test binary, so no other test's threads are counted.

mod common;

use std::time::Duration;

use common::*;
use nkw_sys::SysOpts;

#[test]
fn idle_threads_do_not_wake() {
    let Some(bus) = private_bus() else { return };
    let _player = conn(&bus.addr)
        .name("org.mpris.MediaPlayer2.acme")
        .unwrap()
        .serve_at(
            "/org/mpris/MediaPlayer2",
            FakePlayer {
                status: "Playing".into(),
                title: "demo".into(),
                volume: 1.0,
                calls: Default::default(),
            },
        )
        .unwrap()
        .build()
        .unwrap();
    let _battery = conn(&bus.addr)
        .name("org.freedesktop.UPower")
        .unwrap()
        .serve_at(
            "/org/freedesktop/UPower/devices/DisplayDevice",
            FakeBattery {
                pct: 50.0,
                state: 2,
            },
        )
        .unwrap()
        .build()
        .unwrap();
    let _nm = conn(&bus.addr)
        .name("org.freedesktop.NetworkManager")
        .unwrap()
        .serve_at("/org/freedesktop/NetworkManager", FakeNm { state: 70 })
        .unwrap()
        .build()
        .unwrap();
    // PipeWire talks to the real daemon; only when asked (read-only: nothing is set).
    let pipewire = cfg!(feature = "pipewire") && std::env::var("NKW_SYS_PW").as_deref() == Ok("1");
    let sys = nkw_sys::start(SysOpts {
        bus_override: Some(bus.addr.clone()),
        pipewire,
        ..SysOpts::default()
    });

    std::thread::sleep(Duration::from_millis(1500)); // let the start-up traffic settle
    while sys.events().try_recv().is_ok() {}
    let before = ctx_switches_except_me();
    std::thread::sleep(Duration::from_secs(5));
    let after = ctx_switches_except_me();
    let (total, per) = ctx_delta(&before, &after);
    let per_s = total as f64 / 5.0;
    eprintln!(
        "idle: {per_s:.2} context switches/s across the crate's (and the fakes') threads: {per:?}"
    );
    assert!(per_s <= 2.0, "{per_s} switches/s: {per:?}");
    sys.shutdown();
}
