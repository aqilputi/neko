//! A pane's WebSocket (`WS /v1/panes/{id}/ws`) on its own thread: binary
//! frames are raw PTY bytes both ways, text frames are JSON controls.

use std::io;
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

/// How long a read waits before the thread looks at what to send: the
/// worst-case added latency of a keystroke.
const POLL: Duration = Duration::from_millis(8);
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);
/// Reconnections after a close 1013 (the server asks to come back).
const RETRIES: u32 = 5;

/// An attached pane. Dropping it detaches (the pane keeps running).
pub struct PaneConn {
    id: String,
    reused: bool,
    tx: Sender<Out>,
    rx: Receiver<PaneMsg>,
    ep: Endpoint,
}

impl PaneConn {
    pub(crate) fn attach(ep: &Endpoint, id: &str, reused: bool) -> Result<PaneConn> {
        let ws = connect(ep, id)?;
        let (tx, out_rx) = unbounded();
        let (in_tx, rx) = unbounded();
        let (ep2, id2) = (ep.clone(), id.to_string());
        thread::Builder::new()
            .name(format!("nk-pane-{id}"))
            .spawn(move || run(ws, &ep2, &id2, out_rx, in_tx))
            .context("thread do painel")?;
        Ok(PaneConn { id: id.to_string(), reused, tx, rx, ep: ep.clone() })
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
            let _ = self.tx.send(Out::Data(bytes.to_vec()));
        }
    }

    /// Resizes the PTY (last writer wins among viewers).
    pub fn resize(&self, cols: u16, rows: u16) {
        if cols > 0 && rows > 0 {
            let _ = self.tx.send(Out::Resize(cols, rows));
        }
    }

    /// What the pane sends.
    pub fn rx(&self) -> &Receiver<PaneMsg> {
        &self.rx
    }

    /// Detaches; with `kill`, also closes the pane on the core (SIGHUP to
    /// the program).
    pub fn close(&self, kill: bool) {
        let _ = self.tx.send(Out::Close);
        if kill {
            let path = format!("/v1/panes/{}", encode(&self.id));
            let _ = self.ep.call::<(), serde_json::Value>("DELETE", &path, None);
        }
    }
}

impl Drop for PaneConn {
    fn drop(&mut self) {
        let _ = self.tx.send(Out::Close);
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
    ws.get_ref().set_read_timeout(Some(POLL))?;
    Ok(ws)
}

fn run(mut ws: WebSocket<Conn>, ep: &Endpoint, id: &str, out: Receiver<Out>, tx: Sender<PaneMsg>) {
    let mut retries = 0;
    loop {
        match pump(&mut ws, &out, &tx) {
            End::Detach => {
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

fn pump(ws: &mut WebSocket<Conn>, out: &Receiver<Out>, tx: &Sender<PaneMsg>) -> End {
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
            if ws.send(msg).is_err() {
                return End::Closed;
            }
        }
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
                let _ = ws.flush();
                return if again { End::Again } else { End::Closed };
            }
            Ok(_) => Ok(()), // ping/pong/raw frames
            Err(tungstenite::Error::Io(e))
                if matches!(e.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut) =>
            {
                let _ = ws.flush(); // a queued pong or close reply
                Ok(())
            }
            Err(_) => return End::Closed,
        };
        if sent.is_err() {
            return End::Detach;
        }
    }
}
