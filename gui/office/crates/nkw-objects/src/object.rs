//! Widgets as furniture (addendum §2): the objects, the data the adapters
//! feed them, and what interacting with them does.
//!
//! The data models never carry personal identifiers (SSIDs, device names):
//! those stay in `nkw`'s memory, read for a 2D label at the moment.

use std::collections::BTreeMap;

use glam::{Vec2, Vec3};
use serde::{Deserialize, Serialize};

use crate::Time;
use crate::ids::{DoorId, ObjectId};
use crate::proximity::Tier;
use crate::sound::Cue;
use nkw_core::{Aabb, Pose, RoomId, Want};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Kind {
    Turntable,
    WallClock,
    Battery,
    NetPlant,
    Fireplace,
    Aquarium,
    PackageBox,
    TrayShelf,
    OnAir,
    KeyboardFlag,
    Calendar,
    BtRadio,
    Lamp,
    Vault,
    Door(DoorId),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Object {
    pub id: ObjectId,
    pub kind: Kind,
    pub room: RoomId,
    pub pos: Vec3,
    pub yaw: f32,
    /// where the camera goes to use it (SALTO, `Pick::Object`)
    pub stand: Pose,
    /// floor box for collision
    pub aabb: Aabb,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Media {
    pub playing: bool,
    pub title: String,
    pub artist: String,
    pub has_art: bool,
    /// 0..1
    pub volume: f32,
    pub position_s: Option<f32>,
    pub length_s: Option<f32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Power {
    pub pct: u8,
    pub charging: bool,
    pub time_to_empty_s: Option<u32>,
    pub present: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Net {
    pub connected: bool,
    pub wifi: bool,
    /// 0..4 bars
    pub strength: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Load {
    pub cpu_pct: Option<u8>,
    pub mem_pct: Option<u8>,
    /// PSI crossed its threshold
    pub pressure: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Updates {
    pub count: u32,
    pub checked_at: Option<i64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Clock {
    pub unix_s: i64,
    pub hour: u8,
    pub minute: u8,
    /// the band of the day, 0..8
    pub band: u8,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tray {
    pub items: Vec<TrayItem>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrayItem {
    pub id: u32,
    pub title: String,
    pub has_menu: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bt {
    pub powered: bool,
    pub connected: u8,
    pub battery_pct: Option<u8>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Data {
    Media(Media),
    Power(Power),
    Net(Net),
    Load(Load),
    Updates(Updates),
    Clock(Clock),
    Tray(Tray),
    Bt(Bt),
    Brightness(f32),
    OnAir(bool),
    Layout(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// `enter`/`space`
    Interact,
    /// `j`/`k`: -1/+1
    Adjust(i8),
    /// `h`
    Prev,
    /// `l`
    Next,
    Open,
    Close,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaCmd {
    PlayPause,
    Next,
    Prev,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListKind {
    Tray,
    Updates,
    Calendar,
    Bt,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Anim {
    /// degrees, ±18 per step
    KnobTurn(f32),
    NeedleLift,
    NeedleDrop,
    Spin(bool),
    Jiggle,
    LidOpen,
    LidClose,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Effect {
    Media(MediaCmd),
    SetVolume(f32),
    SetBrightness(f32),
    OpenList(ListKind),
    ToggleDoor(DoorId),
    VaultPrompt,
    Cue(Cue),
    Anim(ObjectId, Anim),
}

/// One `j`/`k` step of a knob or dimmer.
pub const STEP: f32 = 0.05;
/// The knob turns this much per step, degrees.
pub const KNOB_DEG: f32 = 18.0;
/// The dimmer never goes fully dark.
pub const MIN_BRIGHTNESS: f32 = 0.05;
/// The battery cat wakes below this, percent…
pub const BATTERY_LOW: u8 = 15;
/// …and goes back to sleep only this far above it (no meow ping-pong).
pub const BATTERY_HYSTERESIS: u8 = 3;

/// The objects and their latest data.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct World {
    pub objects: Vec<Object>,
    pub data: BTreeMap<ObjectId, Data>,
    battery_warned: bool,
    net_seen: bool,
    updates_seen: u32,
}

/// Snaps to the step grid, moves `d` steps, clamps. Divides whole steps by
/// 20 so the results are exactly the decimal literals (0.65, not 0.6500001).
fn stepped(v: f32, d: i8, lo: f32, hi: f32) -> f32 {
    let per_unit = (1.0 / STEP).round();
    let n = (v * per_unit).round() + d as f32;
    (n / per_unit).clamp(lo, hi)
}

impl World {
    pub fn new(objects: Vec<Object>) -> World {
        World {
            objects,
            ..World::default()
        }
    }

    pub fn object(&self, id: ObjectId) -> Option<&Object> {
        self.objects.iter().find(|o| o.id == id)
    }

    /// The first object of a kind.
    pub fn find(&self, kind: Kind) -> Option<&Object> {
        self.objects.iter().find(|o| o.kind == kind)
    }

    /// Whether the network has been up at least once this session (the
    /// "network down" badge needs it: a desktop without Wi-Fi shows none).
    pub fn net_seen(&self) -> bool {
        self.net_seen
    }

    /// Updates not looked at yet (the badge goes once you read the box).
    pub fn unseen_updates(&self) -> u32 {
        self.data
            .values()
            .find_map(|d| match d {
                Data::Updates(u) => Some(u.count),
                _ => None,
            })
            .map_or(0, |c| c.saturating_sub(self.updates_seen))
    }

    /// What `a` on object `id` does. Objects never change the mode, open a
    /// window or move the camera: they only ask (§3.9).
    pub fn apply(&mut self, id: ObjectId, a: Action, _now: Time) -> Vec<Effect> {
        let Some(kind) = self.object(id).map(|o| o.kind) else {
            return Vec::new();
        };
        let click = Effect::Cue(Cue::Click);
        match (kind, a) {
            (Kind::Turntable, Action::Interact) => match self.data.get(&id) {
                Some(Data::Media(m)) => {
                    let needle = if m.playing {
                        Anim::NeedleLift
                    } else {
                        Anim::NeedleDrop
                    };
                    vec![
                        Effect::Media(MediaCmd::PlayPause),
                        Effect::Anim(id, needle),
                        click,
                    ]
                }
                // no player: the lid stays shut
                _ => vec![Effect::Anim(id, Anim::Jiggle)],
            },
            (Kind::Turntable, Action::Adjust(d)) => {
                let Some(Data::Media(m)) = self.data.get_mut(&id) else {
                    return vec![Effect::Anim(id, Anim::Jiggle)];
                };
                let v = stepped(m.volume, d.signum(), 0.0, 1.0);
                if v == m.volume {
                    return vec![Effect::Anim(id, Anim::Jiggle)];
                }
                m.volume = v;
                vec![
                    Effect::SetVolume(v),
                    Effect::Anim(id, Anim::KnobTurn(KNOB_DEG * d.signum() as f32)),
                    click,
                ]
            }
            (Kind::Turntable, Action::Next) => vec![Effect::Media(MediaCmd::Next), click],
            (Kind::Turntable, Action::Prev) => vec![Effect::Media(MediaCmd::Prev), click],
            (Kind::Lamp, Action::Adjust(d)) => {
                let cur = match self.data.get(&id) {
                    Some(Data::Brightness(b)) => *b,
                    _ => 1.0,
                };
                let v = stepped(cur, d.signum(), MIN_BRIGHTNESS, 1.0);
                if v == cur {
                    return vec![Effect::Anim(id, Anim::Jiggle)];
                }
                self.data.insert(id, Data::Brightness(v));
                vec![
                    Effect::SetBrightness(v),
                    Effect::Anim(id, Anim::KnobTurn(KNOB_DEG * d.signum() as f32)),
                    click,
                ]
            }
            (Kind::Door(door), Action::Interact) => vec![Effect::ToggleDoor(door)],
            (Kind::Vault, Action::Interact) => vec![Effect::VaultPrompt],
            (Kind::PackageBox, Action::Interact | Action::Open) => {
                if let Some(Data::Updates(u)) = self.data.get(&id) {
                    self.updates_seen = u.count;
                }
                vec![
                    Effect::OpenList(ListKind::Updates),
                    Effect::Anim(id, Anim::LidOpen),
                ]
            }
            (Kind::PackageBox, Action::Close) => vec![Effect::Anim(id, Anim::LidClose)],
            (Kind::TrayShelf, Action::Interact | Action::Open) => {
                vec![Effect::OpenList(ListKind::Tray)]
            }
            (Kind::Calendar, Action::Interact | Action::Open) => {
                vec![Effect::OpenList(ListKind::Calendar)]
            }
            (Kind::BtRadio, Action::Interact | Action::Open) => {
                vec![Effect::OpenList(ListKind::Bt)]
            }
            (Kind::Battery, Action::Interact) => {
                vec![Effect::Cue(Cue::Purr), Effect::Anim(id, Anim::Jiggle)]
            }
            (
                Kind::WallClock
                | Kind::NetPlant
                | Kind::Fireplace
                | Kind::Aquarium
                | Kind::OnAir
                | Kind::KeyboardFlag
                | Kind::Lamp,
                Action::Interact,
            ) => vec![Effect::Anim(id, Anim::Jiggle)],
            _ => Vec::new(),
        }
    }

    /// New data from an adapter; the effects are the ambient reactions
    /// (the battery cat's meow, the needle following the player).
    pub fn set(&mut self, id: ObjectId, d: Data) -> Vec<Effect> {
        if self.object(id).is_none() {
            return Vec::new();
        }
        let mut out = Vec::new();
        match &d {
            Data::Power(p) => {
                let low = p.present && !p.charging && p.pct < BATTERY_LOW;
                if low && !self.battery_warned {
                    self.battery_warned = true;
                    out.push(Effect::Cue(Cue::Meow));
                    out.push(Effect::Anim(id, Anim::Jiggle));
                } else if !p.present || p.charging || p.pct >= BATTERY_LOW + BATTERY_HYSTERESIS {
                    self.battery_warned = false;
                }
            }
            Data::Net(n) => self.net_seen |= n.connected,
            Data::Media(m) => {
                let was = match self.data.get(&id) {
                    Some(Data::Media(old)) => old.playing,
                    _ => false,
                };
                if m.playing != was {
                    let needle = if m.playing {
                        Anim::NeedleDrop
                    } else {
                        Anim::NeedleLift
                    };
                    out.push(Effect::Anim(id, needle));
                    out.push(Effect::Anim(id, Anim::Spin(m.playing)));
                }
            }
            Data::Updates(u) => {
                // fewer updates (some applied): the seen mark follows down
                self.updates_seen = self.updates_seen.min(u.count);
            }
            _ => {}
        }
        self.data.insert(id, d);
        out
    }

    /// How often object `id` needs frames at `tier`. `None` for everything
    /// out of view, at rest, or under reduced motion (door swings are
    /// `door::Door::want`, not here).
    pub fn want(&self, id: ObjectId, tier: Tier, reduced_motion: bool) -> Option<Want> {
        if reduced_motion || tier == Tier::Far {
            return None;
        }
        let kind = self.object(id)?.kind;
        let close = tier >= Tier::Readable;
        let pressure = matches!(self.data.get(&id), Some(Data::Load(l)) if l.pressure);
        match kind {
            Kind::Turntable => match self.data.get(&id) {
                Some(Data::Media(m)) if m.playing => {
                    Some(Want::Rate(if close { 12.0 } else { 4.0 }))
                }
                _ => None,
            },
            // the flames rise with pressure; read up close, the exact number
            // is sampled at 1 Hz
            Kind::Fireplace if pressure => Some(Want::Rate(8.0)),
            Kind::Aquarium if pressure => Some(Want::Rate(6.0)),
            Kind::Fireplace | Kind::Aquarium if close => Some(Want::Rate(1.0)),
            _ => None,
        }
    }
}

/// Reception (room 0) and library (room 1) with every kind but `Door` once,
/// and fixed fictional data (no real values, nothing personal).
pub fn demo_world() -> World {
    use std::f32::consts::{FRAC_PI_2, PI};
    const RECEPTION: RoomId = RoomId(0);
    const LIBRARY: RoomId = RoomId(1);
    let place = |id: u32, kind: Kind, room: RoomId, x: f32, y: f32, z: f32, yaw: f32| {
        let pos = Vec3::new(x, y, z);
        // stand 1.2 m in front of it, facing it
        let front = Vec2::new(yaw.sin(), -yaw.cos());
        let eye = Vec3::new(x - front.x * 1.2, 1.6, z - front.y * 1.2);
        Object {
            id: ObjectId(id),
            kind,
            room,
            pos,
            yaw,
            stand: Pose::new(eye, yaw, 0.0),
            aabb: Aabb::new(Vec2::new(x - 0.3, z - 0.3), Vec2::new(x + 0.3, z + 0.3)),
        }
    };
    let objects = vec![
        place(1, Kind::Turntable, RECEPTION, 2.0, 0.9, -4.0, 0.0),
        place(2, Kind::WallClock, RECEPTION, 0.0, 2.2, -5.4, 0.0),
        place(3, Kind::Battery, RECEPTION, -3.0, 0.5, -1.0, -FRAC_PI_2),
        place(4, Kind::NetPlant, RECEPTION, 3.5, 0.8, -5.0, 0.0),
        place(5, Kind::Fireplace, RECEPTION, -3.4, 0.4, -4.0, -FRAC_PI_2),
        place(6, Kind::Aquarium, RECEPTION, 3.4, 1.0, -2.0, FRAC_PI_2),
        place(7, Kind::PackageBox, RECEPTION, -1.0, 0.2, 1.8, PI),
        place(8, Kind::TrayShelf, RECEPTION, -2.0, 1.2, 1.9, PI),
        place(9, Kind::OnAir, RECEPTION, -1.0, 2.3, -5.4, 0.0),
        place(10, Kind::KeyboardFlag, RECEPTION, 1.0, 0.8, -1.5, 0.0),
        place(11, Kind::Calendar, RECEPTION, 1.0, 1.6, -5.4, 0.0),
        place(12, Kind::BtRadio, RECEPTION, 3.0, 0.6, 0.5, FRAC_PI_2),
        place(13, Kind::Lamp, RECEPTION, 0.0, 2.6, -2.0, 0.0),
        place(14, Kind::Vault, LIBRARY, 12.0, 1.0, -5.4, 0.0),
    ];
    let mut w = World::new(objects);
    let data = [
        (
            1,
            Data::Media(Media {
                playing: true,
                title: "Lo-fi for cats".into(),
                artist: "acme radio".into(),
                has_art: false,
                volume: 0.6,
                position_s: Some(42.0),
                length_s: Some(180.0),
            }),
        ),
        (
            2,
            Data::Clock(Clock {
                unix_s: 1_000_000_000,
                hour: 14,
                minute: 5,
                band: 4,
            }),
        ),
        (
            3,
            Data::Power(Power {
                pct: 73,
                charging: false,
                time_to_empty_s: Some(9000),
                present: true,
            }),
        ),
        (
            4,
            Data::Net(Net {
                connected: true,
                wifi: true,
                strength: 3,
            }),
        ),
        (
            5,
            Data::Load(Load {
                cpu_pct: Some(12),
                mem_pct: Some(40),
                pressure: false,
            }),
        ),
        (
            6,
            Data::Load(Load {
                cpu_pct: Some(12),
                mem_pct: Some(40),
                pressure: false,
            }),
        ),
        (
            7,
            Data::Updates(Updates {
                count: 3,
                checked_at: Some(1_000_000_000),
            }),
        ),
        (
            8,
            Data::Tray(Tray {
                items: vec![
                    TrayItem {
                        id: 1,
                        title: "acme".into(),
                        has_menu: true,
                    },
                    TrayItem {
                        id: 2,
                        title: "demo".into(),
                        has_menu: false,
                    },
                ],
            }),
        ),
        (9, Data::OnAir(false)),
        (10, Data::Layout("us".into())),
        (
            12,
            Data::Bt(Bt {
                powered: true,
                connected: 1,
                battery_pct: Some(80),
            }),
        ),
        (13, Data::Brightness(0.8)),
    ];
    for (id, d) in data {
        w.set(ObjectId(id), d);
    }
    w
}

#[cfg(test)]
mod tests {
    use super::*;

    const TT: ObjectId = ObjectId(1);
    const BAT: ObjectId = ObjectId(3);
    const FIRE: ObjectId = ObjectId(5);
    const BOX: ObjectId = ObjectId(7);
    const LAMP: ObjectId = ObjectId(13);
    const VAULT: ObjectId = ObjectId(14);

    fn media(w: &World) -> &Media {
        match w.data.get(&TT) {
            Some(Data::Media(m)) => m,
            d => panic!("{d:?}"),
        }
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
    fn turntable_interact_toggles_play_with_the_needle() {
        let mut w = demo_world();
        let out = w.apply(TT, Action::Interact, 0.0);
        assert_eq!(out[0], Effect::Media(MediaCmd::PlayPause));
        assert!(
            out.contains(&Effect::Anim(TT, Anim::NeedleLift)),
            "playing: lift"
        );
        let mut m = media(&w).clone();
        m.playing = false;
        w.set(TT, Data::Media(m));
        let out = w.apply(TT, Action::Interact, 0.0);
        assert!(out.contains(&Effect::Anim(TT, Anim::NeedleDrop)));
        assert_eq!(
            out.iter().filter(|e| matches!(e, Effect::Cue(_))).count(),
            1,
            "one click"
        );
    }

    #[test]
    fn turntable_without_a_player_only_jiggles() {
        let mut w = demo_world();
        w.data.remove(&TT);
        assert_eq!(
            w.apply(TT, Action::Interact, 0.0),
            [Effect::Anim(TT, Anim::Jiggle)]
        );
        assert_eq!(
            w.apply(TT, Action::Adjust(1), 0.0),
            [Effect::Anim(TT, Anim::Jiggle)]
        );
    }

    #[test]
    fn turntable_knob_steps_volume_by_5_percent_clamped() {
        let mut w = demo_world();
        let out = w.apply(TT, Action::Adjust(1), 0.0);
        assert_eq!(out[0], Effect::SetVolume(0.65));
        assert_eq!(out[1], Effect::Anim(TT, Anim::KnobTurn(18.0)));
        let out = w.apply(TT, Action::Adjust(-1), 0.0);
        assert_eq!(out[0], Effect::SetVolume(0.6));
        assert_eq!(out[1], Effect::Anim(TT, Anim::KnobTurn(-18.0)));
        let mut last = Vec::new();
        for _ in 0..20 {
            let out = w.apply(TT, Action::Adjust(1), 0.0);
            if let Effect::SetVolume(v) = out[0] {
                last.push(v);
            }
        }
        assert_eq!(*last.last().unwrap(), 1.0);
        assert!(last.iter().all(|v| *v <= 1.0));
        assert_eq!(
            w.apply(TT, Action::Adjust(1), 0.0),
            [Effect::Anim(TT, Anim::Jiggle)],
            "at the stop"
        );
        for _ in 0..30 {
            w.apply(TT, Action::Adjust(-1), 0.0);
        }
        assert_eq!(media(&w).volume, 0.0);
        // an off-grid volume from the sink snaps to the grid
        let mut m = media(&w).clone();
        m.volume = 0.33;
        w.set(TT, Data::Media(m));
        assert_eq!(
            w.apply(TT, Action::Adjust(1), 0.0)[0],
            Effect::SetVolume(0.4)
        );
    }

    #[test]
    fn turntable_next_prev_are_media_commands() {
        let mut w = demo_world();
        assert_eq!(
            w.apply(TT, Action::Next, 0.0)[0],
            Effect::Media(MediaCmd::Next)
        );
        assert_eq!(
            w.apply(TT, Action::Prev, 0.0)[0],
            Effect::Media(MediaCmd::Prev)
        );
    }

    #[test]
    fn media_data_moves_the_needle_and_the_disc() {
        let mut w = demo_world();
        let mut m = media(&w).clone();
        m.playing = false;
        assert_eq!(
            w.set(TT, Data::Media(m.clone())),
            [
                Effect::Anim(TT, Anim::NeedleLift),
                Effect::Anim(TT, Anim::Spin(false))
            ]
        );
        assert!(w.set(TT, Data::Media(m)).is_empty(), "no change, no anim");
    }

    #[test]
    fn battery_meows_once_below_15_with_hysteresis() {
        let mut w = demo_world();
        assert!(w.set(BAT, power(16, false)).is_empty());
        let out = w.set(BAT, power(14, false));
        assert_eq!(out[0], Effect::Cue(Cue::Meow));
        assert!(w.set(BAT, power(13, false)).is_empty(), "once");
        assert!(w.set(BAT, power(16, false)).is_empty(), "within hysteresis");
        assert!(w.set(BAT, power(14, false)).is_empty(), "no ping-pong");
        assert!(w.set(BAT, power(18, false)).is_empty(), "re-armed");
        assert_eq!(w.set(BAT, power(12, false))[0], Effect::Cue(Cue::Meow));
        // charging re-arms and never meows
        assert!(w.set(BAT, power(10, true)).is_empty());
        assert_eq!(w.set(BAT, power(10, false))[0], Effect::Cue(Cue::Meow));
        // no battery: nothing
        let mut d = demo_world();
        let none = Data::Power(Power {
            pct: 0,
            charging: false,
            time_to_empty_s: None,
            present: false,
        });
        assert!(d.set(BAT, none).is_empty());
    }

    #[test]
    fn door_and_vault_objects_ask_the_glue() {
        let mut w = demo_world();
        w.objects.push(Object {
            id: ObjectId(50),
            kind: Kind::Door(DoorId(3)),
            ..w.objects[0].clone()
        });
        assert_eq!(
            w.apply(ObjectId(50), Action::Interact, 0.0),
            [Effect::ToggleDoor(DoorId(3))]
        );
        assert_eq!(w.apply(VAULT, Action::Interact, 0.0), [Effect::VaultPrompt]);
        assert!(
            w.apply(ObjectId(999), Action::Interact, 0.0).is_empty(),
            "unknown id"
        );
    }

    #[test]
    fn lamp_dimmer_clamps_between_5_percent_and_full() {
        let mut w = demo_world();
        assert_eq!(
            w.apply(LAMP, Action::Adjust(1), 0.0)[0],
            Effect::SetBrightness(0.85)
        );
        for _ in 0..40 {
            w.apply(LAMP, Action::Adjust(-1), 0.0);
        }
        assert_eq!(w.data.get(&LAMP), Some(&Data::Brightness(MIN_BRIGHTNESS)));
        assert_eq!(
            w.apply(LAMP, Action::Adjust(-1), 0.0),
            [Effect::Anim(LAMP, Anim::Jiggle)]
        );
        for _ in 0..40 {
            w.apply(LAMP, Action::Adjust(1), 0.0);
        }
        assert_eq!(w.data.get(&LAMP), Some(&Data::Brightness(1.0)));
    }

    #[test]
    fn lists_open_from_their_objects() {
        let mut w = demo_world();
        for (id, kind) in [
            (7, ListKind::Updates),
            (8, ListKind::Tray),
            (11, ListKind::Calendar),
            (12, ListKind::Bt),
        ] {
            assert_eq!(
                w.apply(ObjectId(id), Action::Interact, 0.0)[0],
                Effect::OpenList(kind)
            );
        }
    }

    #[test]
    fn reading_the_package_box_clears_its_badge_count() {
        let mut w = demo_world();
        assert_eq!(w.unseen_updates(), 3);
        w.apply(BOX, Action::Interact, 0.0);
        assert_eq!(w.unseen_updates(), 0);
        w.set(
            BOX,
            Data::Updates(Updates {
                count: 5,
                checked_at: None,
            }),
        );
        assert_eq!(w.unseen_updates(), 2);
        w.set(
            BOX,
            Data::Updates(Updates {
                count: 1,
                checked_at: None,
            }),
        );
        assert_eq!(w.unseen_updates(), 0);
        w.set(
            BOX,
            Data::Updates(Updates {
                count: 2,
                checked_at: None,
            }),
        );
        assert_eq!(w.unseen_updates(), 1);
        assert_eq!(
            w.apply(BOX, Action::Close, 0.0),
            [Effect::Anim(BOX, Anim::LidClose)]
        );
    }

    #[test]
    fn inert_objects_jiggle_and_ignore_other_actions() {
        let mut w = demo_world();
        for id in [2, 4, 5, 6, 9, 10] {
            assert_eq!(
                w.apply(ObjectId(id), Action::Interact, 0.0),
                [Effect::Anim(ObjectId(id), Anim::Jiggle)]
            );
            assert!(w.apply(ObjectId(id), Action::Adjust(1), 0.0).is_empty());
        }
        assert_eq!(
            w.apply(BAT, Action::Interact, 0.0)[0],
            Effect::Cue(Cue::Purr)
        );
    }

    #[test]
    fn want_table() {
        let mut w = demo_world();
        use Tier::*;
        for (tier, want) in [
            (Interactable, Some(Want::Rate(12.0))),
            (Readable, Some(Want::Rate(12.0))),
            (Ambient, Some(Want::Rate(4.0))),
            (Far, None),
        ] {
            assert_eq!(
                w.want(TT, tier, false),
                want,
                "turntable playing at {tier:?}"
            );
            assert_eq!(w.want(TT, tier, true), None, "reduced motion");
        }
        let mut m = media(&w).clone();
        m.playing = false;
        w.set(TT, Data::Media(m));
        assert_eq!(w.want(TT, Readable, false), None, "paused: still");
        // fireplace: pressure burns at 8 fps when visible; up close the number at 1 Hz
        assert_eq!(w.want(FIRE, Ambient, false), None);
        assert_eq!(w.want(FIRE, Readable, false), Some(Want::Rate(1.0)));
        w.set(
            FIRE,
            Data::Load(Load {
                cpu_pct: Some(90),
                mem_pct: None,
                pressure: true,
            }),
        );
        assert_eq!(w.want(FIRE, Ambient, false), Some(Want::Rate(8.0)));
        assert_eq!(w.want(FIRE, Far, false), None);
        assert_eq!(w.want(FIRE, Ambient, true), None);
        w.set(
            ObjectId(6),
            Data::Load(Load {
                cpu_pct: None,
                mem_pct: Some(95),
                pressure: true,
            }),
        );
        assert_eq!(w.want(ObjectId(6), Ambient, false), Some(Want::Rate(6.0)));
        // everything else idles at zero, at any tier
        for id in [2, 3, 4, 7, 8, 9, 10, 11, 12, 13, 14, 999] {
            for tier in [Interactable, Readable, Ambient, Far] {
                assert_eq!(
                    w.want(ObjectId(id), tier, false),
                    None,
                    "object {id} at {tier:?}"
                );
            }
        }
    }

    #[test]
    fn demo_world_has_every_kind_once_and_nothing_personal() {
        let w = demo_world();
        let kinds = [
            Kind::Turntable,
            Kind::WallClock,
            Kind::Battery,
            Kind::NetPlant,
            Kind::Fireplace,
            Kind::Aquarium,
            Kind::PackageBox,
            Kind::TrayShelf,
            Kind::OnAir,
            Kind::KeyboardFlag,
            Kind::Calendar,
            Kind::BtRadio,
            Kind::Lamp,
            Kind::Vault,
        ];
        assert_eq!(w.objects.len(), kinds.len());
        for k in kinds {
            assert_eq!(w.objects.iter().filter(|o| o.kind == k).count(), 1, "{k:?}");
        }
        assert!(!w.objects.iter().any(|o| matches!(o.kind, Kind::Door(_))));
        assert_eq!(
            w.find(Kind::Vault).unwrap().room,
            RoomId(1),
            "the vault is in the library"
        );
        let ids: std::collections::BTreeSet<_> = w.objects.iter().map(|o| o.id).collect();
        assert_eq!(ids.len(), w.objects.len(), "unique ids");
        // fixed: two builds are equal
        assert_eq!(demo_world(), w);
        // the strings are the demo's own
        let text = serde_json::to_string(&w.data.values().collect::<Vec<_>>()).unwrap();
        for s in [
            "Lo-fi for cats",
            "acme radio",
            "\"acme\"",
            "\"demo\"",
            "\"us\"",
        ] {
            assert!(text.contains(s), "{s}");
        }
        let strings: Vec<&str> = text.split('"').skip(1).step_by(2).collect();
        let allowed = [
            "Media",
            "playing",
            "title",
            "Lo-fi for cats",
            "artist",
            "acme radio",
            "has_art",
            "volume",
            "position_s",
            "length_s",
            "Clock",
            "unix_s",
            "hour",
            "minute",
            "band",
            "Power",
            "pct",
            "charging",
            "time_to_empty_s",
            "present",
            "Net",
            "connected",
            "wifi",
            "strength",
            "Load",
            "cpu_pct",
            "mem_pct",
            "pressure",
            "Updates",
            "count",
            "checked_at",
            "Tray",
            "items",
            "id",
            "has_menu",
            "acme",
            "demo",
            "OnAir",
            "Layout",
            "us",
            "Bt",
            "powered",
            "battery_pct",
            "Brightness",
        ];
        for s in strings {
            assert!(
                allowed.contains(&s),
                "unexpected string in the demo data: {s}"
            );
        }
        // stands face their objects from 1.2 m
        for o in &w.objects {
            let to = Vec2::new(o.pos.x, o.pos.z) - Vec2::new(o.stand.pos.x, o.stand.pos.z);
            assert!((to.length() - 1.2).abs() < 1e-3, "{:?}", o.kind);
            assert!(
                to.normalize().dot(o.stand.heading()) > 0.999,
                "{:?} faces it",
                o.kind
            );
            assert!(o.aabb.contains(Vec2::new(o.pos.x, o.pos.z)));
        }
    }

    #[test]
    fn set_ignores_unknown_objects() {
        let mut w = demo_world();
        assert!(w.set(ObjectId(999), power(5, false)).is_empty());
        assert!(!w.data.contains_key(&ObjectId(999)));
        assert!(w.net_seen());
    }
}
