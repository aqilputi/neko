//! The room shell (contract S17.3): the hall's floor and each room's floor,
//! walls, ceiling, wallpaper, windows and trims. The composition is
//! `nkw_gl::shell`'s (pure, so the goldens draw the very same rooms); this
//! file feeds it the world's rooms (R5).

use nkw_core::RoomSlot;
use nkw_core::persist::RoomKind;
use nkw_gl::draw::DrawList;

/// Wall height, metres.
pub const WALL_H: f32 = nkw_gl::shell::WALL_H;
/// The ceiling's underside, metres.
pub const CEILING_Y: f32 = nkw_gl::shell::CEILING_Y;

/// One room's shell to draw.
pub struct Shell<'a> {
    pub slot: &'a RoomSlot,
    /// the room's index in the strip
    pub index: usize,
    pub kind: RoomKind,
    /// the time-of-day band (0..8)
    pub band: u8,
    /// in the culling set: interior detail
    pub visible: bool,
}

/// The hall's floor, under every room's door.
pub fn hall(list: &mut DrawList, slots: &[RoomSlot], band: u8) {
    nkw_gl::shell::hall(slots, band, &mut list.instances);
}

/// A room's shell: floor by kind, walls, ceiling; in the culling set also
/// wallpaper, windows lit by the band, skirting, cornice and rug.
pub fn room(list: &mut DrawList, s: &Shell) {
    let look = nkw_gl::shell::Look {
        kind: &s.kind,
        index: s.index,
        band: s.band,
        visible: s.visible,
    };
    let cam = list.camera.pose.pos;
    nkw_gl::shell::room(s.slot, &look, cam, &mut list.instances);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::tests::world;
    use nkw_gl::draw::MeshId;

    fn shell_of(w: &crate::world::World, visible: bool) -> Vec<nkw_gl::draw::Instance> {
        let slots = w.slots();
        let mut l = crate::scene::draw_list(w, &|_| None, &[]);
        l.instances.clear();
        room(
            &mut l,
            &Shell {
                slot: &slots[0],
                index: 0,
                kind: w.room_kind(w.rooms[0].id),
                band: w.band,
                visible,
            },
        );
        l.instances
    }

    #[test]
    fn a_far_room_is_only_its_shell() {
        let w = world(&[&[1]]);
        let far = shell_of(&w, false);
        assert!(far.len() <= 8, "{}", far.len());
        let count = |v: &[nkw_gl::draw::Instance], m| v.iter().filter(|i| i.mesh == m).count();
        assert_eq!(count(&far, MeshId::Ceiling), 1);
        assert_eq!(count(&far, MeshId::WindowPane), 0);
        let near = shell_of(&w, true);
        let n = nkw_gl::shell::window_xs(w.slots()[0].len).len();
        assert_eq!(count(&near, MeshId::WindowPane), n);
        assert!(count(&near, MeshId::Skirting) == 3);
    }

    #[test]
    fn the_hall_is_drawn_once_under_every_door() {
        let w = world(&[&[1], &[2]]);
        let slots = w.slots();
        let mut l = crate::scene::draw_list(&w, &|_| None, &[]);
        l.instances.clear();
        hall(&mut l, &slots, w.band);
        assert!(l.instances.iter().any(|i| i.mesh == MeshId::FloorPlanks));
        let end = slots.iter().map(|s| s.x0 + s.len).fold(0.0, f32::max);
        let reach = l
            .instances
            .iter()
            .map(|i| {
                i.model.transform_point3(glam::Vec3::ZERO).x
                    + i.model.transform_vector3(glam::Vec3::X).length() / 2.0
            })
            .fold(f32::MIN, f32::max);
        assert!(reach >= end, "{reach} < {end}");
    }
}
