//! The room shell's meshes (S17). Pure geometry, like the rest of the kit.
//!
//! Floors and the ceiling are 1×1 m in x/z (the shell scales them to the
//! room; the floor's top is y = 0, the ceiling's underside y = 0 with its
//! beams hanging below). Wallpaper panels, the cornice (`Lintel`) and the
//! skirting are 1 m along x with their back on z = 0, facing +z (into the
//! room); a panel runs y 0..1 (scale y by the wall height). The window
//! frame is in metres (a 1.2 × 1.0 m opening centred on the origin, back on
//! z = 0, with its sill, rod and curtains); the pane is a unit square.
//!
//! Patterns are vertex colours (no textures): the instance tint recolours
//! them, so the parts meant to take the room's colour are near-white and
//! the pattern is a slightly darker near-white.

use super::*;

/// The floorboards' tones (multiplied by the floor tint) and the seam under them.
const BOARDS: [Rgba; 3] = [
    rgba(232, 194, 148),
    rgba(220, 178, 134),
    rgba(238, 204, 160),
];
const SEAM: Rgba = rgba(176, 130, 96);
/// The tiles: cream and sage, grout in sand.
const TILE_A: Rgba = rgba(246, 240, 226);
const TILE_B: Rgba = rgba(214, 228, 212);
const GROUT: Rgba = rgba(204, 192, 170);
/// The pattern's darker near-white (stripes, dots, the rug's border).
const MOTIF: Rgba = rgba(226, 216, 206);
/// The dots are softer than the stripes (many small marks read darker).
const DOT: Rgba = rgba(234, 225, 216);
const RUG_BAND: Rgba = rgba(206, 196, 196);
const CURTAIN: Rgba = rgba(244, 196, 190);
const CURTAIN_HEM: Rgba = rgba(232, 176, 172);
/// Distant hills seen through the pane (tinted by the light of the band).
const HILLS: Rgba = rgba(196, 226, 186);
const HILLS_FAR: Rgba = rgba(214, 232, 214);

/// A quad on y = `y` facing +y, from (x0, z0) to (x1, z1).
fn floor_quad(m: &mut Mesh, x0: f32, z0: f32, x1: f32, z1: f32, y: f32, color: Rgba) {
    let base = m.vertices.len() as u16;
    let normal = pack_normal(Vec3::Y);
    for [x, z] in [[x0, z0], [x0, z1], [x1, z1], [x1, z0]] {
        m.vertices.push(Vertex {
            pos: [x, y, z],
            normal,
            color,
        });
    }
    m.indices
        .extend([base, base + 1, base + 2, base, base + 2, base + 3]);
}

/// A quad on z = `z` facing +z, from (x0, y0) to (x1, y1), colours per corner
/// (bottom-left, bottom-right, top-right, top-left).
fn wall_quad(m: &mut Mesh, x0: f32, y0: f32, x1: f32, y1: f32, z: f32, c: [Rgba; 4]) {
    let base = m.vertices.len() as u16;
    let normal = pack_normal(Vec3::Z);
    for ([x, y], color) in [[x0, y0], [x1, y0], [x1, y1], [x0, y1]].into_iter().zip(c) {
        m.vertices.push(Vertex {
            pos: [x, y, z],
            normal,
            color,
        });
    }
    m.indices
        .extend([base, base + 1, base + 2, base, base + 2, base + 3]);
}

/// A flat disc of `sides` on z = `z` facing +z.
fn dot(m: &mut Mesh, c: Vec2, r: Vec2, z: f32, sides: u32, color: Rgba) {
    let base = m.vertices.len() as u16;
    let normal = pack_normal(Vec3::Z);
    m.vertices.push(Vertex {
        pos: [c.x, c.y, z],
        normal,
        color,
    });
    for k in 0..sides {
        let a = std::f32::consts::TAU * k as f32 / sides as f32;
        m.vertices.push(Vertex {
            pos: [c.x + r.x * a.cos(), c.y + r.y * a.sin(), z],
            normal,
            color,
        });
    }
    let n = sides as u16;
    for k in 0..n {
        m.indices
            .extend([base, base + 1 + k, base + 1 + (k + 1) % n]);
    }
}

/// Floorboards: 14 rows across z, three boards a row with staggered joints,
/// thin seams showing the darker slab beneath.
pub fn planks() -> Mesh {
    let mut m = cuboid(Vec3::new(1.0, 0.029, 1.0), SEAM).moved(Vec3::new(0.0, -0.0155, 0.0));
    const ROWS: usize = 14;
    let gap = 0.0022;
    let w = 1.0 / ROWS as f32;
    for r in 0..ROWS {
        let z0 = -0.5 + r as f32 * w;
        // two joints a row, never lined up with the neighbours'
        let a = (0.17 + 0.37 * r as f32).fract();
        let b = (a + 0.46).fract();
        let (a, b) = (a.min(b) - 0.5, a.max(b) - 0.5);
        let xs = [-0.5, a, b, 0.5];
        for k in 0..3 {
            let (x0, x1) = (xs[k], xs[k + 1]);
            if x1 - x0 < 0.02 {
                continue;
            }
            let lo = if k == 0 { x0 } else { x0 + gap };
            let hi = if k == 2 { x1 } else { x1 - gap };
            let tone = BOARDS[(r * 2 + k) % 3];
            floor_quad(&mut m, lo, z0 + gap, hi, z0 + w - gap, 0.0, tone);
        }
    }
    m
}

/// Square-ish tiles in a cream and sage checker, grout between them.
pub fn tiles() -> Mesh {
    let mut m = cuboid(Vec3::new(1.0, 0.029, 1.0), GROUT).moved(Vec3::new(0.0, -0.0155, 0.0));
    const N: usize = 10;
    let w = 1.0 / N as f32;
    let gap = 0.004;
    for i in 0..N {
        for j in 0..N {
            let x0 = -0.5 + i as f32 * w;
            let z0 = -0.5 + j as f32 * w;
            let c = if (i + j) % 2 == 0 { TILE_A } else { TILE_B };
            floor_quad(
                &mut m,
                x0 + gap,
                z0 + gap,
                x0 + w - gap,
                z0 + w - gap,
                0.0,
                c,
            );
        }
    }
    m
}

/// A soft rug: a rounded mat with a darker band inside its border and a
/// fringe at both short ends (±x).
pub fn rug() -> Mesh {
    let mut m =
        rounded_box(Vec3::new(0.94, 0.012, 1.0), 0.006, 1, SNOW).moved(Vec3::new(0.0, 0.006, 0.0));
    let top = 0.0122;
    // the band: four strips framing the middle
    let (o, i) = (0.4, 0.34);
    for (x0, z0, x1, z1) in [
        (-o, -o - 0.05, o, -i - 0.05),
        (-o, i + 0.05, o, o + 0.05),
        (-o, -i - 0.05, -i, i + 0.05),
        (i, -i - 0.05, o, i + 0.05),
    ] {
        floor_quad(&mut m, x0, z0, x1, z1, top, RUG_BAND);
    }
    // a diamond in the middle
    let base = m.vertices.len() as u16;
    let normal = pack_normal(Vec3::Y);
    for [x, z] in [[0.0, -0.18], [-0.14, 0.0], [0.0, 0.18], [0.14, 0.0]] {
        m.vertices.push(Vertex {
            pos: [x, top, z],
            normal,
            color: MOTIF,
        });
    }
    m.indices
        .extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    // the fringe
    for s in [-1.0f32, 1.0] {
        for k in 0..9 {
            let z = -0.44 + 0.11 * k as f32;
            add(
                &mut m,
                cuboid(Vec3::new(0.03, 0.004, 0.035), SNOW),
                at(Vec3::new(s * 0.485, 0.002, z)),
            );
        }
    }
    m
}

/// The wall behind the wallpaper: a 1 m panel, 2 cm thick.
fn panel_base() -> Mesh {
    cuboid(Vec3::new(1.0, 1.0, 0.02), SNOW).moved(Vec3::new(0.0, 0.5, 0.01))
}

/// Plain wallpaper with a soft darker band near the top (where the light fades).
pub fn wall_panel() -> Mesh {
    let mut m = panel_base();
    let z = 0.0202;
    wall_quad(&mut m, -0.5, 0.0, 0.5, 0.86, z, [SNOW; 4]);
    wall_quad(&mut m, -0.5, 0.86, 0.5, 1.0, z, [SNOW, SNOW, MOTIF, MOTIF]);
    m
}

/// Wallpaper with vertical stripes (8 a metre: 4 dark, 4 light).
pub fn wall_stripes() -> Mesh {
    let mut m = panel_base();
    let z = 0.0202;
    for k in 0..4 {
        let x0 = -0.5 + 0.25 * k as f32 + 0.07;
        wall_quad(&mut m, x0, 0.0, x0 + 0.11, 1.0, z, [MOTIF; 4]);
    }
    m
}

/// Wallpaper with little dots in a staggered grid.
pub fn wall_dots() -> Mesh {
    let mut m = panel_base();
    let z = 0.0202;
    // a panel is ~2 m wide and 2.8 m tall: these radii come out round
    let r = Vec2::new(0.015, 0.011);
    for row in 0..14 {
        let y = 0.035 + row as f32 * (0.93 / 13.0);
        let shift = if row % 2 == 0 { 0.0 } else { 0.125 };
        for col in 0..4 {
            let x = -0.4375 + shift + 0.25 * col as f32;
            if x > 0.47 {
                continue;
            }
            dot(&mut m, Vec2::new(x, y), r, z, 8, DOT);
        }
    }
    m
}

/// The ceiling: a cream slab, three beams running from the back wall to the
/// hall, a fascia along the open (hall) edge and a cove along the back.
pub fn ceiling() -> Mesh {
    let mut m = cuboid(Vec3::new(1.0, 0.06, 1.0), CREAM).moved(Vec3::new(0.0, 0.03, 0.0));
    for x in [-0.25, 0.0, 0.25] {
        add(
            &mut m,
            rounded_box(Vec3::new(0.03, 0.12, 1.0), 0.008, 1, WOOD),
            at(Vec3::new(x, -0.06, 0.0)),
        );
    }
    // the fascia on the hall side
    add(
        &mut m,
        rounded_box(Vec3::new(1.0, 0.12, 0.02), 0.006, 1, WOOD),
        at(Vec3::new(0.0, -0.06, 0.49)),
    );
    m
}

/// A curtain hanging from y = `top`: a sheet in soft folds facing +z,
/// gathered by a tie-back two thirds down, its hem darker.
fn curtain(x: f32, top: f32, len: f32, w: f32) -> Mesh {
    const COLS: u16 = 10;
    const ROWS: u16 = 5;
    let mut m = Mesh::default();
    for r in 0..=ROWS {
        let t = r as f32 / ROWS as f32;
        let y = top - len * t;
        let waist = 1.0 - 0.4 * (-((t - 0.66) * 5.0).powi(2)).exp();
        let color = if r == ROWS { CURTAIN_HEM } else { CURTAIN };
        for c in 0..=COLS {
            let u = c as f32 / COLS as f32 - 0.5;
            let z = 0.02 * (0.5 + 0.5 * (u * 3.0 * std::f32::consts::TAU).cos());
            m.vertices.push(Vertex {
                pos: [x + u * w * waist, y, z],
                normal: pack_normal(Vec3::Z),
                color,
            });
        }
    }
    let w1 = COLS + 1;
    for r in 0..ROWS {
        for c in 0..COLS {
            let a = r * w1 + c;
            // rows go down: (a, a + w1, a + w1 + 1) is counter-clockwise from +z
            m.indices
                .extend([a, a + w1, a + w1 + 1, a, a + w1 + 1, a + 1]);
        }
    }
    smooth(&mut m);
    m
}

/// The frame of a 1.2 × 1.0 m window: rounded jambs, a cross of glazing
/// bars, a deep sill (a plant fits), a rod with two rosy curtains.
pub fn window_frame() -> Mesh {
    let mut m = Mesh::default();
    let (hw, hh) = (0.6, 0.5);
    let bar = 0.07;
    let depth = 0.09;
    // jambs and head
    for x in [-hw - bar / 2.0, hw + bar / 2.0] {
        add(
            &mut m,
            cuboid(Vec3::new(bar, 2.0 * hh + 2.0 * bar, depth), SNOW),
            at(Vec3::new(x, 0.0, depth / 2.0)),
        );
    }
    add(
        &mut m,
        cuboid(Vec3::new(2.0 * hw, bar, depth), SNOW),
        at(Vec3::new(0.0, hh + bar / 2.0, depth / 2.0)),
    );
    // the glazing cross, thinner and set back
    add(
        &mut m,
        cuboid(Vec3::new(0.035, 2.0 * hh, 0.03), SNOW),
        at(Vec3::new(0.0, 0.0, 0.035)),
    );
    add(
        &mut m,
        cuboid(Vec3::new(2.0 * hw, 0.035, 0.03), SNOW),
        at(Vec3::new(0.0, 0.12, 0.035)),
    );
    // the sill, deeper than the frame
    add(
        &mut m,
        rounded_box(Vec3::new(2.0 * hw + 0.26, 0.05, 0.17), 0.015, 1, WOOD),
        at(Vec3::new(0.0, -hh - 0.025, 0.085)),
    );
    // the rod and its knobs
    let rod_y = hh + bar + 0.06;
    add(
        &mut m,
        cylinder(0.012, 2.0 * hw + 0.5, 8, WOOD_DARK),
        at(Vec3::new(0.0, rod_y, 0.13)) * Mat4::from_rotation_z(std::f32::consts::FRAC_PI_2),
    );
    for s in [-1.0f32, 1.0] {
        add(
            &mut m,
            ellipsoid(Vec3::splat(0.022), 6, 3, WOOD_DARK),
            at(Vec3::new(s * (hw + 0.25), rod_y, 0.13)),
        );
        add(
            &mut m,
            curtain(s * (hw + 0.1), rod_y, 1.22, 0.26),
            at(Vec3::new(0.0, 0.0, 0.13)),
        );
    }
    m
}

/// The glass: a unit pane, sky fading to the horizon above distant hills.
pub fn window_pane() -> Mesh {
    let mut m = cuboid(Vec3::new(1.0, 1.0, 0.008), SNOW).moved(Vec3::new(0.0, 0.0, 0.004));
    let z = 0.0082;
    // the sky, paler towards the horizon
    let low = rgba(238, 242, 246);
    wall_quad(&mut m, -0.5, -0.2, 0.5, 0.5, z, [low, low, SNOW, SNOW]);
    // two rows of rolling hills, the far one paler
    for (row, color, y0, amp, z) in [
        (0, HILLS_FAR, -0.12, 0.08, z + 0.0002),
        (1, HILLS, -0.26, 0.1, z + 0.0004),
    ] {
        let base = m.vertices.len() as u16;
        let normal = pack_normal(Vec3::Z);
        const N: u16 = 8;
        for k in 0..=N {
            let x = -0.5 + k as f32 / N as f32;
            let ph = x * 7.0 + row as f32 * 1.7;
            let y = y0 + amp * (0.5 + 0.5 * ph.sin()) * (0.7 + 0.3 * (ph * 0.37).cos());
            m.vertices.push(Vertex {
                pos: [x, -0.5, z],
                normal,
                color,
            });
            m.vertices.push(Vertex {
                pos: [x, y, z],
                normal,
                color,
            });
        }
        for k in 0..N {
            let (b0, t0, b1, t1) = (
                base + 2 * k,
                base + 2 * k + 1,
                base + 2 * k + 2,
                base + 2 * k + 3,
            );
            m.indices.extend([b0, b1, t1, b0, t1, t0]);
        }
    }
    m
}

/// The cornice where the wall meets the ceiling: a stepped wooden moulding.
pub fn lintel() -> Mesh {
    let mut m = cuboid(Vec3::new(1.0, 0.06, 0.05), WOOD).moved(Vec3::new(0.0, -0.03, 0.025));
    add(
        &mut m,
        cuboid(Vec3::new(1.0, 0.04, 0.08), WOOD),
        at(Vec3::new(0.0, -0.02, 0.04)),
    );
    add(
        &mut m,
        cuboid(Vec3::new(1.0, 0.03, 0.03), CREAM),
        at(Vec3::new(0.0, -0.085, 0.015)),
    );
    m
}

/// The skirting board: a rounded-top board with a thin lip.
pub fn skirting() -> Mesh {
    let mut m =
        rounded_box(Vec3::new(1.0, 0.1, 0.022), 0.008, 1, WOOD).moved(Vec3::new(0.0, 0.05, 0.011));
    add(
        &mut m,
        cuboid(Vec3::new(1.0, 0.012, 0.025), WOOD_DARK),
        at(Vec3::new(0.0, 0.006, 0.0125)),
    );
    m
}

/// The light a window throws on the floor: a thin slab of four panes (the
/// glazing cross leaves its shadow), unit-sized on y = 0..0.004; x across
/// the window, z away from the wall (the upper panes land further out).
pub fn light_patch() -> Mesh {
    let mut m = Mesh::default();
    let bar = 0.035;
    // the cross: a vertical bar at x = 0, the transom 62 % of the way out
    let zt = 0.12;
    for (x0, x1) in [(-0.5, -bar), (bar, 0.5)] {
        for (z0, z1) in [(-0.5, zt - bar), (zt + bar, 0.5)] {
            let c = Vec3::new((x0 + x1) / 2.0, 0.002, (z0 + z1) / 2.0);
            add(
                &mut m,
                cuboid(Vec3::new(x1 - x0, 0.004, z1 - z0), SNOW),
                at(c),
            );
        }
    }
    m
}
