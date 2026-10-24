//! `inject overlay <json>` (scripts, shots, tests): fixtures for what the
//! other plugins will feed the overlay.
//!
//! - `{"dialog": DialogView | null}`: a test dialogue (used until a cat's
//!   or the vault's `dialog()` exists; theirs wins). `DialogView` as JSON:
//!   `{"name","rgb":[r,g,b],"text","more","choices":[..],"selected","line":{"text","cursor"}|null}`.
//! - `{"toast": {"text","kind":"info"|"error","ms"}}`
//! - `{"pickables": [{"screen":N}|{"app_id":"…"}|{"cat":N}|{"door":N}, "pos":[x,y,z]?]}`:
//!   extra SALTO targets (`[]` clears); a screen's pos defaults to its place.
//!   `{"book":N,"pos":[..]}` too (a library book; the map draws the shelves
//!   from them).
//! - `{"palette_items": [{"name","title","keys":[..],"source":"world"|"core"|"app"|"goto"|"object"}]}`
//! - `{"aimed": bool|null}`, `{"carrying": bool|null}`, `{"vignette": 0..1|null}`:
//!   force ANDAR's crosshair ring, the "na mão" keys and the comfort
//!   vignette over the hub's/World's values (`null` gives them back).
//! - wave 4 (S21): `{"phone": {"app"?, "sub"?} | null}` opens/closes the
//!   phone; `{"phone_page": PhonePage | null}` draws a given page
//!   ([`phone_page`]); `{"prefs": {"contrast", "font_scale", "strip",
//!   "reduced_motion"}}` sets runtime prefs (never saved); `{"los": bool|null}`
//!   forces the tags' line of sight; `{"hud_hidden": bool}`.

use glam::Vec3;
use nkw_core::{BookId, CatId, Pick, RoomId, ScreenId, Station};
use serde_json::Value;

use crate::ext::{DialogView, LineView, PaletteItem, Pickable, Source, ToastKind};
use crate::ui::Icon;
use crate::ui::phone::{Act, Item, PhonePage, SearchSpec};
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
            } else if let Some(n) = id("book") {
                (Pick::Book(BookId(n)), pos?)
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

/// An icon by its name (`"Cat"`, `"Envelope"` …).
pub fn icon(v: &Value) -> Option<Icon> {
    let s = v.as_str()?;
    crate::ui::icon::ALL
        .iter()
        .copied()
        .find(|i| format!("{i:?}").eq_ignore_ascii_case(s))
}

fn act(v: &Value) -> Option<Act> {
    let name = v.get("name")?.as_str()?.to_string();
    let args = v
        .get("args")
        .and_then(Value::as_object)
        .map(|o| {
            o.iter()
                .map(|(k, x)| {
                    let val = x
                        .as_str()
                        .map(String::from)
                        .unwrap_or_else(|| x.to_string());
                    (k.clone(), val)
                })
                .collect()
        })
        .unwrap_or_default();
    Some(Act {
        name,
        args,
        close: v.get("close").and_then(Value::as_bool).unwrap_or(false),
    })
}

fn opt_s(v: &Value, k: &str) -> Option<String> {
    v.get(k).and_then(Value::as_str).map(String::from)
}

fn pairs(v: Option<&Value>) -> Vec<(String, String)> {
    v.and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|p| {
                    let p = p.as_array()?;
                    Some((
                        p.first()?.as_str()?.to_string(),
                        p.get(1)?.as_str()?.to_string(),
                    ))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// `{"phone_page": PhonePage | null}`: a page drawn instead of the hub's
/// (shots of apps whose plugin is not there yet). As JSON: `{"title",
/// "items": [{"id","title","detail","badge","icon","rgb","toggle",
/// "on_enter": {"name","args","close"}, "on_keys": [[key, act]]}], "empty",
/// "search": {"placeholder","live","run"}, "keys": [[key, what]]}`.
pub fn phone_page(v: &Value) -> Option<PhonePage> {
    if !v.is_object() {
        return None;
    }
    let items = v
        .get("items")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .map(|i| Item {
                    id: s(i, "id"),
                    icon: i.get("icon").and_then(icon),
                    rgb: i.get("rgb").map(rgb),
                    title: s(i, "title"),
                    detail: opt_s(i, "detail"),
                    badge: opt_s(i, "badge"),
                    toggle: i.get("toggle").and_then(Value::as_bool),
                    on_enter: i.get("on_enter").and_then(act),
                    on_keys: i
                        .get("on_keys")
                        .and_then(Value::as_array)
                        .map(|ks| {
                            ks.iter()
                                .filter_map(|p| {
                                    let p = p.as_array()?;
                                    Some((p.first()?.as_str()?.to_string(), act(p.get(1)?)?))
                                })
                                .collect()
                        })
                        .unwrap_or_default(),
                })
                .collect()
        })
        .unwrap_or_default();
    Some(PhonePage {
        title: s(v, "title"),
        header: None,
        items,
        empty: opt_s(v, "empty"),
        search: v
            .get("search")
            .filter(|x| x.is_object())
            .map(|x| SearchSpec {
                placeholder: s(x, "placeholder"),
                live: x.get("live").and_then(Value::as_bool).unwrap_or(false),
                run: opt_s(x, "run"),
            }),
        keys: pairs(v.get("keys")),
    })
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
            &json!([{"screen": 2}, {"cat": 5, "pos": [1, 2, 3]}, {"cat": 6}, {"screen": 99}, {"book": 7, "pos": [0, 1, 0]}, {"book": 8}]),
            &w,
        );
        assert_eq!(p.len(), 3);
        assert_eq!(p[2].pick, Pick::Book(BookId(7)));
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
