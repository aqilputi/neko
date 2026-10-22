//! Cat motion as plain numbers (after gui/web/src/scene/anim.ts): each pose
//! is a set of joint parameters; the Bevy side only copies them onto the
//! rig. The cat faces -z (its monitor); we look at its back.

use crate::model::Pose;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CatParams {
    pub body_y: f32,
    /// lean: negative = towards the monitor
    pub body_rot_x: f32,
    pub body_scale: f32,
    pub head_y: f32,
    pub head_z: f32,
    pub head_rot_x: f32,
    /// turn of the head around y (π = looking back at the camera)
    pub head_rot_y: f32,
    pub head_rot_z: f32,
    /// 0 perked … 1 flat
    pub ears_back: f32,
    /// 0 closed … 1 open
    pub eyes_open: f32,
    /// front legs, rotation around x (0 hanging, ~1.1 on the keyboard)
    pub leg_l: f32,
    pub leg_r: f32,
    /// tail: pitch (back), sway (side) and curl of the outer segments
    pub tail_pitch: f32,
    pub tail_sway: f32,
    pub tail_curl: f32,
    pub tail_scale: f32,
}

pub const SIT: CatParams = CatParams {
    body_y: 0.40,
    body_rot_x: -0.08,
    body_scale: 1.0,
    head_y: 0.88,
    head_z: -0.06,
    head_rot_x: 0.0,
    head_rot_y: 0.0,
    head_rot_z: 0.0,
    ears_back: 0.0,
    eyes_open: 1.0,
    leg_l: 0.25,
    leg_r: 0.25,
    tail_pitch: 0.9,
    tail_sway: 0.35,
    tail_curl: 0.5,
    tail_scale: 1.0,
};

pub fn pose_params(p: Pose) -> CatParams {
    match p {
        Pose::Idle => SIT,
        Pose::Typing => CatParams {
            body_rot_x: -0.22,
            head_y: 0.84,
            head_z: -0.16,
            head_rot_x: -0.25,
            leg_l: 1.15,
            leg_r: 1.15,
            tail_pitch: 1.0,
            tail_sway: 0.5,
            ..SIT
        },
        Pose::Alert => CatParams {
            body_y: 0.43,
            body_rot_x: 0.05,
            head_y: 0.95,
            head_z: -0.02,
            head_rot_x: 0.12,
            head_rot_y: 2.6,
            leg_l: 0.05,
            leg_r: 0.05,
            tail_pitch: 0.25,
            tail_sway: 0.0,
            tail_curl: 0.25,
            ..SIT
        },
        Pose::Sleeping => CatParams {
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
            ..SIT
        },
        Pose::Puff => CatParams {
            eyes_open: 0.6,
            ears_back: 0.25,
            tail_sway: 0.2,
            ..SIT
        },
    }
}

pub fn lerp(a: CatParams, b: CatParams, u: f32) -> CatParams {
    let k = u.clamp(0.0, 1.0);
    let m = |x: f32, y: f32| x + (y - x) * k;
    CatParams {
        body_y: m(a.body_y, b.body_y),
        body_rot_x: m(a.body_rot_x, b.body_rot_x),
        body_scale: m(a.body_scale, b.body_scale),
        head_y: m(a.head_y, b.head_y),
        head_z: m(a.head_z, b.head_z),
        head_rot_x: m(a.head_rot_x, b.head_rot_x),
        head_rot_y: m(a.head_rot_y, b.head_rot_y),
        head_rot_z: m(a.head_rot_z, b.head_rot_z),
        ears_back: m(a.ears_back, b.ears_back),
        eyes_open: m(a.eyes_open, b.eyes_open),
        leg_l: m(a.leg_l, b.leg_l),
        leg_r: m(a.leg_r, b.leg_r),
        tail_pitch: m(a.tail_pitch, b.tail_pitch),
        tail_sway: m(a.tail_sway, b.tail_sway),
        tail_curl: m(a.tail_curl, b.tail_curl),
        tail_scale: m(a.tail_scale, b.tail_scale),
    }
}

/// Small, cheap motion on top of a pose (t in seconds, phase per cat).
pub fn ambient(pose: Pose, base: CatParams, t: f32, phase: f32) -> CatParams {
    let mut p = base;
    match pose {
        Pose::Typing => {
            p.leg_l = base.leg_l + 0.22 * (t * 14.0 + phase).sin();
            p.leg_r = base.leg_r + 0.22 * (t * 14.0 + phase + std::f32::consts::PI).sin();
            p.head_rot_x = base.head_rot_x + 0.06 * (t * 7.0 + phase).sin();
            p.head_y = base.head_y + 0.012 * (t * 7.0 + phase).sin();
            p.tail_sway = base.tail_sway + 0.25 * (t * 2.2 + phase).sin();
        }
        Pose::Alert => {
            p.tail_sway = base.tail_sway + 0.5 * (t * 6.0 + phase).sin();
            p.head_rot_z = 0.12 * (t * 1.5 + phase).sin();
        }
        Pose::Sleeping => {
            p.body_scale = 1.0 + 0.035 * (t * 2.2 + phase).sin();
            p.head_y = base.head_y + 0.01 * (t * 2.2 + phase).sin();
        }
        Pose::Idle | Pose::Puff => {
            p.tail_sway = base.tail_sway + 0.45 * (t * 1.6 + phase).sin();
            p.head_rot_z = 0.08 * (t * 0.7 + phase).sin();
        }
    }
    p
}

/// The one-shot fur puff (u: 0 → 1 over PUFF_S).
pub fn puff_at(base: CatParams, u: f32) -> CatParams {
    let k = (std::f32::consts::PI * u.clamp(0.0, 1.0)).sin();
    CatParams {
        tail_scale: base.tail_scale + 0.8 * k,
        ears_back: base.ears_back + (1.0 - base.ears_back) * k,
        tail_pitch: base.tail_pitch - 0.6 * k,
        body_scale: base.body_scale + 0.12 * k,
        ..base
    }
}

pub const TWEEN_S: f32 = 0.45;
pub const PUFF_S: f32 = 0.7;
/// Ambient motion runs at this rate (the loop sleeps in between).
pub const AMBIENT_FPS: f32 = 12.0;
/// Calm cats (idle/sleeping) move only in short bouts so an idle office
/// really idles: BOUT_S of motion every BOUT_EVERY_S.
pub const BOUT_S: f32 = 1.8;
pub const BOUT_EVERY_S: f32 = 9.0;

/// Whether a calm cat is in a motion bout at t (phase staggers cats).
pub fn in_bout(t: f32, phase: f32) -> bool {
    let off = phase / std::f32::consts::TAU * BOUT_EVERY_S;
    (t + off).rem_euclid(BOUT_EVERY_S) < BOUT_S
}

/// Stable per-cat phase in [0, 2π).
pub fn phase_of(id: &str) -> f32 {
    let mut h: u32 = 7;
    for b in id.bytes() {
        h = h.wrapping_mul(31).wrapping_add(b as u32);
    }
    (h % 1000) as f32 / 1000.0 * std::f32::consts::TAU
}

/// A walking cat's legs: (front-left, front-right, back-left, back-right)
/// swing angles and a body bob, at walking time t.
pub fn walk_cycle(t: f32) -> ([f32; 4], f32) {
    let s = (t * 11.0).sin();
    (
        [0.5 * s, -0.5 * s, -0.5 * s, 0.5 * s],
        0.025 * (t * 22.0).sin().abs(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn poses_differ_where_it_matters() {
        let t = pose_params(Pose::Typing);
        let s = pose_params(Pose::Sleeping);
        let a = pose_params(Pose::Alert);
        assert!(t.leg_l > 1.0, "paws on the keyboard");
        assert_eq!(s.eyes_open, 0.0);
        assert!(s.head_y < 0.6, "head on the desk");
        assert!(a.head_rot_y > 2.0, "looks back at us");
        assert_eq!(pose_params(Pose::Idle), SIT);
    }

    #[test]
    fn lerp_ends() {
        let a = pose_params(Pose::Idle);
        let b = pose_params(Pose::Sleeping);
        assert_eq!(lerp(a, b, 0.0), a);
        assert_eq!(lerp(a, b, 1.0), b);
        assert_eq!(lerp(a, b, 7.0), b);
        let m = lerp(a, b, 0.5);
        assert!((m.head_y - (a.head_y + b.head_y) / 2.0).abs() < 1e-6);
    }

    #[test]
    fn ambient_moves_typing_paws_in_antiphase() {
        let b = pose_params(Pose::Typing);
        let p = ambient(Pose::Typing, b, 0.1, 0.0);
        assert!(((p.leg_l - b.leg_l) + (p.leg_r - b.leg_r)).abs() < 1e-4);
        assert!((p.leg_l - b.leg_l).abs() > 0.01);
    }

    #[test]
    fn puff_returns_to_base() {
        let b = SIT;
        assert_eq!(puff_at(b, 0.0).tail_scale, b.tail_scale);
        assert!(puff_at(b, 0.5).tail_scale > 1.5);
        assert!((puff_at(b, 1.0).tail_scale - b.tail_scale).abs() < 1e-5);
    }

    #[test]
    fn bouts_and_phase() {
        let p = phase_of("abc");
        assert!((0.0..std::f32::consts::TAU).contains(&p));
        assert_eq!(phase_of("abc"), p);
        let n = (0..900).filter(|i| in_bout(*i as f32 * 0.01, 0.0)).count();
        assert_eq!(n, 180, "BOUT_S of each BOUT_EVERY_S");
        let (legs, bob) = walk_cycle(0.3);
        assert!((legs[0] + legs[1]).abs() < 1e-6 && bob >= 0.0);
    }
}
