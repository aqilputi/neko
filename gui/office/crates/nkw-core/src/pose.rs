//! A camera pose: eye position, yaw and pitch. There is no roll, by
//! construction (§3.11: the horizon is always level).
//!
//! Conventions (metres, y up): yaw 0 looks down -z, positive yaw turns
//! right (towards +x); positive pitch looks up.

use glam::{Mat4, Vec2, Vec3};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Default, Serialize, Deserialize)]
pub struct Pose {
    pub pos: Vec3,
    /// radians, positive turns right
    pub yaw: f32,
    /// radians, positive looks up
    pub pitch: f32,
}

impl Pose {
    pub fn new(pos: Vec3, yaw: f32, pitch: f32) -> Pose {
        Pose { pos, yaw, pitch }
    }

    /// Unit vector the camera looks along.
    pub fn forward(&self) -> Vec3 {
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        Vec3::new(sy * cp, sp, -cy * cp)
    }

    /// Unit vector to the camera's right (always horizontal: no roll).
    pub fn right(&self) -> Vec3 {
        let (sy, cy) = self.yaw.sin_cos();
        Vec3::new(cy, 0.0, sy)
    }

    /// Unit vector up in the image (tilts with pitch).
    pub fn up(&self) -> Vec3 {
        self.right().cross(self.forward())
    }

    /// The view matrix (world → camera, right-handed, looking down -z).
    pub fn view(&self) -> Mat4 {
        glam::camera::rh::view::look_to_mat4(self.pos, self.forward(), Vec3::Y)
    }

    /// The horizontal facing direction on the floor (x, z), unit length.
    pub fn heading(&self) -> Vec2 {
        let (sy, cy) = self.yaw.sin_cos();
        Vec2::new(sy, -cy)
    }
}

/// `a - b` wrapped to (-π, π]: the shortest turn from `b` to `a`.
pub fn angle_diff(a: f32, b: f32) -> f32 {
    use std::f32::consts::{PI, TAU};
    let d = (a - b).rem_euclid(TAU);
    if d > PI { d - TAU } else { d }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::{FRAC_PI_2, PI};

    fn close(a: Vec3, b: Vec3) -> bool {
        (a - b).length() < 1e-5
    }

    #[test]
    fn axes_at_rest_and_turned() {
        let p = Pose::default();
        assert!(close(p.forward(), Vec3::NEG_Z));
        assert!(close(p.right(), Vec3::X));
        assert!(close(p.up(), Vec3::Y));
        let r = Pose::new(Vec3::ZERO, FRAC_PI_2, 0.0);
        assert!(close(r.forward(), Vec3::X), "yaw +90° looks at +x");
        assert!(close(r.right(), Vec3::Z));
        assert!((r.heading() - Vec2::new(1.0, 0.0)).length() < 1e-5);
    }

    #[test]
    fn pitch_tilts_forward_and_up_but_never_rolls() {
        let p = Pose::new(Vec3::ZERO, 0.3, 0.4);
        assert!(p.forward().y > 0.0);
        assert!(p.right().y.abs() < 1e-6, "no roll");
        assert!(p.forward().dot(p.up()).abs() < 1e-6);
        assert!(p.forward().dot(p.right()).abs() < 1e-6);
        assert!((p.heading().length() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn view_maps_the_eye_to_origin_and_forward_to_neg_z() {
        let p = Pose::new(Vec3::new(1.0, 1.5, 2.0), 0.7, -0.2);
        let v = p.view();
        assert!(close(v.transform_point3(p.pos), Vec3::ZERO));
        let ahead = v.transform_point3(p.pos + p.forward() * 2.0);
        assert!(close(ahead, Vec3::new(0.0, 0.0, -2.0)));
        let right = v.transform_point3(p.pos + p.right());
        assert!(close(right, Vec3::X));
    }

    #[test]
    fn angle_diff_takes_the_short_way() {
        assert!((angle_diff(0.1, -0.1) - 0.2).abs() < 1e-6);
        assert!((angle_diff(PI - 0.1, -PI + 0.1) + 0.2).abs() < 1e-5);
        assert!((angle_diff(3.0 * PI, 0.0).abs() - PI).abs() < 1e-5);
    }
}
