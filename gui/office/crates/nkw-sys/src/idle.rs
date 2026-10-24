//! Idle inhibition forwarded to the host (S25 stretch): while a client of the
//! world asks the screen to stay on (`zwp_idle_inhibit_manager_v1`), the
//! session is asked through `org.freedesktop.portal.Inhibit` (flag 8, idle);
//! when it stops, the portal's request is closed.
//!
//! One thread, started on the first `set(true)`; it blocks on its channel and
//! never polls. Without a portal (no session bus, no xdg-desktop-portal) the
//! first call fails, the state becomes `Unavailable` and every later `set` is
//! ignored: the caller logs that once.

use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use crossbeam_channel::{Receiver, Sender};

/// `org.freedesktop.portal.Inhibit` flags: 1 logout, 2 user switch, 4
/// suspend, 8 idle.
pub const FLAG_IDLE: u32 = 8;
pub const PORTAL_DEST: &str = "org.freedesktop.portal.Desktop";
pub const PORTAL_PATH: &str = "/org/freedesktop/portal/desktop";
pub const PORTAL_IFACE: &str = "org.freedesktop.portal.Inhibit";
pub const REQUEST_IFACE: &str = "org.freedesktop.portal.Request";
/// The reason the portal may show (pt-BR, never an app's name or title).
pub const REASON: &str = "um app no mundo pede a tela acesa";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum State {
    /// nothing asked of the portal
    Off,
    /// the portal holds our request (its handle path)
    On(String),
    /// the portal failed once: `set` is ignored from now on
    Unavailable(String),
}

/// What the machine wants done on the bus.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Call {
    Inhibit,
    Close(String),
}

/// The pure part: given what is wanted and what is held, the one call to
/// make (or none). `done` feeds the call's result back.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Machine {
    pub state: State,
    pub want: bool,
}

impl Default for Machine {
    fn default() -> Machine {
        Machine {
            state: State::Off,
            want: false,
        }
    }
}

impl Machine {
    pub fn want(&mut self, on: bool) -> Option<Call> {
        self.want = on;
        self.next()
    }

    fn next(&self) -> Option<Call> {
        match (&self.state, self.want) {
            (State::Off, true) => Some(Call::Inhibit),
            (State::On(h), false) => Some(Call::Close(h.clone())),
            _ => None,
        }
    }

    /// The result of `c`; returns the follow-up call (the wish changed while
    /// the call ran: inhibit then close at once, say).
    pub fn done(&mut self, c: Call, r: Result<String, String>) -> Option<Call> {
        self.state = match (c, r) {
            (Call::Inhibit, Ok(handle)) => State::On(handle),
            // closing failed: the portal dropped it already (its sender
            // vanished, say); either way we hold nothing
            (Call::Close(_), _) => State::Off,
            (Call::Inhibit, Err(e)) => State::Unavailable(e),
        };
        self.next()
    }
}

/// The bus side, behind a trait so the machine runs on a fake in tests.
pub trait Portal: Send {
    /// `Inhibit("", FLAG_IDLE, {reason})` → the request's handle path.
    fn inhibit(&mut self) -> Result<String, String>;
    /// `Request.Close()` on `handle`.
    fn close(&mut self, handle: &str) -> Result<(), String>;
}

/// The session bus's portal (connected on the first call).
pub struct BusPortal {
    over: Option<String>,
    conn: Option<zbus::blocking::Connection>,
}

impl BusPortal {
    /// `over`: a bus address (tests' private bus) instead of the session's.
    pub fn new(over: Option<String>) -> BusPortal {
        BusPortal { over, conn: None }
    }

    fn conn(&mut self) -> Result<&zbus::blocking::Connection, String> {
        if self.conn.is_none() {
            let c =
                crate::dbus::connect_session(self.over.as_deref()).map_err(|e| e.to_string())?;
            self.conn = Some(c);
        }
        self.conn
            .as_ref()
            .ok_or_else(|| "sem barramento".to_string())
    }
}

impl Portal for BusPortal {
    fn inhibit(&mut self) -> Result<String, String> {
        use std::collections::HashMap;
        use zbus::zvariant::{OwnedObjectPath, Value};
        let conn = self.conn()?;
        let mut opts: HashMap<&str, Value> = HashMap::new();
        opts.insert("reason", Value::from(REASON));
        let reply = conn
            .call_method(
                Some(PORTAL_DEST),
                PORTAL_PATH,
                Some(PORTAL_IFACE),
                "Inhibit",
                &("", FLAG_IDLE, opts),
            )
            .map_err(|e| e.to_string())?;
        let path: OwnedObjectPath = reply.body().deserialize().map_err(|e| e.to_string())?;
        Ok(path.as_str().to_string())
    }

    fn close(&mut self, handle: &str) -> Result<(), String> {
        let conn = self.conn()?;
        conn.call_method(Some(PORTAL_DEST), handle, Some(REQUEST_IFACE), "Close", &())
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
}

/// The inhibitor: `set` is cheap and never blocks (a channel send); the
/// thread makes the calls.
pub struct Inhibitor {
    tx: Option<Sender<bool>>,
    shared: Arc<Mutex<Machine>>,
    make: Option<Box<dyn FnOnce() -> Box<dyn Portal> + Send>>,
    thread: Option<JoinHandle<()>>,
    last: bool,
}

impl Inhibitor {
    /// On the session bus's portal (or `over`'s).
    pub fn new(over: Option<String>) -> Inhibitor {
        Inhibitor::with(move || Box::new(BusPortal::new(over)))
    }

    /// On any portal (tests).
    pub fn with(make: impl FnOnce() -> Box<dyn Portal> + Send + 'static) -> Inhibitor {
        Inhibitor {
            tx: None,
            shared: Arc::new(Mutex::new(Machine::default())),
            make: Some(Box::new(make)),
            thread: None,
            last: false,
        }
    }

    /// Ask (or stop asking) for the screen to stay on. The thread starts
    /// on the first `true`; repeated values do nothing.
    pub fn set(&mut self, on: bool) {
        if on == self.last {
            return;
        }
        self.last = on;
        if self.tx.is_none() {
            if !on {
                return;
            }
            let Some(make) = self.make.take() else { return };
            let (tx, rx) = crossbeam_channel::unbounded();
            let shared = self.shared.clone();
            let t = std::thread::Builder::new()
                .name("nkw-inhibit".into())
                .spawn(move || run(make(), rx, shared));
            match t {
                Ok(t) => self.thread = Some(t),
                Err(e) => {
                    if let Ok(mut m) = self.shared.lock() {
                        m.state = State::Unavailable(e.to_string());
                    }
                    return;
                }
            }
            self.tx = Some(tx);
        }
        if let Some(tx) = &self.tx {
            let _ = tx.send(on);
        }
    }

    /// What the portal holds now (the thread's view).
    pub fn state(&self) -> State {
        self.shared
            .lock()
            .map(|m| m.state.clone())
            .unwrap_or(State::Off)
    }

    /// What was asked last.
    pub fn wanted(&self) -> bool {
        self.last
    }

    /// Releases the request and joins the thread.
    pub fn shutdown(&mut self) {
        self.tx = None;
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl Drop for Inhibitor {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn run(mut p: Box<dyn Portal>, rx: Receiver<bool>, shared: Arc<Mutex<Machine>>) {
    let call = |p: &mut Box<dyn Portal>, c: &Call| match c {
        Call::Inhibit => p.inhibit(),
        Call::Close(h) => p.close(h).map(|_| String::new()),
    };
    let step = |p: &mut Box<dyn Portal>, first: Option<Call>| {
        let mut next = first;
        while let Some(c) = next {
            let r = call(p, &c);
            next = match shared.lock() {
                Ok(mut m) => m.done(c, r),
                Err(_) => None,
            };
        }
    };
    // a closed channel (shutdown) releases what is held
    while let Ok(on) = rx.recv() {
        let first = match shared.lock() {
            Ok(mut m) => m.want(on),
            Err(_) => return,
        };
        step(&mut p, first);
    }
    let first = match shared.lock() {
        Ok(mut m) => m.want(false),
        Err(_) => return,
    };
    step(&mut p, first);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn the_machine_asks_once_and_closes_once() {
        let mut m = Machine::default();
        assert_eq!(m.want(false), None, "nothing held, nothing to close");
        assert_eq!(m.want(true), Some(Call::Inhibit));
        assert_eq!(m.done(Call::Inhibit, Ok("/r/1".into())), None);
        assert_eq!(m.state, State::On("/r/1".into()));
        assert_eq!(m.want(true), None, "already held");
        assert_eq!(m.want(false), Some(Call::Close("/r/1".into())));
        assert_eq!(m.done(Call::Close("/r/1".into()), Ok(String::new())), None);
        assert_eq!(m.state, State::Off);
    }

    #[test]
    fn a_wish_that_changed_during_the_call_is_followed() {
        let mut m = Machine::default();
        assert_eq!(m.want(true), Some(Call::Inhibit));
        m.want = false; // the client let go while the portal answered
        assert_eq!(
            m.done(Call::Inhibit, Ok("/r/2".into())),
            Some(Call::Close("/r/2".into()))
        );
        // a failed close holds nothing either
        assert_eq!(m.done(Call::Close("/r/2".into()), Err("gone".into())), None);
        assert_eq!(m.state, State::Off);
    }

    #[test]
    fn without_a_portal_it_gives_up_for_good() {
        let mut m = Machine::default();
        m.want(true);
        assert_eq!(m.done(Call::Inhibit, Err("no portal".into())), None);
        assert_eq!(m.state, State::Unavailable("no portal".into()));
        assert_eq!(m.want(false), None);
        assert_eq!(m.want(true), None, "never retried");
    }

    #[derive(Clone, Default)]
    struct Fake(Arc<Mutex<Vec<String>>>);

    impl Portal for Fake {
        fn inhibit(&mut self) -> Result<String, String> {
            let mut l = self.0.lock().unwrap();
            l.push("inhibit".into());
            Ok(format!("/r/{}", l.len()))
        }
        fn close(&mut self, h: &str) -> Result<(), String> {
            self.0.lock().unwrap().push(format!("close {h}"));
            Ok(())
        }
    }

    fn until(f: impl Fn() -> bool) -> bool {
        let end = Instant::now() + Duration::from_secs(5);
        while Instant::now() < end {
            if f() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        false
    }

    #[test]
    fn the_thread_starts_on_the_first_true_and_releases_on_shutdown() {
        let fake = Fake::default();
        let log = fake.0.clone();
        let mut i = Inhibitor::with(move || Box::new(fake));
        i.set(false);
        assert!(i.thread.is_none(), "no thread before it is needed");
        i.set(true);
        i.set(true);
        assert!(until(|| matches!(i.state(), State::On(_))));
        i.set(false);
        assert!(until(|| i.state() == State::Off));
        i.set(true);
        assert!(until(|| matches!(i.state(), State::On(_))));
        i.shutdown();
        assert_eq!(i.state(), State::Off);
        assert_eq!(
            *log.lock().unwrap(),
            ["inhibit", "close /r/1", "inhibit", "close /r/3"]
        );
    }
}
