//! DTO decoding against JSON captured from a real `nk serve --demo`
//! (tests/fixtures: `curl -H "Authorization: Bearer $TOKEN"` of /v1/hello,
//! /v1/snapshot, /v1/view/tree, /v1/theme, /v1/keymap, /v1/commands,
//! /v1/sessions/<indexador>/tail?n=10, a session.stop pending and
//! `/v1/events?since=0` for a few seconds).

use nk_client::*;

fn fixture<T: serde::de::DeserializeOwned>(name: &str) -> T {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{name}: {e}"))
}

const GATEWAY: &str = "d3e0a001-0000-4000-8000-000000000001";
const MIGRATION: &str = "d3e0a002-0000-4000-8000-000000000002";
const INDEXER: &str = "d3e0a006-0000-4000-8000-000000000006";

#[test]
fn hello() {
    let h: Hello = fixture("hello.json");
    assert_eq!(h.app, "nekuroddo");
    assert_eq!(h.protocol, 1);
    assert!(h.has("demo") && h.has("panes") && h.has("lua"));
    assert!(!h.has("nope"));
    assert!(h.pid > 0);
}

#[test]
fn snapshot() {
    let s: Snapshot = fixture("snapshot.json");
    assert_eq!(s.sessions.len(), 7);
    let gw = s.session(GATEWAY).expect("api-gateway");
    assert_eq!(gw.short_id, "dmgatewy");
    assert_eq!(gw.label, "api-gateway");
    assert_eq!(gw.context, "visio");
    assert_eq!(gw.kind, "background");
    assert!(gw.cost > 0.0);
    assert!(gw.tail_seq > 0);
    assert!(!gw.last_at.is_empty());

    let mig = s.session(MIGRATION).unwrap();
    assert_eq!(mig.state, "blocked");
    assert_eq!(mig.attention, 1);
    assert_eq!(mig.questions.len(), 1);
    assert_eq!(mig.questions[0].options.len(), 3);
    assert!(!mig.questions[0].options[0].label.is_empty());

    let idx = s.session(INDEXER).unwrap();
    assert_eq!(idx.label, "indexador");
    assert_eq!(idx.subs.len(), 3);
    assert_eq!(idx.subs.iter().filter(|b| !b.done).count(), 2);
    assert!(idx.subs.iter().any(|b| b.kind == "Explore"));

    assert!(s.sessions.iter().any(|x| x.stale), "the old blog is stale");
    assert_eq!(s.inbox.len(), 3);
    let g = &s.groups["deploy-rc"];
    assert_eq!(g.len(), 4);
    assert_eq!(g[0].session_id, GATEWAY);
    assert!(s.usage.limits.iter().any(|l| l.pct > 0));
    assert!(s.todos.groups.is_empty(), "todos.groups is null in the fixture");
    assert_eq!(s.jira.issues.len(), 4);
    assert_eq!(s.jira.issues[0].kind, "Story");
    assert!(s.pending.is_empty());
    assert!(s.today_cost > 0.0);
}

#[test]
fn tree() {
    let t: Tree = fixture("tree.json");
    assert!(!t.rows.is_empty());
    let ctx = &t.rows[0];
    assert_eq!((ctx.kind.as_str(), ctx.depth, ctx.key.as_str()), ("ctx", 0, "ctx:visio"));
    assert!(ctx.ctx_n > 0 && ctx.ctx_cost > 0.0);
    let gw = t.rows.iter().find(|r| r.session_id == GATEWAY && r.kind == "sess").unwrap();
    assert_eq!(gw.depth, 1);
    assert_eq!(gw.groups[0].tag, "deploy-rc");
    let mig = t.rows.iter().find(|r| r.session_id == MIGRATION).unwrap();
    assert_eq!(mig.options, 3);
    let sub = t.rows.iter().find(|r| r.kind == "sub").expect("a subagent row");
    assert_eq!(sub.depth, 2);
    assert!(sub.sub.as_ref().is_some_and(|b| !b.id.is_empty()));
    assert_eq!(t.state.tab, "sessions");
    assert!(!t.state.order.is_empty());
    assert!(!t.state.ctx_color.is_empty());
    assert_eq!(t.inbox.len(), 3);
    assert_eq!(t.hidden, 1);

    // the state goes back as it came (round trip)
    let back: ViewState = serde_json::from_value(serde_json::to_value(&t.state).unwrap()).unwrap();
    assert_eq!(back, t.state);
}

#[test]
fn view_state_serializes_sparse() {
    let v = serde_json::to_value(ViewState::default()).unwrap();
    assert_eq!(v, serde_json::json!({"cursor": 0, "inbox_cursor": 0}));
    // nil maps come back as null
    let v: ViewState = serde_json::from_str(r#"{"order":null,"collapsed":null,"ctx_color":null,"cursor":2}"#).unwrap();
    assert_eq!(v.cursor, 2);
    assert!(v.order.is_empty());
}

#[test]
fn theme() {
    let t: Theme = fixture("theme.json");
    assert_eq!(t.name, "catppuccin-mocha");
    assert_eq!(t.rgb("bg"), Some([0x1e, 0x1e, 0x2e]));
    assert_eq!(t.rgb("nope"), None);
    assert_eq!(t.contexts.len(), 7);
    assert_eq!(t.context_rgb(0), parse_hex(&t.contexts[0]));
    assert_eq!(t.context_rgb(7), t.context_rgb(0), "wraps around");
    assert_eq!(t.context_rgb(-1), t.context_rgb(6));
    assert!(t.themes.contains(&"dracula".to_string()));
    assert_eq!(t.active, "catppuccin-mocha");
    assert_eq!(parse_hex("#zzzzzz"), None);
    assert_eq!(parse_hex("ff0080"), Some([0xff, 0, 0x80]));
}

#[test]
fn keymap() {
    let k: Vec<Binding> = fixture("keymap.json");
    let stop = k.iter().find(|b| b.command == "session.stop").unwrap();
    assert_eq!(stop.keys, vec!["x"]);
    assert_eq!(stop.mode, "normal");
    assert_eq!(stop.help.key, "x");
    assert!(k.iter().any(|b| b.keys.iter().any(|x| x == "shift+space")));
    let modes: std::collections::BTreeSet<_> = k.iter().map(|b| b.mode.as_str()).collect();
    assert!(modes.contains("normal") && modes.contains("*"));
}

#[test]
fn commands() {
    let c: Vec<CommandInfo> = fixture("commands.json");
    let archive = c.iter().find(|x| x.name == "session.archive").unwrap();
    assert_eq!(archive.args[0].name, "context");
    assert_eq!(archive.args[0].kind, "string");
    assert!(archive.confirm);
    assert_eq!(archive.scope, "core");
    // "keys": null and "args": null read as empty
    let stale = c.iter().find(|x| x.name == "sessions.archive_stale").unwrap();
    assert!(stale.keys.is_empty() && stale.args.is_empty());
    assert!(c.iter().any(|x| x.scope == "client"));
}

#[test]
fn tail() {
    #[derive(serde::Deserialize)]
    struct T {
        lines: Vec<TailLine>,
    }
    let t: T = fixture("tail.json");
    assert_eq!(t.lines.len(), 10);
    let kinds: std::collections::BTreeSet<_> = t.lines.iter().map(|l| l.kind.as_str()).collect();
    assert!(kinds.contains("tool") && kinds.contains("result"));
    assert!(t.lines.iter().all(|l| !l.at.is_empty() && !l.text.is_empty()));
}

#[test]
fn results() {
    let r: CmdResult = fixture("run_refresh.json");
    assert_eq!(r, CmdResult::default());
    let r: CmdResult = fixture("run_pending.json");
    let p = r.pending.unwrap();
    assert!(p.question.contains("api-gateway"));
    assert!(!p.token.is_empty() && !p.expires_at.is_empty());
    assert!(r.data.is_null());
    let r: CmdResult =
        serde_json::from_str(r#"{"toast":"ok","err":true,"data":{"close_panes":["a"],"refresh":true}}"#).unwrap();
    assert!(r.err);
    assert_eq!(r.data["close_panes"][0], "a");
    let e: ErrorBody = fixture("error_unknown_token.json");
    assert_eq!(e.code, "unknown_token");
}

#[test]
fn context_serializes_sparse() {
    assert_eq!(serde_json::to_value(Context::default()).unwrap(), serde_json::json!({}));
    let c = Context { picked: vec!["a".into()], todo_id: 3, ..Context::session("s") };
    assert_eq!(serde_json::to_value(c).unwrap(), serde_json::json!({"session_id":"s","picked":["a"],"todo_id":3}));
}

#[test]
fn unknown_fields_are_ignored() {
    let s: Session = serde_json::from_str(r#"{"session_id":"x","brand_new":{"a":1},"subs":null}"#).unwrap();
    assert_eq!(s.session_id, "x");
    assert!(s.subs.is_empty());
}
