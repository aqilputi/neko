//! Serde mirrors of the core's wire types (`internal/rpc/proto` and the core
//! types it embeds). Protocol v1 only grows additively, so every field has a
//! default, unknown fields are ignored and a JSON `null` (a nil Go slice or
//! map) reads as empty. Timestamps stay RFC 3339 strings.

use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer, Serialize};

/// Reads `null` as `T::default()` (Go marshals nil slices and maps as null).
fn nullable<'de, D, T>(d: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Default + Deserialize<'de>,
{
    Ok(Option::<T>::deserialize(d)?.unwrap_or_default())
}

fn is_zero(n: &i64) -> bool {
    *n == 0
}

/// `GET /v1/hello`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Hello {
    pub app: String,
    pub version: String,
    pub protocol: i64,
    #[serde(deserialize_with = "nullable")]
    pub capabilities: Vec<String>,
    pub pid: i64,
}

impl Hello {
    /// Whether the server announced a capability ("panes", "lua", "demo").
    pub fn has(&self, cap: &str) -> bool {
        self.capabilities.iter().any(|c| c == cap)
    }
}

/// The non-2xx body of every endpoint.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ErrorBody {
    pub error: String,
    pub code: String,
}

/// A numbered option of a pending question.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct QuestionOption {
    pub label: String,
    pub description: String,
}

/// A question a blocked session waits on (AskUserQuestion).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Question {
    pub question: String,
    #[serde(deserialize_with = "nullable")]
    pub options: Vec<QuestionOption>,
}

/// A link a background session reported (a PR, a doc…).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Link {
    pub id: String,
    pub href: String,
    pub kind: String,
    pub title: String,
}

/// A session's subagent.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Sub {
    pub id: String,
    pub description: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub model: String,
    pub last_text: String,
    pub cost: f64,
    pub last_at: String,
    pub done: bool,
}

/// One session of the snapshot (`proto.Session`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Session {
    pub session_id: String,
    pub short_id: String,
    pub label: String,
    pub name: String,
    pub title: String,
    pub custom_name: String,
    pub cwd: String,
    pub context: String,
    pub context_set: bool,
    /// background | interactive | history
    pub kind: String,
    /// busy | blocked | done | stopped | idle | history
    pub state: String,
    /// 0 none, 1 blocked, 2 returned, 3 idle
    pub attention: i64,
    pub pid: i64,
    pub started: String,
    pub last_at: String,
    pub model: String,
    pub cost: f64,
    pub window_cost: f64,
    pub cost_today: f64,
    pub tokens: i64,
    pub first_user: String,
    pub last_user: String,
    pub last_text: String,
    pub last_tool: String,
    pub waiting_tool: String,
    pub waiting_for: String,
    pub detail: String,
    pub needs: String,
    pub suggested_reply: String,
    #[serde(deserialize_with = "nullable")]
    pub questions: Vec<Question>,
    #[serde(deserialize_with = "nullable")]
    pub links: Vec<Link>,
    pub result: String,
    #[serde(deserialize_with = "nullable")]
    pub subs: Vec<Sub>,
    pub stale: bool,
    pub archived: bool,
    pub archived_at: String,
    pub ended: bool,
    pub finished: bool,
    /// Activity lines so far; grows with every `session.tail`.
    pub tail_seq: i64,
}

/// A member of a group (`groups.Member`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Member {
    pub session_id: String,
    pub name: String,
    pub label: String,
    pub cwd: String,
}

/// The ✉ next to a session.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MailCue {
    pub at: String,
    pub sent: bool,
}

/// One line of the /usage report.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Limit {
    pub name: String,
    pub pct: i64,
    pub resets: String,
}

/// The last /usage report.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Usage {
    #[serde(deserialize_with = "nullable")]
    pub limits: Vec<Limit>,
    pub at: String,
    pub err: String,
}

/// A todo (`todo.Task`); refs and notes are kept raw.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Task {
    pub id: i64,
    pub title: String,
    pub desc: String,
    #[serde(deserialize_with = "nullable")]
    pub refs: Vec<serde_json::Value>,
    pub status: String,
    pub initiative: String,
    #[serde(deserialize_with = "nullable")]
    pub contexts: Vec<String>,
    #[serde(deserialize_with = "nullable")]
    pub tags: Vec<String>,
    pub when: Option<String>,
    pub due: Option<String>,
    pub priority: Option<i64>,
    pub waiting_on: Option<String>,
    #[serde(deserialize_with = "nullable")]
    pub links: Vec<String>,
    #[serde(deserialize_with = "nullable")]
    pub notes: Vec<serde_json::Value>,
}

/// A group of todos (`todo.Group`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TodoGroup {
    pub group: String,
    #[serde(deserialize_with = "nullable")]
    pub tasks: Vec<Task>,
}

/// The todo list as last loaded.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Todos {
    #[serde(deserialize_with = "nullable")]
    pub groups: Vec<TodoGroup>,
    pub err: String,
}

/// A Jira issue (`jira.Issue`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Issue {
    pub key: String,
    pub summary: String,
    pub status: String,
    /// new | indeterminate | done
    pub status_cat: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub priority: String,
    pub updated: String,
    pub url: String,
    pub desc: String,
}

/// The Jira cache (`jira.Cache`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Jira {
    pub fetched_at: String,
    #[serde(deserialize_with = "nullable")]
    pub issues: Vec<Issue>,
    pub cost_usd: f64,
}

/// A confirmation waiting for an answer (`core.PendingConfirmation`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Pending {
    pub token: String,
    pub question: String,
    pub expires_at: String,
}

/// `GET /v1/snapshot`: everything a client needs to draw.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Snapshot {
    #[serde(deserialize_with = "nullable")]
    pub sessions: Vec<Session>,
    #[serde(deserialize_with = "nullable")]
    pub inbox: Vec<String>,
    /// tag → members
    #[serde(deserialize_with = "nullable")]
    pub groups: BTreeMap<String, Vec<Member>>,
    /// session id → ✉ cue
    #[serde(deserialize_with = "nullable")]
    pub mail: BTreeMap<String, MailCue>,
    pub usage: Usage,
    pub todos: Todos,
    pub jira: Jira,
    #[serde(deserialize_with = "nullable")]
    pub pending: Vec<Pending>,
    pub at: String,
    pub err: String,
    #[serde(rename = "today_cost")]
    pub today_cost: f64,
}

impl Snapshot {
    /// The session with this id.
    pub fn session(&self, id: &str) -> Option<&Session> {
        self.sessions.iter().find(|s| s.session_id == id)
    }
}

/// One line of a session's activity tail.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TailLine {
    pub at: String,
    /// user | text | tool | result | sub
    pub kind: String,
    pub text: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub(crate) struct Tail {
    #[serde(deserialize_with = "nullable")]
    pub lines: Vec<TailLine>,
}

/// `GET /v1/theme`: a palette as "#rrggbb" tokens.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Theme {
    pub name: String,
    /// bg, surface, overlay, border, text, subtle, muted, accent, accent2,
    /// busy, blocked, done, idle, error, cost_low, cost_high, cat_body, cat_eyes
    #[serde(deserialize_with = "nullable")]
    pub tokens: BTreeMap<String, String>,
    /// Context/group palette: index i uses `contexts[i % len]`.
    #[serde(deserialize_with = "nullable")]
    pub contexts: Vec<String>,
    /// Every theme name, in cycling order.
    #[serde(deserialize_with = "nullable")]
    pub themes: Vec<String>,
    /// The theme the core's config asks for.
    pub active: String,
}

impl Theme {
    /// A token as RGB.
    pub fn rgb(&self, token: &str) -> Option<[u8; 3]> {
        self.tokens.get(token).and_then(|h| parse_hex(h))
    }

    /// The context/group color for a palette index (`TreeRow::color`,
    /// `RowGroup::color`).
    pub fn context_rgb(&self, index: i64) -> Option<[u8; 3]> {
        if self.contexts.is_empty() {
            return None;
        }
        let i = index.rem_euclid(self.contexts.len() as i64) as usize;
        parse_hex(&self.contexts[i])
    }
}

/// Parses "#rrggbb" (or "rrggbb").
pub fn parse_hex(s: &str) -> Option<[u8; 3]> {
    let h = s.strip_prefix('#').unwrap_or(s);
    if h.len() != 6 || !h.is_ascii() {
        return None;
    }
    let b = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).ok();
    Some([b(0)?, b(2)?, b(4)?])
}

/// How a binding shows in the help bar.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Help {
    pub key: String,
    pub desc: String,
}

/// One entry of `GET /v1/keymap`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Binding {
    pub command: String,
    #[serde(deserialize_with = "nullable")]
    pub keys: Vec<String>,
    /// normal | normal.todos | insert | "*"
    pub mode: String,
    pub help: Help,
}

/// An argument of a command.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ArgInfo {
    pub name: String,
    /// string | session | dir | tag | int
    #[serde(rename = "type")]
    pub kind: String,
    pub prompt: String,
    pub required: bool,
}

/// One entry of `GET /v1/commands`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CommandInfo {
    pub name: String,
    pub title: String,
    pub hint: String,
    #[serde(deserialize_with = "nullable")]
    pub keys: Vec<String>,
    pub mode: String,
    #[serde(deserialize_with = "nullable")]
    pub args: Vec<ArgInfo>,
    /// core | client
    pub scope: String,
    pub confirm: bool,
    pub source: String,
    pub available: bool,
}

/// The caller's selection, sent with commands.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Context {
    #[serde(skip_serializing_if = "String::is_empty")]
    pub session_id: String,
    #[serde(skip_serializing_if = "Vec::is_empty", deserialize_with = "nullable")]
    pub picked: Vec<String>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub tab: String,
    #[serde(skip_serializing_if = "is_zero")]
    pub todo_id: i64,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub issue_key: String,
}

impl Context {
    /// A context selecting one session.
    pub fn session(id: impl Into<String>) -> Context {
        Context { session_id: id.into(), ..Context::default() }
    }
}

/// A fullscreen-exec request (TUI only; a GUI can ignore it).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ExecRequest {
    #[serde(deserialize_with = "nullable")]
    pub argv: Vec<String>,
    pub dir: String,
}

/// What a command (or a confirmation) hands back.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CmdResult {
    pub toast: String,
    pub err: bool,
    pub pending: Option<Pending>,
    pub exec: Option<ExecRequest>,
    /// Command-specific (e.g. `{close_panes, refresh}`); `Null` if absent.
    pub data: serde_json::Value,
}

/// A client's tree state (`proto.ViewState`); send back what the last
/// `Tree::state` had.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ViewState {
    /// sessions (default) | inbox | todos | archive
    #[serde(skip_serializing_if = "String::is_empty")]
    pub tab: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub filter: String,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub show_all: bool,
    /// atividade | custo | nome (empty: the config's)
    #[serde(skip_serializing_if = "String::is_empty")]
    pub sort: String,
    #[serde(skip_serializing_if = "BTreeMap::is_empty", deserialize_with = "nullable")]
    pub order: BTreeMap<String, i64>,
    #[serde(skip_serializing_if = "is_zero")]
    pub new_rank: i64,
    #[serde(skip_serializing_if = "BTreeMap::is_empty", deserialize_with = "nullable")]
    pub collapsed: BTreeMap<String, bool>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty", deserialize_with = "nullable")]
    pub ctx_color: BTreeMap<String, i64>,
    pub cursor: i64,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub cursor_key: String,
    pub inbox_cursor: i64,
    /// Request only: re-sort by the current criterion once.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub reset_order: bool,
}

/// A group a session row belongs to, with its collar color index.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RowGroup {
    pub tag: String,
    pub color: i64,
}

/// One line of the tree.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TreeRow {
    /// "ctx:<name>" | "sess:<id>" | "sub:<id>"
    pub key: String,
    /// ctx | sess | sub
    pub kind: String,
    pub ctx: String,
    pub depth: i64,
    pub color: i64,
    pub collapsed: bool,
    pub ctx_cost: f64,
    pub ctx_n: i64,
    pub session_id: String,
    pub short_id: String,
    pub label: String,
    pub session_kind: String,
    pub state: String,
    pub attention: i64,
    pub cost: f64,
    pub stale: bool,
    pub archived: bool,
    pub ended: bool,
    pub finished: bool,
    pub subs_running: i64,
    pub options: i64,
    #[serde(deserialize_with = "nullable")]
    pub groups: Vec<RowGroup>,
    pub mail: Option<MailCue>,
    /// blink | dim
    pub mail_state: String,
    pub sub: Option<Sub>,
}

/// `POST /v1/view/tree`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Tree {
    #[serde(deserialize_with = "nullable")]
    pub rows: Vec<TreeRow>,
    pub state: ViewState,
    #[serde(deserialize_with = "nullable")]
    pub inbox: Vec<String>,
    pub hidden: i64,
    pub archived: i64,
}

/// One pane this server opened (`GET /v1/panes`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PaneInfo {
    pub id: String,
    pub session_id: String,
    pub title: String,
    pub cols: i64,
    pub rows: i64,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub(crate) struct PaneOpened {
    pub pane_id: String,
    pub reused: bool,
}

/// The data of one SSE message (`proto.Event`).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub(crate) struct WireEvent {
    pub seq: u64,
    pub name: String,
    pub data: serde_json::Value,
}

/// A text control frame of the pane WebSocket.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Control {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(skip_serializing_if = "is_zero")]
    pub cols: i64,
    #[serde(skip_serializing_if = "is_zero")]
    pub rows: i64,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub text: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub err: String,
}
