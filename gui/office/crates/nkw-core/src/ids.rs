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
