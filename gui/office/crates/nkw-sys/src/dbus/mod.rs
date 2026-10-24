//! D-Bus adapters: one thread and one `zbus::blocking::Connection` per bus (session: MPRIS; system:
//! UPower, NetworkManager), one file per interface. Each thread runs its adapters as futures and
//! sleeps until the bus has a signal; a socket pair is its doorbell for commands and for stop.

pub(crate) mod mpris;
pub(crate) mod networkmanager;
pub(crate) mod upower;

use std::cell::RefCell;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::sync::Arc;
use std::time::Duration;

use async_io::Async;
use crossbeam_channel::Receiver;
use futures_util::future::{self, Either};
use zbus::zvariant::{OwnedValue, Value};

use crate::{MediaControl, Out, SysOpts, Worker};

pub(crate) type Props = HashMap<String, OwnedValue>;

/// Strips nested variants (`a{sv}` values arrive boxed).
pub(crate) fn peel<'a>(v: &'a Value<'a>) -> &'a Value<'a> {
    match v {
        Value::Value(b) => peel(b),
        o => o,
    }
}

pub(crate) fn v_str<'a>(v: &'a Value<'a>) -> Option<&'a str> {
    match peel(v) {
        Value::Str(s) => Some(s.as_str()),
        Value::ObjectPath(p) => Some(p.as_str()),
        _ => None,
    }
}

pub(crate) fn v_i64(v: &Value<'_>) -> Option<i64> {
    match peel(v) {
        Value::I64(x) => Some(*x),
        Value::U64(x) => i64::try_from(*x).ok(),
        Value::I32(x) => Some(i64::from(*x)),
        Value::U32(x) => Some(i64::from(*x)),
        Value::I16(x) => Some(i64::from(*x)),
        Value::U16(x) => Some(i64::from(*x)),
        Value::U8(x) => Some(i64::from(*x)),
        _ => None,
    }
}

pub(crate) fn v_f64(v: &Value<'_>) -> Option<f64> {
    match peel(v) {
        Value::F64(x) => Some(*x),
        o => v_i64(o).map(|x| x as f64),
    }
}

pub(crate) fn v_bool(v: &Value<'_>) -> Option<bool> {
    match peel(v) {
        Value::Bool(b) => Some(*b),
        _ => None,
    }
}

pub(crate) fn v_strs(v: &Value<'_>) -> Vec<String> {
    match peel(v) {
        Value::Array(a) => a
            .iter()
            .filter_map(|x| v_str(x).map(str::to_string))
            .collect(),
        o => v_str(o).map(|s| vec![s.to_string()]).unwrap_or_default(),
    }
}

pub(crate) fn get<'a>(p: &'a Props, k: &str) -> Option<&'a Value<'static>> {
    p.get(k).map(|v| &**v)
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum Cmd {
    Stop,
    PlayPause,
    Next,
    Previous,
    Volume(f64),
}

/// The caller's side of a bus thread: queue a command, ring the socket.
pub(crate) struct Bell {
    q: crossbeam_channel::Sender<Cmd>,
    w: UnixStream,
}

impl Bell {
    pub(crate) fn send(&self, c: Cmd) {
        let _ = self.q.send(c);
        let _ = (&self.w).write(&[1]);
    }
}

fn bell() -> std::io::Result<(Arc<Bell>, UnixStream, Receiver<Cmd>)> {
    let (w, r) = UnixStream::pair()?;
    w.set_nonblocking(true)?;
    let (q, rx) = crossbeam_channel::unbounded();
    Ok((Arc::new(Bell { q, w }), r, rx))
}

#[derive(Clone, Copy)]
enum Bus {
    Session,
    System,
}

/// Connects a local socket ourselves: zbus would do it on the `blocking` pool, whose idle thread
/// (blocking 1.7) never exits and wakes every 500 ms, forever.
fn unix_connect(addr: &zbus::Address) -> std::io::Result<Option<UnixStream>> {
    use std::os::linux::net::SocketAddrExt;
    use zbus::address::transport::{Transport, UnixSocket};
    let Transport::Unix(u) = addr.transport() else {
        return Ok(None);
    };
    match u.path() {
        UnixSocket::File(p) => UnixStream::connect(p).map(Some),
        UnixSocket::Abstract(name) => {
            let sa = std::os::unix::net::SocketAddr::from_abstract_name(name.as_encoded_bytes())?;
            UnixStream::connect_addr(&sa).map(Some)
        }
        _ => Ok(None),
    }
}

/// The session bus (or `over`), as the adapters connect: for `idle`.
pub(crate) fn connect_session(over: Option<&str>) -> zbus::Result<zbus::blocking::Connection> {
    connect(Bus::Session, over)
}

fn connect(bus: Bus, over: Option<&str>) -> zbus::Result<zbus::blocking::Connection> {
    use std::str::FromStr;
    use zbus::blocking::connection::Builder;
    let addr = match (over, bus) {
        (Some(a), _) => zbus::Address::from_str(a)?,
        (None, Bus::Session) => zbus::Address::session()?,
        (None, Bus::System) => zbus::Address::system()?,
    };
    let b = match unix_connect(&addr)? {
        Some(s) => Builder::async_io_unix_stream(s),
        None => Builder::address(addr)?,
    };
    // a hung player must not wedge the thread for zbus's default 25 s
    b.method_timeout(Duration::from_secs(3)).build()
}

/// Waits on the doorbell; hands every command to `on` and returns on `Stop` or a closed bell.
async fn control(ctl: Async<UnixStream>, q: Receiver<Cmd>, mut on: impl AsyncFnMut(Cmd)) {
    loop {
        if ctl.readable().await.is_err() {
            return;
        }
        let mut buf = [0u8; 64];
        let mut r: &UnixStream = ctl.get_ref();
        match r.read(&mut buf) {
            Ok(0) => return,
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(_) => return,
        }
        while let Ok(c) = q.try_recv() {
            match c {
                Cmd::Stop => return,
                c => on(c).await,
            }
        }
    }
}

/// An adapter that fails reports once and then idles, so its siblings on the bus keep running.
pub(crate) async fn report(out: &Out, source: &'static str, r: zbus::Result<()>) {
    if let Err(e) = r {
        out.error(source, e);
    }
    future::pending::<()>().await
}

struct NoMedia;
impl MediaControl for NoMedia {
    fn play_pause(&self) {}
    fn next(&self) {}
    fn previous(&self) {}
    fn set_volume(&self, _: f32) {}
}

struct Mpris(Arc<Bell>);
impl MediaControl for Mpris {
    fn play_pause(&self) {
        self.0.send(Cmd::PlayPause)
    }
    fn next(&self) {
        self.0.send(Cmd::Next)
    }
    fn previous(&self) {
        self.0.send(Cmd::Previous)
    }
    fn set_volume(&self, v: f32) {
        self.0.send(Cmd::Volume(f64::from(v.clamp(0.0, 1.0))))
    }
}

fn spawn_bus<F>(name: &str, bell: Arc<Bell>, ctl: UnixStream, body: F) -> std::io::Result<Worker>
where
    F: FnOnce(UnixStream) + Send + 'static,
{
    let handle = std::thread::Builder::new()
        .name(name.into())
        .spawn(move || body(ctl))?;
    Ok(Worker {
        stop: Box::new(move || bell.send(Cmd::Stop)),
        handle,
    })
}

pub(crate) fn start_session(opts: &SysOpts, out: &Out) -> (Arc<dyn MediaControl>, Option<Worker>) {
    if !(opts.session_bus && opts.mpris) {
        return (Arc::new(NoMedia), None);
    }
    let (bell, ctl, rx) = match bell() {
        Ok(b) => b,
        Err(e) => {
            out.error("mpris", e);
            return (Arc::new(NoMedia), None);
        }
    };
    let over = opts.bus_override.clone();
    let out2 = out.clone();
    let w = spawn_bus("nkw-sys-session", bell.clone(), ctl, move |ctl| {
        let out = out2;
        let conn = match connect(Bus::Session, over.as_deref()) {
            Ok(c) => c,
            Err(e) => return out.error("mpris", e),
        };
        let Ok(ctl) = Async::new(ctl) else {
            return out.error("mpris", "doorbell");
        };
        let conn = conn.inner().clone();
        let active: RefCell<Option<String>> = RefCell::new(None);
        async_io::block_on(async {
            let work =
                async { report(&out, "mpris", mpris::run(&conn, &out, &active).await).await };
            let ctl = control(ctl, rx, async |c| {
                let name = active.borrow().clone();
                if let Some(name) = name {
                    let _ = mpris::command(&conn, &name, c).await;
                }
            });
            futures_util::pin_mut!(work, ctl);
            let _ = future::select(work, ctl).await;
        });
    });
    match w {
        Ok(w) => (Arc::new(Mpris(bell)), Some(w)),
        Err(e) => {
            out.error("mpris", e);
            (Arc::new(NoMedia), None)
        }
    }
}

pub(crate) fn start_system(opts: &SysOpts, out: &Out) -> Option<Worker> {
    if !(opts.system_bus && (opts.upower || opts.nm)) {
        return None;
    }
    let (bell, ctl, rx) = match bell() {
        Ok(b) => b,
        Err(e) => {
            out.error("system-bus", e);
            return None;
        }
    };
    let (upower, nm) = (opts.upower, opts.nm);
    let over = opts.bus_override.clone();
    let out2 = out.clone();
    let w = spawn_bus("nkw-sys-system", bell, ctl, move |ctl| {
        let out = out2;
        let conn = match connect(Bus::System, over.as_deref()) {
            Ok(c) => c,
            Err(e) => {
                // one Error per adapter that could not start
                if upower {
                    out.error("upower", &e);
                }
                if nm {
                    out.error("nm", &e);
                }
                return;
            }
        };
        let Ok(ctl) = Async::new(ctl) else {
            return out.error("system-bus", "doorbell");
        };
        let conn = conn.inner().clone();
        async_io::block_on(async {
            let up = async {
                if upower {
                    report(&out, "upower", upower::run(&conn, &out).await).await
                } else {
                    future::pending().await
                }
            };
            let net = async {
                if nm {
                    report(&out, "nm", networkmanager::run(&conn, &out).await).await
                } else {
                    future::pending().await
                }
            };
            let work = future::join(up, net);
            let ctl = control(ctl, rx, async |_| {});
            futures_util::pin_mut!(work, ctl);
            let _ = future::select(work, ctl).await;
        });
    });
    match w {
        Ok(w) => Some(w),
        Err(e) => {
            out.error("system-bus", e);
            None
        }
    }
}

const PROPS_IFACE: &str = "org.freedesktop.DBus.Properties";

/// A `PropertiesChanged` subscription, narrowed by whatever is given.
pub(crate) async fn props_stream(
    conn: &zbus::Connection,
    sender: Option<&str>,
    path: Option<&str>,
    iface: Option<&str>,
) -> zbus::Result<zbus::MessageStream> {
    let mut b = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .interface(PROPS_IFACE)?
        .member("PropertiesChanged")?;
    if let Some(s) = sender {
        b = b.sender(s)?;
    }
    if let Some(p) = path {
        b = b.path(p)?;
    }
    if let Some(i) = iface {
        b = b.arg(0, i)?;
    }
    zbus::MessageStream::for_match_rule(b.build(), conn, None).await
}

/// `(interface, changed, invalidated)` of a `PropertiesChanged` signal.
pub(crate) fn decode_changed(msg: &zbus::Message) -> Option<(String, Props, Vec<String>)> {
    msg.body()
        .deserialize::<(String, Props, Vec<String>)>()
        .ok()
}

pub(crate) async fn get_all(
    conn: &zbus::Connection,
    dest: &str,
    path: &str,
    iface: &str,
) -> zbus::Result<Props> {
    let reply = conn
        .call_method(Some(dest), path, Some(PROPS_IFACE), "GetAll", &(iface,))
        .await?;
    reply.body().deserialize::<Props>()
}

pub(crate) async fn get_prop(
    conn: &zbus::Connection,
    dest: &str,
    path: &str,
    iface: &str,
    name: &str,
) -> zbus::Result<OwnedValue> {
    let reply = conn
        .call_method(Some(dest), path, Some(PROPS_IFACE), "Get", &(iface, name))
        .await?;
    reply.body().deserialize::<OwnedValue>()
}

#[cfg(test)]
pub(crate) fn props(pairs: Vec<(&str, Value<'_>)>) -> Props {
    pairs
        .into_iter()
        .map(|(k, v)| (k.to_string(), OwnedValue::try_from(v).expect("owned value")))
        .collect()
}

/// The next item of whichever stream yields first.
pub(crate) async fn next_of<A, B>(a: &mut A, b: &mut B) -> Either<Option<A::Item>, Option<B::Item>>
where
    A: futures_util::Stream + Unpin,
    B: futures_util::Stream + Unpin,
{
    use futures_util::StreamExt;
    match future::select(a.next(), b.next()).await {
        Either::Left((x, _)) => Either::Left(x),
        Either::Right((x, _)) => Either::Right(x),
    }
}
