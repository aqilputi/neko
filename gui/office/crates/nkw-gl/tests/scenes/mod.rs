//! Wave-2 fixture scenes (P6): villagers, kittens, the reception's
//! furniture, a door mid-swing and the desk + chair + monitor + cat
//! composition from the NAV station. Shared by the goldens and the
//! `preview` test. Fixture content only.
#![allow(dead_code)]

use glam::{Mat4, Quat, Vec2, Vec3};
use nkw_core::{Pose, RoomSlot, Side, layout};
use nkw_gl::draw::{Blob, Camera, DrawList, Instance, MeshId, sky_for_band};
use nkw_gl::rig::{self, CatJoints, Coat, piece};

pub const WALL: [u8; 4] = [247, 232, 212, 255];
pub const FLOOR: [u8; 4] = [226, 206, 170, 255];
pub const SIDE_WALL: [u8; 4] = [222, 212, 236, 255];

pub fn camera(pos: Vec3, yaw: f32, pitch: f32, aspect: f32) -> Camera {
    Camera {
        pose: Pose::new(pos, yaw, pitch),
        fov_y: 60f32.to_radians(),
        near: 0.05,
        far: 60.0,
        aspect,
    }
}

fn tinted(mesh: MeshId, m: Mat4, tint: [u8; 4]) -> Instance {
    Instance {
        tint,
        ..piece(mesh, m)
    }
}

/// The root of something standing at `pos`, turned by `yaw` (the `Pose` convention).
pub fn root(pos: Vec3, yaw: f32) -> Mat4 {
    Mat4::from_rotation_translation(Quat::from_rotation_y(-yaw), pos)
}

/// A floor, a back wall at z = `back` and two side walls.
pub fn shell(l: &mut DrawList, w: f32, back: f32, depth: f32) {
    let mid = back + depth / 2.0;
    let i = &mut l.instances;
    i.push(tinted(
        MeshId::Plane,
        Mat4::from_scale_rotation_translation(
            Vec3::new(w, 1.0, depth),
            Quat::IDENTITY,
            Vec3::new(0.0, 0.0, mid),
        ),
        FLOOR,
    ));
    i.push(tinted(
        MeshId::Box,
        Mat4::from_scale_rotation_translation(
            Vec3::new(w, 3.0, 0.1),
            Quat::IDENTITY,
            Vec3::new(0.0, 1.5, back - 0.05),
        ),
        WALL,
    ));
    for x in [-w / 2.0, w / 2.0] {
        i.push(tinted(
            MeshId::Box,
            Mat4::from_scale_rotation_translation(
                Vec3::new(0.1, 3.0, depth),
                Quat::IDENTITY,
                Vec3::new(x, 1.5, mid),
            ),
            SIDE_WALL,
        ));
    }
}

/// `nk_cats::catpose` poses, copied as joints: SIT, Typing, Sleeping, Alert.
pub fn sit() -> CatJoints {
    CatJoints::default()
}

pub fn typing() -> CatJoints {
    CatJoints {
        body_rot_x: -0.22,
        head_y: 0.84,
        head_z: -0.16,
        head_rot_x: -0.25,
        leg_l: 1.15,
        leg_r: 1.15,
        tail_pitch: 1.0,
        tail_sway: 0.5,
        ..sit()
    }
}

pub fn sleeping() -> CatJoints {
    CatJoints {
        body_y: 0.36,
        body_rot_x: -0.55,
        head_y: 0.52,
        head_z: -0.5,
        head_rot_x: -0.2,
        head_rot_z: 0.45,
        ears_back: 0.35,
        eyes_open: 0.0,
        leg_l: 1.45,
        leg_r: 1.45,
        tail_pitch: 1.75,
        tail_sway: 1.1,
        tail_curl: 0.8,
        ..sit()
    }
}

/// CONVERSA: the head turned back to you.
pub fn talking() -> CatJoints {
    CatJoints {
        head_rot_y: std::f32::consts::PI,
        head_rot_x: 0.12,
        ..sit()
    }
}

pub const SEEDS: [u64; 6] = [11, 23, 37, 41, 58, 64];

fn cat_with_blob(l: &mut DrawList, j: &CatJoints, c: &Coat, r: Mat4, collar: Option<[u8; 4]>) {
    rig::cat_instances(j, c, r, collar, 0.0, &mut l.instances);
    let p = r.transform_point3(Vec3::ZERO);
    l.blobs.push(Blob {
        center: Vec3::new(p.x, 0.005, p.z),
        radius: 0.3,
        alpha: 0.35,
    });
}

/// Six coats × three poses (SIT, typing, sleeping), sitting on the floor,
/// seen from behind and above (`front`: turned to face the camera).
pub fn cats_coats(aspect: f32, front: bool) -> DrawList {
    let mut l = DrawList::empty(
        camera(Vec3::new(0.0, 1.55, 3.3), 0.0, -0.32, aspect),
        sky_for_band(3),
    );
    shell(&mut l, 8.0, -2.4, 7.0);
    let yaw = if front { std::f32::consts::PI } else { 0.0 };
    for (row, j) in [sit(), typing(), sleeping()].iter().enumerate() {
        for (k, seed) in SEEDS.iter().enumerate() {
            let x = -2.0 + 0.8 * k as f32;
            let z = -0.9 + 0.9 * row as f32;
            let collar = (k % 2 == 0).then_some([226, 96, 96, 255]);
            cat_with_blob(
                &mut l,
                j,
                &Coat::from_seed(*seed),
                root(Vec3::new(x, 0.0, z), yaw),
                collar,
            );
        }
    }
    l
}

/// One cat close up: `front` turns its head to you (CONVERSA), else its back.
pub fn cat_closeup(aspect: f32, front: bool, seeds: &[u64]) -> DrawList {
    let mut l = DrawList::empty(
        camera(Vec3::new(0.0, 0.95, 1.55), 0.0, -0.2, aspect),
        sky_for_band(3),
    );
    shell(&mut l, 6.0, -1.6, 5.0);
    let n = seeds.len() as f32;
    for (k, seed) in seeds.iter().enumerate() {
        let x = (k as f32 - (n - 1.0) / 2.0) * 0.62;
        let j = if front { talking() } else { sit() };
        cat_with_blob(
            &mut l,
            &j,
            &Coat::from_seed(*seed),
            root(Vec3::new(x, 0.0, 0.0), 0.0),
            Some([120, 170, 226, 255]),
        );
    }
    let _ = aspect;
    l
}

/// Four kittens: two typing on their laptops, two sitting.
pub fn kittens(aspect: f32) -> DrawList {
    let mut l = DrawList::empty(
        camera(Vec3::new(0.0, 0.7, 1.1), 0.0, -0.35, aspect),
        sky_for_band(3),
    );
    shell(&mut l, 5.0, -1.2, 4.0);
    let _ = aspect;
    for (k, seed) in [5u64, 17, 29, 44].iter().enumerate() {
        let x = -0.45 + 0.3 * k as f32;
        let r = root(
            Vec3::new(x, 0.0, -0.1),
            if k % 2 == 0 {
                0.0
            } else {
                std::f32::consts::PI
            },
        );
        rig::kitten_instances(&Coat::from_seed(*seed), r, k < 2, 0.0, &mut l.instances);
        l.blobs.push(Blob {
            center: Vec3::new(x, 0.005, -0.1),
            radius: 0.14,
            alpha: 0.35,
        });
    }
    l
}

/// Every furniture kind once, as a reception would have them.
pub fn reception(aspect: f32) -> DrawList {
    let _ = aspect;
    let mut l = DrawList::empty(
        camera(Vec3::new(0.0, 1.55, 3.4), 0.0, -0.2, aspect),
        sky_for_band(3),
    );
    shell(&mut l, 8.0, -2.0, 7.0);
    let back = -2.0;
    let o = &mut l.instances;
    rig::turntable(
        root(Vec3::new(-2.3, 0.0, back + 0.3), 0.0),
        0.6,
        true,
        0.8,
        o,
    );
    rig::fireplace(root(Vec3::new(0.2, 0.0, back + 0.22), 0.0), 0.6, o);
    rig::aquarium(root(Vec3::new(2.4, 0.0, back + 0.25), 0.0), 0.15, 0.7, o);
    // on the wall
    let clock = root(Vec3::new(0.2, 1.9, back), 0.0);
    o.push(piece(MeshId::WallClock, clock));
    o.extend(rig::clock_hands(clock, 10, 10));
    o.push(piece(
        MeshId::Calendar,
        root(Vec3::new(1.2, 1.55, back), 0.0),
    ));
    o.push(piece(MeshId::Board, root(Vec3::new(-1.0, 1.6, back), 0.0)));
    rig::on_air(root(Vec3::new(-2.3, 2.2, back), 0.0), true, o);
    // the door corner: lamp oil with the battery cat's cushion, the safe, a parcel, jars
    rig::oil_lamp(root(Vec3::new(-1.35, 0.0, -0.6), 0.0), 0.7, o);
    o.push(piece(
        MeshId::Cushion,
        root(Vec3::new(-0.85, 0.0, -0.55), 0.0),
    ));
    rig::safe(root(Vec3::new(3.3, 0.0, -0.6), -0.5), 0.35, o);
    o.push(piece(MeshId::Package, root(Vec3::new(1.4, 0.0, -0.4), 0.3)));
    for (k, tint) in [
        [240, 168, 176, 255],
        [176, 226, 204, 255],
        [164, 206, 236, 255],
    ]
    .into_iter()
    .enumerate()
    {
        o.push(Instance {
            tint,
            ..piece(
                MeshId::Jar,
                root(Vec3::new(1.25 + 0.13 * k as f32, 0.26, -0.4), 0.0),
            )
        });
    }
    o.push(piece(
        MeshId::Lamp,
        root(Vec3::new(3.4, 0.0, back + 0.4), 0.0),
    ));
    rig::plant(root(Vec3::new(-3.2, 0.0, back + 0.4), 0.0), 3, o);
    o.push(piece(
        MeshId::Envelope,
        root(Vec3::new(1.3, 0.01, 0.2), 0.0) * Mat4::from_rotation_x(-1.4),
    ));
    for (x, z, r) in [
        (-2.3, back + 0.3, 0.45),
        (0.2, back + 0.3, 0.6),
        (2.4, back + 0.25, 0.45),
        (-1.1, -0.58, 0.4),
        (3.3, -0.6, 0.35),
    ] {
        l.blobs.push(Blob {
            center: Vec3::new(x, 0.005, z),
            radius: r,
            alpha: 0.3,
        });
    }
    l
}

/// A door frame in a wall with its leaf half open (hinged left) and a
/// second one hinged right, closed.
pub fn door_swing(aspect: f32) -> DrawList {
    let _ = aspect;
    let mut l = DrawList::empty(
        camera(Vec3::new(0.0, 1.5, 3.0), 0.0, -0.12, aspect),
        sky_for_band(3),
    );
    shell(&mut l, 6.0, -1.0, 5.0);
    for (x, open, left) in [(-0.9f32, 0.5f32, true), (0.9, 0.0, false)] {
        // turned round: the leaf swings out of the wall towards the camera
        let frame = root(Vec3::new(x, 0.0, -0.95), std::f32::consts::PI);
        l.instances.push(piece(MeshId::DoorFrame, frame));
        l.instances
            .push(piece(MeshId::DoorLeaf, rig::door_leaf(frame, open, left)));
    }
    l
}

/// A North room of `desks` desks from `x0`, seen from the NAV station of desk `at`.
pub fn office(aspect: f32, seeds: &[u64], at: usize, talk: Option<usize>) -> DrawList {
    let slot = RoomSlot {
        side: Side::North,
        x0: 0.0,
        len: 3.4 * seeds.len() as f32 + 1.0,
        door_x: 1.0,
    };
    let screen = Vec2::new(0.72, 0.45);
    let desk_x = |k: usize| 1.6 + 2.0 * k as f32;
    let station = layout::desk_station(&slot, desk_x(at));
    let mut l = DrawList::empty(
        camera(station.pos, station.yaw, station.pitch, aspect),
        sky_for_band(3),
    );
    // the room shell from its frame
    let f = layout::room_frame(&slot);
    let w = slot.len;
    let centre = layout::to_world(&slot, Vec3::new(w / 2.0, 0.0, layout::ROOM_DEPTH / 2.0));
    l.instances.push(tinted(
        MeshId::Plane,
        Mat4::from_scale_rotation_translation(
            Vec3::new(w, 1.0, layout::ROOM_DEPTH),
            Quat::IDENTITY,
            centre,
        ),
        FLOOR,
    ));
    l.instances.push(tinted(
        MeshId::Box,
        Mat4::from_scale_rotation_translation(
            Vec3::new(w, 3.0, 0.1),
            Quat::IDENTITY,
            f.origin + Vec3::new(w / 2.0, 1.5, -0.05),
        ),
        WALL,
    ));
    for (k, seed) in seeds.iter().enumerate() {
        let x = desk_x(k);
        let (droot, yaw) = layout::desk_root(&slot, x);
        let dw = layout::desk_width(screen.x);
        l.instances.push(piece(
            MeshId::Desk,
            Mat4::from_scale_rotation_translation(
                Vec3::new(dw / 1.6, 1.0, 1.0),
                Quat::from_rotation_y(-yaw),
                droot,
            ),
        ));
        let (croot, cyaw) = layout::chair_root(&slot, x);
        l.instances.push(piece(MeshId::Chair, root(croot, cyaw)));
        let (froot, fyaw) = layout::monitor_root(&slot, x, screen);
        let lift = layout::stack_bottom(1, screen) - layout::DESK_TOP;
        let foot = root(froot, fyaw);
        rig::monitor(foot, screen, lift, &mut l.instances);
        // the screen: a lit panel standing in for the cat's terminal
        let s =
            rig::monitor_screen(foot, screen, lift) * Mat4::from_scale(Vec3::new(1.0, 1.0, 0.004));
        l.instances.push(Instance {
            tint: [60, 64, 80, 255],
            emissive: 1.0,
            ..piece(MeshId::Box, s)
        });
        let seat = layout::cat_seat(&slot, x);
        let j = if talk == Some(k) {
            talking()
        } else if k % 2 == 0 {
            typing()
        } else {
            sit()
        };
        let cam = l.camera.pose.pos;
        let mut cat = Vec::new();
        rig::cat_instances(
            &j,
            &Coat::from_seed(*seed),
            root(seat.pos, seat.yaw),
            Some([226, 120, 120, 255]),
            0.0,
            &mut cat,
        );
        let fade = layout::fade_for(cam, seat.pos + Vec3::Y * 0.3);
        l.instances
            .extend(cat.into_iter().map(|i| Instance { fade, ..i }));
        l.blobs.push(Blob {
            center: droot + Vec3::Y * 0.01,
            radius: dw * 0.5,
            alpha: 0.25,
        });
        l.blobs.push(Blob {
            center: croot + Vec3::Y * 0.01,
            radius: 0.3,
            alpha: 0.3,
        });
    }
    l
}

/// The composition reference: one desk, its chair, monitor and seated cat, from its NAV station.
pub fn desk_monitor_cat(aspect: f32) -> DrawList {
    office(aspect, &[11, 23, 37], 1, None)
}
