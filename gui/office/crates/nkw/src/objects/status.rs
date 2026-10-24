//! What the objects tell the status strip and the phone's header (S20/S25):
//! the clock and its band, the battery, the network, whether a player plays
//! and whether the vault is open. Pure: from `nkw_objects::object::World`'s
//! data; never a media title, an SSID or a device name (the `Status` type
//! has no field for them).

use nkw_objects::object::{self, Data};
use serde_json::{Map, Value};

use crate::ext::Status;

/// `--demo`'s fixture (S25): 14:05, band 4, battery 42 % discharging, net up
/// with 3 bars, nothing playing.
pub fn demo() -> Status {
    Status {
        clock: Some((14, 5)),
        band: Some(4),
        battery: Some((42, false)),
        net: Some((true, 3)),
        playing: Some(false),
        ..Status::default()
    }
}

/// The objects' fields of `Status` from the latest data.
pub fn from_world(w: &object::World, vault_open: bool) -> Status {
    let mut s = Status {
        vault_open,
        ..Status::default()
    };
    for d in w.data.values() {
        match d {
            Data::Clock(c) if s.clock.is_none() => {
                s.clock = Some((c.hour.min(23), c.minute.min(59)));
                s.band = Some(c.band.min(7));
            }
            Data::Power(p) if s.battery.is_none() && p.present => {
                s.battery = Some((p.pct.min(100), p.charging));
            }
            Data::Net(n) if s.net.is_none() => s.net = Some((n.connected, n.strength.min(4))),
            Data::Media(m) if s.playing.is_none() => s.playing = Some(m.playing),
            _ => {}
        }
    }
    s
}

fn pair<A, B>(
    v: &Value,
    a: impl Fn(&Value) -> Option<A>,
    b: impl Fn(&Value) -> Option<B>,
) -> Option<(A, B)> {
    let arr = v.as_array()?;
    match arr.as_slice() {
        [x, y] => Some((a(x)?, b(y)?)),
        _ => None,
    }
}

fn byte(v: &Value) -> Option<u8> {
    v.as_u64().and_then(|n| u8::try_from(n).ok())
}

/// `inject objects {"status": {...}}` over `s`: each key given replaces that
/// field (`null` clears it); unknown keys or bad values are an error (pt-BR).
/// Keys: `clock [h, m]`, `band n`, `battery [pct, charging]`, `net
/// [connected, bars]`, `playing bool`, `vault_open bool`.
pub fn apply(s: &mut Status, over: &Map<String, Value>) -> Result<(), String> {
    let bad = |k: &str| format!("status: valor ruim em {k:?}");
    for (k, v) in over {
        let null = v.is_null();
        match k.as_str() {
            "clock" => {
                s.clock = if null {
                    None
                } else {
                    Some(pair(v, byte, byte).ok_or_else(|| bad(k))?)
                }
            }
            "band" => {
                s.band = if null {
                    None
                } else {
                    Some(byte(v).ok_or_else(|| bad(k))?)
                }
            }
            "battery" => {
                s.battery = if null {
                    None
                } else {
                    Some(pair(v, byte, Value::as_bool).ok_or_else(|| bad(k))?)
                }
            }
            "net" => {
                s.net = if null {
                    None
                } else {
                    Some(pair(v, Value::as_bool, byte).ok_or_else(|| bad(k))?)
                }
            }
            "playing" => {
                s.playing = if null {
                    None
                } else {
                    Some(v.as_bool().ok_or_else(|| bad(k))?)
                }
            }
            "vault_open" => s.vault_open = v.as_bool().ok_or_else(|| bad(k))?,
            other => return Err(format!("status: campo desconhecido {other:?}")),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use nkw_objects::ObjectId;
    use nkw_objects::object::{Clock, Media, Net, Power};

    fn world(data: Vec<Data>) -> object::World {
        let mut w = object::World::default();
        for (i, d) in data.into_iter().enumerate() {
            w.data.insert(ObjectId(i as u32 + 1), d);
        }
        w
    }

    fn media(playing: bool) -> Data {
        Data::Media(Media {
            playing,
            title: "Lo-fi for cats".into(),
            artist: "acme radio".into(),
            has_art: false,
            volume: 0.5,
            position_s: None,
            length_s: None,
        })
    }

    #[test]
    fn status_from_data_table() {
        let clock = Data::Clock(Clock {
            unix_s: 0,
            hour: 9,
            minute: 41,
            band: 3,
        });
        let power = |present, pct, charging| {
            Data::Power(Power {
                pct,
                charging,
                time_to_empty_s: None,
                present,
            })
        };
        let net = |connected, strength| {
            Data::Net(Net {
                connected,
                wifi: true,
                strength,
            })
        };
        let cases: Vec<(Vec<Data>, bool, Status)> = vec![
            (vec![], false, Status::default()),
            (
                vec![],
                true,
                Status {
                    vault_open: true,
                    ..Status::default()
                },
            ),
            (
                vec![clock.clone()],
                false,
                Status {
                    clock: Some((9, 41)),
                    band: Some(3),
                    ..Status::default()
                },
            ),
            (vec![power(false, 0, false)], false, Status::default()),
            (
                vec![power(true, 12, true)],
                false,
                Status {
                    battery: Some((12, true)),
                    ..Status::default()
                },
            ),
            (
                vec![net(false, 0)],
                false,
                Status {
                    net: Some((false, 0)),
                    ..Status::default()
                },
            ),
            (
                vec![net(true, 9)],
                false,
                Status {
                    net: Some((true, 4)),
                    ..Status::default()
                },
            ),
            (
                vec![media(true), clock, power(true, 80, false), net(true, 2)],
                true,
                Status {
                    clock: Some((9, 41)),
                    band: Some(3),
                    battery: Some((80, false)),
                    net: Some((true, 2)),
                    playing: Some(true),
                    vault_open: true,
                    ..Status::default()
                },
            ),
            (
                vec![media(false)],
                false,
                Status {
                    playing: Some(false),
                    ..Status::default()
                },
            ),
        ];
        for (i, (data, open, want)) in cases.into_iter().enumerate() {
            assert_eq!(from_world(&world(data), open), want, "case {i}");
        }
    }

    #[test]
    fn the_status_never_carries_a_title() {
        let s = from_world(&world(vec![media(true)]), false);
        let j = serde_json::to_string(&s).unwrap();
        assert!(!j.contains("Lo-fi") && !j.contains("acme"), "{j}");
    }

    #[test]
    fn the_demo_fixture_is_the_contract_s() {
        let d = demo();
        assert_eq!(d.clock, Some((14, 5)));
        assert_eq!(d.band, Some(4));
        assert_eq!(d.battery, Some((42, false)));
        assert_eq!(d.net, Some((true, 3)));
        assert_eq!(d.playing, Some(false));
        assert!(!d.vault_open);
        assert_eq!(d.inbox, None, "the cats' fields are the cats'");
    }

    #[test]
    fn overrides_replace_clear_and_refuse() {
        let mut s = demo();
        let m = |j: Value| j.as_object().unwrap().clone();
        apply(
            &mut s,
            &m(serde_json::json!({"battery": [12, false], "net": null, "playing": true, "clock": [8, 0], "band": 2, "vault_open": true})),
        )
        .unwrap();
        assert_eq!(s.battery, Some((12, false)));
        assert_eq!(s.net, None);
        assert_eq!(s.playing, Some(true));
        assert_eq!(s.clock, Some((8, 0)));
        assert_eq!(s.band, Some(2));
        assert!(s.vault_open);
        let before = s.clone();
        assert!(apply(&mut s, &m(serde_json::json!({"ssid": "x"}))).is_err());
        assert!(apply(&mut s, &m(serde_json::json!({"battery": [300, false]}))).is_err());
        assert!(apply(&mut s, &m(serde_json::json!({"clock": [1]}))).is_err());
        assert_eq!(s, before);
    }
}
