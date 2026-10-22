//! The world's memory across restarts (§4.4, §10 phase 1): `world.json`.
//!
//! Pure: text or JSON in, plain data out. The file keeps the rooms (by name
//! and kind), the desks and which apps sat on them, the station, and the
//! place memory (`app_id` → where its last window lived). [`apply`] turns a
//! file into a [`Plan`] of what to restore: the rooms that do not exist yet
//! and, per `app_id`, the places its windows go back to as they reappear.
//! Windows themselves are never restored (their processes are gone); their
//! desks come back when they do.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::strip::Width;

/// The current file version.
pub const VERSION: u32 = 1;

/// What a room is for (§6.1).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoomKind {
    /// "recepção": always the first room
    Reception,
    /// one per core context (the context's name)
    Context(String),
    /// "minha sala": always the last room
    User,
    /// the library (phase 4)
    Library,
    /// a room the user made (`world.room_new`)
    Free,
}

impl RoomKind {
    /// The kind a room has from its name alone (rooms made before kinds).
    pub fn from_name(name: &str, reception: &str, user: &str) -> RoomKind {
        if name == reception {
            RoomKind::Reception
        } else if name == user {
            RoomKind::User
        } else {
            RoomKind::Context(name.to_string())
        }
    }

    /// The dump's spelling.
    pub fn label(&self) -> &'static str {
        match self {
            RoomKind::Reception => "reception",
            RoomKind::Context(_) => "context",
            RoomKind::User => "user",
            RoomKind::Library => "library",
            RoomKind::Free => "free",
        }
    }
}

/// Which desk of a room: a new one at the end of the strip (`"last"`) or a
/// new one at that index (clamped to the strip).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "DeskRaw", into = "DeskRaw")]
pub enum DeskAt {
    Last,
    Index(usize),
}

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum DeskRaw {
    Index(usize),
    Name(String),
}

impl TryFrom<DeskRaw> for DeskAt {
    type Error = String;
    fn try_from(r: DeskRaw) -> Result<DeskAt, String> {
        match r {
            DeskRaw::Index(i) => Ok(DeskAt::Index(i)),
            DeskRaw::Name(s) if s == "last" => Ok(DeskAt::Last),
            DeskRaw::Name(s) => Err(format!("desk: \"last\" or a number, not {s:?}")),
        }
    }
}

impl From<DeskAt> for DeskRaw {
    fn from(d: DeskAt) -> DeskRaw {
        match d {
            DeskAt::Last => DeskRaw::Name("last".into()),
            DeskAt::Index(i) => DeskRaw::Index(i),
        }
    }
}

/// Where a window goes. Every field is optional: `None` is "the default"
/// (the current room; a new desk right of the station; `Half`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Place {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub room: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub desk: Option<DeskAt>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<Width>,
}

impl Place {
    pub fn is_empty(&self) -> bool {
        self.room.is_none() && self.desk.is_none() && self.width.is_none()
    }

    /// Field by field: `self` where set, else `other`.
    pub fn or(self, other: Place) -> Place {
        Place {
            room: self.room.or(other.room),
            desk: self.desk.or(other.desk),
            width: self.width.or(other.width),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoomRec {
    pub id: u32,
    pub name: String,
    pub kind: RoomKind,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeskRec {
    /// the room's name
    pub room: String,
    pub width: Width,
    /// the app_ids of its stack, top first
    #[serde(default)]
    pub screens: Vec<String>,
    /// a cat's desk (comes back with the cat, not from the file)
    #[serde(default)]
    pub cat: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScreenRec {
    pub app_id: String,
    pub place: Place,
}

/// The station, by room name (ids change across runs).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StationRec {
    Desk {
        room: String,
        desk: usize,
        slot: usize,
    },
    Door {
        room: String,
    },
}

impl StationRec {
    pub fn room(&self) -> &str {
        match self {
            StationRec::Desk { room, .. } | StationRec::Door { room } => room,
        }
    }
}

/// `world.json`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldFile {
    pub version: u32,
    #[serde(default)]
    pub rooms: Vec<RoomRec>,
    #[serde(default)]
    pub desks: Vec<DeskRec>,
    #[serde(default)]
    pub screens: Vec<ScreenRec>,
    #[serde(default)]
    pub station: Option<StationRec>,
    #[serde(default)]
    pub places: BTreeMap<String, Place>,
}

impl Default for WorldFile {
    fn default() -> WorldFile {
        WorldFile {
            version: VERSION,
            rooms: Vec::new(),
            desks: Vec::new(),
            screens: Vec::new(),
            station: None,
            places: BTreeMap::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PersistError {
    /// not JSON at all
    Json(String),
    /// a version this build does not know (newer)
    Version(u32),
    /// JSON, but not a world file
    Shape(String),
}

impl std::fmt::Display for PersistError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PersistError::Json(e) => write!(f, "world.json: não é JSON: {e}"),
            PersistError::Version(v) => write!(f, "world.json: versão {v} desconhecida"),
            PersistError::Shape(e) => write!(f, "world.json: formato inválido: {e}"),
        }
    }
}

impl std::error::Error for PersistError {}

/// The names of the rooms that always exist, for [`migrate`] of files
/// without kinds.
pub const RECEPTION: &str = "recepção";
pub const USER_ROOM: &str = "minha sala";

/// Read `world.json`'s text.
pub fn parse(text: &str) -> Result<WorldFile, PersistError> {
    let v: Value = serde_json::from_str(text).map_err(|e| PersistError::Json(e.to_string()))?;
    migrate(v)
}

/// The file's text (pretty, stable key order).
pub fn to_string(file: &WorldFile) -> String {
    let mut s = serde_json::to_string_pretty(file).expect("a world file serialises");
    s.push('\n');
    s
}

/// Any known version to the current one. Version 0 (phase 0's sketch, no
/// `version` key or `"version": 0`) had `"rooms": ["name", …]` and
/// `"places": {app_id: [room, desk]}`.
pub fn migrate(v: Value) -> Result<WorldFile, PersistError> {
    let Value::Object(map) = &v else {
        return Err(PersistError::Shape("not an object".into()));
    };
    let version = match map.get("version") {
        None => 0,
        Some(n) => n
            .as_u64()
            .and_then(|n| u32::try_from(n).ok())
            .ok_or_else(|| PersistError::Shape("version is not a number".into()))?,
    };
    match version {
        0 => migrate_v0(map),
        VERSION => serde_json::from_value(v).map_err(|e| PersistError::Shape(e.to_string())),
        v => Err(PersistError::Version(v)),
    }
}

fn migrate_v0(map: &serde_json::Map<String, Value>) -> Result<WorldFile, PersistError> {
    let bad = |what: &str| PersistError::Shape(format!("version 0: {what}"));
    let mut file = WorldFile::default();
    if let Some(rooms) = map.get("rooms") {
        let rooms = rooms.as_array().ok_or_else(|| bad("rooms"))?;
        for (i, r) in rooms.iter().enumerate() {
            let name = r.as_str().ok_or_else(|| bad("room name"))?;
            file.rooms.push(RoomRec {
                id: i as u32,
                name: name.into(),
                kind: RoomKind::from_name(name, RECEPTION, USER_ROOM),
            });
        }
    }
    if let Some(places) = map.get("places") {
        let places = places.as_object().ok_or_else(|| bad("places"))?;
        for (app, p) in places {
            let pair = p
                .as_array()
                .filter(|a| a.len() == 2)
                .ok_or_else(|| bad("place"))?;
            let room = pair[0].as_str().ok_or_else(|| bad("place room"))?;
            let desk = pair[1].as_u64().ok_or_else(|| bad("place desk"))? as usize;
            file.places.insert(
                app.clone(),
                Place {
                    room: Some(room.into()),
                    desk: Some(DeskAt::Index(desk)),
                    width: None,
                },
            );
        }
    }
    Ok(file)
}

/// What to restore from a file.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Plan {
    /// rooms to make, in the file's order (the ones not already there)
    pub rooms: Vec<(String, RoomKind)>,
    /// per app_id, where its windows go back to, in order (one per window)
    pub queue: BTreeMap<String, Vec<Place>>,
    /// the place memory
    pub places: BTreeMap<String, Place>,
    pub station: Option<StationRec>,
}

impl Plan {
    /// The next restored place of a window of `app_id` (taken).
    pub fn take(&mut self, app_id: &str) -> Option<Place> {
        let q = self.queue.get_mut(app_id)?;
        let p = (!q.is_empty()).then(|| q.remove(0));
        if q.is_empty() {
            self.queue.remove(app_id);
        }
        p
    }
}

/// The plan of `file` for a world that already has the rooms `live`.
/// The reception and "minha sala" always exist and are never made; cat
/// desks are the core's and are not queued.
pub fn apply(file: &WorldFile, live: &[String]) -> Plan {
    let rooms = file
        .rooms
        .iter()
        .filter(|r| !matches!(r.kind, RoomKind::Reception | RoomKind::User))
        .filter(|r| !live.contains(&r.name))
        .map(|r| (r.name.clone(), r.kind.clone()))
        .collect();
    let mut queue: BTreeMap<String, Vec<Place>> = BTreeMap::new();
    for s in &file.screens {
        if s.app_id.is_empty() {
            continue;
        }
        queue
            .entry(s.app_id.clone())
            .or_default()
            .push(s.place.clone());
    }
    Plan {
        rooms,
        queue,
        places: file.places.clone(),
        station: file.station.clone(),
    }
}

/// The place memory after a window of `app_id` was at `place`.
pub fn remember(places: &mut BTreeMap<String, Place>, app_id: &str, place: Place) {
    if app_id.is_empty() || place.is_empty() {
        return;
    }
    places.insert(app_id.to_string(), place);
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample() -> WorldFile {
        WorldFile {
            version: VERSION,
            rooms: vec![
                RoomRec {
                    id: 0,
                    name: RECEPTION.into(),
                    kind: RoomKind::Reception,
                },
                RoomRec {
                    id: 3,
                    name: "acme".into(),
                    kind: RoomKind::Context("acme".into()),
                },
                RoomRec {
                    id: 4,
                    name: "bravo".into(),
                    kind: RoomKind::Free,
                },
                RoomRec {
                    id: 1,
                    name: USER_ROOM.into(),
                    kind: RoomKind::User,
                },
            ],
            desks: vec![DeskRec {
                room: "acme".into(),
                width: Width::TwoThirds,
                screens: vec!["acme.term".into()],
                cat: false,
            }],
            screens: vec![
                ScreenRec {
                    app_id: "acme.term".into(),
                    place: Place {
                        room: Some("acme".into()),
                        desk: Some(DeskAt::Index(1)),
                        width: Some(Width::TwoThirds),
                    },
                },
                ScreenRec {
                    app_id: "acme.term".into(),
                    place: Place {
                        room: Some("bravo".into()),
                        desk: Some(DeskAt::Index(0)),
                        width: None,
                    },
                },
            ],
            station: Some(StationRec::Desk {
                room: "acme".into(),
                desk: 1,
                slot: 0,
            }),
            places: BTreeMap::from([(
                "demo.viewer".to_string(),
                Place {
                    room: Some("bravo".into()),
                    desk: Some(DeskAt::Last),
                    width: Some(Width::Third),
                },
            )]),
        }
    }

    #[test]
    fn round_trip() {
        let f = sample();
        let text = to_string(&f);
        assert_eq!(parse(&text), Ok(f));
        assert!(text.ends_with('\n'));
        let v: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(v["version"], 1);
        assert_eq!(v["rooms"][1]["kind"], json!({"context": "acme"}));
        assert_eq!(v["rooms"][2]["kind"], "free");
        assert_eq!(v["places"]["demo.viewer"]["desk"], "last");
        assert_eq!(v["screens"][0]["place"]["desk"], 1);
        assert_eq!(v["station"]["desk"]["room"], "acme");
    }

    #[test]
    fn migrates_version_0() {
        for v in [
            json!({"rooms": ["recepção", "acme", "minha sala"], "places": {"acme.term": ["acme", 2]}}),
            json!({"version": 0, "rooms": ["recepção", "acme", "minha sala"], "places": {"acme.term": ["acme", 2]}}),
        ] {
            let f = migrate(v).unwrap();
            assert_eq!(f.version, VERSION);
            let kinds: Vec<_> = f.rooms.iter().map(|r| r.kind.clone()).collect();
            assert_eq!(
                kinds,
                [
                    RoomKind::Reception,
                    RoomKind::Context("acme".into()),
                    RoomKind::User
                ]
            );
            assert_eq!(
                f.places["acme.term"],
                Place {
                    room: Some("acme".into()),
                    desk: Some(DeskAt::Index(2)),
                    width: None
                }
            );
        }
        assert_eq!(migrate(json!({})).unwrap(), WorldFile::default());
    }

    #[test]
    fn corrupted_is_an_error() {
        assert!(matches!(
            parse("{\"rooms\": [1,"),
            Err(PersistError::Json(_))
        ));
        assert!(matches!(parse(""), Err(PersistError::Json(_))));
        assert!(matches!(parse("[]"), Err(PersistError::Shape(_))));
        assert!(matches!(
            parse("{\"version\": \"x\"}"),
            Err(PersistError::Shape(_))
        ));
        assert!(matches!(
            parse("{\"version\": 0, \"rooms\": [3]}"),
            Err(PersistError::Shape(_))
        ));
        assert!(matches!(
            parse("{\"version\": 0, \"places\": {\"a\": [\"r\"]}}"),
            Err(PersistError::Shape(_))
        ));
        assert!(matches!(
            parse("{\"version\": 1, \"rooms\": 7}"),
            Err(PersistError::Shape(_))
        ));
        assert_eq!(parse("{\"version\": 9}"), Err(PersistError::Version(9)));
        assert!(matches!(
            parse("{\"version\": 1, \"places\": {\"a\": {\"desk\": \"first\"}}}"),
            Err(PersistError::Shape(_))
        ));
        let e = parse("{\"version\": 9}").unwrap_err().to_string();
        assert!(e.contains("versão 9"), "{e}");
    }

    #[test]
    fn the_plan_skips_live_and_fixed_rooms() {
        let f = sample();
        let mut p = apply(&f, &[RECEPTION.into(), USER_ROOM.into(), "acme".into()]);
        assert_eq!(p.rooms, [("bravo".to_string(), RoomKind::Free)]);
        let p2 = apply(&f, &[USER_ROOM.into()]);
        assert_eq!(p2.rooms.len(), 2, "acme and bravo; never the reception");
        assert_eq!(p.station.as_ref().map(|s| s.room()), Some("acme"));
        // two windows of the same app come back in order
        assert_eq!(p.take("acme.term").unwrap().room.as_deref(), Some("acme"));
        assert_eq!(p.take("acme.term").unwrap().room.as_deref(), Some("bravo"));
        assert_eq!(p.take("acme.term"), None);
        assert!(p.queue.is_empty());
        assert_eq!(p.take("nobody"), None);
        assert_eq!(p.places["demo.viewer"].width, Some(Width::Third));
    }

    #[test]
    fn place_memory_updates() {
        let mut m = BTreeMap::new();
        let at = |r: &str, d| Place {
            room: Some(r.into()),
            desk: Some(DeskAt::Index(d)),
            width: None,
        };
        remember(&mut m, "acme.term", at("acme", 0));
        remember(&mut m, "acme.term", at("bravo", 2));
        remember(&mut m, "", at("acme", 1));
        remember(&mut m, "demo", Place::default());
        assert_eq!(m.len(), 1);
        assert_eq!(m["acme.term"], at("bravo", 2));
    }

    #[test]
    fn places_merge_field_by_field() {
        let a = Place {
            room: Some("acme".into()),
            ..Place::default()
        };
        let b = Place {
            room: Some("bravo".into()),
            desk: Some(DeskAt::Last),
            width: Some(Width::Full),
        };
        let m = a.or(b);
        assert_eq!(m.room.as_deref(), Some("acme"));
        assert_eq!(m.desk, Some(DeskAt::Last));
        assert_eq!(m.width, Some(Width::Full));
        assert!(Place::default().is_empty());
        assert!(!m.is_empty());
    }

    #[test]
    fn kinds_from_names_and_labels() {
        assert_eq!(
            RoomKind::from_name(RECEPTION, RECEPTION, USER_ROOM),
            RoomKind::Reception
        );
        assert_eq!(
            RoomKind::from_name(USER_ROOM, RECEPTION, USER_ROOM),
            RoomKind::User
        );
        assert_eq!(
            RoomKind::from_name("alpha", RECEPTION, USER_ROOM),
            RoomKind::Context("alpha".into())
        );
        let labels: Vec<_> = [
            RoomKind::Reception,
            RoomKind::Context("x".into()),
            RoomKind::User,
            RoomKind::Library,
            RoomKind::Free,
        ]
        .iter()
        .map(|k| k.label())
        .collect();
        assert_eq!(labels, ["reception", "context", "user", "library", "free"]);
    }
}
