//! `inject overlay <json>` (scripts, shots, tests): fixtures for what the
//! other plugins will feed the overlay.
//!
//! - `{"dialog": DialogView | null}`: a test dialogue (used until a cat's
//!   or the vault's `dialog()` exists; theirs wins). `DialogView` as JSON:
//!   `{"name","rgb":[r,g,b],"text","more","choices":[..],"selected","line":{"text","cursor"}|null}`.
//! - `{"toast": {"text","kind":"info"|"error","ms"}}`
//! - `{"pickables": [{"screen":N}|{"app_id":"…"}|{"cat":N}|{"door":N}, "pos":[x,y,z]?]}`:
//!   extra SALTO targets (`[]` clears); a screen's pos defaults to its place.
//! - `{"palette_items": [{"name","title","keys":[..],"source":"world"|"core"|"app"|"goto"|"object"}]}`

use glam::Vec3;
use nkw_core::{CatId, Pick, RoomId, ScreenId, Station};
use serde_json::Value;

use crate::ext::{DialogView, LineView, PaletteItem, Pickable, Source, ToastKind};
use crate::world::World;

fn rgb(v: &Value) -> [u8; 3] {
    let c = |i: usize| v.get(i).and_then(Value::as_u64).unwrap_or(0).min(255) as u8;
    [c(0), c(1), c(2)]
}

fn s(v: &Value, k: &str) -> String {
    v.get(k)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

pub fn dialog(v: &Value) -> Option<DialogView> {
    if !v.is_object() {
        return None;
    }
    Some(DialogView {
        name: s(v, "name"),
        rgb: v.get("rgb").map(rgb).unwrap_or([0x6a, 0x9f, 0xd8]),
        text: s(v, "text"),
        more: v.get("more").and_then(Value::as_bool).unwrap_or(false),
        choices: v
            .get("choices")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|c| c.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default(),
        selected: v.get("selected").and_then(Value::as_u64).unwrap_or(0) as usize,
        line: v.get("line").filter(|l| l.is_object()).map(|l| LineView {
            text: s(l, "text"),
            cursor: l.get("cursor").and_then(Value::as_u64).unwrap_or(0) as usize,
        }),
    })
}

pub fn toast(v: &Value) -> Option<(String, ToastKind, u32)> {
    let text = v.get("text")?.as_str()?.to_string();
    let kind = match v.get("kind").and_then(Value::as_str) {
        Some("error") => ToastKind::Error,
        _ => ToastKind::Info,
    };
    let ms = v.get("ms").and_then(Value::as_u64).unwrap_or(0) as u32;
    Some((text, kind, ms))
}

fn vec3(v: &Value) -> Option<Vec3> {
    let a = v.as_array()?;
    let f = |i: usize| a.get(i).and_then(Value::as_f64).map(|x| x as f32);
    Some(Vec3::new(f(0)?, f(1)?, f(2)?))
}

pub fn pickables(v: &Value, world: &World) -> Vec<Pickable> {
    let Some(a) = v.as_array() else {
        return Vec::new();
    };
    a.iter()
        .filter_map(|p| {
            let id = |k: &str| p.get(k).and_then(Value::as_u64).map(|n| n as u32);
            let pos = p.get("pos").and_then(vec3);
            let by_app = p.get("app_id").and_then(Value::as_str).and_then(|a| {
                world
                    .screens
                    .values()
                    .find(|s| s.app_id == a)
                    .map(|s| s.id.0)
            });
            let (pick, at) = if let Some(n) = id("screen").or(by_app) {
                let (r, d, k) = world.locate(ScreenId(n))?;
                let station = Station::Desk {
                    room: world.rooms[r].id,
                    desk: d,
                    slot: k,
                };
                let at = pos.unwrap_or(world.screen_place(r, d, k).center);
                (
                    Pick::Screen {
                        station,
                        screen: ScreenId(n),
                    },
                    at,
                )
            } else if let Some(n) = id("cat") {
                (Pick::Cat(CatId(n)), pos?)
            } else {
                (Pick::Door(RoomId(id("door")?)), pos?)
            };
            Some(Pickable {
                pick,
                pos: at,
                radius: 0.3,
            })
        })
        .collect()
}

pub fn palette_items(v: &Value) -> Vec<PaletteItem> {
    let Some(a) = v.as_array() else {
        return Vec::new();
    };
    a.iter()
        .filter_map(|i| {
            let name = i.get("name")?.as_str()?.to_string();
            let source = match i.get("source").and_then(Value::as_str) {
                Some("core") => Source::Core,
                Some("app") => Source::App { exec: s(i, "exec") },
                Some("goto") => Source::Goto,
                Some("object") => Source::Object,
                _ => Source::World,
            };
            Some(PaletteItem {
                title: s(i, "title"),
                keys: i
                    .get("keys")
                    .and_then(Value::as_array)
                    .map(|k| {
                        k.iter()
                            .filter_map(|x| x.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default(),
                name,
                source,
                args: Vec::new(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_a_dialog_view() {
        let d = dialog(&json!({
            "name": "api-gateway", "rgb": [10, 20, 300], "text": "Aplicar?",
            "more": true, "choices": ["sim", "não"], "selected": 1,
            "line": {"text": "ok", "cursor": 2}
        }))
        .unwrap();
        assert_eq!(d.name, "api-gateway");
        assert_eq!(d.rgb, [10, 20, 255]);
        assert_eq!(d.choices, ["sim", "não"]);
        assert_eq!(d.selected, 1);
        assert!(d.more);
        assert_eq!(d.line.unwrap().cursor, 2);
        assert_eq!(dialog(&Value::Null), None);
        assert_eq!(dialog(&json!({"text": "oi"})).unwrap().line, None);
    }

    #[test]
    fn parses_toasts_pickables_items() {
        assert_eq!(
            toast(&json!({"text": "x", "kind": "error", "ms": 10})),
            Some(("x".into(), ToastKind::Error, 10))
        );
        assert_eq!(toast(&json!({"kind": "error"})), None);
        let w = crate::render::tests::world(&[&[1, 2]]);
        let p = pickables(
            &json!([{"screen": 2}, {"cat": 5, "pos": [1, 2, 3]}, {"cat": 6}, {"screen": 99}]),
            &w,
        );
        assert_eq!(p.len(), 2);
        assert!(matches!(
            p[0].pick,
            Pick::Screen {
                screen: ScreenId(2),
                ..
            }
        ));
        assert_eq!(p[1].pos, Vec3::new(1.0, 2.0, 3.0));
        let items = palette_items(
            &json!([{"name": "session.stop", "title": "parar", "source": "core"}, {"title": "x"}]),
        );
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].source, Source::Core);
    }
}
