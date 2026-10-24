//! `idle::Inhibitor` against a fake `org.freedesktop.portal.Inhibit` on a
//! private bus (never the user's session bus, never the real portal).

mod common;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use common::*;
use nkw_sys::idle::{FLAG_IDLE, Inhibitor, State};
use zbus::zvariant::{OwnedObjectPath, OwnedValue};

const REQ: &str = "/org/freedesktop/portal/desktop/request/1_1/nkw";

struct FakeInhibit {
    calls: Arc<Mutex<Vec<String>>>,
}

#[zbus::interface(name = "org.freedesktop.portal.Inhibit")]
impl FakeInhibit {
    fn inhibit(
        &self,
        window: &str,
        flags: u32,
        options: HashMap<String, OwnedValue>,
    ) -> OwnedObjectPath {
        let reason = options
            .get("reason")
            .and_then(|v| String::try_from(v.clone()).ok())
            .unwrap_or_default();
        self.calls
            .lock()
            .unwrap()
            .push(format!("inhibit {window:?} {flags} {reason}"));
        OwnedObjectPath::try_from(REQ).unwrap()
    }
}

struct FakeRequest {
    calls: Arc<Mutex<Vec<String>>>,
}

#[zbus::interface(name = "org.freedesktop.portal.Request")]
impl FakeRequest {
    fn close(&self) {
        self.calls.lock().unwrap().push("close".into());
    }
}

fn until(f: impl Fn() -> bool) -> bool {
    let end = Instant::now() + Duration::from_secs(5);
    while Instant::now() < end {
        if f() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    false
}

#[test]
fn inhibits_with_the_idle_flag_and_closes_the_request() {
    let Some(bus) = private_bus() else { return };
    let calls = Arc::new(Mutex::new(Vec::new()));
    let _portal = conn(&bus.addr)
        .name("org.freedesktop.portal.Desktop")
        .unwrap()
        .serve_at(
            "/org/freedesktop/portal/desktop",
            FakeInhibit {
                calls: calls.clone(),
            },
        )
        .unwrap()
        .serve_at(
            REQ,
            FakeRequest {
                calls: calls.clone(),
            },
        )
        .unwrap()
        .build()
        .unwrap();
    let mut i = Inhibitor::new(Some(bus.addr.clone()));
    i.set(true);
    assert!(
        until(|| i.state() == State::On(REQ.into())),
        "{:?}",
        i.state()
    );
    i.set(false);
    assert!(until(|| i.state() == State::Off));
    i.shutdown();
    let got = calls.lock().unwrap().clone();
    assert_eq!(
        got,
        [
            format!("inhibit \"\" {FLAG_IDLE} um app no mundo pede a tela acesa"),
            "close".to_string()
        ]
    );
}

#[test]
fn without_a_portal_it_is_unavailable_and_quiet() {
    let Some(bus) = private_bus() else { return };
    let mut i = Inhibitor::new(Some(bus.addr.clone()));
    i.set(true);
    assert!(until(|| matches!(i.state(), State::Unavailable(_))));
    i.set(false);
    i.set(true);
    assert!(matches!(i.state(), State::Unavailable(_)));
}
