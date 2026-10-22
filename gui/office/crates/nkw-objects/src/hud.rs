//! The HUD fallback (addendum §2.3): badges, never pop-ups, only for what
//! is critical; visible even in FOCO. Everything else lives on objects.

use crate::object::{BATTERY_LOW, Data, World};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Badge {
    BatteryLow(u8),
    NetDown,
    OnAir,
    VaultOpen,
    Updates(u32),
}

/// The badges to show, in this fixed order: battery, network, on air,
/// vault, updates.
pub fn badges(world: &World, vault_unlocked: bool) -> Vec<Badge> {
    let mut battery = None;
    let mut net_down = false;
    let mut on_air = false;
    for d in world.data.values() {
        match d {
            Data::Power(p) if p.present && !p.charging && p.pct < BATTERY_LOW => {
                battery = Some(battery.map_or(p.pct, |b: u8| b.min(p.pct)));
            }
            Data::Net(n) if !n.connected && world.net_seen() => net_down = true,
            Data::OnAir(true) => on_air = true,
            _ => {}
        }
    }
    let mut out = Vec::new();
    if let Some(pct) = battery {
        out.push(Badge::BatteryLow(pct));
    }
    if net_down {
        out.push(Badge::NetDown);
    }
    if on_air {
        out.push(Badge::OnAir);
    }
    if vault_unlocked {
        out.push(Badge::VaultOpen);
    }
    let n = world.unseen_updates();
    if n > 0 {
        out.push(Badge::Updates(n));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::ObjectId;
    use crate::object::{Action, Net, Power, Updates, demo_world};

    const BAT: ObjectId = ObjectId(3);
    const NET: ObjectId = ObjectId(4);
    const BOX: ObjectId = ObjectId(7);
    const ONAIR: ObjectId = ObjectId(9);

    fn calm() -> World {
        let mut w = demo_world();
        w.set(
            BOX,
            Data::Updates(Updates {
                count: 0,
                checked_at: None,
            }),
        );
        w
    }

    fn power(pct: u8, charging: bool) -> Data {
        Data::Power(Power {
            pct,
            charging,
            time_to_empty_s: None,
            present: true,
        })
    }

    #[test]
    fn calm_demo_shows_nothing() {
        assert!(badges(&calm(), false).is_empty());
        assert_eq!(badges(&demo_world(), false), [Badge::Updates(3)]);
    }

    #[test]
    fn battery_low_only_below_15_and_discharging() {
        let mut w = calm();
        for (pct, charging, shown) in [
            (15, false, false),
            (14, false, true),
            (3, false, true),
            (5, true, false),
        ] {
            w.set(BAT, power(pct, charging));
            let b = badges(&w, false);
            assert_eq!(
                b.contains(&Badge::BatteryLow(pct)),
                shown,
                "{pct}% charging={charging}"
            );
        }
        w.set(
            BAT,
            Data::Power(Power {
                pct: 0,
                charging: false,
                time_to_empty_s: None,
                present: false,
            }),
        );
        assert!(badges(&w, false).is_empty(), "no battery, no badge");
    }

    #[test]
    fn net_down_only_after_being_up_once() {
        // a desktop that never had a network: no badge
        let mut w = calm();
        w.data.clear();
        w = {
            let mut fresh = crate::object::World::new(w.objects.clone());
            fresh.set(
                NET,
                Data::Net(Net {
                    connected: false,
                    wifi: false,
                    strength: 0,
                }),
            );
            fresh
        };
        assert!(badges(&w, false).is_empty());
        w.set(
            NET,
            Data::Net(Net {
                connected: true,
                wifi: true,
                strength: 2,
            }),
        );
        assert!(badges(&w, false).is_empty());
        w.set(
            NET,
            Data::Net(Net {
                connected: false,
                wifi: true,
                strength: 0,
            }),
        );
        assert_eq!(badges(&w, false), [Badge::NetDown]);
    }

    #[test]
    fn on_air_while_active_and_vault_while_open() {
        let mut w = calm();
        w.set(ONAIR, Data::OnAir(true));
        assert_eq!(badges(&w, false), [Badge::OnAir]);
        assert_eq!(badges(&w, true), [Badge::OnAir, Badge::VaultOpen]);
        w.set(ONAIR, Data::OnAir(false));
        assert_eq!(badges(&w, true), [Badge::VaultOpen]);
    }

    #[test]
    fn updates_badge_goes_once_the_box_is_read() {
        let mut w = demo_world();
        assert_eq!(badges(&w, false), [Badge::Updates(3)]);
        w.apply(BOX, Action::Interact, 0.0);
        assert!(badges(&w, false).is_empty());
    }

    #[test]
    fn order_is_stable() {
        let mut w = demo_world();
        w.set(BAT, power(9, false));
        w.set(
            NET,
            Data::Net(Net {
                connected: false,
                wifi: true,
                strength: 0,
            }),
        );
        w.set(ONAIR, Data::OnAir(true));
        let want = [
            Badge::BatteryLow(9),
            Badge::NetDown,
            Badge::OnAir,
            Badge::VaultOpen,
            Badge::Updates(3),
        ];
        for _ in 0..3 {
            assert_eq!(badges(&w, true), want);
        }
    }
}
