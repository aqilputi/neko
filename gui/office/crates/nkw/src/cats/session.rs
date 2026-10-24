//! The core's session keys in NAV (plan §3.4) and the cats' commands of the
//! phone (S24): `session.reply|stop|rename|move|archive|remove|new` on the
//! cat they are about, `nav.next_attention|prev_attention`, `inbox.summon`,
//! `help.open`, `switcher.open`, `world.goto_cat`, `world.focus_cat` and
//! `world.talk {cat}`.
//!
//! Which cat: an explicit `cat` argument (the phone's acts carry it), the
//! cat you talk to, the cat the cats last handed to the palette (a command
//! with a missing argument comes back from COMANDOS without it), the
//! phone's sub-page cat while it is fresh, else the station's (a desk, or a
//! `Free` pose standing at a cat's desk — where `world.goto_cat` leaves you).
//!
//! [`plan`] is the pure table; [`Cats::session_command`] the glue.

use std::collections::BTreeMap;

use nkw_core::{Action, CatId, Pose, Station, TalkPurpose, Time, layout};

use super::Cats;
use super::office::RECEPTIONIST;
use super::talk::Call;
use crate::ext::{ArgKind, ArgSpec, Ctx, PaletteItem, Req, Source, ToastKind};

/// No cat at the station (the core's 409, plan §3.4).
pub const NO_CAT: &str = "nenhum gato nesta mesa";
/// No cat waiting (`a`/`A` with an empty inbox).
pub const NO_ATTENTION: &str = "ninguém te esperando";
/// The argument that names the cat a command is about (never sent to the
/// core).
pub const CAT_ARG: &str = "cat";
/// The palette's "run `name` asking its arguments" (W2's overlay).
pub const PALETTE_RUN: &str = "world.palette_run";
/// How long the phone's sub-page cat (or the cat handed to the palette)
/// stays the target of an argument-less session command.
pub const HANDOFF_S: Time = 60.0;
/// A walker is framed this far before its face (`world.goto_cat`).
pub const WALKER_GOTO_M: f32 = 2.0;

/// The session commands: (name, required arguments, needs a cat).
pub const SESSION: &[(&str, &[&str], bool)] = &[
    ("session.reply", &["text"], true),
    ("session.stop", &[], true),
    ("session.rename", &["name"], true),
    ("session.move", &["context"], true),
    ("session.archive", &[], true),
    ("session.remove", &[], true),
    ("session.new", &["dir", "prompt"], false),
];

/// The prompts COMANDOS asks for each argument (the core's own, `nome` and
/// `contexto` asked although the core takes them empty).
pub const PROMPTS: &[(&str, &str)] = &[
    ("text", "resposta › "),
    ("name", "nome"),
    ("context", "contexto"),
    ("dir", "diretório › "),
    ("prompt", "prompt › "),
];

/// The cats' rows of COMANDOS: the session commands (with the arguments it
/// asks for), the attention jumps, the inbox and the tour.
pub fn palette_items() -> Vec<PaletteItem> {
    let keys_of = |name: &str| {
        super::demo::KEYMAP
            .iter()
            .filter(|(_, c, _)| *c == name)
            .map(|(k, _, _)| k.to_string())
            .collect::<Vec<_>>()
    };
    let title_of = |name: &str| {
        super::demo::KEYMAP
            .iter()
            .find(|(_, c, _)| *c == name)
            .map_or(name.to_string(), |(_, _, t)| t.to_string())
    };
    let mut v: Vec<PaletteItem> = SESSION
        .iter()
        .map(|(name, required, _)| PaletteItem {
            name: name.to_string(),
            title: title_of(name),
            keys: keys_of(name),
            source: Source::Core,
            args: required
                .iter()
                .map(|a| ArgSpec {
                    name: a.to_string(),
                    prompt: PROMPTS
                        .iter()
                        .find(|(k, _)| k == a)
                        .map_or(format!("{a} › "), |(_, p)| p.to_string()),
                    kind: ArgKind::Text,
                    required: true,
                })
                .collect(),
        })
        .collect();
    for name in ["nav.next_attention", "nav.prev_attention", "inbox.summon"] {
        v.push(PaletteItem {
            name: name.into(),
            title: title_of(name),
            keys: keys_of(name),
            source: Source::Core,
            args: Vec::new(),
        });
    }
    v.push(PaletteItem {
        name: "world.tour".into(),
        title: "tour da recepcionista".into(),
        keys: Vec::new(),
        source: Source::World,
        args: Vec::new(),
    });
    v
}

/// What a session command does once its cat is known.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Plan {
    /// `core.run(name, args)` with the cat's session as the context (none
    /// for `session.new`)
    Run {
        name: String,
        args: BTreeMap<String, String>,
        needs_cat: bool,
    },
    /// a required argument is missing: COMANDOS asks for it
    /// (`world.palette_run {name}`)
    Ask { name: String, needs_cat: bool },
    /// `session.reply` without text: CONVERSA already on the reply line
    /// (plan §3.4: "abre a CONVERSA já na linha de texto")
    TalkLine,
    /// `session.reply {text}`: typed into the cat's pane
    Reply(String),
}

/// The plan of a session command; `None`: not a session command. The
/// `cat` argument is dropped (it is ours, not the core's).
pub fn plan(name: &str, args: &BTreeMap<String, String>) -> Option<Plan> {
    let (_, required, needs_cat) = SESSION.iter().find(|(n, _, _)| *n == name)?;
    let given = |k: &str| args.get(k).is_some_and(|v| !v.trim().is_empty());
    if name == "session.reply" {
        return Some(match args.get("text").filter(|t| !t.trim().is_empty()) {
            Some(t) => Plan::Reply(t.clone()),
            None => Plan::TalkLine,
        });
    }
    if !required.iter().all(|k| given(k)) {
        return Some(Plan::Ask {
            name: name.to_string(),
            needs_cat: *needs_cat,
        });
    }
    let args = args
        .iter()
        .filter(|(k, _)| k.as_str() != CAT_ARG)
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    Some(Plan::Run {
        name: name.to_string(),
        args,
        needs_cat: *needs_cat,
    })
}

/// The next cat that waits for you after `from` (`dir` = +1 / −1), in the
/// office's order, wrapping; `from` itself only when it is the only one.
pub fn next_waiting(
    waiting: &[CatId],
    from: Option<CatId>,
    all: &[CatId],
    dir: i32,
) -> Option<CatId> {
    if waiting.is_empty() {
        return None;
    }
    let n = all.len() as i32;
    let start = from.and_then(|c| all.iter().position(|x| *x == c));
    let Some(s) = start else {
        return match dir >= 0 {
            true => waiting.first().copied(),
            false => waiting.last().copied(),
        };
    };
    (1..=n)
        .map(|i| all[((s as i32 + dir * i).rem_euclid(n)) as usize])
        .find(|c| waiting.contains(c))
}

/// Whether two poses are the same spot (a `Free` station at a desk).
fn same_spot(a: &Pose, b: &Pose) -> bool {
    a.pos.distance(b.pos) < 0.05 && (a.yaw - b.yaw).abs() < 0.05
}

impl Cats {
    /// The cat at the station: a desk's, or the desk a `Free` pose stands
    /// at (`world.goto_cat`, a glide to a desk).
    pub(super) fn station_or_spot_cat(&self, ctx: &Ctx) -> Option<CatId> {
        if let Some(c) = self.station_cat(ctx) {
            return Some(c);
        }
        let Station::Free(p) = ctx.world.station else {
            return None;
        };
        ctx.world.rooms.iter().find_map(|r| {
            r.desks.iter().enumerate().find_map(|(d, desk)| {
                let cat = desk.cat?;
                let at = ctx.world.desk_pose(r.id, d)?;
                same_spot(&at, &p).then_some(cat)
            })
        })
    }

    /// The cat a command is about (see the module doc).
    pub(super) fn target_cat(
        &mut self,
        args: &BTreeMap<String, String>,
        ctx: &Ctx,
    ) -> Option<CatId> {
        if let Some(c) = args.get(CAT_ARG) {
            return c.parse::<u32>().ok().map(CatId).filter(|c| {
                self.office.get(*c).is_some() || self.walks.out(*c) || *c == RECEPTIONIST
            });
        }
        if let Some(t) = &self.talk {
            return Some(t.cat);
        }
        let fresh = |h: Option<(CatId, Time)>| h.filter(|(_, at)| ctx.now - at <= HANDOFF_S);
        if let Some((c, _)) = fresh(self.handoff.take()) {
            return Some(c);
        }
        if let Some((c, _)) = fresh(self.phone_cat.take()) {
            return Some(c);
        }
        self.station_or_spot_cat(ctx)
    }

    /// The session commands and the cats' phone/NAV commands; `None`: not
    /// ours.
    pub(super) fn session_command(
        &mut self,
        name: &str,
        args: &BTreeMap<String, String>,
        ctx: &Ctx,
    ) -> Option<Vec<Req>> {
        if let Some(p) = plan(name, args) {
            return Some(self.run_plan(p, args, ctx));
        }
        Some(match name {
            "world.goto_cat" => {
                let Some(cat) = self.target_cat(args, ctx) else {
                    return Some(info(NO_CAT));
                };
                self.goto_cat(cat, ctx)
            }
            "world.focus_cat" => {
                let Some(cat) = self.target_cat(args, ctx) else {
                    return Some(info(NO_CAT));
                };
                let mut out = leave_palette(ctx);
                out.push(Req::Action(Action::Focus(nkw_core::Target::CatTerm(cat))));
                out
            }
            "world.talk" if args.contains_key(CAT_ARG) => {
                let Some(cat) = self.target_cat(args, ctx) else {
                    return Some(info(NO_CAT));
                };
                let mut out = leave_palette(ctx);
                out.push(Req::Action(Action::Talk {
                    cat,
                    purpose: TalkPurpose::Session,
                }));
                out
            }
            "nav.next_attention" | "nav.prev_attention" => {
                let dir = if name == "nav.next_attention" { 1 } else { -1 };
                let all: Vec<CatId> = self.office.cats.iter().map(|v| v.cat).collect();
                let waiting: Vec<CatId> = self
                    .office
                    .cats
                    .iter()
                    .filter(|v| v.sess.attention != 0)
                    .map(|v| v.cat)
                    .collect();
                let from = self.station_or_spot_cat(ctx);
                match next_waiting(&waiting, from, &all, dir) {
                    Some(cat) => self.goto_cat(cat, ctx),
                    None => info(NO_ATTENTION),
                }
            }
            "inbox.summon" => return self.talk_command("world.summon", &BTreeMap::new(), ctx),
            "help.open" => vec![command("world.help", &[])],
            "switcher.open" => vec![command("world.phone", &[("app", "gatos")])],
            _ => return None,
        })
    }

    fn run_plan(&mut self, p: Plan, args: &BTreeMap<String, String>, ctx: &Ctx) -> Vec<Req> {
        let needs = match &p {
            Plan::Run { needs_cat, .. } | Plan::Ask { needs_cat, .. } => *needs_cat,
            Plan::TalkLine | Plan::Reply(_) => true,
        };
        // a session's cat (the receptionist and the librarian have none)
        let cat = match needs {
            true => match self.target_cat(args, ctx) {
                Some(c) if self.office.get(c).is_some() => Some(c),
                _ => return info(NO_CAT),
            },
            false => None,
        };
        let session = cat
            .and_then(|c| self.office.get(c))
            .map(|v| v.sess.session_id.clone())
            .unwrap_or_default();
        match p {
            Plan::Run { name, args, .. } => {
                self.core_call(
                    Call::RunWith {
                        name,
                        session,
                        args,
                    },
                    ctx,
                );
                Vec::new()
            }
            Plan::Ask { name, .. } => {
                let mut a = vec![("name", name.as_str())];
                let id = cat.map(|c| c.0.to_string());
                if let (Some(c), Some(id)) = (cat, id.as_deref()) {
                    self.handoff = Some((c, ctx.now));
                    a.push((CAT_ARG, id));
                }
                vec![command(PALETTE_RUN, &a)]
            }
            Plan::Reply(text) => {
                self.core_call(Call::Reply { session, text }, ctx);
                Vec::new()
            }
            Plan::TalkLine => {
                let Some(cat) = cat else {
                    return info(NO_CAT);
                };
                let mut out = Vec::new();
                if self.talk.as_ref().is_none_or(|t| t.cat != cat) {
                    // out of COMANDOS, FOCO or another talk first
                    if ctx.world.mode().kind().is_some() {
                        out.push(Req::Action(Action::Back));
                    }
                    out.push(Req::Action(Action::Talk {
                        cat,
                        purpose: TalkPurpose::Session,
                    }));
                }
                out.push(Req::Action(Action::Reply));
                out
            }
        }
    }

    /// `world.goto_cat`: to its desk (the station then knows its cat), or
    /// `WALKER_GOTO_M` before a cat that is out walking.
    fn goto_cat(&self, cat: CatId, ctx: &Ctx) -> Vec<Req> {
        let mut out = leave_palette(ctx);
        if let Some((face, yaw)) = self.walks.face(cat, ctx.now) {
            let level = layout::stand_before(face, yaw, WALKER_GOTO_M);
            let pitch = ((face.y - level.pos.y) / WALKER_GOTO_M).atan();
            out.push(Req::GlideTo(Pose::new(level.pos, yaw, pitch)));
            return out;
        }
        match self
            .desk_of(ctx, cat)
            .and_then(|(room, d)| ctx.world.desk_pose(room, d))
        {
            Some(p) => out.push(Req::GlideTo(p)),
            None => return info(NO_CAT),
        }
        out
    }
}

/// COMANDOS (the phone rides it) is closed first: the machine takes no
/// talk, focus or glide over it, and its own `Back` glides home.
fn leave_palette(ctx: &Ctx) -> Vec<Req> {
    match ctx.world.mode().label() == "COMANDOS" {
        true => vec![Req::Action(Action::Back)],
        false => Vec::new(),
    }
}

fn command(name: &str, args: &[(&str, &str)]) -> Req {
    Req::Command {
        name: name.to_string(),
        args: args
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
    }
}

fn info(text: &str) -> Vec<Req> {
    vec![Req::Toast {
        text: text.to_string(),
        kind: ToastKind::Info,
        ms: 3000,
    }]
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn args(kv: &[(&str, &str)]) -> BTreeMap<String, String> {
        kv.iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn the_session_table() {
        let run = |n: &str, a: &[(&str, &str)], cat: bool| Plan::Run {
            name: n.into(),
            args: args(a),
            needs_cat: cat,
        };
        let ask = |n: &str, cat: bool| Plan::Ask {
            name: n.into(),
            needs_cat: cat,
        };
        let rows: Vec<(&str, BTreeMap<String, String>, Option<Plan>)> = vec![
            (
                "session.stop",
                args(&[]),
                Some(run("session.stop", &[], true)),
            ),
            (
                "session.stop",
                args(&[("cat", "3")]),
                Some(run("session.stop", &[], true)),
            ),
            (
                "session.archive",
                args(&[]),
                Some(run("session.archive", &[], true)),
            ),
            (
                "session.remove",
                args(&[]),
                Some(run("session.remove", &[], true)),
            ),
            (
                "session.rename",
                args(&[]),
                Some(ask("session.rename", true)),
            ),
            (
                "session.rename",
                args(&[("name", "  ")]),
                Some(ask("session.rename", true)),
            ),
            (
                "session.rename",
                args(&[("name", "bravo"), ("cat", "2")]),
                Some(run("session.rename", &[("name", "bravo")], true)),
            ),
            ("session.move", args(&[]), Some(ask("session.move", true))),
            (
                "session.move",
                args(&[("context", "demo")]),
                Some(run("session.move", &[("context", "demo")], true)),
            ),
            (
                "session.new",
                args(&[("dir", "/tmp")]),
                Some(ask("session.new", false)),
            ),
            (
                "session.new",
                args(&[("dir", "projeto"), ("prompt", "oi")]),
                Some(run(
                    "session.new",
                    &[("dir", "projeto"), ("prompt", "oi")],
                    false,
                )),
            ),
            ("session.reply", args(&[]), Some(Plan::TalkLine)),
            (
                "session.reply",
                args(&[("text", "2")]),
                Some(Plan::Reply("2".into())),
            ),
            ("session.open", args(&[]), None),
            ("world.talk", args(&[]), None),
        ];
        for (name, a, want) in rows {
            assert_eq!(plan(name, &a), want, "{name} {a:?}");
        }
    }

    /// Every session command with the cat (`--core none`: what `calls`
    /// records), from the reception (no cat at the station).
    #[test]
    fn commands_to_core_calls_with_their_cat() {
        use crate::cats::tests::{fixture_world, with_ctx};
        let (w, mut cats) = fixture_world();
        let rooms = w.visible_rooms(&[]);
        let mut cmd = |name: &str, a: &[(&str, &str)]| {
            with_ctx(&w, &rooms, 1.0, |ctx| {
                crate::ext::Plugin::command(&mut cats, name, &args(a), ctx).unwrap_or_default()
            })
        };
        // the reception's station: the receptionist, not a villager
        assert_eq!(cmd("session.stop", &[]), info(NO_CAT));
        assert_eq!(cmd("session.stop", &[("cat", "3")]), []);
        assert_eq!(cmd("session.rename", &[("cat", "5"), ("name", "zeta")]), []);
        assert_eq!(
            cmd("session.new", &[("dir", "projeto"), ("prompt", "oi")]),
            []
        );
        assert_eq!(
            cmd("session.move", &[("cat", "4")]),
            [command(
                PALETTE_RUN,
                &[("name", "session.move"), ("cat", "4")]
            )]
        );
        // what COMANDOS sends back without the cat lands on the one handed over
        assert_eq!(cmd("session.move", &[("context", "acme")]), []);
        assert_eq!(
            cmd("session.move", &[("context", "acme")]),
            info(NO_CAT),
            "once"
        );
        assert_eq!(cmd("help.open", &[]), [command("world.help", &[])]);
        assert_eq!(
            cmd("switcher.open", &[]),
            [command("world.phone", &[("app", "gatos")])]
        );
        assert_eq!(
            cmd("session.open", &[]),
            Vec::<Req>::new(),
            "not ours: talk's None"
        );
        let got: Vec<(String, String, Value)> = cats
            .calls
            .iter()
            .map(|c| {
                (
                    c["name"].as_str().unwrap().to_string(),
                    c["session"].as_str().unwrap().to_string(),
                    c["args"].clone(),
                )
            })
            .collect();
        let j = |kv: &[(&str, &str)]| serde_json::to_value(args(kv)).unwrap();
        assert_eq!(
            got,
            [
                ("session.stop".into(), "demo-api-gateway".into(), j(&[])),
                (
                    "session.rename".into(),
                    "demo-bravo".into(),
                    j(&[("name", "zeta")])
                ),
                (
                    "session.new".into(),
                    String::new(),
                    j(&[("dir", "projeto"), ("prompt", "oi")])
                ),
                (
                    "session.move".into(),
                    "demo-alpha".into(),
                    j(&[("context", "acme")])
                ),
            ]
        );
    }

    #[test]
    fn the_palette_asks_what_the_table_requires() {
        let items = palette_items();
        for (name, required, _) in SESSION {
            let it = items.iter().find(|i| i.name == *name).unwrap();
            assert_eq!(it.source, Source::Core);
            let args: Vec<&str> = it.args.iter().map(|a| a.name.as_str()).collect();
            assert_eq!(args, *required, "{name}");
            assert!(it.args.iter().all(|a| a.required && !a.prompt.is_empty()));
            assert!(!it.keys.is_empty() && !it.title.is_empty(), "{name}");
        }
        let rename = items.iter().find(|i| i.name == "session.rename").unwrap();
        assert_eq!(
            (rename.args[0].prompt.as_str(), rename.keys.as_slice()),
            ("nome", &["n".to_string()][..])
        );
        assert!(
            items
                .iter()
                .any(|i| i.name == "world.tour" && i.title == "tour da recepcionista")
        );
    }

    #[test]
    fn next_waiting_wraps_both_ways() {
        let all = [CatId(1), CatId(2), CatId(3), CatId(4)];
        let w = [CatId(2), CatId(4)];
        let rows = [
            (None, 1, Some(CatId(2))),
            (None, -1, Some(CatId(4))),
            (Some(CatId(1)), 1, Some(CatId(2))),
            (Some(CatId(2)), 1, Some(CatId(4))),
            (Some(CatId(4)), 1, Some(CatId(2))),
            (Some(CatId(2)), -1, Some(CatId(4))),
            (Some(CatId(3)), -1, Some(CatId(2))),
            (Some(CatId(9)), 1, Some(CatId(2))),
        ];
        for (from, dir, want) in rows {
            assert_eq!(next_waiting(&w, from, &all, dir), want, "{from:?} {dir}");
        }
        assert_eq!(
            next_waiting(&[CatId(2)], Some(CatId(2)), &all, 1),
            Some(CatId(2))
        );
        assert_eq!(next_waiting(&[], None, &all, 1), None);
    }
}
