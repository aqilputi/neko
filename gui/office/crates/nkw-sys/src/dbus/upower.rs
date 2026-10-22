//! UPower `DisplayDevice` (`org.freedesktop.UPower.Device`): the battery the desktop shows, already
//! merged across batteries by UPower.

use futures_util::StreamExt;

use super::{Props, decode_changed, get, get_all, props_stream, v_bool, v_f64, v_i64};
use crate::{Out, PowerState, SysEvent};

pub(crate) const SERVICE: &str = "org.freedesktop.UPower";
pub(crate) const PATH: &str = "/org/freedesktop/UPower/devices/DisplayDevice";
pub(crate) const IFACE: &str = "org.freedesktop.UPower.Device";

// UPower `State`
const CHARGING: i64 = 1;
const DISCHARGING: i64 = 2;
const FULLY_CHARGED: i64 = 4;
const PENDING_CHARGE: i64 = 5;
const PENDING_DISCHARGE: i64 = 6;

/// `charging` means "on mains" (charging, full, or waiting to charge). Missing keys read as absent.
pub fn power_from(p: &Props) -> PowerState {
    let state = get(p, "State").and_then(v_i64).unwrap_or(0);
    let pct = get(p, "Percentage").and_then(v_f64).unwrap_or(0.0);
    let tte = get(p, "TimeToEmpty").and_then(v_i64).unwrap_or(0);
    let discharging = state == DISCHARGING || state == PENDING_DISCHARGE;
    PowerState {
        present: get(p, "IsPresent").and_then(v_bool).unwrap_or(false),
        pct: pct.round().clamp(0.0, 100.0) as u8,
        charging: matches!(state, CHARGING | FULLY_CHARGED | PENDING_CHARGE),
        time_to_empty_s: (discharging && tte > 0).then(|| tte.min(i64::from(u32::MAX)) as u32),
    }
}

pub(crate) async fn run(conn: &zbus::Connection, out: &Out) -> zbus::Result<()> {
    let mut changes = props_stream(conn, Some(SERVICE), Some(PATH), Some(IFACE)).await?;
    let mut cache = get_all(conn, SERVICE, PATH, IFACE).await?;
    let mut last = power_from(&cache);
    out.send(SysEvent::Power(last));
    while let Some(msg) = changes.next().await {
        let Ok(msg) = msg else { continue };
        let Some((_, changed, invalidated)) = decode_changed(&msg) else {
            continue;
        };
        if invalidated.is_empty() {
            cache.extend(changed);
        } else if let Ok(all) = get_all(conn, SERVICE, PATH, IFACE).await {
            cache = all;
        }
        let now = power_from(&cache);
        if now != last {
            last = now;
            out.send(SysEvent::Power(now));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dbus::props;
    use zbus::zvariant::Value;

    #[test]
    fn power_table() {
        let cases: Vec<(Props, PowerState)> = vec![
            (
                props(vec![
                    ("IsPresent", Value::Bool(true)),
                    ("Percentage", Value::F64(72.6)),
                    ("State", Value::U32(2)),
                    ("TimeToEmpty", Value::I64(7200)),
                ]),
                PowerState {
                    present: true,
                    pct: 73,
                    charging: false,
                    time_to_empty_s: Some(7200),
                },
            ),
            (
                props(vec![
                    ("IsPresent", Value::Bool(true)),
                    ("Percentage", Value::F64(40.0)),
                    ("State", Value::U32(1)),
                    ("TimeToEmpty", Value::I64(0)),
                ]),
                PowerState {
                    present: true,
                    pct: 40,
                    charging: true,
                    time_to_empty_s: None,
                },
            ),
            (
                props(vec![
                    ("IsPresent", Value::Bool(true)),
                    ("Percentage", Value::F64(100.0)),
                    ("State", Value::U32(4)),
                ]),
                PowerState {
                    present: true,
                    pct: 100,
                    charging: true,
                    time_to_empty_s: None,
                },
            ),
            (
                // charging with a stale TimeToEmpty: never reported
                props(vec![
                    ("State", Value::U32(1)),
                    ("TimeToEmpty", Value::I64(99)),
                ]),
                PowerState {
                    present: false,
                    pct: 0,
                    charging: true,
                    time_to_empty_s: None,
                },
            ),
            (props(vec![]), PowerState::default()),
            (
                props(vec![
                    ("Percentage", Value::F64(140.0)),
                    ("IsPresent", Value::Bool(false)),
                ]),
                PowerState {
                    present: false,
                    pct: 100,
                    charging: false,
                    time_to_empty_s: None,
                },
            ),
        ];
        for (i, (p, want)) in cases.iter().enumerate() {
            assert_eq!(power_from(p), *want, "case {i}");
        }
    }
}
