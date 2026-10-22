//! Typed ids for the objects of the world: they live in `nkw_core::ids`
//! (addendum §2.4) and are re-exported here.

pub use nkw_core::{DoorId, ObjectId, SourceId};

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
