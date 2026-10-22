//! Small typed ids for the things of the world. The `nkw` side maps them to
//! its own handles (Wayland surfaces, core session ids, paths).

use serde::{Deserialize, Serialize};

macro_rules! id {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        pub struct $name(pub u32);
    };
}

id!(
    /// A screen: a window, an agent pane, a tail or a preview (§4.1).
    ScreenId
);
id!(
    /// A cat: one agent session.
    CatId
);
id!(
    /// A room along the hall.
    RoomId
);
id!(
    /// A book (a file) in the library.
    BookId
);
id!(
    /// A shelf (a directory) in the library.
    DirId
);
id!(
    /// A piece of furniture that carries a widget (turntable, clock …).
    ObjectId
);
id!(
    /// A door between two rooms.
    DoorId
);
id!(
    /// A sound source: an app stream or one of our own sounds.
    SourceId
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn object_ids_serialize_as_plain_numbers() {
        assert!(ObjectId(1) < ObjectId(2));
        assert_eq!(serde_json::to_string(&DoorId(7)).unwrap(), "7");
        let s: SourceId = serde_json::from_str("3").unwrap();
        assert_eq!(s, SourceId(3));
    }
}
