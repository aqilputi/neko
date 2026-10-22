//! Rendering policy, the pure part: level of detail per screen (texture
//! update rate and frame-callback throttle, §5.3) and the order of the
//! composed layers (§5.1). The GL side lives in `frame.rs`.

use std::time::Duration;

use glam::Vec4;
use nkw_core::ScreenId;

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
    /// the mode label
    Hud,
    /// the sky-coloured fade of a cut
    Fade,
    /// the FOCO window and its popups, 1:1
    Focus(ScreenId),
    /// the frozen world around FOCO, darkened (§3.6)
    Dim,
    /// the world texture
    World,
}

/// The layers, topmost first (the order `render_output` wants).
pub fn layers(w: &World, fading: bool) -> Vec<Layer> {
    let mut l = Vec::new();
    let focus = w.focus_2d().map(|(id, _)| id);
    if focus.is_none() {
        l.push(Layer::Hud);
    }
    if fading {
        l.push(Layer::Fade);
    }
    if let Some(id) = focus {
        l.push(Layer::Focus(id));
        l.push(Layer::Dim);
    }
    l.push(Layer::World);
    l
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
            [Layer::Focus(ScreenId(1)), Layer::Dim, Layer::World]
        );
    }
}
