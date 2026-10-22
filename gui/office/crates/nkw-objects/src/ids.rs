//! Typed ids for the objects of the world (same shape as `nkw_core::ids`;
//! addendum §2.4 moves them there later).

use serde::{Deserialize, Serialize};

macro_rules! id {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        pub struct $name(pub u32);
    };
}

id!(
    /// A piece of furniture that carries a widget (turntable, clock…).
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
    fn ids_order_and_serialize_as_plain_numbers() {
        assert!(ObjectId(1) < ObjectId(2));
        assert_eq!(serde_json::to_string(&DoorId(7)).unwrap(), "7");
        let s: SourceId = serde_json::from_str("3").unwrap();
        assert_eq!(s, SourceId(3));
    }
}
