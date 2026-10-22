//! The world pass's draw list, filled from the world every pass (§5.1):
//! rooms (floor, walls), the hall, desks, monitor frames, cat
//! placeholders, blob shadows and the windows as textured quads.

use glam::{Mat4, Quat, Vec3};
use nkw_core::ScreenId;
use nkw_core::strip::Side;
use nkw_gl::draw::{Blob, Camera, ClientTex, DrawList, Instance, MeshId, ScreenQuad, sky_for_band};

use crate::world::{HALL_W, ROOM_DEPTH, World, facing};

/// Wall height, metres.
pub const WALL_H: f32 = 2.8;
/// Near and far planes, metres (fog ends at 40).
pub const NEAR: f32 = 0.05;
pub const FAR: f32 = 60.0;
/// The time-of-day band of phase 0 (the clock adapter comes later).
pub const BAND: u8 = 3;
/// Dimming of the world around a FOCO window (a 2D layer does it for the
/// whole frame; the quads keep `dim: 0` so they are not darkened twice).
pub const FOCUS_DIM: f32 = 0.2;

/// Room floor tints (pastel; one per room, cycling).
const FLOORS: [[u8; 4]; 4] = [
    [0xe8, 0xd5, 0xb5, 0xff],
    [0xc9, 0xdd, 0xc0, 0xff],
    [0xd9, 0xcc, 0xe6, 0xff],
    [0xf2, 0xc9, 0xb8, 0xff],
];
const WALL: [u8; 4] = [0xf6, 0xee, 0xdf, 0xff];
const HALL: [u8; 4] = [0xd8, 0xc8, 0xac, 0xff];
const WOOD: [u8; 4] = [0xb9, 0x8a, 0x5e, 0xff];
const FRAME: [u8; 4] = [0x6b, 0x5a, 0x4e, 0xff];
const CATS: [[u8; 4]; 2] = [[0xf0, 0xa8, 0x5a, 0xff], [0x8c, 0x8c, 0x96, 0xff]];

fn model(pos: Vec3, yaw: f32, scale: Vec3) -> Mat4 {
    Mat4::from_scale_rotation_translation(scale, Quat::from_rotation_y(-yaw), pos)
}

fn inst(mesh: MeshId, m: Mat4, tint: [u8; 4]) -> Instance {
    Instance {
        mesh,
        model: m,
        tint,
        emissive: 0.0,
        fade: 0.0,
    }
}

/// The draw list of the world as it is now. `tex` gives the GL texture of
/// a window (none: not drawn yet, the quad is skipped).
pub fn draw_list(w: &World, tex: &dyn Fn(ScreenId) -> Option<ClientTex>) -> DrawList {
    let aspect = w.out.width_px as f32 / w.out.height_px.max(1) as f32;
    let camera = Camera {
        pose: w.camera,
        fov_y: crate::world::FOV_Y,
        near: NEAR,
        far: FAR,
        aspect,
    };
    let mut list = DrawList::empty(camera, sky_for_band(BAND));
    let slots = w.slots();
    let hidden = w.focus_2d().map(|(id, _)| id);

    // the hall
    let end = slots.iter().map(|s| s.x0 + s.len).fold(0.0, f32::max);
    list.instances.push(inst(
        MeshId::Box,
        model(
            Vec3::new(end / 2.0, -0.025, 0.0),
            0.0,
            Vec3::new(end + 2.0, 0.05, HALL_W),
        ),
        HALL,
    ));

    let mut cat_n = 0;
    for (ri, (room, slot)) in w.rooms.iter().zip(&slots).enumerate() {
        let sign = if slot.side == Side::North { -1.0 } else { 1.0 };
        let zc = sign * (HALL_W / 2.0 + ROOM_DEPTH / 2.0);
        let xc = slot.x0 + slot.len / 2.0;
        let back = sign * (HALL_W / 2.0 + ROOM_DEPTH);
        list.instances.push(inst(
            MeshId::Box,
            model(
                Vec3::new(xc, -0.02, zc),
                0.0,
                Vec3::new(slot.len, 0.04, ROOM_DEPTH),
            ),
            FLOORS[ri % FLOORS.len()],
        ));
        // back wall and the two side walls
        list.instances.push(inst(
            MeshId::Box,
            model(
                Vec3::new(xc, WALL_H / 2.0, back),
                0.0,
                Vec3::new(slot.len, WALL_H, 0.1),
            ),
            WALL,
        ));
        for x in [slot.x0, slot.x0 + slot.len] {
            list.instances.push(inst(
                MeshId::Box,
                model(
                    Vec3::new(x, WALL_H / 2.0, zc),
                    0.0,
                    Vec3::new(0.1, WALL_H, ROOM_DEPTH),
                ),
                WALL,
            ));
        }

        for (di, desk) in room.desks.iter().enumerate() {
            let place = w.screen_place(ri, di, 0);
            let toward = -facing(place.yaw); // from the wall into the room
            let desk_pos = Vec3::new(place.center.x, 0.0, place.center.z) + toward * 0.25;
            list.instances.push(inst(
                MeshId::Desk,
                model(desk_pos, place.yaw, Vec3::ONE),
                WOOD,
            ));
            list.blobs.push(Blob {
                center: desk_pos + Vec3::Y * 0.01,
                radius: 0.6,
                alpha: 0.25,
            });
            for (k, &sid) in desk.screens.iter().enumerate() {
                let p = w.screen_place(ri, di, k);
                let size = w.screen_logical(ri, di, k) * nkw_core::onetoone::M_PER_PX;
                list.instances.push(inst(
                    MeshId::MonitorFrame,
                    model(
                        p.center - toward * 0.02,
                        p.yaw,
                        Vec3::new(size.x + 0.04, size.y + 0.04, 0.03),
                    ),
                    FRAME,
                ));
                if hidden == Some(sid) {
                    continue;
                }
                if let Some(t) = tex(sid) {
                    list.screens.push(ScreenQuad {
                        model: model(p.center, p.yaw, Vec3::new(size.x, size.y, 1.0)),
                        tex: t,
                        dim: 0.0,
                        selected: w.selected() == Some(sid) && !w.frozen,
                    });
                }
            }
            if desk.cat.is_some() {
                // the cat sits between the camera and its screen, back to you
                let c = desk_pos + toward * 0.45;
                let tint = CATS[cat_n % CATS.len()];
                cat_n += 1;
                let parts = [
                    (MeshId::CatBody, Vec3::new(0.0, 0.55, 0.0)),
                    (MeshId::CatHead, Vec3::new(0.0, 0.85, -0.02)),
                    (MeshId::CatEar, Vec3::new(-0.07, 1.0, -0.02)),
                    (MeshId::CatEar, Vec3::new(0.07, 1.0, -0.02)),
                    (MeshId::CatTail, Vec3::new(0.0, 0.5, 0.18)),
                ];
                for (mesh, off) in parts {
                    let off = Quat::from_rotation_y(-place.yaw) * off;
                    list.instances
                        .push(inst(mesh, model(c + off, place.yaw, Vec3::ONE), tint));
                }
                list.blobs.push(Blob {
                    center: c + Vec3::Y * 0.01,
                    radius: 0.35,
                    alpha: 0.35,
                });
            }
        }
    }
    list
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::tests::world;
    use crate::world::Cmd;

    fn tex(id: ScreenId) -> Option<ClientTex> {
        Some(ClientTex {
            id: id.0 + 100,
            external: false,
            size: [624, 720],
            y_flip: false,
            opaque: true,
        })
    }

    #[test]
    fn one_quad_per_textured_screen() {
        let w = world(&[&[1, 2], &[3]]);
        let l = draw_list(&w, &tex);
        assert_eq!(l.screens.len(), 3);
        assert_eq!(l.screens.iter().filter(|s| s.selected).count(), 1);
        assert!(l.screens.iter().all(|s| s.dim == 0.0));
        let none = draw_list(&w, &|_| None);
        assert!(none.screens.is_empty());
        assert_eq!(
            none.instances.len(),
            l.instances.len(),
            "frames still drawn"
        );
    }

    #[test]
    fn quad_matches_the_screen_place() {
        let w = world(&[&[1]]);
        let l = draw_list(&w, &tex);
        let q = &l.screens[0];
        let p = w.screen_place(0, 0, 0);
        assert!((q.model.transform_point3(Vec3::ZERO) - p.center).length() < 1e-5);
        // unit quad corner (+0.5, +0.5) → top-right of a 0.624×0.72 m screen
        let tr = q.model.transform_point3(Vec3::new(0.5, 0.5, 0.0));
        assert!((tr - p.center - Vec3::new(0.312, 0.36, 0.0)).length() < 1e-5);
    }

    #[test]
    fn focus_hides_its_quad() {
        let mut w = world(&[&[1, 2]]);
        w.command(Cmd::Focus, 0.0);
        w.tick(1e9);
        let l = draw_list(&w, &tex);
        assert_eq!(l.screens.len(), 1);
        assert_eq!(l.screens[0].tex.id, 102);
        assert_eq!(l.screens[0].dim, 0.0, "the 2D dim layer darkens it");
    }

    #[test]
    fn a_typical_room_stays_in_budget() {
        let w = crate::demo::world(
            nkw_core::Output {
                width_px: 1920,
                height_px: 1080,
                scale: 1.0,
            },
            crate::world::Opts {
                reduced_motion: false,
            },
        );
        let l = draw_list(&w, &tex);
        assert!(l.instances.len() < 80, "{}", l.instances.len());
    }
}
