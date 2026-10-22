//! CONVERSA (§6.4): what a cat says when you talk to it and what each
//! choice asks of the core. Pure: `opening` is the table, `on_choice` the
//! mapping; the plugin turns `Step`s into `Req`s and core calls.
//!
//! [`Reveal`] is the letter-by-letter text (45 chars/s, all at once under
//! reduced motion); a local stand-in with the API of P3's
//! `nkw_core::dialog::Dialog`, which replaces it once it is on disk.

use nk_client::{Session, Sub};
use nkw_core::Time;

/// Characters per second of the reveal.
pub const CPS: f32 = 45.0;
/// The reveal's frame rate (§3.7: text at 30 Hz).
pub const REVEAL_HZ: f32 = 30.0;
/// Options of a question shown as choices (keys 1–9).
pub const MAX_OPTIONS: usize = 9;
/// How much of `last_text` a busy cat quotes.
pub const SNIPPET: usize = 80;

/// One choice of the box.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Choice {
    /// option N (1-based) of the question
    Option(usize, String),
    /// "Outra resposta…"
    Other,
    /// "Depois"
    Later,
    /// "Ver o terminal"
    ViewTerminal,
    /// "Responder…"
    Reply,
    /// "Mandar mensagem…"
    Message,
    /// "Parar"
    Stop,
    /// "Abrir {title}" (href, title)
    OpenLink(String, String),
    /// "Mandar mais…"
    More,
    /// "Arquivar"
    Archive,
    /// "Remover"
    Remove,
    /// "Voltar" (a kitten: read only)
    Back,
    /// "Ir até {to}" (the postman)
    GoTo(String),
    /// "Ok"
    Ok,
    /// a confirmation: "Sim" / "Não"
    Yes,
    No,
}

impl Choice {
    /// The text on the box (pt-BR).
    pub fn label(&self) -> String {
        match self {
            Choice::Option(_, l) => l.clone(),
            Choice::Other => "Outra resposta…".into(),
            Choice::Later => "Depois".into(),
            Choice::ViewTerminal => "Ver o terminal".into(),
            Choice::Reply => "Responder…".into(),
            Choice::Message => "Mandar mensagem…".into(),
            Choice::Stop => "Parar".into(),
            Choice::OpenLink(_, t) => format!("Abrir {t}"),
            Choice::More => "Mandar mais…".into(),
            Choice::Archive => "Arquivar".into(),
            Choice::Remove => "Remover".into(),
            Choice::Back => "Voltar".into(),
            Choice::GoTo(to) => format!("Ir até {to}"),
            Choice::Ok => "Ok".into(),
            Choice::Yes => "Sim".into(),
            Choice::No => "Não".into(),
        }
    }
}

/// Who is talking.
#[derive(Clone, Debug, PartialEq)]
#[allow(clippy::large_enum_variant)] // one per conversation
pub enum Who {
    Session(Session),
    Kitten(Sub),
    /// from, to, text
    Postman(String, String, String),
    /// the receptionist with a confirmation from outside the conversation
    /// (token, question)
    Confirm(String, String),
    /// the receptionist, nothing to bring
    Receptionist,
    /// a session that left during the conversation (its label)
    Gone(String),
}

/// The §6.4 row a session is in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Row {
    BlockedQuestion,
    BlockedPermission,
    Busy,
    Done,
    Idle,
    Error,
    Kitten,
    Postman,
    Confirm,
    Receptionist,
    Gone,
}

/// The row of a session (the same precedence as its pose).
pub fn row_of(s: &Session) -> Row {
    use nk_cats::model::Pose;
    let pose = nk_cats::model::pose_for(&s.state, s.attention, s.ended, s.finished);
    match pose {
        Pose::Puff => Row::Error,
        Pose::Alert if s.questions.iter().any(|q| !q.options.is_empty()) => Row::BlockedQuestion,
        Pose::Alert => Row::BlockedPermission,
        Pose::Typing => Row::Busy,
        Pose::Sleeping => Row::Done,
        Pose::Idle => Row::Idle,
    }
}

fn snippet(s: &str) -> String {
    let t = s.trim();
    if t.chars().count() <= SNIPPET {
        return t.to_string();
    }
    let mut out: String = t.chars().take(SNIPPET - 1).collect();
    out.push('…');
    out
}

fn join(a: &str, b: &str) -> String {
    match (a.is_empty(), b.is_empty()) {
        (_, true) => a.to_string(),
        (true, false) => b.to_string(),
        _ => format!("{a} {b}"),
    }
}

/// The opening line and the choices (§6.4).
pub fn opening(who: &Who) -> (Row, String, Vec<Choice>) {
    match who {
        Who::Kitten(k) => {
            let kind = if k.kind.is_empty() {
                "filhote"
            } else {
                &k.kind
            };
            let text = join(
                &format!("{kind}: {}", k.description),
                &snippet(&k.last_text),
            );
            (Row::Kitten, text, vec![Choice::Back])
        }
        Who::Postman(from, to, text) => (
            Row::Postman,
            format!("Carta de {from} para {to}: {text}"),
            vec![Choice::GoTo(to.clone()), Choice::Ok],
        ),
        Who::Confirm(_, q) => (Row::Confirm, q.clone(), vec![Choice::Yes, Choice::No]),
        Who::Receptionist => (
            Row::Receptionist,
            "Olá! Os avisos e as confirmações passam por mim.".into(),
            vec![Choice::Ok],
        ),
        Who::Gone(label) => (Row::Gone, format!("{label} foi embora."), Vec::new()),
        Who::Session(s) => {
            let row = row_of(s);
            let (text, choices) = match row {
                Row::BlockedQuestion => {
                    let q = s
                        .questions
                        .iter()
                        .find(|q| !q.options.is_empty())
                        .expect("row_of checked");
                    let mut c: Vec<Choice> = q
                        .options
                        .iter()
                        .take(MAX_OPTIONS)
                        .enumerate()
                        .map(|(i, o)| Choice::Option(i + 1, o.label.clone()))
                        .collect();
                    c.extend([Choice::Other, Choice::Later]);
                    (q.question.clone(), c)
                }
                Row::BlockedPermission => {
                    let what = [&s.waiting_for, &s.needs, &s.waiting_tool]
                        .into_iter()
                        .find(|x| !x.is_empty())
                        .cloned()
                        .unwrap_or_else(|| "uma resposta".into());
                    (
                        format!("Preciso de você: {what}"),
                        vec![Choice::ViewTerminal, Choice::Reply, Choice::Later],
                    )
                }
                Row::Busy => {
                    let tool = if s.last_tool.is_empty() {
                        "trabalho"
                    } else {
                        &s.last_tool
                    };
                    (
                        join(&format!("Estou em {tool}…"), &snippet(&s.last_text)),
                        vec![Choice::Message, Choice::Stop, Choice::ViewTerminal],
                    )
                }
                Row::Done => {
                    let r = if s.result.is_empty() {
                        &s.last_text
                    } else {
                        &s.result
                    };
                    let mut c: Vec<Choice> = s
                        .links
                        .iter()
                        .filter(|l| !l.href.is_empty())
                        .map(|l| {
                            let t = if l.title.is_empty() {
                                &l.href
                            } else {
                                &l.title
                            };
                            Choice::OpenLink(l.href.clone(), t.clone())
                        })
                        .collect();
                    c.extend([Choice::More, Choice::Archive]);
                    (join("Terminei!", &snippet(r)), c)
                }
                Row::Idle => (
                    "Oi! Pode mandar.".into(),
                    vec![Choice::Message, Choice::ViewTerminal],
                ),
                _ => {
                    let d = if s.detail.is_empty() {
                        "algo deu errado"
                    } else {
                        &s.detail
                    };
                    (
                        format!("Ops… {d}"),
                        vec![Choice::ViewTerminal, Choice::Remove],
                    )
                }
            };
            (row, text, choices)
        }
    }
}

/// A call to the core.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Call {
    /// `POST /v1/panes {session_id, cols, rows, reply}` (`open_pane_reply`);
    /// a numeric reply is resolved by the server (`core.ResolveOption`)
    Reply { session: String, text: String },
    /// `POST /v1/commands/{name}` with the session as context
    Run { name: String, session: String },
    /// `POST /v1/confirm/{token}`
    Confirm { token: String, yes: bool },
}

/// What a choice does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Step {
    /// call the core, then the cat goes back to work
    Core(Call),
    /// call the core and wait for its answer (a confirmation may follow)
    Ask(Call),
    /// open the reply line (`Action::Reply`)
    Line,
    /// FOCO on the cat's terminal (`Action::Sit`)
    Sit,
    /// open a link with the world's display, then end
    Open(String),
    /// glide to a session's desk (the postman's recipient), then end
    GoTo(String),
    /// end the conversation (`Action::Back`)
    End,
}

/// What choosing `c` in a conversation with `who` does. `pending`: the
/// token of the confirmation the box is asking about.
pub fn on_choice(who: &Who, c: &Choice, pending: Option<&str>) -> Step {
    let session = match who {
        Who::Session(s) => s.session_id.clone(),
        _ => String::new(),
    };
    let pending = match who {
        Who::Confirm(t, _) => Some(t.as_str()),
        _ => pending,
    };
    let run = |name: &str| Call::Run {
        name: name.into(),
        session: session.clone(),
    };
    match c {
        Choice::Option(n, _) => Step::Core(Call::Reply {
            session,
            text: n.to_string(),
        }),
        Choice::Other | Choice::Reply | Choice::Message | Choice::More => Step::Line,
        Choice::ViewTerminal => Step::Sit,
        Choice::Stop => Step::Ask(run("session.stop")),
        Choice::Archive => Step::Core(run("session.archive")),
        Choice::Remove => Step::Ask(run("session.remove")),
        Choice::OpenLink(href, _) => Step::Open(href.clone()),
        Choice::GoTo(to) => Step::GoTo(to.clone()),
        Choice::Yes | Choice::No => match pending {
            Some(t) => Step::Core(Call::Confirm {
                token: t.into(),
                yes: *c == Choice::Yes,
            }),
            None => Step::End,
        },
        Choice::Later | Choice::Back | Choice::Ok => Step::End,
    }
}

/// What the reply line sends: the text typed, to the session's pane.
pub fn on_line(who: &Who, text: &str) -> Option<Call> {
    match who {
        Who::Session(s) if !text.trim().is_empty() => Some(Call::Reply {
            session: s.session_id.clone(),
            text: text.to_string(),
        }),
        _ => None,
    }
}

/// The question of a confirmation (`session.stop` → "Parar {label}?").
pub fn confirm_text(question: &str, call: &Call, label: &str) -> String {
    if !question.is_empty() {
        return question.to_string();
    }
    match call {
        Call::Run { name, .. } if name == "session.stop" => format!("Parar {label}?"),
        Call::Run { name, .. } if name == "session.remove" => format!("Remover {label}?"),
        _ => "Tem certeza?".into(),
    }
}

/// Letter-by-letter text with choices (the API of P3's `Dialog`).
#[derive(Clone, Debug, PartialEq)]
pub struct Reveal {
    text: String,
    start: Time,
    all: bool,
    choices: Vec<String>,
    selected: usize,
}

impl Reveal {
    pub fn new(text: &str, choices: Vec<String>, now: Time, reduced_motion: bool) -> Reveal {
        Reveal {
            text: text.to_string(),
            start: now,
            all: reduced_motion,
            choices,
            selected: 0,
        }
    }

    fn count(&self, now: Time) -> usize {
        let n = self.text.chars().count();
        if self.all {
            return n;
        }
        (((now - self.start).max(0.0) * CPS as f64).floor() as usize).min(n)
    }

    /// The text revealed by `now`.
    pub fn shown(&self, now: Time) -> &str {
        let n = self.count(now);
        match self.text.char_indices().nth(n) {
            Some((i, _)) => &self.text[..i],
            None => &self.text,
        }
    }

    pub fn done(&self, now: Time) -> bool {
        self.count(now) >= self.text.chars().count()
    }

    pub fn reveal_all(&mut self) {
        self.all = true;
    }

    /// The next frame the text needs (30 Hz while revealing).
    pub fn next_wake(&self, now: Time) -> Option<Time> {
        (!self.done(now)).then(|| now + 1.0 / REVEAL_HZ as f64)
    }

    pub fn select(&mut self, i: usize) {
        if i < self.choices.len() {
            self.selected = i;
        }
    }

    pub fn down(&mut self) {
        if !self.choices.is_empty() {
            self.selected = (self.selected + 1) % self.choices.len();
        }
    }

    pub fn up(&mut self) {
        if !self.choices.is_empty() {
            let n = self.choices.len();
            self.selected = (self.selected + n - 1) % n;
        }
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    pub fn choices(&self) -> &[String] {
        &self.choices
    }

    pub fn text(&self) -> &str {
        &self.text
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cats::demo::{self, Fixture};

    fn sessions() -> Vec<Session> {
        demo::sessions(Fixture::Demo)
    }

    fn labels(c: &[Choice]) -> Vec<String> {
        c.iter().map(Choice::label).collect()
    }

    #[test]
    fn the_opening_table_for_every_row() {
        let s = sessions();
        let mut perm = s[1].clone();
        perm.questions.clear();
        perm.waiting_for = "permissão para rodar make".into();
        let kitten = s[0].subs[0].clone();
        let rows: Vec<(Who, Row, &str, Vec<&str>)> = vec![
            (
                Who::Session(s[1].clone()),
                Row::BlockedQuestion,
                demo::QUESTION,
                vec![
                    demo::OPTIONS[0],
                    demo::OPTIONS[1],
                    "Outra resposta…",
                    "Depois",
                ],
            ),
            (
                Who::Session(perm),
                Row::BlockedPermission,
                "Preciso de você: permissão para rodar make",
                vec!["Ver o terminal", "Responder…", "Depois"],
            ),
            (
                Who::Session(s[0].clone()),
                Row::Busy,
                "Estou em Edit… Ajustando o handler de pedidos para validar o cabeçalho.",
                vec!["Mandar mensagem…", "Parar", "Ver o terminal"],
            ),
            (
                Who::Session(s[2].clone()),
                Row::Done,
                "Terminei! Pronto: os testes passam.",
                vec!["Abrir PR #1", "Mandar mais…", "Arquivar"],
            ),
            (
                Who::Session(s[3].clone()),
                Row::Idle,
                "Oi! Pode mandar.",
                vec!["Mandar mensagem…", "Ver o terminal"],
            ),
            (
                Who::Session(s[4].clone()),
                Row::Error,
                "Ops… o processo saiu com código 1",
                vec!["Ver o terminal", "Remover"],
            ),
            (
                Who::Kitten(kitten),
                Row::Kitten,
                "Explore: procurar os testes do handler Achei 3 arquivos de teste.",
                vec!["Voltar"],
            ),
            (
                Who::Postman("acme".into(), "bravo".into(), "oi".into()),
                Row::Postman,
                "Carta de acme para bravo: oi",
                vec!["Ir até bravo", "Ok"],
            ),
            (
                Who::Confirm("t9".into(), "Arquivar 3 sessões?".into()),
                Row::Confirm,
                "Arquivar 3 sessões?",
                vec!["Sim", "Não"],
            ),
            (
                Who::Receptionist,
                Row::Receptionist,
                "Olá! Os avisos e as confirmações passam por mim.",
                vec!["Ok"],
            ),
            (
                Who::Gone("bravo".into()),
                Row::Gone,
                "bravo foi embora.",
                vec![],
            ),
        ];
        for (who, row, text, choices) in rows {
            let (r, t, c) = opening(&who);
            assert_eq!(r, row);
            assert_eq!(t, text, "{row:?}");
            assert_eq!(labels(&c), choices, "{row:?}");
        }
    }

    #[test]
    fn fallbacks_and_limits() {
        let mut s = sessions()[0].clone();
        s.last_tool.clear();
        s.last_text = "x".repeat(200);
        let (_, t, _) = opening(&Who::Session(s));
        assert!(t.starts_with("Estou em trabalho… "));
        assert_eq!(
            t.chars().count(),
            "Estou em trabalho… ".chars().count() + SNIPPET
        );
        let mut q = sessions()[1].clone();
        q.questions[0].options = (0..12)
            .map(|i| nk_client::QuestionOption {
                label: format!("o{i}"),
                description: String::new(),
            })
            .collect();
        let (_, _, c) = opening(&Who::Session(q));
        assert_eq!(c.len(), MAX_OPTIONS + 2);
        let mut e = sessions()[4].clone();
        e.detail.clear();
        assert_eq!(opening(&Who::Session(e)).1, "Ops… algo deu errado");
        let mut p = sessions()[1].clone();
        p.questions.clear();
        p.needs.clear();
        p.waiting_tool.clear();
        assert_eq!(opening(&Who::Session(p)).1, "Preciso de você: uma resposta");
    }

    #[test]
    fn choices_map_to_core_calls() {
        let s = sessions();
        let gw = Who::Session(s[1].clone());
        let id = s[1].session_id.clone();
        let rows: Vec<(Choice, Option<&str>, Step)> = vec![
            (
                Choice::Option(1, demo::OPTIONS[0].into()),
                None,
                Step::Core(Call::Reply {
                    session: id.clone(),
                    text: "1".into(),
                }),
            ),
            (Choice::Other, None, Step::Line),
            (Choice::Reply, None, Step::Line),
            (Choice::Message, None, Step::Line),
            (Choice::More, None, Step::Line),
            (Choice::ViewTerminal, None, Step::Sit),
            (
                Choice::Stop,
                None,
                Step::Ask(Call::Run {
                    name: "session.stop".into(),
                    session: id.clone(),
                }),
            ),
            (
                Choice::Archive,
                None,
                Step::Core(Call::Run {
                    name: "session.archive".into(),
                    session: id.clone(),
                }),
            ),
            (
                Choice::Remove,
                None,
                Step::Ask(Call::Run {
                    name: "session.remove".into(),
                    session: id.clone(),
                }),
            ),
            (
                Choice::OpenLink("https://example.com".into(), "x".into()),
                None,
                Step::Open("https://example.com".into()),
            ),
            (
                Choice::GoTo("bravo".into()),
                None,
                Step::GoTo("bravo".into()),
            ),
            (
                Choice::Yes,
                Some("t1"),
                Step::Core(Call::Confirm {
                    token: "t1".into(),
                    yes: true,
                }),
            ),
            (
                Choice::No,
                Some("t1"),
                Step::Core(Call::Confirm {
                    token: "t1".into(),
                    yes: false,
                }),
            ),
            (Choice::Yes, None, Step::End),
            (Choice::Later, None, Step::End),
            (Choice::Back, None, Step::End),
            (Choice::Ok, None, Step::End),
        ];
        for (c, pending, want) in rows {
            assert_eq!(on_choice(&gw, &c, pending), want, "{c:?}");
        }
        assert_eq!(
            on_line(&gw, "pode"),
            Some(Call::Reply {
                session: id,
                text: "pode".into()
            })
        );
        assert_eq!(on_line(&gw, "  "), None);
        assert_eq!(
            on_line(&Who::Postman("a".into(), "b".into(), "c".into()), "x"),
            None
        );
        // the receptionist's confirmation carries its own token
        let c = Who::Confirm("t9".into(), "?".into());
        assert_eq!(
            on_choice(&c, &Choice::No, None),
            Step::Core(Call::Confirm {
                token: "t9".into(),
                yes: false
            })
        );
    }

    #[test]
    fn confirmation_text() {
        let stop = Call::Run {
            name: "session.stop".into(),
            session: "s".into(),
        };
        assert_eq!(confirm_text("", &stop, "acme"), "Parar acme?");
        assert_eq!(confirm_text("Parar mesmo?", &stop, "acme"), "Parar mesmo?");
        let rm = Call::Run {
            name: "session.remove".into(),
            session: "s".into(),
        };
        assert_eq!(confirm_text("", &rm, "demo"), "Remover demo?");
    }

    #[test]
    fn reveal_by_time() {
        let mut r = Reveal::new(
            "Olá, ação!",
            vec!["a".into(), "b".into(), "c".into()],
            10.0,
            false,
        );
        assert_eq!(r.shown(10.0), "");
        assert_eq!(r.shown(10.0 + 4.0 / 45.0 + 1e-6), "Olá,");
        assert!(!r.done(10.1));
        assert_eq!(r.next_wake(10.1), Some(10.1 + 1.0 / 30.0));
        assert_eq!(r.shown(100.0), "Olá, ação!");
        assert!(r.done(100.0) && r.next_wake(100.0).is_none());
        r.reveal_all();
        assert!(r.done(10.0));
        let still = Reveal::new("abc", vec![], 0.0, true);
        assert_eq!(still.shown(0.0), "abc", "reduced motion: all at once");
        r.down();
        r.down();
        assert_eq!(r.selected(), 2);
        r.down();
        assert_eq!(r.selected(), 0, "wraps");
        r.up();
        assert_eq!(r.selected(), 2);
        r.select(9);
        assert_eq!(r.selected(), 2, "out of range ignored");
        r.select(1);
        assert_eq!(r.selected(), 1);
        assert_eq!(r.choices().len(), 3);
        let mut none = Reveal::new("x", vec![], 0.0, false);
        none.down();
        none.up();
        assert_eq!(none.selected(), 0);
    }
}
