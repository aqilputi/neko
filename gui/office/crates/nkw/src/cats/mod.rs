//! The cats: core link, villagers, monitors, CONVERSA, inbox, postman,
//! receptionist (§6). P4 fills this in wave 2b; the stub only draws phase
//! 0's placeholder cat on the chair of each cat desk (back to the camera,
//! its head below the screen).

use glam::{Mat4, Quat, Vec3};
use nkw_core::{CatId, layout};
use nkw_gl::draw::{Blob, DrawList, Instance, MeshId};
use serde_json::Value;
use smithay::backend::renderer::element::memory::MemoryRenderBuffer;

use crate::ext::{Ctx, DialogView, Plugin};
use crate::router::KeyEv;

/// Placeholder coats, by cat id.
const COATS: [[u8; 4]; 2] = [[0xf0, 0xa8, 0x5a, 0xff], [0x8c, 0x8c, 0x96, 0xff]];

/// The placeholder's size: toy proportions, so the head clears the chair's
/// back (its top stays under `EYE − 0.25`).
pub const CAT_SCALE: f32 = 1.25;

/// The seated placeholder at scale 1, relative to the seat top (3 cm above
/// the hips): mesh, offset, pitch (radians, about the cat's x axis). The
/// body stands upright on the seat, the head on top of it, the tail hangs
/// behind.
const SEATED: [(MeshId, Vec3, f32); 5] = [
    (
        MeshId::CatBody,
        Vec3::new(0.0, 0.19, 0.0),
        std::f32::consts::FRAC_PI_2,
    ),
    (MeshId::CatHead, Vec3::new(0.0, 0.47, -0.01), 0.0),
    (MeshId::CatEar, Vec3::new(-0.07, 0.59, 0.0), 0.0),
    (MeshId::CatEar, Vec3::new(0.07, 0.59, 0.0), 0.0),
    (MeshId::CatTail, Vec3::new(0.0, 0.06, 0.12), 0.9),
];
/// The seat top above the hips (`Chair`: seat 6 cm thick at `SEAT_H`).
const SEAT_TOP: f32 = 0.03;

/// Where the placeholder's middle is above the hips (its fade is measured
/// from there, so the whole cat dithers out at once).
pub const CAT_CENTRE: f32 = 0.3;

pub struct Cats {}

impl Cats {
    pub fn new(cli: &crate::cli::Cli) -> Cats {
        let _ = cli;
        Cats {}
    }

    /// CONVERSA with purpose `Session`.
    pub fn dialog(&self) -> Option<DialogView> {
        None
    }

    /// FOCO(CatTerm): the cat's terminal and its position (logical px).
    pub fn focus_buffer(
        &mut self,
        cat: CatId,
        ctx: &Ctx,
    ) -> Option<(MemoryRenderBuffer, (i32, i32))> {
        let _ = (cat, ctx);
        None
    }

    /// FOCO(CatTerm) + `Route::Forward`: a key for the cat's pane.
    pub fn key_to_pane(&mut self, cat: CatId, ev: &KeyEv) {
        let _ = (cat, ev);
    }
}

/// The placeholder cat's instances on `seat`, faded as a whole by `fade`.
pub fn placeholder(cat: CatId, seat: &nkw_core::Pose, fade: f32, list: &mut DrawList) {
    let tint = COATS[(cat.0 as usize + 1) % COATS.len()];
    let turn = Quat::from_rotation_y(-seat.yaw);
    let base = seat.pos + Vec3::Y * SEAT_TOP;
    for (mesh, off, pitch) in SEATED {
        let m = Mat4::from_scale_rotation_translation(
            Vec3::splat(CAT_SCALE),
            turn * Quat::from_rotation_x(pitch),
            base + turn * (off * CAT_SCALE),
        );
        list.instances.push(Instance {
            mesh,
            model: m,
            tint,
            emissive: 0.0,
            fade,
        });
    }
    let floor = Vec3::new(seat.pos.x, 0.01, seat.pos.z);
    list.blobs.push(Blob {
        center: floor,
        radius: 0.32,
        alpha: 0.35,
    });
}

impl Plugin for Cats {
    fn name(&self) -> &'static str {
        "cats"
    }

    fn draw(&self, ctx: &Ctx, list: &mut DrawList) {
        let w = ctx.world;
        let cam = list.camera.pose.pos;
        for room in w.rooms.iter().filter(|r| ctx.visible_rooms.contains(&r.id)) {
            for (d, desk) in room.desks.iter().enumerate() {
                let (Some(cat), Some(seat)) = (desk.cat, w.cat_seat(room.id, d)) else {
                    continue;
                };
                let fade = layout::fade_for(cam, seat.pos + Vec3::Y * CAT_CENTRE);
                placeholder(cat, &seat, fade, list);
            }
        }
    }

    fn dump(&self) -> Value {
        serde_json::json!({"stub": true})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ext::Ctx;
    use nkw_core::RoomId;
    use std::collections::BTreeMap;

    fn draw(rooms: &[RoomId]) -> DrawList {
        let w = crate::demo::world(
            nkw_core::Output {
                width_px: 1280,
                height_px: 720,
                scale: 1.0,
            },
            crate::world::Opts {
                reduced_motion: false,
            },
        );
        let pids = BTreeMap::new();
        let ctx = Ctx {
            world: &w,
            now: 0.0,
            frame: 0,
            reduced_motion: false,
            demo: true,
            scale: 1.0,
            size_px: [1280, 720],
            pids: &pids,
            visible_rooms: rooms,
            line: None,
        };
        let mut list = crate::scene::draw_list(&w, &|_| None, rooms);
        let before = list.instances.len();
        Cats::new(&crate::cli::Cli::default()).draw(&ctx, &mut list);
        list.instances.drain(..before);
        list
    }

    #[test]
    fn one_cat_per_cat_desk_of_the_visible_rooms() {
        let l = draw(&[RoomId(0), RoomId(1), RoomId(2)]);
        let heads: Vec<_> = l
            .instances
            .iter()
            .filter(|i| i.mesh == MeshId::CatHead)
            .collect();
        assert_eq!(heads.len(), 2, "the demo has two cats");
        assert_eq!(draw(&[RoomId(2)]).instances.len(), 0, "no cat in demo");
        assert_eq!(draw(&[RoomId(1)]).instances.len(), SEATED.len());
    }

    #[test]
    fn the_cat_sits_low_with_its_back_to_you() {
        // the reception's cat, seen from the demo's first station
        let l = draw(&[RoomId(0)]);
        let head = l
            .instances
            .iter()
            .find(|i| i.mesh == MeshId::CatHead)
            .unwrap();
        let top = head.model.transform_point3(Vec3::new(0.0, 0.105, 0.0)).y;
        assert!(top <= layout::EYE - 0.25, "head top {top}");
        let bottom = head.model.transform_point3(Vec3::new(0.0, -0.105, 0.0)).y;
        assert!(
            bottom < 0.95 && top > 1.1,
            "most of it above the chair's back: {top}"
        );
        // the head's front (−z, the nose) points away from the camera
        let nose = head.model.transform_vector3(Vec3::NEG_Z);
        assert!(nose.dot(l.camera.pose.forward()) > 0.9);
        assert!(
            l.instances.iter().all(|i| i.fade == 0.0),
            "far from the camera"
        );
    }

    /// The glide into FOCO on a cat's terminal passes over the cat on its
    /// chair: sampled at 10 points, the cat is never visible inside the
    /// near zone (no popping through the near plane), and wherever the
    /// camera is within `FADE_NEAR / 2` of it, it is fully dithered out.
    #[test]
    fn the_glide_into_focus_fades_the_cat_it_passes() {
        use nkw_core::{Action, Target};
        for (w_px, h_px, scale) in [(1280, 720, 1.0), (1920, 1080, 1.0), (1280, 720, 1.25)] {
            let mut w = crate::demo::world(
                nkw_core::Output {
                    width_px: w_px,
                    height_px: h_px,
                    scale,
                },
                crate::world::Opts {
                    reduced_motion: false,
                },
            );
            let cat = CatId(1);
            w.action(Action::Focus(Target::CatTerm(cat)), 0.0);
            assert!(w.gliding());
            let end = w.one_to_one_target(Target::CatTerm(cat)).unwrap().pose;
            let start = w.camera;
            let seat = w.cat_seat(RoomId(0), 0).unwrap();
            let centre = seat.pos + Vec3::Y * CAT_CENTRE;
            let pids = BTreeMap::new();
            let cats = Cats::new(&crate::cli::Cli::default());
            let mut closest = f32::MAX;
            for i in 1..=10 {
                let now = 0.26 * i as f64 / 10.0;
                w.tick(now);
                let ctx = Ctx {
                    world: &w,
                    now,
                    frame: i,
                    reduced_motion: false,
                    demo: true,
                    scale: scale as f64,
                    size_px: [w_px, h_px],
                    pids: &pids,
                    visible_rooms: &[RoomId(0)],
                    line: None,
                };
                let mut list = crate::scene::draw_list(&w, &|_| None, &[RoomId(0)]);
                cats.draw(&ctx, &mut list);
                let cam = w.camera.pos;
                let d = cam.distance(centre);
                closest = closest.min(d);
                for inst in list.instances.iter().filter(|i| {
                    matches!(
                        i.mesh,
                        MeshId::CatBody | MeshId::CatHead | MeshId::CatEar | MeshId::CatTail
                    )
                }) {
                    if d <= layout::FADE_NEAR * 0.5 {
                        assert_eq!(inst.fade, 1.0, "inside the cat at sample {i}");
                    }
                    let part = cam.distance(inst.model.transform_point3(Vec3::ZERO));
                    assert!(
                        inst.fade == 1.0 || part > crate::scene::NEAR + 0.2,
                        "a visible part at {part} m (sample {i})"
                    );
                }
            }
            assert_eq!(w.camera, end, "arrived");
            // the path really goes past the cat: from behind it to in front of it
            let depth = |p: Vec3| (p - seat.pos).dot(-layout::facing(seat.yaw));
            assert!(depth(start.pos) > 0.0 && depth(end.pos) < depth(start.pos));
            assert!(closest < start.pos.distance(centre));
        }
        // a path straight through it dithers it out before the camera is in it
        let c = Vec3::new(0.0, 0.75, 0.0);
        let mut last = 0.0;
        for i in 0..=20 {
            let cam = c + Vec3::new(0.0, 0.0, 1.0 - i as f32 * 0.05);
            let f = layout::fade_for(cam, c);
            assert!(f >= last, "no popping back");
            if cam.distance(c) <= 0.3 {
                assert_eq!(f, 1.0);
            }
            last = f;
        }
        assert_eq!(last, 1.0);
    }
}
