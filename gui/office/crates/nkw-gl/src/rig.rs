//! Cats and moving furniture as rigid instances (S10). Pure: no GL, no
//! time; callers pass joint values and get [`Instance`]s to append to a
//! [`crate::draw::DrawList`].
//!
//! # The cat
//!
//! [`CatJoints`] mirrors `nk_cats::catpose::CatParams` field for field
//! (same names, same meaning, same units: the Bevy rig's), so the cats
//! plugin maps them 1:1. Positions in the params are in the Bevy rig's
//! units; [`RIG_SCALE`] turns them into metres for the toy villager here
//! (body 0.36 × 0.46 × 0.30 m, head 0.34 × 0.28 × 0.28 m).
//!
//! `root` is the point the cat sits or stands on (the seat top for
//! `layout::cat_seat`, the floor for walkers) with its yaw: build it as
//! `Mat4::from_rotation_translation(Quat::from_rotation_y(-pose.yaw), pose.pos)`;
//! the cat looks along root's -z (its monitor), its back towards +z.
//!
//! Joint order (all matrices are root-relative, multiplied left to right):
//! - body  = T(0, body_y·s, 0) · Rx(body_rot_x) · S(1 + (body_scale − 1)/2, body_scale, 1)
//! - head  = T(0, head_y·s, head_z·s) · Ry(head_rot_y) · Rx(head_rot_x) · Rz(head_rot_z)
//! - ear   = head · T(±0.115, 0.1, 0) · Rz(∓0.32) · Rx(ears_back) · (ear style)
//! - leg   = T(±0.11·s, 0.52·s, −0.14·s) · Rx(leg_l | leg_r)  (front legs hang from the shoulder)
//! - tail0 = T(0, 0.12·s, 0.25·s) · Rz(tail_sway) · Rx(tail_pitch); tail1 = tail0 · T(0, 0.2·s, 0) ·
//!   Rx(0.7·tail_curl); tail2 likewise; each segment is drawn scaled S(tail_scale, 1, tail_scale)
//!
//! where s = [`RIG_SCALE`] — the same order as `nkg`'s `rig_transforms`.
//! `head_rot_y = π` turns the face to the camera behind the cat (CONVERSA).

use std::f32::consts::{FRAC_PI_2, PI, TAU};

use glam::{Mat4, Quat, Vec2, Vec3, Vec4};
use nkw_core::layout::STAND_BEHIND;

use crate::draw::{Instance, MeshId};

/// Bevy-rig units → metres.
pub const RIG_SCALE: f32 = 0.575;

/// The joints of one cat: a field-for-field mirror of
/// `nk_cats::catpose::CatParams`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CatJoints {
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
    pub tail_pitch: f32,
    pub tail_sway: f32,
    pub tail_curl: f32,
    pub tail_scale: f32,
}

impl Default for CatJoints {
    /// `nk_cats::catpose::SIT` (copied, not imported).
    fn default() -> CatJoints {
        CatJoints {
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
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pattern {
    Solid,
    Tabby,
    Tuxedo,
    Calico,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ear {
    Pointy,
    Round,
    Folded,
}

/// A villager's colours. `accent` is the inner ear; `muzzle` also colours
/// the tuxedo's chest and socks; a calico's patches take `ears` (head and
/// back) and `tail` (the dark one).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Coat {
    pub body: [u8; 4],
    pub head: [u8; 4],
    pub ears: [u8; 4],
    pub tail: [u8; 4],
    pub muzzle: [u8; 4],
    pub accent: [u8; 4],
    pub pattern: Pattern,
    pub ear: Ear,
}

/// The eight pastel bases (ginger, blue-grey, butter, lilac, rose, mint, cocoa, snow).
const BASES: [[u8; 3]; 8] = [
    [246, 192, 146],
    [182, 190, 212],
    [244, 222, 160],
    [206, 190, 228],
    [242, 190, 198],
    [184, 222, 204],
    [200, 164, 140],
    [246, 242, 234],
];
const INNER: [[u8; 3]; 4] = [
    [242, 168, 180],
    [236, 150, 162],
    [250, 192, 190],
    [228, 160, 194],
];

fn splitmix(x: &mut u64) -> u64 {
    *x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *x;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D1_049B_1331_11EB);
    z ^ (z >> 31)
}

fn rgb(c: [u8; 3]) -> [u8; 4] {
    [c[0], c[1], c[2], 255]
}

/// `c` scaled by `k` (0..1 darkens).
pub fn shade(c: [u8; 4], k: f32) -> [u8; 4] {
    let f = |v: u8| (v as f32 * k).round().clamp(0.0, 255.0) as u8;
    [f(c[0]), f(c[1]), f(c[2]), c[3]]
}

/// `c` moved towards cream by `t`.
fn lighten(c: [u8; 4], t: f32) -> [u8; 4] {
    const CREAM: [f32; 3] = [255.0, 250.0, 240.0];
    let f = |v: u8, w: f32| (v as f32 + (w - v as f32) * t).round() as u8;
    [
        f(c[0], CREAM[0]),
        f(c[1], CREAM[1]),
        f(c[2], CREAM[2]),
        c[3],
    ]
}

impl Coat {
    /// Deterministic: the same seed always gives the same coat.
    pub fn from_seed(seed: u64) -> Coat {
        let mut s = seed;
        let a = splitmix(&mut s);
        let b = splitmix(&mut s);
        let mut base = BASES[(a & 7) as usize];
        // a little jitter so two cats of one base still differ
        for (k, ch) in base.iter_mut().enumerate() {
            let j = ((b >> (8 * k)) & 0xF) as i32 - 8;
            *ch = (*ch as i32 + j).clamp(0, 255) as u8;
        }
        let base = rgb(base);
        let pattern = match (a >> 3) % 10 {
            0..=2 => Pattern::Solid,
            3..=5 => Pattern::Tabby,
            6 | 7 => Pattern::Tuxedo,
            _ => Pattern::Calico,
        };
        let ear = match (a >> 8) % 10 {
            0..=4 => Ear::Pointy,
            5..=7 => Ear::Round,
            _ => Ear::Folded,
        };
        let accent = rgb(INNER[((a >> 12) & 3) as usize]);
        match pattern {
            Pattern::Solid | Pattern::Tabby => Coat {
                body: base,
                head: base,
                ears: shade(base, 0.93),
                tail: base,
                muzzle: lighten(base, 0.6),
                accent,
                pattern,
                ear,
            },
            Pattern::Tuxedo => {
                // soft darks (slate, charcoal-lilac, cocoa), never muddy shades of the pastel
                const DARKS: [[u8; 3]; 3] = [[104, 112, 138], [100, 94, 114], [126, 100, 92]];
                let d = DARKS[((a >> 16) % 3) as usize];
                let j = ((b >> 24) & 0x7) as u8;
                let dark = [d[0] + j, d[1] + j, d[2] + j, 255];
                Coat {
                    body: dark,
                    head: dark,
                    ears: dark,
                    tail: dark,
                    muzzle: [250, 248, 242, 255],
                    accent,
                    pattern,
                    ear,
                }
            }
            Pattern::Calico => {
                let white = lighten(base, 0.85);
                Coat {
                    body: white,
                    head: white,
                    ears: [240, 168, 108, 255],
                    tail: [128, 108, 104, 255],
                    muzzle: [255, 252, 246, 255],
                    accent,
                    pattern,
                    ear,
                }
            }
        }
    }
}

/// The joint frames of one cat (world matrices).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CatFrames {
    pub body: Mat4,
    pub head: Mat4,
    pub ears: [Mat4; 2],
    /// front legs, at the shoulder (left, right)
    pub legs: [Mat4; 2],
    /// the tail's three joints, base first
    pub tail: [Mat4; 3],
}

fn t(x: f32, y: f32, z: f32) -> Mat4 {
    Mat4::from_translation(Vec3::new(x, y, z))
}

/// The joint frames for `j` under `root` (see the module doc for the order).
pub fn cat_frames(j: &CatJoints, root: Mat4) -> CatFrames {
    let s = RIG_SCALE;
    let body = root
        * t(0.0, j.body_y * s, 0.0)
        * Mat4::from_rotation_x(j.body_rot_x)
        * Mat4::from_scale(Vec3::new(
            1.0 + (j.body_scale - 1.0) * 0.5,
            j.body_scale,
            1.0,
        ));
    let head = root
        * t(0.0, j.head_y * s, j.head_z * s)
        * Mat4::from_quat(
            Quat::from_rotation_y(j.head_rot_y)
                * Quat::from_rotation_x(j.head_rot_x)
                * Quat::from_rotation_z(j.head_rot_z),
        );
    let ears = [-1.0f32, 1.0].map(|k| {
        head * t(k * 0.115, 0.1, 0.0)
            * Mat4::from_rotation_z(-k * 0.32)
            * Mat4::from_rotation_x(j.ears_back)
    });
    let legs = [(-1.0f32, j.leg_l), (1.0, j.leg_r)]
        .map(|(k, a)| root * t(k * 0.11 * s, 0.52 * s, -0.14 * s) * Mat4::from_rotation_x(a));
    let t0 = root
        * t(0.0, 0.12 * s, 0.25 * s)
        * Mat4::from_rotation_z(j.tail_sway)
        * Mat4::from_rotation_x(j.tail_pitch);
    let step = t(0.0, 0.2 * s, 0.0) * Mat4::from_rotation_x(0.7 * j.tail_curl);
    let t1 = t0 * step;
    let t2 = t1 * step;
    CatFrames {
        body,
        head,
        ears,
        legs,
        tail: [t0, t1, t2],
    }
}

/// An instance with a white tint (vertex colours as built), not emissive, not faded.
pub fn piece(mesh: MeshId, model: Mat4) -> Instance {
    Instance {
        mesh,
        model,
        tint: [255; 4],
        emissive: 0.0,
        fade: 0.0,
    }
}

fn tinted(mesh: MeshId, model: Mat4, tint: [u8; 4], fade: f32) -> Instance {
    Instance {
        mesh,
        model,
        tint,
        emissive: 0.0,
        fade,
    }
}

/// The torso's half extents at torso-local height `y` (x, z), from the mesh profile.
fn torso_half_at(y: f32) -> Vec2 {
    let u = (y / 0.23).clamp(-1.0, 1.0);
    let p = &crate::kit::TORSO_PROFILE;
    let mut r = 0.0;
    for w in p.windows(2) {
        let ((r0, y0), (r1, y1)) = (w[0], w[1]);
        if u >= y0 && u <= y1 {
            r = r0 + (r1 - r0) * (u - y0) / (y1 - y0).max(1e-6);
            break;
        }
    }
    Vec2::new(0.18, 0.15) * r
}

/// One cat's instances (≤ 1.5k triangles). `collar`: a ring at the neck.
pub fn cat_instances(
    j: &CatJoints,
    c: &Coat,
    root: Mat4,
    collar: Option<[u8; 4]>,
    fade: f32,
    out: &mut Vec<Instance>,
) {
    parts(j, c, root, collar, fade, 1.0, out);
}

fn parts(
    j: &CatJoints,
    c: &Coat,
    root: Mat4,
    collar: Option<[u8; 4]>,
    fade: f32,
    big_head: f32,
    out: &mut Vec<Instance>,
) {
    use MeshId::*;
    let f = cat_frames(j, root);
    let head = f.head * Mat4::from_scale(Vec3::splat(big_head));
    let mut push = |mesh, model, tint| out.push(tinted(mesh, model, tint, fade));
    let tux = c.pattern == Pattern::Tuxedo;

    push(CatTorso, f.body, c.body);
    for k in [-1.0f32, 1.0] {
        push(CatPawBack, root * t(k * 0.112, 0.09, -0.01), c.body);
    }
    for leg in f.legs {
        push(CatPawFront, leg, if tux { c.muzzle } else { c.body });
    }
    push(CatSkull, head, c.head);
    push(CatMuzzle, head * t(0.0, -0.052, -0.128), c.muzzle);
    let open = j.eyes_open.clamp(0.0, 1.0).max(0.12);
    for k in [-1.0f32, 1.0] {
        let eye = head
            * t(k * 0.076, 0.0, -0.133)
            * Mat4::from_rotation_y(-k * 0.42)
            * Mat4::from_scale(Vec3::new(1.0, open, 1.0));
        push(CatEye, eye, [255; 4]);
    }
    let style = match c.ear {
        Ear::Pointy => Mat4::from_scale(Vec3::new(0.95, 1.15, 1.0)),
        Ear::Round => Mat4::from_scale(Vec3::new(1.15, 0.78, 1.0)),
        Ear::Folded => Mat4::from_rotation_x(-1.05) * Mat4::from_scale(Vec3::new(1.1, 0.7, 1.0)),
    };
    for ear in f.ears {
        let e = if big_head != 1.0 {
            f.head * Mat4::from_scale(Vec3::splat(big_head)) * f.head.inverse() * ear
        } else {
            ear
        };
        push(CatEarOuter, e * style, c.ears);
        push(CatEarInner, e * style, c.accent);
    }
    let thick = Mat4::from_scale(Vec3::new(j.tail_scale, 1.0, j.tail_scale));
    for (k, seg) in f.tail.iter().enumerate() {
        let tint = match (c.pattern, k) {
            // ringed tip for tabbies
            (Pattern::Tabby, 2) => shade(c.tail, 0.78),
            _ => c.tail,
        };
        push(CatTailSeg, *seg * thick, tint);
    }
    match c.pattern {
        Pattern::Solid => {}
        Pattern::Tabby => {
            let dark = shade(c.body, 0.8);
            for y in [0.15f32, 0.07, -0.01] {
                let h = torso_half_at(y);
                let m = f.body
                    * t(0.0, y, h.y - 0.014)
                    * Mat4::from_scale(Vec3::new(h.x / 0.15, 1.0, 1.0));
                push(CatStripe, m, dark);
            }
            // two stripes over the crown, front to back, wrapping down the back of the head
            let crown = Mat4::from_cols(Vec4::Z, Vec4::X, Vec4::Y, Vec4::W);
            for x in [-0.045f32, 0.045] {
                let m = head
                    * t(x, 0.0975, 0.0995)
                    * Mat4::from_rotation_x(0.78)
                    * crown
                    * Mat4::from_scale(Vec3::new(0.8, 0.8, 0.5));
                push(CatStripe, m, shade(c.head, 0.8));
            }
        }
        Pattern::Tuxedo => {
            push(CatChest, f.body * t(0.0, 0.03, -0.118), c.muzzle);
        }
        Pattern::Calico => {
            let h = torso_half_at(0.07);
            push(
                CatPatch,
                f.body * t(-0.05, 0.07, h.y - 0.012) * Mat4::from_rotation_y(-0.35),
                c.tail,
            );
            let h = torso_half_at(-0.1);
            push(
                CatPatch,
                f.body * t(0.09, -0.1, h.y - 0.02) * Mat4::from_rotation_y(0.55),
                c.ears,
            );
            push(
                CatPatch,
                head * t(0.085, 0.06, 0.02)
                    * Mat4::from_rotation_z(-0.7)
                    * Mat4::from_rotation_y(FRAC_PI_2),
                c.ears,
            );
        }
    }
    if let Some(col) = collar {
        push(
            CatCollar,
            f.body * t(0.0, 0.15, -0.005) * Mat4::from_scale(Vec3::new(1.12, 1.0, 1.0)),
            col,
        );
    }
}

/// The typing pose of a kitten: paws on the laptop in its lap.
fn kitten_typing() -> CatJoints {
    CatJoints {
        body_rot_x: -0.18,
        head_rot_x: -0.3,
        head_y: 0.84,
        head_z: -0.12,
        leg_l: 0.78,
        leg_r: 0.72,
        tail_pitch: 1.1,
        tail_sway: 0.5,
        ..CatJoints::default()
    }
}

/// Kittens' scale against a grown-up villager.
pub const KITTEN_SCALE: f32 = 0.45;

/// A kitten (0.45 scale, a bigger head) with a laptop in its lap when `typing`.
pub fn kitten_instances(c: &Coat, root: Mat4, typing: bool, fade: f32, out: &mut Vec<Instance>) {
    kitten_instances_looking(c, root, typing, 0.0, fade, out);
}

/// A kitten whose head is turned by `head_rot_y` (radians, around y; the
/// paws keep typing): "kittens look but keep typing" (FEEL §3.5).
pub fn kitten_instances_looking(
    c: &Coat,
    root: Mat4,
    typing: bool,
    head_rot_y: f32,
    fade: f32,
    out: &mut Vec<Instance>,
) {
    let mut j = if typing {
        kitten_typing()
    } else {
        CatJoints::default()
    };
    j.head_rot_y = head_rot_y;
    let small = root * Mat4::from_scale(Vec3::splat(KITTEN_SCALE));
    parts(&j, c, small, None, fade, 1.18, out);
    if typing {
        let lap = root * t(0.0, 0.03, -0.095) * Mat4::from_rotation_x(0.12);
        out.push(Instance {
            fade,
            ..piece(MeshId::CatLaptop, lap)
        });
    }
}

// ---------------------------------------------------------------- walking

/// The legs of a walking cat at one moment (S17).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Gait {
    /// radians, 2π per stride
    pub phase: f32,
    /// swing of each leg around its hip/shoulder, radians (FL FR BL BR);
    /// positive = the paw forward (towards -z)
    pub legs: [f32; 4],
    /// the body's rise over the stance, metres (≥ 0)
    pub bob: f32,
    /// forward lean, radians
    pub lean: f32,
}

impl Gait {
    /// Standing still: legs under the body, no bob, no lean.
    pub const NEUTRAL: Gait = Gait {
        phase: 0.0,
        legs: [0.0; 4],
        bob: 0.0,
        lean: 0.0,
    };
}

/// One stride (a full cycle of the four legs), metres.
pub const STRIDE_M: f32 = 0.5;
/// The speed at which the gait reaches its full swing, m/s.
const FULL_SWING_MPS: f32 = 0.9;
/// The widest leg swing, radians.
const SWING_RAD: f32 = 0.42;
/// The highest bob, metres.
const BOB_M: f32 = 0.025;
/// The lean at full swing, radians.
const LEAN_RAD: f32 = 0.06;

/// The gait after walking `dist_m` at `speed_mps`: a trot, diagonal pairs
/// (FL+BR, FR+BL) swinging opposite; the phase comes from the distance, so
/// a walker that stops keeps its legs where they were; speed 0 → neutral.
pub fn gait_at(dist_m: f32, speed_mps: f32) -> Gait {
    if speed_mps.is_nan() || speed_mps <= 1e-4 || !dist_m.is_finite() {
        return Gait::NEUTRAL;
    }
    let k = (speed_mps / FULL_SWING_MPS).clamp(0.0, 1.0);
    let phase = (TAU * dist_m / STRIDE_M).rem_euclid(TAU);
    let a = SWING_RAD * k * phase.sin();
    Gait {
        phase,
        legs: [a, -a, -a, a],
        // up twice a stride (once per pair), never below the stance
        bob: BOB_M * k * 0.5 * (1.0 - (2.0 * phase).cos()),
        lean: LEAN_RAD * k,
    }
}

/// The walking cat's proportions, metres (root = the floor under its middle).
pub mod walk {
    /// hip and shoulder joints above the floor
    pub const HIP_Y: f32 = 0.26;
    pub const SHOULDER_Y: f32 = 0.24;
    /// hips behind the middle (+z), shoulders ahead (-z)
    pub const HIP_Z: f32 = 0.13;
    pub const SHOULDER_Z: f32 = -0.13;
    pub const LEG_X: f32 = 0.085;
    /// legs a little sturdier than the seated cat's arms
    pub const LEG_THICK: f32 = 1.3;
    /// the torso drawn lying down: scale of its width, length and thickness
    pub const TORSO_SCALE: [f32; 3] = [0.82, 0.88, 0.78];
    /// the head: scale, and its centre (y, z) at rest
    pub const HEAD_SCALE: f32 = 0.8;
    pub const HEAD_Y: f32 = 0.265;
    pub const HEAD_Z: f32 = -0.33;
    /// where the tail leaves the rump
    pub const TAIL_Y: f32 = 0.31;
    pub const TAIL_Z: f32 = 0.19;
    /// the leg mesh's length (joint to the bottom of the paw)
    pub const LEG_LEN: f32 = 0.249;
}

/// The joint frames of a walking cat (body, head, ears, the two front
/// legs at the shoulder, the tail), for name tags and envelopes.
pub fn cat_walking_frames(j: &CatJoints, g: &Gait, root: Mat4) -> CatFrames {
    let w = walk_frames(j, g, root);
    CatFrames {
        body: w.body,
        head: w.head,
        ears: w.ears,
        legs: [w.legs[0], w.legs[1]],
        tail: w.tail,
    }
}

struct WalkFrames {
    /// the torso, its y along the cat's length
    body: Mat4,
    head: Mat4,
    ears: [Mat4; 2],
    /// FL FR BL BR, at the joint, y up the leg, scaled to reach the floor
    legs: [Mat4; 4],
    tail: [Mat4; 3],
}

fn walk_frames(j: &CatJoints, g: &Gait, root: Mat4) -> WalkFrames {
    use walk::*;
    // the bob lifts everything above the paws; the lean pitches it nose-down around the hips
    let local =
        t(0.0, HIP_Y + g.bob, HIP_Z) * Mat4::from_rotation_x(-g.lean) * t(0.0, -HIP_Y, -HIP_Z);
    let frame = root * local;
    let [sx, sl, st] = TORSO_SCALE;
    let body = frame
        * t(0.0, (HIP_Y + SHOULDER_Y) / 2.0, 0.0)
        * Mat4::from_rotation_x(-FRAC_PI_2)
        * Mat4::from_scale(Vec3::new(sx, sl, st));
    let head = frame
        * t(0.0, HEAD_Y, HEAD_Z)
        * Mat4::from_quat(
            Quat::from_rotation_y(j.head_rot_y)
                * Quat::from_rotation_x(j.head_rot_x)
                * Quat::from_rotation_z(j.head_rot_z),
        )
        * Mat4::from_scale(Vec3::splat(HEAD_SCALE));
    let ears = [-1.0f32, 1.0].map(|k| {
        head * t(k * 0.115, 0.1, 0.0)
            * Mat4::from_rotation_z(-k * 0.32)
            * Mat4::from_rotation_x(j.ears_back)
    });
    // joints on the (leaning) body, legs hanging plumb from them and
    // swinging around x; each leg is stretched so its paw meets the floor
    let joint = |x: f32, y: f32, z: f32, swing: f32| {
        let at = local.transform_point3(Vec3::new(x, y, z));
        root * t(at.x, at.y, at.z)
            * Mat4::from_rotation_x(swing)
            * Mat4::from_scale(Vec3::new(LEG_THICK, at.y / LEG_LEN, LEG_THICK))
    };
    let legs = [
        joint(-LEG_X, SHOULDER_Y, SHOULDER_Z, g.legs[0]),
        joint(LEG_X, SHOULDER_Y, SHOULDER_Z, g.legs[1]),
        joint(-LEG_X, HIP_Y, HIP_Z, g.legs[2]),
        joint(LEG_X, HIP_Y, HIP_Z, g.legs[3]),
    ];
    let t0 = frame
        * t(0.0, TAIL_Y, TAIL_Z)
        * Mat4::from_rotation_z(j.tail_sway)
        * Mat4::from_rotation_x(j.tail_pitch)
        * Mat4::from_scale(Vec3::splat(0.9));
    let step = t(0.0, 0.2 * RIG_SCALE, 0.0) * Mat4::from_rotation_x(0.7 * j.tail_curl);
    let t1 = t0 * step;
    let t2 = t1 * step;
    WalkFrames {
        body,
        head,
        ears,
        legs,
        tail: [t0, t1, t2],
    }
}

/// A villager walking on four legs (S17): the torso level at the hips, the
/// paws from `g`, the head ahead (with `j.head_*` on top), the tail per `j`;
/// the same parts and tints as [`cat_instances`] plus two more legs.
/// `root`: the floor point under the cat's middle with its yaw (it walks
/// towards root's -z).
pub fn cat_walking(
    j: &CatJoints,
    g: &Gait,
    c: &Coat,
    root: Mat4,
    collar: Option<[u8; 4]>,
    fade: f32,
    out: &mut Vec<Instance>,
) {
    use MeshId::*;
    let f = walk_frames(j, g, root);
    let mut push = |mesh, model, tint| out.push(tinted(mesh, model, tint, fade));
    let tux = c.pattern == Pattern::Tuxedo;
    let sock = if tux { c.muzzle } else { c.body };

    push(CatTorso, f.body, c.body);
    // the haunches sit on the back legs' joints
    for (k, leg) in [(-1.0f32, f.legs[2]), (1.0, f.legs[3])] {
        let hip = leg.transform_point3(Vec3::ZERO);
        let haunch = Mat4::from_translation(hip)
            * Mat4::from_quat(Quat::from_mat4(&root))
            * t(k * 0.02, -0.01, -0.02)
            * Mat4::from_scale(Vec3::new(0.9, 0.85, 0.85));
        push(CatPawBack, haunch, c.body);
    }
    for leg in f.legs {
        push(CatPawFront, leg, sock);
    }
    head_parts(j, c, f.head, f.ears, &mut push);
    let thick = Mat4::from_scale(Vec3::new(j.tail_scale, 1.0, j.tail_scale));
    for (k, seg) in f.tail.iter().enumerate() {
        let tint = match (c.pattern, k) {
            (Pattern::Tabby, 2) => shade(c.tail, 0.78),
            _ => c.tail,
        };
        push(CatTailSeg, *seg * thick, tint);
    }
    match c.pattern {
        Pattern::Solid => {}
        Pattern::Tabby => {
            let dark = shade(c.body, 0.8);
            // across the back (the lying torso's +z is up)
            for y in [0.12f32, 0.02, -0.08] {
                let h = torso_half_at(y);
                let m = f.body
                    * t(0.0, y, h.y - 0.014)
                    * Mat4::from_scale(Vec3::new(h.x / 0.15, 1.0, 1.0));
                push(CatStripe, m, dark);
            }
            let crown = Mat4::from_cols(Vec4::Z, Vec4::X, Vec4::Y, Vec4::W);
            for x in [-0.045f32, 0.045] {
                let m = f.head
                    * t(x, 0.0975, 0.0995)
                    * Mat4::from_rotation_x(0.78)
                    * crown
                    * Mat4::from_scale(Vec3::new(0.8, 0.8, 0.5));
                push(CatStripe, m, shade(c.head, 0.8));
            }
        }
        Pattern::Tuxedo => {
            // the bib under the chest
            push(
                CatChest,
                f.body * t(0.0, 0.14, -0.1) * Mat4::from_rotation_x(FRAC_PI_2),
                c.muzzle,
            );
        }
        Pattern::Calico => {
            let h = torso_half_at(0.07);
            push(
                CatPatch,
                f.body * t(-0.05, 0.07, h.y - 0.012) * Mat4::from_rotation_y(-0.35),
                c.tail,
            );
            let h = torso_half_at(-0.1);
            push(
                CatPatch,
                f.body * t(0.09, -0.1, h.y - 0.02) * Mat4::from_rotation_y(0.55),
                c.ears,
            );
            push(
                CatPatch,
                f.head
                    * t(0.085, 0.06, 0.02)
                    * Mat4::from_rotation_z(-0.7)
                    * Mat4::from_rotation_y(FRAC_PI_2),
                c.ears,
            );
        }
    }
    if let Some(col) = collar {
        // around the neck, where the torso narrows towards the head
        let h = torso_half_at(0.17);
        let neck = f.body
            * t(0.0, 0.17, 0.0)
            * Mat4::from_scale(Vec3::new(1.08 * h.x / 0.112, 1.0, 1.08 * h.y / 0.112));
        push(CatCollar, neck, col);
    }
}

/// Skull, muzzle, eyes and ears on `head`.
fn head_parts(
    j: &CatJoints,
    c: &Coat,
    head: Mat4,
    ears: [Mat4; 2],
    push: &mut impl FnMut(MeshId, Mat4, [u8; 4]),
) {
    use MeshId::*;
    push(CatSkull, head, c.head);
    push(CatMuzzle, head * t(0.0, -0.052, -0.128), c.muzzle);
    let open = j.eyes_open.clamp(0.0, 1.0).max(0.12);
    for k in [-1.0f32, 1.0] {
        let eye = head
            * t(k * 0.076, 0.0, -0.133)
            * Mat4::from_rotation_y(-k * 0.42)
            * Mat4::from_scale(Vec3::new(1.0, open, 1.0));
        push(CatEye, eye, [255; 4]);
    }
    let style = ear_style(c.ear);
    for e in ears {
        push(CatEarOuter, e * style, c.ears);
        push(CatEarInner, e * style, c.accent);
    }
}

fn ear_style(e: Ear) -> Mat4 {
    match e {
        Ear::Pointy => Mat4::from_scale(Vec3::new(0.95, 1.15, 1.0)),
        Ear::Round => Mat4::from_scale(Vec3::new(1.15, 0.78, 1.0)),
        Ear::Folded => Mat4::from_rotation_x(-1.05) * Mat4::from_scale(Vec3::new(1.1, 0.7, 1.0)),
    }
}

// ---------------------------------------------------------------- monitor

/// The bezel's margin around the screen and the chubby back's depth, metres.
pub const BEZEL_M: f32 = 0.02;

/// The neck and the foot only (a stack of screens draws its own bezels).
pub fn monitor_stand(foot: Mat4, size_m: Vec2, lift: f32, out: &mut Vec<Instance>) {
    let neck = (lift + size_m.y * 0.3).max(0.02);
    out.push(piece(
        MeshId::MonitorStand,
        foot * Mat4::from_scale(Vec3::new(1.0, neck, 1.0)),
    ));
    let w = (0.4 * size_m.x).clamp(0.16, 0.5);
    out.push(piece(
        MeshId::MonitorFoot,
        foot * t(0.0, 0.0, 0.02) * Mat4::from_scale(Vec3::new(w, 1.0, 0.22)),
    ));
}

/// The screen's model (the `ScreenQuad` convention: unit quad, its face
/// towards +z of `foot`) for a monitor on `foot`.
pub fn monitor_screen(foot: Mat4, size_m: Vec2, lift: f32) -> Mat4 {
    foot * t(0.0, lift + size_m.y / 2.0, STAND_BEHIND)
        * Mat4::from_scale(Vec3::new(size_m.x, size_m.y, 1.0))
}

/// The chubby AC monitor: a rounded bezel 2 cm around the screen, a soft
/// back, a neck from the desk to the bezel, a foot plate on the desk.
/// `foot`: where the stand meets the desk top (`layout::monitor_root`),
/// yaw as there; the screen plane sits `layout::STAND_BEHIND` in front of
/// it, its bottom `lift` above the desk. Instances are not faded.
pub fn monitor(foot: Mat4, size_m: Vec2, lift: f32, out: &mut Vec<Instance>) {
    monitor_stand(foot, size_m, lift, out);
    let centre = foot * t(0.0, lift + size_m.y / 2.0, STAND_BEHIND);
    let frame = centre
        * Mat4::from_scale(Vec3::new(
            size_m.x + 2.0 * BEZEL_M,
            size_m.y + 2.0 * BEZEL_M,
            1.0,
        ));
    out.push(piece(MeshId::MonitorFrame, frame));
    // the back: a rounded hump behind the bezel
    let back = centre
        * t(0.0, 0.0, -0.095)
        * Mat4::from_scale(Vec3::new(size_m.x * 0.7, size_m.y * 0.72, 0.07));
    out.push(Instance {
        tint: [247, 236, 210, 255],
        ..piece(MeshId::RoundedBox, back)
    });
}

// ---------------------------------------------------------------- doors, clock

/// How far a door leaf swings when fully open.
pub const DOOR_OPEN_RAD: f32 = 100.0 * PI / 180.0;
/// The hinge's offset from the frame's centre line (`DoorLeaf` is 0.98 wide).
pub const DOOR_HINGE_X: f32 = 0.49;

/// The `DoorLeaf` model in a `DoorFrame` at `frame`: hinged on the left
/// (−x) or right post, swung `openness` (0 closed … 1 = 100°) towards the
/// frame's −z.
pub fn door_leaf(frame: Mat4, openness: f32, hinge_left: bool) -> Mat4 {
    let a = openness.clamp(0.0, 1.0) * DOOR_OPEN_RAD;
    if hinge_left {
        frame * t(-DOOR_HINGE_X, 0.0, 0.0) * Mat4::from_rotation_y(a)
    } else {
        frame * t(DOOR_HINGE_X, 0.0, 0.0) * Mat4::from_rotation_y(PI - a)
    }
}

/// The hour and minute hands of a `WallClock` at `root`, clockwise seen
/// from the front.
pub fn clock_hands(root: Mat4, hour: u8, minute: u8) -> [Instance; 2] {
    let m = (minute % 60) as f32;
    let ah = ((hour % 12) as f32 + m / 60.0) / 12.0 * TAU;
    let am = m / 60.0 * TAU;
    let hand = |angle: f32, z: f32, len: f32| {
        piece(
            MeshId::ClockHand,
            root * t(0.0, 0.0, z)
                * Mat4::from_rotation_z(-angle)
                * Mat4::from_scale(Vec3::splat(len)),
        )
    };
    [hand(ah, 0.053, 0.075), hand(am, 0.057, 0.112)]
}

// ---------------------------------------------------------------- furniture with state

/// The tone arm's yaw over the record and its lift when up.
pub const ARM_PLAY: f32 = -0.75;
pub const ARM_LIFT: f32 = -0.12;

/// The vitrola: console, platter (turned by `spin`), tone arm (down on the
/// record or parked up), volume knob (turned by `knob`), two speakers on
/// the floor beside it.
pub fn turntable(root: Mat4, spin: f32, needle_down: bool, knob: f32, out: &mut Vec<Instance>) {
    use crate::kit::{ARM_AT, KNOB_AT, PLATTER_AT};
    out.push(piece(MeshId::Turntable, root));
    out.push(piece(
        MeshId::Platter,
        root * Mat4::from_translation(PLATTER_AT) * Mat4::from_rotation_y(spin),
    ));
    let (yaw, lift) = if needle_down {
        (ARM_PLAY, 0.0)
    } else {
        (0.0, ARM_LIFT)
    };
    out.push(piece(
        MeshId::ToneArm,
        root * Mat4::from_translation(ARM_AT)
            * Mat4::from_rotation_y(yaw)
            * Mat4::from_rotation_x(lift),
    ));
    out.push(piece(
        MeshId::Knob,
        root * Mat4::from_translation(KNOB_AT) * Mat4::from_rotation_y(knob),
    ));
    for k in [-1.0f32, 1.0] {
        out.push(piece(
            MeshId::Speaker,
            root * t(k * 0.5, 0.0, 0.03) * Mat4::from_rotation_y(-k * 0.18),
        ));
    }
}

/// The antenna plant: a pot and four leaves, one per signal bar; with no
/// bar (no network) all four droop, wilted.
pub fn plant(root: Mat4, bars: u8, out: &mut Vec<Instance>) {
    out.push(piece(MeshId::Pot, root));
    for i in 0..4u8 {
        let yaw = i as f32 * FRAC_PI_2 + 0.6;
        let (tilt, tint) = if bars == 0 {
            (1.45, [196, 172, 120, 255])
        } else {
            (
                0.16 + 0.14 * (i % 2) as f32,
                [255, 255, 255, if i < bars { 255 } else { 0 }],
            )
        };
        out.push(Instance {
            tint,
            ..piece(
                MeshId::Plant,
                root * t(0.0, 0.19, 0.0) * Mat4::from_rotation_y(yaw) * Mat4::from_rotation_x(tilt),
            )
        });
    }
}

/// The oil jar of the lamp (the battery): the oil fills it to `level`
/// (0..1), a small flame burns on the wick while there is any.
pub fn oil_lamp(root: Mat4, level: f32, out: &mut Vec<Instance>) {
    out.push(piece(MeshId::OilJar, root));
    let level = level.clamp(0.0, 1.0);
    let h = 0.16 * level;
    if h > 0.002 {
        out.push(Instance {
            tint: [236, 176, 84, 255],
            ..piece(
                MeshId::Cylinder,
                root * t(0.0, 0.01 + h / 2.0, 0.0) * Mat4::from_scale(Vec3::new(0.174, h, 0.174)),
            )
        });
        out.push(Instance {
            emissive: 1.0,
            ..piece(
                MeshId::Flame,
                root * t(0.0, 0.252, 0.0) * Mat4::from_scale(Vec3::splat(0.25 + 0.2 * level)),
            )
        });
    }
}

/// The fireplace with three flames whose height follows `heat` (0..1).
pub fn fireplace(root: Mat4, heat: f32, out: &mut Vec<Instance>) {
    out.push(piece(MeshId::Fireplace, root));
    let h = 0.35 + 1.4 * heat.clamp(0.0, 1.0);
    for (x, k) in [(-0.12f32, 0.8f32), (0.0, 1.15), (0.12, 0.9)] {
        out.push(Instance {
            emissive: 1.0,
            ..piece(
                MeshId::Flame,
                root * t(x, 0.13, 0.02) * Mat4::from_scale(Vec3::new(1.2, h * k, 1.2)),
            )
        });
    }
}

/// The aquarium: water (clouding with `murk`, 0..1) and two fish at
/// `swim` (radians of their back-and-forth).
pub fn aquarium(root: Mat4, murk: f32, swim: f32, out: &mut Vec<Instance>) {
    out.push(piece(MeshId::Aquarium, root));
    out.push(Instance {
        tint: [196, 226, 244, 255],
        emissive: 0.15,
        ..piece(
            MeshId::Plane,
            root * t(0.0, 0.94, 0.0) * Mat4::from_scale(Vec3::new(0.66, 1.0, 0.3)),
        )
    });
    for k in 0..2 {
        let ph = swim + k as f32 * 2.1;
        let dir = if ph.cos() >= 0.0 { 0.0 } else { PI };
        let m =
            root * t(
                0.22 * ph.sin(),
                0.7 + 0.1 * k as f32,
                -0.03 + 0.08 * k as f32,
            ) * Mat4::from_rotation_y(dir);
        out.push(Instance {
            tint: if k == 0 {
                [255; 4]
            } else {
                [200, 220, 255, 255]
            },
            ..piece(MeshId::Fish, m * Mat4::from_scale(Vec3::splat(1.8)))
        });
    }
    let a = (murk.clamp(0.0, 1.0) * 0.7 * 255.0) as u8;
    if a >= 8 {
        out.push(Instance {
            tint: [168, 180, 146, a],
            ..piece(
                MeshId::Box,
                root * t(0.0, 0.76, 0.0) * Mat4::from_scale(Vec3::new(0.66, 0.4, 0.31)),
            )
        });
    }
}

/// The safe and its door, open by `openness` (0..1 → 0..100°, towards the viewer).
pub fn safe(root: Mat4, openness: f32, out: &mut Vec<Instance>) {
    out.push(piece(MeshId::Safe, root));
    let a = openness.clamp(0.0, 1.0) * DOOR_OPEN_RAD;
    out.push(piece(
        MeshId::SafeDoor,
        root * t(-0.23, 0.34, 0.22) * Mat4::from_rotation_y(-a),
    ));
}

/// The ON AIR sign, lit (emissive) while recording.
pub fn on_air(root: Mat4, lit: bool, out: &mut Vec<Instance>) {
    out.push(Instance {
        tint: if lit { [255; 4] } else { [150, 140, 140, 255] },
        emissive: if lit { 0.9 } else { 0.0 },
        ..piece(MeshId::OnAir, root)
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::triangle_count;

    fn tris(v: &[Instance]) -> u32 {
        v.iter().map(|i| triangle_count(i.mesh)).sum()
    }

    fn near(a: Vec3, b: Vec3, eps: f32) -> bool {
        (a - b).abs().max_element() <= eps
    }

    fn pos(m: Mat4) -> Vec3 {
        m.transform_point3(Vec3::ZERO)
    }

    #[test]
    fn default_joints_are_catpose_sit() {
        let j = CatJoints::default();
        let want = [
            0.40, -0.08, 1.0, 0.88, -0.06, 0.0, 0.0, 0.0, 0.0, 1.0, 0.25, 0.25, 0.9, 0.35, 0.5, 1.0,
        ];
        let got = [
            j.body_y,
            j.body_rot_x,
            j.body_scale,
            j.head_y,
            j.head_z,
            j.head_rot_x,
            j.head_rot_y,
            j.head_rot_z,
            j.ears_back,
            j.eyes_open,
            j.leg_l,
            j.leg_r,
            j.tail_pitch,
            j.tail_sway,
            j.tail_curl,
            j.tail_scale,
        ];
        assert_eq!(got, want);
    }

    #[test]
    fn default_frames_reproduce_sit_within_a_millimetre() {
        let s = RIG_SCALE;
        let root = Mat4::from_translation(Vec3::new(1.0, 0.45, -2.0));
        let f = cat_frames(&CatJoints::default(), root);
        let o = Vec3::new(1.0, 0.45, -2.0);
        assert!(near(pos(f.body), o + Vec3::new(0.0, 0.40 * s, 0.0), 1e-3));
        assert!(near(
            pos(f.head),
            o + Vec3::new(0.0, 0.88 * s, -0.06 * s),
            1e-3
        ));
        assert!(near(
            pos(f.legs[0]),
            o + Vec3::new(-0.11 * s, 0.52 * s, -0.14 * s),
            1e-3
        ));
        assert!(near(
            pos(f.tail[0]),
            o + Vec3::new(0.0, 0.12 * s, 0.25 * s),
            1e-3
        ));
        // the segments are 0.2 rig units apart
        assert!((pos(f.tail[1]) - pos(f.tail[0])).length() - 0.2 * s < 1e-3);
        // body bottom on the seat: the torso is 0.46 m tall
        assert!((0.40 * s - 0.23).abs() < 1e-3);
        // and the head (with its ears) stays under EYE − 0.25 on a chair at SEAT_H
        let mut v = Vec::new();
        cat_instances(
            &CatJoints::default(),
            &Coat::from_seed(1),
            root,
            None,
            0.0,
            &mut v,
        );
        let top = v
            .iter()
            .filter(|i| matches!(i.mesh, MeshId::CatEarOuter))
            .map(|i| i.model.transform_point3(Vec3::new(0.0, 0.11, 0.0)).y)
            .fold(0.0f32, f32::max);
        assert!(
            top <= nkw_core::layout::EYE - 0.25 + 1e-3,
            "ear tips at {top}"
        );
    }

    #[test]
    fn head_turned_by_pi_faces_the_camera() {
        let yaw = 0.7;
        let root =
            Mat4::from_rotation_translation(Quat::from_rotation_y(-yaw), Vec3::new(3.0, 0.45, 1.0));
        let forward = root.transform_vector3(Vec3::NEG_Z);
        let j = CatJoints {
            head_rot_y: PI,
            ..CatJoints::default()
        };
        let head = cat_frames(&j, root).head.transform_vector3(Vec3::NEG_Z);
        assert!((head + forward).length() < 1e-5, "{head} vs {forward}");
        // at rest it looks where the cat looks
        let rest = cat_frames(&CatJoints::default(), root)
            .head
            .transform_vector3(Vec3::NEG_Z);
        assert!((rest - forward).length() < 1e-5);
    }

    #[test]
    fn legs_and_tail_follow_their_joints() {
        let rest = cat_frames(&CatJoints::default(), Mat4::IDENTITY);
        let typing = cat_frames(
            &CatJoints {
                leg_l: 1.15,
                leg_r: 1.15,
                ..CatJoints::default()
            },
            Mat4::IDENTITY,
        );
        let paw = |m: Mat4| m.transform_point3(Vec3::new(0.0, -0.21, 0.0));
        assert!(
            paw(typing.legs[0]).z < paw(rest.legs[0]).z - 0.1,
            "paws reach forward"
        );
        assert!(paw(typing.legs[0]).y > paw(rest.legs[0]).y);
        // the tail goes up and back (+z) at SIT and sways to one side
        let tip = rest.tail[2].transform_point3(Vec3::new(0.0, 0.115, 0.0));
        assert!(tip.z > 0.25 && tip.y > 0.1, "{tip}");
        let flat = cat_frames(
            &CatJoints {
                tail_sway: 0.0,
                ..CatJoints::default()
            },
            Mat4::IDENTITY,
        );
        assert!(flat.tail[2].transform_point3(Vec3::Y * 0.1).x.abs() < 1e-5);
        assert!(tip.x.abs() > 0.02);
    }

    #[test]
    fn coats_are_deterministic_distinct_and_pastel() {
        assert_eq!(Coat::from_seed(42), Coat::from_seed(42));
        let coats: Vec<_> = (0..64u64).map(Coat::from_seed).collect();
        let mut distinct = coats.clone();
        distinct.sort_by_key(|c| (c.body, c.accent, c.pattern as u8, c.ear as u8));
        distinct.dedup();
        assert!(distinct.len() >= 48, "{} distinct coats", distinct.len());
        for p in [
            Pattern::Solid,
            Pattern::Tabby,
            Pattern::Tuxedo,
            Pattern::Calico,
        ] {
            assert!(coats.iter().any(|c| c.pattern == p), "{p:?} never drawn");
        }
        for e in [Ear::Pointy, Ear::Round, Ear::Folded] {
            assert!(coats.iter().any(|c| c.ear == e), "{e:?} never drawn");
        }
        let lum = |c: [u8; 4]| 0.3 * c[0] as f32 + 0.59 * c[1] as f32 + 0.11 * c[2] as f32;
        for c in &coats {
            assert!(c.body[3] == 255 && c.muzzle[3] == 255);
            if c.pattern != Pattern::Tuxedo {
                assert!(lum(c.body) > 150.0, "pastel body {:?}", c.body);
            }
            assert!(
                lum(c.muzzle) >= lum(c.body) - 3.0,
                "muzzle lighter than the body"
            );
        }
    }

    #[test]
    fn every_cat_fits_its_triangle_budget() {
        let mut worst = 0;
        for seed in 0..256u64 {
            let c = Coat::from_seed(seed);
            let mut v = Vec::new();
            cat_instances(
                &CatJoints::default(),
                &c,
                Mat4::IDENTITY,
                Some([200, 80, 80, 255]),
                0.0,
                &mut v,
            );
            worst = worst.max(tris(&v));
            assert!(v.iter().all(|i| i.fade == 0.0));
        }
        assert!(worst <= 1500, "{worst} tris");
    }

    #[test]
    fn patterns_add_their_markings() {
        let count = |p: Pattern, mesh: MeshId| {
            let c = Coat {
                pattern: p,
                ..Coat::from_seed(3)
            };
            let mut v = Vec::new();
            cat_instances(&CatJoints::default(), &c, Mat4::IDENTITY, None, 0.0, &mut v);
            v.iter().filter(|i| i.mesh == mesh).count()
        };
        assert_eq!(count(Pattern::Solid, MeshId::CatStripe), 0);
        assert_eq!(
            count(Pattern::Tabby, MeshId::CatStripe),
            5,
            "3 on the back, 2 on the head"
        );
        assert_eq!(count(Pattern::Tuxedo, MeshId::CatChest), 1);
        assert_eq!(count(Pattern::Calico, MeshId::CatPatch), 3);
        assert_eq!(count(Pattern::Solid, MeshId::CatEye), 2);
        assert_eq!(count(Pattern::Solid, MeshId::CatTailSeg), 3);
    }

    #[test]
    fn closed_eyes_flatten_and_fade_reaches_every_part() {
        let mut v = Vec::new();
        let j = CatJoints {
            eyes_open: 0.0,
            ..CatJoints::default()
        };
        cat_instances(
            &j,
            &Coat::from_seed(9),
            Mat4::IDENTITY,
            Some([1, 2, 3, 255]),
            0.4,
            &mut v,
        );
        let eye = v.iter().find(|i| i.mesh == MeshId::CatEye).unwrap();
        let h = eye.model.transform_vector3(Vec3::Y).length();
        assert!(h < 0.2, "eye height scale {h}");
        assert!(v.iter().all(|i| i.fade == 0.4));
        assert!(
            v.iter()
                .any(|i| i.mesh == MeshId::CatCollar && i.tint == [1, 2, 3, 255])
        );
    }

    #[test]
    fn kittens_are_small_and_type_on_a_laptop() {
        let c = Coat::from_seed(5);
        let (mut a, mut b) = (Vec::new(), Vec::new());
        kitten_instances(&c, Mat4::IDENTITY, false, 0.0, &mut a);
        kitten_instances(&c, Mat4::IDENTITY, true, 0.0, &mut b);
        assert!(!a.iter().any(|i| i.mesh == MeshId::CatLaptop));
        assert_eq!(b.iter().filter(|i| i.mesh == MeshId::CatLaptop).count(), 1);
        let head =
            |v: &[Instance]| pos(v.iter().find(|i| i.mesh == MeshId::CatSkull).unwrap().model);
        let mut grown = Vec::new();
        cat_instances(
            &CatJoints::default(),
            &c,
            Mat4::IDENTITY,
            None,
            0.0,
            &mut grown,
        );
        let ratio = head(&a).y / head(&grown).y;
        assert!((ratio - KITTEN_SCALE).abs() < 1e-3, "{ratio}");
        assert!(tris(&b) <= 1500);
        // looking: the head turns, the paws and the laptop stay
        let mut l = Vec::new();
        kitten_instances_looking(&c, Mat4::IDENTITY, true, 0.9, 0.0, &mut l);
        assert_eq!(l.len(), b.len());
        let face = |v: &[Instance]| {
            v.iter()
                .find(|i| i.mesh == MeshId::CatSkull)
                .unwrap()
                .model
                .transform_vector3(Vec3::NEG_Z)
        };
        assert!(face(&l).normalize().dot(face(&b).normalize()) < 0.7);
        let paws = |v: &[Instance]| -> Vec<Mat4> {
            v.iter()
                .filter(|i| i.mesh == MeshId::CatPawFront || i.mesh == MeshId::CatLaptop)
                .map(|i| i.model)
                .collect()
        };
        assert_eq!(paws(&l), paws(&b));
    }

    #[test]
    fn gait_moves_diagonal_pairs_together_and_opposite() {
        for k in 0..32 {
            let d = k as f32 * 0.037;
            let g = gait_at(d, 0.9);
            let [fl, fr, bl, br] = g.legs;
            assert!((fl - br).abs() < 1e-6 && (fr - bl).abs() < 1e-6, "{g:?}");
            assert!((fl + fr).abs() < 1e-6, "pairs opposite: {g:?}");
            assert!(g.bob >= 0.0 && g.bob <= 0.025 + 1e-6, "{g:?}");
            assert!((0.0..TAU).contains(&g.phase));
            // one stride later it is the same gait
            let h = gait_at(d + STRIDE_M, 0.9);
            assert!((g.bob - h.bob).abs() < 1e-4 && (g.legs[0] - h.legs[0]).abs() < 1e-4);
        }
        // a quarter stride: the widest swing; half a stride: the other pair forward
        let q = gait_at(STRIDE_M / 4.0, 0.9);
        let t = gait_at(STRIDE_M * 0.75, 0.9);
        assert!(q.legs[0] > 0.3 && t.legs[0] < -0.3);
        assert!(q.bob > 0.02, "up when the legs are out: {q:?}");
        // slow walkers swing less, standing ones not at all
        assert!(gait_at(STRIDE_M / 4.0, 0.3).legs[0] < q.legs[0]);
        assert_eq!(gait_at(1.234, 0.0), Gait::NEUTRAL);
        assert_eq!(gait_at(f32::NAN, 1.0), Gait::NEUTRAL);
        assert!(gait_at(1.0, 0.9).lean > 0.0);
    }

    fn walker(g: &Gait, root: Mat4) -> Vec<Instance> {
        let mut v = Vec::new();
        cat_walking(
            &CatJoints::default(),
            g,
            &Coat::from_seed(7),
            root,
            Some([200, 80, 80, 255]),
            0.0,
            &mut v,
        );
        v
    }

    #[test]
    fn a_walking_cat_stands_on_four_paws() {
        let root =
            Mat4::from_rotation_translation(Quat::from_rotation_y(0.6), Vec3::new(2.0, 0.0, -1.0));
        for phase in [0.0f32, PI] {
            let g = gait_at(STRIDE_M * phase / TAU, 0.9);
            let v = walker(&g, root);
            let paws: Vec<_> = v.iter().filter(|i| i.mesh == MeshId::CatPawFront).collect();
            assert_eq!(paws.len(), 4);
            for p in paws {
                let foot = p
                    .model
                    .transform_point3(Vec3::new(0.0, -walk::LEG_LEN, -0.01));
                assert!(foot.y.abs() <= 0.01, "phase {phase}: paw at {foot}");
            }
        }
        // the paws swing about the hips: mid-stride the front-left is ahead of the front-right
        let g = gait_at(STRIDE_M / 4.0, 0.9);
        let v = walker(&g, Mat4::IDENTITY);
        let feet: Vec<Vec3> = v
            .iter()
            .filter(|i| i.mesh == MeshId::CatPawFront)
            .map(|i| {
                i.model
                    .transform_point3(Vec3::new(0.0, -walk::LEG_LEN, 0.0))
            })
            .collect();
        assert!(feet[0].z < feet[1].z - 0.05, "{feet:?}");
        assert!(feet[3].z < feet[2].z - 0.05, "{feet:?}");
    }

    #[test]
    fn a_walking_cat_has_the_seated_parts_and_two_more_legs() {
        for seed in 0..64u64 {
            let c = Coat::from_seed(seed);
            let (mut sat, mut walking) = (Vec::new(), Vec::new());
            cat_instances(
                &CatJoints::default(),
                &c,
                Mat4::IDENTITY,
                None,
                0.3,
                &mut sat,
            );
            cat_walking(
                &CatJoints::default(),
                &gait_at(0.3, 0.9),
                &c,
                Mat4::IDENTITY,
                None,
                0.3,
                &mut walking,
            );
            assert_eq!(walking.len(), sat.len() + 2, "seed {seed}");
            assert!(walking.iter().all(|i| i.fade == 0.3));
            assert!(tris(&walking) <= 1700);
            // the same tints part for part
            let tints = |v: &[Instance], m| {
                let mut t: Vec<_> = v.iter().filter(|i| i.mesh == m).map(|i| i.tint).collect();
                t.sort();
                t.dedup();
                t
            };
            for m in [
                MeshId::CatTorso,
                MeshId::CatSkull,
                MeshId::CatEarOuter,
                MeshId::CatTailSeg,
            ] {
                assert_eq!(tints(&walking, m), tints(&sat, m), "{m:?}");
            }
        }
    }

    #[test]
    fn a_walking_cat_is_low_level_and_faces_its_way() {
        let yaw = 1.1;
        let root =
            Mat4::from_rotation_translation(Quat::from_rotation_y(-yaw), Vec3::new(1.0, 0.0, 3.0));
        for d in [0.0f32, 0.1, 0.2, 0.3] {
            let v = walker(&gait_at(d, 0.9), root);
            let top = v
                .iter()
                .filter(|i| i.mesh == MeshId::CatEarOuter)
                .map(|i| i.model.transform_point3(Vec3::new(0.0, 0.11, 0.0)).y)
                .fold(0.0f32, f32::max);
            assert!(top <= 0.45, "ear tips at {top}");
            let f = cat_walking_frames(&CatJoints::default(), &gait_at(d, 0.9), root);
            // the torso lies level: its long axis is horizontal
            let along = f.body.transform_vector3(Vec3::Y).normalize();
            assert!(along.y.abs() < 0.1, "{along}");
            // the head is ahead of the body, the tail behind
            let fwd = root.transform_vector3(Vec3::NEG_Z);
            let body = f.body.transform_point3(Vec3::ZERO);
            assert!((f.head.transform_point3(Vec3::ZERO) - body).dot(fwd) > 0.2);
            assert!((f.tail[0].transform_point3(Vec3::ZERO) - body).dot(fwd) < -0.1);
            // hips at 0.26 m when not bobbing
        }
        let f = cat_walking_frames(&CatJoints::default(), &Gait::NEUTRAL, Mat4::IDENTITY);
        let body = f.body.transform_point3(Vec3::ZERO);
        assert!((body.y - (walk::HIP_Y + walk::SHOULDER_Y) / 2.0).abs() < 1e-5);
        // the head turns on top of the walk
        let look = CatJoints {
            head_rot_y: 0.8,
            ..CatJoints::default()
        };
        let turned = cat_walking_frames(&look, &Gait::NEUTRAL, Mat4::IDENTITY);
        assert!(
            (turned.head.transform_vector3(Vec3::NEG_Z).normalize()
                - f.head.transform_vector3(Vec3::NEG_Z).normalize())
            .length()
                > 0.5
        );
    }

    #[test]
    fn the_monitor_neck_is_sturdy_with_a_flared_foot() {
        let m = crate::mesh::build(MeshId::MonitorStand);
        let r_at = |y0: f32, y1: f32| {
            m.vertices
                .iter()
                .filter(|v| v.pos[1] > y0 && v.pos[1] < y1)
                .map(|v| Vec2::new(v.pos[0], v.pos[2]).length())
                .fold(0.0f32, f32::max)
        };
        assert!(
            (r_at(0.075, 0.905) - 0.035).abs() < 1e-3,
            "Ø {}",
            2.0 * r_at(0.075, 0.905)
        );
        assert!(r_at(-0.01, 0.005) > 0.055, "flared foot");
    }

    #[test]
    fn the_monitor_stands_on_its_foot_under_the_screen() {
        let foot = Mat4::from_rotation_translation(
            Quat::from_rotation_y(-0.3),
            Vec3::new(1.0, 0.75, -3.0),
        );
        let size = Vec2::new(0.9, 0.56);
        let lift = 0.2;
        let mut v = Vec::new();
        monitor(foot, size, lift, &mut v);
        let kinds: Vec<_> = v.iter().map(|i| i.mesh).collect();
        assert_eq!(
            kinds,
            [
                MeshId::MonitorStand,
                MeshId::MonitorFoot,
                MeshId::MonitorFrame,
                MeshId::RoundedBox
            ]
        );
        // the bezel is centred on the screen, 2 cm wider on every side
        let screen = monitor_screen(foot, size, lift);
        let frame = v[2].model;
        assert!(near(pos(frame), pos(screen), 1e-5));
        assert!((frame.transform_vector3(Vec3::X).length() - (size.x + 0.04)).abs() < 1e-5);
        // the screen bottom is `lift` above the foot
        let bottom = screen.transform_point3(Vec3::new(0.0, -0.5, 0.0));
        assert!((bottom.y - (0.75 + lift)).abs() < 1e-5);
        // the neck reaches from the desk into the bezel, the foot lies on the desk
        let neck = v[0].model;
        assert!((pos(neck).y - 0.75).abs() < 1e-5);
        let top = neck.transform_point3(Vec3::Y).y;
        assert!(top > bottom.y && top < bottom.y + size.y / 2.0);
        assert!((pos(v[1].model).y - 0.75).abs() < 1e-5);
        let mut s = Vec::new();
        monitor_stand(foot, size, lift, &mut s);
        assert_eq!(s.as_slice(), &v[..2]);
    }

    #[test]
    fn door_leaf_swings_around_its_hinge() {
        let frame =
            Mat4::from_rotation_translation(Quat::from_rotation_y(0.4), Vec3::new(2.0, 0.0, -1.0));
        for left in [true, false] {
            let closed = door_leaf(frame, 0.0, left);
            let open = door_leaf(frame, 1.0, left);
            let hinge = frame.transform_point3(Vec3::new(
                if left { -DOOR_HINGE_X } else { DOOR_HINGE_X },
                0.0,
                0.0,
            ));
            assert!(near(pos(closed), hinge, 1e-5) && near(pos(open), hinge, 1e-5));
            // closed: the leaf spans the opening along the frame's x
            let far = closed.transform_point3(Vec3::new(0.98, 0.0, 0.0));
            let mirror =
                frame.transform_point3(Vec3::new(if left { 0.49 } else { -0.49 }, 0.0, 0.0));
            assert!(near(far, mirror, 1e-4));
            let a = closed
                .transform_vector3(Vec3::X)
                .angle_between(open.transform_vector3(Vec3::X));
            assert!((a - DOOR_OPEN_RAD).abs() < 1e-4, "{a}");
            // it swings towards the frame's -z
            let tip = frame
                .inverse()
                .transform_point3(door_leaf(frame, 0.5, left).transform_point3(Vec3::X * 0.98));
            assert!(tip.z < -0.3, "{tip}");
        }
        assert_eq!(
            door_leaf(frame, 0.0, true),
            frame * Mat4::from_translation(Vec3::new(-DOOR_HINGE_X, 0.0, 0.0))
        );
        assert_eq!(door_leaf(frame, 7.0, true), door_leaf(frame, 1.0, true));
    }

    #[test]
    fn clock_hands_point_at_the_time() {
        let dir = |i: &Instance| i.model.transform_vector3(Vec3::Y).normalize();
        let [h, m] = clock_hands(Mat4::IDENTITY, 3, 0);
        assert!(near(dir(&h), Vec3::X, 1e-5), "3 o'clock: {}", dir(&h));
        assert!(near(dir(&m), Vec3::Y, 1e-5));
        let [h, m] = clock_hands(Mat4::IDENTITY, 18, 30);
        assert!(near(dir(&m), Vec3::NEG_Y, 1e-5));
        let a = 195f32.to_radians();
        assert!(near(dir(&h), Vec3::new(a.sin(), a.cos(), 0.0), 1e-5));
        // the minute hand is longer and in front
        assert!(
            m.model.transform_vector3(Vec3::Y).length()
                > h.model.transform_vector3(Vec3::Y).length()
        );
        assert!(pos(m.model).z > pos(h.model).z);
    }

    #[test]
    fn turntable_parts_move_with_their_inputs() {
        use crate::kit::PLATTER_AT;
        let (mut a, mut b) = (Vec::new(), Vec::new());
        turntable(Mat4::IDENTITY, 0.0, false, 0.0, &mut a);
        turntable(Mat4::IDENTITY, 1.0, true, 0.5, &mut b);
        assert_eq!(a.len(), 6);
        let find = |v: &[Instance], m: MeshId| v.iter().find(|i| i.mesh == m).unwrap().model;
        assert_ne!(find(&a, MeshId::Platter), find(&b, MeshId::Platter));
        assert_ne!(find(&a, MeshId::Knob), find(&b, MeshId::Knob));
        assert_eq!(find(&a, MeshId::Turntable), find(&b, MeshId::Turntable));
        // needle down: the headshell is over the record, parked it is off it and higher
        let tip = |m: Mat4| m.transform_point3(Vec3::new(0.0, 0.032, 0.228));
        let (up, down) = (
            tip(find(&a, MeshId::ToneArm)),
            tip(find(&b, MeshId::ToneArm)),
        );
        let r = |p: Vec3| Vec2::new(p.x - PLATTER_AT.x, p.z - PLATTER_AT.z).length();
        assert!(r(down) < 0.15 && r(up) > 0.155, "{} {}", r(down), r(up));
        assert!(up.y > down.y);
    }

    #[test]
    fn plant_leaves_are_bars() {
        for bars in 0..=4u8 {
            let mut v = Vec::new();
            plant(Mat4::IDENTITY, bars, &mut v);
            let leaves: Vec<_> = v.iter().filter(|i| i.mesh == MeshId::Plant).collect();
            assert_eq!(leaves.len(), 4);
            let shown = leaves.iter().filter(|i| i.tint[3] == 255).count();
            assert_eq!(shown, if bars == 0 { 4 } else { bars as usize });
        }
    }

    #[test]
    fn stateful_furniture_follows_its_value() {
        let height = |v: &[Instance], m: MeshId| {
            v.iter()
                .filter(|i| i.mesh == m)
                .map(|i| i.model.transform_vector3(Vec3::Y).length())
                .fold(0.0, f32::max)
        };
        let (mut lo, mut hi) = (Vec::new(), Vec::new());
        oil_lamp(Mat4::IDENTITY, 0.1, &mut lo);
        oil_lamp(Mat4::IDENTITY, 0.9, &mut hi);
        assert!(height(&hi, MeshId::Cylinder) > 5.0 * height(&lo, MeshId::Cylinder));
        let mut empty = Vec::new();
        oil_lamp(Mat4::IDENTITY, 0.0, &mut empty);
        assert_eq!(empty.len(), 1, "no oil, no flame");
        let (mut cold, mut hot) = (Vec::new(), Vec::new());
        fireplace(Mat4::IDENTITY, 0.0, &mut cold);
        fireplace(Mat4::IDENTITY, 1.0, &mut hot);
        assert!(height(&hot, MeshId::Flame) > 2.0 * height(&cold, MeshId::Flame));
        let (mut clear, mut murky) = (Vec::new(), Vec::new());
        aquarium(Mat4::IDENTITY, 0.0, 0.0, &mut clear);
        aquarium(Mat4::IDENTITY, 1.0, 0.0, &mut murky);
        assert_eq!(murky.len(), clear.len() + 1);
        let (mut shut, mut open) = (Vec::new(), Vec::new());
        safe(Mat4::IDENTITY, 0.0, &mut shut);
        safe(Mat4::IDENTITY, 1.0, &mut open);
        assert_ne!(shut[1].model, open[1].model);
        let tip = open[1].model.transform_point3(Vec3::X * 0.46);
        assert!(tip.z > 0.6, "the safe door opens towards the viewer: {tip}");
        let (mut off, mut on) = (Vec::new(), Vec::new());
        on_air(Mat4::IDENTITY, false, &mut off);
        on_air(Mat4::IDENTITY, true, &mut on);
        assert!(on[0].emissive > 0.5 && off[0].emissive == 0.0);
    }
}
