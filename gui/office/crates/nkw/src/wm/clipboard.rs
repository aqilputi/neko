//! The clipboard bridge to the host (§4.6). Inside the world the selection
//! already works through Smithay's data device; this connects it to the
//! host's: a thread with `wayland-client` on the host's `WAYLAND_DISPLAY`
//! (the process env: children get the world's socket only) speaking
//! `zwlr_data_control_v1` (niri, sway …).
//!
//! - host selection → our seat's selection, same mimes; a client of ours
//!   that pastes gets the host's bytes through the fd it handed us;
//! - our client's selection → the host's, same mimes; a host app that
//!   pastes gets our client's bytes the same way.
//!
//! Content is never read here, never logged, never kept: only fds pass.
//! Thread ↔ loop: `calloop::channel` one way, an mpsc + eventfd the other.

use std::os::fd::{AsFd, AsRawFd, FromRawFd, OwnedFd};
use std::sync::mpsc;

use wayland_client::globals::{GlobalListContents, registry_queue_init};
use wayland_client::protocol::{wl_registry, wl_seat};
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle, event_created_child};
use wayland_protocols_wlr::data_control::v1::client::{
    zwlr_data_control_device_v1::{self, ZwlrDataControlDeviceV1},
    zwlr_data_control_manager_v1::ZwlrDataControlManagerV1,
    zwlr_data_control_offer_v1::{self, ZwlrDataControlOfferV1},
    zwlr_data_control_source_v1::{self, ZwlrDataControlSourceV1},
};

/// Loop → thread.
#[derive(Debug)]
pub enum ToHost {
    /// a client of ours set the selection with these mimes
    Offer(Vec<String>),
    /// a client of ours pastes the host's selection: write `mime` into `fd`
    Receive {
        mime: String,
        fd: OwnedFd,
    },
    Quit,
}

/// Thread → loop.
#[derive(Debug)]
pub enum FromHost {
    /// the bridge is up (or failed: `false`)
    Up(bool),
    /// the host's selection changed (not ours): its mimes
    Selection(Vec<String>),
    /// the host's selection went away
    Cleared,
    /// a host app pastes ours: our client writes `mime` into `fd`
    Send { mime: String, fd: OwnedFd },
}

/// What the loop knows of the bridge (for `dump`: no content, ever).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Status {
    pub host: bool,
    /// mimes of the selection last mirrored either way
    pub mimes: usize,
    /// whether the seat's selection is the host's mirror
    pub mirrored: bool,
}

/// The loop's end of the bridge.
pub struct Bridge {
    tx: mpsc::Sender<ToHost>,
    wake: OwnedFd,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Bridge {
    pub fn send(&self, m: ToHost) {
        if self.tx.send(m).is_ok() {
            ping(&self.wake);
        }
    }

    /// Stop the thread (bounded: it wakes at once).
    pub fn shutdown(&mut self) {
        self.send(ToHost::Quit);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

fn ping(fd: &OwnedFd) {
    let one: u64 = 1;
    // SAFETY: writing 8 bytes from a local to an eventfd we own
    unsafe { libc::write(fd.as_raw_fd(), (&one as *const u64).cast(), 8) };
}

fn eventfd() -> Option<OwnedFd> {
    // SAFETY: plain libc call; the fd is owned right away
    let fd = unsafe { libc::eventfd(0, libc::EFD_CLOEXEC | libc::EFD_NONBLOCK) };
    // SAFETY: a fresh fd we own
    (fd >= 0).then(|| unsafe { OwnedFd::from_raw_fd(fd) })
}

/// Start the bridge thread; `out` reaches the loop. `None`: no host display.
pub fn start(out: calloop::channel::Sender<FromHost>) -> Option<Bridge> {
    std::env::var_os("WAYLAND_DISPLAY")?;
    let wake = eventfd()?;
    let theirs = wake.try_clone().ok()?;
    let (tx, rx) = mpsc::channel();
    let thread = std::thread::Builder::new()
        .name("nkw-clipboard".into())
        .spawn(move || {
            if let Err(e) = run(rx, theirs, out.clone()) {
                tracing::info!("clipboard bridge: {e}");
                let _ = out.send(FromHost::Up(false));
            }
        })
        .ok()?;
    Some(Bridge {
        tx,
        wake,
        thread: Some(thread),
    })
}

struct State {
    out: calloop::channel::Sender<FromHost>,
    /// offers announced but not yet a selection, with their mimes
    offers: Vec<(ZwlrDataControlOfferV1, Vec<String>)>,
    /// the host's current selection
    current: Option<ZwlrDataControlOfferV1>,
    /// a selection event arrived in this batch (decided after it)
    pending: Option<Option<ZwlrDataControlOfferV1>>,
    /// our own source on the host, while it is the selection
    ours: Option<ZwlrDataControlSourceV1>,
    finished: bool,
}

fn run(
    rx: mpsc::Receiver<ToHost>,
    wake: OwnedFd,
    out: calloop::channel::Sender<FromHost>,
) -> Result<(), String> {
    let conn = Connection::connect_to_env().map_err(|e| e.to_string())?;
    let (globals, mut queue) = registry_queue_init::<State>(&conn).map_err(|e| e.to_string())?;
    let qh = queue.handle();
    let seat: wl_seat::WlSeat = globals.bind(&qh, 1..=1, ()).map_err(|e| e.to_string())?;
    let manager: ZwlrDataControlManagerV1 = globals
        .bind(&qh, 1..=2, ())
        .map_err(|_| "no zwlr_data_control_manager_v1 on the host".to_string())?;
    let device = manager.get_data_device(&seat, &qh, ());
    let mut st = State {
        out,
        offers: Vec::new(),
        current: None,
        pending: None,
        ours: None,
        finished: false,
    };
    let _ = st.out.send(FromHost::Up(true));
    loop {
        queue.dispatch_pending(&mut st).map_err(|e| e.to_string())?;
        st.settle();
        if st.finished {
            return Err("the host ended data control".into());
        }
        conn.flush().map_err(|e| e.to_string())?;
        let Some(guard) = queue.prepare_read() else {
            continue;
        };
        let mut fds = [
            libc::pollfd {
                fd: guard.connection_fd().as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            },
            libc::pollfd {
                fd: wake.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            },
        ];
        // SAFETY: two valid pollfds on the stack
        let n = unsafe { libc::poll(fds.as_mut_ptr(), 2, -1) };
        if n < 0 {
            drop(guard);
            continue;
        }
        if fds[0].revents != 0 {
            guard.read().map_err(|e| e.to_string())?;
        } else {
            drop(guard);
        }
        if fds[1].revents != 0 {
            let mut buf = [0u8; 8];
            // SAFETY: draining the eventfd into a local
            unsafe { libc::read(wake.as_raw_fd(), buf.as_mut_ptr().cast(), 8) };
        }
        while let Ok(m) = rx.try_recv() {
            match m {
                ToHost::Quit => return Ok(()),
                ToHost::Offer(mimes) => {
                    if let Some(old) = st.ours.take() {
                        old.destroy();
                    }
                    if mimes.is_empty() {
                        device.set_selection(None);
                        continue;
                    }
                    let src = manager.create_data_source(&qh, ());
                    for m in mimes {
                        src.offer(m);
                    }
                    device.set_selection(Some(&src));
                    st.ours = Some(src);
                }
                ToHost::Receive { mime, fd } => {
                    if let Some(o) = &st.current {
                        o.receive(mime, fd.as_fd());
                    }
                    // our copy of the fd closes here: the host app writes,
                    // our client reads until EOF
                }
            }
        }
    }
}

impl State {
    /// After a batch: a new host selection that is not our own echo goes
    /// to the loop.
    fn settle(&mut self) {
        let Some(sel) = self.pending.take() else {
            return;
        };
        let mimes = sel.as_ref().and_then(|o| {
            let i = self.offers.iter().position(|(x, _)| x == o)?;
            Some(self.offers.remove(i).1)
        });
        // offers that never became the selection
        for (o, _) in self.offers.drain(..) {
            o.destroy();
        }
        if let Some(old) = self.current.take() {
            old.destroy();
        }
        self.current = sel;
        if self.ours.is_some() {
            // our own selection coming back
            return;
        }
        let _ = self.out.send(match mimes {
            Some(m) if !m.is_empty() => FromHost::Selection(m),
            _ => FromHost::Cleared,
        });
    }
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for State {
    fn event(
        _: &mut State,
        _: &wl_registry::WlRegistry,
        _: wl_registry::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<State>,
    ) {
    }
}

impl Dispatch<wl_seat::WlSeat, ()> for State {
    fn event(
        _: &mut State,
        _: &wl_seat::WlSeat,
        _: wl_seat::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<State>,
    ) {
    }
}

impl Dispatch<ZwlrDataControlManagerV1, ()> for State {
    fn event(
        _: &mut State,
        _: &ZwlrDataControlManagerV1,
        _: <ZwlrDataControlManagerV1 as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<State>,
    ) {
    }
}

impl Dispatch<ZwlrDataControlDeviceV1, ()> for State {
    fn event(
        st: &mut State,
        _: &ZwlrDataControlDeviceV1,
        ev: zwlr_data_control_device_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<State>,
    ) {
        use zwlr_data_control_device_v1::Event as E;
        match ev {
            E::DataOffer { id } => st.offers.push((id, Vec::new())),
            E::Selection { id } => st.pending = Some(id),
            E::Finished => st.finished = true,
            // the primary selection is not bridged: forget it
            E::PrimarySelection { id: Some(o) } => {
                st.offers.retain(|(x, _)| *x != o);
                o.destroy();
            }
            _ => {}
        }
    }

    event_created_child!(State, ZwlrDataControlDeviceV1, [
        zwlr_data_control_device_v1::EVT_DATA_OFFER_OPCODE => (ZwlrDataControlOfferV1, ()),
    ]);
}

impl Dispatch<ZwlrDataControlOfferV1, ()> for State {
    fn event(
        st: &mut State,
        offer: &ZwlrDataControlOfferV1,
        ev: zwlr_data_control_offer_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<State>,
    ) {
        if let zwlr_data_control_offer_v1::Event::Offer { mime_type } = ev
            && let Some((_, m)) = st.offers.iter_mut().find(|(o, _)| o == offer)
        {
            m.push(mime_type);
        }
    }
}

impl Dispatch<ZwlrDataControlSourceV1, ()> for State {
    fn event(
        st: &mut State,
        src: &ZwlrDataControlSourceV1,
        ev: zwlr_data_control_source_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<State>,
    ) {
        use zwlr_data_control_source_v1::Event as E;
        match ev {
            E::Send { mime_type, fd } => {
                let _ = st.out.send(FromHost::Send {
                    mime: mime_type,
                    fd,
                });
            }
            E::Cancelled => {
                if st.ours.as_ref() == Some(src) {
                    st.ours = None;
                }
                src.destroy();
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_host_display_no_bridge() {
        // the test process may run under a host; only the shape is checked
        let (tx, _rx) = calloop::channel::channel::<FromHost>();
        if std::env::var_os("WAYLAND_DISPLAY").is_none() {
            assert!(start(tx).is_none());
        }
        assert_eq!(
            Status::default(),
            Status {
                host: false,
                mimes: 0,
                mirrored: false
            }
        );
    }

    #[test]
    fn eventfd_pings() {
        let fd = eventfd().unwrap();
        ping(&fd);
        let mut buf = [0u8; 8];
        // SAFETY: reading the eventfd into a local
        let n = unsafe { libc::read(fd.as_raw_fd(), buf.as_mut_ptr().cast(), 8) };
        assert_eq!(n, 8);
        assert_eq!(u64::from_ne_bytes(buf), 1);
    }

    /// Copy in a host app (niri), paste in the world, and back: needs a real
    /// host session. `NKW_E2E=1 cargo test -p nkw clipboard -- --ignored`.
    #[test]
    #[ignore]
    fn host_bridge_comes_up_under_niri() {
        if std::env::var("NKW_E2E").as_deref() != Ok("1") {
            return;
        }
        let (tx, rx) = calloop::channel::channel::<FromHost>();
        let mut b = start(tx).expect("a host display");
        let mut el = calloop::EventLoop::<Vec<String>>::try_new().unwrap();
        el.handle()
            .insert_source(rx, |e, _, seen: &mut Vec<String>| {
                if let calloop::channel::Event::Msg(m) = e {
                    seen.push(format!("{m:?}").split('(').next().unwrap().to_string());
                }
            })
            .unwrap();
        let mut seen = Vec::new();
        el.dispatch(std::time::Duration::from_secs(2), &mut seen)
            .unwrap();
        assert_eq!(seen.first().map(String::as_str), Some("Up"), "{seen:?}");
        b.shutdown();
    }
}
