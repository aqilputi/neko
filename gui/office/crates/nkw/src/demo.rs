//! `--demo`: a fixture world (3 rooms along the hall, 5 desks, 2 cats), no
//! core connection, nothing read from `$HOME`.

use nkw_core::strip::{Desk, Width};
use nkw_core::{CatId, Output, RoomId};

use crate::world::{Opts, Room, World};

pub fn rooms() -> Vec<Room> {
    let desk = |cat: Option<u32>| Desk {
        width: Width::Half,
        screens: Vec::new(),
        cat: cat.map(CatId),
    };
    vec![
        Room {
            id: RoomId(0),
            name: "recepção".into(),
            desks: vec![desk(Some(1))],
        },
        Room {
            id: RoomId(1),
            name: "acme".into(),
            desks: vec![desk(Some(2)), desk(None), desk(None)],
        },
        Room {
            id: RoomId(2),
            name: "demo".into(),
            desks: vec![desk(None)],
        },
    ]
}

pub fn world(out: Output, opts: Opts) -> World {
    World::with_rooms(out, opts, rooms())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fixture() {
        let r = rooms();
        assert_eq!(r.len(), 3);
        assert_eq!(r.iter().map(|r| r.desks.len()).sum::<usize>(), 5);
        let cats = r
            .iter()
            .flat_map(|r| &r.desks)
            .filter(|d| d.cat.is_some())
            .count();
        assert_eq!(cats, 2);
        let out = Output {
            width_px: 1280,
            height_px: 720,
            scale: 1.0,
        };
        let w = world(
            out,
            Opts {
                reduced_motion: false,
            },
        );
        // rooms alternate sides of the hall
        let s = w.slots();
        assert_ne!(s[0].side, s[1].side);
        assert_eq!(s[0].side, s[2].side);
    }
}
