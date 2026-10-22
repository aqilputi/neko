//! NetworkManager: global `State`, the primary connection's kind and, on Wi-Fi, the active access
//! point's `Strength`. The SSID is never read: it is a personal identifier and stays out of events.

use futures_util::StreamExt;

use super::{Props, decode_changed, get, get_all, get_prop, props_stream, v_i64, v_str, v_strs};
use crate::{NetState, Out, SysEvent};

pub(crate) const SERVICE: &str = "org.freedesktop.NetworkManager";
pub(crate) const PATH: &str = "/org/freedesktop/NetworkManager";
pub(crate) const IFACE: &str = "org.freedesktop.NetworkManager";
const ACTIVE_IFACE: &str = "org.freedesktop.NetworkManager.Connection.Active";
const WIFI_IFACE: &str = "org.freedesktop.NetworkManager.Device.Wireless";
const AP_IFACE: &str = "org.freedesktop.NetworkManager.AccessPoint";
const WIFI_TYPE: &str = "802-11-wireless";

/// `NM_STATE_CONNECTED_SITE`: a default route exists (70 = global connectivity checked).
const CONNECTED_SITE: u32 = 60;

/// Signal bars 0..=4 from NM's 0..=100 strength (nm-applet's thresholds).
pub fn bars_of(strength: u8) -> u8 {
    match strength {
        81.. => 4,
        56..=80 => 3,
        31..=55 => 2,
        6..=30 => 1,
        _ => 0,
    }
}

/// A wired link counts as full bars; no connection is zero.
pub fn net_from(state: u32, conn_type: &str, ap_strength: Option<u8>) -> NetState {
    let connected = state >= CONNECTED_SITE;
    let wifi = connected && conn_type == WIFI_TYPE;
    let strength = match (connected, wifi) {
        (false, _) => 0,
        (true, true) => bars_of(ap_strength.unwrap_or(0)),
        (true, false) => 4,
    };
    NetState {
        connected,
        wifi,
        strength,
    }
}

#[derive(Default)]
struct Chain {
    device: Option<String>,
    ap: Option<String>,
    strength: Option<u8>,
}

fn not_root(p: Option<&str>) -> Option<String> {
    p.filter(|p| *p != "/").map(str::to_string)
}

async fn resolve_ap(conn: &zbus::Connection, device: &str) -> (Option<String>, Option<u8>) {
    let Ok(v) = get_prop(conn, SERVICE, device, WIFI_IFACE, "ActiveAccessPoint").await else {
        return (None, None);
    };
    let Some(ap) = not_root(v_str(&v)) else {
        return (None, None);
    };
    let strength = get_prop(conn, SERVICE, &ap, AP_IFACE, "Strength")
        .await
        .ok()
        .and_then(|s| v_i64(&s))
        .map(|s| s.clamp(0, 100) as u8);
    (Some(ap), strength)
}

async fn resolve(conn: &zbus::Connection, root: &Props) -> Chain {
    let kind = get(root, "PrimaryConnectionType")
        .and_then(v_str)
        .unwrap_or("");
    let Some(primary) = not_root(get(root, "PrimaryConnection").and_then(v_str)) else {
        return Chain::default();
    };
    if kind != WIFI_TYPE {
        return Chain::default();
    }
    let Ok(devs) = get_prop(conn, SERVICE, &primary, ACTIVE_IFACE, "Devices").await else {
        return Chain::default();
    };
    let Some(device) = v_strs(&devs).into_iter().next() else {
        return Chain::default();
    };
    let (ap, strength) = resolve_ap(conn, &device).await;
    Chain {
        device: Some(device),
        ap,
        strength,
    }
}

pub(crate) async fn run(conn: &zbus::Connection, out: &Out) -> zbus::Result<()> {
    // One subscription for every NM object; we keep only the three paths we follow.
    let mut changes = props_stream(conn, Some(SERVICE), None, None).await?;
    let mut root = get_all(conn, SERVICE, PATH, IFACE).await?;
    let mut chain = resolve(conn, &root).await;
    let mut last = net_from_root(&root, chain.strength);
    out.send(SysEvent::Net(last));
    while let Some(msg) = changes.next().await {
        let Ok(msg) = msg else { continue };
        let path = msg.header().path().map(|p| p.as_str().to_string());
        let Some((iface, changed, _)) = decode_changed(&msg) else {
            continue;
        };
        let path = path.as_deref();
        if path == Some(PATH) && iface == IFACE {
            let rewire = changed.contains_key("PrimaryConnection")
                || changed.contains_key("PrimaryConnectionType");
            root.extend(changed);
            if rewire {
                chain = resolve(conn, &root).await;
            }
        } else if path.is_some() && path == chain.device.as_deref() && iface == WIFI_IFACE {
            if changed.contains_key("ActiveAccessPoint")
                && let Some(dev) = chain.device.clone()
            {
                (chain.ap, chain.strength) = resolve_ap(conn, &dev).await;
            }
        } else if path.is_some() && path == chain.ap.as_deref() && iface == AP_IFACE {
            if let Some(s) = get(&changed, "Strength").and_then(v_i64) {
                chain.strength = Some(s.clamp(0, 100) as u8);
            }
        } else {
            continue;
        }
        let now = net_from_root(&root, chain.strength);
        if now != last {
            last = now;
            out.send(SysEvent::Net(now));
        }
    }
    Ok(())
}

fn net_from_root(root: &Props, strength: Option<u8>) -> NetState {
    let state = get(root, "State")
        .and_then(v_i64)
        .unwrap_or(0)
        .clamp(0, i64::from(u32::MAX)) as u32;
    let kind = get(root, "PrimaryConnectionType")
        .and_then(v_str)
        .unwrap_or("");
    net_from(state, kind, strength)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn net_table() {
        let cases: &[(u32, &str, Option<u8>, NetState)] = &[
            (
                70,
                "802-11-wireless",
                Some(90),
                NetState {
                    connected: true,
                    wifi: true,
                    strength: 4,
                },
            ),
            (
                70,
                "802-11-wireless",
                Some(60),
                NetState {
                    connected: true,
                    wifi: true,
                    strength: 3,
                },
            ),
            (
                60,
                "802-11-wireless",
                Some(40),
                NetState {
                    connected: true,
                    wifi: true,
                    strength: 2,
                },
            ),
            (
                70,
                "802-11-wireless",
                None,
                NetState {
                    connected: true,
                    wifi: true,
                    strength: 0,
                },
            ),
            (
                70,
                "802-3-ethernet",
                None,
                NetState {
                    connected: true,
                    wifi: false,
                    strength: 4,
                },
            ),
            (
                50,
                "802-3-ethernet",
                None,
                NetState {
                    connected: false,
                    wifi: false,
                    strength: 0,
                },
            ),
            (
                40,
                "802-11-wireless",
                Some(90),
                NetState {
                    connected: false,
                    wifi: false,
                    strength: 0,
                },
            ),
            (20, "", None, NetState::default()),
            (0, "", None, NetState::default()),
        ];
        for &(state, kind, s, want) in cases {
            assert_eq!(net_from(state, kind, s), want, "{state} {kind} {s:?}");
        }
    }

    #[test]
    fn bars_table() {
        for (s, b) in [
            (0, 0),
            (5, 0),
            (6, 1),
            (30, 1),
            (31, 2),
            (55, 2),
            (56, 3),
            (80, 3),
            (81, 4),
            (100, 4),
            (255, 4),
        ] {
            assert_eq!(bars_of(s), b, "{s}");
        }
    }
}
