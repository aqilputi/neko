//! Key routing: key names (core keymap notation: "h", "G", "ctrl+d",
//! "shift+space", "enter") → what to do, resolved against the core's keymap
//! (modes "normal" and "*"), with multi-key sequences ("dd", "gg").
//! Client-handled commands (nav, zoom, quit…) map to local actions; the rest
//! go to the core with the focused session as context.

use std::collections::HashSet;

/// Something the GUI does by itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Client {
    Left,
    Right,
    NextCtx,
    PrevCtx,
    First,
    Last,
    Zoom,
    Back,
    Close,
    ToggleBars,
    Insert,
    NextAttention,
    PrevAttention,
    Quit,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Route {
    Client(Client),
    /// run this core command (context = focused session)
    Core(String),
    /// a client command the GUI doesn't do (tabs, filter, palette…)
    Unsupported(String),
    /// a prefix of a longer binding: wait for the next key
    Pending,
    /// nothing bound
    None,
}

/// One binding: the keys (each possibly a sequence) of a command in a mode.
#[derive(Clone, Debug, PartialEq)]
pub struct Bind {
    pub command: String,
    pub keys: Vec<String>,
    pub mode: String,
    pub help: String,
}

const NAMED: &[&str] = &[
    "enter",
    "esc",
    "tab",
    "space",
    "backspace",
    "delete",
    "insert",
    "up",
    "down",
    "left",
    "right",
    "home",
    "end",
    "pgup",
    "pgdown",
];

/// Splits a binding key into the key names it is typed as: "dd" → [d, d],
/// "gg" → [g, g], "ctrl+d" → [ctrl+d], "enter" → [enter], "f5" → [f5].
pub fn split_seq(k: &str) -> Vec<String> {
    if k.chars().count() <= 1 || k.contains('+') || NAMED.contains(&k) || is_fkey(k) {
        return vec![k.to_string()];
    }
    k.chars().map(|c| c.to_string()).collect()
}

fn is_fkey(k: &str) -> bool {
    k.len() >= 2 && k.starts_with('f') && k[1..].chars().all(|c| c.is_ascii_digit())
}

/// The GUI's own meaning of a client-scope command, if it has one.
pub fn client_action(command: &str) -> Option<Client> {
    Some(match command {
        "nav.collapse" => Client::Left,
        "nav.expand" => Client::Right,
        "nav.down" => Client::NextCtx,
        "nav.up" => Client::PrevCtx,
        "nav.top" => Client::First,
        "nav.bottom" => Client::Last,
        "nav.next_attention" => Client::NextAttention,
        "nav.prev_attention" => Client::PrevAttention,
        "session.open" => Client::Zoom,
        "pane.close" => Client::Close,
        "pane.zoom" => Client::ToggleBars,
        "session.insert" => Client::Insert,
        "app.quit" => Client::Quit,
        _ => return None,
    })
}

/// Commands that are client-scope in the core even if the keymap's
/// CommandInfo list didn't come (prefix test).
fn looks_client(command: &str) -> bool {
    [
        "nav.",
        "tab.",
        "view.",
        "palette.",
        "help.",
        "switcher.",
        "pane.",
        "app.",
    ]
    .iter()
    .any(|p| command.starts_with(p))
        || command == "session.open"
        || command == "session.insert"
        || command == "session.preview"
        || command == "session.fullscreen"
}

/// Built-in bindings when the core's keymap is unavailable.
pub fn fallback() -> Vec<Bind> {
    let b = |c: &str, k: &[&str], h: &str| Bind {
        command: c.into(),
        keys: k.iter().map(|s| s.to_string()).collect(),
        mode: "normal".into(),
        help: h.into(),
    };
    vec![
        b("nav.collapse", &["h", "left"], "coluna ←"),
        b("nav.expand", &["l", "right"], "coluna →"),
        b("nav.down", &["j", "down"], "contexto ↓"),
        b("nav.up", &["k", "up"], "contexto ↑"),
        b("session.open", &["enter"], "zoom"),
        b("session.insert", &["i"], "INSERT"),
        b("pane.close", &["w"], "fecha"),
        b("pane.zoom", &["z"], "barras"),
        b("inbox.summon", &["shift+space"], "inbox"),
        b("app.quit", &["q", "ctrl+c"], "sai"),
    ]
}

pub struct Router {
    binds: Vec<(Vec<String>, String, String)>, // (sequence, command, mode)
    client: HashSet<String>,
    buf: Vec<String>,
}

impl Router {
    /// `client_cmds`: the commands with scope "client" (from /v1/commands);
    /// empty = guess by name.
    pub fn new(binds: &[Bind], client_cmds: &HashSet<String>) -> Router {
        let mut out = Vec::new();
        for b in binds {
            if b.mode != "normal" && b.mode != "*" {
                continue;
            }
            for k in &b.keys {
                out.push((split_seq(k), b.command.clone(), b.mode.clone()));
            }
        }
        Router {
            binds: out,
            client: client_cmds.clone(),
            buf: Vec::new(),
        }
    }

    pub fn pending(&self) -> &[String] {
        &self.buf
    }

    /// Feeds one key (NORMAL mode, office or zoomed).
    pub fn feed(&mut self, key: &str) -> Route {
        if key == "esc" && self.buf.is_empty() {
            return Route::Client(Client::Back);
        }
        if key == "esc" {
            self.buf.clear();
            return Route::None;
        }
        self.buf.push(key.to_string());
        if let Some(r) = self.resolve() {
            return r;
        }
        // not a binding nor a prefix: retry with just this key
        if self.buf.len() > 1 {
            self.buf = vec![key.to_string()];
            if let Some(r) = self.resolve() {
                return r;
            }
        }
        self.buf.clear();
        Route::None
    }

    fn resolve(&mut self) -> Option<Route> {
        let exact = self
            .binds
            .iter()
            .find(|(seq, _, _)| *seq == self.buf)
            .map(|(_, c, _)| c.clone());
        let longer = self
            .binds
            .iter()
            .any(|(seq, _, _)| seq.len() > self.buf.len() && seq.starts_with(&self.buf));
        if let Some(cmd) = exact {
            if !longer || self.buf.len() > 1 {
                self.buf.clear();
                return Some(self.route_cmd(&cmd));
            }
            // e.g. "g" bound and "gg" bound: prefer waiting (rare); fall through
            return Some(Route::Pending);
        }
        if longer {
            return Some(Route::Pending);
        }
        None
    }

    fn route_cmd(&self, cmd: &str) -> Route {
        if let Some(a) = client_action(cmd) {
            return Route::Client(a);
        }
        let is_client = if self.client.is_empty() {
            looks_client(cmd)
        } else {
            self.client.contains(cmd)
        };
        if is_client {
            Route::Unsupported(cmd.to_string())
        } else {
            Route::Core(cmd.to_string())
        }
    }

    /// Key hints for the status bar: "key desc" of the most useful commands.
    pub fn hints(binds: &[Bind], zoomed: bool) -> Vec<(String, String)> {
        let want: &[(&str, &str)] = if zoomed {
            &[
                ("session.insert", "insert"),
                ("nav.collapse", "←"),
                ("nav.expand", "→"),
                ("pane.close", "fecha"),
                ("session.stop", "para"),
            ]
        } else {
            &[
                ("nav.collapse", "←"),
                ("nav.expand", "→"),
                ("nav.down", "contexto"),
                ("session.open", "zoom"),
                ("session.insert", "insert"),
                ("inbox.summon", "inbox"),
                ("session.new", "novo"),
                ("app.quit", "sai"),
            ]
        };
        let mut out = Vec::new();
        for (cmd, label) in want {
            if let Some(b) = binds
                .iter()
                .find(|b| b.command == *cmd && (b.mode == "normal" || b.mode == "*"))
                && let Some(k) = b.keys.first()
            {
                out.push((k.clone(), label.to_string()));
            }
        }
        if zoomed {
            out.push(("esc".into(), "escritório".into()));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn router() -> Router {
        let mut b = fallback();
        b.push(Bind {
            command: "session.remove".into(),
            keys: vec!["dd".into()],
            mode: "normal".into(),
            help: "".into(),
        });
        b.push(Bind {
            command: "nav.top".into(),
            keys: vec!["gg".into()],
            mode: "normal".into(),
            help: "".into(),
        });
        b.push(Bind {
            command: "nav.bottom".into(),
            keys: vec!["G".into()],
            mode: "normal".into(),
            help: "".into(),
        });
        b.push(Bind {
            command: "switcher.open".into(),
            keys: vec!["ctrl+k".into()],
            mode: "*".into(),
            help: "".into(),
        });
        b.push(Bind {
            command: "todo.done".into(),
            keys: vec!["D".into()],
            mode: "normal.todos".into(),
            help: "".into(),
        });
        b.push(Bind {
            command: "session.stop".into(),
            keys: vec!["x".into()],
            mode: "normal".into(),
            help: "".into(),
        });
        Router::new(&b, &HashSet::new())
    }

    #[test]
    fn splits() {
        assert_eq!(split_seq("dd"), vec!["d", "d"]);
        assert_eq!(split_seq("ctrl+d"), vec!["ctrl+d"]);
        assert_eq!(split_seq("enter"), vec!["enter"]);
        assert_eq!(split_seq("G"), vec!["G"]);
        assert_eq!(split_seq("f5"), vec!["f5"]);
        assert_eq!(split_seq("ctrl+/"), vec!["ctrl+/"]);
    }

    #[test]
    fn client_and_core() {
        let mut r = router();
        assert_eq!(r.feed("h"), Route::Client(Client::Left));
        assert_eq!(r.feed("right"), Route::Client(Client::Right));
        assert_eq!(r.feed("enter"), Route::Client(Client::Zoom));
        assert_eq!(r.feed("shift+space"), Route::Core("inbox.summon".into()));
        assert_eq!(r.feed("x"), Route::Core("session.stop".into()));
        assert_eq!(r.feed("ctrl+k"), Route::Unsupported("switcher.open".into()));
        assert_eq!(
            r.feed("D"),
            Route::None,
            "normal.todos is not the office's mode"
        );
        assert_eq!(r.feed("esc"), Route::Client(Client::Back));
    }

    #[test]
    fn sequences() {
        let mut r = router();
        assert_eq!(r.feed("d"), Route::Pending);
        assert_eq!(r.feed("d"), Route::Core("session.remove".into()));
        assert_eq!(r.feed("g"), Route::Pending);
        // a broken sequence retries the last key alone
        assert_eq!(r.feed("h"), Route::Client(Client::Left));
        assert!(r.pending().is_empty());
        assert_eq!(r.feed("g"), Route::Pending);
        assert_eq!(r.feed("esc"), Route::None, "esc cancels a sequence");
        assert_eq!(r.feed("g"), Route::Pending);
        assert_eq!(r.feed("g"), Route::Client(Client::First));
        assert_eq!(r.feed("G"), Route::Client(Client::Last));
        assert_eq!(r.feed("y"), Route::None);
    }

    #[test]
    fn scopes_from_the_core_win() {
        let mut client = HashSet::new();
        client.insert("usage.show".to_string());
        let b = vec![Bind {
            command: "usage.show".into(),
            keys: vec!["u".into()],
            mode: "normal".into(),
            help: "".into(),
        }];
        let mut r = Router::new(&b, &client);
        assert_eq!(r.feed("u"), Route::Unsupported("usage.show".into()));
    }

    #[test]
    fn hints_pick_known_commands() {
        let h = Router::hints(&fallback(), false);
        assert_eq!(h[0], ("h".to_string(), "←".to_string()));
        assert!(h.iter().any(|(k, _)| k == "shift+space"));
        let z = Router::hints(&fallback(), true);
        assert_eq!(z.last().unwrap().0, "esc");
    }
}
