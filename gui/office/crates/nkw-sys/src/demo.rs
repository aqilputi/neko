//! `--demo` values: the only system data that may appear in screenshots and docs. Never reads the
//! machine (except the clock, which is not personal).

use crate::{MediaState, NetState, PowerState, SysEvent};

pub fn fixture() -> Vec<SysEvent> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    vec![
        SysEvent::Power(PowerState {
            present: true,
            pct: 73,
            charging: false,
            time_to_empty_s: Some(3 * 3600 + 20 * 60),
        }),
        SysEvent::Net(NetState {
            connected: true,
            wifi: true,
            strength: 3,
        }),
        SysEvent::Media(MediaState {
            playing: true,
            title: "Lo-fi for cats".into(),
            artist: "acme radio".into(),
            art_url: None,
            volume: 0.6,
            position_us: Some(42_000_000),
            length_us: Some(180_000_000),
            player: "demo".into(),
        }),
        SysEvent::Clock(crate::clock::tick_at(now)),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(v: &serde_json::Value, out: &mut Vec<String>) {
        match v {
            serde_json::Value::Object(m) => {
                for (k, v) in m {
                    out.push(k.to_lowercase());
                    keys(v, out);
                }
            }
            serde_json::Value::Array(a) => a.iter().for_each(|v| keys(v, out)),
            _ => {}
        }
    }

    #[test]
    fn fixture_matches_the_addendum() {
        let f = fixture();
        assert!(
            f.iter()
                .any(|e| matches!(e, SysEvent::Power(p) if p.pct == 73 && !p.charging))
        );
        assert!(
            f.iter()
                .any(|e| matches!(e, SysEvent::Net(n) if n.wifi && n.strength == 3))
        );
        assert!(f.iter().any(|e| matches!(e, SysEvent::Media(m) if m.title == "Lo-fi for cats" && m.artist == "acme radio")));
        assert!(f.iter().any(|e| matches!(e, SysEvent::Clock(_))));
    }

    #[test]
    fn fixture_has_no_personal_fields() {
        let mut all = Vec::new();
        keys(&serde_json::to_value(fixture()).unwrap(), &mut all);
        for bad in [
            "ssid", "device", "hostname", "mac", "address", "bssid", "name",
        ] {
            assert!(
                !all.iter().any(|k| k.contains(bad)),
                "field {bad:?} in {all:?}"
            );
        }
    }
}
