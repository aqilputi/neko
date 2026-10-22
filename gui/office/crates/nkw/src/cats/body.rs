//! A villager's body: `nk_cats::catpose` joints mapped 1:1 onto
//! `nkw_gl::rig` (S10), whose coats and instances these are. The root of a
//! seated cat is its seat top; of a walker, the floor.

use glam::{Mat4, Quat, Vec3};
use nk_cats::catpose::CatParams;
pub use nkw_gl::rig::{CatJoints, Coat, Ear, Pattern, cat_instances, kitten_instances};

/// The rig's root above `layout::cat_seat` (none: the seat point is the
/// root, `rig`'s convention).
pub const SEAT_TOP: f32 = 0.0;
/// Where the cat's middle is above its root (its fade is measured there,
/// so the whole cat dithers out at once).
pub const CAT_CENTRE: f32 = 0.3;
/// A seated cat's face above its root.
pub const FACE_Y: f32 = 0.52;

/// The rig's joints of a pose's params (the same fields, the same units).
pub fn joints(p: &CatParams) -> CatJoints {
    CatJoints {
        body_y: p.body_y,
        body_rot_x: p.body_rot_x,
        body_scale: p.body_scale,
        head_y: p.head_y,
        head_z: p.head_z,
        head_rot_x: p.head_rot_x,
        head_rot_y: p.head_rot_y,
        head_rot_z: p.head_rot_z,
        ears_back: p.ears_back,
        eyes_open: p.eyes_open,
        leg_l: p.leg_l,
        leg_r: p.leg_r,
        tail_pitch: p.tail_pitch,
        tail_sway: p.tail_sway,
        tail_curl: p.tail_curl,
        tail_scale: p.tail_scale,
    }
}

/// The root of something at `pos` facing `yaw` (the rig's convention: it
/// looks along the root's −z).
pub fn root(pos: Vec3, yaw: f32) -> Mat4 {
    Mat4::from_rotation_translation(Quat::from_rotation_y(-yaw), pos)
}

#[cfg(test)]
mod tests {
    use super::*;
    use nk_cats::catpose::{SIT, pose_params};
    use nk_cats::model::Pose;
    use nkw_gl::draw::MeshId;

    #[test]
    fn sit_is_the_rig_default() {
        assert_eq!(joints(&SIT), CatJoints::default());
        let t = joints(&pose_params(Pose::Typing));
        assert_eq!(t.leg_l, pose_params(Pose::Typing).leg_l);
    }

    #[test]
    fn the_root_faces_its_yaw() {
        for yaw in [0.0f32, 1.0, -2.5] {
            let f = root(Vec3::ZERO, yaw).transform_vector3(Vec3::NEG_Z);
            let want = nkw_core::layout::facing(yaw);
            assert!(f.distance(want) < 1e-5, "{yaw}");
        }
    }

    #[test]
    fn a_villager_is_one_head_and_a_coat() {
        let mut v = Vec::new();
        let c = Coat::from_seed(7);
        cat_instances(&joints(&SIT), &c, Mat4::IDENTITY, None, 0.0, &mut v);
        assert_eq!(v.iter().filter(|i| i.mesh == MeshId::CatSkull).count(), 1);
        assert!(v.iter().any(|i| i.tint == c.body));
        let skull = v.iter().find(|i| i.mesh == MeshId::CatSkull).unwrap();
        let y = skull.model.transform_point3(Vec3::ZERO).y;
        assert!((y - FACE_Y).abs() < 0.08, "the face near FACE_Y: {y}");
    }
}
