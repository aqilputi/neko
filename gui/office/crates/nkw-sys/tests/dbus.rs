//! MPRIS, UPower and NetworkManager adapters against fakes on a private bus.

mod common;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use common::*;
use nkw_sys::{NetState, PowerState, SysEvent, SysOpts};

const T: Duration = Duration::from_secs(5);

fn opts(bus: &PrivateBus) -> SysOpts {
    SysOpts {
        bus_override: Some(bus.addr.clone()),
        ..SysOpts::none()
    }
}

#[test]
fn mpris_player_state_updates_and_commands() {
    let Some(bus) = private_bus() else { return };
    let calls = Arc::new(Mutex::new(Vec::new()));
    let player = FakePlayer {
        status: "Playing".into(),
        title: "Lo-fi for cats".into(),
        volume: 0.8,
        calls: calls.clone(),
    };
    let fake = conn(&bus.addr)
        .name("org.mpris.MediaPlayer2.acme")
        .unwrap()
        .serve_at("/org/mpris/MediaPlayer2", player)
        .unwrap()
        .build()
        .unwrap();

    let wakes = Arc::new(AtomicUsize::new(0));
    let w = wakes.clone();
    let sys = nkw_sys::start(SysOpts {
        session_bus: true,
        mpris: true,
        wake: Some(Arc::new(move || {
            w.fetch_add(1, Ordering::SeqCst);
        })),
        ..opts(&bus)
    });

    let m = wait_for(sys.events(), T, |e| match e {
        SysEvent::Media(m) => Some(m.clone()),
        _ => None,
    })
    .expect("initial Media");
    assert!(m.playing);
    assert_eq!(
        (m.title.as_str(), m.artist.as_str(), m.player.as_str()),
        ("Lo-fi for cats", "acme radio", "acme")
    );
    assert_eq!(m.length_us, Some(180_000_000));
    assert!((m.volume - 0.8).abs() < 1e-6);
    // wake runs right after the send, on the adapter thread
    let end = std::time::Instant::now() + T;
    while wakes.load(Ordering::SeqCst) == 0 && std::time::Instant::now() < end {
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(wakes.load(Ordering::SeqCst) >= 1, "wake called after send");

    // the player pauses and says so
    let iface = fake
        .object_server()
        .interface::<_, FakePlayer>("/org/mpris/MediaPlayer2")
        .unwrap();
    iface.get_mut().status = "Paused".into();
    async_io::block_on(iface.get().playback_status_changed(iface.signal_emitter())).unwrap();
    let m = wait_for(sys.events(), T, |e| match e {
        SysEvent::Media(m) if !m.playing => Some(m.clone()),
        _ => None,
    })
    .expect("paused update");
    assert_eq!(m.title, "Lo-fi for cats");

    // commands reach the active player
    sys.media().play_pause();
    sys.media().next();
    sys.media().set_volume(0.25);
    let end = std::time::Instant::now() + T;
    while calls.lock().unwrap().len() < 3 && std::time::Instant::now() < end {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(*calls.lock().unwrap(), ["PlayPause", "Next", "Volume=0.25"]);

    // the player leaves the bus
    drop(iface);
    drop(fake);
    assert!(
        wait_for(sys.events(), T, |e| matches!(e, SysEvent::MediaGone)
            .then_some(()))
        .is_some(),
        "MediaGone after the player quits"
    );
    sys.shutdown();
}

#[test]
fn mpris_player_appearing_later_is_found() {
    let Some(bus) = private_bus() else { return };
    let sys = nkw_sys::start(SysOpts {
        session_bus: true,
        mpris: true,
        ..opts(&bus)
    });
    std::thread::sleep(Duration::from_millis(100));
    let player = FakePlayer {
        status: "Playing".into(),
        title: "bravo".into(),
        volume: 1.0,
        calls: Default::default(),
    };
    let _fake = conn(&bus.addr)
        .name("org.mpris.MediaPlayer2.bravo")
        .unwrap()
        .serve_at("/org/mpris/MediaPlayer2", player)
        .unwrap()
        .build()
        .unwrap();
    let m = wait_for(sys.events(), T, |e| match e {
        SysEvent::Media(m) => Some(m.clone()),
        _ => None,
    })
    .expect("Media from the late player");
    assert_eq!((m.player.as_str(), m.title.as_str()), ("bravo", "bravo"));
    sys.shutdown();
}

#[test]
fn upower_display_device() {
    let Some(bus) = private_bus() else { return };
    let fake = conn(&bus.addr)
        .name("org.freedesktop.UPower")
        .unwrap()
        .serve_at(
            "/org/freedesktop/UPower/devices/DisplayDevice",
            FakeBattery {
                pct: 73.0,
                state: 2,
            },
        )
        .unwrap()
        .build()
        .unwrap();
    let sys = nkw_sys::start(SysOpts {
        system_bus: true,
        upower: true,
        ..opts(&bus)
    });
    let p = wait_for(sys.events(), T, |e| match e {
        SysEvent::Power(p) => Some(*p),
        _ => None,
    })
    .expect("Power");
    assert_eq!(
        p,
        PowerState {
            present: true,
            pct: 73,
            charging: false,
            time_to_empty_s: Some(3600)
        }
    );

    let iface = fake
        .object_server()
        .interface::<_, FakeBattery>("/org/freedesktop/UPower/devices/DisplayDevice")
        .unwrap();
    iface.get_mut().state = 1;
    iface.get_mut().pct = 74.0;
    let ctx = iface.signal_emitter();
    async_io::block_on(async {
        iface.get().state_changed(ctx).await.unwrap();
        iface.get().percentage_changed(ctx).await.unwrap();
    });
    let p = wait_for(sys.events(), T, |e| match e {
        SysEvent::Power(p) if p.pct == 74 => Some(*p),
        _ => None,
    })
    .expect("Power update");
    assert!(p.charging && p.time_to_empty_s.is_none());
    sys.shutdown();
}

#[test]
fn networkmanager_wired() {
    let Some(bus) = private_bus() else { return };
    let fake = conn(&bus.addr)
        .name("org.freedesktop.NetworkManager")
        .unwrap()
        .serve_at("/org/freedesktop/NetworkManager", FakeNm { state: 70 })
        .unwrap()
        .build()
        .unwrap();
    let sys = nkw_sys::start(SysOpts {
        system_bus: true,
        nm: true,
        ..opts(&bus)
    });
    let n = wait_for(sys.events(), T, |e| match e {
        SysEvent::Net(n) => Some(*n),
        _ => None,
    })
    .expect("Net");
    assert_eq!(
        n,
        NetState {
            connected: true,
            wifi: false,
            strength: 4
        }
    );

    let iface = fake
        .object_server()
        .interface::<_, FakeNm>("/org/freedesktop/NetworkManager")
        .unwrap();
    iface.get_mut().state = 20;
    async_io::block_on(iface.get().state_changed(iface.signal_emitter())).unwrap();
    let n = wait_for(sys.events(), T, |e| match e {
        SysEvent::Net(n) => Some(*n),
        _ => None,
    })
    .expect("Net drop");
    assert_eq!(n, NetState::default());
    sys.shutdown();
}

#[test]
fn missing_services_report_one_error_each_and_the_rest_runs() {
    let Some(bus) = private_bus() else { return };
    let player = FakePlayer {
        status: "Playing".into(),
        title: "alpha".into(),
        volume: 1.0,
        calls: Default::default(),
    };
    let _fake = conn(&bus.addr)
        .name("org.mpris.MediaPlayer2.alpha")
        .unwrap()
        .serve_at("/org/mpris/MediaPlayer2", player)
        .unwrap()
        .build()
        .unwrap();
    // no UPower, no NM on this bus
    let sys = nkw_sys::start(SysOpts {
        session_bus: true,
        system_bus: true,
        mpris: true,
        upower: true,
        nm: true,
        ..opts(&bus)
    });
    let mut errors = Vec::new();
    let mut media = false;
    let end = std::time::Instant::now() + T;
    while (errors.len() < 2 || !media) && std::time::Instant::now() < end {
        match sys.events().recv_timeout(Duration::from_millis(200)) {
            Ok(SysEvent::Error { source, .. }) => errors.push(source),
            Ok(SysEvent::Media(_)) => media = true,
            _ => {}
        }
    }
    errors.sort();
    assert_eq!(errors, ["nm", "upower"]);
    assert!(media, "MPRIS kept running");
    sys.shutdown();
}
