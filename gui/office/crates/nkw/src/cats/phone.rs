//! The cats' phone apps (S24): **Gatos** (every villager; enter opens its
//! actions) and **Inbox** (the cats waiting for you, most urgent first).
//! Data only (S21): the overlay draws and navigates; every action is a
//! command by name carrying the cat (`cat: "<CatId>"`), so it lands on that
//! cat whatever the station. Pages are rebuilt on each call, ≤ `MAX_ITEMS`.

use nk_cats::model::Pose;
use nkw_core::CatId;

use super::office::{Office, RECEPTIONIST, Villager};
use super::session::CAT_ARG;
use super::walk::Walks;
use crate::ui::Icon;
use crate::ui::phone::{ACT_SUB, Act, Item, PhoneApp, PhonePage, PhoneState, SearchSpec};

pub const GATOS: &str = "gatos";
pub const INBOX: &str = "inbox";
/// A page lists at most this many items.
pub const MAX_ITEMS: usize = 100;
/// The receptionist's colour (her tag's).
const CREAM: [u8; 3] = [0xe8, 0x9a, 0x6a];

/// What a cat is doing, in the words of the list (pt-BR).
pub fn state_word(p: Pose) -> &'static str {
    match p {
        Pose::Typing => "trabalhando",
        Pose::Alert => "te esperando",
        Pose::Sleeping => "terminou",
        Pose::Idle => "livre",
        Pose::Puff => "com erro",
    }
}

/// The mark over a cat's head, as a list badge.
pub fn mark(p: Pose) -> Option<&'static str> {
    match p {
        Pose::Alert => Some("!"),
        Pose::Sleeping => Some("zZ"),
        _ => None,
    }
}

fn cat_act(name: &str, cat: CatId, close: bool) -> Act {
    Act::new(name, &[(CAT_ARG, &cat.0.to_string())], close)
}

/// The two apps, with their badges: Gatos = the blocked cats, Inbox = the
/// cats waiting (none: no badge).
pub fn apps(office: &Office, waiting: usize) -> Vec<PhoneApp> {
    let blocked = office.cats.iter().filter(|v| v.pose == Pose::Alert).count() as u32;
    let some = |n: u32| (n > 0).then_some(n);
    vec![
        PhoneApp {
            id: GATOS,
            title: "Gatos",
            icon: Icon::Cat,
            order: 1,
            badge: some(blocked),
        },
        PhoneApp {
            id: INBOX,
            title: "Inbox",
            icon: Icon::Envelope,
            order: 2,
            badge: some(waiting as u32),
        },
    ]
}

/// Gatos: one item per villager, in the office's order.
pub fn gatos(office: &Office) -> PhonePage {
    let items = office
        .cats
        .iter()
        .take(MAX_ITEMS)
        .map(|v| Item {
            id: v.cat.0.to_string(),
            icon: Some(Icon::Cat),
            rgb: Some(v.ctx_rgb),
            title: v.label().to_string(),
            detail: Some(format!("{} · {}", state_word(v.pose), v.ctx)),
            badge: mark(v.pose).map(str::to_string),
            toggle: None,
            on_enter: Some(Act::new(ACT_SUB, &[("sub", &v.cat.0.to_string())], false)),
            on_keys: vec![
                ("t".into(), cat_act("world.talk", v.cat, true)),
                ("g".into(), cat_act("world.goto_cat", v.cat, true)),
                ("r".into(), cat_act("session.reply", v.cat, true)),
            ],
        })
        .collect();
    PhonePage {
        title: "Gatos".into(),
        items,
        empty: Some("nenhum gato: nenhuma sessão no core".into()),
        search: Some(SearchSpec {
            placeholder: "buscar gato".into(),
            live: true,
            run: None,
        }),
        keys: vec![
            ("enter".into(), "ações".into()),
            ("t".into(), "conversa".into()),
            ("g".into(), "ir até".into()),
        ],
        ..PhonePage::default()
    }
}

/// A cat's actions (the Gatos sub-page `sub` = its id).
pub fn actions(v: &Villager) -> PhonePage {
    let c = v.cat;
    let id = c.0.to_string();
    let palette = |name: &str| {
        Act::new(
            "world.palette_run",
            &[("name", name), (CAT_ARG, &id)],
            false,
        )
    };
    let row = |key: &str, title: &str, icon: Icon, act: Act| Item {
        id: key.into(),
        icon: Some(icon),
        title: title.into(),
        on_enter: Some(act),
        ..Item::default()
    };
    let items = vec![
        row(
            "goto",
            "ir até",
            Icon::Map,
            cat_act("world.goto_cat", c, true),
        ),
        row(
            "talk",
            "conversar",
            Icon::Cat,
            cat_act("world.talk", c, true),
        ),
        row(
            "focus",
            "sentar no terminal",
            Icon::Terminal,
            cat_act("world.focus_cat", c, true),
        ),
        row(
            "reply",
            "responder",
            Icon::Envelope,
            cat_act("session.reply", c, true),
        ),
        row(
            "stop",
            "parar",
            Icon::Warning,
            cat_act("session.stop", c, false),
        ),
        row("rename", "renomear", Icon::Info, palette("session.rename")),
        row(
            "move",
            "mover de contexto",
            Icon::Home,
            palette("session.move"),
        ),
        row(
            "archive",
            "arquivar",
            Icon::Check,
            cat_act("session.archive", c, false),
        ),
    ];
    PhonePage {
        title: v.label().to_string(),
        items,
        keys: vec![("enter".into(), "faz".into())],
        ..PhonePage::default()
    }
}

/// Inbox: the queue, most urgent first (a confirmation the receptionist
/// holds comes first, like `world.summon`).
pub fn inbox(office: &Office, walks: &Walks) -> PhonePage {
    let mut items = Vec::new();
    if let Some((_, q)) = &walks.confirm {
        items.push(Item {
            id: RECEPTIONIST.0.to_string(),
            icon: Some(Icon::Question),
            rgb: Some(CREAM),
            title: "recepcionista".into(),
            detail: Some(format!("confirmação: {q}")),
            badge: Some("?".into()),
            on_enter: Some(cat_act("world.talk", RECEPTIONIST, true)),
            ..Item::default()
        });
    }
    for w in walks.q.ordered() {
        let Some(cat) = walks.cat_of(w, office) else {
            continue;
        };
        let (detail, icon, rgb) = match w.kind {
            nk_cats::inbox::Kind::Mail => {
                (format!("carta para {}", w.label), Icon::Envelope, CREAM)
            }
            nk_cats::inbox::Kind::Blocked => (
                "te esperando".to_string(),
                Icon::Cat,
                office.get(cat).map_or(CREAM, |v| v.ctx_rgb),
            ),
            nk_cats::inbox::Kind::Returned => (
                "terminou".to_string(),
                Icon::Cat,
                office.get(cat).map_or(CREAM, |v| v.ctx_rgb),
            ),
        };
        let title = match w.kind {
            nk_cats::inbox::Kind::Mail => "carteiro".to_string(),
            _ => w.label.clone(),
        };
        items.push(Item {
            id: w.key.clone(),
            icon: Some(icon),
            rgb: Some(rgb),
            title,
            detail: Some(detail),
            on_enter: Some(cat_act("world.talk", cat, true)),
            ..Item::default()
        });
    }
    items.truncate(MAX_ITEMS);
    PhonePage {
        title: "Inbox".into(),
        items,
        empty: Some("ninguém te esperando".into()),
        keys: vec![
            ("enter".into(), "conversa".into()),
            ("shift+space".into(), "chama o mais urgente".into()),
        ],
        ..PhonePage::default()
    }
}

/// The page of one of the cats' apps for the phone's state; `None`: not
/// ours. A Gatos sub-page also says which cat it is about (for the
/// palette's argument-less commands that follow).
pub fn page(
    app: &str,
    st: &PhoneState,
    office: &Office,
    walks: &Walks,
) -> Option<(PhonePage, Option<CatId>)> {
    match app {
        GATOS => Some(match st.sub.as_deref() {
            None => (gatos(office), None),
            Some(sub) => {
                let cat = sub.parse::<u32>().ok().map(CatId);
                match cat.and_then(|c| office.get(c)) {
                    Some(v) => (actions(v), Some(v.cat)),
                    None => (
                        PhonePage {
                            title: "Gatos".into(),
                            empty: Some("esse gato foi embora".into()),
                            ..PhonePage::default()
                        },
                        None,
                    ),
                }
            }
        }),
        INBOX => Some((inbox(office, walks), None)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cats::demo::{self, Fixture};

    fn office(f: Fixture) -> Office {
        let (snap, tree) = demo::snapshot(f);
        let mut o = Office::new();
        o.apply(&snap, &tree.rows, 0.0);
        o
    }

    fn act(name: &str, args: &[(&str, &str)], close: bool) -> Option<Act> {
        Some(Act::new(name, args, close))
    }

    #[test]
    fn apps_and_badges() {
        let o = office(Fixture::Demo);
        let a = apps(&o, 2);
        assert_eq!(
            a.iter()
                .map(|x| (x.id, x.title, x.order, x.badge))
                .collect::<Vec<_>>(),
            [
                ("gatos", "Gatos", 1, Some(1)),
                ("inbox", "Inbox", 2, Some(2))
            ]
        );
        assert_eq!((a[0].icon, a[1].icon), (Icon::Cat, Icon::Envelope));
        let a = apps(&Office::new(), 0);
        assert_eq!((a[0].badge, a[1].badge), (None, None));
    }

    #[test]
    fn gatos_lists_every_villager() {
        let o = office(Fixture::Demo);
        let p = gatos(&o);
        let rows: Vec<_> = p
            .items
            .iter()
            .map(|i| (i.title.as_str(), i.detail.clone().unwrap(), i.badge.clone()))
            .collect();
        assert_eq!(
            rows,
            [
                ("acme", "trabalhando · acme".to_string(), None),
                (
                    "api-gateway",
                    "te esperando · acme".into(),
                    Some("!".into())
                ),
                ("alpha", "terminou · demo".into(), Some("zZ".into())),
                ("bravo", "livre · demo".into(), None),
                ("demo", "com erro · demo".into(), None),
            ]
        );
        let gw = &p.items[1];
        let id = o
            .by_session(&demo::id_of("api-gateway"))
            .unwrap()
            .cat
            .0
            .to_string();
        assert_eq!(gw.id, id);
        assert_eq!(gw.on_enter, act(ACT_SUB, &[("sub", &id)], false));
        assert_eq!(
            gw.on_keys[0],
            ("t".into(), Act::new("world.talk", &[("cat", &id)], true))
        );
        assert_eq!(
            gw.rgb,
            Some(o.by_session(&demo::id_of("api-gateway")).unwrap().ctx_rgb)
        );
        assert!(p.search.as_ref().is_some_and(|s| s.live && s.run.is_none()));
        assert_eq!(gatos(&office(Fixture::Busy)).items.len(), 8);
        assert!(gatos(&Office::new()).items.is_empty());
    }

    #[test]
    fn a_cats_actions_carry_the_cat() {
        let o = office(Fixture::Demo);
        let v = o.by_session(&demo::id_of("acme")).unwrap();
        let id = v.cat.0.to_string();
        let c = [("cat", id.as_str())];
        let p = actions(v);
        assert_eq!(p.title, "acme");
        let rows: Vec<_> = p
            .items
            .iter()
            .map(|i| (i.title.as_str(), i.on_enter.clone()))
            .collect();
        assert_eq!(
            rows,
            [
                ("ir até", act("world.goto_cat", &c, true)),
                ("conversar", act("world.talk", &c, true)),
                ("sentar no terminal", act("world.focus_cat", &c, true)),
                ("responder", act("session.reply", &c, true)),
                ("parar", act("session.stop", &c, false)),
                (
                    "renomear",
                    act(
                        "world.palette_run",
                        &[("name", "session.rename"), ("cat", &id)],
                        false
                    )
                ),
                (
                    "mover de contexto",
                    act(
                        "world.palette_run",
                        &[("name", "session.move"), ("cat", &id)],
                        false
                    )
                ),
                ("arquivar", act("session.archive", &c, false)),
            ]
        );
    }

    #[test]
    fn page_routes_apps_and_subs() {
        let o = office(Fixture::Demo);
        let w = Walks::default();
        let st = |sub: Option<&str>| PhoneState {
            app: Some("gatos".into()),
            sub: sub.map(str::to_string),
            ..PhoneState::default()
        };
        let (p, c) = page("gatos", &st(None), &o, &w).unwrap();
        assert_eq!((p.items.len(), c), (5, None));
        let acme = o.by_session(&demo::id_of("acme")).unwrap().cat;
        let (p, c) = page("gatos", &st(Some(&acme.0.to_string())), &o, &w).unwrap();
        assert_eq!((p.title.as_str(), c), ("acme", Some(acme)));
        let (p, c) = page("gatos", &st(Some("424242")), &o, &w).unwrap();
        assert_eq!(
            (p.items.len(), c, p.empty.as_deref()),
            (0, None, Some("esse gato foi embora"))
        );
        let (p, _) = page("inbox", &st(None), &o, &w).unwrap();
        assert_eq!(p.title, "Inbox");
        assert!(page("musica", &st(None), &o, &w).is_none());
    }

    #[test]
    fn inbox_lists_the_queue_most_urgent_first() {
        let o = office(Fixture::Busy);
        let mut w = Walks::default();
        w.mail.insert(demo::id_of("bravo"), (0, false));
        w.step(&o, nk_cats::model::parse_rfc3339(demo::T0) + 12 * 60_000);
        let p = inbox(&o, &w);
        assert!(!p.items.is_empty());
        for i in &p.items {
            let a = i.on_enter.as_ref().unwrap();
            assert_eq!((a.name.as_str(), a.close), ("world.talk", true));
            assert!(a.args.contains_key("cat"));
        }
        assert_eq!(p.items.len(), w.q.walkers.len());
        let first = w.most_urgent(&o).unwrap();
        assert_eq!(
            p.items[0].on_enter.as_ref().unwrap().args["cat"],
            first.0.to_string()
        );
        assert!(
            p.keys
                .iter()
                .any(|(k, w)| k == "shift+space" && w == "chama o mais urgente")
        );
        // a confirmation comes first, at the receptionist
        w.confirm = Some(("t1".into(), "Parar acme?".into()));
        let p = inbox(&o, &w);
        assert_eq!(p.items[0].title, "recepcionista");
        assert_eq!(
            p.items[0].detail.as_deref(),
            Some("confirmação: Parar acme?")
        );
        assert_eq!(
            inbox(&Office::new(), &Walks::default()).empty.as_deref(),
            Some("ninguém te esperando")
        );
    }
}
