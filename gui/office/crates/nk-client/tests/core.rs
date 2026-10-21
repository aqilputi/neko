//! Integration tests against the real core: each test spawns its own
//! `nk serve --demo` (NK_BIN, default ~/.local/bin/nk) with every XDG dir in
//! a tempdir and NEKO_CLAUDE=/bin/false, so nothing touches the user's state.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use nk_client::*;

const GATEWAY: &str = "d3e0a001-0000-4000-8000-000000000001";
const DOTFILES: &str = "d3e0a004-0000-4000-8000-000000000004";
const INDEXER: &str = "d3e0a006-0000-4000-8000-000000000006";

fn nk_bin() -> PathBuf {
    std::env::var_os("NK_BIN")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(std::env::var("HOME").unwrap()).join(".local/bin/nk"))
}

fn sandbox_env(dir: &std::path::Path) -> Vec<(String, String)> {
    let mut env = Vec::new();
    for (k, sub) in
        [("HOME", "home"), ("XDG_STATE_HOME", "state"), ("XDG_CONFIG_HOME", "config"), ("XDG_CACHE_HOME", "cache")]
    {
        let p = dir.join(sub);
        std::fs::create_dir_all(&p).unwrap();
        env.push((k.to_string(), p.display().to_string()));
    }
    env.push(("NEKO_CLAUDE".into(), "/bin/false".into()));
    env
}

/// A demo core in a sandbox (XDG_RUNTIME_DIR is Core's own private dir).
fn demo() -> (Core, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let core = Core::start(StartOpts {
        demo: true,
        nk_bin: Some(nk_bin()),
        env: sandbox_env(dir.path()),
        ..StartOpts::default()
    })
    .expect("nk serve --demo");
    wait_sessions(&core);
    (core, dir)
}

/// The snapshot is empty until the core's first refresh.
fn wait_sessions(core: &Core) {
    let ok = wait_for(Duration::from_secs(10), || (core.snapshot().ok()?.sessions.len() >= 6).then_some(()));
    assert!(ok.is_some(), "the demo sessions never showed up");
}

fn wait_for<T>(within: Duration, mut f: impl FnMut() -> Option<T>) -> Option<T> {
    let deadline = Instant::now() + within;
    while Instant::now() < deadline {
        if let Some(v) = f() {
            return Some(v);
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    None
}

#[test]
fn hello_announces_demo() {
    let (core, _d) = demo();
    let h = core.hello().unwrap();
    assert_eq!(h.app, "nekuroddo");
    assert_eq!(h.protocol, 1);
    assert!(h.has("demo"), "{:?}", h.capabilities);
    assert_eq!(Some(h.pid as u32), core.child_pid());
    assert!(core.addr().starts_with("tcp://127.0.0.1:"));
    assert!(core.token().is_some_and(|t| t.len() >= 20));
}

#[test]
fn snapshot_tree_theme_keymap_commands() {
    let (core, _d) = demo();
    let s = core.snapshot().unwrap();
    let labels: Vec<&str> = s.sessions.iter().map(|x| x.label.as_str()).collect();
    for want in ["api-gateway", "migração do banco", "relatório Q3", "dotfiles", "indexador", "site de docs"] {
        assert!(labels.contains(&want), "{want} in {labels:?}");
    }
    assert_eq!(s.groups["deploy-rc"].len(), 4);

    let t = core.view_tree(&ViewState::default()).unwrap();
    assert!(t.rows.iter().any(|r| r.kind == "ctx"));
    assert!(t.rows.iter().any(|r| r.session_id == GATEWAY));
    // the state round-trips: a second build keeps the frozen order
    let t2 = core.view_tree(&t.state).unwrap();
    assert_eq!(t2.state.order, t.state.order);
    let all = core.view_tree(&ViewState { show_all: true, ..t.state.clone() }).unwrap();
    assert!(all.rows.len() > t.rows.len(), "show_all lists the stale blog");
    let e = core.view_tree(&ViewState { tab: "nope".into(), ..ViewState::default() }).unwrap_err();
    assert_eq!(error_code(&e), Some("bad_request"));

    let th = core.theme(None).unwrap();
    assert!(th.rgb("bg").is_some());
    assert_eq!(core.theme(Some("dracula")).unwrap().rgb("bg"), Some([0x28, 0x2a, 0x36]));
    assert_eq!(error_code(&core.theme(Some("nope")).unwrap_err()), Some("not_found"));

    let k = core.keymap().unwrap();
    assert!(k.iter().any(|b| b.command == "inbox.summon" && b.keys.contains(&"shift+space".to_string())));

    let none = core.commands(&Context::default()).unwrap();
    let with = core.commands(&Context::session(GATEWAY)).unwrap();
    let avail = |v: &[CommandInfo], n: &str| v.iter().find(|c| c.name == n).map(|c| c.available);
    assert_eq!(avail(&with, "session.stop"), Some(true));
    assert_eq!(avail(&none, "session.stop"), Some(false));
    assert!(core.panes().unwrap().is_empty());
}

#[test]
fn tail_of_a_busy_session() {
    let (core, _d) = demo();
    let lines = core.tail(INDEXER, 10).unwrap();
    assert!(!lines.is_empty() && lines.len() <= 10, "{}", lines.len());
    assert!(lines.iter().all(|l| !l.kind.is_empty()));
    assert_eq!(core.tail(INDEXER, 500).unwrap().len().min(200), core.tail(INDEXER, 500).unwrap().len());
    assert_eq!(error_code(&core.tail("nope", 5).unwrap_err()), Some("not_found"));
}

#[test]
fn events_deliver_snapshot_changed() {
    let (core, _d) = demo();
    let rx = core.events();
    let mut seen = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut got = false;
    while Instant::now() < deadline {
        let Ok(ev) = rx.recv_timeout(Duration::from_millis(200)) else { continue };
        let is = matches!(&ev, Event::Core { name, .. } if name == "snapshot.changed");
        seen.push(ev);
        if is {
            got = true;
            break;
        }
    }
    assert!(got, "{seen:?}");
    assert_eq!(seen[0], Event::Status(ConnStatus::Connecting));
    assert_eq!(seen[1], Event::Status(ConnStatus::Connected));
    assert_eq!(seen[2], Event::Resync, "a fresh stream asks for a resync");
    // a command's effect shows up on the bus too
    let r = core.run("session.stop", &BTreeMap::new(), &Context::session(GATEWAY)).unwrap();
    let token = r.pending.unwrap().token;
    let ok = wait_for(Duration::from_secs(5), || match rx.try_recv() {
        Ok(Event::Core { name, data, .. }) if name == "confirm.requested" => Some(data),
        _ => None,
    });
    assert!(ok.is_some_and(|d| d.to_string().contains(&token)));
}

#[test]
fn events_reconnect_after_the_core_dies() {
    let (core, _d) = demo();
    let rx = core.events();
    let pid = core.child_pid().unwrap();
    assert!(wait_for(Duration::from_secs(5), || matches!(rx.try_recv(), Ok(Event::Resync)).then_some(())).is_some());
    // SAFETY: killing our own child.
    unsafe { libc::kill(pid as libc::pid_t, libc::SIGKILL) };
    let st = wait_for(Duration::from_secs(5), || match rx.try_recv() {
        Ok(Event::Status(ConnStatus::Disconnected(why))) => Some(why),
        _ => None,
    });
    assert!(st.is_some(), "a dead core is reported");
}

#[test]
fn pane_echo_in_the_term() {
    let (core, _d) = demo();
    let pane = core.open_pane(GATEWAY, 80, 24).unwrap();
    assert!(!pane.id().is_empty());
    let mut term = Term::new(80, 24);
    let mut title = None;
    let drain = |term: &mut Term, title: &mut Option<String>| {
        while let Ok(m) = pane.rx().try_recv() {
            match m {
                PaneMsg::Data(b) => term.feed(&b),
                PaneMsg::Title(t) => *title = Some(t),
                other => panic!("unexpected {other:?}"),
            }
        }
    };
    // the demo screen draws first; type once it is up
    let up = wait_for(Duration::from_secs(10), || {
        drain(&mut term, &mut title);
        (!term.grid().text().trim().is_empty()).then_some(())
    });
    assert!(up.is_some(), "the demo screen never drew");
    pane.write(b"ola\r");
    let echoed = wait_for(Duration::from_secs(10), || {
        drain(&mut term, &mut title);
        let reply = term.take_replies();
        if !reply.is_empty() {
            pane.write(&reply);
        }
        term.grid().text().contains("recebi: ola").then_some(())
    });
    assert!(echoed.is_some(), "screen:\n{}", term.grid().text());
    assert!(title.is_some(), "the server sends the pane title");

    // the pane is listed, resize is accepted, a second open reuses it
    pane.resize(100, 30);
    let listed = wait_for(Duration::from_secs(5), || {
        core.panes().unwrap().into_iter().find(|p| p.id == pane.id() && p.cols == 100 && p.rows == 30)
    });
    assert!(listed.is_some(), "{:?}", core.panes());
    let again = core.open_pane(GATEWAY, 80, 24).unwrap();
    assert_eq!(again.id(), pane.id());
    assert!(again.reused());
    again.close(false);
    assert!(
        wait_for(Duration::from_secs(5), || matches!(again.rx().try_recv(), Ok(PaneMsg::Closed)).then_some(()))
            .is_some()
    );
    assert_eq!(core.panes().unwrap().len(), 1, "detaching keeps the pane");

    // kill closes it on the core: the viewer gets the exit, then Closed
    pane.close(true);
    let gone = wait_for(Duration::from_secs(5), || core.panes().unwrap().is_empty().then_some(()));
    assert!(gone.is_some());
    let end = wait_for(Duration::from_secs(5), || match pane.rx().try_recv() {
        Ok(PaneMsg::Closed) => Some(()),
        _ => None,
    });
    assert!(end.is_some());
    assert_eq!(error_code(&core.open_pane("nope", 80, 24).err().unwrap()), Some("not_found"));
}

#[test]
fn pane_exit_is_reported() {
    let (core, _d) = demo();
    let pane = core.open_pane(GATEWAY, 80, 24).unwrap();
    std::thread::sleep(Duration::from_millis(500));
    pane.write(b"/exit\r");
    let mut msgs = Vec::new();
    let done = wait_for(Duration::from_secs(10), || {
        while let Ok(m) = pane.rx().try_recv() {
            if !matches!(m, PaneMsg::Data(_)) {
                msgs.push(m);
            }
        }
        msgs.contains(&PaneMsg::Closed).then_some(())
    });
    assert!(done.is_some(), "{msgs:?}");
    assert!(msgs.iter().any(|m| matches!(m, PaneMsg::Exit(_))), "{msgs:?}");
}

#[test]
fn run_and_confirm() {
    let (core, _d) = demo();
    let r = core.run("sessions.refresh", &BTreeMap::new(), &Context::default()).unwrap();
    assert!(!r.err, "{r:?}");

    // stopping a background session asks first; "no" cancels
    let r = core.run("session.stop", &BTreeMap::new(), &Context::session(GATEWAY)).unwrap();
    let p = r.pending.expect("pending");
    assert!(p.question.contains("api-gateway"), "{}", p.question);
    assert!(core.snapshot().unwrap().pending.iter().any(|x| x.token == p.token));
    let no = core.confirm(&p.token, false).unwrap();
    assert_eq!(no, CmdResult::default());
    assert_eq!(error_code(&core.confirm(&p.token, false).unwrap_err()), Some("unknown_token"));
    assert_eq!(core.snapshot().unwrap().session(GATEWAY).map(|s| s.state != "stopped"), Some(true));

    // "yes" runs it
    let p = core.run("session.stop", &BTreeMap::new(), &Context::session(GATEWAY)).unwrap().pending.unwrap();
    let yes = core.confirm(&p.token, true).unwrap();
    assert!(!yes.err, "{yes:?}");

    // a toast error is a result, not an Err
    let r = core.run("session.stop", &BTreeMap::new(), &Context::session(DOTFILES)).unwrap();
    assert!(r.err && !r.toast.is_empty());

    // args go through; unknown names are API errors
    let mut args = BTreeMap::new();
    args.insert("name".to_string(), "gateway novo".to_string());
    core.run("session.rename", &args, &Context::session(INDEXER)).unwrap();
    let renamed = wait_for(Duration::from_secs(5), || {
        let s = core.snapshot().unwrap();
        (s.session(INDEXER)?.label == "gateway novo").then_some(())
    });
    assert!(renamed.is_some());
    assert_eq!(
        error_code(&core.run("nope.nope", &BTreeMap::new(), &Context::default()).unwrap_err()),
        Some("unknown_command")
    );
}

#[test]
fn connects_over_a_unix_socket() {
    let dir = tempfile::tempdir().unwrap();
    let sock = dir.path().join("nk.sock");
    let mut cmd = std::process::Command::new(nk_bin());
    cmd.args(["serve", "--demo", "--listen"]).arg(format!("unix://{}", sock.display()));
    for (k, v) in sandbox_env(dir.path()) {
        cmd.env(k, v);
    }
    cmd.env("XDG_RUNTIME_DIR", dir.path()).stdout(std::process::Stdio::null());
    let mut child = cmd.spawn().unwrap();
    let ready = wait_for(Duration::from_secs(10), || sock.exists().then_some(()));
    assert!(ready.is_some());
    let res = std::panic::catch_unwind(|| {
        let core =
            Core::start(StartOpts { connect: Some(format!("unix://{}", sock.display())), ..StartOpts::default() })
                .unwrap();
        assert!(core.hello().unwrap().has("demo"));
        assert!(core.token().is_none());
        assert_eq!(core.child_pid(), None);
        wait_sessions(&core);
        let rx = core.events();
        assert!(
            wait_for(Duration::from_secs(5), || matches!(rx.try_recv(), Ok(Event::Resync)).then_some(())).is_some()
        );
        let pane = core.open_pane(GATEWAY, 80, 24).unwrap();
        std::thread::sleep(Duration::from_millis(500));
        pane.write(b"unix\r");
        let mut term = Term::new(80, 24);
        let ok = wait_for(Duration::from_secs(10), || {
            while let Ok(PaneMsg::Data(b)) = pane.rx().try_recv() {
                term.feed(&b);
            }
            term.grid().text().contains("recebi: unix").then_some(())
        });
        assert!(ok.is_some(), "{}", term.grid().text());
    });
    let _ = child.kill();
    let _ = child.wait();
    if let Err(e) = res {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn dropping_core_stops_the_child() {
    let (core, _d) = demo();
    let pid = core.child_pid().unwrap();
    let proc = PathBuf::from(format!("/proc/{pid}"));
    assert!(proc.exists());
    drop(core);
    assert!(!proc.exists(), "the core is reaped on drop");
}
