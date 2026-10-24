//! The phone's apps the overlay owns (S21): Mapa (an action: it closes the
//! phone and opens the map), Ajustes (the prefs, live, and the walk options
//! through `world.walk_set`) and Ajuda (the keys of every mode from the live
//! keymap, the current mode's first). Pages are data; the shell draws them.

use nk_cats::keys::Bind;
use nkw_core::TurnMode;
use nkw_core::rules::WalkConfig;

use super::phone::ACT_PREF;
use super::prefs::Prefs;
use crate::router::table;
use crate::ui::Icon;
use crate::ui::phone::{Act, Item, PhoneApp, PhonePage, SearchSpec};

pub const MAPA: &str = "mapa";
pub const AJUSTES: &str = "ajustes";
pub const AJUDA: &str = "ajuda";

pub fn apps() -> Vec<PhoneApp> {
    vec![
        PhoneApp {
            id: MAPA,
            title: "Mapa",
            icon: Icon::Map,
            order: 3,
            badge: None,
        },
        PhoneApp {
            id: AJUSTES,
            title: "Ajustes",
            icon: Icon::Gear,
            order: 9,
            badge: None,
        },
        PhoneApp {
            id: AJUDA,
            title: "Ajuda",
            icon: Icon::Question,
            order: 10,
            badge: None,
        },
    ]
}

/// What Ajustes shows besides the prefs.
pub struct Facts<'a> {
    pub walk: &'a WalkConfig,
    /// `ctrl+alt` | `super`
    pub world_mod: &'a str,
    pub focus_escape: &'a str,
    /// the hub's reduced motion (CLI/env), under the overlay's override
    pub hub_reduced: bool,
    /// prefs are written (not under `--demo`)
    pub persisted: bool,
}

fn pref(key: &str) -> Act {
    Act::new(ACT_PREF, &[("key", key)], false)
}

fn walk_set(key: &str, value: &str) -> Act {
    Act::new("world.walk_set", &[("key", key), ("value", value)], false)
}

/// The Ajustes page.
pub fn ajustes(p: &Prefs, f: &Facts) -> PhonePage {
    let saved = if f.persisted {
        "fica salvo no ui.toml"
    } else {
        "não fica salvo (--demo)"
    };
    let walk_note = "só agora · para ficar: world.toml [walk]";
    let reduced = p.reduced_motion.unwrap_or(f.hub_reduced);
    let snap = f.walk.turn == TurnMode::Snap;
    let items = vec![
        Item {
            id: "reduced_motion".into(),
            icon: Some(Icon::Pause),
            title: "movimento reduzido".into(),
            detail: Some(format!("{saved} · no mundo inteiro: --reduced-motion")),
            toggle: Some(reduced),
            on_enter: Some(pref("reduced_motion")),
            ..Item::default()
        },
        Item {
            id: "contrast".into(),
            icon: Some(Icon::Info),
            title: "alto contraste".into(),
            detail: Some(saved.into()),
            toggle: Some(p.high_contrast),
            on_enter: Some(pref("contrast")),
            ..Item::default()
        },
        Item {
            id: "font_scale".into(),
            icon: Some(Icon::Search),
            title: "escala da fonte".into(),
            detail: Some(format!("h/l ajusta · {saved}")),
            badge: Some(format!("{} %", p.font_scale_pct)),
            on_enter: Some(Act::new(
                ACT_PREF,
                &[("key", "font_scale"), ("dir", "up")],
                false,
            )),
            on_keys: vec![
                (
                    "h".into(),
                    Act::new(ACT_PREF, &[("key", "font_scale"), ("dir", "down")], false),
                ),
                (
                    "l".into(),
                    Act::new(ACT_PREF, &[("key", "font_scale"), ("dir", "up")], false),
                ),
            ],
            ..Item::default()
        },
        Item {
            id: "strip".into(),
            icon: Some(Icon::Clock),
            title: "faixa de status".into(),
            detail: Some(format!("auto · sempre · nunca · {saved}")),
            badge: Some(
                p.strip
                    .map_or("auto (world.toml)".into(), |s| s.label().into()),
            ),
            on_enter: Some(pref("strip")),
            ..Item::default()
        },
        Item {
            id: "turn".into(),
            icon: Some(Icon::Arrow),
            title: "giro em passos".into(),
            detail: Some(walk_note.into()),
            toggle: Some(snap),
            on_enter: Some(walk_set("turn", if snap { "smooth" } else { "snap" })),
            ..Item::default()
        },
        Item {
            id: "mouse_look".into(),
            icon: Some(Icon::Dot),
            title: "olhar com o mouse".into(),
            detail: Some(walk_note.into()),
            toggle: Some(f.walk.mouse_look),
            on_enter: Some(walk_set(
                "mouse_look",
                if f.walk.mouse_look { "false" } else { "true" },
            )),
            ..Item::default()
        },
        Item {
            id: "comfort_vignette".into(),
            icon: Some(Icon::Window),
            title: "vinheta de conforto".into(),
            detail: Some(walk_note.into()),
            toggle: Some(f.walk.comfort_vignette),
            on_enter: Some(walk_set(
                "comfort_vignette",
                if f.walk.comfort_vignette {
                    "false"
                } else {
                    "true"
                },
            )),
            ..Item::default()
        },
        Item {
            id: "world_mod".into(),
            icon: Some(Icon::Grid),
            title: format!("tecla do mundo: {}", f.world_mod),
            detail: Some("--world-mod ou world.toml [keys] world_mod".into()),
            ..Item::default()
        },
        Item {
            id: "focus_escape".into(),
            icon: Some(Icon::Window),
            title: format!("sair do FOCO: {}", f.focus_escape),
            detail: Some("world.toml [keys] focus_escape".into()),
            ..Item::default()
        },
    ];
    PhonePage {
        title: "Ajustes".into(),
        items,
        keys: vec![("enter".into(), "muda".into())],
        ..PhonePage::default()
    }
}

/// The keymap modes the help lists, in order, with their pt-BR names.
pub const SECTIONS: &[(&str, &str)] = &[
    ("normal.world", "NAVEGAR"),
    ("world.walk", "ANDAR"),
    ("world.focus", "FOCO"),
    ("world.talk", "CONVERSA"),
    ("world.phone", "TELEFONE"),
    ("world", "em qualquer modo (tecla do mundo)"),
    ("world.library", "ESTANTE"),
    ("world.map", "MAPA"),
    ("world.hint", "SALTO"),
    ("world.palette", "COMANDOS"),
    ("world.line", "LINHA DE RESPOSTA"),
];

/// A binding's title: the keymap's help (the core's `help.desc`), else the
/// table's, else the command.
fn title(b: &Bind) -> String {
    if !b.help.is_empty() {
        return b.help.clone();
    }
    table::title(&b.command)
        .map(String::from)
        .unwrap_or_else(|| b.command.clone())
}

/// The bindings of one mode as (keys, title), in the keymap's order.
pub fn bindings_of(binds: &[Bind], mode: &str) -> Vec<(Vec<String>, String)> {
    binds
        .iter()
        .filter(|b| b.mode == mode && !b.keys.is_empty() && !b.command.is_empty())
        .map(|b| (b.keys.clone(), title(b)))
        .collect()
}

/// The Ajuda page: the current mode's section first, then the others.
pub fn ajuda(binds: &[Bind], current: &str) -> PhonePage {
    let mut order: Vec<&(&str, &str)> = SECTIONS.iter().filter(|s| s.0 == current).collect();
    order.extend(SECTIONS.iter().filter(|s| s.0 != current));
    let mut items = Vec::new();
    for (mode, name) in order {
        let rows = bindings_of(binds, mode);
        if rows.is_empty() {
            continue;
        }
        items.push(Item {
            id: format!("sec:{mode}"),
            icon: Some(Icon::Info),
            title: if *mode == current {
                format!("{name} (agora)")
            } else {
                name.to_string()
            },
            ..Item::default()
        });
        for (k, (keys, t)) in rows.into_iter().enumerate() {
            items.push(Item {
                id: format!("{mode}#{k}"),
                title: t,
                badge: Some(keys.join("  ")),
                detail: Some(name.to_string()),
                ..Item::default()
            });
        }
    }
    PhonePage {
        title: "Ajuda".into(),
        items,
        empty: Some("sem teclas".into()),
        search: Some(SearchSpec {
            placeholder: "buscar uma tecla ou um comando".into(),
            live: true,
            run: None,
        }),
        ..PhonePage::default()
    }
}

/// The Mapa page (never shown: the app is an action; kept for a plugin
/// that asks for it by name).
pub fn mapa() -> PhonePage {
    PhonePage {
        title: "Mapa".into(),
        items: vec![Item {
            id: "overview".into(),
            icon: Some(Icon::Map),
            title: "abrir o mapa".into(),
            on_enter: Some(Act::new("world.overview", &[], true)),
            ..Item::default()
        }],
        ..PhonePage::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::router::{WorldMod, fallback_binds};

    #[test]
    fn ajuda_lists_the_current_mode_first_from_the_live_keymap() {
        let mut binds = fallback_binds(WorldMod::CtrlAlt, "ctrl+]");
        let p = ajuda(&binds, "world.walk");
        assert_eq!(p.items[0].id, "sec:world.walk");
        assert!(p.items[0].title.contains("agora"));
        assert!(p.items.iter().any(|i| i.title == "andar"));
        assert!(p.search.as_ref().unwrap().live);
        // a Lua rebind shows with its help
        binds.push(Bind {
            command: "lua.key:normal.world:n".into(),
            keys: vec!["n".into()],
            mode: "normal.world".into(),
            help: "meu atalho".into(),
        });
        let p = ajuda(&binds, "normal.world");
        assert!(
            p.items
                .iter()
                .any(|i| i.title == "meu atalho" && i.badge.as_deref() == Some("n"))
        );
        let rows = bindings_of(&binds, "world.focus");
        assert_eq!(
            rows,
            [(vec!["ctrl+]".to_string()], "sai do FOCO".to_string())]
        );
    }

    #[test]
    fn ajustes_reflects_prefs_and_walk() {
        let walk = WalkConfig::default();
        let f = Facts {
            walk: &walk,
            world_mod: "ctrl+alt",
            focus_escape: "ctrl+]",
            hub_reduced: true,
            persisted: false,
        };
        let p = ajustes(&Prefs::default(), &f);
        let by = |id: &str| p.items.iter().find(|i| i.id == id).unwrap().clone();
        assert_eq!(by("reduced_motion").toggle, Some(true), "the hub's value");
        assert_eq!(by("contrast").toggle, Some(false));
        assert_eq!(by("font_scale").badge.as_deref(), Some("100 %"));
        assert_eq!(by("font_scale").on_keys.len(), 2);
        assert!(by("turn").on_enter.unwrap().name == "world.walk_set");
        assert!(by("world_mod").title.contains("ctrl+alt"));
        assert!(by("contrast").detail.unwrap().contains("--demo"));
        assert!(
            by("mouse_look")
                .detail
                .unwrap()
                .contains("world.toml [walk]")
        );
        let q = ajustes(
            &Prefs {
                reduced_motion: Some(false),
                ..Prefs::default()
            },
            &f,
        );
        assert_eq!(
            q.items
                .iter()
                .find(|i| i.id == "reduced_motion")
                .unwrap()
                .toggle,
            Some(false),
            "the override wins"
        );
        assert_eq!(
            apps().iter().map(|a| a.order).collect::<Vec<_>>(),
            [3, 9, 10]
        );
        assert_eq!(
            mapa().items[0].on_enter.as_ref().unwrap().name,
            "world.overview"
        );
    }
}
