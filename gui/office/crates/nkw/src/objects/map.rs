//! The pure glue between `nkw-sys`, `nkw-objects` and the hub: system
//! events as object data, the readouts and badges, the palette entries and
//! the scheduling cap.

use std::collections::BTreeMap;

use nkw_core::{ObjectId, Want};
use nkw_objects::hud;
use nkw_objects::object::{Clock, Data, Kind, Load, Media, Net, Power};
use nkw_sys::{ClockTick, MediaState, NetState, PowerState, SysEvent};

use crate::ext::{ArgKind, ArgSpec, Badge, PaletteItem, Source};

/// Most `Want::Rate` wishes the objects hold at once (the nearest win).
pub const MAX_RATES: usize = 4;

pub fn media(m: &MediaState, sink_volume: Option<f32>) -> Media {
    Media {
        playing: m.playing,
        title: m.title.clone(),
        artist: m.artist.clone(),
        has_art: m.art_url.is_some(),
        // the knob is the default sink's volume (addendum §2.2)
        volume: sink_volume.unwrap_or(m.volume).clamp(0.0, 1.0),
        position_s: m.position_us.map(|us| us as f32 / 1e6),
        length_s: m.length_us.map(|us| us as f32 / 1e6),
    }
}

pub fn power(p: &PowerState) -> Power {
    Power {
        pct: p.pct.min(100),
        charging: p.charging,
        time_to_empty_s: p.time_to_empty_s,
        present: p.present,
    }
}

pub fn net(n: &NetState) -> Net {
    Net {
        connected: n.connected,
        wifi: n.wifi,
        strength: n.strength.min(4),
    }
}

pub fn clock(c: &ClockTick) -> Clock {
    Clock {
        unix_s: c.unix_s,
        hour: c.hour,
        minute: c.minute,
        band: c.band.min(8),
    }
}

/// Which objects (by kind) an event feeds, and with what. `Media` and the
/// sink volume need the other's latest value: the caller passes it.
/// `MediaGone`, `Audio` and `Error` feed no object data here.
pub fn feeds(ev: &SysEvent, sink_volume: Option<f32>) -> Vec<(Kind, Data)> {
    match ev {
        SysEvent::Media(m) => vec![(Kind::Turntable, Data::Media(media(m, sink_volume)))],
        SysEvent::Power(p) => vec![(Kind::Battery, Data::Power(power(p)))],
        SysEvent::Net(n) => vec![(Kind::NetPlant, Data::Net(net(n)))],
        SysEvent::Clock(c) => vec![(Kind::WallClock, Data::Clock(clock(c)))],
        SysEvent::Pressure { cpu, mem } => {
            let load = |pressure| {
                Data::Load(Load {
                    cpu_pct: None,
                    mem_pct: None,
                    pressure,
                })
            };
            vec![(Kind::Fireplace, load(*cpu)), (Kind::Aquarium, load(*mem))]
        }
        SysEvent::MediaGone | SysEvent::Audio(_) | SysEvent::Error { .. } => Vec::new(),
    }
}

/// The 2D readout of an object at the readable tier (§2.3), pt-BR.
pub fn readout(kind: Kind, d: Option<&Data>) -> Option<String> {
    Some(match (kind, d?) {
        (Kind::WallClock, Data::Clock(c)) => format!("{:02}:{:02}", c.hour, c.minute),
        (Kind::Battery, Data::Power(p)) if p.present => {
            if p.charging {
                format!("{} % ⚡", p.pct)
            } else {
                format!("{} %", p.pct)
            }
        }
        (Kind::Battery, Data::Power(_)) => "sem bateria".into(),
        (Kind::Turntable, Data::Media(m)) => match (m.title.is_empty(), m.artist.is_empty()) {
            (true, _) => "sem título".into(),
            (false, true) => m.title.clone(),
            (false, false) => format!("{} — {}", m.title, m.artist),
        },
        (Kind::NetPlant, Data::Net(n)) if !n.connected => "sem rede".into(),
        (Kind::NetPlant, Data::Net(n)) if !n.wifi => "cabo".into(),
        (Kind::NetPlant, Data::Net(n)) if n.strength == 1 => "1 barra".into(),
        (Kind::NetPlant, Data::Net(n)) => format!("{} barras", n.strength),
        (Kind::Lamp, Data::Brightness(b)) => format!("{} %", (b * 100.0).round() as u32),
        (Kind::PackageBox, Data::Updates(u)) => format!("{} atualizações", u.count),
        (Kind::KeyboardFlag, Data::Layout(l)) => l.clone(),
        _ => return None,
    })
}

/// `nkw-objects`' badge as the HUD's.
pub fn badge(b: hud::Badge) -> Badge {
    match b {
        hud::Badge::BatteryLow(p) => Badge::BatteryLow(p),
        hud::Badge::NetDown => Badge::NetDown,
        hud::Badge::OnAir => Badge::OnAir,
        hud::Badge::VaultOpen => Badge::VaultOpen,
        hud::Badge::Updates(n) => Badge::Updates(n),
    }
}

/// The names `world.goto_object` takes for a kind (pt-BR first).
pub fn names(kind: Kind) -> &'static [&'static str] {
    match kind {
        Kind::Turntable => &["vitrola", "turntable", "música"],
        Kind::WallClock => &["relógio", "clock"],
        Kind::Battery => &["bateria", "pote", "battery"],
        Kind::NetPlant => &["planta", "rede", "plant"],
        Kind::Fireplace => &["lareira", "fireplace"],
        Kind::Aquarium => &["aquário", "aquarium"],
        Kind::PackageBox => &["encomendas", "caixa", "updates"],
        Kind::TrayShelf => &["prateleira", "bandeja", "tray"],
        Kind::OnAir => &["on air", "onair"],
        Kind::KeyboardFlag => &["bandeira", "teclado", "keyboard"],
        Kind::Calendar => &["calendário", "calendar"],
        Kind::BtRadio => &["rádio", "bluetooth", "radio"],
        Kind::Lamp => &["lustre", "luz", "lamp"],
        Kind::Vault => &["cofre", "vault"],
        Kind::Door(_) => &["porta", "door"],
    }
}

/// Whether `query` names `kind` (case and accents as typed, trimmed).
pub fn matches_name(kind: Kind, query: &str) -> bool {
    let q = query.trim().to_lowercase();
    !q.is_empty() && names(kind).iter().any(|n| *n == q)
}

/// The palette's object commands (S9: palette-only, plus the media keys of
/// the `world` mode).
pub fn palette() -> Vec<PaletteItem> {
    let item = |name: &str, title: &str, keys: &[&str]| PaletteItem {
        name: name.into(),
        title: title.into(),
        keys: keys.iter().map(|k| k.to_string()).collect(),
        source: Source::Object,
        args: Vec::new(),
    };
    let mut v = vec![
        item("media.play_pause", "tocar/pausar", &["XF86AudioPlay"]),
        item("media.next", "próxima faixa", &["XF86AudioNext"]),
        item("media.prev", "faixa anterior", &["XF86AudioPrev"]),
        item("volume.up", "aumentar o volume", &["XF86AudioRaiseVolume"]),
        item("volume.down", "baixar o volume", &["XF86AudioLowerVolume"]),
        item("volume.mute", "mudo", &["XF86AudioMute"]),
        item("world.goto_object", "ir até o objeto", &[]),
    ];
    v[6].args.push(ArgSpec {
        name: "name".into(),
        prompt: "objeto".into(),
        kind: ArgKind::Text,
        required: true,
    });
    // the library (S19): palette-only, plus the shelf search
    let text = |name: &str, prompt: &str| ArgSpec {
        name: name.into(),
        prompt: prompt.into(),
        kind: ArgKind::Text,
        required: true,
    };
    let mut open = item("world.open", "abre um arquivo no mundo", &[]);
    open.args.push(text("path", "caminho"));
    let mut search = item("world.library_search", "busca na estante", &["/"]);
    search.args.push(text("q", "procurar"));
    v.push(open);
    v.push(search);
    v.push(item("world.vault_lock", "tranca o cofre", &[]));
    v
}

/// One object's scheduling need this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Need {
    pub id: ObjectId,
    pub want: Want,
    /// metres from the camera (the nearest `Rate`s win)
    pub dist: f32,
}

/// At most `max` `Rate` wishes, the nearest; everything else passes.
pub fn cap_rates(mut needs: Vec<Need>, max: usize) -> Vec<Need> {
    needs.sort_by(|a, b| a.dist.total_cmp(&b.dist).then(a.id.cmp(&b.id)));
    let mut rates = 0;
    needs.retain(|n| {
        if matches!(n.want, Want::Rate(_)) {
            rates += 1;
            rates <= max
        } else {
            true
        }
    });
    needs
}

/// `sys` in the dump: each adapter's state.
pub fn sys_status(running: bool, errors: &BTreeMap<String, String>) -> serde_json::Value {
    let mut m = serde_json::Map::new();
    for name in ["mpris", "upower", "nm", "clock", "pipewire", "psi"] {
        let s = if !running {
            "off"
        } else if errors.contains_key(name)
            || (matches!(name, "upower" | "nm") && errors.contains_key("system-bus"))
        {
            "error"
        } else {
            "ok"
        };
        m.insert(name.into(), s.into());
    }
    serde_json::Value::Object(m)
}

#[cfg(test)]
mod tests {
    use super::*;
    use nkw_sys::{AudioEvent, StreamId};

    fn media_state(playing: bool) -> MediaState {
        MediaState {
            playing,
            title: "Lo-fi for cats".into(),
            artist: "acme radio".into(),
            art_url: Some("file:///x.png".into()),
            volume: 0.4,
            position_us: Some(1_500_000),
            length_us: None,
            player: "demo".into(),
        }
    }

    #[test]
    fn sys_events_as_object_data() {
        let rows: Vec<(SysEvent, Vec<Kind>)> = vec![
            (SysEvent::Media(media_state(true)), vec![Kind::Turntable]),
            (
                SysEvent::Power(PowerState {
                    present: true,
                    pct: 12,
                    charging: false,
                    time_to_empty_s: Some(1800),
                }),
                vec![Kind::Battery],
            ),
            (
                SysEvent::Net(NetState {
                    connected: true,
                    wifi: true,
                    strength: 9,
                }),
                vec![Kind::NetPlant],
            ),
            (
                SysEvent::Clock(ClockTick {
                    unix_s: 5,
                    hour: 14,
                    minute: 5,
                    band: 4,
                }),
                vec![Kind::WallClock],
            ),
            (
                SysEvent::Pressure {
                    cpu: true,
                    mem: false,
                },
                vec![Kind::Fireplace, Kind::Aquarium],
            ),
            (SysEvent::MediaGone, vec![]),
            (SysEvent::Audio(AudioEvent::Removed(StreamId(3))), vec![]),
            (
                SysEvent::Error {
                    source: "mpris",
                    msg: "x".into(),
                },
                vec![],
            ),
        ];
        for (ev, kinds) in rows {
            let got: Vec<Kind> = feeds(&ev, None).into_iter().map(|(k, _)| k).collect();
            assert_eq!(got, kinds, "{ev:?}");
        }
        // the fields
        let Data::Media(m) = &feeds(&SysEvent::Media(media_state(true)), None)[0].1 else {
            panic!()
        };
        assert!(m.playing && m.has_art);
        assert_eq!(
            m.volume, 0.4,
            "the player's volume until the sink's is known"
        );
        assert_eq!(m.position_s, Some(1.5));
        let Data::Media(m) = &feeds(&SysEvent::Media(media_state(false)), Some(0.7))[0].1 else {
            panic!()
        };
        assert_eq!(m.volume, 0.7, "the knob is the sink");
        let n = net(&NetState {
            connected: true,
            wifi: true,
            strength: 9,
        });
        assert_eq!(n.strength, 4, "bars clamp");
        let Data::Load(l) = &feeds(
            &SysEvent::Pressure {
                cpu: true,
                mem: false,
            },
            None,
        )[1]
        .1
        else {
            panic!()
        };
        assert!(!l.pressure, "the aquarium follows memory");
    }

    #[test]
    fn readouts() {
        let clock = Data::Clock(Clock {
            unix_s: 0,
            hour: 9,
            minute: 5,
            band: 2,
        });
        let pw = |pct, charging, present| {
            Data::Power(Power {
                pct,
                charging,
                time_to_empty_s: None,
                present,
            })
        };
        let nt = |connected, wifi, strength| {
            Data::Net(Net {
                connected,
                wifi,
                strength,
            })
        };
        let md = |title: &str, artist: &str| {
            Data::Media(Media {
                playing: true,
                title: title.into(),
                artist: artist.into(),
                has_art: false,
                volume: 0.5,
                position_s: None,
                length_s: None,
            })
        };
        let rows = [
            (Kind::WallClock, Some(clock), Some("09:05")),
            (Kind::Battery, Some(pw(73, false, true)), Some("73 %")),
            (Kind::Battery, Some(pw(40, true, true)), Some("40 % ⚡")),
            (
                Kind::Battery,
                Some(pw(0, false, false)),
                Some("sem bateria"),
            ),
            (
                Kind::Turntable,
                Some(md("Lo-fi for cats", "acme radio")),
                Some("Lo-fi for cats — acme radio"),
            ),
            (Kind::Turntable, Some(md("Lo-fi", "")), Some("Lo-fi")),
            (Kind::Turntable, Some(md("", "")), Some("sem título")),
            (Kind::NetPlant, Some(nt(true, true, 3)), Some("3 barras")),
            (Kind::NetPlant, Some(nt(true, true, 1)), Some("1 barra")),
            (Kind::NetPlant, Some(nt(true, false, 4)), Some("cabo")),
            (Kind::NetPlant, Some(nt(false, true, 0)), Some("sem rede")),
            (Kind::Lamp, Some(Data::Brightness(0.8)), Some("80 %")),
            (Kind::WallClock, None, None),
            (Kind::Fireplace, Some(Data::Brightness(1.0)), None),
        ];
        for (k, d, want) in rows {
            assert_eq!(readout(k, d.as_ref()).as_deref(), want, "{k:?}");
        }
    }

    #[test]
    fn badges_map_one_to_one() {
        assert_eq!(badge(hud::Badge::BatteryLow(12)), Badge::BatteryLow(12));
        assert_eq!(badge(hud::Badge::NetDown), Badge::NetDown);
        assert_eq!(badge(hud::Badge::OnAir), Badge::OnAir);
        assert_eq!(badge(hud::Badge::VaultOpen), Badge::VaultOpen);
        assert_eq!(badge(hud::Badge::Updates(2)), Badge::Updates(2));
    }

    #[test]
    fn goto_names() {
        assert!(matches_name(Kind::Turntable, "Vitrola"));
        assert!(matches_name(Kind::Turntable, " turntable "));
        assert!(matches_name(Kind::WallClock, "relógio"));
        assert!(!matches_name(Kind::WallClock, ""));
        assert!(!matches_name(Kind::Battery, "vitrola"));
    }

    #[test]
    fn palette_has_media_volume_and_goto() {
        let p = palette();
        let names: Vec<&str> = p.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "media.play_pause",
                "media.next",
                "media.prev",
                "volume.up",
                "volume.down",
                "volume.mute",
                "world.goto_object",
                "world.open",
                "world.library_search",
                "world.vault_lock"
            ]
        );
        let open = p.iter().find(|i| i.name == "world.open").unwrap();
        assert_eq!(open.args[0].name, "path");
        let search = p.iter().find(|i| i.name == "world.library_search").unwrap();
        assert_eq!(
            (search.args[0].name.as_str(), search.keys[0].as_str()),
            ("q", "/")
        );
        assert!(
            p.iter()
                .all(|i| !i.title.is_empty() && i.source == Source::Object)
        );
        assert_eq!(p[6].args.len(), 1);
        assert_eq!(p[6].args[0].name, "name");
        assert_eq!(p[0].keys, ["XF86AudioPlay"]);
    }

    #[test]
    fn six_animated_objects_get_four_rates_nearest_first() {
        let needs: Vec<Need> = (0..6)
            .map(|i| Need {
                id: ObjectId(i),
                want: Want::Rate(12.0),
                dist: 10.0 - i as f32,
            })
            .chain([Need {
                id: ObjectId(99),
                want: Want::Continuous,
                dist: 50.0,
            }])
            .collect();
        let kept = cap_rates(needs, MAX_RATES);
        let rates: Vec<u32> = kept
            .iter()
            .filter(|n| matches!(n.want, Want::Rate(_)))
            .map(|n| n.id.0)
            .collect();
        assert_eq!(rates, [5, 4, 3, 2], "the four nearest");
        assert!(kept.iter().any(|n| n.id == ObjectId(99)), "non-rates pass");
        assert!(cap_rates(Vec::new(), MAX_RATES).is_empty());
    }

    #[test]
    fn sys_status_table() {
        let none = BTreeMap::new();
        let off = sys_status(false, &none);
        assert_eq!(off["mpris"], "off");
        assert_eq!(sys_status(true, &none)["pipewire"], "ok");
        let errs = BTreeMap::from([("system-bus".to_string(), "x".to_string())]);
        let s = sys_status(true, &errs);
        assert_eq!(s["upower"], "error");
        assert_eq!(s["nm"], "error");
        assert_eq!(s["mpris"], "ok");
    }
}
