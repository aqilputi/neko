//! The exact 1:1 camera pose (§4.2): from it, a screen's quad projects onto
//! precisely the 2D rectangle the window would get on the output, so the
//! quad and the 2D element can be swapped in one frame (§5.5).
//!
//! 1 logical px of window = 1 mm of world. With an output `H` physical px
//! high, scale `s` and vertical FOV `θ`, the focal length is
//! `f = (H/2)/tan(θ/2)` physical px; a plane at distance `d` shows
//! `size_m · f / d` px, and `size_logical · s` px is wanted, so
//! `d = 0.001 · f / s`. A sub-millimetre shift then puts the top-left corner
//! on a whole pixel.

use glam::{Vec2, Vec3};

use crate::pose::Pose;

/// Metres per logical pixel.
pub const M_PER_PX: f32 = 0.001;

/// The output the world is shown on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Output {
    /// physical pixels
    pub width_px: u32,
    pub height_px: u32,
    /// logical → physical scale (1, 1.25, 2 …)
    pub scale: f32,
}

/// Where a screen hangs and which way it faces: it faces the camera that
/// looks at it with this `yaw`/`pitch` (no roll).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScreenPlace {
    /// centre of the screen's face, metres
    pub center: Vec3,
    pub yaw: f32,
    pub pitch: f32,
}

/// The 1:1 pose and the 2D rectangle (physical px, y down) it lines up with.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OneToOne {
    pub pose: Pose,
    /// top-left corner, whole pixels
    pub origin: Vec2,
    /// size in physical px (`logical · scale`)
    pub size: Vec2,
}

/// Focal length in physical px for an output `height_px` high.
pub fn focal_px(height_px: u32, fov_y: f32) -> f32 {
    (height_px as f32 / 2.0) / (fov_y / 2.0).tan()
}

/// Distance at which 1 logical px of window = `scale` physical px.
pub fn distance(height_px: u32, scale: f32, fov_y: f32) -> f32 {
    M_PER_PX * focal_px(height_px, fov_y) / scale
}

/// The 1:1 pose for a window of `logical` size (px) hung at `place`, centred
/// on `out` with its top-left snapped to a whole pixel.
pub fn pose(out: &Output, fov_y: f32, logical: Vec2, place: &ScreenPlace) -> OneToOne {
    let size = logical * out.scale;
    let screen = Vec2::new(out.width_px as f32, out.height_px as f32);
    let ideal = (screen - size) / 2.0;
    let origin = ideal.round();
    // At the 1:1 distance one metre on the screen's plane is 1000·s px; to
    // move the image by `shift` px (y down) the camera moves the other way.
    let shift = (origin - ideal) / (out.scale / M_PER_PX);
    let mut p = Pose::new(place.center, place.yaw, place.pitch);
    p.pos = place.center
        - p.forward() * distance(out.height_px, out.scale, fov_y)
        - p.right() * shift.x
        + p.up() * shift.y;
    OneToOne {
        pose: p,
        origin,
        size,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::{DVec3, DVec4};

    const FOV: f32 = std::f32::consts::FRAC_PI_3; // 60°

    /// A tiny pinhole camera: world point → physical pixel (y down), in f64
    /// so the test's own error does not hide the pose's.
    fn project(p: &Pose, out: &Output, fov_y: f32, world: Vec3) -> Vec2 {
        let aspect = out.width_px as f64 / out.height_px as f64;
        let proj = glam::dcamera::rh::proj::opengl::perspective(fov_y as f64, aspect, 0.05, 100.0);
        let view = glam::dcamera::rh::view::look_to_mat4(
            p.pos.as_dvec3(),
            p.forward().as_dvec3(),
            DVec3::Y,
        );
        let c = proj * view * DVec4::from((world.as_dvec3(), 1.0));
        let ndc = c.truncate() / c.w;
        Vec2::new(
            ((ndc.x + 1.0) / 2.0 * out.width_px as f64) as f32,
            ((1.0 - ndc.y) / 2.0 * out.height_px as f64) as f32,
        )
    }

    /// The four corners of the screen's face in the world.
    fn corners(place: &ScreenPlace, logical: Vec2) -> [Vec3; 4] {
        let p = Pose::new(place.center, place.yaw, place.pitch);
        let (r, u) = (p.right(), p.up());
        let (hw, hh) = (logical.x * M_PER_PX / 2.0, logical.y * M_PER_PX / 2.0);
        [
            place.center - r * hw + u * hh, // top-left
            place.center + r * hw + u * hh, // top-right
            place.center - r * hw - u * hh, // bottom-left
            place.center + r * hw - u * hh, // bottom-right
        ]
    }

    fn check(out: Output, logical: Vec2, place: ScreenPlace) {
        let o = pose(&out, FOV, logical, &place);
        assert_eq!(o.origin, o.origin.round(), "top-left on a whole pixel");
        assert_eq!(o.size, logical * out.scale);
        let want = [
            o.origin,
            o.origin + Vec2::new(o.size.x, 0.0),
            o.origin + Vec2::new(0.0, o.size.y),
            o.origin + o.size,
        ];
        for (c, w) in corners(&place, logical).iter().zip(want) {
            let got = project(&o.pose, &out, FOV, *c);
            assert!(
                (got - w).abs().max_element() < 0.01,
                "corner {got} want {w} ({out:?}, {logical}, {place:?})"
            );
        }
    }

    #[test]
    fn distance_matches_the_plan_examples() {
        assert!((distance(1080, 1.0, FOV) - 0.935).abs() < 0.001);
        assert!((distance(1440, 1.25, FOV) - 0.998).abs() < 0.001);
        assert!((focal_px(1080, FOV) - 935.307).abs() < 0.01);
    }

    #[test]
    fn corners_land_on_the_2d_rect_head_on() {
        let out = Output {
            width_px: 1920,
            height_px: 1080,
            scale: 1.0,
        };
        let place = ScreenPlace {
            center: Vec3::new(0.0, 1.5, -2.0),
            yaw: 0.0,
            pitch: 0.0,
        };
        check(out, Vec2::new(1600.0, 1000.0), place);
    }

    #[test]
    fn odd_sizes_and_fractional_scale_are_snapped() {
        // (2560 - 1001·1.25)/2 = 654.375: needs the sub-millimetre shift
        let out = Output {
            width_px: 2560,
            height_px: 1440,
            scale: 1.25,
        };
        let place = ScreenPlace {
            center: Vec3::new(3.2, 1.5, -1.0),
            yaw: 0.0,
            pitch: 0.0,
        };
        check(out, Vec2::new(1001.0, 777.0), place);
    }

    #[test]
    fn turned_and_tilted_screens_too() {
        let out = Output {
            width_px: 1920,
            height_px: 1080,
            scale: 1.0,
        };
        let place = ScreenPlace {
            center: Vec3::new(-4.0, 1.6, 5.0),
            yaw: 0.6,
            pitch: -0.3,
        };
        check(out, Vec2::new(853.0, 541.0), place);
        let out = Output {
            width_px: 3840,
            height_px: 2160,
            scale: 2.0,
        };
        let place = ScreenPlace {
            center: Vec3::new(12.0, 1.5, -3.0),
            yaw: -2.4,
            pitch: 0.2,
        };
        check(out, Vec2::new(911.0, 603.0), place);
    }

    #[test]
    fn the_pose_never_rolls_and_faces_the_screen() {
        let out = Output {
            width_px: 1920,
            height_px: 1080,
            scale: 1.0,
        };
        let place = ScreenPlace {
            center: Vec3::new(1.0, 1.5, -2.0),
            yaw: 0.4,
            pitch: 0.1,
        };
        let o = pose(&out, FOV, Vec2::new(800.0, 600.0), &place);
        assert_eq!((o.pose.yaw, o.pose.pitch), (place.yaw, place.pitch));
        let d = (o.pose.pos - place.center).length();
        assert!((d - distance(1080, 1.0, FOV)).abs() < 0.001, "{d}");
    }
}
