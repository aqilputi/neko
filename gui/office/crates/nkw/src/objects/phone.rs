//! The objects' phone apps (contract S25): Música (the turntable's player),
//! Biblioteca (the library's search) and Cofre (the vault's state). Pure page
//! builders: data in, `PhonePage` out; the overlay draws and navigates them.
//!
//! Privacy (rule 15): a media title and artist reach only the Música page's
//! header (drawn, never dumped by us); the Cofre page has a state and a count,
//! never an entry name; the Biblioteca page lists the library's last search,
//! which the library already keeps clear of the vault.

use crate::ui::phone::{Act, Item, PhoneApp, PhonePage, SearchSpec};
use crate::ui::{Icon, Node, Theme};

pub const MUSICA: &str = "musica";
pub const BIBLIOTECA: &str = "biblioteca";
pub const COFRE: &str = "cofre";
/// The Biblioteca app lists at most this many hits.
pub const MAX_HITS: usize = super::library::MAX_PHONE_HITS;
/// The search command the Biblioteca's line runs (S19, unchanged).
pub const SEARCH_CMD: &str = "world.library_search";

/// The three apps, in the S25 order.
pub fn apps(vault_open: bool) -> Vec<PhoneApp> {
    vec![
        PhoneApp {
            id: MUSICA,
            title: "Música",
            icon: Icon::Play,
            order: 6,
            badge: None,
        },
        PhoneApp {
            id: BIBLIOTECA,
            title: "Biblioteca",
            icon: Icon::Book,
            order: 7,
            badge: None,
        },
        PhoneApp {
            id: COFRE,
            title: "Cofre",
            icon: if vault_open {
                Icon::LockOpen
            } else {
                Icon::Lock
            },
            order: 8,
            badge: None,
        },
    ]
}

fn item(id: &str, icon: Icon, title: &str, act: Act) -> Item {
    Item {
        id: id.into(),
        icon: Some(icon),
        title: title.into(),
        on_enter: Some(act),
        ..Item::default()
    }
}

/// What the Música page shows of the player: read at the moment from the
/// turntable's data.
#[derive(Clone, Debug, PartialEq)]
pub struct Player<'a> {
    pub title: &'a str,
    pub artist: &'a str,
    pub playing: bool,
    /// 0..1
    pub volume: f32,
}

/// Música: the player's controls; `None` = no MPRIS player in the session.
pub fn musica(player: Option<&Player>, muted: bool) -> PhonePage {
    let Some(p) = player else {
        return PhonePage {
            title: "Música".into(),
            empty: Some("nenhum player (MPRIS) na sessão".into()),
            ..PhonePage::default()
        };
    };
    let t = Theme::cozy();
    let pct = (p.volume.clamp(0.0, 1.0) * 100.0).round() as u16;
    let header = Node::Column {
        gap: 4,
        items: vec![
            Node::Row {
                gap: 8,
                items: vec![
                    Node::Icon {
                        kind: if p.playing { Icon::Play } else { Icon::Pause },
                        px: 16,
                        rgb: t.accent,
                    },
                    Node::Text {
                        s: if p.title.is_empty() {
                            "—".into()
                        } else {
                            p.title.into()
                        },
                        px: 16,
                        rgb: t.ink,
                        bold: true,
                    },
                ],
            },
            Node::Text {
                s: p.artist.into(),
                px: 13,
                rgb: t.soft,
                bold: false,
            },
            Node::Gauge {
                permille: if muted { 0 } else { pct * 10 },
                w: 160,
                h: 6,
                rgb: t.accent,
            },
        ],
    };
    let mut mute = item(
        "mudo",
        Icon::Dot,
        "mudo",
        Act::new("volume.mute", &[], false),
    );
    mute.toggle = Some(muted);
    PhonePage {
        title: "Música".into(),
        header: Some(header),
        items: vec![
            item(
                "tocar",
                if p.playing { Icon::Pause } else { Icon::Play },
                if p.playing { "pausar" } else { "tocar" },
                Act::new("media.play_pause", &[], false),
            ),
            item(
                "anterior",
                Icon::Arrow,
                "anterior",
                Act::new("media.prev", &[], false),
            ),
            item(
                "proxima",
                Icon::Arrow,
                "próxima",
                Act::new("media.next", &[], false),
            ),
            Item {
                detail: Some(format!("{pct} %")),
                ..item(
                    "vol-",
                    Icon::Dot,
                    "volume −",
                    Act::new("volume.down", &[], false),
                )
            },
            Item {
                detail: Some(format!("{pct} %")),
                ..item(
                    "vol+",
                    Icon::Dot,
                    "volume +",
                    Act::new("volume.up", &[], false),
                )
            },
            mute,
        ],
        keys: vec![("enter".into(), "aciona".into())],
        ..PhonePage::default()
    }
}

pub use super::library::Hit;

/// Biblioteca: the search line (`run` = the library's search) and the last
/// search's hits (≤ `MAX_HITS`). `hits = None`: nothing searched yet.
pub fn biblioteca(hits: Option<&[Hit]>, searching: bool) -> PhonePage {
    let search = Some(SearchSpec {
        placeholder: "buscar na biblioteca".into(),
        live: false,
        run: Some(SEARCH_CMD.into()),
    });
    let keys = vec![
        ("/".into(), "busca".into()),
        ("enter".into(), "abre".into()),
        ("e".into(), "edita".into()),
        ("y".into(), "copia o caminho".into()),
    ];
    let empty = Some(
        if searching {
            "procurando…"
        } else if hits.is_some() {
            "nada encontrado"
        } else {
            "/ para buscar nas estantes"
        }
        .into(),
    );
    let items: Vec<Item> = hits
        .unwrap_or(&[])
        .iter()
        .take(MAX_HITS)
        .enumerate()
        .map(|(i, h)| {
            let path = [("path", h.path.as_str())];
            Item {
                id: format!("hit:{i}"),
                icon: Some(if h.is_dir { Icon::Grid } else { Icon::Book }),
                title: h.name.clone(),
                detail: (!h.folder.is_empty()).then(|| h.folder.clone()),
                on_enter: Some(Act::new("world.open", &path, true)),
                on_keys: vec![
                    ("e".into(), Act::new("world.book_edit", &path, true)),
                    (
                        "y".into(),
                        Act::new("world.copy", &[("text", h.path.as_str())], false),
                    ),
                ],
                ..Item::default()
            }
        })
        .collect();
    let n = hits.map_or(0, <[Hit]>::len);
    PhonePage {
        title: "Biblioteca".into(),
        header: hits.map(|_| Node::Text {
            s: format!("{n} achados"),
            px: 13,
            rgb: Theme::cozy().soft,
            bold: false,
        }),
        items,
        empty,
        search,
        keys,
    }
}

/// The vault as the Cofre page sees it: a state and numbers, never a name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VaultView {
    Locked,
    /// open for `secs`, `cards` cards shown
    Open {
        secs: u64,
        cards: usize,
    },
    /// too many wrong phrases: `secs` left
    Lockout {
        secs: u64,
    },
}

/// Cofre: the state, "trancar" and "ir até o cofre".
pub fn cofre(v: VaultView) -> PhonePage {
    let t = Theme::cozy();
    let (icon, line) = match v {
        VaultView::Locked => (Icon::Lock, "trancado".to_string()),
        VaultView::Open { secs, cards } => (
            Icon::LockOpen,
            format!("aberto há {secs} s · {cards} cartões"),
        ),
        VaultView::Lockout { secs } => (Icon::Warning, format!("bloqueado por {secs} s")),
    };
    let header = Node::Row {
        gap: 8,
        items: vec![
            Node::Icon {
                kind: icon,
                px: 16,
                rgb: t.ink,
            },
            Node::Text {
                s: line,
                px: 15,
                rgb: t.ink,
                bold: true,
            },
        ],
    };
    PhonePage {
        title: "Cofre".into(),
        header: Some(header),
        items: vec![
            Item {
                detail: Some("fecha agora; os cartões somem".into()),
                ..item(
                    "trancar",
                    Icon::Lock,
                    "trancar",
                    Act::new("world.vault_lock", &[], false),
                )
            },
            Item {
                detail: Some("abre com a frase, falando com a bibliotecária".into()),
                ..item(
                    "ir",
                    Icon::Arrow,
                    "ir até o cofre",
                    Act::new("world.goto_object", &[("name", "cofre")], true),
                )
            },
        ],
        ..PhonePage::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn acts(p: &PhonePage) -> Vec<(String, bool)> {
        p.items
            .iter()
            .filter_map(|i| i.on_enter.as_ref())
            .map(|a| (a.name.clone(), a.close))
            .collect()
    }

    #[test]
    fn three_apps_in_the_s25_order() {
        let a = apps(false);
        let ids: Vec<(&str, &str, u8)> = a.iter().map(|x| (x.id, x.title, x.order)).collect();
        assert_eq!(
            ids,
            [
                ("musica", "Música", 6),
                ("biblioteca", "Biblioteca", 7),
                ("cofre", "Cofre", 8)
            ]
        );
        assert_eq!(a[0].icon, Icon::Play);
        assert_eq!(a[1].icon, Icon::Book);
        assert_eq!(a[2].icon, Icon::Lock);
        assert_eq!(apps(true)[2].icon, Icon::LockOpen);
    }

    #[test]
    fn musica_without_a_player_is_empty() {
        let p = musica(None, false);
        assert!(p.items.is_empty());
        assert_eq!(p.empty.as_deref(), Some("nenhum player (MPRIS) na sessão"));
        assert!(p.header.is_none());
    }

    #[test]
    fn musica_controls_dispatch_the_media_and_volume_commands() {
        let pl = Player {
            title: "Lo-fi for cats",
            artist: "acme radio",
            playing: true,
            volume: 0.6,
        };
        let p = musica(Some(&pl), false);
        assert_eq!(
            acts(&p),
            [
                ("media.play_pause".into(), false),
                ("media.prev".into(), false),
                ("media.next".into(), false),
                ("volume.down".into(), false),
                ("volume.up".into(), false),
                ("volume.mute".into(), false),
            ]
        );
        assert_eq!(p.items[0].title, "pausar");
        assert_eq!(p.items[3].detail.as_deref(), Some("60 %"));
        assert_eq!(p.items[5].toggle, Some(false));
        assert!(format!("{:?}", p.header).contains("Lo-fi for cats"));
        let paused = musica(
            Some(&Player {
                playing: false,
                ..pl.clone()
            }),
            true,
        );
        assert_eq!(paused.items[0].title, "tocar");
        assert_eq!(paused.items[5].toggle, Some(true));
    }

    fn hit(i: usize) -> Hit {
        Hit {
            path: format!("~/projeto/notas{i}.md"),
            name: format!("notas{i}.md"),
            is_dir: false,
            folder: "projeto".into(),
        }
    }

    #[test]
    fn biblioteca_searches_through_the_library_and_lists_hits() {
        let p = biblioteca(None, false);
        let s = p.search.clone().unwrap();
        assert!(!s.live);
        assert_eq!(s.run.as_deref(), Some("world.library_search"));
        assert!(p.items.is_empty());
        assert_eq!(p.empty.as_deref(), Some("/ para buscar nas estantes"));
        assert_eq!(biblioteca(None, true).empty.as_deref(), Some("procurando…"));
        assert_eq!(
            biblioteca(Some(&[]), false).empty.as_deref(),
            Some("nada encontrado")
        );
        let hits: Vec<Hit> = (0..70).map(hit).collect();
        let p = biblioteca(Some(&hits), false);
        assert_eq!(p.items.len(), MAX_HITS);
        assert!(format!("{:?}", p.header).contains("70 achados"));
        let i = &p.items[2];
        assert_eq!(i.title, "notas2.md");
        assert_eq!(i.detail.as_deref(), Some("projeto"));
        let open = i.on_enter.as_ref().unwrap();
        assert_eq!(
            (open.name.as_str(), open.args["path"].as_str(), open.close),
            ("world.open", "~/projeto/notas2.md", true)
        );
        let keys: Vec<(&str, &str)> = i
            .on_keys
            .iter()
            .map(|(k, a)| (k.as_str(), a.name.as_str()))
            .collect();
        assert_eq!(keys, [("e", "world.book_edit"), ("y", "world.copy")]);
        assert_eq!(i.on_keys[1].1.args["text"], "~/projeto/notas2.md");
    }

    #[test]
    fn cofre_shows_a_state_and_numbers_only() {
        let p = cofre(VaultView::Locked);
        assert!(format!("{:?}", p.header).contains("trancado"));
        assert_eq!(
            acts(&p),
            [
                ("world.vault_lock".into(), false),
                ("world.goto_object".into(), true)
            ]
        );
        assert_eq!(p.items[1].on_enter.as_ref().unwrap().args["name"], "cofre");
        let o = cofre(VaultView::Open { secs: 7, cards: 5 });
        assert!(format!("{:?}", o.header).contains("aberto há 7 s · 5 cartões"));
        let l = cofre(VaultView::Lockout { secs: 42 });
        assert!(format!("{:?}", l.header).contains("bloqueado por 42 s"));
    }
}
