//! The bridge to the core: a worker thread owns `nk_client::Core` (all its
//! calls block on HTTP), turns requests from the UI into calls and the SSE
//! stream into messages, and wakes the (reactive, idle) Bevy loop each time
//! it hands something over. Pane bytes get one forwarding thread per pane.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, Sender, select, unbounded};

use crate::keys::Bind;
use crate::model::{Palette, Row, Sess, parse_rfc3339};
use crate::monitor::{Line, role_of};
use crate::nkc;

pub type Wake = Arc<dyn Fn() + Send + Sync>;

/// UI → worker.
pub enum Req {
    Tail(String),
    Run {
        name: String,
        session: Option<String>,
    },
    Confirm {
        token: String,
        yes: bool,
    },
    OpenPane {
        session: String,
        cols: u16,
        rows: u16,
    },
}

/// Everything the snapshot + tree tell the office.
#[derive(Clone, Debug, Default)]
pub struct WorldData {
    pub sessions: Vec<Sess>,
    pub rows: Vec<Row>,
    /// mail cues: session → (at ms, sent)
    pub mail: HashMap<String, (i64, bool)>,
    pub usage_5h: Option<i64>,
    pub today_cost: f64,
}

/// Worker → UI.
pub enum Resp {
    Init {
        palette: Palette,
        binds: Vec<Bind>,
        client_cmds: HashSet<String>,
        version: String,
    },
    World(WorldData),
    Tail {
        id: String,
        lines: Vec<Line>,
    },
    Event {
        name: String,
        data: serde_json::Value,
    },
    Cmd {
        toast: String,
        err: bool,
        pending: Option<(String, String)>,
        data: serde_json::Value,
    },
    Pane {
        session: String,
        conn: nkc::PaneConn,
    },
    PaneMsg {
        pane: String,
        msg: nkc::PaneMsg,
    },
    Status(String),
    Error(String),
}

pub struct Io {
    pub tx: Sender<Req>,
    pub rx: Receiver<Resp>,
}

/// How many tail lines a monitor asks for.
pub const TAIL_N: usize = 40;

pub fn spawn(opts: nkc::StartOpts, wake: Wake) -> Io {
    let (req_tx, req_rx) = unbounded::<Req>();
    let (resp_tx, resp_rx) = unbounded::<Resp>();
    std::thread::Builder::new()
        .name("nk-core".into())
        .spawn(move || worker(opts, req_rx, resp_tx, wake))
        .expect("spawn core worker");
    Io {
        tx: req_tx,
        rx: resp_rx,
    }
}

fn worker(opts: nkc::StartOpts, req_rx: Receiver<Req>, tx: Sender<Resp>, wake: Wake) {
    let send = |r: Resp| {
        let _ = tx.send(r);
        wake();
    };
    let core = match nkc::Core::start(opts) {
        Ok(c) => c,
        Err(e) => {
            send(Resp::Error(format!("sem core: {e:#}")));
            return;
        }
    };
    let version = core.hello().map(|h| h.version).unwrap_or_default();
    let palette = match core.theme(None) {
        Ok(t) => {
            let tokens: HashMap<String, String> = t.tokens.into_iter().collect();
            Palette::from_tokens(&tokens, &t.contexts)
        }
        Err(_) => Palette::default(),
    };
    let binds: Vec<Bind> = core
        .keymap()
        .map(|bs| {
            bs.into_iter()
                .map(|b| Bind {
                    command: b.command,
                    keys: b.keys,
                    mode: b.mode,
                    help: b.help.desc,
                })
                .collect()
        })
        .unwrap_or_default();
    let client_cmds: HashSet<String> = core
        .commands(&nkc::Context::default())
        .map(|cs| {
            cs.into_iter()
                .filter(|c| c.scope == "client")
                .map(|c| c.name)
                .collect()
        })
        .unwrap_or_default();
    send(Resp::Init {
        palette,
        binds,
        client_cmds,
        version,
    });

    let events = core.events();
    let mut view = nkc::ViewState::default();
    let mut dirty = true;
    let mut last_refresh = Instant::now() - Duration::from_secs(10);
    let mut tails_wanted: HashSet<String> = HashSet::new();
    loop {
        // refresh at most every 250 ms, coalescing bursts of events
        if dirty && last_refresh.elapsed() >= Duration::from_millis(250) {
            dirty = false;
            last_refresh = Instant::now();
            match world(&core, &mut view) {
                Ok(w) => send(Resp::World(w)),
                Err(e) => send(Resp::Status(format!("{e:#}"))),
            }
        }
        for id in std::mem::take(&mut tails_wanted) {
            if let Ok(lines) = core.tail(&id, TAIL_N) {
                let lines = lines
                    .into_iter()
                    .map(|l| Line {
                        role: role_of(&l.kind, &l.text),
                        at: l.at.to_string(),
                        text: l.text,
                    })
                    .collect();
                send(Resp::Tail { id, lines });
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
                    Req::Tail(id) => { tails_wanted.insert(id); }
                    Req::Run { name, session } => {
                        let ctx = nkc::Context { session_id: session.unwrap_or_default(), ..Default::default() };
                        let res = core.run(&name, &BTreeMap::new(), &ctx);
                        send(cmd_resp(res));
                        dirty = true;
                    }
                    Req::Confirm { token, yes } => {
                        send(cmd_resp(core.confirm(&token, yes)));
                        dirty = true;
                    }
                    Req::OpenPane { session, cols, rows } => match core.open_pane(&session, cols, rows) {
                        Ok(conn) => {
                            forward_pane(&conn, tx.clone(), wake.clone());
                            send(Resp::Pane { session, conn });
                        }
                        Err(e) => send(Resp::Cmd { toast: format!("painel: {e:#}"), err: true, pending: None, data: serde_json::Value::Null }),
                    },
                }
            }
            recv(events) -> ev => {
                let Ok(ev) = ev else {
                    send(Resp::Status("eventos: desconectado".into()));
                    std::thread::sleep(Duration::from_secs(1));
                    continue;
                };
                match ev {
                    nkc::Event::Core { name, data, .. } => {
                        match name.as_str() {
                            "snapshot.changed" | "session.state" | "pane.opened" | "pane.closed" | "pane.exited" => dirty = true,
                            "session.tail" => {
                                if let Some(id) = data.get("id").and_then(|v| v.as_str()) {
                                    tails_wanted.insert(id.to_string());
                                }
                            }
                            _ => {}
                        }
                        send(Resp::Event { name, data });
                    }
                    nkc::Event::Resync => dirty = true,
                    nkc::Event::Status(s) => send(Resp::Status(status_text(&s))),
                }
            }
            default(timeout) => {}
        }
    }
}

fn cmd_resp(res: anyhow::Result<nkc::CmdResult>) -> Resp {
    match res {
        Ok(r) => Resp::Cmd {
            toast: r.toast,
            err: r.err,
            pending: r.pending.map(|p| (p.token, p.question)),
            data: r.data,
        },
        Err(e) => Resp::Cmd {
            toast: format!("{e:#}"),
            err: true,
            pending: None,
            data: serde_json::Value::Null,
        },
    }
}

fn forward_pane(conn: &nkc::PaneConn, tx: Sender<Resp>, wake: Wake) {
    let rx = conn.rx().clone();
    let pane = conn.id().to_string();
    std::thread::Builder::new()
        .name(format!("pane-{pane}"))
        .spawn(move || {
            while let Ok(msg) = rx.recv() {
                let last = matches!(msg, nkc::PaneMsg::Closed);
                if tx
                    .send(Resp::PaneMsg {
                        pane: pane.clone(),
                        msg,
                    })
                    .is_err()
                {
                    return;
                }
                wake();
                if last {
                    return;
                }
            }
        })
        .expect("spawn pane forwarder");
}

fn world(core: &nkc::Core, view: &mut nkc::ViewState) -> anyhow::Result<WorldData> {
    let snap = core.snapshot()?;
    let tree = core.view_tree(view)?;
    *view = tree.state.clone();
    let sessions = snap
        .sessions
        .iter()
        .map(|s| Sess {
            id: s.session_id.clone(),
            short: s.short_id.clone(),
            label: if s.label.is_empty() {
                s.session_id.chars().take(8).collect()
            } else {
                s.label.clone()
            },
            ctx: s.context.clone(),
            state: s.state.clone(),
            attention: s.attention,
            last_at_ms: parse_rfc3339(&s.last_at.to_string()),
            needs: s.needs.clone(),
            running_subs: s.subs.iter().filter(|x| !x.done).count(),
            groups: vec![],
            dim: s.stale || s.ended || s.kind == "history",
            archived: s.archived,
            ended: s.ended,
            finished: s.finished,
            cost: s.cost,
        })
        .collect();
    let rows = tree
        .rows
        .iter()
        .map(|r| Row {
            kind: r.kind.clone(),
            ctx: r.ctx.clone(),
            color: r.color,
            session_id: r.session_id.clone(),
            groups: r.groups.iter().map(|g| (g.tag.clone(), g.color)).collect(),
        })
        .collect();
    let mail = snap
        .mail
        .iter()
        .map(|(id, m)| (id.clone(), (parse_rfc3339(&m.at.to_string()), m.sent)))
        .collect();
    let usage_5h = snap
        .usage
        .limits
        .iter()
        .find(|l| l.name == "5h")
        .map(|l| l.pct);
    Ok(WorldData {
        sessions,
        rows,
        mail,
        usage_5h,
        today_cost: snap.today_cost,
    })
}

/// The status bar's error slot: empty while the event stream is healthy.
pub fn status_text(s: &nkc::ConnStatus) -> String {
    match s {
        nkc::ConnStatus::Connected => String::new(),
        nkc::ConnStatus::Connecting => "reconectando ao core…".into(),
        nkc::ConnStatus::Disconnected(why) if why.is_empty() => "core desconectado".into(),
        nkc::ConnStatus::Disconnected(why) => format!("core desconectado: {why}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn healthy_stream_shows_nothing() {
        assert_eq!(status_text(&nkc::ConnStatus::Connected), "");
        assert_eq!(
            status_text(&nkc::ConnStatus::Connecting),
            "reconectando ao core…"
        );
        assert_eq!(
            status_text(&nkc::ConnStatus::Disconnected(String::new())),
            "core desconectado"
        );
        assert_eq!(
            status_text(&nkc::ConnStatus::Disconnected("EOF".into())),
            "core desconectado: EOF"
        );
    }
}
