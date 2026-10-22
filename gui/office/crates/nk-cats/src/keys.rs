//! Key routing: key names (core keymap notation: "h", "G", "ctrl+d",
//! "shift+space", "enter") → what to do, resolved against the core's keymap
//! (modes "normal" and "*"), with multi-key sequences ("dd", "gg").
//! Client-handled commands (nav, zoom, quit…) map to local actions; the rest
//! go to the core with the focused session as context.

use std::collections::{HashMap, HashSet};

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
    if k.chars().count() <= 1
        || k.contains('+')
        || NAMED.contains(&k)
        || is_fkey(k)
        || is_keysym_name(k)
    {
        return vec![k.to_string()];
    }
    k.chars().map(|c| c.to_string()).collect()
}

/// An XKB keysym name (`XF86AudioPlay`, `Print`, `Menu`): one key, not a
/// sequence of letters.
fn is_keysym_name(k: &str) -> bool {
    let mut cs = k.chars();
    k.starts_with("XF86")
        || (k.len() > 2
            && cs.next().is_some_and(|c| c.is_ascii_uppercase())
            && cs.any(|c| c.is_ascii_lowercase()))
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
    /// `with_modes`: mode → the modes it resolves through, in order
    chains: HashMap<String, Vec<String>>,
    /// `with_modes`: routes carry the command's name (no [`Client`] mapping)
    named: bool,
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
            chains: HashMap::new(),
            named: false,
        }
    }

    /// A router over every mode of `binds`, for [`Router::feed_in`]:
    /// `chain(mode)` lists the modes a key in `mode` is looked up in, first
    /// wins (the mode itself included where it belongs). The chains are
    /// taken once, for every mode the bindings name. Routes name the command
    /// (`Core` or `Unsupported` by `client_cmds`, never `Client`): the
    /// caller dispatches by name.
    pub fn with_modes(
        binds: &[Bind],
        client_cmds: &HashSet<String>,
        chain: &dyn Fn(&str) -> Vec<String>,
    ) -> Router {
        let mut out = Vec::new();
        let mut chains = HashMap::new();
        for b in binds {
            for k in &b.keys {
                out.push((split_seq(k), b.command.clone(), b.mode.clone()));
            }
            chains
                .entry(b.mode.clone())
                .or_insert_with(|| chain(&b.mode));
        }
        Router {
            binds: out,
            client: client_cmds.clone(),
            buf: Vec::new(),
            chains,
            named: true,
        }
    }

    pub fn pending(&self) -> &[String] {
        &self.buf
    }

    /// Feeds one key typed in `mode`, resolved along that mode's chain (a
    /// mode no binding names resolves in itself only). A sequence in
    /// progress (`Pending`) continues across calls; a key that breaks it is
    /// retried alone.
    pub fn feed_in(&mut self, mode: &str, key: &str) -> Route {
        let chain = self
            .chains
            .get(mode)
            .cloned()
            .unwrap_or_else(|| vec![mode.to_string()]);
        self.buf.push(key.to_string());
        if let Some(r) = self.resolve_in(&chain) {
            return r;
        }
        if self.buf.len() > 1 {
            self.buf = vec![key.to_string()];
            if let Some(r) = self.resolve_in(&chain) {
                return r;
            }
        }
        self.buf.clear();
        Route::None
    }

    /// Drop a sequence in progress (the mode changed under it).
    pub fn reset(&mut self) {
        self.buf.clear();
    }

    fn resolve_in(&mut self, chain: &[String]) -> Option<Route> {
        let rank = |mode: &str| chain.iter().position(|m| m == mode);
        let exact = self
            .binds
            .iter()
            .filter(|(seq, _, _)| *seq == self.buf)
            .filter_map(|(_, c, m)| rank(m).map(|r| (r, c)))
            .min_by_key(|(r, _)| *r)
            .map(|(_, c)| c.clone());
        let longer = self.binds.iter().any(|(seq, _, m)| {
            seq.len() > self.buf.len() && seq.starts_with(&self.buf) && rank(m).is_some()
        });
        if let Some(cmd) = exact {
            if !longer || self.buf.len() > 1 {
                self.buf.clear();
                return Some(self.route_cmd(&cmd));
            }
            return Some(Route::Pending);
        }
        if longer {
            return Some(Route::Pending);
        }
        None
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
        if !self.named
            && let Some(a) = client_action(cmd)
        {
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
        assert_eq!(split_seq("XF86AudioPlay"), vec!["XF86AudioPlay"]);
        assert_eq!(split_seq("Print"), vec!["Print"]);
        assert_eq!(split_seq("GG"), vec!["G", "G"]);
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

    fn bind(mode: &str, keys: &[&str], command: &str) -> Bind {
        Bind {
            command: command.into(),
            keys: keys.iter().map(|k| k.to_string()).collect(),
            mode: mode.into(),
            help: String::new(),
        }
    }

    /// The world's chains (plano-mundo §3.12), as `nkw` passes them.
    fn chain(mode: &str) -> Vec<String> {
        let c: &[&str] = match mode {
            "normal.world" => &["world", "normal.world", "normal", "*"],
            "world.walk" => &["world", "world.walk", "*"],
            "world.focus" => &["world", "world.focus"],
            m => return vec!["world".into(), m.into()],
        };
        c.iter().map(|s| s.to_string()).collect()
    }

    fn world_router() -> Router {
        let b = vec![
            bind("world", &["ctrl+alt+w"], "world.walk"),
            bind("normal.world", &["w"], "world.walk"),
            bind("normal.world", &["h", "left"], "world.left"),
            bind("normal.world", &["gg"], "world.first"),
            bind("normal", &["h"], "nav.collapse"),
            bind("normal", &["x"], "session.stop"),
            bind("normal", &["dd"], "session.remove"),
            bind("*", &["ctrl+k"], "switcher.open"),
            bind("world.walk", &["w"], "world.forward"),
            bind("world.focus", &["ctrl+]"], "world.back"),
            bind("normal.todos", &["D"], "todo.done"),
        ];
        let mut client = HashSet::new();
        for c in [
            "world.walk",
            "world.left",
            "world.first",
            "world.forward",
            "world.back",
        ] {
            client.insert(c.to_string());
        }
        client.insert("nav.collapse".into());
        client.insert("switcher.open".into());
        Router::with_modes(&b, &client, &chain)
    }

    #[test]
    fn feed_in_resolves_along_the_chain() {
        use Route::{Core, Pending, Unsupported as U};
        let mut r = world_router();
        let rows: &[(&str, &str, Route)] = &[
            ("normal.world", "h", U("world.left".into())), // shadows normal's h
            ("normal.world", "left", U("world.left".into())),
            ("normal.world", "x", Core("session.stop".into())), // falls to normal
            ("normal.world", "ctrl+k", U("switcher.open".into())), // then *
            ("normal.world", "ctrl+alt+w", U("world.walk".into())), // world first
            ("normal.world", "D", Route::None),                 // not in the chain
            ("world.walk", "w", U("world.forward".into())),
            ("world.walk", "x", Route::None), // normal is not in ANDAR's chain
            ("world.walk", "ctrl+k", U("switcher.open".into())),
            ("world.focus", "ctrl+alt+w", U("world.walk".into())),
            ("world.focus", "ctrl+]", U("world.back".into())),
            ("world.focus", "h", Route::None), // no fallback in FOCO
            ("world.focus", "ctrl+k", Route::None),
            ("world.nothing", "ctrl+alt+w", Route::None), // unknown mode: itself only
            ("normal.world", "g", Pending),
            ("normal.world", "g", U("world.first".into())),
            ("normal.world", "d", Pending),
            ("normal.world", "d", Core("session.remove".into())),
        ];
        for (mode, key, want) in rows {
            assert_eq!(r.feed_in(mode, key), *want, "{mode} {key}");
        }
    }

    #[test]
    fn feed_in_sequences_break_and_retry() {
        let mut r = world_router();
        assert_eq!(r.feed_in("normal.world", "g"), Route::Pending);
        assert_eq!(r.pending(), ["g"]);
        // a broken sequence retries the key alone
        assert_eq!(
            r.feed_in("normal.world", "h"),
            Route::Unsupported("world.left".into())
        );
        assert!(r.pending().is_empty());
        assert_eq!(r.feed_in("normal.world", "d"), Route::Pending);
        r.reset();
        assert!(r.pending().is_empty());
        assert_eq!(r.feed_in("normal.world", "q"), Route::None);
    }

    #[test]
    fn with_modes_never_maps_to_client_actions() {
        let b = vec![bind("normal", &["h"], "nav.collapse")];
        let mut r = Router::with_modes(&b, &HashSet::new(), &|m| vec![m.to_string()]);
        assert_eq!(
            r.feed_in("normal", "h"),
            Route::Unsupported("nav.collapse".into())
        );
        // and `new`/`feed` (nkg) keep mapping them
        let mut old = Router::new(&b, &HashSet::new());
        assert_eq!(old.feed("h"), Route::Client(Client::Left));
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
