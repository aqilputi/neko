//! The wave-2 kit (S10): the monitor body, the villagers' parts and the
//! furniture of the objects addendum. Pure geometry, like [`crate::mesh`].
//!
//! Everything round is a *lathe* (a profile turned around y) squashed or
//! bent afterwards; boxy things are [`rounded_box`]es. Normals of lathed
//! and bent parts are recomputed from the faces (smooth, area weighted),
//! so the winding and the normals always agree.
//!
//! Conventions (metres, y up, as in `mesh`):
//! - floor furniture (`Turntable`, `Speaker`, `Fireplace`, `Aquarium`,
//!   `Lamp`, `Safe`, `Package`, `Cushion`, `Pot`, `OilJar`, `Jar`) stands on
//!   y = 0, centred in x and z, its front towards +z;
//! - wall things (`WallClock`, `Calendar`, `Board`, `OnAir`) have their back
//!   on z = 0 and their centre on x = y = 0, their face towards +z;
//! - `SafeDoor` hangs from its hinge at x = 0 (like `DoorLeaf`), its back on
//!   z = 0; `ClockHand` points up +y from its pivot at the origin (unit
//!   length: scale it by the hand's length);
//! - `MonitorStand` is a unit-height neck from y = 0 (scale y by the lift),
//!   `MonitorFoot` a unit-footprint plate on y = 0 (scale x and z);
//! - `Platter`, `ToneArm`, `Knob` sit on y = 0 at their spindle/pivot;
//!   `Plant` is one leaf growing from the origin up +y, leaning to +z;
//!   `Flame` rises from the origin; `Fish` swims towards +x;
//! - cat parts are centred on their joint (see `rig`), the cat looking
//!   towards -z; parts meant to take the coat (`CatTorso`, `CatSkull`, ears,
//!   paws, tail, chest, stripes, patches, muzzle, collar) are near-white.

use glam::{Mat4, Vec2, Vec3};

use crate::draw::MeshId;
use crate::mesh::palette::*;
use crate::mesh::{Mesh, Vertex, cuboid, cylinder, pack_normal, rounded_box};

// ---------------------------------------------------------------- shapes

/// Turns `profile` (radius, y, colour), listed bottom to top, around y.
/// A radius of 0 is a pole (one vertex); a point listed twice is a hard
/// edge (the colour may change there).
pub fn lathe(profile: &[(f32, f32, Rgba)], sides: u32) -> Mesh {
    // a sharp turn of the profile (> ~75°) is a hard edge too
    let mut sharp = Vec::with_capacity(profile.len() * 2);
    for (i, &p) in profile.iter().enumerate() {
        sharp.push(p);
        if i == 0 || i + 1 == profile.len() || p.0 < 1e-6 {
            continue;
        }
        let d = |a: (f32, f32, Rgba), b: (f32, f32, Rgba)| Vec2::new(b.0 - a.0, b.1 - a.1);
        let (din, dout) = (d(profile[i - 1], p), d(p, profile[i + 1]));
        if din.length() > 1e-6 && dout.length() > 1e-6 && din.angle_to(dout).abs() > 1.3 {
            sharp.push(p);
        }
    }
    let profile = &sharp[..];
    let n = sides.max(3) as u16;
    let ring = |i: u16| {
        let a = std::f32::consts::TAU * i as f32 / n as f32;
        Vec3::new(a.cos(), 0.0, -a.sin())
    };
    let mut m = Mesh::default();
    // per profile point: (first vertex, is a pole, (r, y))
    let mut rows = Vec::with_capacity(profile.len());
    for &(r, y, color) in profile {
        let first = m.vertices.len() as u16;
        let pole = r < 1e-6;
        let count = if pole { 1 } else { n };
        for i in 0..count {
            let p = ring(i) * r + Vec3::Y * y;
            m.vertices.push(Vertex {
                pos: p.to_array(),
                normal: [0, 127, 0, 0],
                color,
            });
        }
        rows.push((first, pole, Vec2::new(r, y)));
    }
    for w in rows.windows(2) {
        let ((l, lp, a), (u, up, b)) = (w[0], w[1]);
        if a.distance(b) < 1e-7 {
            continue; // a hard edge: no band
        }
        for i in 0..n {
            let j = (i + 1) % n;
            match (lp, up) {
                (true, true) => {}
                (true, false) => m.indices.extend([l, u + j, u + i]),
                (false, true) => m.indices.extend([l + i, l + j, u]),
                (false, false) => {
                    m.indices.extend([l + i, l + j, u + j, l + i, u + j, u + i]);
                }
            }
        }
    }
    smooth(&mut m);
    m
}

/// Recomputes every normal as the area-weighted mean of its faces.
pub fn smooth(m: &mut Mesh) {
    let mut acc = vec![Vec3::ZERO; m.vertices.len()];
    for t in m.indices.chunks(3) {
        let [a, b, c] = [t[0], t[1], t[2]].map(|i| Vec3::from(m.vertices[i as usize].pos));
        let f = (b - a).cross(c - a);
        for &i in t {
            acc[i as usize] += f;
        }
    }
    for (v, n) in m.vertices.iter_mut().zip(acc) {
        v.normal = pack_normal(n);
    }
}

/// Moves every vertex through `f` and recomputes the normals.
pub fn warp(mut m: Mesh, f: impl Fn(Vec3) -> Vec3) -> Mesh {
    for v in &mut m.vertices {
        v.pos = f(Vec3::from(v.pos)).to_array();
    }
    smooth(&mut m);
    m
}

/// Recolours the vertices `f` picks.
pub fn paint(mut m: Mesh, f: impl Fn(Vec3) -> Option<Rgba>) -> Mesh {
    for v in &mut m.vertices {
        if let Some(c) = f(Vec3::from(v.pos)) {
            v.color = c;
        }
    }
    m
}

/// A semicircle profile of `rings` steps (radius 1, y -1..1).
fn circle(rings: u32, color: Rgba) -> Vec<(f32, f32, Rgba)> {
    (0..=rings)
        .map(|k| {
            let t = std::f32::consts::PI * k as f32 / rings as f32;
            let r = if k == 0 || k == rings { 0.0 } else { t.sin() };
            (r, -t.cos(), color)
        })
        .collect()
}

/// An ellipsoid of half extents `half`, centred.
pub fn ellipsoid(half: Vec3, sides: u32, rings: u32, color: Rgba) -> Mesh {
    let mut m = Mesh::default();
    m.append(&lathe(&circle(rings, color), sides), Mat4::from_scale(half));
    m
}

/// A torus around y: ring radius `major`, tube radius `minor`.
pub fn torus(major: f32, minor: f32, sides: u32, tube: u32, color: Rgba) -> Mesh {
    let prof: Vec<_> = (0..=tube)
        .map(|k| {
            let t = -std::f32::consts::FRAC_PI_2 + std::f32::consts::TAU * k as f32 / tube as f32;
            (major + minor * t.cos(), minor * t.sin(), color)
        })
        .collect();
    lathe(&prof, sides)
}

/// A one-sided rectangle facing +z at depth `z`, centred on `c`.
pub fn quad(c: Vec2, size: Vec2, z: f32, color: Rgba) -> Mesh {
    let (x, y) = (size.x / 2.0, size.y / 2.0);
    let normal = pack_normal(Vec3::Z);
    let vertices = [[-x, -y], [x, -y], [x, y], [-x, y]]
        .map(|[a, b]| Vertex {
            pos: [c.x + a, c.y + b, z],
            normal,
            color,
        })
        .to_vec();
    Mesh {
        vertices,
        indices: vec![0, 1, 2, 0, 2, 3],
    }
}

/// `part` placed by `m` into `into`.
fn add(into: &mut Mesh, part: Mesh, m: Mat4) {
    into.append(&part, m);
}

fn at(p: Vec3) -> Mat4 {
    Mat4::from_translation(p)
}

/// y → z: lathed parts that face the viewer (clock, cones, dial).
fn facing_z(p: Vec3) -> Mat4 {
    at(p) * Mat4::from_rotation_x(std::f32::consts::FRAC_PI_2)
}

const fn rgba(r: u8, g: u8, b: u8) -> Rgba {
    [r, g, b, 255]
}

/// Darker vinyl, glass, flame and sign colours the palette does not have.
const VINYL: Rgba = rgba(58, 52, 62);
const GLASS: Rgba = rgba(214, 236, 234);
const BRASS: Rgba = rgba(224, 186, 104);
const SIGN_RED: Rgba = rgba(226, 84, 84);
const CARD: Rgba = rgba(222, 186, 136);
const CORK: Rgba = rgba(206, 160, 118);
const SOOT: Rgba = rgba(70, 56, 54);

// ---------------------------------------------------------------- boxes

/// The box each kit mesh promises to stay inside (min, max); tests check
/// the built mesh fills it.
pub fn stated_bounds(id: MeshId) -> Option<(Vec3, Vec3)> {
    use MeshId::*;
    let v = Vec3::new;
    Some(match id {
        MonitorStand => (v(-0.025, 0.0, -0.025), v(0.025, 1.0, 0.025)),
        MonitorFoot => (v(-0.5, 0.0, -0.5), v(0.5, 0.02, 0.5)),
        CatTorso => (v(-0.18, -0.23, -0.15), v(0.18, 0.23, 0.15)),
        CatSkull => (v(-0.19, -0.145, -0.15), v(0.19, 0.145, 0.15)),
        CatEarOuter => (v(-0.055, 0.0, -0.025), v(0.055, 0.11, 0.025)),
        CatEarInner => (v(-0.035, 0.0, -0.034), v(0.035, 0.08, -0.01)),
        CatEye => (v(-0.026, -0.034, -0.0135), v(0.026, 0.034, 0.012)),
        CatMuzzle => (v(-0.075, -0.045, -0.05), v(0.075, 0.035, 0.015)),
        CatPawFront => (v(-0.045, -0.25, -0.06), v(0.045, 0.035, 0.04)),
        CatPawBack => (v(-0.075, -0.1, -0.2), v(0.075, 0.09, 0.11)),
        CatTailSeg => (v(-0.032, -0.02, -0.032), v(0.032, 0.135, 0.032)),
        CatChest => (v(-0.1, -0.13, -0.04), v(0.1, 0.13, 0.04)),
        CatStripe => (v(-0.12, -0.02, -0.035), v(0.12, 0.02, 0.03)),
        CatPatch => (v(-0.08, -0.07, -0.03), v(0.08, 0.07, 0.03)),
        CatCollar => (v(-0.13, -0.02, -0.13), v(0.13, 0.02, 0.13)),
        CatLaptop => (v(-0.08, 0.0, -0.08), v(0.08, 0.105, 0.055)),
        Envelope => (v(-0.08, -0.05, -0.006), v(0.08, 0.05, 0.011)),
        Turntable => (v(-0.32, 0.0, -0.2), v(0.32, 0.64, 0.22)),
        Platter => (v(-0.155, 0.0, -0.155), v(0.155, 0.04, 0.155)),
        ToneArm => (v(-0.03, 0.0, -0.03), v(0.03, 0.07, 0.25)),
        Speaker => (v(-0.12, 0.0, -0.11), v(0.12, 0.4, 0.125)),
        WallClock => (v(-0.17, -0.17, 0.0), v(0.17, 0.25, 0.06)),
        ClockHand => (v(-0.07, -0.18, 0.0), v(0.07, 1.0, 0.05)),
        OilJar => (v(-0.1, 0.0, -0.1), v(0.1, 0.26, 0.1)),
        Cushion => (v(-0.23, 0.0, -0.23), v(0.23, 0.12, 0.23)),
        Plant => (v(-0.055, 0.0, -0.01), v(0.055, 0.3, 0.14)),
        Pot => (v(-0.13, 0.0, -0.13), v(0.13, 0.21, 0.13)),
        Fireplace => (v(-0.55, 0.0, -0.235), v(0.55, 1.05, 0.27)),
        Flame => (v(-0.05, 0.0, -0.05), v(0.05, 0.2, 0.05)),
        Aquarium => (v(-0.36, 0.0, -0.18), v(0.36, 1.0, 0.18)),
        Calendar => (v(-0.15, -0.21, -0.003), v(0.15, 0.225, 0.03)),
        Lamp => (v(-0.22, 0.0, -0.22), v(0.22, 1.45, 0.22)),
        Safe => (v(-0.27, 0.0, -0.22), v(0.27, 0.62, 0.23)),
        SafeDoor => (v(-0.016, -0.23, 0.0), v(0.46, 0.23, 0.08)),
        Jar => (v(-0.055, 0.0, -0.055), v(0.055, 0.13, 0.055)),
        Board => (v(-0.47, -0.32, 0.0), v(0.47, 0.32, 0.055)),
        Ball => (Vec3::splat(-0.5), Vec3::splat(0.5)),
        Knob => (v(-0.03, 0.0, -0.03), v(0.03, 0.035, 0.03)),
        Fish => (v(-0.035, -0.02, -0.008), v(0.035, 0.02, 0.008)),
        Package => (v(-0.18, 0.0, -0.152), v(0.18, 0.27, 0.154)),
        OnAir => (v(-0.24, -0.09, 0.0), v(0.24, 0.09, 0.07)),
        _ => return None,
    })
}

// ---------------------------------------------------------------- build

/// The wave-2 kit mesh for `id` (`None`: one of the phase-0 meshes).
pub fn build(id: MeshId) -> Option<Mesh> {
    use MeshId::*;
    Some(match id {
        MonitorStand => cylinder(0.025, 1.0, 12, CREAM).moved(Vec3::Y * 0.5),
        MonitorFoot => lathe(
            &[
                (0.0, 0.0, CREAM),
                (0.48, 0.0, CREAM),
                (0.5, 0.008, CREAM),
                (0.47, 0.02, CREAM),
                (0.0, 0.02, CREAM),
            ],
            20,
        ),
        CatTorso => torso(),
        CatSkull => skull(),
        CatEarOuter => ear(0.055, 0.11, 0.025, SNOW),
        CatEarInner => ear(0.035, 0.078, 0.012, SNOW).moved(Vec3::new(0.0, 0.002, -0.022)),
        CatEye => eye(),
        CatMuzzle => muzzle(),
        CatPawFront => paw_front(),
        CatPawBack => paw_back(),
        CatTailSeg => lathe(
            &[
                (0.0, -0.02, SNOW),
                (0.02, -0.012, SNOW),
                (0.027, 0.012, SNOW),
                (0.027, 0.105, SNOW),
                (0.02, 0.128, SNOW),
                (0.0, 0.135, SNOW),
            ],
            7,
        ),
        CatChest => ellipsoid(Vec3::new(0.1, 0.13, 0.04), 10, 6, SNOW),
        CatStripe => warp(ellipsoid(Vec3::new(0.12, 0.02, 0.03), 8, 3, SNOW), |p| {
            Vec3::new(p.x, p.y, p.z - 1.9 * p.x * p.x)
        }),
        CatPatch => ellipsoid(Vec3::new(0.08, 0.07, 0.03), 8, 4, SNOW),
        CatCollar => torus(0.112, 0.018, 12, 4, SNOW),
        CatLaptop => laptop(),
        Envelope => envelope(),
        Turntable => turntable(),
        Platter => platter(),
        ToneArm => tone_arm(),
        Speaker => speaker(),
        WallClock => wall_clock(),
        ClockHand => {
            let mut m = Mesh::default();
            add(
                &mut m,
                rounded_box(Vec3::new(0.11, 1.1, 0.05), 0.03, 1, INK),
                at(Vec3::new(0.0, 0.42, 0.025)),
            );
            add(
                &mut m,
                ellipsoid(Vec3::new(0.07, 0.07, 0.025), 8, 4, INK),
                at(Vec3::new(0.0, 0.0, 0.025)),
            );
            m
        }
        OilJar => oil_jar(),
        Cushion => cushion(),
        Plant => leaf(),
        Pot => pot(),
        Fireplace => fireplace(),
        Flame => lathe(
            &[
                (0.0, 0.0, BUTTER),
                (0.035, 0.01, BUTTER),
                (0.05, 0.045, PEACH),
                (0.042, 0.09, PEACH),
                (0.022, 0.145, CLAY),
                (0.0, 0.2, CLAY),
            ],
            10,
        ),
        Aquarium => aquarium(),
        Calendar => calendar(),
        Lamp => lamp(),
        Safe => safe(),
        SafeDoor => safe_door(),
        Jar => lathe(
            &[
                (0.0, 0.0, SNOW),
                (0.045, 0.0, SNOW),
                (0.055, 0.02, SNOW),
                (0.055, 0.085, SNOW),
                (0.042, 0.098, SNOW),
                (0.042, 0.098, SNOW),
                (0.05, 0.102, SNOW),
                (0.05, 0.125, SNOW),
                (0.03, 0.13, SNOW),
                (0.0, 0.13, SNOW),
            ],
            12,
        ),
        Board => board(),
        Ball => ellipsoid(Vec3::splat(0.5), 12, 8, SNOW),
        Knob => {
            let mut m = lathe(
                &[
                    (0.0, 0.0, CREAM),
                    (0.03, 0.0, CREAM),
                    (0.03, 0.008, CREAM),
                    (0.022, 0.012, CREAM),
                    (0.02, 0.03, CREAM),
                    (0.0, 0.032, CREAM),
                ],
                12,
            );
            add(
                &mut m,
                cuboid(Vec3::new(0.006, 0.006, 0.016), ROSE),
                at(Vec3::new(0.0, 0.031, 0.011)),
            );
            m
        }
        Fish => fish(),
        Package => package(),
        OnAir => on_air(),
        _ => return None,
    })
}

trait Moved {
    fn moved(self, by: Vec3) -> Mesh;
}

impl Moved for Mesh {
    fn moved(self, by: Vec3) -> Mesh {
        let mut m = Mesh::default();
        m.append(&self, at(by));
        m
    }
}

// ---------------------------------------------------------------- cat parts

/// The torso's radius profile (fraction of the widest), y from -1 (seat)
/// to 1 (shoulders): a pear, wide at the haunches, narrow at the neck.
pub(crate) const TORSO_PROFILE: [(f32, f32); 9] = [
    (0.0, -1.0),
    (0.72, -0.98),
    (0.96, -0.78),
    (1.0, -0.45),
    (0.94, -0.05),
    (0.82, 0.35),
    (0.62, 0.72),
    (0.34, 0.94),
    (0.0, 1.0),
];

fn torso() -> Mesh {
    let prof: Vec<_> = TORSO_PROFILE.iter().map(|&(r, y)| (r, y, SNOW)).collect();
    let mut m = Mesh::default();
    m.append(
        &lathe(&prof, 12),
        Mat4::from_scale(Vec3::new(0.18, 0.23, 0.15)),
    );
    m
}

fn skull() -> Mesh {
    // wide, soft-cornered: AC villager heads are a squashed ball
    let prof = [
        (0.0, -1.0),
        (0.55, -0.95),
        (0.88, -0.7),
        (1.0, -0.25),
        (0.98, 0.2),
        (0.86, 0.6),
        (0.55, 0.92),
        (0.0, 1.0),
    ];
    let prof: Vec<_> = prof.iter().map(|&(r, y)| (r, y, SNOW)).collect();
    let mut m = Mesh::default();
    m.append(
        &lathe(&prof, 14),
        Mat4::from_scale(Vec3::new(0.19, 0.145, 0.15)),
    );
    m
}

/// A rounded, flattened cone: base half-width `w`, height `h`, depth `d`.
fn ear(w: f32, h: f32, d: f32, color: Rgba) -> Mesh {
    let prof = [
        (0.0, 0.0, color),
        (1.0, 0.0, color),
        (0.55, 0.65, color),
        (0.18, 0.95, color),
        (0.0, 1.0, color),
    ];
    let mut m = Mesh::default();
    m.append(&lathe(&prof, 6), Mat4::from_scale(Vec3::new(w, h, d)));
    m
}

fn eye() -> Mesh {
    let mut m = ellipsoid(Vec3::new(0.026, 0.034, 0.012), 8, 4, INK);
    // the sparkle: a white dot up and to the cat's right, proud of the eye's front
    add(
        &mut m,
        ellipsoid(Vec3::new(0.0085, 0.0095, 0.004), 6, 3, SNOW),
        at(Vec3::new(0.008, 0.013, -0.0095)),
    );
    m
}

fn muzzle() -> Mesh {
    let mut m = Mesh::default();
    for s in [-1.0f32, 1.0] {
        add(
            &mut m,
            ellipsoid(Vec3::new(0.04, 0.03, 0.03), 6, 4, SNOW),
            at(Vec3::new(s * 0.034, -0.012, -0.016)),
        );
    }
    // the nose, pink, on top between the cheeks
    add(
        &mut m,
        ellipsoid(Vec3::new(0.017, 0.012, 0.012), 6, 3, ROSE),
        at(Vec3::new(0.0, 0.02, -0.035)),
    );
    m
}

fn paw_front() -> Mesh {
    let mut m = lathe(
        &[
            (0.0, -0.21, SNOW),
            (0.03, -0.205, SNOW),
            (0.034, -0.15, SNOW),
            (0.036, -0.05, SNOW),
            (0.034, 0.01, SNOW),
            (0.0, 0.035, SNOW),
        ],
        6,
    );
    add(
        &mut m,
        ellipsoid(Vec3::new(0.045, 0.034, 0.05), 6, 4, SNOW),
        at(Vec3::new(0.0, -0.215, -0.01)),
    );
    m
}

fn paw_back() -> Mesh {
    let mut m = ellipsoid(Vec3::new(0.075, 0.09, 0.11), 8, 5, SNOW);
    add(
        &mut m,
        ellipsoid(Vec3::new(0.042, 0.03, 0.06), 6, 3, SNOW),
        at(Vec3::new(0.0, -0.07, -0.14)),
    );
    m
}

fn laptop() -> Mesh {
    let mut m = Mesh::default();
    // cuboids: a kitten's laptop is small, and kittens spend their triangles on fur
    add(
        &mut m,
        cuboid(Vec3::new(0.16, 0.012, 0.11), STONE),
        at(Vec3::new(0.0, 0.006, 0.0)),
    );
    // the keyboard
    add(
        &mut m,
        quad(Vec2::new(0.0, 0.01), Vec2::new(0.13, 0.05), 0.0, INK),
        at(Vec3::new(0.0, 0.0125, 0.0)) * Mat4::from_rotation_x(-std::f32::consts::FRAC_PI_2),
    );
    // the lid, tilted back 15°, its screen towards the cat (+z)
    let lid = at(Vec3::new(0.0, 0.012, -0.052)) * Mat4::from_rotation_x(-0.26);
    add(
        &mut m,
        cuboid(Vec3::new(0.16, 0.095, 0.008), STONE),
        lid * at(Vec3::new(0.0, 0.0475, 0.0)),
    );
    add(
        &mut m,
        quad(Vec2::new(0.0, 0.0475), Vec2::new(0.14, 0.078), 0.0045, SKY),
        lid,
    );
    m
}

fn envelope() -> Mesh {
    let mut m = rounded_box(Vec3::new(0.16, 0.1, 0.012), 0.003, 1, CREAM);
    // the flap: a thin wedge on the front, pointing down
    let flap = Mesh {
        vertices: [[-0.078, 0.048], [0.0, -0.005], [0.078, 0.048]]
            .map(|[x, y]| Vertex {
                pos: [x, y, 0.0075],
                normal: pack_normal(Vec3::Z),
                color: SAND,
            })
            .to_vec(),
        indices: vec![0, 1, 2],
    };
    add(&mut m, flap, Mat4::IDENTITY);
    add(
        &mut m,
        ellipsoid(Vec3::new(0.012, 0.012, 0.003), 8, 3, ROSE),
        at(Vec3::new(0.0, 0.0, 0.0075)),
    );
    m
}

// ---------------------------------------------------------------- vitrola

/// Where the platter's spindle, the arm's pivot and the knob sit on the
/// console (its local frame); `rig::turntable` puts the parts there.
pub(crate) const DECK_Y: f32 = 0.625;
pub(crate) const PLATTER_AT: Vec3 = Vec3::new(-0.07, DECK_Y, 0.0);
pub(crate) const ARM_AT: Vec3 = Vec3::new(0.19, DECK_Y, -0.13);
pub(crate) const KNOB_AT: Vec3 = Vec3::new(0.24, DECK_Y, 0.13);

fn turntable() -> Mesh {
    let mut m = Mesh::default();
    // the cabinet on four short tapered legs
    add(
        &mut m,
        rounded_box(Vec3::new(0.62, 0.42, 0.38), 0.035, 1, WOOD),
        at(Vec3::new(0.0, 0.39, 0.0)),
    );
    add(
        &mut m,
        rounded_box(Vec3::new(0.5, 0.26, 0.02), 0.01, 1, SAND),
        at(Vec3::new(0.0, 0.37, 0.19)),
    );
    for x in [-0.13, 0.13] {
        add(
            &mut m,
            ellipsoid(Vec3::new(0.018, 0.018, 0.012), 6, 3, BRASS),
            at(Vec3::new(x, 0.37, 0.205)),
        );
    }
    for (x, z) in [(-0.26, -0.14), (0.26, -0.14), (-0.26, 0.14), (0.26, 0.14)] {
        add(
            &mut m,
            lathe(
                &[
                    (0.0, 0.0, WOOD_DARK),
                    (0.016, 0.0, WOOD_DARK),
                    (0.026, 0.18, WOOD_DARK),
                    (0.0, 0.18, WOOD_DARK),
                ],
                6,
            ),
            at(Vec3::new(x, 0.0, z)),
        );
    }
    // the deck plate
    add(
        &mut m,
        rounded_box(Vec3::new(0.64, 0.025, 0.4), 0.01, 1, CREAM),
        at(Vec3::new(0.0, 0.6125, 0.0)),
    );
    m
}

fn platter() -> Mesh {
    let mut m = lathe(
        &[
            (0.0, 0.0, STONE),
            (0.155, 0.0, STONE),
            (0.155, 0.014, STONE),
            (0.155, 0.014, VINYL),
            (0.15, 0.022, VINYL),
            (0.15, 0.022, VINYL),
            (0.05, 0.022, VINYL),
            (0.05, 0.022, ROSE),
            (0.05, 0.023, ROSE),
            (0.008, 0.023, ROSE),
            (0.008, 0.023, IRON),
            (0.005, 0.04, IRON),
            (0.0, 0.04, IRON),
        ],
        24,
    );
    // a bright mark on the label so the spin shows
    add(
        &mut m,
        cuboid(Vec3::new(0.035, 0.002, 0.012), BUTTER),
        at(Vec3::new(0.025, 0.024, 0.0)),
    );
    // one groove ring
    add(
        &mut m,
        torus(0.105, 0.0015, 24, 3, INK),
        at(Vec3::new(0.0, 0.0225, 0.0)),
    );
    m
}

fn tone_arm() -> Mesh {
    let mut m = lathe(
        &[
            (0.0, 0.0, IRON),
            (0.03, 0.0, IRON),
            (0.026, 0.04, IRON),
            (0.016, 0.05, IRON),
            (0.0, 0.05, IRON),
        ],
        10,
    );
    add(
        &mut m,
        rounded_box(Vec3::new(0.012, 0.012, 0.22), 0.005, 1, SNOW),
        at(Vec3::new(0.0, 0.05, 0.11)),
    );
    add(
        &mut m,
        rounded_box(Vec3::new(0.03, 0.02, 0.04), 0.006, 1, INK),
        at(Vec3::new(0.0, 0.042, 0.228)),
    );
    // counterweight behind the pivot is skipped: the pivot is at the back edge
    m
}

fn speaker() -> Mesh {
    let mut m =
        rounded_box(Vec3::new(0.24, 0.4, 0.22), 0.03, 1, WOOD_DARK).moved(Vec3::new(0.0, 0.2, 0.0));
    add(
        &mut m,
        rounded_box(Vec3::new(0.2, 0.36, 0.01), 0.008, 1, SAND),
        at(Vec3::new(0.0, 0.2, 0.108)),
    );
    for (y, r) in [(0.27, 0.07), (0.11, 0.045)] {
        add(
            &mut m,
            lathe(
                &[
                    (0.0, 0.0, INK),
                    (r, 0.0, STONE),
                    (r * 0.92, 0.006, STONE),
                    (r * 0.3, 0.004, INK),
                    (0.0, 0.009, INK),
                ],
                12,
            ),
            facing_z(Vec3::new(0.0, y, 0.113)),
        );
    }
    m
}

// ---------------------------------------------------------------- walls

fn wall_clock() -> Mesh {
    let mut m = Mesh::default();
    // the wooden rim and the cream face, lathed along z
    add(
        &mut m,
        lathe(
            &[
                (0.0, 0.0, WOOD),
                (0.16, 0.0, WOOD),
                (0.17, 0.02, WOOD),
                (0.165, 0.045, WOOD),
                (0.148, 0.05, WOOD),
                (0.148, 0.05, CREAM),
                (0.0, 0.052, CREAM),
            ],
            24,
        ),
        Mat4::from_rotation_x(std::f32::consts::FRAC_PI_2),
    );
    for k in 0..12 {
        let a = std::f32::consts::TAU * k as f32 / 12.0;
        let big = k % 3 == 0;
        let r = 0.122;
        let s = if big {
            Vec2::new(0.014, 0.026)
        } else {
            Vec2::new(0.008, 0.012)
        };
        let tick = quad(Vec2::ZERO, s, 0.0, INK);
        add(
            &mut m,
            tick,
            at(Vec3::new(a.sin() * r, a.cos() * r, 0.0525)) * Mat4::from_rotation_z(-a),
        );
    }
    // two little bells on top: an alarm clock that lives on the wall
    for s in [-1.0f32, 1.0] {
        add(
            &mut m,
            ellipsoid(Vec3::new(0.045, 0.035, 0.022), 8, 4, BUTTER),
            at(Vec3::new(s * 0.09, 0.19, 0.025)) * Mat4::from_rotation_z(-s * 0.5),
        );
    }
    add(
        &mut m,
        ellipsoid(Vec3::new(0.014, 0.014, 0.008), 6, 3, BRASS),
        at(Vec3::new(0.0, 0.0, 0.053)),
    );
    m
}

fn calendar() -> Mesh {
    let mut m =
        rounded_box(Vec3::new(0.3, 0.42, 0.012), 0.004, 1, SNOW).moved(Vec3::new(0.0, 0.0, 0.006));
    add(
        &mut m,
        rounded_box(Vec3::new(0.3, 0.1, 0.016), 0.005, 1, ROSE),
        at(Vec3::new(0.0, 0.16, 0.008)),
    );
    for x in [-0.08, 0.08] {
        add(
            &mut m,
            torus(0.012, 0.003, 8, 3, IRON),
            at(Vec3::new(x, 0.205, 0.012)) * Mat4::from_rotation_z(std::f32::consts::FRAC_PI_2),
        );
    }
    for row in 0..4 {
        for col in 0..5 {
            let c = Vec2::new(-0.112 + 0.056 * col as f32, 0.065 - 0.06 * row as f32);
            let color = if (row, col) == (1, 2) { ROSE } else { SAND };
            add(
                &mut m,
                quad(c, Vec2::new(0.04, 0.042), 0.0125, color),
                Mat4::IDENTITY,
            );
        }
    }
    // a post-it
    add(
        &mut m,
        quad(Vec2::new(0.07, -0.16), Vec2::new(0.07, 0.06), 0.013, BUTTER),
        Mat4::IDENTITY,
    );
    m
}

fn board() -> Mesh {
    let mut m = Mesh::default();
    add(
        &mut m,
        rounded_box(Vec3::new(0.94, 0.64, 0.035), 0.015, 1, WOOD),
        at(Vec3::new(0.0, 0.0, 0.0175)),
    );
    add(
        &mut m,
        cuboid(Vec3::new(0.84, 0.54, 0.01), CORK),
        at(Vec3::new(0.0, 0.0, 0.035)),
    );
    let notes = [
        (Vec2::new(-0.27, 0.12), Vec2::new(0.2, 0.16), CREAM, -0.08),
        (Vec2::new(0.02, 0.14), Vec2::new(0.16, 0.14), BUTTER, 0.06),
        (Vec2::new(0.27, 0.1), Vec2::new(0.18, 0.2), MINT, -0.04),
        (Vec2::new(-0.18, -0.13), Vec2::new(0.22, 0.14), SKY, 0.05),
        (Vec2::new(0.16, -0.14), Vec2::new(0.17, 0.15), ROSE, -0.1),
    ];
    for (c, s, color, tilt) in notes {
        add(
            &mut m,
            quad(Vec2::ZERO, s, 0.0, color),
            at(Vec3::new(c.x, c.y, 0.0405)) * Mat4::from_rotation_z(tilt),
        );
        add(
            &mut m,
            ellipsoid(Vec3::new(0.012, 0.012, 0.008), 6, 3, CLAY),
            at(Vec3::new(c.x, c.y + s.y * 0.38, 0.044)),
        );
    }
    m
}

fn on_air() -> Mesh {
    let mut m = rounded_box(Vec3::new(0.48, 0.18, 0.06), 0.025, 1, SIGN_RED)
        .moved(Vec3::new(0.0, 0.0, 0.03));
    // "ON AIR" in a 5-row pixel font, as quads on the face ("" = a space)
    const GLYPHS: [[&str; 5]; 6] = [
        ["###", "#.#", "#.#", "#.#", "###"],      // O
        ["#..#", "##.#", "#.##", "#..#", "#..#"], // N
        ["", "", "", "", ""],                     //
        [".#.", "#.#", "###", "#.#", "#.#"],      // A
        ["#", "#", "#", "#", "#"],                // I
        ["##.", "#.#", "##.", "#.#", "#.#"],      // R
    ];
    let px = 0.02;
    let width = |g: &[&str; 5]| g[0].len().max(1) as f32;
    let total: f32 = GLYPHS.iter().map(width).sum::<f32>() + (GLYPHS.len() - 1) as f32;
    let mut x = -total * px / 2.0;
    for g in &GLYPHS {
        for (row, line) in g.iter().enumerate() {
            // merge runs of '#' into one quad
            let bytes = line.as_bytes();
            let mut col = 0;
            while col < bytes.len() {
                if bytes[col] != b'#' {
                    col += 1;
                    continue;
                }
                let start = col;
                while col < bytes.len() && bytes[col] == b'#' {
                    col += 1;
                }
                let w = (col - start) as f32 * px;
                let cx = x + start as f32 * px + w / 2.0;
                let cy = (2.0 - row as f32) * px;
                add(
                    &mut m,
                    quad(Vec2::new(cx, cy), Vec2::new(w, px), 0.0605, SNOW),
                    Mat4::IDENTITY,
                );
            }
        }
        x += (width(g) + 1.0) * px;
    }
    // the bracket on top
    add(
        &mut m,
        cuboid(Vec3::new(0.04, 0.02, 0.06), IRON),
        at(Vec3::new(0.0, 0.08, 0.03)),
    );
    m
}

// ---------------------------------------------------------------- floor things

fn oil_jar() -> Mesh {
    // the straight part (y 0.01..0.17, r 0.085) is where `rig::oil_lamp` paints the oil
    let mut m = lathe(
        &[
            (0.0, 0.0, GLASS),
            (0.075, 0.0, GLASS),
            (0.085, 0.01, GLASS),
            (0.085, 0.17, GLASS),
            (0.07, 0.19, GLASS),
            (0.05, 0.2, GLASS),
            (0.05, 0.2, WOOD),
            (0.056, 0.205, WOOD),
            (0.056, 0.23, WOOD),
            (0.012, 0.235, WOOD),
            (0.012, 0.235, BRASS),
            (0.01, 0.255, BRASS),
            (0.0, 0.255, BRASS),
        ],
        14,
    );
    // a cream label band would hide the oil: a small tag on the lid instead
    add(
        &mut m,
        cuboid(Vec3::new(0.05, 0.03, 0.004), CREAM),
        at(Vec3::new(0.0, 0.215, 0.058)),
    );
    m
}

fn cushion() -> Mesh {
    let m = rounded_box(Vec3::new(0.42, 0.1, 0.42), 0.045, 2, ROSE);
    let mut m = warp(m, |p| {
        // puffy: the middle of the top rises
        let d = (p.x * p.x + p.z * p.z).sqrt() / 0.3;
        let k = (1.0 - d.min(1.0)).powi(2);
        let y = if p.y > 0.0 {
            p.y * (1.0 + 0.4 * k)
        } else {
            p.y
        };
        Vec3::new(p.x, y + 0.05, p.z)
    });
    for (x, z) in [(-0.2, -0.2), (0.2, -0.2), (-0.2, 0.2), (0.2, 0.2)] {
        add(
            &mut m,
            ellipsoid(Vec3::new(0.022, 0.02, 0.022), 6, 3, BUTTER),
            at(Vec3::new(x, 0.05, z)),
        );
    }
    m
}

fn leaf() -> Mesh {
    let blade = ellipsoid(Vec3::new(0.055, 0.15, 0.009), 8, 6, LEAF);
    let blade = paint(blade, |p| (p.y > 0.08).then_some(MINT));
    warp(blade.moved(Vec3::new(0.0, 0.15, 0.0)), |p| {
        let t = p.y / 0.3;
        Vec3::new(
            p.x * (1.0 - 0.2 * t),
            p.y * (1.0 - 0.1 * t * t),
            p.z + 0.13 * t * t,
        )
    })
}

fn pot() -> Mesh {
    lathe(
        &[
            (0.0, 0.0, CLAY),
            (0.085, 0.0, CLAY),
            (0.095, 0.02, CLAY),
            (0.11, 0.12, CLAY),
            (0.11, 0.12, CREAM),
            (0.115, 0.15, CREAM),
            (0.115, 0.15, CLAY),
            (0.13, 0.17, CLAY),
            (0.13, 0.205, CLAY),
            (0.115, 0.21, CLAY),
            (0.112, 0.19, CLAY),
            (0.112, 0.19, WOOD_DARK),
            (0.0, 0.195, WOOD_DARK),
        ],
        16,
    )
}

fn fireplace() -> Mesh {
    let mut m = Mesh::default();
    // pillars and lintel around a dark opening (x ±0.3, y 0.08..0.62)
    for x in [-0.42, 0.42] {
        add(
            &mut m,
            rounded_box(Vec3::new(0.24, 0.86, 0.42), 0.03, 1, STONE),
            at(Vec3::new(x, 0.47, 0.0)),
        );
    }
    add(
        &mut m,
        rounded_box(Vec3::new(0.64, 0.3, 0.42), 0.03, 1, STONE),
        at(Vec3::new(0.0, 0.75, 0.0)),
    );
    add(
        &mut m,
        cuboid(Vec3::new(0.62, 0.6, 0.02), SOOT),
        at(Vec3::new(0.0, 0.4, -0.19)),
    );
    add(
        &mut m,
        cuboid(Vec3::new(0.62, 0.02, 0.4), SOOT),
        at(Vec3::new(0.0, 0.07, 0.0)),
    );
    // the hearth slab and the mantel shelf
    add(
        &mut m,
        cuboid(Vec3::new(1.1, 0.06, 0.5), SAND),
        at(Vec3::new(0.0, 0.03, 0.02)),
    );
    add(
        &mut m,
        rounded_box(Vec3::new(1.1, 0.06, 0.48), 0.02, 1, WOOD),
        at(Vec3::new(0.0, 0.93, 0.03)),
    );
    add(
        &mut m,
        cuboid(Vec3::new(1.04, 0.06, 0.42), STONE),
        at(Vec3::new(0.0, 1.02, 0.0)),
    );
    // two logs, crossed
    for (yaw, y) in [(0.25f32, 0.11), (-0.3, 0.15)] {
        add(
            &mut m,
            cylinder(0.04, 0.44, 8, WOOD_DARK),
            at(Vec3::new(0.0, y, 0.0))
                * Mat4::from_rotation_y(yaw)
                * Mat4::from_rotation_z(std::f32::consts::FRAC_PI_2),
        );
    }
    m
}

fn aquarium() -> Mesh {
    let mut m = Mesh::default();
    // the stand
    add(
        &mut m,
        rounded_box(Vec3::new(0.7, 0.5, 0.34), 0.03, 1, WOOD),
        at(Vec3::new(0.0, 0.25, 0.0)),
    );
    // the tank: rims and corner posts; the water is drawn by `rig::aquarium`
    add(
        &mut m,
        rounded_box(Vec3::new(0.72, 0.05, 0.36), 0.015, 1, IRON),
        at(Vec3::new(0.0, 0.525, 0.0)),
    );
    add(
        &mut m,
        cuboid(Vec3::new(0.72, 0.03, 0.36), IRON),
        at(Vec3::new(0.0, 0.985, 0.0)),
    );
    for (x, z) in [
        (-0.345, -0.165),
        (0.345, -0.165),
        (-0.345, 0.165),
        (0.345, 0.165),
    ] {
        add(
            &mut m,
            cuboid(Vec3::new(0.03, 0.42, 0.03), IRON),
            at(Vec3::new(x, 0.76, z)),
        );
    }
    add(
        &mut m,
        cuboid(Vec3::new(0.66, 0.4, 0.01), SKY),
        at(Vec3::new(0.0, 0.75, -0.16)),
    );
    add(
        &mut m,
        cuboid(Vec3::new(0.66, 0.05, 0.3), SAND),
        at(Vec3::new(0.0, 0.575, 0.0)),
    );
    for (x, s) in [(-0.2f32, 0.05f32), (0.16, 0.035)] {
        add(
            &mut m,
            ellipsoid(Vec3::new(s, s * 0.7, s), 7, 3, STONE),
            at(Vec3::new(x, 0.6, -0.05)),
        );
    }
    for (k, x) in [-0.1f32, -0.06, 0.24].into_iter().enumerate() {
        let weed = ellipsoid(Vec3::new(0.014, 0.12, 0.01), 6, 4, LEAF);
        add(
            &mut m,
            weed,
            at(Vec3::new(x, 0.72, -0.1 + 0.03 * k as f32))
                * Mat4::from_rotation_z(0.12 * (k as f32 - 1.0)),
        );
    }
    m
}

fn lamp() -> Mesh {
    let mut m = lathe(
        &[
            (0.0, 0.0, WOOD_DARK),
            (0.16, 0.0, WOOD_DARK),
            (0.17, 0.02, WOOD_DARK),
            (0.12, 0.045, WOOD_DARK),
            (0.02, 0.06, WOOD_DARK),
            (0.016, 1.1, WOOD_DARK),
            (0.0, 1.1, WOOD_DARK),
        ],
        12,
    );
    // the shade: a soft truncated cone, open-looking (a darker inside ring at the bottom)
    add(
        &mut m,
        lathe(
            &[
                (0.0, 1.06, BUTTER),
                (0.2, 1.06, BUTTER),
                (0.22, 1.08, BUTTER),
                (0.16, 1.4, BUTTER),
                (0.13, 1.43, BUTTER),
                (0.0, 1.45, BUTTER),
            ],
            16,
        ),
        Mat4::IDENTITY,
    );
    add(
        &mut m,
        ellipsoid(Vec3::new(0.06, 0.06, 0.06), 8, 4, SNOW),
        at(Vec3::new(0.0, 1.06, 0.0)),
    );
    m
}

fn safe() -> Mesh {
    let mut m =
        rounded_box(Vec3::new(0.54, 0.54, 0.44), 0.05, 1, IRON).moved(Vec3::new(0.0, 0.34, 0.0));
    // the inside, painted on the front behind the door: dark, a shelf, name cards
    add(
        &mut m,
        quad(Vec2::new(0.0, 0.34), Vec2::new(0.42, 0.42), 0.2205, SOOT),
        Mat4::IDENTITY,
    );
    add(
        &mut m,
        cuboid(Vec3::new(0.42, 0.015, 0.01), WOOD),
        at(Vec3::new(0.0, 0.3, 0.222)),
    );
    for (k, c) in [CREAM, BUTTER, MINT, SKY, CREAM].into_iter().enumerate() {
        add(
            &mut m,
            quad(
                Vec2::new(-0.14 + 0.07 * k as f32, 0.37),
                Vec2::new(0.05, 0.12),
                0.2215,
                c,
            ),
            Mat4::IDENTITY,
        );
    }
    for (x, z) in [(-0.21, -0.16), (0.21, -0.16), (-0.21, 0.16), (0.21, 0.16)] {
        add(
            &mut m,
            cuboid(Vec3::new(0.06, 0.07, 0.06), INK),
            at(Vec3::new(x, 0.035, z)),
        );
    }
    // the plaque on top
    add(
        &mut m,
        cuboid(Vec3::new(0.14, 0.012, 0.05), BRASS),
        at(Vec3::new(0.0, 0.614, 0.14)),
    );
    m
}

fn safe_door() -> Mesh {
    let mut m =
        rounded_box(Vec3::new(0.46, 0.46, 0.04), 0.015, 1, LILAC).moved(Vec3::new(0.23, 0.0, 0.02));
    // the dial (facing +z) and the lever
    add(
        &mut m,
        lathe(
            &[
                (0.0, 0.0, STONE),
                (0.075, 0.0, STONE),
                (0.075, 0.02, STONE),
                (0.06, 0.03, STONE),
                (0.06, 0.03, SNOW),
                (0.0, 0.032, SNOW),
            ],
            16,
        ),
        facing_z(Vec3::new(0.25, 0.04, 0.04)),
    );
    add(
        &mut m,
        cuboid(Vec3::new(0.008, 0.03, 0.004), ROSE),
        at(Vec3::new(0.25, 0.09, 0.073)),
    );
    add(
        &mut m,
        rounded_box(Vec3::new(0.12, 0.025, 0.025), 0.01, 1, BRASS),
        at(Vec3::new(0.25, -0.12, 0.06)),
    );
    add(
        &mut m,
        cylinder(0.01, 0.04, 6, BRASS),
        facing_z(Vec3::new(0.25, -0.12, 0.04)),
    );
    for y in [-0.15, 0.15] {
        add(
            &mut m,
            cylinder(0.015, 0.06, 8, INK),
            at(Vec3::new(0.0, y, 0.02)),
        );
    }
    m
}

fn fish() -> Mesh {
    let mut m = ellipsoid(Vec3::new(0.025, 0.016, 0.008), 8, 4, PEACH);
    m = paint(m, |p| {
        (p.x > 0.012 && p.y > 0.0)
            .then_some(INK)
            .filter(|_| p.x > 0.016 && p.y > 0.004)
    });
    let fin = Mesh {
        vertices: [
            [-0.02, 0.0, 0.0],
            [-0.035, 0.016, 0.0],
            [-0.035, -0.016, 0.0],
        ]
        .map(|pos| Vertex {
            pos,
            normal: pack_normal(Vec3::Z),
            color: BUTTER,
        })
        .to_vec(),
        indices: vec![0, 1, 2],
    };
    // both sides of the tail fin
    add(&mut m, fin.clone(), Mat4::IDENTITY);
    add(&mut m, fin, Mat4::from_scale(Vec3::new(1.0, 1.0, -1.0)));
    m
}

fn package() -> Mesh {
    let mut m =
        rounded_box(Vec3::new(0.36, 0.26, 0.3), 0.015, 1, CARD).moved(Vec3::new(0.0, 0.13, 0.0));
    add(
        &mut m,
        cuboid(Vec3::new(0.06, 0.004, 0.302), BUTTER),
        at(Vec3::new(0.0, 0.261, 0.0)),
    );
    add(
        &mut m,
        cuboid(Vec3::new(0.06, 0.26, 0.004), BUTTER),
        at(Vec3::new(0.0, 0.13, 0.151)),
    );
    // the address label
    add(
        &mut m,
        quad(Vec2::new(-0.1, 0.17), Vec2::new(0.1, 0.07), 0.1505, SNOW),
        Mat4::IDENTITY,
    );
    m
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::{build as kit_build, unpack_normal};

    /// Signed volume: positive for closed, outward-wound meshes.
    fn volume(m: &Mesh) -> f32 {
        m.indices
            .chunks(3)
            .map(|t| {
                let [a, b, c] = [t[0], t[1], t[2]].map(|i| Vec3::from(m.vertices[i as usize].pos));
                a.dot(b.cross(c)) / 6.0
            })
            .sum()
    }

    #[test]
    #[ignore]
    fn report() {
        for id in MeshId::ALL {
            let m = kit_build(id);
            let (lo, hi) = m.bounds();
            eprintln!(
                "{id:?}: {} tris, {} verts, {lo} .. {hi}, vol {:.5}",
                m.triangles(),
                m.vertices.len(),
                volume(&m)
            );
        }
    }

    #[test]
    fn every_wave2_mesh_states_its_box_and_is_turned_outwards() {
        for id in MeshId::ALL.into_iter().skip(15) {
            assert!(stated_bounds(id).is_some(), "{id:?}");
            let m = kit_build(id);
            let (lo, hi) = m.bounds();
            let boxv = (hi - lo).element_product();
            // closed parts: inside-out geometry would have a negative volume
            assert!(
                volume(&m) > 0.05 * boxv,
                "{id:?}: volume {} of box {boxv}",
                volume(&m)
            );
        }
    }

    #[test]
    fn coat_parts_are_near_white() {
        use MeshId::*;
        for id in [
            CatTorso,
            CatSkull,
            CatEarOuter,
            CatEarInner,
            CatPawFront,
            CatPawBack,
            CatTailSeg,
            CatChest,
            CatStripe,
            CatPatch,
            CatCollar,
            Ball,
            Jar,
        ] {
            assert!(
                kit_build(id).vertices.iter().all(|v| v.color == SNOW),
                "{id:?}"
            );
        }
        // the muzzle is near-white but for its pink nose
        assert!(
            kit_build(CatMuzzle)
                .vertices
                .iter()
                .all(|v| v.color == SNOW || v.color == ROSE)
        );
    }

    #[test]
    fn lathe_poles_rings_and_hard_edges() {
        // a pole, two rings, a pole: 2 fans + 1 band
        let p = [
            (0.0, 0.0, SNOW),
            (1.0, 0.0, SNOW),
            (1.0, 1.0, SNOW),
            (0.0, 1.0, SNOW),
        ];
        let m = lathe(&p, 8);
        // both corners turn 90°: each ring is split in two, bands between duplicates are skipped
        assert_eq!(m.vertices.len(), 1 + 8 * 4 + 1);
        assert_eq!(m.triangles(), 8 + 16 + 8);
        // the side's normals are horizontal (hard edges kept the caps out of them)
        for v in &m.vertices[9..25] {
            assert!(unpack_normal(v.normal).y.abs() < 0.02, "{:?}", v.normal);
        }
        // an octagonal prism of circumradius 1, height 1
        let area = 8.0 * 0.5 * (std::f32::consts::TAU / 8.0).sin();
        assert!((volume(&m) - area).abs() < 1e-4, "{}", volume(&m));
    }

    #[test]
    fn torus_and_ellipsoid_have_their_size() {
        let t = torus(0.1, 0.02, 12, 6, SNOW);
        let (lo, hi) = t.bounds();
        assert!((hi.x - 0.12).abs() < 3e-3 && (hi.y - 0.02).abs() < 2e-3 && lo.y > -0.0201);
        let e = ellipsoid(Vec3::new(0.3, 0.2, 0.1), 10, 6, SNOW);
        let (lo, hi) = e.bounds();
        assert!(
            (hi - Vec3::new(0.3, 0.2, 0.1)).abs().max_element() < 6e-3 && (lo + hi).length() < 1e-4
        );
    }

    #[test]
    fn warp_recomputes_normals_and_paint_recolours() {
        let m = warp(quad(Vec2::ZERO, Vec2::ONE, 0.0, SNOW), |p| {
            Vec3::new(p.x, p.y, -p.x)
        });
        let n = unpack_normal(m.vertices[0].normal);
        assert!(
            (n - Vec3::new(1.0, 0.0, 1.0).normalize()).length() < 0.02,
            "{n}"
        );
        let m = paint(m, |p| (p.x > 0.0).then_some(INK));
        assert_eq!(m.vertices.iter().filter(|v| v.color == INK).count(), 2);
    }
}
