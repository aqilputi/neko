//! The core link (§6.9): one thread owns `nk_client::Core` (every call
//! blocks on HTTP), turns the plugin's requests into calls and the SSE bus
//! into messages, and pings the loop each time it hands something over.
//! It sleeps in `select!` on its two channels (a 250 ms timeout only while
//! a burst of events is being coalesced into one snapshot read). Each open
//! pane gets a forwarding thread that blocks on the pane's channel.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::time::{Duration, Instant};

use nk_client::crossbeam_channel::{Receiver, Sender, select, unbounded};
use nk_client::{
    Binding, CmdResult, ConnStatus, Context, Core, Event, PaneConn, PaneMsg, Snapshot, StartOpts,
    TailLine, Tree, ViewState,
};

use crate::cli;

/// The plugin → the core.
#[derive(Debug, Clone, PartialEq)]
pub enum ToCore {
    /// re-read a session's tail
    Tail(String),
    /// run a core command with the session as context; `tag` comes back
    /// with the result
    Run {
        tag: u64,
        name: String,
        session: String,
        args: BTreeMap<String, String>,
    },
    Confirm {
        tag: u64,
        token: String,
        yes: bool,
    },
    /// open (or reuse) a session's pane, typing `reply` once it is up
    Pane {
        session: String,
        cols: u16,
        rows: u16,
        reply: Option<String>,
    },
}

/// The core → the plugin.
pub enum FromCore {
    /// "connecting" | "ok" | "error", with a message to show (empty: none)
    Status(&'static str, String),
    /// the theme's tokens and context palette
    Theme(nk_cats::model::Palette),
    Keymap(Vec<Binding>),
    World(Box<Snapshot>, Box<Tree>),
    Tail(String, Vec<TailLine>),
    Event(String, serde_json::Value),
    Done(u64, Result<CmdResult, String>),
    Pane(String, PaneConn),
    PaneFailed(String, String),
    PaneMsg(String, PaneMsg),
}

pub struct Link {
    pub tx: Sender<ToCore>,
    pub rx: Receiver<FromCore>,
}

/// What `--core` connects to.
pub fn opts_for(core: &cli::Core) -> Option<StartOpts> {
    match core {
        cli::Core::None => None,
        cli::Core::Auto => Some(StartOpts::default()),
        cli::Core::Demo => Some(StartOpts {
            demo: true,
            ..StartOpts::default()
        }),
        cli::Core::Url(u) => Some(StartOpts {
            connect: Some(u.clone()),
            ..StartOpts::default()
        }),
    }
}

/// The dump's `core` field and the toast of a connection state.
pub fn status_of(s: &ConnStatus) -> (&'static str, String) {
    match s {
        ConnStatus::Connected => ("ok", String::new()),
        ConnStatus::Connecting => ("connecting", String::new()),
        ConnStatus::Disconnected(why) if why.is_empty() => ("error", "core desconectado".into()),
        ConnStatus::Disconnected(why) => ("error", format!("core desconectado: {why}")),
    }
}

/// Events after which the world is read again.
pub fn refreshes(name: &str) -> bool {
    matches!(
        name,
        "snapshot.changed"
            | "session.state"
            | "session.new"
            | "session.removed"
            | "pane.opened"
            | "pane.closed"
            | "pane.exited"
    )
}

pub type Wake = std::sync::Arc<dyn Fn() + Send + Sync>;

/// Start the link's thread.
pub fn spawn(opts: StartOpts, wake: Wake) -> Link {
    let (tx, req_rx) = unbounded::<ToCore>();
    let (resp_tx, rx) = unbounded::<FromCore>();
    let _ = std::thread::Builder::new()
        .name("nkw-core".into())
        .spawn(move || worker(opts, req_rx, resp_tx, wake));
    Link { tx, rx }
}

fn worker(opts: StartOpts, req_rx: Receiver<ToCore>, tx: Sender<FromCore>, wake: Wake) {
    let send = |m: FromCore| {
        let _ = tx.send(m);
        wake();
    };
    let core = match Core::start(opts) {
        Ok(c) => c,
        Err(e) => {
            send(FromCore::Status("error", format!("sem core: {e:#}")));
            return;
        }
    };
    if let Ok(t) = core.theme(None) {
        let tokens: HashMap<String, String> = t.tokens.into_iter().collect();
        send(FromCore::Theme(nk_cats::model::Palette::from_tokens(
            &tokens,
            &t.contexts,
        )));
    }
    if let Ok(k) = core.keymap() {
        send(FromCore::Keymap(k));
    }
    let events = core.events();
    let mut view = ViewState::default();
    let mut dirty = true;
    let mut last = Instant::now() - Duration::from_secs(10);
    let mut tails: HashSet<String> = HashSet::new();
    loop {
        if dirty && last.elapsed() >= Duration::from_millis(250) {
            dirty = false;
            last = Instant::now();
            match core.snapshot().and_then(|s| {
                let t = core.view_tree(&view)?;
                Ok((s, t))
            }) {
                Ok((s, t)) => {
                    view = t.state.clone();
                    send(FromCore::World(Box::new(s), Box::new(t)));
                }
                Err(e) => send(FromCore::Status("error", format!("{e:#}"))),
            }
        }
        for id in std::mem::take(&mut tails) {
            if let Ok(lines) = core.tail(&id, crate::cats::monitor::TAIL_N) {
                send(FromCore::Tail(id, lines));
            }
        }
        let timeout = if dirty {
            Duration::from_millis(250)
        } else {
            Duration::from_secs(3600)
        };
        select! {
            recv(req_rx) -> r => {
                let Ok(r) = r else { return };
                match r {
                    ToCore::Tail(id) => { tails.insert(id); }
                    ToCore::Run { tag, name, session, args } => {
                        let ctx = Context::session(session);
                        let res = core.run(&name, &args, &ctx).map_err(|e| format!("{e:#}"));
                        send(FromCore::Done(tag, res));
                        dirty = true;
                    }
                    ToCore::Confirm { tag, token, yes } => {
                        let res = core.confirm(&token, yes).map_err(|e| format!("{e:#}"));
                        send(FromCore::Done(tag, res));
                        dirty = true;
                    }
                    ToCore::Pane { session, cols, rows, reply } => {
                        match core.open_pane_reply(&session, cols, rows, reply.as_deref()) {
                            Ok(conn) => {
                                forward(&conn, &session, tx.clone(), wake.clone());
                                send(FromCore::Pane(session, conn));
                            }
                            Err(e) => send(FromCore::PaneFailed(session, format!("{e:#}"))),
                        }
                        dirty = true;
                    }
                }
            }
            recv(events) -> ev => {
                let Ok(ev) = ev else {
                    send(FromCore::Status("error", "eventos: desconectado".into()));
                    return;
                };
                match ev {
                    Event::Core { name, data, .. } => {
                        if refreshes(&name) {
                            dirty = true;
                        }
                        if name == "session.tail"
                            && let Some(id) = data.get("id").or(data.get("session_id")).and_then(|v| v.as_str())
                        {
                            tails.insert(id.to_string());
                        }
                        if name == "keymap.changed"
                            && let Ok(k) = core.keymap()
                        {
                            send(FromCore::Keymap(k));
                        }
                        send(FromCore::Event(name, data));
                    }
                    Event::Resync => dirty = true,
                    Event::Status(s) => {
                        let (st, msg) = status_of(&s);
                        send(FromCore::Status(st, msg));
                    }
                }
            }
            default(timeout) => {}
        }
    }
}

/// A thread that hands a pane's messages to the plugin until it closes.
fn forward(conn: &PaneConn, session: &str, tx: Sender<FromCore>, wake: Wake) {
    let rx = conn.rx().clone();
    let session = session.to_string();
    let _ = std::thread::Builder::new()
        .name("nkw-pane".into())
        .spawn(move || {
            while let Ok(msg) = rx.recv() {
                let last = matches!(msg, PaneMsg::Closed);
                if tx.send(FromCore::PaneMsg(session.clone(), msg)).is_err() {
                    return;
                }
                wake();
                if last {
                    return;
                }
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn core_option_to_start_opts() {
        assert!(opts_for(&cli::Core::None).is_none());
        let a = opts_for(&cli::Core::Auto).unwrap();
        assert!(!a.demo && a.connect.is_none());
        assert!(opts_for(&cli::Core::Demo).unwrap().demo);
        let u = opts_for(&cli::Core::Url("tcp://127.0.0.1:9".into())).unwrap();
        assert_eq!(u.connect.as_deref(), Some("tcp://127.0.0.1:9"));
    }

    #[test]
    fn connection_states() {
        assert_eq!(status_of(&ConnStatus::Connected), ("ok", String::new()));
        assert_eq!(status_of(&ConnStatus::Connecting).0, "connecting");
        assert_eq!(
            status_of(&ConnStatus::Disconnected("EOF".into())),
            ("error", "core desconectado: EOF".into())
        );
        assert_eq!(
            status_of(&ConnStatus::Disconnected(String::new())).1,
            "core desconectado"
        );
    }

    #[test]
    fn which_events_refresh() {
        for n in ["snapshot.changed", "session.state", "pane.closed"] {
            assert!(refreshes(n), "{n}");
        }
        for n in ["toast", "session.tail", "confirm.requested", "ui.scene"] {
            assert!(!refreshes(n), "{n}");
        }
    }

    /// The link against `nk serve --demo` (needs `nk` on PATH; never the
    /// user's own core): the world arrives, then a tail.
    #[test]
    #[ignore = "needs nk on PATH; NKW_E2E=1 cargo test -p nkw -- --ignored"]
    fn against_the_demo_core() {
        if std::env::var("NKW_E2E").as_deref() != Ok("1") {
            return;
        }
        let link = spawn(
            StartOpts {
                demo: true,
                ..StartOpts::default()
            },
            std::sync::Arc::new(|| {}),
        );
        let deadline = Instant::now() + Duration::from_secs(20);
        let mut world = None;
        while world.is_none() && Instant::now() < deadline {
            if let Ok(FromCore::World(s, _)) = link.rx.recv_timeout(Duration::from_secs(1)) {
                world = Some(s);
            }
        }
        let snap = world.expect("a snapshot from the demo core");
        let gw = snap
            .sessions
            .iter()
            .find(|s| s.label == "api-gateway")
            .expect("api-gateway");
        link.tx.send(ToCore::Tail(gw.session_id.clone())).unwrap();
        let mut got = false;
        while !got && Instant::now() < deadline {
            if let Ok(FromCore::Tail(id, _)) = link.rx.recv_timeout(Duration::from_secs(1)) {
                got = id == gw.session_id;
            }
        }
        assert!(got, "a tail");
    }
}
