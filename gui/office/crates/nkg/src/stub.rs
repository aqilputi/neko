//! A tiny stand-in for the `nk-client` crate (same public API, fake demo
//! data), used only when the `client` feature is off. Lets the GUI build and
//! run before the real client lands; delete once `nk-client` is the only path.

#![allow(dead_code)]

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, Sender, unbounded};

#[derive(Default)]
pub struct StartOpts {
    pub demo: bool,
    pub connect: Option<String>,
    pub nk_bin: Option<std::path::PathBuf>,
}

#[derive(Clone, Debug, Default)]
pub struct Hello {
    pub app: String,
    pub version: String,
    pub protocol: i64,
    pub capabilities: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub struct Sub {
    pub id: String,
    pub description: String,
    pub r#type: String,
    pub done: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Session {
    pub session_id: String,
    pub short_id: String,
    pub label: String,
    pub name: String,
    pub cwd: String,
    pub context: String,
    pub kind: String,
    pub state: String,
    pub attention: i64,
    pub last_at: String,
    pub needs: String,
    pub cost: f64,
    pub subs: Vec<Sub>,
    pub stale: bool,
    pub archived: bool,
    pub ended: bool,
    pub finished: bool,
}

#[derive(Clone, Debug, Default)]
pub struct MailCue {
    pub at: String,
    pub sent: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Limit {
    pub name: String,
    pub pct: i64,
}

#[derive(Clone, Debug, Default)]
pub struct Usage {
    pub limits: Vec<Limit>,
}

#[derive(Clone, Debug, Default)]
pub struct Member {
    pub session_id: String,
}

#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub sessions: Vec<Session>,
    pub inbox: Vec<String>,
    pub groups: BTreeMap<String, Vec<Member>>,
    pub mail: BTreeMap<String, MailCue>,
    pub usage: Usage,
    pub today_cost: f64,
}

#[derive(Clone, Debug, Default)]
pub struct TailLine {
    pub at: String,
    pub kind: String,
    pub text: String,
}

#[derive(Clone, Debug, Default)]
pub struct Theme {
    pub name: String,
    pub tokens: BTreeMap<String, String>,
    pub contexts: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub struct Help {
    pub key: String,
    pub desc: String,
}

#[derive(Clone, Debug, Default)]
pub struct Binding {
    pub command: String,
    pub keys: Vec<String>,
    pub mode: String,
    pub help: Help,
}

#[derive(Clone, Debug, Default)]
pub struct CommandInfo {
    pub name: String,
    pub title: String,
    pub scope: String,
    pub available: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Context {
    pub session_id: String,
    pub picked: Vec<String>,
    pub tab: String,
    pub todo_id: i64,
    pub issue_key: String,
}

#[derive(Clone, Debug, Default)]
pub struct Pending {
    pub token: String,
    pub question: String,
    pub expires_at: String,
}

#[derive(Clone, Debug, Default)]
pub struct CmdResult {
    pub toast: String,
    pub err: bool,
    pub pending: Option<Pending>,
    pub data: serde_json::Value,
}

#[derive(Clone, Debug, Default)]
pub struct ViewState {
    pub tab: String,
}

#[derive(Clone, Debug, Default)]
pub struct GroupTag {
    pub tag: String,
    pub color: i64,
}

#[derive(Clone, Debug, Default)]
pub struct TreeRow {
    pub key: String,
    pub kind: String,
    pub ctx: String,
    pub depth: i64,
    pub color: i64,
    pub session_id: String,
    pub label: String,
    pub state: String,
    pub attention: i64,
    pub groups: Vec<GroupTag>,
    pub mail_state: String,
}

#[derive(Clone, Debug, Default)]
pub struct Tree {
    pub rows: Vec<TreeRow>,
    pub state: ViewState,
    pub inbox: Vec<String>,
    pub hidden: i64,
    pub archived: i64,
}

#[derive(Clone, Debug)]
pub enum ConnStatus {
    Connecting,
    Connected,
    Disconnected(String),
}

#[derive(Clone, Debug)]
pub enum Event {
    Core {
        seq: u64,
        name: String,
        data: serde_json::Value,
    },
    Resync,
    Status(ConnStatus),
}

pub enum PaneMsg {
    Data(Vec<u8>),
    Title(String),
    Exit(Option<String>),
    Closed,
}

struct World {
    t0: Instant,
}

pub struct Core {
    world: Arc<Mutex<World>>,
}

const DEMO: &[(&str, &str, &str, &str, i64, usize)] = &[
    ("s1", "api-gateway", "visio", "busy", 0, 0),
    ("s2", "migração do banco", "visio", "blocked", 1, 0),
    ("s3", "relatório Q3", "visio", "done", 2, 0),
    ("s4", "dotfiles", "pessoal", "idle", 3, 0),
    ("s5", "indexador", "oss", "busy", 0, 2),
    ("s6", "site de docs", "oss", "busy", 0, 0),
];

const LINES: &[(&str, &str)] = &[
    ("user", "> roda a suíte de integração antes de subir"),
    ("text", "Vou rodar os testes de integração primeiro."),
    ("tool", "● Bash(go test ./... -tags integration)"),
    ("result", "  ⎿ ok   gateway/ratelimit  0.41s … +12 lines"),
    ("tool", "● Edit(internal/ratelimit/tenant.go)"),
    ("result", "  ⎿ Updated with 14 additions and 3 removals"),
    ("text", "O limite por tenant agora usa um token bucket."),
    ("tool", "● Read(internal/ratelimit/bucket.go)"),
    ("result", "  ⎿ 212 lines"),
    ("sub", "◆ Explore: mapear os módulos"),
    ("tool", "● Grep(\"func New\")"),
    ("result", "  ⎿ 9 lines"),
];

fn ts(secs_ago: u64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        - secs_ago;
    crate::model::format_rfc3339(now as i64 * 1000)
}

impl Core {
    pub fn start(_opts: StartOpts) -> anyhow::Result<Core> {
        Ok(Core {
            world: Arc::new(Mutex::new(World { t0: Instant::now() })),
        })
    }
    fn phase(&self) -> u64 {
        self.world.lock().unwrap().t0.elapsed().as_secs() / 15
    }
    pub fn hello(&self) -> anyhow::Result<Hello> {
        Ok(Hello {
            app: "nekuroddo".into(),
            version: "stub".into(),
            protocol: 1,
            capabilities: vec!["demo".into()],
        })
    }
    pub fn snapshot(&self) -> anyhow::Result<Snapshot> {
        let flip = self.phase() % 2 == 1;
        let sessions = DEMO
            .iter()
            .enumerate()
            .map(|(i, (id, label, ctx, state, att, subs))| {
                let (state, att) = if *id == "s1" && flip {
                    ("blocked", 1)
                } else {
                    (*state, *att)
                };
                Session {
                    session_id: id.to_string(),
                    short_id: id.to_string(),
                    label: label.to_string(),
                    name: label.to_string(),
                    context: ctx.to_string(),
                    kind: "background".into(),
                    state: state.into(),
                    attention: att,
                    last_at: ts(60 * i as u64),
                    needs: if att == 1 {
                        "decidir a migração".into()
                    } else {
                        String::new()
                    },
                    subs: (0..*subs)
                        .map(|k| Sub {
                            id: format!("{id}-k{k}"),
                            description: "explorar".into(),
                            r#type: "Explore".into(),
                            done: false,
                        })
                        .collect(),
                    ..Default::default()
                }
            })
            .collect();
        let mut mail = BTreeMap::new();
        mail.insert(
            "s5".into(),
            MailCue {
                at: ts(10),
                sent: false,
            },
        );
        Ok(Snapshot {
            sessions,
            mail,
            usage: Usage {
                limits: vec![Limit {
                    name: "5h".into(),
                    pct: 61,
                }],
            },
            ..Default::default()
        })
    }
    pub fn view_tree(&self, _state: &ViewState) -> anyhow::Result<Tree> {
        let snap = self.snapshot()?;
        let mut rows = vec![];
        let mut ctx_idx: Vec<String> = vec![];
        for s in &snap.sessions {
            if !ctx_idx.contains(&s.context) {
                ctx_idx.push(s.context.clone());
                rows.push(TreeRow {
                    key: format!("ctx:{}", s.context),
                    kind: "ctx".into(),
                    ctx: s.context.clone(),
                    color: ctx_idx.len() as i64 - 1,
                    ..Default::default()
                });
            }
            rows.push(TreeRow {
                key: format!("sess:{}", s.session_id),
                kind: "sess".into(),
                ctx: s.context.clone(),
                depth: 1,
                color: ctx_idx.len() as i64 - 1,
                session_id: s.session_id.clone(),
                label: s.label.clone(),
                state: s.state.clone(),
                attention: s.attention,
                groups: if ["s2", "s5"].contains(&s.session_id.as_str()) {
                    vec![GroupTag {
                        tag: "deploy-rc".into(),
                        color: 4,
                    }]
                } else {
                    vec![]
                },
                mail_state: String::new(),
            });
        }
        Ok(Tree {
            rows,
            state: ViewState {
                tab: "sessions".into(),
            },
            ..Default::default()
        })
    }
    pub fn tail(&self, session_id: &str, n: usize) -> anyhow::Result<Vec<TailLine>> {
        let el = self.world.lock().unwrap().t0.elapsed().as_millis() as usize / 900;
        let seed = session_id.bytes().map(|b| b as usize).sum::<usize>();
        let count = 20 + el;
        let out = (0..count)
            .map(|i| {
                let (k, t) = LINES[(i + seed) % LINES.len()];
                TailLine {
                    at: format!("{i}"),
                    kind: k.into(),
                    text: t.into(),
                }
            })
            .collect::<Vec<_>>();
        let skip = out.len().saturating_sub(n);
        Ok(out[skip..].to_vec())
    }
    pub fn theme(&self, _name: Option<&str>) -> anyhow::Result<Theme> {
        let mut tokens = BTreeMap::new();
        for (k, v) in [
            ("bg", "#1e1e2e"),
            ("surface", "#313244"),
            ("overlay", "#45475a"),
            ("border", "#585b70"),
            ("text", "#cdd6f4"),
            ("subtle", "#a6adc8"),
            ("muted", "#7f849c"),
            ("accent", "#cba6f7"),
            ("accent2", "#89b4fa"),
            ("busy", "#89dceb"),
            ("blocked", "#f38ba8"),
            ("done", "#a6e3a1"),
            ("idle", "#9399b2"),
            ("error", "#eba0ac"),
            ("cat_body", "#11111b"),
            ("cat_eyes", "#f9e2af"),
        ] {
            tokens.insert(k.to_string(), v.to_string());
        }
        Ok(Theme {
            name: "catppuccin-mocha".into(),
            tokens,
            contexts: [
                "#f5c2e7", "#89b4fa", "#a6e3a1", "#fab387", "#94e2d5", "#f9e2af", "#b4befe",
            ]
            .iter()
            .map(|s| s.to_string())
            .collect(),
        })
    }
    pub fn keymap(&self) -> anyhow::Result<Vec<Binding>> {
        Ok(vec![])
    }
    pub fn commands(&self, _ctx: &Context) -> anyhow::Result<Vec<CommandInfo>> {
        Ok(vec![])
    }
    pub fn run(
        &self,
        name: &str,
        _args: &BTreeMap<String, String>,
        ctx: &Context,
    ) -> anyhow::Result<CmdResult> {
        if name == "session.stop" {
            return Ok(CmdResult {
                pending: Some(Pending {
                    token: "t1".into(),
                    question: format!("parar {}?", ctx.session_id),
                    expires_at: String::new(),
                }),
                ..Default::default()
            });
        }
        Ok(CmdResult {
            toast: format!("(stub) {name}"),
            ..Default::default()
        })
    }
    pub fn confirm(&self, _token: &str, yes: bool) -> anyhow::Result<CmdResult> {
        Ok(CmdResult {
            toast: if yes { "ok".into() } else { "cancelado".into() },
            ..Default::default()
        })
    }
    pub fn events(&self) -> Receiver<Event> {
        let (tx, rx) = unbounded();
        let w = self.world.clone();
        std::thread::spawn(move || {
            let mut seq = 0;
            loop {
                std::thread::sleep(Duration::from_millis(900));
                seq += 1;
                let _ = w.lock().unwrap().t0;
                for id in ["s1", "s5", "s6"] {
                    if tx
                        .send(Event::Core {
                            seq,
                            name: "session.tail".into(),
                            data: serde_json::json!({"id": id, "n_new": 1}),
                        })
                        .is_err()
                    {
                        return;
                    }
                }
                if seq % 15 == 0
                    && tx
                        .send(Event::Core {
                            seq,
                            name: "snapshot.changed".into(),
                            data: serde_json::Value::Null,
                        })
                        .is_err()
                {
                    return;
                }
            }
        });
        rx
    }
    pub fn open_pane(&self, session_id: &str, cols: u16, rows: u16) -> anyhow::Result<PaneConn> {
        let (tx, rx) = unbounded();
        let _ = tx.send(PaneMsg::Data(
            format!(
                "\x1b[1;35m(stub)\x1b[0m pane for {session_id} {cols}x{rows}\r\n\x1b[32m❯\x1b[0m "
            )
            .into_bytes(),
        ));
        Ok(PaneConn {
            id: format!("p-{session_id}"),
            tx,
            rx,
        })
    }
}

pub struct PaneConn {
    id: String,
    tx: Sender<PaneMsg>,
    rx: Receiver<PaneMsg>,
}

impl PaneConn {
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn write(&self, bytes: &[u8]) {
        // local echo
        let mut out = Vec::new();
        for &b in bytes {
            if b == b'\r' {
                out.extend_from_slice(b"\r\n\x1b[32m\xe2\x9d\xaf\x1b[0m ");
            } else {
                out.push(b);
            }
        }
        let _ = self.tx.send(PaneMsg::Data(out));
    }
    pub fn resize(&self, _cols: u16, _rows: u16) {}
    pub fn rx(&self) -> &Receiver<PaneMsg> {
        &self.rx
    }
    pub fn close(&self, _kill: bool) {
        let _ = self.tx.send(PaneMsg::Closed);
    }
}

// ---- terminal emulation stub: a naive line grid (no escapes beyond SGR stripping)

#[derive(Clone, Debug, Default)]
pub struct Cell {
    pub ch: char,
    pub fg: [u8; 3],
    pub bg: [u8; 3],
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub inverse: bool,
    pub wide: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Grid {
    pub cols: u16,
    pub rows: u16,
    pub cells: Vec<Cell>,
    pub cursor: Option<(u16, u16)>,
}

pub struct Term {
    cols: u16,
    rows: u16,
    lines: Vec<String>,
    changed: bool,
}

impl Term {
    pub fn new(cols: u16, rows: u16) -> Term {
        Term {
            cols,
            rows,
            lines: vec![String::new()],
            changed: true,
        }
    }
    pub fn feed(&mut self, bytes: &[u8]) {
        let s = String::from_utf8_lossy(bytes);
        let mut esc = false;
        for ch in s.chars() {
            if esc {
                if ch.is_ascii_alphabetic() {
                    esc = false;
                }
                continue;
            }
            match ch {
                '\x1b' => esc = true,
                '\r' => {}
                '\n' => self.lines.push(String::new()),
                '\x7f' | '\x08' => {
                    self.lines.last_mut().unwrap().pop();
                }
                c if !c.is_control() => self.lines.last_mut().unwrap().push(c),
                _ => {}
            }
        }
        self.changed = true;
    }
    pub fn resize(&mut self, cols: u16, rows: u16) {
        self.cols = cols;
        self.rows = rows;
        self.changed = true;
    }
    pub fn grid(&self) -> Grid {
        let (c, r) = (self.cols as usize, self.rows as usize);
        let mut cells = vec![
            Cell {
                ch: ' ',
                fg: [205, 214, 244],
                bg: [30, 30, 46],
                ..Default::default()
            };
            c * r
        ];
        let start = self.lines.len().saturating_sub(r);
        for (y, line) in self.lines[start..].iter().enumerate() {
            for (x, ch) in line.chars().take(c).enumerate() {
                cells[y * c + x].ch = ch;
            }
        }
        let cy = (self.lines.len() - start - 1) as u16;
        let cx = self.lines.last().map(|l| l.chars().count()).unwrap_or(0) as u16;
        Grid {
            cols: self.cols,
            rows: self.rows,
            cells,
            cursor: Some((cx.min(self.cols.saturating_sub(1)), cy)),
        }
    }
    pub fn title(&self) -> Option<String> {
        None
    }
    pub fn changed(&mut self) -> bool {
        std::mem::take(&mut self.changed)
    }
}

pub enum Key {
    Char(char),
    Enter,
    Tab,
    Backspace,
    Esc,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
    Insert,
    Delete,
    F(u8),
}

#[derive(Clone, Copy, Default)]
pub struct Mods {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
}

pub fn encode_key(key: Key, mods: Mods) -> Vec<u8> {
    let mut out = Vec::new();
    if mods.alt {
        out.push(0x1b);
    }
    match key {
        Key::Char(c) if mods.ctrl && c.is_ascii_alphabetic() => {
            out.push((c.to_ascii_lowercase() as u8) & 0x1f)
        }
        Key::Char(c) => out.extend_from_slice(c.to_string().as_bytes()),
        Key::Enter => out.push(b'\r'),
        Key::Tab => out.push(b'\t'),
        Key::Backspace => out.push(0x7f),
        Key::Esc => out.push(0x1b),
        Key::Up => out.extend_from_slice(b"\x1b[A"),
        Key::Down => out.extend_from_slice(b"\x1b[B"),
        Key::Right => out.extend_from_slice(b"\x1b[C"),
        Key::Left => out.extend_from_slice(b"\x1b[D"),
        _ => {}
    }
    out
}

pub fn key_name(key: &Key, mods: Mods) -> String {
    let base = match key {
        Key::Char(' ') => "space".to_string(),
        Key::Char(c) => c.to_string(),
        Key::Enter => "enter".into(),
        Key::Tab => "tab".into(),
        Key::Backspace => "backspace".into(),
        Key::Esc => "esc".into(),
        Key::Up => "up".into(),
        Key::Down => "down".into(),
        Key::Left => "left".into(),
        Key::Right => "right".into(),
        Key::Home => "home".into(),
        Key::End => "end".into(),
        Key::PageUp => "pgup".into(),
        Key::PageDown => "pgdown".into(),
        Key::Insert => "insert".into(),
        Key::Delete => "delete".into(),
        Key::F(n) => format!("f{n}"),
    };
    let mut s = String::new();
    if mods.ctrl {
        s.push_str("ctrl+");
    }
    if mods.alt {
        s.push_str("alt+");
    }
    let is_char = matches!(key, Key::Char(c) if *c != ' ');
    if mods.shift && !is_char {
        s.push_str("shift+");
    }
    s + &base
}
