//! The `--demo` office (`--core none`): sessions, tree rows, tails and pane
//! text as the core would send them, so the fixture takes the same path as
//! a real snapshot. Names follow the repo's fixtures only (`acme`, `demo`,
//! `alpha`, `bravo`, `api-gateway`); nothing comes from `$HOME`.
//!
//! `demo`: five sessions in two contexts and two kittens on `acme`.
//! `busy`: three more active sessions, fresh activity on the two that need
//! you and a letter for `bravo` (the postman): five active cats, three
//! walkers.

use nk_client::{Link, Question, QuestionOption, Session, Snapshot, Sub, TailLine, Tree, TreeRow};

/// Which fixture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fixture {
    Demo,
    Busy,
}

impl Fixture {
    pub fn parse(s: &str) -> Option<Fixture> {
        match s {
            "demo" => Some(Fixture::Demo),
            "busy" => Some(Fixture::Busy),
            _ => None,
        }
    }
}

/// The question `api-gateway` waits on, and its options.
pub const QUESTION: &str = "Aplicar a migração agora?";
pub const OPTIONS: [&str; 2] = ["sim, aplicar agora", "não, revisar antes"];

/// The fixed clock of the fixture (activity times are relative to it).
pub const T0: &str = "2026-01-05T10:00:00Z";

fn at(min: u32) -> String {
    format!("2026-01-05T10:{min:02}:00Z")
}

fn sess(id: &str, label: &str, ctx: &str, state: &str, attention: i64, last: u32) -> Session {
    Session {
        session_id: id.into(),
        short_id: id.chars().take(8).collect(),
        label: label.into(),
        name: label.into(),
        context: ctx.into(),
        context_set: true,
        kind: "background".into(),
        state: state.into(),
        attention,
        started: T0.into(),
        last_at: at(last),
        model: "demo".into(),
        ..Session::default()
    }
}

fn kitten(id: &str, kind: &str, desc: &str, text: &str) -> Sub {
    Sub {
        id: id.into(),
        description: desc.into(),
        kind: kind.into(),
        last_text: text.into(),
        last_at: at(9),
        ..Sub::default()
    }
}

/// The session id of a fixture label (`demo-<label>`).
pub fn id_of(label: &str) -> String {
    format!("demo-{label}")
}

/// The sessions of a fixture, in tree order.
pub fn sessions(f: Fixture) -> Vec<Session> {
    let mut acme = sess(&id_of("acme"), "acme", "acme", "busy", 0, 9);
    acme.last_tool = "Edit".into();
    acme.last_text = "Ajustando o handler de pedidos para validar o cabeçalho.".into();
    acme.subs = vec![
        kitten(
            "demo-acme-k1",
            "Explore",
            "procurar os testes do handler",
            "Achei 3 arquivos de teste.",
        ),
        kitten(
            "demo-acme-k2",
            "general-purpose",
            "revisar o diff",
            "Lendo o diff…",
        ),
    ];
    let mut gw = sess(
        &id_of("api-gateway"),
        "api-gateway",
        "acme",
        "blocked",
        1,
        7,
    );
    gw.waiting_tool = "AskUserQuestion".into();
    gw.needs = QUESTION.into();
    gw.questions = vec![Question {
        question: QUESTION.into(),
        options: OPTIONS
            .iter()
            .map(|o| QuestionOption {
                label: (*o).into(),
                description: String::new(),
            })
            .collect(),
    }];
    let mut alpha = sess(&id_of("alpha"), "alpha", "demo", "done", 2, 5);
    alpha.finished = true;
    alpha.result = "Pronto: os testes passam.".into();
    alpha.last_text = alpha.result.clone();
    alpha.links = vec![Link {
        id: "l1".into(),
        href: "https://example.com/acme/pull/1".into(),
        kind: "pr".into(),
        title: "PR #1".into(),
    }];
    let bravo = sess(&id_of("bravo"), "bravo", "demo", "idle", 3, 2);
    let mut demo = sess(&id_of("demo"), "demo", "demo", "stopped", 0, 1);
    demo.ended = true;
    demo.detail = "o processo saiu com código 1".into();
    let mut out = vec![acme, gw, alpha, bravo, demo];
    if f == Fixture::Busy {
        for (label, ctx) in [("acme-2", "acme"), ("alpha-2", "demo"), ("demo-2", "demo")] {
            let mut s = sess(&id_of(label), label, ctx, "busy", 0, 10);
            s.last_tool = "Bash".into();
            s.last_text = "Rodando os testes.".into();
            out.push(s);
        }
        // fresh activity on the two that need you: they get up again
        for s in out
            .iter_mut()
            .filter(|s| s.attention == 1 || s.attention == 2)
        {
            s.last_at = at(11);
        }
    }
    out
}

/// The tree rows of the sessions tab: a `ctx` row per context, then its
/// sessions (and their kittens as `sub` rows), contexts in first-seen order.
pub fn rows(sessions: &[Session]) -> Vec<TreeRow> {
    let mut ctxs: Vec<&str> = Vec::new();
    for s in sessions {
        if !ctxs.contains(&s.context.as_str()) {
            ctxs.push(&s.context);
        }
    }
    let mut rows = Vec::new();
    for (ci, c) in ctxs.iter().enumerate() {
        rows.push(TreeRow {
            key: format!("ctx:{c}"),
            kind: "ctx".into(),
            ctx: (*c).into(),
            color: ci as i64,
            ..TreeRow::default()
        });
        for s in sessions.iter().filter(|s| s.context == *c) {
            rows.push(TreeRow {
                key: format!("sess:{}", s.session_id),
                kind: "sess".into(),
                ctx: (*c).into(),
                depth: 1,
                color: ci as i64,
                session_id: s.session_id.clone(),
                short_id: s.short_id.clone(),
                label: s.label.clone(),
                state: s.state.clone(),
                attention: s.attention,
                ended: s.ended,
                finished: s.finished,
                subs_running: s.subs.iter().filter(|k| !k.done).count() as i64,
                ..TreeRow::default()
            });
            for k in &s.subs {
                rows.push(TreeRow {
                    key: format!("sub:{}", k.id),
                    kind: "sub".into(),
                    ctx: (*c).into(),
                    depth: 2,
                    session_id: s.session_id.clone(),
                    sub: Some(k.clone()),
                    ..TreeRow::default()
                });
            }
        }
    }
    rows
}

/// The snapshot and tree of a fixture.
pub fn snapshot(f: Fixture) -> (Snapshot, Tree) {
    let sessions = sessions(f);
    let tree = Tree {
        rows: rows(&sessions),
        ..Tree::default()
    };
    let mut snap = Snapshot {
        sessions,
        at: T0.into(),
        ..Snapshot::default()
    };
    if f == Fixture::Busy {
        snap.mail.insert(
            id_of("bravo"),
            nk_client::MailCue {
                at: at(11),
                sent: false,
            },
        );
    }
    (snap, tree)
}

/// The postman's letter of the busy fixture: (from, to, text).
pub const LETTER: (&str, &str, &str) =
    ("acme", "bravo", "O handler novo está no ramo acme/pedidos.");

/// A fixture session's activity tail (oldest first).
pub fn tail(label: &str) -> Vec<TailLine> {
    let l = |kind: &str, text: &str| TailLine {
        at: T0.into(),
        kind: kind.into(),
        text: text.into(),
    };
    match label {
        "acme" | "acme-2" => vec![
            l("user", "> valide o cabeçalho X-Pedido"),
            l("text", "Vou olhar o handler primeiro."),
            l("tool", "● Read(src/pedidos/handler.go)"),
            l("result", "  ⎿ 120 linhas"),
            l("tool", "● Edit(src/pedidos/handler.go)"),
            l("result", "+ if r.Header.Get(\"X-Pedido\") == \"\" {"),
            l("result", "+     return errSemPedido"),
            l("result", "+ }"),
            l("sub", "◆ Explore: procurar os testes do handler"),
        ],
        "api-gateway" => vec![
            l("user", "> migre a tabela de rotas"),
            l("tool", "● Bash(make migrate-plan)"),
            l("result", "  ⎿ 2 migrações pendentes"),
            l("text", QUESTION),
        ],
        "alpha" | "alpha-2" => vec![
            l("tool", "● Bash(go test ./...)"),
            l("result", "  ⎿ ok  	acme/pedidos	0.42s"),
            l("text", "Pronto: os testes passam."),
        ],
        "bravo" => vec![l("text", "Oi! Pode mandar.")],
        "demo" | "demo-2" => vec![
            l("tool", "● Bash(./demo.sh)"),
            l("result", "- erro: o processo saiu com código 1"),
        ],
        _ => Vec::new(),
    }
}

/// What a fixture pane shows (no core: a still terminal with the tail and
/// a prompt), as bytes for a `Term`.
pub fn pane_text(label: &str) -> Vec<u8> {
    let mut s = String::new();
    for l in tail(label) {
        s.push_str(&l.text);
        s.push_str("\r\n");
    }
    s.push_str("\x1b[1m❯\x1b[0m ");
    s.into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_demo_fixture() {
        let s = sessions(Fixture::Demo);
        let labels: Vec<_> = s.iter().map(|s| s.label.as_str()).collect();
        assert_eq!(labels, ["acme", "api-gateway", "alpha", "bravo", "demo"]);
        let states: Vec<_> = s.iter().map(|s| s.state.as_str()).collect();
        assert_eq!(states, ["busy", "blocked", "done", "idle", "stopped"]);
        assert_eq!(s[0].last_tool, "Edit");
        assert_eq!(s[0].subs.len(), 2, "two kittens on acme");
        assert_eq!(s[1].questions[0].question, QUESTION);
        let opts: Vec<_> = s[1].questions[0]
            .options
            .iter()
            .map(|o| o.label.as_str())
            .collect();
        assert_eq!(opts, OPTIONS);
        assert_eq!(s[2].links.len(), 1);
        assert!(s[4].ended);
        // ids are unique
        let mut ids: Vec<_> = s.iter().map(|s| &s.session_id).collect();
        ids.dedup();
        assert_eq!(ids.len(), 5);
    }

    #[test]
    fn busy_adds_three_active_and_a_letter() {
        let (snap, tree) = snapshot(Fixture::Busy);
        let busy = snap.sessions.iter().filter(|s| s.state == "busy").count();
        assert_eq!(busy, 4, "acme + 3");
        assert_eq!(snap.sessions.len(), 8);
        assert!(!snap.mail[&id_of(LETTER.1)].sent);
        let sess_rows = tree.rows.iter().filter(|r| r.kind == "sess").count();
        assert_eq!(sess_rows, 8);
        // fresh activity on the two that need you
        let demo = sessions(Fixture::Demo);
        for (a, b) in demo.iter().zip(&snap.sessions) {
            assert_eq!(a.last_at != b.last_at, a.attention == 1 || a.attention == 2);
        }
    }

    #[test]
    fn rows_group_by_context_in_first_seen_order() {
        let rows = rows(&sessions(Fixture::Demo));
        let kinds: Vec<_> = rows
            .iter()
            .map(|r| (r.kind.as_str(), r.ctx.as_str()))
            .collect();
        assert_eq!(
            kinds,
            [
                ("ctx", "acme"),
                ("sess", "acme"),
                ("sub", "acme"),
                ("sub", "acme"),
                ("sess", "acme"),
                ("ctx", "demo"),
                ("sess", "demo"),
                ("sess", "demo"),
                ("sess", "demo"),
            ]
        );
        assert_eq!(rows[6].color, 1);
    }

    #[test]
    fn every_fixture_session_has_a_tail_and_a_pane() {
        for s in sessions(Fixture::Busy) {
            assert!(!tail(&s.label).is_empty(), "{}", s.label);
            assert!(!pane_text(&s.label).is_empty());
        }
        assert_eq!(Fixture::parse("busy"), Some(Fixture::Busy));
        assert_eq!(Fixture::parse("x"), None);
    }
}
