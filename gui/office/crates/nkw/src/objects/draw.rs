//! The objects as kit instances. Until `nkw-gl`'s furniture meshes (S10)
//! land, every object is a few tinted primitives (`Box`, `RoundedBox`,
//! `Cylinder`) and the cat parts; the data shows on them (the oil level,
//! the leaves, the clock's hands, the turntable's platter and needle).
//!
//! Floor furniture is built from the floor point under the object, wall
//! and ceiling things from the object's own point; local +z is the front,
//! towards where you stand (`scene::model`'s convention).

use glam::{Mat4, Quat, Vec3};
use nkw_core::Time;
use nkw_core::layout;
use nkw_gl::draw::{Instance, MeshId};
use nkw_gl::rig;
use nkw_objects::object::{Data, Kind, Object};

/// The per-object animation state the effects drive (`object::Anim`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AnimState {
    pub spin: bool,
    pub needle_down: bool,
    /// the volume knob, degrees
    pub knob_deg: f32,
    /// a wobble until then
    pub jiggle_until: Time,
    pub lid_open: bool,
}

impl Default for AnimState {
    fn default() -> AnimState {
        AnimState {
            spin: false,
            needle_down: false,
            knob_deg: 0.0,
            jiggle_until: f64::NEG_INFINITY,
            lid_open: false,
        }
    }
}

/// How long a jiggle lasts, seconds.
pub const JIGGLE_S: f64 = 0.35;
/// The platter's speed, rad/s (33⅓ rpm).
pub const SPIN_RAD_S: f32 = 33.333 / 60.0 * std::f32::consts::TAU;

impl AnimState {
    pub fn apply(&mut self, a: nkw_objects::object::Anim, now: Time) {
        use nkw_objects::object::Anim;
        match a {
            Anim::KnobTurn(d) => self.knob_deg += d,
            Anim::NeedleLift => self.needle_down = false,
            Anim::NeedleDrop => self.needle_down = true,
            Anim::Spin(on) => self.spin = on,
            Anim::Jiggle => self.jiggle_until = now + JIGGLE_S,
            Anim::LidOpen => self.lid_open = true,
            Anim::LidClose => self.lid_open = false,
        }
    }

    pub fn jiggling(&self, now: Time) -> bool {
        now < self.jiggle_until
    }
}

const WOOD: [u8; 4] = [0xb0, 0x80, 0x5a, 0xff];
const BRASS: [u8; 4] = [0xd8, 0xb0, 0x58, 0xff];
const FLAME: [u8; 4] = [0xff, 0xa0, 0x30, 0xff];
const CARD: [u8; 4] = [0xc8, 0xa0, 0x70, 0xff];
const STEEL: [u8; 4] = [0x70, 0x76, 0x80, 0xff];
const FUR: [u8; 4] = [0xe8, 0xc8, 0xa0, 0xff];
const BLUE: [u8; 4] = [0x60, 0x90, 0xd0, 0xff];

/// Instances of one object for this frame.
pub struct Ctx<'a> {
    pub cam: Vec3,
    pub now: Time,
    pub reduced_motion: bool,
    pub data: Option<&'a Data>,
    pub anim: AnimState,
    /// the vault's door, 0 shut .. 1 open
    pub vault_open: f32,
}

struct Out<'a> {
    cam: Vec3,
    base: Mat4,
    v: &'a mut Vec<Instance>,
}

impl Out<'_> {
    /// A part at `off` (object frame), `size` big, turned by `rot`.
    fn part(
        &mut self,
        mesh: MeshId,
        off: Vec3,
        size: Vec3,
        rot: Quat,
        tint: [u8; 4],
        emissive: f32,
    ) {
        let m = self.base * Mat4::from_scale_rotation_translation(size, rot, off);
        let centre = m.transform_point3(Vec3::ZERO);
        self.v.push(Instance {
            mesh,
            model: m,
            tint,
            emissive,
            fade: layout::fade_for(self.cam, centre),
        });
    }

    fn cube(&mut self, off: Vec3, size: Vec3, tint: [u8; 4]) {
        self.part(MeshId::Box, off, size, Quat::IDENTITY, tint, 0.0);
    }

    fn rounded(&mut self, off: Vec3, size: Vec3, tint: [u8; 4]) {
        self.part(MeshId::RoundedBox, off, size, Quat::IDENTITY, tint, 0.0);
    }

    fn cyl(&mut self, off: Vec3, diam: f32, h: f32, tint: [u8; 4]) {
        self.part(
            MeshId::Cylinder,
            off,
            Vec3::new(diam, h, diam),
            Quat::IDENTITY,
            tint,
            0.0,
        );
    }
}

/// Clock hand angles, radians clockwise from 12: (hour, minute).
pub fn hand_angles(hour: u8, minute: u8) -> (f32, f32) {
    let m = minute.min(59) as f32;
    let h = (hour % 12) as f32 + m / 60.0;
    (h * 30f32.to_radians(), m * 6f32.to_radians())
}

/// The platter's angle at `now` (still when not spinning or under reduced
/// motion).
pub fn platter_angle(anim: &AnimState, now: Time, reduced: bool) -> f32 {
    if anim.spin && !reduced {
        ((now as f32) * SPIN_RAD_S) % std::f32::consts::TAU
    } else {
        0.0
    }
}

/// Append the instances of `o`.
pub fn object(o: &Object, c: &Ctx, out: &mut Vec<Instance>) {
    let wobble = if c.anim.jiggling(c.now) && !c.reduced_motion {
        let t = (c.anim.jiggle_until - c.now) as f32;
        (t * 40.0).sin() * 0.06 * (t / JIGGLE_S as f32)
    } else {
        0.0
    };
    let turn = Quat::from_rotation_y(-o.yaw) * Quat::from_rotation_z(wobble);
    let floor = Vec3::new(o.pos.x, 0.0, o.pos.z);
    let on_floor = Mat4::from_rotation_translation(turn, floor);
    let at = Mat4::from_rotation_translation(turn, o.pos);
    let first = out.len();
    let mut f = Out {
        cam: c.cam,
        base: on_floor,
        v: out,
    };
    match o.kind {
        Kind::Turntable => {
            let spin = platter_angle(&c.anim, c.now, c.reduced_motion);
            rig::turntable(
                on_floor,
                spin,
                c.anim.needle_down,
                -c.anim.knob_deg.to_radians(),
                f.v,
            );
        }
        Kind::WallClock => {
            f.v.push(rig::piece(MeshId::WallClock, at));
            let (h, m) = match c.data {
                Some(Data::Clock(k)) => (k.hour, k.minute),
                _ => (10, 10),
            };
            f.v.extend(rig::clock_hands(at, h, m));
        }
        Kind::Battery => battery(&mut f, c),
        Kind::NetPlant => {
            let bars = match c.data {
                Some(Data::Net(n)) if n.connected => n.strength.clamp(1, 4),
                Some(Data::Net(_)) => 0,
                _ => 2,
            };
            // on the window sill: its point is PLANT_UP above the pot's base
            let sill =
                Mat4::from_rotation_translation(turn, o.pos - Vec3::Y * super::place::PLANT_UP);
            rig::plant(sill, bars, f.v);
        }
        Kind::Fireplace => {
            let pressure = matches!(c.data, Some(Data::Load(l)) if l.pressure);
            rig::fireplace(on_floor, if pressure { 0.8 } else { 0.15 }, f.v);
        }
        Kind::Aquarium => {
            let pressure = matches!(c.data, Some(Data::Load(l)) if l.pressure);
            rig::aquarium(on_floor, if pressure { 0.8 } else { 0.0 }, 0.0, f.v);
        }
        Kind::PackageBox => {
            f.cube(Vec3::new(0.0, 0.15, 0.0), Vec3::new(0.45, 0.3, 0.35), CARD);
            if c.anim.lid_open {
                f.cube(Vec3::new(0.0, 0.32, -0.2), Vec3::new(0.45, 0.02, 0.2), CARD);
            }
        }
        Kind::TrayShelf => {
            f.base = at;
            f.cube(Vec3::ZERO, Vec3::new(0.7, 0.03, 0.2), WOOD);
            let n = match c.data {
                Some(Data::Tray(t)) => t.items.len().min(4),
                _ => 0,
            };
            for i in 0..n {
                let x = -0.24 + i as f32 * 0.16;
                f.v.push(rig::piece(
                    MeshId::Jar,
                    at * Mat4::from_translation(Vec3::new(x, 0.015, 0.0)),
                ));
            }
        }
        Kind::OnAir => rig::on_air(at, matches!(c.data, Some(Data::OnAir(true))), f.v),
        Kind::KeyboardFlag => {
            f.cube(Vec3::new(0.0, 0.35, 0.0), Vec3::new(0.4, 0.7, 0.4), WOOD);
            f.cyl(Vec3::new(0.0, 0.85, 0.0), 0.015, 0.3, BRASS);
            f.cube(
                Vec3::new(0.07, 0.93, 0.0),
                Vec3::new(0.14, 0.09, 0.005),
                BLUE,
            );
        }
        Kind::Calendar => f.v.push(rig::piece(MeshId::Calendar, at)),
        Kind::BtRadio => {
            let lit = matches!(c.data, Some(Data::Bt(b)) if b.connected > 0);
            f.rounded(
                Vec3::new(0.0, 0.125, 0.0),
                Vec3::new(0.35, 0.25, 0.18),
                WOOD,
            );
            f.part(
                MeshId::Cylinder,
                Vec3::new(0.12, 0.35, 0.0),
                Vec3::new(0.012, 0.22, 0.012),
                Quat::IDENTITY,
                if lit { FLAME } else { STEEL },
                if lit { 0.7 } else { 0.0 },
            );
        }
        Kind::Lamp => {
            let b = match c.data {
                Some(Data::Brightness(b)) => *b,
                _ => 1.0,
            };
            f.v.push(Instance {
                emissive: 0.6 * b,
                ..rig::piece(MeshId::Lamp, on_floor)
            });
        }
        Kind::Vault => rig::safe(on_floor, c.vault_open, f.v),
        // doors are drawn by `doors::Doors::models`
        Kind::Door(_) => {}
    }
    // the kit's pieces come without a fade: dither them like the rest
    for i in &mut out[first..] {
        let centre = i.model.transform_point3(Vec3::ZERO);
        i.fade = layout::fade_for(c.cam, centre);
    }
}

fn battery(f: &mut Out, c: &Ctx) {
    let (pct, low) = match c.data {
        Some(Data::Power(p)) if p.present => (
            p.pct.min(100) as f32 / 100.0,
            !p.charging && p.pct < nkw_objects::object::BATTERY_LOW,
        ),
        _ => (0.0, false),
    };
    let base = f.base;
    // the cushion, the oil jar on it to one side, the cat curled on the other
    f.v.push(rig::piece(MeshId::Cushion, base));
    rig::oil_lamp(
        base * Mat4::from_translation(Vec3::new(-0.12, 0.12, 0.0)),
        pct,
        f.v,
    );
    f.base = base
        * Mat4::from_translation(Vec3::new(0.1, 0.2, 0.0))
        * Mat4::from_scale(Vec3::splat(0.6));
    f.part(
        MeshId::CatBody,
        Vec3::ZERO,
        Vec3::ONE,
        Quat::IDENTITY,
        FUR,
        0.0,
    );
    // asleep, or head up and calling when low
    let head = if low {
        Vec3::new(0.0, 0.22, 0.2)
    } else {
        Vec3::new(0.0, 0.02, 0.24)
    };
    f.part(MeshId::CatHead, head, Vec3::ONE, Quat::IDENTITY, FUR, 0.0);
    f.base = base;
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec2;
    use nkw_core::{Aabb, ObjectId, Pose, RoomId};
    use nkw_objects::object::{Anim, Clock, Net, Power, demo_world};

    fn obj(kind: Kind) -> Object {
        Object {
            id: ObjectId(1),
            kind,
            room: RoomId(0),
            pos: Vec3::new(1.0, 1.0, -3.0),
            yaw: 0.0,
            stand: Pose::default(),
            aabb: Aabb::new(Vec2::ZERO, Vec2::ONE),
        }
    }

    fn ctx(data: Option<&Data>) -> Ctx<'_> {
        Ctx {
            cam: Vec3::new(1.0, 1.5, 0.0),
            now: 1.0,
            reduced_motion: false,
            data,
            anim: AnimState::default(),
            vault_open: 0.0,
        }
    }

    #[test]
    fn every_demo_object_draws_a_few_finite_instances() {
        let w = demo_world();
        for o in &w.objects {
            let mut v = Vec::new();
            object(o, &ctx(w.data.get(&o.id)), &mut v);
            assert!(!v.is_empty() && v.len() <= 12, "{:?}: {}", o.kind, v.len());
            for i in &v {
                assert!(i.model.is_finite(), "{:?}", o.kind);
            }
        }
        let mut v = Vec::new();
        object(&obj(Kind::Door(nkw_core::DoorId(0))), &ctx(None), &mut v);
        assert!(v.is_empty(), "doors are the doors module's");
    }

    #[test]
    fn hand_angle_table() {
        let d = |x: f32| x.to_degrees().round();
        let rows = [
            ((3, 0), (90.0, 0.0)),
            ((12, 30), (15.0, 180.0)),
            ((14, 6), (63.0, 36.0)),
        ];
        for ((h, m), (hd, md)) in rows {
            let (a, b) = hand_angles(h, m);
            assert_eq!((d(a), d(b)), (hd, md), "{h}:{m}");
        }
    }

    #[test]
    fn the_clock_shows_its_time_three_oclock_points_right() {
        let data = Data::Clock(Clock {
            unix_s: 0,
            hour: 3,
            minute: 0,
            band: 3,
        });
        let mut v = Vec::new();
        object(&obj(Kind::WallClock), &ctx(Some(&data)), &mut v);
        assert_eq!(v[1].mesh, MeshId::ClockHand);
        // the hand runs along its +y: at 3 o'clock that is the clock's +x
        let dir = v[1].model.transform_vector3(Vec3::Y);
        assert!(dir.x > 0.0 && dir.y.abs() < 1e-4, "{dir:?}");
    }

    #[test]
    fn the_oil_follows_the_charge_and_the_leaves_the_bars() {
        let height = |pct| {
            let d = Data::Power(Power {
                pct,
                charging: false,
                time_to_empty_s: None,
                present: true,
            });
            let mut v = Vec::new();
            object(&obj(Kind::Battery), &ctx(Some(&d)), &mut v);
            v.iter()
                .find(|i| i.mesh == MeshId::Cylinder)
                .map(|i| i.model.transform_vector3(Vec3::Y).length())
                .unwrap()
        };
        assert!(height(80) > height(20) * 3.0);
        let leaves = |strength| {
            let d = Data::Net(Net {
                connected: true,
                wifi: true,
                strength,
            });
            let mut v = Vec::new();
            object(&obj(Kind::NetPlant), &ctx(Some(&d)), &mut v);
            v.iter()
                .filter(|i| i.mesh == MeshId::Plant && i.tint[3] == 255)
                .count()
        };
        assert_eq!(leaves(3), 3);
        assert_eq!(leaves(4), 4);
        assert_eq!(leaves(0), 1, "connected with no signal: one leaf");
    }

    #[test]
    fn the_platter_spins_only_when_asked_and_not_under_reduced_motion() {
        let mut a = AnimState::default();
        assert_eq!(platter_angle(&a, 3.0, false), 0.0);
        a.apply(Anim::Spin(true), 0.0);
        assert!(platter_angle(&a, 1.0, false) > 0.0);
        assert_eq!(platter_angle(&a, 1.0, true), 0.0);
        a.apply(Anim::NeedleDrop, 0.0);
        assert!(a.needle_down);
        a.apply(Anim::KnobTurn(18.0), 0.0);
        a.apply(Anim::KnobTurn(18.0), 0.0);
        assert_eq!(a.knob_deg, 36.0);
        a.apply(Anim::Jiggle, 5.0);
        assert!(a.jiggling(5.1) && !a.jiggling(5.0 + JIGGLE_S));
    }

    #[test]
    fn near_parts_fade() {
        let mut c = ctx(None);
        c.cam = Vec3::new(1.0, 1.0, -3.0);
        let mut v = Vec::new();
        object(&obj(Kind::WallClock), &c, &mut v);
        assert!(v[0].fade > 0.9);
    }
}
