//! Client of the nekuroddo core over protocol v1 (`docs/protocolo.md`):
//! HTTP+JSON calls, the SSE event bus, pane WebSockets, terminal emulation
//! for panes and key encoding for the PTY. Blocking calls plus background
//! threads and crossbeam channels; no async runtime.
//!
//! ```no_run
//! use nk_client::{Core, StartOpts, Event};
//! let core = Core::start(StartOpts { demo: true, ..Default::default() })?;
//! let snap = core.snapshot()?;
//! let events = core.events();
//! while let Ok(ev) = events.recv() {
//!     if let Event::Resync = ev { /* re-read the snapshot */ }
//! }
//! # anyhow::Ok(())
//! ```

mod core;
mod http;
mod keys;
mod pane;
mod proto;
mod sse;
mod term;

pub use crate::core::{Core, StartOpts};
pub use crate::http::{ApiError, error_code};
pub use crate::keys::{Key, Mods, encode_key, encode_key_mode, key_name};
pub use crate::pane::{PaneConn, PaneMsg};
pub use crate::proto::{
    ArgInfo, Binding, CmdResult, CommandInfo, Context, ErrorBody, ExecRequest, Hello, Help, Issue, Jira, Limit, Link,
    MailCue, Member, PaneInfo, Pending, Question, QuestionOption, RowGroup, Session, Snapshot, Sub, TailLine, Task,
    Theme, TodoGroup, Todos, Tree, TreeRow, Usage, ViewState, parse_hex,
};
pub use crate::sse::{ConnStatus, Event, SseFrame, SseParser, decode_frame};
pub use crate::term::{Cell, Grid, Term};

/// Re-exported so callers need not depend on crossbeam-channel themselves.
pub use crossbeam_channel;
