//! `start` without buses, the minute clock (with a 1 s period) and shutdown.

mod common;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use nkw_sys::{SysEvent, SysOpts};

#[test]
fn start_without_buses_is_fast_and_reports_errors() {
    let dir = tempfile::tempdir().unwrap();
    let nowhere = format!("unix:path={}/no-bus", dir.path().display());
    let t0 = Instant::now();
    let sys = nkw_sys::start(SysOpts {
        bus_override: Some(nowhere),
        clock: false,
        psi: false,
        pipewire: false,
        ..SysOpts::default()
    });
    assert!(
        t0.elapsed() < Duration::from_millis(100),
        "start took {:?}",
        t0.elapsed()
    );
    let mut sources = Vec::new();
    while sources.len() < 3 {
        match sys.events().recv_timeout(Duration::from_secs(2)) {
            Ok(SysEvent::Error { source, .. }) => sources.push(source),
            Ok(_) => {}
            Err(_) => break,
        }
    }
    sources.sort();
    assert_eq!(sources, ["mpris", "nm", "upower"]);
    // controls are inert, never panic
    sys.media().play_pause();
    sys.audio().set_default_sink_volume(0.5);
    assert!(sys.playback().is_none());
    let t0 = Instant::now();
    sys.shutdown();
    assert!(t0.elapsed() < Duration::from_secs(1));
}

#[cfg(feature = "pipewire")]
#[test]
fn missing_pipewire_daemon_is_an_error_not_a_panic() {
    let dir = tempfile::tempdir().unwrap();
    let sys = nkw_sys::start(SysOpts {
        pipewire: true,
        pw_remote: Some(format!("{}/no-pipewire", dir.path().display())),
        ..SysOpts::none()
    });
    assert!(sys.playback().is_none());
    let err = common::wait_for(sys.events(), Duration::from_secs(1), |e| match e {
        SysEvent::Error { source, .. } => Some(*source),
        _ => None,
    });
    assert_eq!(err, Some("pipewire"));
    sys.audio()
        .set_channel_volumes(nkw_sys::StreamId(1), [0.5, 0.5]);
    sys.shutdown();
}

#[test]
fn clock_ticks_and_wakes_then_stops() {
    let wakes = Arc::new(AtomicUsize::new(0));
    let w = wakes.clone();
    let sys = nkw_sys::start(SysOpts {
        clock: true,
        clock_period_s: 1,
        wake: Some(Arc::new(move || {
            w.fetch_add(1, Ordering::SeqCst);
        })),
        ..SysOpts::none()
    });
    let mut ticks = Vec::new();
    while ticks.len() < 3 {
        match sys.events().recv_timeout(Duration::from_secs(3)) {
            Ok(SysEvent::Clock(t)) => ticks.push(t),
            Ok(e) => panic!("unexpected {e:?}"),
            Err(_) => break,
        }
    }
    // the start tick plus two boundary ticks
    assert_eq!(ticks.len(), 3, "{ticks:?}");
    assert!(ticks[2].unix_s > ticks[1].unix_s, "{ticks:?}");
    for t in &ticks {
        assert!(
            t.hour < 24 && t.minute < 60 && t.band == nkw_sys::clock::band_of(t.hour, t.minute)
        );
    }
    // wake runs right after each send, on the clock thread
    let end = Instant::now() + Duration::from_secs(1);
    while wakes.load(Ordering::SeqCst) < 3 && Instant::now() < end {
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(wakes.load(Ordering::SeqCst) >= 3);
    let t0 = Instant::now();
    sys.shutdown();
    assert!(
        t0.elapsed() < Duration::from_millis(500),
        "shutdown took {:?}",
        t0.elapsed()
    );
    let n = wakes.load(Ordering::SeqCst);
    std::thread::sleep(Duration::from_millis(1200));
    assert_eq!(wakes.load(Ordering::SeqCst), n, "no ticks after shutdown");
}
