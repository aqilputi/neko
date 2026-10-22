//! A pane's WebSocket (`WS /v1/panes/{id}/ws`) on its own thread: binary
//! frames are raw PTY bytes both ways, text frames are JSON controls.
//!
//! The thread sleeps in `poll(2)` on the (non-blocking) socket and a
//! [`Bell`] the `PaneConn` rings on every write/resize/close: an idle pane
//! costs no wakeups, and a keystroke goes out at once.

use std::io;
use std::os::fd::AsRawFd;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use anyhow::{Context as _, Result, bail};
use crossbeam_channel::{Receiver, Sender, TryRecvError, unbounded};
use tungstenite::client::IntoClientRequest;
use tungstenite::protocol::CloseFrame;
use tungstenite::protocol::frame::coding::CloseCode;
use tungstenite::{Message, WebSocket};

use crate::http::{Conn, Endpoint, encode};
use crate::proto::Control;
use crate::wake::{Bell, poll};

/// What a pane sends.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PaneMsg {
    /// Raw PTY output: feed it to a `Term`.
    Data(Vec<u8>),
    /// The program's title changed.
    Title(String),
    /// The program exited (with its error, if any); `Closed` follows.
    Exit(Option<String>),
    /// The socket is gone; nothing else will arrive.
    Closed,
}

enum Out {
    Data(Vec<u8>),
    Resize(u16, u16),
    Close,
}

const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);
/// How long the goodbye (close frame) may take to go out.
const CLOSE_TIMEOUT: Duration = Duration::from_secs(1);
/// Reconnections after a close 1013 (the server asks to come back).
const RETRIES: u32 = 5;

/// An attached pane. Dropping it detaches (the pane keeps running).
pub struct PaneConn {
    id: String,
    reused: bool,
    tx: Sender<Out>,
    bell: Arc<Bell>,
    rx: Receiver<PaneMsg>,
    ep: Endpoint,
}

impl PaneConn {
    pub(crate) fn attach(ep: &Endpoint, id: &str, reused: bool) -> Result<PaneConn> {
        let ws = connect(ep, id)?;
        let bell = Arc::new(Bell::new().context("eventfd do painel")?);
        let (tx, out_rx) = unbounded();
        let (in_tx, rx) = unbounded();
        let (ep2, id2, bell2) = (ep.clone(), id.to_string(), bell.clone());
        thread::Builder::new()
            .name(format!("nk-pane-{id}"))
            .spawn(move || run(ws, &ep2, &id2, &out_rx, &in_tx, &bell2))
            .context("thread do painel")?;
        Ok(PaneConn { id: id.to_string(), reused, tx, bell, rx, ep: ep.clone() })
    }

    fn send(&self, o: Out) {
        if self.tx.send(o).is_ok() {
            self.bell.ring();
        }
    }

    /// The pane id (`pane_id` of `POST /v1/panes`).
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Whether the core reused a pane that was already open.
    pub fn reused(&self) -> bool {
        self.reused
    }

    /// Keyboard/paste bytes for the PTY.
    pub fn write(&self, bytes: &[u8]) {
        if !bytes.is_empty() {
            self.send(Out::Data(bytes.to_vec()));
        }
    }

    /// Resizes the PTY (last writer wins among viewers).
    pub fn resize(&self, cols: u16, rows: u16) {
        if cols > 0 && rows > 0 {
            self.send(Out::Resize(cols, rows));
        }
    }

    /// What the pane sends.
    pub fn rx(&self) -> &Receiver<PaneMsg> {
        &self.rx
    }

    /// Detaches; with `kill`, also closes the pane on the core (SIGHUP to
    /// the program).
    pub fn close(&self, kill: bool) {
        self.send(Out::Close);
        if kill {
            let path = format!("/v1/panes/{}", encode(&self.id));
            let _ = self.ep.call::<(), serde_json::Value>("DELETE", &path, None);
        }
    }
}

impl Drop for PaneConn {
    fn drop(&mut self) {
        self.send(Out::Close);
    }
}

fn connect(ep: &Endpoint, id: &str) -> Result<WebSocket<Conn>> {
    let conn = ep.connect().with_context(|| format!("conectando em {}", ep.addr))?;
    conn.set_read_timeout(Some(HANDSHAKE_TIMEOUT))?;
    let mut url = format!("ws://{}/v1/panes/{}/ws", ep.addr.host(), encode(id));
    if let Some(t) = &ep.token {
        url.push_str(&format!("?token={}", encode(t)));
    }
    let req = url.into_client_request()?;
    let ws = match tungstenite::client(req, conn) {
        Ok((ws, _)) => ws,
        Err(tungstenite::HandshakeError::Failure(tungstenite::Error::Http(resp))) => {
            let body = resp.body().as_deref().map(String::from_utf8_lossy).unwrap_or_default().to_string();
            crate::http::check(resp.status().as_u16(), &body)?;
            bail!("painel {id}: HTTP {}", resp.status())
        }
        Err(e) => bail!("painel {id}: {e}"),
    };
    ws.get_ref().set_read_timeout(None)?;
    ws.get_ref().set_nonblocking(true)?;
    Ok(ws)
}

fn run(mut ws: WebSocket<Conn>, ep: &Endpoint, id: &str, out: &Receiver<Out>, tx: &Sender<PaneMsg>, bell: &Bell) {
    let mut retries = 0;
    loop {
        match pump(&mut ws, out, tx, bell) {
            End::Detach => {
                // say goodbye with a bounded blocking write
                let s = ws.get_ref();
                let _ = s.set_nonblocking(false);
                let _ = s.set_write_timeout(Some(CLOSE_TIMEOUT));
                let _ = ws.close(None);
                let _ = ws.flush();
                let _ = tx.send(PaneMsg::Closed);
                return;
            }
            End::Again if retries < RETRIES => {
                retries += 1;
                thread::sleep(Duration::from_millis(200));
                match connect(ep, id) {
                    Ok(w) => ws = w,
                    Err(_) => {
                        let _ = tx.send(PaneMsg::Closed);
                        return;
                    }
                }
            }
            End::Again | End::Closed => {
                let _ = tx.send(PaneMsg::Closed);
                return;
            }
        }
    }
}

enum End {
    /// We were told to close (or the PaneConn is gone).
    Detach,
    /// The server closed with 1013: reconnect.
    Again,
    /// The socket ended.
    Closed,
}

fn would_block(e: &tungstenite::Error) -> bool {
    matches!(e, tungstenite::Error::Io(e) if e.kind() == io::ErrorKind::WouldBlock)
}

fn interrupted(e: &tungstenite::Error) -> bool {
    matches!(e, tungstenite::Error::Io(e) if e.kind() == io::ErrorKind::Interrupted)
}

/// Moves bytes both ways until the pane ends: queue what the `PaneConn`
/// sent, read every frame there is (tungstenite may hold whole frames in its
/// buffer, so read until the socket would block, never poll with data
/// pending), flush, then sleep until the socket or the bell has something.
fn pump(ws: &mut WebSocket<Conn>, out: &Receiver<Out>, tx: &Sender<PaneMsg>, bell: &Bell) -> End {
    loop {
        loop {
            let msg = match out.try_recv() {
                Ok(Out::Data(b)) => Message::Binary(b.into()),
                Ok(Out::Resize(cols, rows)) => {
                    let c =
                        Control { kind: "resize".into(), cols: cols.into(), rows: rows.into(), ..Control::default() };
                    Message::Text(serde_json::to_string(&c).unwrap_or_default().into())
                }
                Ok(Out::Close) | Err(TryRecvError::Disconnected) => return End::Detach,
                Err(TryRecvError::Empty) => break,
            };
            // WouldBlock here means "queued, the socket is full": flushed below
            match ws.write(msg) {
                Ok(()) => {}
                Err(e) if would_block(&e) => {}
                Err(_) => return End::Closed,
            }
        }
        loop {
            let sent = match ws.read() {
                Ok(Message::Binary(b)) => tx.send(PaneMsg::Data(b.to_vec())),
                Ok(Message::Text(t)) => match serde_json::from_str::<Control>(t.as_str()) {
                    Ok(c) if c.kind == "title" => tx.send(PaneMsg::Title(c.text)),
                    Ok(c) if c.kind == "exit" => tx.send(PaneMsg::Exit((!c.err.is_empty()).then_some(c.err))),
                    _ => Ok(()),
                },
                Ok(Message::Close(frame)) => {
                    let again = matches!(frame, Some(CloseFrame { code: CloseCode::Again, .. }));
                    // let tungstenite answer the close, then stop
                    let _ = ws.get_ref().set_nonblocking(false);
                    let _ = ws.get_ref().set_write_timeout(Some(CLOSE_TIMEOUT));
                    let _ = ws.flush();
                    return if again { End::Again } else { End::Closed };
                }
                Ok(_) => Ok(()), // ping/pong/raw frames (a pong reply is flushed below)
                Err(e) if would_block(&e) => break,
                Err(e) if interrupted(&e) => Ok(()),
                Err(_) => return End::Closed,
            };
            if sent.is_err() {
                return End::Detach;
            }
        }
        let pending = match ws.flush() {
            Ok(()) => false,
            Err(e) if would_block(&e) => true,
            Err(_) => return End::Closed,
        };
        let sock = (ws.get_ref().as_raw_fd(), libc::POLLIN | if pending { libc::POLLOUT } else { 0 });
        match poll(&[sock, (bell.fd(), libc::POLLIN)], None) {
            Ok(r) if r[1] != 0 => bell.clear(),
            Ok(_) => {}
            Err(_) => return End::Closed,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::Addr;
    use crossbeam_channel::RecvTimeoutError;
    use std::net::{TcpListener, TcpStream};
    use std::time::Instant;

    type Srv = WebSocket<TcpStream>;

    /// A fake core for one pane: accepts one WebSocket, hands it to `serve`
    /// on its own thread. Returns the endpoint to attach to.
    fn fake(serve: impl FnOnce(Srv) + Send + 'static) -> Endpoint {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap();
        thread::spawn(move || {
            let (s, _) = l.accept().unwrap();
            s.set_nodelay(true).unwrap();
            serve(tungstenite::accept(s).unwrap());
        });
        Endpoint { addr: Addr::Tcp(addr.to_string()), token: None }
    }

    /// Server that reports each message it reads, with when it read it.
    fn recorder() -> (Endpoint, Receiver<(Instant, Message)>) {
        let (tx, rx) = unbounded();
        let ep = fake(move |mut ws| {
            while let Ok(m) = ws.read() {
                if tx.send((Instant::now(), m)).is_err() {
                    return;
                }
            }
        });
        (ep, rx)
    }

    #[test]
    fn idle_pane_thread_does_not_wake() {
        let (ep, _seen) = recorder();
        let pane = PaneConn::attach(&ep, "idlepane", false).unwrap();
        thread::sleep(Duration::from_millis(100));
        let name = "nk-pane-idlepan"; // /proc truncates names to 15 bytes
        let before = crate::wake::ctx_switches(name);
        assert!(before > 0, "thread {name} not found");
        thread::sleep(Duration::from_secs(1));
        let woke = crate::wake::ctx_switches(name) - before;
        assert!(woke <= 1, "idle pane thread woke {woke} times in 1 s");
        drop(pane);
    }

    #[test]
    fn writes_reach_the_server_at_once() {
        let (ep, seen) = recorder();
        let pane = PaneConn::attach(&ep, "latency", false).unwrap();
        let mut worst = Duration::ZERO;
        for i in 0..10u8 {
            thread::sleep(Duration::from_millis(30)); // the pane thread is idle again
            let t0 = Instant::now();
            pane.write(&[b'a' + i]);
            let (at, m) = seen.recv_timeout(Duration::from_secs(2)).expect("the write never arrived");
            assert_eq!(m, Message::Binary(vec![b'a' + i].into()));
            worst = worst.max(at - t0);
        }
        assert!(worst < Duration::from_millis(5), "a write took {worst:?} to reach the server");
        pane.resize(100, 30);
        let (_, m) = seen.recv_timeout(Duration::from_secs(2)).unwrap();
        let Message::Text(t) = m else { panic!("{m:?}") };
        let c: Control = serde_json::from_str(t.as_str()).unwrap();
        assert_eq!((c.kind.as_str(), c.cols, c.rows), ("resize", 100, 30));
    }

    #[test]
    fn a_burst_of_frames_is_delivered_whole() {
        // several frames land in one read: tungstenite buffers the rest,
        // which must come out without waiting for more socket traffic
        let (hold_tx, hold_rx) = unbounded::<()>();
        let ep = fake(move |mut ws| {
            for b in [&b"one"[..], b"two", b"three"] {
                ws.write(Message::Binary(b.to_vec().into())).unwrap();
            }
            ws.write(Message::Text(r#"{"type":"title","text":"hi"}"#.into())).unwrap();
            ws.flush().unwrap();
            let _ = hold_rx.recv(); // keep the socket open and silent
        });
        let pane = PaneConn::attach(&ep, "burst", false).unwrap();
        let mut got = Vec::new();
        while got.len() < 4 {
            match pane.rx().recv_timeout(Duration::from_secs(1)) {
                Ok(m) => got.push(m),
                Err(e) => panic!("{e:?} after {got:?}"),
            }
        }
        assert_eq!(
            got,
            vec![
                PaneMsg::Data(b"one".to_vec()),
                PaneMsg::Data(b"two".to_vec()),
                PaneMsg::Data(b"three".to_vec()),
                PaneMsg::Title("hi".into()),
            ]
        );
        drop(hold_tx);
    }

    #[test]
    fn server_close_and_detach_are_prompt() {
        // the server going away ends the pane with Closed
        let ep = fake(|mut ws| {
            ws.write(Message::Binary(b"bye".to_vec().into())).unwrap();
            ws.close(None).unwrap();
            while ws.read().is_ok() {}
        });
        let pane = PaneConn::attach(&ep, "srvclose", false).unwrap();
        assert_eq!(pane.rx().recv_timeout(Duration::from_secs(1)), Ok(PaneMsg::Data(b"bye".to_vec())));
        assert_eq!(pane.rx().recv_timeout(Duration::from_secs(1)), Ok(PaneMsg::Closed));

        // dropping the PaneConn closes the socket at once
        let (ep, seen) = recorder();
        let pane = PaneConn::attach(&ep, "detach", false).unwrap();
        thread::sleep(Duration::from_millis(50));
        let t0 = Instant::now();
        drop(pane);
        match seen.recv_timeout(Duration::from_secs(1)) {
            Ok((at, Message::Close(_))) => assert!(at - t0 < Duration::from_millis(50), "{:?}", at - t0),
            Ok((_, m)) => panic!("{m:?}"),
            Err(RecvTimeoutError::Timeout) => panic!("no close frame"),
            Err(RecvTimeoutError::Disconnected) => {} // reader ended: socket closed
        }
    }
}
