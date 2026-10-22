//! `Core`: connects to (or spawns) the nekuroddo core and speaks protocol v1.

use std::collections::{BTreeMap, VecDeque};
use std::io::{BufRead, BufReader, Read};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{DirBuilderExt, MetadataExt};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{ChildStderr, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context as _, Result, bail};
use crossbeam_channel::{Receiver, Sender, bounded, unbounded};
use serde::Serialize;

use crate::http::{Addr, Endpoint, encode};
use crate::pane::PaneConn;
use crate::proto::*;
use crate::sse::{ConnStatus, Event, SseParser, decode_frame};
use crate::wake::{Bell, poll};

/// How to reach the core.
#[derive(Debug, Clone, Default)]
pub struct StartOpts {
    /// Spawn `nk serve --demo` (the scripted demo world; ignored with
    /// `connect`).
    pub demo: bool,
    /// Connect to a running core: "tcp://127.0.0.1:PORT" (token from
    /// `$NEKO_TOKEN` or `$XDG_RUNTIME_DIR/nekuroddo-<uid>.token`) or
    /// "unix:///path". `None` spawns a core of our own.
    pub connect: Option<String>,
    /// The `nk` binary to spawn (default: `nk` on PATH, then ~/.local/bin/nk).
    pub nk_bin: Option<PathBuf>,
    /// Extra environment for a spawned core (HOME, XDG_*, NEKO_CLAUDE…).
    pub env: Vec<(String, String)>,
}

/// A spawned `nk serve`. The child belongs to a keeper thread that lives as
/// long as this value: PR_SET_PDEATHSIG fires when the thread that forked
/// exits, so the core dies with the keeper (on drop: SIGTERM, then SIGKILL)
/// or with the whole process if it crashes, and never earlier. The keeper
/// sleeps in `poll(2)` on the child's pidfd and a quit bell: no wakeups
/// until the child exits or we drop.
struct Spawned {
    pid: u32,
    exit: Arc<Mutex<Option<String>>>,
    quit: Arc<Bell>,
    keeper: Option<thread::JoinHandle<()>>,
}

impl Spawned {
    fn exited(&self) -> Option<String> {
        self.exit.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }
}

impl Drop for Spawned {
    fn drop(&mut self) {
        self.quit.ring();
        if let Some(h) = self.keeper.take() {
            let _ = h.join();
        }
    }
}

type Pipes = (u32, ChildStdout, ChildStderr);

/// Without a pidfd (kernel < 5.3) the keeper checks the child this often.
const KEEPER_FALLBACK_POLL: Duration = Duration::from_millis(50);
/// How long the core gets between SIGTERM and SIGKILL.
const TERM_GRACE: Duration = Duration::from_secs(2);

fn spawn_core(mut cmd: Command, runtime: PathBuf) -> std::io::Result<(Spawned, ChildStdout, ChildStderr)> {
    let (ready_tx, ready_rx) = bounded::<std::io::Result<Pipes>>(1);
    let quit = Arc::new(Bell::new()?);
    let quit2 = quit.clone();
    let exit = Arc::new(Mutex::new(None));
    let exit2 = exit.clone();
    let keeper = thread::Builder::new().name("nk-core-keeper".into()).spawn(move || {
        // SAFETY: prctl is async-signal-safe.
        unsafe {
            cmd.pre_exec(|| {
                libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
                Ok(())
            });
        }
        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                let _ = std::fs::remove_dir_all(&runtime);
                let _ = ready_tx.send(Err(e));
                return;
            }
        };
        let (out, err) = (child.stdout.take().expect("piped"), child.stderr.take().expect("piped"));
        let _ = ready_tx.send(Ok((child.id(), out, err)));
        let pidfd = crate::wake::pidfd(child.id()).ok();
        let mut reaped = false;
        loop {
            // once reaped, only the quit bell matters (the pidfd stays readable)
            let mut fds = vec![(quit2.fd(), libc::POLLIN)];
            if let (Some(p), false) = (&pidfd, reaped) {
                fds.push((p.as_raw_fd(), libc::POLLIN));
            }
            let timeout = (pidfd.is_none() && !reaped).then_some(KEEPER_FALLBACK_POLL);
            let quit_rang = match poll(&fds, timeout) {
                Ok(r) => r[0] != 0,
                Err(_) => {
                    thread::sleep(KEEPER_FALLBACK_POLL);
                    false
                }
            };
            if quit_rang {
                break;
            }
            if !reaped && let Ok(Some(st)) = child.try_wait() {
                *exit2.lock().unwrap_or_else(|p| p.into_inner()) = Some(st.to_string());
                reaped = true;
            }
        }
        if !reaped {
            // SAFETY: kill(2) on our own child's pid, not reaped yet (only
            // this thread waits on it), so the pid cannot have been reused.
            unsafe {
                libc::kill(child.id() as libc::pid_t, libc::SIGTERM);
            }
            match &pidfd {
                Some(p) => {
                    let _ = poll(&[(p.as_raw_fd(), libc::POLLIN)], Some(TERM_GRACE));
                }
                None => {
                    let deadline = Instant::now() + TERM_GRACE;
                    while Instant::now() < deadline && matches!(child.try_wait(), Ok(None)) {
                        thread::sleep(Duration::from_millis(10));
                    }
                }
            }
            let _ = child.kill();
        }
        let _ = child.wait();
        let _ = std::fs::remove_dir_all(&runtime);
    })?;
    let (pid, out, err) = match ready_rx.recv() {
        Ok(r) => r?,
        Err(_) => return Err(std::io::Error::other("o keeper do core morreu")),
    };
    Ok((Spawned { pid, exit, quit, keeper: Some(keeper) }, out, err))
}

/// A connection to the core. Cheap calls are blocking HTTP requests (one
/// connection each); `events()` and panes run on their own threads.
pub struct Core {
    ep: Endpoint,
    spawned: Option<Spawned>,
    stop: Arc<AtomicBool>,
}

impl Drop for Core {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

impl Core {
    /// Connects to `opts.connect`, or spawns `nk serve --listen
    /// tcp://127.0.0.1:0 [--demo]` with a private XDG_RUNTIME_DIR (so its
    /// token never clobbers a user's), reads the address it prints and the
    /// token file, and waits until `/v1/hello` answers.
    pub fn start(opts: StartOpts) -> Result<Core> {
        let stop = Arc::new(AtomicBool::new(false));
        if let Some(spec) = &opts.connect {
            let addr = Addr::parse(spec)?;
            let token = if addr.is_tcp() {
                Some(match std::env::var("NEKO_TOKEN") {
                    Ok(t) if !t.trim().is_empty() => t.trim().to_string(),
                    _ => read_token(&default_token_path())?,
                })
            } else {
                None
            };
            let core = Core { ep: Endpoint { addr, token }, spawned: None, stop };
            core.wait_hello(Duration::from_secs(5))?;
            return Ok(core);
        }
        let bin = opts.nk_bin.clone().unwrap_or_else(find_nk);
        let runtime = private_runtime_dir()?;
        let mut cmd = Command::new(&bin);
        cmd.arg("serve").arg("--listen").arg("tcp://127.0.0.1:0");
        if opts.demo {
            cmd.arg("--demo");
        }
        for (k, v) in &opts.env {
            cmd.env(k, v);
        }
        cmd.env("XDG_RUNTIME_DIR", &runtime).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
        let token_path = runtime.join(format!("nekuroddo-{}.token", uid()));
        let (spawned, stdout, stderr) =
            spawn_core(cmd, runtime).with_context(|| format!("rodando {}", bin.display()))?;
        let errlog = drain_stderr(stderr);
        let (addr_tx, addr_rx) = unbounded();
        thread::Builder::new().name("nk-core-stdout".into()).spawn(move || {
            let mut sent = false;
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if !sent && let Some(a) = parse_serving(&line) {
                    let _ = addr_tx.send(a);
                    sent = true;
                }
            }
        })?;
        let deadline = Instant::now() + Duration::from_secs(15);
        let addr = loop {
            if let Ok(a) = addr_rx.recv_timeout(Duration::from_millis(50)) {
                break a;
            }
            let why = match spawned.exited() {
                Some(st) => format!("o core saiu ({st})"),
                None if Instant::now() > deadline => "o core não disse onde escuta".into(),
                None => continue,
            };
            thread::sleep(Duration::from_millis(50)); // the last stderr lines
            let log = errlog.lock().unwrap_or_else(|p| p.into_inner()).iter().cloned().collect::<Vec<_>>().join("\n");
            bail!("{why}: {}{}", bin.display(), if log.is_empty() { String::new() } else { format!("\n{log}") });
        };
        let addr = Addr::parse(&addr)?;
        let token = read_token(&token_path)?;
        // on any error from here the Core drops and takes the child with it
        let core = Core { ep: Endpoint { addr, token: Some(token) }, spawned: Some(spawned), stop };
        core.wait_hello(Duration::from_secs(10))?;
        Ok(core)
    }

    fn wait_hello(&self, within: Duration) -> Result<()> {
        let deadline = Instant::now() + within;
        loop {
            match self.hello() {
                Ok(_) => return Ok(()),
                Err(e) if Instant::now() > deadline => return Err(e.context("o core não respondeu /v1/hello")),
                Err(_) => thread::sleep(Duration::from_millis(50)),
            }
        }
    }

    /// The address in use ("tcp://127.0.0.1:PORT" / "unix:///path").
    pub fn addr(&self) -> String {
        self.ep.addr.to_string()
    }

    /// The bearer token (TCP), e.g. to hand to `nk run --token`.
    pub fn token(&self) -> Option<&str> {
        self.ep.token.as_deref()
    }

    /// The pid of the core this `Core` spawned, if it did.
    pub fn child_pid(&self) -> Option<u32> {
        self.spawned.as_ref().map(|s| s.pid)
    }

    pub fn hello(&self) -> Result<Hello> {
        self.ep.call::<(), _>("GET", "/v1/hello", None)
    }

    pub fn snapshot(&self) -> Result<Snapshot> {
        self.ep.call::<(), _>("GET", "/v1/snapshot", None)
    }

    /// `POST /v1/view/tree`: send back the `state` of the last `Tree`.
    pub fn view_tree(&self, state: &ViewState) -> Result<Tree> {
        #[derive(Serialize)]
        struct Req<'a> {
            state: &'a ViewState,
        }
        self.ep.call("POST", "/v1/view/tree", Some(&Req { state }))
    }

    /// The last `n` (1..200) activity lines of a session, oldest first.
    pub fn tail(&self, session_id: &str, n: usize) -> Result<Vec<TailLine>> {
        let path = format!("/v1/sessions/{}/tail?n={}", encode(session_id), n.clamp(1, 200));
        Ok(self.ep.call::<(), Tail>("GET", &path, None)?.lines)
    }

    /// A theme by name, or the config's (`None`).
    pub fn theme(&self, name: Option<&str>) -> Result<Theme> {
        let path = match name {
            Some(n) => format!("/v1/theme?name={}", encode(n)),
            None => "/v1/theme".into(),
        };
        self.ep.call::<(), _>("GET", &path, None)
    }

    pub fn keymap(&self) -> Result<Vec<Binding>> {
        self.ep.call::<(), _>("GET", "/v1/keymap", None)
    }

    /// Every command, with `available` computed for `ctx`.
    pub fn commands(&self, ctx: &Context) -> Result<Vec<CommandInfo>> {
        let mut q: Vec<String> = Vec::new();
        if !ctx.session_id.is_empty() {
            q.push(format!("session_id={}", encode(&ctx.session_id)));
        }
        if !ctx.picked.is_empty() {
            q.push(format!("picked={}", ctx.picked.iter().map(|p| encode(p)).collect::<Vec<_>>().join(",")));
        }
        if !ctx.tab.is_empty() {
            q.push(format!("tab={}", encode(&ctx.tab)));
        }
        if ctx.todo_id != 0 {
            q.push(format!("todo_id={}", ctx.todo_id));
        }
        if !ctx.issue_key.is_empty() {
            q.push(format!("issue_key={}", encode(&ctx.issue_key)));
        }
        let path = if q.is_empty() { "/v1/commands".to_string() } else { format!("/v1/commands?{}", q.join("&")) };
        self.ep.call::<(), _>("GET", &path, None)
    }

    /// Runs a core command. A `pending` in the result asks for `confirm`.
    pub fn run(&self, name: &str, args: &BTreeMap<String, String>, ctx: &Context) -> Result<CmdResult> {
        #[derive(Serialize)]
        struct Req<'a> {
            #[serde(skip_serializing_if = "BTreeMap::is_empty")]
            args: &'a BTreeMap<String, String>,
            context: &'a Context,
        }
        let path = format!("/v1/commands/{}", encode(name));
        self.ep.call("POST", &path, Some(&Req { args, context: ctx }))
    }

    /// Answers a pending confirmation: `yes` runs the parked action and
    /// returns its result, `no` cancels (an empty result).
    pub fn confirm(&self, token: &str, yes: bool) -> Result<CmdResult> {
        #[derive(Serialize)]
        struct Req {
            yes: bool,
        }
        let path = format!("/v1/confirm/{}", encode(token));
        self.ep.call("POST", &path, Some(&Req { yes }))
    }

    /// The panes this server opened.
    pub fn panes(&self) -> Result<Vec<PaneInfo>> {
        self.ep.call::<(), _>("GET", "/v1/panes", None)
    }

    /// The core's event bus. Each call opens its own SSE stream on a
    /// background thread, which reconnects by itself with Last-Event-ID.
    /// Right after a connection that could not resume (the first one, or a
    /// reconnect before any event arrived) and on a `gap`, it sends
    /// `Event::Resync`: re-read the snapshot then. The thread ends when the
    /// receiver is dropped (noticed on the next event) or the `Core` is.
    pub fn events(&self) -> Receiver<Event> {
        let (tx, rx) = unbounded();
        let (ep, stop) = (self.ep.clone(), self.stop.clone());
        let _ = thread::Builder::new().name("nk-events".into()).spawn(move || events_loop(&ep, &stop, &tx));
        rx
    }

    /// Opens (or reuses) the pane of a session and attaches to it.
    pub fn open_pane(&self, session_id: &str, cols: u16, rows: u16) -> Result<PaneConn> {
        self.open_pane_reply(session_id, cols, rows, None)
    }

    /// `open_pane`, typing `reply` into the session once the pane is up
    /// (what the TUI does for a reply to a blocked session).
    pub fn open_pane_reply(&self, session_id: &str, cols: u16, rows: u16, reply: Option<&str>) -> Result<PaneConn> {
        #[derive(Serialize)]
        struct Req<'a> {
            session_id: &'a str,
            cols: u16,
            rows: u16,
            #[serde(skip_serializing_if = "Option::is_none")]
            reply: Option<&'a str>,
        }
        let opened: PaneOpened = self.ep.call("POST", "/v1/panes", Some(&Req { session_id, cols, rows, reply }))?;
        let pane = PaneConn::attach(&self.ep, &opened.pane_id, opened.reused)?;
        // a reused pane keeps its old size until someone resizes it
        if opened.reused {
            pane.resize(cols, rows);
        }
        Ok(pane)
    }

    /// Attaches to a pane already open on the server (`panes()`).
    pub fn attach_pane(&self, pane_id: &str) -> Result<PaneConn> {
        PaneConn::attach(&self.ep, pane_id, true)
    }
}

fn events_loop(ep: &Endpoint, stop: &AtomicBool, tx: &Sender<Event>) {
    let mut last: Option<String> = None;
    let mut fails = 0u32;
    loop {
        if stop.load(Ordering::SeqCst) || tx.send(Event::Status(ConnStatus::Connecting)).is_err() {
            return;
        }
        let mut headers = vec![("Accept", "text/event-stream")];
        if let Some(id) = &last {
            headers.push(("Last-Event-ID", id.as_str()));
        }
        // heartbeat is 15 s: a silent minute means a dead connection
        let mut retry = Duration::from_millis(1000);
        let why = match ep.send("GET", "/v1/events", None, &headers, Some(Duration::from_secs(45))) {
            Ok(resp) if resp.status == 200 => {
                fails = 0;
                if tx.send(Event::Status(ConnStatus::Connected)).is_err() {
                    return;
                }
                if last.is_none() && tx.send(Event::Resync).is_err() {
                    return;
                }
                let mut p = SseParser::new();
                let mut r = BufReader::new(resp.body);
                let mut line = String::new();
                let why = loop {
                    if stop.load(Ordering::SeqCst) {
                        return;
                    }
                    line.clear();
                    match r.read_line(&mut line) {
                        Ok(0) => break "o core fechou o fluxo de eventos".to_string(),
                        Ok(_) => {}
                        Err(e) => break e.to_string(),
                    }
                    if let Some(f) = p.line(line.trim_end_matches('\n')) {
                        if let Some(ev) = decode_frame(&f)
                            && tx.send(ev).is_err()
                        {
                            return;
                        }
                        if p.last_id.is_some() {
                            last = p.last_id.clone();
                        }
                    }
                };
                if let Some(ms) = p.retry {
                    retry = Duration::from_millis(ms.clamp(100, 30_000));
                }
                why
            }
            Ok(resp) => {
                fails += 1;
                let status = resp.status;
                let body = resp.text().unwrap_or_default();
                match crate::http::check(status, &body) {
                    Err(e) => e.to_string(),
                    Ok(()) => format!("HTTP {status}"),
                }
            }
            Err(e) => {
                fails += 1;
                format!("{e:#}")
            }
        };
        if stop.load(Ordering::SeqCst) || tx.send(Event::Status(ConnStatus::Disconnected(why))).is_err() {
            return;
        }
        let wait = retry.max(Duration::from_millis(250 * u64::from(fails.min(20))));
        let until = Instant::now() + wait;
        while Instant::now() < until {
            if stop.load(Ordering::SeqCst) {
                return;
            }
            thread::sleep(Duration::from_millis(50));
        }
    }
}

/// "nekuroddo: servindo em tcp://127.0.0.1:PORT" → the address.
fn parse_serving(line: &str) -> Option<String> {
    let i = line.find("servindo em ")?;
    let a = line[i + "servindo em ".len()..].trim();
    (a.starts_with("tcp://") || a.starts_with("unix://")).then(|| a.to_string())
}

fn drain_stderr(stderr: impl Read + Send + 'static) -> Arc<Mutex<VecDeque<String>>> {
    let log = Arc::new(Mutex::new(VecDeque::new()));
    let l = log.clone();
    let _ = thread::Builder::new().name("nk-core-stderr".into()).spawn(move || {
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            let mut q = l.lock().unwrap_or_else(|p| p.into_inner());
            if q.len() == 40 {
                q.pop_front();
            }
            q.push_back(line);
        }
    });
    log
}

fn uid() -> u32 {
    std::fs::metadata("/proc/self").map(|m| m.uid()).unwrap_or_else(|_| {
        // SAFETY: getuid never fails.
        unsafe { libc::getuid() }
    })
}

/// `rpc.DefaultTokenPath()`: $XDG_RUNTIME_DIR (or the temp dir)/nekuroddo-<uid>.token.
pub(crate) fn default_token_path() -> PathBuf {
    let dir = std::env::var_os("XDG_RUNTIME_DIR")
        .filter(|d| !d.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    dir.join(format!("nekuroddo-{}.token", uid()))
}

fn read_token(path: &Path) -> Result<String> {
    let t = std::fs::read_to_string(path).with_context(|| format!("lendo o token em {}", path.display()))?;
    let t = t.trim().to_string();
    if t.is_empty() {
        bail!("token vazio em {}", path.display());
    }
    Ok(t)
}

fn private_runtime_dir() -> Result<PathBuf> {
    let nanos =
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(0);
    let dir = std::env::temp_dir().join(format!("nkg-{}-{nanos:x}", std::process::id()));
    std::fs::DirBuilder::new()
        .mode(0o700)
        .recursive(true)
        .create(&dir)
        .with_context(|| format!("criando {}", dir.display()))?;
    Ok(dir)
}

fn find_nk() -> PathBuf {
    if let Some(path) = std::env::var_os("PATH") {
        for d in std::env::split_paths(&path) {
            let p = d.join("nk");
            if p.is_file() {
                return p;
            }
        }
    }
    if let Some(home) = std::env::var_os("HOME") {
        let p = PathBuf::from(home).join(".local/bin/nk");
        if p.is_file() {
            return p;
        }
    }
    PathBuf::from("nk")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serving_line() {
        assert_eq!(
            parse_serving("nekuroddo: servindo em tcp://127.0.0.1:37869").as_deref(),
            Some("tcp://127.0.0.1:37869")
        );
        assert_eq!(parse_serving("nekuroddo: servindo em unix:///run/x.sock\n").as_deref(), Some("unix:///run/x.sock"));
        assert_eq!(parse_serving("outra coisa"), None);
    }

    #[test]
    fn token_path_follows_runtime_dir() {
        let p = default_token_path();
        assert!(p.file_name().unwrap().to_string_lossy().starts_with("nekuroddo-"));
        assert!(p.to_string_lossy().ends_with(".token"));
    }

    #[test]
    fn spawn_failure_is_an_error() {
        let e =
            Core::start(StartOpts { nk_bin: Some("/nonexistent/nk".into()), ..StartOpts::default() }).err().unwrap();
        assert!(format!("{e:#}").contains("/nonexistent/nk"));
        let e = Core::start(StartOpts { nk_bin: Some("/bin/false".into()), ..StartOpts::default() }).err().unwrap();
        assert!(format!("{e:#}").contains("saiu"), "{e:#}");
    }

    fn sh(script: &str) -> (Spawned, PathBuf) {
        let runtime = private_runtime_dir().unwrap();
        let mut cmd = Command::new("/bin/sh");
        cmd.arg("-c").arg(script).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
        let (s, _out, _err) = spawn_core(cmd, runtime.clone()).unwrap();
        (s, runtime)
    }

    fn alive(pid: u32) -> bool {
        // a zombie still answers kill(0): look at its state instead
        std::fs::read_to_string(format!("/proc/{pid}/stat"))
            .map(|s| !s.rsplit_once(')').is_some_and(|(_, rest)| rest.trim_start().starts_with('Z')))
            .unwrap_or(false)
    }

    #[test]
    fn keeper_sees_the_child_exit_at_once() {
        let (s, _rt) = sh("sleep 0.2; exit 3");
        let t0 = Instant::now();
        while s.exited().is_none() && t0.elapsed() < Duration::from_secs(3) {
            thread::sleep(Duration::from_millis(5));
        }
        let st = s.exited().expect("exit never seen");
        assert!(st.contains('3'), "{st}");
        assert!(t0.elapsed() < Duration::from_millis(400), "{:?}", t0.elapsed());
    }

    #[test]
    fn dropping_the_keeper_kills_the_child_promptly() {
        let (s, rt) = sh("sleep 30");
        let pid = s.pid;
        assert!(alive(pid));
        let t0 = Instant::now();
        drop(s);
        assert!(t0.elapsed() < Duration::from_millis(300), "{:?}", t0.elapsed());
        assert!(!alive(pid), "child {pid} survived");
        assert!(!rt.exists(), "runtime dir left behind");

        // a child that ignores SIGTERM gets SIGKILL after the grace period
        let (s, _rt) = sh("trap '' TERM; while :; do sleep 0.05; done");
        let pid = s.pid;
        thread::sleep(Duration::from_millis(100)); // the trap is set
        let t0 = Instant::now();
        drop(s);
        let took = t0.elapsed();
        assert!(took >= Duration::from_millis(1500) && took < Duration::from_secs(3), "{took:?}");
        assert!(!alive(pid), "child {pid} survived");
    }

    #[test]
    fn connect_refused_is_an_error() {
        let e = Core::start(StartOpts { connect: Some("unix:///nonexistent/nk.sock".into()), ..StartOpts::default() });
        assert!(e.is_err());
    }
}
