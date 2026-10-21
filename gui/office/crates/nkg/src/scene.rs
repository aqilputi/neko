//! Building the office out of Bevy primitives: shared meshes and materials
//! (theme-coloured, recoloured on a theme change), the procedural cat rig,
//! desks/stools/monitors, rugs, and the floating icons. Only spawning here;
//! motion lives in `systems`.

use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use bevy::render::render_resource::Face;

use crate::catpose::{CatParams, SIT};
use crate::model::{Palette, Rgb, mix};
use crate::strip::*;

pub fn rgb(c: Rgb) -> Color {
    Color::srgb_u8(c[0], c[1], c[2])
}

/// Meshes every cat/desk shares.
#[derive(Resource)]
pub struct Kit {
    pub body: Handle<Mesh>,
    pub haunch: Handle<Mesh>,
    pub head: Handle<Mesh>,
    pub ear: Handle<Mesh>,
    pub eye: Handle<Mesh>,
    pub pupil: Handle<Mesh>,
    pub leg: Handle<Mesh>,
    pub paw: Handle<Mesh>,
    pub tail_seg: Handle<Mesh>,
    pub collar: Handle<Mesh>,
    pub walker_body: Handle<Mesh>,
    pub walker_leg: Handle<Mesh>,
    pub cube: Handle<Mesh>,
    pub screen: Handle<Mesh>,
    pub disc: Handle<Mesh>,
    pub cyl: Handle<Mesh>,
    pub floor: Handle<Mesh>,
}

/// Materials, recoloured in place by `Mats::recolor` when the theme arrives.
#[derive(Resource, Clone)]
pub struct Mats {
    pub fur: Handle<StandardMaterial>,
    pub fur_dim: Handle<StandardMaterial>,
    pub fur_puff: Handle<StandardMaterial>,
    pub outline: Handle<StandardMaterial>,
    pub eyes: Handle<StandardMaterial>,
    pub pupil: Handle<StandardMaterial>,
    pub desk: Handle<StandardMaterial>,
    pub desk_leg: Handle<StandardMaterial>,
    pub stool: Handle<StandardMaterial>,
    pub bezel: Handle<StandardMaterial>,
    pub bezel_focus: Handle<StandardMaterial>,
    pub kb: Handle<StandardMaterial>,
    pub kb_on: Handle<StandardMaterial>,
    pub floor: Handle<StandardMaterial>,
    pub wall: Handle<StandardMaterial>,
    pub mug: Handle<StandardMaterial>,
    pub blocked: Handle<StandardMaterial>,
    pub done: Handle<StandardMaterial>,
    pub mail: Handle<StandardMaterial>,
    pub ink: Handle<StandardMaterial>,
    pub paper: Handle<StandardMaterial>,
    pub zz: Handle<StandardMaterial>,
}

fn lit(c: Rgb, rough: f32) -> StandardMaterial {
    StandardMaterial {
        base_color: rgb(c),
        perceptual_roughness: rough,
        reflectance: 0.2,
        ..default()
    }
}

fn flat(c: Rgb) -> StandardMaterial {
    StandardMaterial {
        base_color: rgb(c),
        unlit: true,
        ..default()
    }
}

impl Mats {
    pub fn new(p: &Palette, m: &mut Assets<StandardMaterial>) -> Mats {
        let mut out = Mats {
            fur: m.add(StandardMaterial::default()),
            fur_dim: m.add(StandardMaterial::default()),
            fur_puff: m.add(StandardMaterial::default()),
            outline: m.add(StandardMaterial::default()),
            eyes: m.add(StandardMaterial::default()),
            pupil: m.add(StandardMaterial::default()),
            desk: m.add(StandardMaterial::default()),
            desk_leg: m.add(StandardMaterial::default()),
            stool: m.add(StandardMaterial::default()),
            bezel: m.add(StandardMaterial::default()),
            bezel_focus: m.add(StandardMaterial::default()),
            kb: m.add(StandardMaterial::default()),
            kb_on: m.add(StandardMaterial::default()),
            floor: m.add(StandardMaterial::default()),
            wall: m.add(StandardMaterial::default()),
            mug: m.add(StandardMaterial::default()),
            blocked: m.add(StandardMaterial::default()),
            done: m.add(StandardMaterial::default()),
            mail: m.add(StandardMaterial::default()),
            ink: m.add(StandardMaterial::default()),
            paper: m.add(StandardMaterial::default()),
            zz: m.add(StandardMaterial::default()),
        };
        out.recolor(p, m);
        out
    }

    pub fn recolor(&mut self, p: &Palette, m: &mut Assets<StandardMaterial>) {
        let set = |m: &mut Assets<StandardMaterial>,
                   h: &Handle<StandardMaterial>,
                   v: StandardMaterial| {
            if let Some(mut x) = m.get_mut(h) {
                *x = v;
            }
        };
        // the cat: near-black fur, lifted a touch so the lit side reads
        let fur = mix(p.cat_body, p.overlay, 0.18);
        set(m, &self.fur, lit(fur, 0.9));
        set(m, &self.fur_dim, lit(mix(fur, p.muted, 0.35), 0.9));
        set(
            m,
            &self.fur_puff,
            StandardMaterial {
                emissive: rgb(p.error).to_linear() * 0.6,
                ..lit(mix(fur, p.error, 0.5), 0.9)
            },
        );
        set(
            m,
            &self.outline,
            StandardMaterial {
                cull_mode: Some(Face::Front),
                ..flat(mix(p.bg, p.border, 0.45))
            },
        );
        set(m, &self.eyes, flat(p.cat_eyes));
        set(m, &self.pupil, flat(p.cat_body));
        set(m, &self.desk, lit(mix(p.surface, p.overlay, 0.6), 0.85));
        set(m, &self.desk_leg, lit(p.surface, 0.85));
        set(m, &self.stool, lit(mix(p.overlay, p.border, 0.3), 0.8));
        set(m, &self.bezel, lit(p.crust, 0.6));
        set(
            m,
            &self.bezel_focus,
            StandardMaterial {
                emissive: rgb(p.accent).to_linear() * 0.9,
                ..lit(p.accent, 0.6)
            },
        );
        set(m, &self.kb, lit(p.overlay, 0.7));
        set(
            m,
            &self.kb_on,
            StandardMaterial {
                emissive: rgb(p.busy).to_linear() * 0.5,
                ..lit(mix(p.overlay, p.busy, 0.4), 0.7)
            },
        );
        set(m, &self.floor, lit(mix(p.bg, p.crust, 0.35), 1.0));
        set(m, &self.wall, lit(mix(p.bg, p.surface, 0.15), 1.0));
        set(m, &self.mug, lit(p.accent2, 0.6));
        set(m, &self.blocked, flat(p.blocked));
        set(m, &self.done, flat(p.done));
        set(m, &self.mail, flat(p.accent2));
        set(m, &self.ink, flat(p.crust));
        set(m, &self.paper, flat(p.text));
        set(m, &self.zz, flat(p.subtle));
    }
}

impl Kit {
    pub fn new(meshes: &mut Assets<Mesh>) -> Kit {
        Kit {
            body: meshes.add(Capsule3d::new(0.25, 0.3)),
            haunch: meshes.add(Sphere::new(0.19).mesh().uv(14, 10)),
            head: meshes.add(Sphere::new(0.27).mesh().uv(18, 12)),
            ear: meshes.add(Cone::new(0.1, 0.22)),
            eye: meshes.add(Sphere::new(0.058).mesh().uv(10, 8)),
            pupil: meshes.add(Capsule3d::new(0.016, 0.05)),
            leg: meshes.add(Capsule3d::new(0.055, 0.26)),
            paw: meshes.add(Sphere::new(0.07).mesh().uv(10, 8)),
            tail_seg: meshes.add(Capsule3d::new(0.05, 0.16)),
            collar: meshes.add(Torus::new(0.17, 0.235)),
            walker_body: meshes.add(Capsule3d::new(0.2, 0.42)),
            walker_leg: meshes.add(Capsule3d::new(0.05, 0.2)),
            cube: meshes.add(Cuboid::new(1.0, 1.0, 1.0)),
            screen: meshes.add(Rectangle::new(SCREEN_W, SCREEN_H)),
            disc: meshes.add(Circle::new(0.5)),
            cyl: meshes.add(Cylinder::new(0.5, 1.0)),
            floor: meshes.add(Plane3d::default().mesh().size(400.0, 60.0)),
        }
    }
}

/// A mesh part with a toon outline (inverted hull) child.
fn part(
    c: &mut Commands,
    parent: Entity,
    mesh: &Handle<Mesh>,
    mat: &Handle<StandardMaterial>,
    outline: Option<(&Handle<StandardMaterial>, f32)>,
    tf: Transform,
) -> Entity {
    let e = c
        .spawn((
            Mesh3d(mesh.clone()),
            MeshMaterial3d(mat.clone()),
            tf,
            ChildOf(parent),
        ))
        .id();
    if let Some((om, s)) = outline {
        c.spawn((
            Mesh3d(mesh.clone()),
            MeshMaterial3d(om.clone()),
            Transform::from_scale(Vec3::splat(s)),
            NotShadowCaster,
            ChildOf(e),
        ));
    }
    e
}

fn pivot(c: &mut Commands, parent: Entity, tf: Transform) -> Entity {
    c.spawn((tf, Visibility::default(), ChildOf(parent))).id()
}

/// The joints of one sitting cat (for `apply`).
#[derive(Component, Clone)]
pub struct Rig {
    pub body: Entity,
    pub head: Entity,
    pub ears: [Entity; 2],
    pub legs: [Entity; 2],
    pub tail: [Entity; 3],
    pub eyes: [Entity; 2],
    pub collar: Entity,
    pub fur: Vec<Entity>,
}

/// The sitting cat, root at the seat, facing -z. Returns (root, rig).
pub fn spawn_cat(
    c: &mut Commands,
    kit: &Kit,
    m: &Mats,
    parent: Entity,
    tf: Transform,
) -> (Entity, Rig) {
    let root = c.spawn((tf, Visibility::default(), ChildOf(parent))).id();
    let ol = Some((&m.outline, 1.1));
    let body = part(
        c,
        root,
        &kit.body,
        &m.fur,
        Some((&m.outline, 1.09)),
        Transform::from_xyz(0.0, SIT.body_y, 0.0),
    );
    let mut fur = vec![body];
    for s in [-1.0f32, 1.0] {
        let h = part(
            c,
            root,
            &kit.haunch,
            &m.fur,
            Some((&m.outline, 1.1)),
            Transform::from_xyz(s * 0.15, 0.17, 0.08).with_scale(Vec3::new(0.9, 1.0, 1.15)),
        );
        fur.push(h);
    }
    let head = pivot(c, root, Transform::from_xyz(0.0, SIT.head_y, SIT.head_z));
    let skull = part(
        c,
        head,
        &kit.head,
        &m.fur,
        Some((&m.outline, 1.09)),
        Transform::from_scale(Vec3::new(1.06, 0.9, 0.95)),
    );
    fur.push(skull);
    let mut ears = [Entity::PLACEHOLDER; 2];
    for (i, s) in [-1.0f32, 1.0].into_iter().enumerate() {
        let p = pivot(
            c,
            head,
            Transform::from_xyz(s * 0.15, 0.17, 0.02)
                .with_rotation(Quat::from_rotation_z(-s * 0.32)),
        );
        let e = part(
            c,
            p,
            &kit.ear,
            &m.fur,
            ol,
            Transform::from_xyz(0.0, 0.09, 0.0),
        );
        fur.push(e);
        ears[i] = p;
    }
    let mut eyes = [Entity::PLACEHOLDER; 2];
    for (i, s) in [-1.0f32, 1.0].into_iter().enumerate() {
        let e = part(
            c,
            head,
            &kit.eye,
            &m.eyes,
            None,
            Transform::from_xyz(s * 0.1, 0.03, -0.235).with_scale(Vec3::new(1.0, 1.0, 0.55)),
        );
        part(
            c,
            e,
            &kit.pupil,
            &m.pupil,
            None,
            Transform::from_xyz(0.0, 0.0, -0.045),
        );
        eyes[i] = e;
    }
    let collar = part(
        c,
        root,
        &kit.collar,
        &m.mail,
        None,
        Transform::from_xyz(0.0, 0.64, -0.03).with_rotation(Quat::from_rotation_x(-0.2)),
    );
    let mut legs = [Entity::PLACEHOLDER; 2];
    for (i, s) in [-1.0f32, 1.0].into_iter().enumerate() {
        let p = pivot(c, root, Transform::from_xyz(s * 0.11, 0.52, -0.14));
        let l = part(
            c,
            p,
            &kit.leg,
            &m.fur,
            Some((&m.outline, 1.2)),
            Transform::from_xyz(0.0, -0.18, 0.0),
        );
        let paw = part(
            c,
            p,
            &kit.paw,
            &m.fur,
            Some((&m.outline, 1.2)),
            Transform::from_xyz(0.0, -0.36, -0.01),
        );
        fur.push(l);
        fur.push(paw);
        legs[i] = p;
    }
    let mut tail = [Entity::PLACEHOLDER; 3];
    let mut at = root;
    for (i, slot) in tail.iter_mut().enumerate() {
        let tf = if i == 0 {
            Transform::from_xyz(0.0, 0.12, 0.25)
        } else {
            Transform::from_xyz(0.0, 0.2, 0.0)
        };
        let p = pivot(c, at, tf);
        let seg = part(
            c,
            p,
            &kit.tail_seg,
            &m.fur,
            Some((&m.outline, 1.22)),
            Transform::from_xyz(0.0, 0.1, 0.0),
        );
        fur.push(seg);
        *slot = p;
        at = p;
    }
    let rig = Rig {
        body,
        head,
        ears,
        legs,
        tail,
        eyes,
        collar,
        fur,
    };
    c.entity(root).insert(rig.clone());
    (root, rig)
}

/// Joint transforms for a set of params (the Bevy side writes them).
pub fn rig_transforms(p: &CatParams) -> RigPose {
    let head_rot = Quat::from_rotation_y(p.head_rot_y)
        * Quat::from_rotation_x(p.head_rot_x)
        * Quat::from_rotation_z(p.head_rot_z);
    RigPose {
        body: Transform::from_xyz(0.0, p.body_y, 0.0)
            .with_rotation(Quat::from_rotation_x(p.body_rot_x))
            .with_scale(Vec3::new(
                1.0 + (p.body_scale - 1.0) * 0.5,
                p.body_scale,
                1.0,
            )),
        head: Transform::from_xyz(0.0, p.head_y, p.head_z).with_rotation(head_rot),
        ears: [-1.0f32, 1.0].map(|s| {
            Transform::from_xyz(s * 0.15, 0.17, 0.02).with_rotation(
                Quat::from_rotation_z(-s * 0.32) * Quat::from_rotation_x(1.0 * p.ears_back),
            )
        }),
        legs: [p.leg_l, p.leg_r].map(Quat::from_rotation_x),
        tail0: Quat::from_rotation_z(p.tail_sway) * Quat::from_rotation_x(p.tail_pitch),
        tail_curl: Quat::from_rotation_x(p.tail_curl * 0.7),
        tail_scale: Vec3::new(p.tail_scale, 1.0, p.tail_scale),
        eye_scale: Vec3::new(1.0, p.eyes_open.max(0.1), 0.55),
    }
}

pub struct RigPose {
    pub body: Transform,
    pub head: Transform,
    pub ears: [Transform; 2],
    pub legs: [Quat; 2],
    pub tail0: Quat,
    pub tail_curl: Quat,
    pub tail_scale: Vec3,
    pub eye_scale: Vec3,
}

/// Which floating icon.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IconKind {
    Bang,
    BangDone,
    Mail,
    Zz,
}

#[derive(Component)]
pub struct Icon {
    #[allow(dead_code)] // which icon it is, for debugging and picking later
    pub kind: IconKind,
    pub base_y: f32,
}

/// A bold floating icon: a coloured disc badge with a glyph built from
/// boxes ("!", envelope) or a pair of Zs. Faces +z.
pub fn spawn_icon(
    c: &mut Commands,
    kit: &Kit,
    m: &Mats,
    parent: Entity,
    kind: IconKind,
    at: Vec3,
    size: f32,
) -> Entity {
    let root = c
        .spawn((
            Transform::from_translation(at).with_scale(Vec3::splat(size)),
            Visibility::Hidden,
            Icon { kind, base_y: at.y },
            ChildOf(parent),
        ))
        .id();
    let disc_mat = match kind {
        IconKind::Bang => &m.blocked,
        IconKind::BangDone => &m.done,
        IconKind::Mail => &m.mail,
        IconKind::Zz => &m.zz,
    };
    let bx = |c: &mut Commands, mat: &Handle<StandardMaterial>, pos: Vec3, sz: Vec3, rot: f32| {
        c.spawn((
            Mesh3d(kit.cube.clone()),
            MeshMaterial3d(mat.clone()),
            Transform::from_translation(pos)
                .with_rotation(Quat::from_rotation_z(rot))
                .with_scale(sz),
            NotShadowCaster,
            ChildOf(root),
        ));
    };
    match kind {
        IconKind::Zz => {
            for (off, s) in [
                (Vec3::new(0.0, 0.0, 0.0), 1.0f32),
                (Vec3::new(0.3, 0.32, 0.0), 0.7),
            ] {
                let w = 0.34 * s;
                let t = 0.07 * s;
                bx(
                    c,
                    disc_mat,
                    off + Vec3::new(0.0, w / 2.0, 0.0),
                    Vec3::new(w, t, t),
                    0.0,
                );
                bx(
                    c,
                    disc_mat,
                    off + Vec3::new(0.0, -w / 2.0, 0.0),
                    Vec3::new(w, t, t),
                    0.0,
                );
                bx(
                    c,
                    disc_mat,
                    off,
                    Vec3::new(w * 1.3, t, t),
                    std::f32::consts::FRAC_PI_4,
                );
            }
        }
        _ => {
            // outline ring, then the disc
            c.spawn((
                Mesh3d(kit.disc.clone()),
                MeshMaterial3d(m.ink.clone()),
                Transform::from_xyz(0.0, 0.0, -0.01).with_scale(Vec3::splat(1.14)),
                NotShadowCaster,
                ChildOf(root),
            ));
            c.spawn((
                Mesh3d(kit.disc.clone()),
                MeshMaterial3d(disc_mat.clone()),
                NotShadowCaster,
                ChildOf(root),
            ));
            if kind == IconKind::Mail {
                bx(
                    c,
                    &m.paper,
                    Vec3::new(0.0, 0.0, 0.02),
                    Vec3::new(0.56, 0.38, 0.02),
                    0.0,
                );
                bx(
                    c,
                    &m.ink,
                    Vec3::new(-0.13, 0.07, 0.04),
                    Vec3::new(0.32, 0.045, 0.02),
                    -0.55,
                );
                bx(
                    c,
                    &m.ink,
                    Vec3::new(0.13, 0.07, 0.04),
                    Vec3::new(0.32, 0.045, 0.02),
                    0.55,
                );
            } else {
                bx(
                    c,
                    &m.ink,
                    Vec3::new(0.0, 0.08, 0.03),
                    Vec3::new(0.12, 0.42, 0.02),
                    0.0,
                );
                bx(
                    c,
                    &m.ink,
                    Vec3::new(0.0, -0.27, 0.03),
                    Vec3::new(0.12, 0.12, 0.02),
                    0.0,
                );
            }
        }
    }
    root
}

/// A box with given size at a position (desk parts).
fn block(
    c: &mut Commands,
    kit: &Kit,
    mat: &Handle<StandardMaterial>,
    parent: Entity,
    pos: Vec3,
    size: Vec3,
) -> Entity {
    c.spawn((
        Mesh3d(kit.cube.clone()),
        MeshMaterial3d(mat.clone()),
        Transform::from_translation(pos).with_scale(size),
        ChildOf(parent),
    ))
    .id()
}

/// Entities of a column the systems touch.
#[derive(Component)]
pub struct ColumnParts {
    pub sid: String,
    pub cat: Entity,
    #[allow(dead_code)]
    pub screen: Entity,
    pub bezel: Entity,
    pub keyboard: Entity,
    pub bang: Entity,
    pub zz: Entity,
    pub kittens: Vec<Entity>,
}

/// One column: desk, monitor (screen material given), stool, cat, kittens.
#[allow(clippy::too_many_arguments)]
pub fn spawn_column(
    c: &mut Commands,
    kit: &Kit,
    m: &Mats,
    x: f32,
    sid: &str,
    screen_mat: Handle<StandardMaterial>,
    kittens: usize,
    seed: u32,
) -> (Entity, ColumnParts, Rig) {
    let root = c
        .spawn((Transform::from_xyz(x, 0.0, 0.0), Visibility::default()))
        .id();
    // desk
    block(
        c,
        kit,
        &m.desk,
        root,
        Vec3::new(0.0, DESK_TOP - 0.04, DESK_Z),
        Vec3::new(DESK_W, 0.08, DESK_D),
    );
    for (sx, sz) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
        block(
            c,
            kit,
            &m.desk_leg,
            root,
            Vec3::new(
                sx * (DESK_W / 2.0 - 0.12),
                (DESK_TOP - 0.08) / 2.0,
                DESK_Z + sz * (DESK_D / 2.0 - 0.12),
            ),
            Vec3::new(0.09, DESK_TOP - 0.08, 0.09),
        );
    }
    // monitor: stand, bezel, screen
    block(
        c,
        kit,
        &m.bezel,
        root,
        Vec3::new(0.0, DESK_TOP + 0.015, SCREEN_Z + 0.05),
        Vec3::new(0.7, 0.03, 0.4),
    );
    block(
        c,
        kit,
        &m.bezel,
        root,
        Vec3::new(0.0, (DESK_TOP + SCREEN_Y) / 2.0, SCREEN_Z - 0.08),
        Vec3::new(0.14, SCREEN_Y - DESK_TOP, 0.06),
    );
    let bezel = block(
        c,
        kit,
        &m.bezel,
        root,
        Vec3::new(0.0, SCREEN_Y, SCREEN_Z - 0.045),
        Vec3::new(SCREEN_W + 0.14, SCREEN_H + 0.14, 0.08),
    );
    let screen = c
        .spawn((
            Mesh3d(kit.screen.clone()),
            MeshMaterial3d(screen_mat),
            Transform::from_xyz(0.0, SCREEN_Y, SCREEN_Z),
            NotShadowCaster,
            ChildOf(root),
        ))
        .id();
    // keyboard
    let keyboard = block(
        c,
        kit,
        &m.kb,
        root,
        Vec3::new(0.0, DESK_TOP + 0.02, DESK_Z + 0.32),
        Vec3::new(0.95, 0.04, 0.3),
    );
    // a mug on some desks
    if seed % 3 != 1 {
        let side = if seed.is_multiple_of(2) { 1.0 } else { -1.0 };
        c.spawn((
            Mesh3d(kit.cyl.clone()),
            MeshMaterial3d(m.mug.clone()),
            Transform::from_xyz(side * 0.95, DESK_TOP + 0.09, DESK_Z + 0.3)
                .with_scale(Vec3::new(0.16, 0.18, 0.16)),
            ChildOf(root),
        ));
    }
    // stool
    c.spawn((
        Mesh3d(kit.cyl.clone()),
        MeshMaterial3d(m.stool.clone()),
        Transform::from_xyz(0.0, SEAT_Y - 0.04, CHAIR_Z).with_scale(Vec3::new(0.78, 0.08, 0.78)),
        ChildOf(root),
    ));
    c.spawn((
        Mesh3d(kit.cyl.clone()),
        MeshMaterial3d(m.desk_leg.clone()),
        Transform::from_xyz(0.0, (SEAT_Y - 0.08) / 2.0, CHAIR_Z).with_scale(Vec3::new(
            0.1,
            SEAT_Y - 0.08,
            0.1,
        )),
        ChildOf(root),
    ));
    c.spawn((
        Mesh3d(kit.cyl.clone()),
        MeshMaterial3d(m.desk_leg.clone()),
        Transform::from_xyz(0.0, 0.02, CHAIR_Z).with_scale(Vec3::new(0.55, 0.04, 0.55)),
        ChildOf(root),
    ));
    // the cat
    let (cat, rig) = spawn_cat(c, kit, m, root, Transform::from_xyz(0.0, SEAT_Y, CAT_Z));
    let bang = spawn_icon(
        c,
        kit,
        m,
        cat,
        IconKind::Bang,
        Vec3::new(0.0, 1.45, 0.0),
        0.62,
    );
    let zz = spawn_icon(
        c,
        kit,
        m,
        cat,
        IconKind::Zz,
        Vec3::new(0.35, 1.0, -0.35),
        0.75,
    );
    // kittens (running subagents) on the desk, facing us
    let mut ks = Vec::new();
    for i in 0..kittens.min(3) {
        let kx = 1.05 - i as f32 * 0.38;
        let (k, _) = spawn_cat(
            c,
            kit,
            m,
            root,
            Transform::from_xyz(if i % 2 == 0 { kx } else { -kx }, DESK_TOP, DESK_Z - 0.05)
                .with_rotation(Quat::from_rotation_y(std::f32::consts::PI))
                .with_scale(Vec3::splat(0.4)),
        );
        ks.push(k);
    }
    let parts = ColumnParts {
        sid: sid.to_string(),
        cat,
        screen,
        bezel,
        keyboard,
        bang,
        zz,
        kittens: ks,
    };
    (root, parts, rig)
}

/// A walking inbox cat (on all fours, facing +x) with its icon.
#[derive(Component)]
pub struct WalkerRig {
    pub key: String,
    #[allow(dead_code)]
    pub body: Entity,
    #[allow(dead_code)]
    pub head: Entity,
    pub legs: [Entity; 4],
    pub tail: Entity,
    pub icon: Entity,
}

pub fn spawn_walker(
    c: &mut Commands,
    kit: &Kit,
    m: &Mats,
    key: &str,
    icon: IconKind,
    at: Vec3,
) -> Entity {
    let root = c
        .spawn((
            Transform::from_translation(at).with_scale(Vec3::splat(WALKER_SCALE)),
            Visibility::default(),
        ))
        .id();
    let body = part(
        c,
        root,
        &kit.walker_body,
        &m.fur,
        Some((&m.outline, 1.1)),
        Transform::from_xyz(0.0, 0.42, 0.0)
            .with_rotation(Quat::from_rotation_z(std::f32::consts::FRAC_PI_2)),
    );
    let head = pivot(c, root, Transform::from_xyz(0.42, 0.68, 0.0));
    part(
        c,
        head,
        &kit.head,
        &m.fur,
        Some((&m.outline, 1.09)),
        Transform::from_scale(Vec3::new(0.95, 0.9, 1.0)),
    );
    for s in [-1.0f32, 1.0] {
        let p = pivot(
            c,
            head,
            Transform::from_xyz(0.0, 0.17, s * 0.15).with_rotation(Quat::from_rotation_x(s * 0.32)),
        );
        part(
            c,
            p,
            &kit.ear,
            &m.fur,
            Some((&m.outline, 1.1)),
            Transform::from_xyz(0.0, 0.09, 0.0),
        );
    }
    // eyes on the +x face, turned a bit towards the camera (+z)
    for s in [-1.0f32, 1.0] {
        let e = part(
            c,
            head,
            &kit.eye,
            &m.eyes,
            None,
            Transform::from_xyz(0.2, 0.03, s * 0.1 + 0.06).with_scale(Vec3::new(0.55, 1.0, 1.0)),
        );
        part(
            c,
            e,
            &kit.pupil,
            &m.pupil,
            None,
            Transform::from_xyz(0.045, 0.0, 0.0),
        );
    }
    let mut legs = [Entity::PLACEHOLDER; 4];
    for (i, (lx, lz)) in [(0.25, -0.12), (0.25, 0.12), (-0.25, -0.12), (-0.25, 0.12)]
        .into_iter()
        .enumerate()
    {
        let p = pivot(c, root, Transform::from_xyz(lx, 0.36, lz));
        part(
            c,
            p,
            &kit.walker_leg,
            &m.fur,
            Some((&m.outline, 1.2)),
            Transform::from_xyz(0.0, -0.17, 0.0),
        );
        legs[i] = p;
    }
    let tail = pivot(
        c,
        root,
        Transform::from_xyz(-0.4, 0.5, 0.0).with_rotation(Quat::from_rotation_z(0.5)),
    );
    let t1 = part(
        c,
        tail,
        &kit.tail_seg,
        &m.fur,
        Some((&m.outline, 1.22)),
        Transform::from_xyz(0.0, 0.1, 0.0),
    );
    let t2 = pivot(
        c,
        t1,
        Transform::from_xyz(0.0, 0.1, 0.0).with_rotation(Quat::from_rotation_z(0.6)),
    );
    part(
        c,
        t2,
        &kit.tail_seg,
        &m.fur,
        Some((&m.outline, 1.22)),
        Transform::from_xyz(0.0, 0.1, 0.0),
    );
    let icon = spawn_icon(c, kit, m, root, icon, Vec3::new(0.1, 1.3, 0.0), 0.5);
    c.entity(icon).insert(Visibility::Inherited);
    c.entity(root).insert(WalkerRig {
        key: key.to_string(),
        body,
        head,
        legs,
        tail,
        icon,
    });
    root
}

/// The floor, a back wall and the light.
pub fn spawn_room(c: &mut Commands, kit: &Kit, m: &Mats) {
    c.spawn((
        Mesh3d(kit.floor.clone()),
        MeshMaterial3d(m.floor.clone()),
        Transform::from_xyz(100.0, 0.0, 0.0),
    ));
    c.spawn((
        Mesh3d(kit.cube.clone()),
        MeshMaterial3d(m.wall.clone()),
        Transform::from_xyz(100.0, 4.0, -2.6).with_scale(Vec3::new(400.0, 8.0, 0.2)),
    ));
    // a skirting line on the wall
    c.spawn((
        Mesh3d(kit.cube.clone()),
        MeshMaterial3d(m.desk_leg.clone()),
        Transform::from_xyz(100.0, 0.12, -2.48).with_scale(Vec3::new(400.0, 0.24, 0.06)),
    ));
}

/// Context rug under a run of columns.
#[derive(Component)]
pub struct Rug;

pub fn rug_material(p: &Palette, ctx_color: i64) -> StandardMaterial {
    lit(mix(p.context(ctx_color), p.bg, 0.62), 1.0)
}

pub fn spawn_rug(
    c: &mut Commands,
    kit: &Kit,
    mat: Handle<StandardMaterial>,
    x0: f32,
    x1: f32,
) -> Entity {
    let z0 = DESK_Z - DESK_D / 2.0 - 0.35;
    let z1 = CHAIR_Z + 0.75;
    c.spawn((
        Mesh3d(kit.cube.clone()),
        MeshMaterial3d(mat),
        Transform::from_xyz((x0 + x1) / 2.0, 0.01, (z0 + z1) / 2.0).with_scale(Vec3::new(
            x1 - x0,
            0.02,
            z1 - z0,
        )),
        Rug,
    ))
    .id()
}
