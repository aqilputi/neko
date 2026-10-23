//! The world pass's draw list, filled from the world every pass (§5.1):
//! rooms (floor, walls) and the hall; for the visible rooms, each desk with
//! its chair and its monitor (bezel, stand, foot) on top, the windows as
//! textured quads on the monitors, blob shadows. Geometry from
//! `nkw_core::layout` (§4.2: the monitor ON the desk, the chair at its
//! front edge). The cats are the cats plugin's (`Plugin::draw`).

use glam::{Mat4, Quat, Vec2, Vec3};
use nkw_core::onetoone::M_PER_PX;
use nkw_core::{RoomId, ScreenId, ScreenPlace, layout};
use nkw_gl::draw::{Blob, Camera, ClientTex, DrawList, Instance, MeshId, ScreenQuad, sky_for_band};

use crate::world::World;

/// Wall height, metres (the shell's, S17.3).
pub use crate::shell::WALL_H;
/// Near and far planes, metres (fog ends at 40).
pub const NEAR: f32 = 0.05;
pub const FAR: f32 = 60.0;
/// Dimming of the world around a FOCO window (a 2D layer does it for the
/// whole frame; the quads keep `dim: 0` so they are not darkened twice).
pub const FOCUS_DIM: f32 = 0.2;
/// The monitor's bezel around the screen, metres.
pub const BEZEL: f32 = 0.02;
/// The placeholder stand: neck diameter, foot depth and thickness, metres.
pub const NECK_D: f32 = 0.05;
pub const FOOT_DEPTH: f32 = 0.22;
pub const FOOT_H: f32 = 0.02;

const WOOD: [u8; 4] = [0xff, 0xff, 0xff, 0xff];
const FRAME: [u8; 4] = [0x6b, 0x5a, 0x4e, 0xff];
const CHAIR: [u8; 4] = [0xff, 0xff, 0xff, 0xff];
/// A monitor with nothing on it (a cat's terminal before P4, an empty desk).
const SCREEN_OFF: [u8; 4] = [0x3a, 0x3f, 0x4f, 0xff];

/// The model matrix of a thing at `pos`, turned to `yaw`, scaled.
pub fn model(pos: Vec3, yaw: f32, scale: Vec3) -> Mat4 {
    Mat4::from_scale_rotation_translation(scale, Quat::from_rotation_y(-yaw), pos)
}

/// The model matrix of a screen (or its bezel) at a place, its face
/// towards the camera that looks at it (yaw, then pitch).
pub fn screen_model(p: &ScreenPlace, scale: Vec3) -> Mat4 {
    let rot = Quat::from_rotation_y(-p.yaw) * Quat::from_rotation_x(p.pitch);
    Mat4::from_scale_rotation_translation(scale, rot, p.center)
}

/// An instance, its fade from the camera at `cam` measured at `centre`.
fn inst(mesh: MeshId, m: Mat4, tint: [u8; 4], cam: Vec3, centre: Vec3) -> Instance {
    Instance {
        mesh,
        model: m,
        tint,
        emissive: 0.0,
        fade: layout::fade_for(cam, centre),
    }
}

/// The middle of a mesh's bounds in its local frame (furniture stands on
/// y = 0; primitives and the bezel are centred).
fn local_centre(mesh: MeshId) -> Vec3 {
    match mesh {
        MeshId::Desk => Vec3::new(0.0, 0.375, 0.0),
        MeshId::Chair => Vec3::new(0.0, 0.475, 0.0),
        _ => Vec3::ZERO,
    }
}

pub(crate) fn push(list: &mut DrawList, mesh: MeshId, m: Mat4, tint: [u8; 4]) {
    let cam = list.camera.pose.pos;
    let centre = m.transform_point3(local_centre(mesh));
    list.instances.push(inst(mesh, m, tint, cam, centre));
}

/// The draw list of the world as it is now. `tex` gives the GL texture of
/// a window (none: not drawn yet, the quad is skipped); `visible`: the
/// rooms whose furniture and screens are drawn (every room's shell is).
pub fn draw_list(
    w: &World,
    tex: &dyn Fn(ScreenId) -> Option<ClientTex>,
    visible: &[RoomId],
) -> DrawList {
    let mut list = DrawList::empty(camera_of(w), sky_for_band(w.band));
    draw_list_into(w, tex, visible, &mut list);
    list
}

/// The world's camera for the pass.
fn camera_of(w: &World) -> Camera {
    Camera {
        pose: w.camera,
        fov_y: crate::world::FOV_Y,
        near: NEAR,
        far: FAR,
        aspect: w.out.width_px as f32 / w.out.height_px.max(1) as f32,
    }
}

/// [`draw_list`] into a list the caller keeps (cleared first; its vectors
/// keep their capacity: no allocation per pass once warm, S4).
pub fn draw_list_into(
    w: &World,
    tex: &dyn Fn(ScreenId) -> Option<ClientTex>,
    visible: &[RoomId],
    list: &mut DrawList,
) {
    list.camera = camera_of(w);
    list.sky = sky_for_band(w.band);
    list.instances.clear();
    list.screens.clear();
    list.blobs.clear();
    let slots = w.slots();
    let hidden = w.focus_2d().map(|(id, _)| id);

    crate::shell::hall(list, &slots, w.band);

    for (ri, (room, slot)) in w.rooms.iter().zip(&slots).enumerate() {
        let shown = visible.contains(&room.id);
        crate::shell::room(
            list,
            &crate::shell::Shell {
                slot,
                index: ri,
                kind: w.room_kind(room.id),
                band: w.band,
                visible: shown,
            },
        );
        if !shown {
            continue;
        }

        for (di, desk) in room.desks.iter().enumerate() {
            let x = w.desk_x(ri, di);
            let first = w.screen_logical(ri, di, 0) * M_PER_PX;
            // the desk, as wide as its screen and some elbow room
            let (root, yaw) = layout::desk_root(slot, x);
            let dw = layout::desk_width(first.x);
            push(
                list,
                MeshId::Desk,
                model(root, yaw, Vec3::new(dw / 1.6, 1.0, 1.0)),
                WOOD,
            );
            list.blobs.push(Blob {
                center: root + Vec3::Y * 0.01,
                radius: dw * 0.5,
                alpha: 0.25,
            });
            // the chair at its front edge
            let (chair, cyaw) = layout::chair_root(slot, x);
            push(list, MeshId::Chair, model(chair, cyaw, Vec3::ONE), CHAIR);
            list.blobs.push(Blob {
                center: chair + Vec3::Y * 0.01,
                radius: 0.3,
                alpha: 0.25,
            });

            // the monitor: a stand on the desk, a bezel per screen
            // loose dialogs/air screens at the end of `desk.screens` do
            // not raise the stand
            let n = w.stack_len(ri, di);
            let lift = layout::stack_bottom(n, first) - layout::DESK_TOP;
            monitor_stand(list, slot, x, first, lift);
            if desk.screens.is_empty() {
                let p = layout::monitor_place(slot, x, first);
                bezel(list, &p, first);
                push(
                    list,
                    MeshId::Box,
                    screen_model(&p, Vec3::new(first.x, first.y, 0.004)),
                    SCREEN_OFF,
                );
            }
            for (k, &sid) in desk.screens.iter().enumerate() {
                let p = w.screen_place(ri, di, k);
                let size = w.screen_logical(ri, di, k) * M_PER_PX;
                bezel(list, &p, size);
                if hidden == Some(sid) {
                    continue;
                }
                if let Some(t) = tex(sid) {
                    list.screens.push(ScreenQuad {
                        model: screen_model(&p, Vec3::new(size.x, size.y, 1.0)),
                        tex: t,
                        dim: 0.0,
                        selected: w.selected() == Some(sid) && !w.frozen,
                    });
                }
            }
        }
    }
}

/// The bezel behind a screen of `size` metres.
fn bezel(list: &mut DrawList, p: &ScreenPlace, size: Vec2) {
    let s = Vec3::new(size.x + 2.0 * BEZEL, size.y + 2.0 * BEZEL, 1.0);
    push(list, MeshId::MonitorFrame, screen_model(p, s), FRAME);
}

/// The placeholder stand (until `nkw_gl::rig::monitor`): a neck from the
/// desk top up to the lowest screen, and a foot on the desk.
fn monitor_stand(list: &mut DrawList, slot: &nkw_core::RoomSlot, x: f32, size: Vec2, lift: f32) {
    // P6's monitor body (S10): neck + foot; the scene keeps one bezel per screen
    let (foot, yaw) = layout::monitor_root(slot, x, size);
    let root = Mat4::from_rotation_translation(Quat::from_rotation_y(-yaw), foot);
    let mut parts = Vec::with_capacity(2);
    nkw_gl::rig::monitor_stand(root, size, lift, &mut parts);
    let cam = list.camera.pose.pos;
    for mut i in parts {
        i.fade = layout::fade_for(cam, i.model.transform_point3(Vec3::ZERO));
        list.instances.push(i);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::tests::world;
    use crate::world::Cmd;

    const ALL: &[RoomId] = &[RoomId(0), RoomId(1), RoomId(2)];

    fn tex(id: ScreenId) -> Option<ClientTex> {
        Some(ClientTex {
            id: id.0 + 100,
            external: false,
            size: [624, 720],
            y_flip: false,
            opaque: true,
        })
    }

    fn count(l: &DrawList, mesh: MeshId) -> usize {
        l.instances.iter().filter(|i| i.mesh == mesh).count()
    }

    #[test]
    fn one_quad_per_textured_screen() {
        let w = world(&[&[1, 2], &[3]]);
        let l = draw_list(&w, &tex, ALL);
        assert_eq!(l.screens.len(), 3);
        assert_eq!(l.screens.iter().filter(|s| s.selected).count(), 1);
        assert!(l.screens.iter().all(|s| s.dim == 0.0));
        let none = draw_list(&w, &|_| None, ALL);
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
        let l = draw_list(&w, &tex, ALL);
        let q = &l.screens[0];
        let p = w.screen_place(0, 0, 0);
        assert!((q.model.transform_point3(Vec3::ZERO) - p.center).length() < 1e-5);
        // unit quad corner (+0.5, +0.5) → top-right of a 0.624×0.72 m screen
        let tr = q.model.transform_point3(Vec3::new(0.5, 0.5, 0.0));
        assert!((tr - p.center - Vec3::new(0.312, 0.36, 0.0)).length() < 1e-5);
    }

    #[test]
    fn stacked_quads_tilt_like_their_place() {
        let w = world(&[&[1, 2]]);
        let l = draw_list(&w, &tex, ALL);
        for (k, q) in l.screens.iter().enumerate() {
            let p = w.screen_place(0, 0, k);
            let pose = nkw_core::Pose::new(p.center, p.yaw, p.pitch);
            let n = q.model.transform_vector3(Vec3::Z).normalize();
            assert!((n + pose.forward()).length() < 1e-4, "faces its camera");
            let up = q.model.transform_vector3(Vec3::Y).normalize();
            assert!((up - pose.up()).length() < 1e-4);
        }
    }

    #[test]
    fn focus_hides_its_quad() {
        let mut w = world(&[&[1, 2]]);
        w.command(Cmd::Focus, 0.0);
        w.tick(1e9);
        let l = draw_list(&w, &tex, ALL);
        assert_eq!(l.screens.len(), 1);
        assert_eq!(l.screens[0].tex.id, 102);
        assert_eq!(l.screens[0].dim, 0.0, "the 2D dim layer darkens it");
    }

    fn demo(out: (u32, u32)) -> World {
        crate::demo::world(
            nkw_core::Output {
                width_px: out.0,
                height_px: out.1,
                scale: 1.0,
            },
            crate::world::Opts {
                reduced_motion: false,
            },
        )
    }

    #[test]
    fn every_desk_has_a_desk_a_chair_and_one_monitor_body() {
        let w = demo((1280, 720));
        let desks: usize = w.rooms.iter().map(|r| r.desks.len()).sum();
        let l = draw_list(&w, &tex, ALL);
        assert_eq!(count(&l, MeshId::Desk), desks);
        assert_eq!(count(&l, MeshId::Chair), desks);
        assert_eq!(count(&l, MeshId::MonitorStand), desks, "one stand each");
        assert_eq!(count(&l, MeshId::MonitorFoot), desks, "one foot each");
        assert_eq!(
            count(&l, MeshId::MonitorFrame),
            desks,
            "one screen each here"
        );
        assert_eq!(count(&l, MeshId::CatHead), 0, "the cats are the plugin's");
        // only the visible rooms get furniture
        let one = draw_list(&w, &tex, &[RoomId(1)]);
        assert_eq!(count(&one, MeshId::Desk), w.rooms[1].desks.len());
        assert_eq!(
            count(&one, MeshId::Box),
            count(&l, MeshId::Box) - (desks - 3),
        );
        assert_eq!(count(&one, MeshId::MonitorStand), w.rooms[1].desks.len());
    }

    #[test]
    fn the_monitor_stands_on_the_desk() {
        let w = world(&[&[1]]);
        let l = draw_list(&w, &tex, ALL);
        let neck = l
            .instances
            .iter()
            .find(|i| i.mesh == MeshId::MonitorStand)
            .unwrap();
        // the neck mesh runs y = 0..1 (kit convention)
        let (bot, top) = (
            neck.model.transform_point3(Vec3::ZERO).y,
            neck.model.transform_point3(Vec3::Y).y,
        );
        assert!((bot - layout::DESK_TOP).abs() < 1e-5, "{bot}");
        let p = w.screen_place(0, 0, 0);
        let screen_bottom = p.center.y - 0.36;
        assert!(
            top >= screen_bottom && top <= p.center.y + 0.36,
            "up into the screen: {top} vs bottom {screen_bottom}"
        );
        let desk = l.instances.iter().find(|i| i.mesh == MeshId::Desk).unwrap();
        let d = desk.model.transform_point3(Vec3::ZERO);
        let n = neck.model.transform_point3(Vec3::ZERO);
        assert!(
            (d.z - n.z).abs() < layout::DESK_DEPTH / 2.0,
            "over the desk top"
        );
    }

    #[test]
    fn a_typical_room_stays_in_budget() {
        // the plan's budget: ≤ 80 draws (one per distinct mesh) and ≤ 100k
        // triangles per room; the room shell adds instances, not draws
        let w = demo((1920, 1080));
        let l = draw_list(&w, &tex, ALL);
        let mut meshes: Vec<MeshId> = Vec::new();
        for i in &l.instances {
            if !meshes.contains(&i.mesh) {
                meshes.push(i.mesh);
            }
        }
        let draws = meshes.len() + l.screens.len();
        assert!(draws <= 80, "{draws} draws");
        assert!(
            l.triangle_estimate() <= 100_000,
            "{}",
            l.triangle_estimate()
        );
        assert!(l.instances.len() < 400, "{}", l.instances.len());
    }

    #[test]
    fn a_loose_dialog_does_not_raise_the_stand() {
        let stand_top = |w: &World| {
            let l = draw_list(w, &tex, ALL);
            let neck = l
                .instances
                .iter()
                .find(|i| i.mesh == MeshId::MonitorStand)
                .unwrap();
            neck.model.transform_point3(Vec3::Y).y
        };
        let mut w = world(&[&[1, 2]]);

        // the second screen becomes a dialog of the first: loose, at the end
        w.set_parent(ScreenId(2), Some(ScreenId(1)), 0.0);
        assert_eq!(w.stack_len(0, 0), 1);
        assert_eq!(w.rooms[0].desks[0].screens.len(), 2, "still on the desk");
        let one = world(&[&[1]]);
        assert!(
            (stand_top(&w) - stand_top(&one)).abs() < 1e-5,
            "the stand of a one-screen stack: {} vs {}",
            stand_top(&w),
            stand_top(&one)
        );
    }

    #[test]
    fn near_instances_fade_far_ones_do_not() {
        let mut w = world(&[&[1]]);
        let l = draw_list(&w, &tex, ALL);
        assert!(
            l.instances.iter().all(|i| i.fade == 0.0),
            "NAV: nothing near"
        );
        // the camera inside the chair
        let (chair, _) = layout::chair_root(&w.slots()[0], w.desk_x(0, 0));
        w.camera.pos = chair + Vec3::Y * 0.475;
        let l = draw_list(&w, &tex, ALL);
        let c = l
            .instances
            .iter()
            .find(|i| i.mesh == MeshId::Chair)
            .unwrap();
        assert_eq!(c.fade, 1.0);
        let wall = &l.instances[2];
        assert_eq!(wall.fade, 0.0);
    }
}
