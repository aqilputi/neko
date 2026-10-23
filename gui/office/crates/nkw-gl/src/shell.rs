//! The room shell (S17.3), composed: what `nkw`'s `shell::room` and
//! `shell::hall` draw, kept here so the goldens render the very same
//! composition. Pure: instances only.
//!
//! A room (its local frame: x along the room from `slot.x0`, z from the
//! back wall towards the hall, `layout::to_world`'s convention) is a floor
//! by kind, three walls, a ceiling with beams; when it is in the culling
//! set also wallpaper (pattern and colour by index), back-wall windows
//! (one per 3 m) whose panes glow with the band's light and throw a soft
//! patch on the floor, a cornice, skirting and a rug. The side towards the
//! hall stays open (a dollhouse), framed by the ceiling's fascia.

use glam::{Mat4, Quat, Vec2, Vec3};
use nkw_core::RoomSlot;
use nkw_core::layout::{self, HALL_W, ROOM_DEPTH};
use nkw_core::persist::RoomKind;

use crate::draw::{Instance, MeshId, sky_for_band, window_light};
use crate::mesh::palette;

/// Wall height, metres.
pub const WALL_H: f32 = 2.8;
/// The ceiling's underside, metres.
pub const CEILING_Y: f32 = WALL_H;
/// Wall thickness, metres (the inner faces are half of it off the room's edges).
pub const WALL_T: f32 = 0.1;
/// One window per this many metres of back wall (at least one).
pub const WINDOW_EVERY_M: f32 = 3.0;
/// The window opening (width, height) and the sill's height, metres.
pub const WINDOW: Vec2 = Vec2::new(1.2, 1.0);
pub const SILL_Y: f32 = 1.0;
/// Wallpaper panels are about this wide, metres (patterns are not stretched much).
const PANEL_M: f32 = 2.0;

/// The wallpaper colours: cream, peach, lilac (light versions of the palette's).
pub const WALLPAPERS: [[u8; 4]; 3] = [
    [250, 240, 222, 255],
    [252, 226, 210, 255],
    [232, 222, 244, 255],
];
/// The walls' plain colour (the outside and the far rooms).
pub const WALL: [u8; 4] = [246, 238, 223, 255];
const CEILING: [u8; 4] = [255, 250, 240, 255];
const FLOOR: [u8; 4] = [255, 255, 255, 255];
const HALL_FLOOR: [u8; 4] = [236, 226, 214, 255];
const RUNNER: [u8; 4] = [190, 214, 196, 255];

/// Rug colours by room index: rose, mint, sky, butter, lilac, peach.
pub const RUGS: [[u8; 4]; 6] = [
    palette::ROSE,
    palette::MINT,
    palette::SKY,
    palette::BUTTER,
    palette::LILAC,
    palette::PEACH,
];

/// What the shell needs to know of a room.
#[derive(Clone, Copy, Debug)]
pub struct Look<'a> {
    pub kind: &'a RoomKind,
    /// its index in the strip (picks the wallpaper and the rug)
    pub index: usize,
    /// the time-of-day band (0..8)
    pub band: u8,
    /// in the culling set: interior detail
    pub visible: bool,
}

/// The wallpaper of the room at `index`: the pattern (stripes, dots,
/// plain) and its colour (cream, peach, lilac).
pub fn wallpaper(index: usize) -> (MeshId, [u8; 4]) {
    let mesh = [MeshId::WallStripes, MeshId::WallDots, MeshId::WallPanel][index % 3];
    (mesh, WALLPAPERS[index % 3])
}

/// The floor of a room of `kind`.
pub fn floor_mesh(kind: &RoomKind) -> MeshId {
    match kind {
        RoomKind::Library => MeshId::FloorTiles,
        _ => MeshId::FloorPlanks,
    }
}

/// The rug of a room: (centre x, centre z, width, depth) in its local frame
/// and its tint; the reception has none (its furniture is the point).
pub fn rug(kind: &RoomKind, index: usize, len: f32) -> Option<(Vec2, Vec2, [u8; 4])> {
    match kind {
        RoomKind::Reception => None,
        // a long runner down the reading room
        RoomKind::Library => Some((
            Vec2::new(len / 2.0, 2.75),
            Vec2::new((len - 1.4).max(1.5), 1.0),
            RUGS[(index + 2) % RUGS.len()],
        )),
        // under the chairs, where the villagers' paws are
        _ => Some((
            Vec2::new(len / 2.0, 1.85),
            Vec2::new((len - 1.6).clamp(1.5, 6.0), 1.9),
            RUGS[index % RUGS.len()],
        )),
    }
}

/// The local x of each back-wall window of a room `len` metres long.
pub fn window_xs(len: f32) -> Vec<f32> {
    let n = ((len / WINDOW_EVERY_M).floor() as usize).max(1);
    (0..n).map(|k| len * (k as f32 + 0.5) / n as f32).collect()
}

/// Panel centres and width for a wall `len` metres long.
fn panels(len: f32) -> (usize, f32) {
    let n = ((len / PANEL_M).round() as usize).max(1);
    (n, len / n as f32)
}

/// The room's local frame in the world.
pub fn frame(slot: &RoomSlot) -> Mat4 {
    let f = layout::room_frame(slot);
    Mat4::from_rotation_translation(Quat::from_rotation_y(-f.yaw), f.origin)
}

fn inst(mesh: MeshId, model: Mat4, tint: [u8; 4]) -> Instance {
    Instance {
        mesh,
        model,
        tint,
        emissive: 0.0,
        fade: 0.0,
    }
}

fn trs(scale: Vec3, yaw: f32, pos: Vec3) -> Mat4 {
    Mat4::from_scale_rotation_translation(scale, Quat::from_rotation_y(yaw), pos)
}

/// The three walls as `(position of the inner face's middle at the floor,
/// yaw turning +z into the room, length)` in the local frame.
fn walls(len: f32) -> [(Vec3, f32, f32); 3] {
    let h = WALL_T / 2.0;
    [
        (Vec3::new(len / 2.0, 0.0, h), 0.0, len),
        (
            Vec3::new(h, 0.0, ROOM_DEPTH / 2.0),
            std::f32::consts::FRAC_PI_2,
            ROOM_DEPTH,
        ),
        (
            Vec3::new(len - h, 0.0, ROOM_DEPTH / 2.0),
            -std::f32::consts::FRAC_PI_2,
            ROOM_DEPTH,
        ),
    ]
}

/// How bright the patch a window throws on the floor glows by band (none
/// at night); it lies under the rug, so only the boards show it.
pub fn floor_light(band: u8) -> f32 {
    [0.0, 0.08, 0.14, 0.16, 0.16, 0.16, 0.14, 0.0][(band % 8) as usize]
}

/// The colour of the sunlight patch on the floor by band (a warm bright
/// wood tone, golden late, pale at dawn).
fn sun_patch(band: u8) -> [u8; 4] {
    [
        [200, 190, 200, 255],
        [238, 214, 196, 255],
        [250, 222, 178, 255],
        [252, 222, 174, 255],
        [252, 220, 170, 255],
        [255, 210, 150, 255],
        [255, 196, 146, 255],
        [200, 190, 200, 255],
    ][(band % 8) as usize]
}

/// A room's shell: the instances appended to `out`; `cam` fades the
/// windows you walk up to (the structure never fades).
pub fn room(slot: &RoomSlot, look: &Look, cam: Vec3, out: &mut Vec<Instance>) {
    let f = frame(slot);
    let len = slot.len;
    let d = ROOM_DEPTH;
    let mid = Vec3::new(len / 2.0, 0.0, d / 2.0);
    // the floor and the ceiling
    out.push(inst(
        floor_mesh(look.kind),
        f * trs(Vec3::new(len, 1.0, d), 0.0, mid),
        FLOOR,
    ));
    out.push(Instance {
        emissive: 0.5,
        ..inst(
            MeshId::Ceiling,
            f * trs(Vec3::new(len, 1.0, d), 0.0, mid + Vec3::Y * CEILING_Y),
            CEILING,
        )
    });
    // the walls (plain boxes: their edges show from the hall)
    out.push(inst(
        MeshId::Box,
        f * trs(
            Vec3::new(len + WALL_T, WALL_H, WALL_T),
            0.0,
            Vec3::new(len / 2.0, WALL_H / 2.0, 0.0),
        ),
        WALL,
    ));
    for x in [0.0, len] {
        out.push(inst(
            MeshId::Box,
            f * trs(
                Vec3::new(WALL_T, WALL_H, d),
                0.0,
                Vec3::new(x, WALL_H / 2.0, d / 2.0),
            ),
            WALL,
        ));
    }
    if !look.visible {
        return;
    }

    // wallpaper, cornice and skirting on the three inner faces
    let (paper, tint) = wallpaper(look.index);
    for (at, yaw, wl) in walls(len) {
        let along = Quat::from_rotation_y(yaw) * Vec3::X;
        let (n, w) = panels(wl);
        for k in 0..n {
            let off = (k as f32 + 0.5) * w - wl / 2.0;
            out.push(inst(
                paper,
                f * trs(Vec3::new(w, WALL_H, 1.0), yaw, at + along * off),
                tint,
            ));
        }
        let trim = |mesh, y: f32| {
            inst(
                mesh,
                f * trs(Vec3::new(wl, 1.0, 1.0), yaw, at + Vec3::Y * y),
                [255; 4],
            )
        };
        out.push(trim(MeshId::Skirting, 0.0));
        out.push(trim(MeshId::Lintel, CEILING_Y));
    }

    // the windows: frame, glowing pane, the patch of light below
    let (pane, glow) = window_light(look.band);
    let patch = floor_light(look.band);
    let sun = sky_for_band(look.band).sun_dir;
    let face = WALL_T / 2.0 + 0.0205;
    for x in window_xs(len) {
        let c = Vec3::new(x, SILL_Y + WINDOW.y / 2.0, face);
        let wf = f * trs(Vec3::ONE, 0.0, c);
        out.push(Instance {
            fade: layout::fade_for(cam, wf.transform_point3(Vec3::ZERO)),
            ..inst(MeshId::WindowFrame, wf, [255; 4])
        });
        let pm = f * trs(
            Vec3::new(WINDOW.x, WINDOW.y, 1.0),
            0.0,
            c + Vec3::Z * 0.0005,
        );
        out.push(Instance {
            emissive: glow,
            fade: layout::fade_for(cam, pm.transform_point3(Vec3::ZERO)),
            ..inst(MeshId::WindowPane, pm, pane)
        });
        if patch > 0.0 {
            // the sun's local x leans the patch away from it
            let sx = f.inverse().transform_vector3(sun).x;
            let at = Vec3::new(x - 0.45 * sx, 0.004, face + 1.3);
            let warm = sun_patch(look.band);
            out.push(Instance {
                emissive: patch,
                ..inst(
                    MeshId::LightPatch,
                    f * trs(Vec3::new(WINDOW.x, 1.0, 1.1), 0.0, at),
                    warm,
                )
            });
        }
    }

    if let Some((c, size, tint)) = rug(look.kind, look.index, len) {
        out.push(inst(
            MeshId::Rug,
            f * trs(
                Vec3::new(size.x, 1.0, size.y),
                0.0,
                Vec3::new(c.x, 0.0, c.y),
            ),
            tint,
        ));
    }
}

/// The hall's floor under every door: boards along the hall in ~4 m
/// stretches, a runner down the middle of each.
pub fn hall(slots: &[RoomSlot], band: u8, out: &mut Vec<Instance>) {
    let _ = band;
    let end = slots.iter().map(|s| s.x0 + s.len).fold(0.0, f32::max) + 2.0;
    let start = -1.0;
    let n = ((end - start) / 4.0).ceil().max(1.0) as usize;
    let w = (end - start) / n as f32;
    for k in 0..n {
        let x = start + (k as f32 + 0.5) * w;
        out.push(inst(
            MeshId::FloorPlanks,
            trs(Vec3::new(w, 1.0, HALL_W), 0.0, Vec3::new(x, 0.0, 0.0)),
            HALL_FLOOR,
        ));
        out.push(inst(
            MeshId::Rug,
            trs(Vec3::new(w - 0.5, 1.0, 1.1), 0.0, Vec3::new(x, 0.0, 0.0)),
            RUNNER,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nkw_core::strip::rooms_along_hall;

    fn slot(len: f32) -> RoomSlot {
        rooms_along_hall(&[len])[0]
    }

    fn count(v: &[Instance], m: MeshId) -> usize {
        v.iter().filter(|i| i.mesh == m).count()
    }

    fn build(kind: &RoomKind, index: usize, len: f32, band: u8, visible: bool) -> Vec<Instance> {
        let mut v = Vec::new();
        let look = Look {
            kind,
            index,
            band,
            visible,
        };
        room(&slot(len), &look, Vec3::new(0.0, 1.5, 10.0), &mut v);
        v
    }

    #[test]
    fn outside_the_culling_set_only_the_shell() {
        let ctx = RoomKind::Context("acme".into());
        for len in [6.0, 9.4, 14.0] {
            let v = build(&ctx, 1, len, 3, false);
            assert!(v.len() <= 8, "{}", v.len());
            assert_eq!(count(&v, MeshId::WindowPane), 0);
            assert_eq!(count(&v, MeshId::Skirting), 0);
            assert_eq!(count(&v, MeshId::Ceiling), 1);
            let inside = build(&ctx, 1, len, 3, true);
            assert!(inside.len() > v.len());
            assert_eq!(
                &inside[..v.len()],
                &v[..],
                "the shell comes first, the same"
            );
        }
    }

    #[test]
    fn one_window_per_three_metres_of_back_wall() {
        for (len, n) in [(2.5, 1), (6.0, 2), (8.9, 2), (9.0, 3), (14.0, 4)] {
            assert_eq!(window_xs(len).len(), n, "{len}");
            let v = build(&RoomKind::User, 0, len, 4, true);
            assert_eq!(count(&v, MeshId::WindowFrame), n);
            assert_eq!(count(&v, MeshId::WindowPane), n);
            // evenly spaced, inside the wall with room for the frame
            let xs = window_xs(len);
            assert!(xs.iter().all(|x| *x > 0.6 && *x < len - 0.6), "{xs:?}");
        }
    }

    #[test]
    fn wallpaper_patterns_cycle_by_index() {
        let pats: Vec<MeshId> = (0..6).map(|i| wallpaper(i).0).collect();
        assert_eq!(
            pats,
            [
                MeshId::WallStripes,
                MeshId::WallDots,
                MeshId::WallPanel,
                MeshId::WallStripes,
                MeshId::WallDots,
                MeshId::WallPanel
            ]
        );
        assert_ne!(wallpaper(0).1, wallpaper(1).1);
        assert_ne!(wallpaper(1).1, wallpaper(2).1);
        let v = build(&RoomKind::Free, 4, 6.0, 3, true);
        assert!(count(&v, MeshId::WallDots) >= 3);
        assert_eq!(count(&v, MeshId::WallStripes), 0);
    }

    #[test]
    fn floors_and_rugs_by_kind() {
        let ctx = RoomKind::Context("demo".into());
        let rec = build(&RoomKind::Reception, 0, 6.0, 3, true);
        assert_eq!(count(&rec, MeshId::FloorPlanks), 1);
        assert_eq!(count(&rec, MeshId::Rug), 0);
        let lib = build(&RoomKind::Library, 3, 6.0, 3, true);
        assert_eq!(count(&lib, MeshId::FloorTiles), 1);
        assert_eq!(count(&lib, MeshId::Rug), 1);
        let a = build(&ctx, 1, 6.0, 3, true);
        let b = build(&ctx, 2, 6.0, 3, true);
        assert_eq!(count(&a, MeshId::FloorPlanks), 1);
        let tint = |v: &[Instance]| v.iter().find(|i| i.mesh == MeshId::Rug).unwrap().tint;
        assert_ne!(tint(&a), tint(&b), "the rug's hue follows the index");
    }

    #[test]
    fn panes_follow_the_band_and_the_patch_goes_at_night() {
        let day = build(&RoomKind::User, 0, 6.0, 4, true);
        let night = build(&RoomKind::User, 0, 6.0, 0, true);
        let pane = |v: &[Instance]| *v.iter().find(|i| i.mesh == MeshId::WindowPane).unwrap();
        assert!(pane(&day).emissive > pane(&night).emissive);
        assert_eq!(pane(&night).emissive, 0.15);
        assert_eq!(count(&day, MeshId::LightPatch), 2);
        assert_eq!(count(&night, MeshId::LightPatch), 0);
    }

    #[test]
    fn the_shell_sits_on_the_rooms_frame() {
        for side in [0usize, 1] {
            let s = rooms_along_hall(&[6.0, 6.0])[side];
            let mut v = Vec::new();
            let look = Look {
                kind: &RoomKind::User,
                index: 0,
                band: 3,
                visible: true,
            };
            room(&s, &look, Vec3::ZERO, &mut v);
            let floor = v[0].model.transform_point3(Vec3::ZERO);
            let centre = layout::to_world(&s, Vec3::new(3.0, 0.0, ROOM_DEPTH / 2.0));
            assert!((floor - centre).length() < 1e-4);
            let ceil = v[1].model.transform_point3(Vec3::ZERO);
            assert!((ceil.y - CEILING_Y).abs() < 1e-5);
            // every window pane faces into the room (towards the hall)
            for p in v.iter().filter(|i| i.mesh == MeshId::WindowPane) {
                let n = p.model.transform_vector3(Vec3::Z).normalize();
                let into =
                    (layout::to_world(&s, Vec3::Z) - layout::to_world(&s, Vec3::ZERO)).normalize();
                assert!(n.dot(into) > 0.99);
                let y = p.model.transform_point3(Vec3::new(0.0, -0.5, 0.0)).y;
                assert!((y - SILL_Y).abs() < 1e-4);
            }
            // the structure never fades, even with the camera inside a wall
            assert!(
                v.iter()
                    .filter(|i| i.mesh != MeshId::WindowFrame && i.mesh != MeshId::WindowPane)
                    .all(|i| i.fade == 0.0)
            );
        }
    }

    #[test]
    fn the_hall_has_boards_and_a_runner() {
        let slots = rooms_along_hall(&[6.0, 7.5, 6.0, 8.0]);
        let mut v = Vec::new();
        hall(&slots, 3, &mut v);
        assert_eq!(count(&v, MeshId::FloorPlanks), count(&v, MeshId::Rug));
        let end = slots.iter().map(|s| s.x0 + s.len).fold(0.0, f32::max);
        let covered: f32 = v
            .iter()
            .filter(|i| i.mesh == MeshId::FloorPlanks)
            .map(|i| i.model.transform_vector3(Vec3::X).length())
            .sum();
        assert!(covered >= end + 2.0, "{covered} < {end}");
    }

    #[test]
    fn a_typical_room_stays_in_budget() {
        let v = build(&RoomKind::Context("acme".into()), 1, 9.4, 3, true);
        assert!(v.len() <= 40, "{} instances", v.len());
        let tris: u32 = v.iter().map(|i| crate::mesh::triangle_count(i.mesh)).sum();
        assert!(tris <= 12_000, "{tris} tris");
    }
}
