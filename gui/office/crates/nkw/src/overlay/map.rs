//! The map (§3.10): a top-down sheet of the current room (`o`) or of the
//! whole floor (`o o`), AC-style. Not a mode: a sub-view of NAV the router
//! consults (`submode() == Some("map")`). `h/j/k/l` move a highlight over
//! the desks (or the rooms), `enter` goes there, `esc` closes.

use glam::{Vec2, Vec3};
use nkw_core::layout::{CHAIR_FROM_WALL, DESK_DEPTH, DESK_FROM_WALL, SCREEN_FROM_WALL};
use nkw_core::strip::{self, RoomSlot, Side};
use nkw_core::{Dir, Pose, layout};

use super::draw::{MapBox, MapKind, Spec};
use crate::world::{HALL_W, ROOM_DEPTH, World};

/// The map's state while it is open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MapState {
    /// the whole floor, not just the room
    pub floor: bool,
    /// the highlighted target
    pub sel: usize,
}

/// Something the highlight can rest on.
#[derive(Clone, Debug, PartialEq)]
pub struct Target {
    /// logical px on the output
    pub rect: (i32, i32, i32, i32),
    /// where `enter` glides to
    pub go: Pose,
    pub label: String,
}

/// The sheet and its targets for an output of `size` logical px.
pub fn layout(world: &World, floor: bool, size: Vec2) -> (Spec, Vec<Target>) {
    layout_with(world, floor, size, &[])
}

/// The same, with the library's books (`Pick::Book` targets' points): the room
/// sheet draws the shelves they stand on as thin boxes along the walls.
pub fn layout_with(world: &World, floor: bool, size: Vec2, books: &[Vec3]) -> (Spec, Vec<Target>) {
    let (w, h) = (size.x as i32, size.y as i32);
    let mut boxes = Vec::new();
    let mut targets = Vec::new();
    let title;
    if floor {
        title = "mapa — o andar".to_string();
        floor_sheet(world, size, &mut boxes, &mut targets);
    } else {
        let r = world.current_room();
        title = format!("mapa — {}", world.rooms[r].name);
        room_sheet(world, r, size, &mut boxes, &mut targets);
        shelves(world, r, size, books, &mut boxes);
    }
    let help = if floor {
        "h j k l escolhe · enter vai · o a sala · esc volta"
    } else {
        "h l escolhe · enter vai · o o andar inteiro · esc volta"
    };
    (
        Spec::Map {
            w,
            h,
            title,
            help: help.into(),
            boxes,
        },
        targets,
    )
}

fn label_of(world: &World, r: usize, d: usize) -> String {
    let desk = &world.rooms[r].desks[d];
    if let Some(s) = desk.screens.first().and_then(|s| world.screens.get(s)) {
        let t = if s.app_id.is_empty() {
            &s.title
        } else {
            &s.app_id
        };
        return t.clone();
    }
    match desk.cat {
        Some(c) => format!("gato {}", c.0),
        None => format!("mesa {}", d + 1),
    }
}

/// One room, top-down: back wall at the top, the desks along it, the cat
/// on its chair, the screens on the desk.
fn room_sheet(
    world: &World,
    r: usize,
    size: Vec2,
    boxes: &mut Vec<MapBox>,
    targets: &mut Vec<Target>,
) {
    let slot = world.slots()[r];
    let (x0, y0, k) = room_px(&slot, size);
    let rw = slot.len * k;
    let px = |v: f32| v.round() as i32;
    boxes.push(MapBox {
        x: px(x0),
        y: px(y0),
        w: px(rw),
        h: px(ROOM_DEPTH * k),
        kind: MapKind::Room,
        label: world.rooms[r].name.clone(),
    });
    let usable = world.usable();
    let id = world.rooms[r].id;
    for (d, desk) in world.rooms[r].desks.iter().enumerate() {
        let cx = world.desk_x(r, d);
        let dw = strip::desk_width_m(desk.width, usable);
        let rect = (
            px(x0 + (cx - dw / 2.0) * k),
            px(y0 + DESK_FROM_WALL * k),
            px(dw * k),
            px(DESK_DEPTH * k),
        );
        boxes.push(MapBox {
            x: rect.0,
            y: rect.1,
            w: rect.2,
            h: rect.3,
            kind: MapKind::Desk,
            label: String::new(),
        });
        let n = desk.screens.len();
        for i in 0..n {
            let sw = dw * 0.8 / n as f32;
            let sx = cx - dw * 0.4 + sw * i as f32;
            boxes.push(MapBox {
                x: px(x0 + sx * k) + 1,
                y: px(y0 + SCREEN_FROM_WALL * k) - 2,
                w: px(sw * k) - 2,
                h: 4,
                kind: MapKind::Screen,
                label: String::new(),
            });
        }
        let cat_y = y0 + CHAIR_FROM_WALL * k;
        let cat_r = (0.3 * k).max(8.0);
        if desk.cat.is_some() {
            boxes.push(MapBox {
                x: px(x0 + cx * k - cat_r / 2.0),
                y: px(cat_y - cat_r / 2.0),
                w: px(cat_r),
                h: px(cat_r),
                kind: MapKind::Cat,
                label: String::new(),
            });
        }
        // the label under the chair
        boxes.push(MapBox {
            x: rect.0,
            y: px(cat_y + cat_r / 2.0),
            w: rect.2,
            h: 0,
            kind: MapKind::Screen,
            label: label_of(world, r, d),
        });
        let go = world
            .desk_pose(id, d)
            .unwrap_or_else(|| layout::desk_station(&slot, cx));
        let pad = 6;
        targets.push(Target {
            rect: (
                rect.0 - pad,
                rect.1 - pad,
                rect.2 + 2 * pad,
                (cat_y + cat_r) as i32 - rect.1 + 2 * pad,
            ),
            go,
            label: label_of(world, r, d),
        });
    }
}

/// The room sheet's frame: its top-left (logical px) and px per metre.
fn room_px(slot: &RoomSlot, size: Vec2) -> (f32, f32, f32) {
    let (mx, top, bottom) = (80.0, 90.0, 90.0);
    let k = ((size.x - 2.0 * mx) / slot.len).min((size.y - top - bottom) / ROOM_DEPTH);
    ((size.x - slot.len * k) / 2.0, top, k)
}

/// A world point in a room's local frame: x along the back wall from its
/// left end, z out from the back wall (`layout::to_world`'s inverse).
pub fn to_local(slot: &RoomSlot, p: Vec3) -> Vec2 {
    let f = layout::room_frame(slot);
    let pose = Pose::new(Vec3::ZERO, f.yaw, 0.0);
    let d = p - f.origin;
    Vec2::new(d.dot(pose.right()), -d.dot(pose.forward()))
}

/// How deep a shelf is drawn on the map, metres.
const SHELF_DEPTH: f32 = 0.3;
/// Off the room's outline, logical px.
const INSET: f32 = 3.0;

/// The shelves of room `r` from the books on them: each book goes to its
/// nearest wall (back, left, right); a wall's books make one thin box
/// spanning them.
fn shelves(world: &World, r: usize, size: Vec2, books: &[Vec3], boxes: &mut Vec<MapBox>) {
    let slot = world.slots()[r];
    let (x0, y0, k) = room_px(&slot, size);
    // per wall: (min, max) along it
    let mut walls: [Option<(f32, f32)>; 3] = [None; 3];
    for p in books {
        let l = to_local(&slot, *p);
        if l.x < -0.05 || l.x > slot.len + 0.05 || l.y < -0.05 || l.y > ROOM_DEPTH + 0.05 {
            continue;
        }
        let d = [l.y, l.x, slot.len - l.x];
        let (wall, along) = match (0..3).min_by(|a, b| d[*a].total_cmp(&d[*b])) {
            Some(0) | None => (0, l.x),
            Some(w) => (w, l.y),
        };
        let e = walls[wall].get_or_insert((along, along));
        e.0 = e.0.min(along);
        e.1 = e.1.max(along);
    }
    let px = |v: f32| v.round() as i32;
    let pad = 0.15;
    for (wall, span) in walls.iter().enumerate() {
        let Some((a, b)) = *span else { continue };
        let (a, b) = (a - pad, b + pad);
        let (x, y, w, h) = match wall {
            0 => (x0 + a * k, y0 + INSET, (b - a) * k, SHELF_DEPTH * k),
            1 => (x0 + INSET, y0 + a * k, SHELF_DEPTH * k, (b - a) * k),
            _ => (
                x0 + (slot.len - SHELF_DEPTH) * k - INSET,
                y0 + a * k,
                SHELF_DEPTH * k,
                (b - a) * k,
            ),
        };
        boxes.push(MapBox {
            x: px(x),
            y: px(y),
            w: px(w).max(3),
            h: px(h).max(3),
            kind: MapKind::Shelf,
            label: String::new(),
        });
    }
}

/// The floor: the hall across the middle, the rooms on both sides.
fn floor_sheet(world: &World, size: Vec2, boxes: &mut Vec<MapBox>, targets: &mut Vec<Target>) {
    let slots = world.slots();
    let end = slots
        .iter()
        .map(|s| s.x0 + s.len)
        .fold(0.0, f32::max)
        .max(1.0);
    let (mx, top, bottom) = (60.0, 90.0, 90.0);
    let depth = 2.0 * ROOM_DEPTH + HALL_W;
    let k = ((size.x - 2.0 * mx) / end).min((size.y - top - bottom) / depth);
    let x0 = (size.x - end * k) / 2.0;
    let mid = top + (size.y - top - bottom) / 2.0;
    let px = |v: f32| v.round() as i32;
    let hall_h = HALL_W * k;
    boxes.push(MapBox {
        x: px(x0),
        y: px(mid - hall_h / 2.0),
        w: px(end * k),
        h: px(hall_h),
        kind: MapKind::Hall,
        label: String::new(),
    });
    let here = world.current_room();
    for (i, s) in slots.iter().enumerate() {
        let y = match s.side {
            Side::North => mid - hall_h / 2.0 - ROOM_DEPTH * k,
            Side::South => mid + hall_h / 2.0,
        };
        let rect = (px(x0 + s.x0 * k), px(y), px(s.len * k), px(ROOM_DEPTH * k));
        let room = &world.rooms[i];
        let cats = room.desks.iter().filter(|d| d.cat.is_some()).count();
        let screens: usize = room.desks.iter().map(|d| d.screens.len()).sum();
        boxes.push(MapBox {
            x: rect.0,
            y: rect.1,
            w: rect.2,
            h: rect.3,
            kind: if i == here {
                MapKind::Here
            } else {
                MapKind::Room
            },
            label: room.name.clone(),
        });
        // a dot per cat, a bar per screen, in a row inside the room
        let mut cx = rect.0 + 10;
        let cy = rect.1 + rect.3 / 2;
        for _ in 0..cats.min(8) {
            boxes.push(MapBox {
                x: cx,
                y: cy - 6,
                w: 12,
                h: 12,
                kind: MapKind::Cat,
                label: String::new(),
            });
            cx += 16;
        }
        for _ in 0..screens.min(8) {
            boxes.push(MapBox {
                x: cx,
                y: cy + 10,
                w: 12,
                h: 4,
                kind: MapKind::Screen,
                label: String::new(),
            });
            cx += 16;
        }
        let go = world
            .desk_pose(room.id, 0)
            .unwrap_or_else(|| layout::room_entry(s));
        targets.push(Target {
            rect: (rect.0 - 4, rect.1 - 4, rect.2 + 8, rect.3 + 8),
            go,
            label: room.name.clone(),
        });
    }
}

/// The target the highlight moves to from `sel` going `dir`: the nearest
/// one whose centre lies that way (ties: the lower index); none: stay.
pub fn step(targets: &[Target], sel: usize, dir: Dir) -> usize {
    let centre = |t: &Target| {
        Vec2::new(
            t.rect.0 as f32 + t.rect.2 as f32 / 2.0,
            t.rect.1 as f32 + t.rect.3 as f32 / 2.0,
        )
    };
    let Some(from) = targets.get(sel).map(centre) else {
        return 0;
    };
    let want = match dir {
        Dir::Left => Vec2::NEG_X,
        Dir::Right => Vec2::X,
        Dir::Up => Vec2::NEG_Y,
        Dir::Down => Vec2::Y,
    };
    targets
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != sel)
        .filter_map(|(i, t)| {
            let d = centre(t) - from;
            let along = d.dot(want);
            (along > 0.5).then(|| (i, along + 2.0 * (d - want * along).length()))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)))
        .map_or(sel, |t| t.0)
}

/// Where the map starts highlighted: the station's desk or room.
pub fn initial(world: &World, floor: bool, targets: &[Target]) -> usize {
    if floor {
        return world.current_room().min(targets.len().saturating_sub(1));
    }
    let cam = world.camera.pos;
    let r = world.current_room();
    (0..world.rooms[r].desks.len())
        .min_by(|a, b| {
            let d = |i: usize| {
                world
                    .desk_pose(world.rooms[r].id, i)
                    .map_or(f32::MAX, |p| p.pos.distance(cam))
            };
            d(*a).total_cmp(&d(*b))
        })
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn world() -> World {
        crate::render::tests::world(&[&[1, 2, 3], &[4]])
    }

    #[test]
    fn room_sheet_has_one_target_per_desk() {
        let w = world();
        let (spec, t) = layout(&w, false, Vec2::new(1280.0, 720.0));
        assert_eq!(t.len(), 3);
        let Spec::Map { boxes, title, .. } = spec else {
            panic!()
        };
        assert!(title.contains("r0"));
        assert_eq!(boxes.iter().filter(|b| b.kind == MapKind::Desk).count(), 3);
        assert_eq!(boxes.iter().filter(|b| b.kind == MapKind::Room).count(), 1);
        // desks left to right, inside the output, apart
        assert!(
            t.windows(2)
                .all(|p| p[0].rect.0 + p[0].rect.2 <= p[1].rect.0 + 12)
        );
        for x in &t {
            assert!(x.rect.0 >= 0 && x.rect.0 + x.rect.2 <= 1280);
            assert!(x.rect.2 > 0 && x.rect.3 > 0);
            assert_eq!(
                Some(x.go),
                w.desk_pose(w.rooms[0].id, t.iter().position(|y| y == x).unwrap())
            );
        }
    }

    /// Books against the back and the left wall → two thin shelf boxes
    /// there; a book outside the room is not drawn; no books, no shelves.
    #[test]
    fn library_shelves_from_books() {
        let w = world();
        let size = Vec2::new(1280.0, 720.0);
        let slot = w.slots()[w.current_room()];
        let at = |x: f32, z: f32| layout::to_world(&slot, Vec3::new(x, 1.0, z));
        for (x, z) in [(1.0, 0.1), (2.5, 0.1), (3.0, 0.15)] {
            let l = to_local(&slot, at(x, z));
            assert!((l.x - x).abs() < 1e-4 && (l.y - z).abs() < 1e-4, "{l}");
        }
        let books = [
            at(1.0, 0.1),
            at(2.0, 0.12),
            at(3.0, 0.1),
            at(0.1, 2.0),
            at(0.12, 3.0),
            at(slot.len + 5.0, 1.0),
        ];
        let shelf = |b: &[Vec3]| -> Vec<MapBox> {
            let Spec::Map { boxes, .. } = layout_with(&w, false, size, b).0 else {
                panic!()
            };
            boxes
                .into_iter()
                .filter(|b| b.kind == MapKind::Shelf)
                .collect()
        };
        let s = shelf(&books);
        assert_eq!(s.len(), 2, "{s:?}");
        let (x0, y0, k) = room_px(&slot, size);
        let back = &s[0];
        assert_eq!(back.y, (y0 + INSET).round() as i32, "against the back wall");
        assert!(back.w > back.h, "along it");
        assert_eq!(back.x, (x0 + 0.85 * k).round() as i32);
        assert_eq!(back.w, ((2.3) * k).round() as i32);
        let left = &s[1];
        assert_eq!(left.x, (x0 + INSET).round() as i32);
        assert!(left.h > left.w);
        assert!(shelf(&[]).is_empty());
        // the floor sheet has no shelves; the targets do not change
        let Spec::Map { boxes, .. } = layout_with(&w, true, size, &books).0 else {
            panic!()
        };
        assert!(boxes.iter().all(|b| b.kind != MapKind::Shelf));
        assert_eq!(
            layout_with(&w, false, size, &books).1,
            layout(&w, false, size).1
        );
    }

    #[test]
    fn floor_sheet_has_one_target_per_room() {
        let w = world();
        let (spec, t) = layout(&w, true, Vec2::new(1280.0, 720.0));
        assert_eq!(t.len(), w.rooms.len());
        let Spec::Map { boxes, .. } = spec else {
            panic!()
        };
        assert_eq!(boxes.iter().filter(|b| b.kind == MapKind::Here).count(), 1);
        assert_eq!(boxes.iter().filter(|b| b.kind == MapKind::Hall).count(), 1);
        // north above the hall, south below
        assert!(t[0].rect.1 < t[1].rect.1);
    }

    #[test]
    fn the_highlight_moves_spatially() {
        let w = world();
        let (_, t) = layout(&w, false, Vec2::new(1280.0, 720.0));
        assert_eq!(step(&t, 0, Dir::Right), 1);
        assert_eq!(step(&t, 1, Dir::Right), 2);
        assert_eq!(step(&t, 2, Dir::Right), 2, "stays at the end");
        assert_eq!(step(&t, 2, Dir::Left), 1);
        assert_eq!(step(&t, 0, Dir::Up), 0, "nothing above");
        assert_eq!(step(&[], 0, Dir::Up), 0);
        let (_, f) = layout(&w, true, Vec2::new(1280.0, 720.0));
        assert_eq!(step(&f, 0, Dir::Down), 1, "across the hall");
        assert_eq!(step(&f, 1, Dir::Up), 0);
    }

    #[test]
    fn starts_on_the_stations_desk() {
        let w = world();
        let (_, t) = layout(&w, false, Vec2::new(1280.0, 720.0));
        let i = initial(&w, false, &t);
        assert_eq!(Some(t[i].go.pos), w.station_pose(&w.station).map(|p| p.pos));
        let (_, f) = layout(&w, true, Vec2::new(1280.0, 720.0));
        assert_eq!(initial(&w, true, &f), 0);
    }
}
