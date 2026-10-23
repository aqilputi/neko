//! Rendering policy, the pure part: level of detail per screen (texture
//! update rate and frame-callback throttle, §5.3) and the order of the
//! composed layers (§5.1). The GL side lives in `frame.rs`.

use std::time::Duration;

use glam::Vec4;
use nkw_core::{CatId, ScreenId, Target};

use crate::world::{FOV_Y, World};

/// How much a screen matters for this frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lod {
    /// the FOCO target
    Focus,
    /// the selected screen of the current station (NAV)
    Current,
    /// visible, closer than 6 m
    Near,
    /// visible, 6 m or farther
    Far,
    /// out of view, another room, or frozen behind FOCO
    Hidden,
}

/// Beyond this a visible screen is `Far`, metres.
pub const NEAR_M: f32 = 6.0;

/// The frame-callback throttle of a LOD (`Window::send_frame`).
pub fn throttle(l: Lod) -> Duration {
    match l {
        Lod::Focus | Lod::Current => Duration::ZERO,
        Lod::Near => Duration::from_millis(66),
        Lod::Far => Duration::from_millis(250),
        Lod::Hidden => Duration::from_secs(1),
    }
}

/// How often a commit of a screen at this LOD may redraw the world, Hz;
/// `None`: it only recomposes (FOCO, 2D); `Some(0)`: the texture is frozen.
pub fn world_rate(l: Lod) -> Option<f64> {
    match l {
        Lod::Focus => None,
        Lod::Current => Some(30.0),
        Lod::Near => Some(15.0),
        Lod::Far => Some(4.0),
        Lod::Hidden => Some(0.0),
    }
}

/// Is `p` inside the camera's view (with a margin, NDC units)?
fn in_view(w: &World, p: glam::Vec3) -> bool {
    let aspect = w.out.width_px as f32 / w.out.height_px.max(1) as f32;
    let proj = glam::camera::rh::proj::opengl::perspective(FOV_Y, aspect, 0.05, 100.0);
    let c = proj * w.camera.view() * Vec4::from((p, 1.0));
    if c.w <= 0.0 {
        return false;
    }
    let n = c.truncate() / c.w;
    n.x.abs() <= 1.2 && n.y.abs() <= 1.2
}

pub fn lod(w: &World, id: ScreenId) -> Lod {
    if w.focus_target() == Some(id) {
        return Lod::Focus;
    }
    if w.frozen {
        return Lod::Hidden;
    }
    if w.selected() == Some(id) {
        return Lod::Current;
    }
    let Some((r, d, k)) = w.locate(id) else {
        return Lod::Hidden;
    };
    if r != w.current_room() {
        return Lod::Hidden;
    }
    let c = w.screen_place(r, d, k).center;
    if !in_view(w, c) {
        return Lod::Hidden;
    }
    if w.camera.pos.distance(c) < NEAR_M {
        Lod::Near
    } else {
        Lod::Far
    }
}

/// A layer of the composed frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layer {
    /// the overlay's elements (`Overlay::elements`: the mode label …)
    Hud,
    /// the sky-coloured fade of a cut
    Fade,
    /// the FOCO window and its popups, 1:1
    Focus(ScreenId),
    /// FOCO on a cat's terminal: `Cats::focus_buffer`, 1:1
    FocusCat(CatId),
    /// the frozen world around FOCO, darkened (§3.6)
    Dim,
    /// the world texture
    World,
}

/// The layers, topmost first (the order `render_output` wants). The
/// overlay is always asked; it hides the mode label in FOCO itself.
pub fn layers(w: &World, fading: bool) -> Vec<Layer> {
    let mut l = vec![Layer::Hud];
    if fading {
        l.push(Layer::Fade);
    }
    if let Some(t) = w.focus_2d_target().map(|(t, _)| t) {
        l.push(match t {
            Target::Screen(id) => Layer::Focus(id),
            Target::CatTerm(c) => Layer::FocusCat(c),
        });
        // a fullscreen FOCO covers the output: nothing to darken
        if !w.covers_output(t) {
            l.push(Layer::Dim);
        }
    }
    l.push(Layer::World);
    l
}

/// What a world pass changed in the world texture (S4, hypothesis a).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorldDamage {
    /// the camera, the instances or the list changed: everything
    Full,
    /// the same list and no client committed: the texture is still right
    None,
    /// only these screens' contents changed: their bounds, texture px `[x, y, w, h]`
    Rects(Vec<[i32; 4]>),
}

/// Padding around a screen's projected bounds (outline glow, MSAA), px.
pub const DAMAGE_PAD: i32 = 12;

/// The damage of the world texture for this pass: `prev` is the last pass's
/// list, `committed` the indices of `now.screens` whose client committed
/// since then; `size` the texture's px; `gl_rows`: row 0 is the bottom
/// (a GL render target read as a buffer).
pub fn world_damage(
    prev: Option<&nkw_gl::draw::DrawList>,
    now: &nkw_gl::draw::DrawList,
    committed: &[usize],
    size: [u32; 2],
    gl_rows: bool,
) -> WorldDamage {
    if prev != Some(now) {
        return WorldDamage::Full;
    }
    if committed.is_empty() {
        return WorldDamage::None;
    }
    let vp = now.camera.proj() * now.camera.view();
    let (w, h) = (size[0] as f32, size[1] as f32);
    let mut rects = Vec::new();
    for &i in committed {
        let Some(q) = now.screens.get(i) else {
            continue;
        };
        let mut lo = glam::Vec2::splat(f32::INFINITY);
        let mut hi = glam::Vec2::splat(f32::NEG_INFINITY);
        for (x, y) in [(-0.5, -0.5), (0.5, -0.5), (-0.5, 0.5), (0.5, 0.5)] {
            let c = vp * (q.model * Vec4::new(x, y, 0.0, 1.0));
            if c.w <= 1e-4 {
                // a corner behind the camera: the projection is unbounded
                return WorldDamage::Full;
            }
            let n = c.truncate() / c.w;
            let px = (n.x * 0.5 + 0.5) * w;
            let py = if gl_rows {
                (n.y * 0.5 + 0.5) * h
            } else {
                (0.5 - n.y * 0.5) * h
            };
            lo = lo.min(glam::Vec2::new(px, py));
            hi = hi.max(glam::Vec2::new(px, py));
        }
        let x0 = (lo.x.floor() as i32 - DAMAGE_PAD).max(0);
        let y0 = (lo.y.floor() as i32 - DAMAGE_PAD).max(0);
        let x1 = (hi.x.ceil() as i32 + DAMAGE_PAD).min(size[0] as i32);
        let y1 = (hi.y.ceil() as i32 + DAMAGE_PAD).min(size[1] as i32);
        if x1 > x0 && y1 > y0 {
            rects.push([x0, y0, x1 - x0, y1 - y0]);
        }
    }
    if rects.is_empty() {
        WorldDamage::None
    } else {
        WorldDamage::Rects(rects)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::world::{Cmd, Opts, Room, Screen};
    use glam::Vec2;
    use nkw_core::strip::{Desk, Width};
    use nkw_core::{Output, RoomId};

    /// Rooms of one-screen desks with these screen ids, 624×720 each.
    pub(crate) fn world(rooms: &[&[u32]]) -> World {
        let out = Output {
            width_px: 1280,
            height_px: 720,
            scale: 1.0,
        };
        let rooms: Vec<Room> = rooms
            .iter()
            .enumerate()
            .map(|(i, desks)| Room {
                id: RoomId(i as u32),
                name: format!("r{i}"),
                desks: desks
                    .iter()
                    .map(|&s| Desk {
                        width: Width::Half,
                        screens: vec![ScreenId(s)],
                        cat: None,
                    })
                    .collect(),
            })
            .collect();
        let ids: Vec<u32> = rooms
            .iter()
            .flat_map(|r| r.desks.iter().flat_map(|d| d.screens.iter().map(|s| s.0)))
            .collect();
        let mut w = World::with_rooms(
            out,
            Opts {
                reduced_motion: true,
            },
            rooms,
        );
        for id in ids {
            w.screens.insert(
                ScreenId(id),
                Screen {
                    id: ScreenId(id),
                    app_id: String::new(),
                    title: String::new(),
                    size: Vec2::new(624.0, 720.0),
                },
            );
        }
        w
    }

    #[test]
    fn throttle_table() {
        let t = |l| throttle(l).as_millis();
        assert_eq!(
            [Lod::Focus, Lod::Current, Lod::Near, Lod::Far, Lod::Hidden].map(t),
            [0, 0, 66, 250, 1000]
        );
        assert_eq!(world_rate(Lod::Focus), None);
        assert_eq!(world_rate(Lod::Hidden), Some(0.0));
        assert_eq!(world_rate(Lod::Near), Some(15.0));
    }

    #[test]
    fn lod_by_place() {
        // room 0: desks 1..=8 (the strip runs far to the right); room 1: 9
        let mut w = world(&[&[1, 2, 3, 4, 5, 6, 7, 8], &[9]]);
        assert_eq!(lod(&w, ScreenId(1)), Lod::Current);
        assert_eq!(lod(&w, ScreenId(2)), Lod::Near, "the neighbour");
        assert_eq!(lod(&w, ScreenId(8)), Lod::Hidden, "out of the view");
        assert_eq!(lod(&w, ScreenId(9)), Lod::Hidden, "another room");
        assert_eq!(lod(&w, ScreenId(42)), Lod::Hidden, "unknown");
        w.command(Cmd::Focus, 0.0);
        w.tick(1e9);
        assert_eq!(lod(&w, ScreenId(1)), Lod::Focus);
        assert_eq!(lod(&w, ScreenId(2)), Lod::Hidden, "frozen behind FOCO");
    }

    #[test]
    fn far_screens() {
        let mut w = world(&[&[1, 2]]);
        w.camera.pos.z += 6.0;
        assert_eq!(lod(&w, ScreenId(2)), Lod::Far);
    }

    #[test]
    fn layer_order() {
        let mut w = world(&[&[1]]);
        assert_eq!(layers(&w, false), [Layer::Hud, Layer::World]);
        assert_eq!(layers(&w, true), [Layer::Hud, Layer::Fade, Layer::World]);
        w.command(Cmd::Focus, 0.0);
        assert_eq!(
            layers(&w, false),
            [Layer::Hud, Layer::World],
            "gliding: still a quad"
        );
        w.tick(1e9);
        assert_eq!(
            layers(&w, false),
            [
                Layer::Hud,
                Layer::Focus(ScreenId(1)),
                Layer::Dim,
                Layer::World
            ]
        );
    }

    #[test]
    fn no_dim_under_a_fullscreen_focus() {
        let mut w = world(&[&[1]]);
        w.set_fullscreen(ScreenId(1), true, 0.0);
        w.command(Cmd::Focus, 0.0);
        w.tick(1e9);
        assert!(w.covers_output(Target::Screen(ScreenId(1))));
        assert_eq!(
            layers(&w, false),
            [Layer::Hud, Layer::Focus(ScreenId(1)), Layer::World]
        );
    }

    #[test]
    fn a_cats_terminal_is_a_focus_layer_too() {
        let mut w = world(&[&[1]]);
        w.rooms[0].desks[0].cat = Some(CatId(5));
        w.action(nkw_core::Action::Focus(Target::CatTerm(CatId(5))), 0.0);
        w.tick(1e9);
        assert_eq!(
            layers(&w, false),
            [
                Layer::Hud,
                Layer::FocusCat(CatId(5)),
                Layer::Dim,
                Layer::World
            ]
        );
    }

    fn list_with_screen() -> nkw_gl::draw::DrawList {
        use nkw_gl::draw::{Camera, ClientTex, DrawList, ScreenQuad, sky_for_band};
        let mut l = DrawList::empty(
            Camera {
                pose: nkw_core::Pose {
                    pos: glam::Vec3::new(0.0, 1.5, 2.0),
                    yaw: 0.0,
                    pitch: 0.0,
                },
                fov_y: FOV_Y,
                near: 0.05,
                far: 60.0,
                aspect: 16.0 / 9.0,
            },
            sky_for_band(3),
        );
        l.screens.push(ScreenQuad {
            model: glam::Mat4::from_scale_rotation_translation(
                glam::Vec3::new(0.6, 0.4, 1.0),
                glam::Quat::IDENTITY,
                glam::Vec3::new(0.0, 1.5, 0.0),
            ),
            tex: ClientTex {
                id: 7,
                external: false,
                size: [640, 480],
                y_flip: false,
                opaque: true,
            },
            dim: 0.0,
            selected: false,
        });
        l
    }

    #[test]
    fn world_damage_rules() {
        let a = list_with_screen();
        let size = [1280, 720];
        assert_eq!(world_damage(None, &a, &[], size, true), WorldDamage::Full);
        assert_eq!(
            world_damage(Some(&a), &a, &[], size, true),
            WorldDamage::None
        );
        let mut moved = a.clone();
        moved.camera.pose.pos.x += 0.01;
        assert_eq!(
            world_damage(Some(&a), &moved, &[0], size, true),
            WorldDamage::Full
        );
        let WorldDamage::Rects(r) = world_damage(Some(&a), &a, &[0], size, true) else {
            panic!("rects")
        };
        assert_eq!(r.len(), 1);
        let [x, y, w, h] = r[0];
        // centred on the frame (the screen is straight ahead at eye height)
        assert!(((x + w / 2) - 640).abs() <= 1, "{r:?}");
        assert!(((y + h / 2) - 360).abs() <= 1, "{r:?}");
        // 0.6 m at 2 m with a 60° fov: 0.6 / (2·tan30°·2) of the height ≈ 0.26·720·(16/9)… in px
        let want_w = 0.6 / (2.0 * 2.0 * (FOV_Y / 2.0).tan() * 16.0 / 9.0) * 1280.0;
        assert!(
            (w as f32 - (want_w + 2.0 * DAMAGE_PAD as f32)).abs() <= 3.0,
            "{w} vs {want_w}"
        );
        assert!(w < 1280 / 2 && h < 720 / 2);
        // top-down rows mirror the GL rows
        let WorldDamage::Rects(t) = world_damage(Some(&a), &a, &[0], [1280, 700], false) else {
            panic!()
        };
        let WorldDamage::Rects(g) = world_damage(Some(&a), &a, &[0], [1280, 700], true) else {
            panic!()
        };
        assert_eq!(t[0][1], 700 - g[0][1] - g[0][3]);
        // an unknown index is ignored
        assert_eq!(
            world_damage(Some(&a), &a, &[5], size, true),
            WorldDamage::None
        );
    }
}
