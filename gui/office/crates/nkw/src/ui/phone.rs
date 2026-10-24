//! The phone API every plugin's apps speak (contract S21). Data only: the
//! overlay owns navigation and drawing.

use std::collections::BTreeMap;

use super::{Icon, Node, Rgb};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PhoneApp {
    pub id: &'static str,
    pub title: &'static str,
    pub icon: Icon,
    pub order: u8,
    pub badge: Option<u32>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Act {
    pub name: String,
    pub args: BTreeMap<String, String>,
    /// close the phone after
    pub close: bool,
}

impl Act {
    /// An act with `args` given as pairs.
    pub fn new(name: &str, args: &[(&str, &str)], close: bool) -> Act {
        Act {
            name: name.to_string(),
            args: args
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            close,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Item {
    pub id: String,
    pub icon: Option<Icon>,
    pub rgb: Option<Rgb>,
    pub title: String,
    pub detail: Option<String>,
    pub badge: Option<String>,
    /// drawn as a switch; `on_enter` flips it
    pub toggle: Option<bool>,
    pub on_enter: Option<Act>,
    /// (key name as the router names it: "t", "Q", "x"; act)
    pub on_keys: Vec<(String, Act)>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct SearchSpec {
    pub placeholder: String,
    /// the overlay filters `items` by fuzzy on `query`
    pub live: bool,
    /// enter in the line → `Req::Command { run, {"q": query} }`
    pub run: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct PhonePage {
    pub title: String,
    pub header: Option<Node>,
    pub items: Vec<Item>,
    pub empty: Option<String>,
    pub search: Option<SearchSpec>,
    /// footer: (key, what)
    pub keys: Vec<(String, String)>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct PhoneState {
    /// None = home grid
    pub app: Option<String>,
    /// app-defined sub-page
    pub sub: Option<String>,
    pub sel: usize,
    pub scroll: usize,
    pub query: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PhoneView {
    pub state: PhoneState,
    pub apps: Vec<PhoneApp>,
    pub page: Option<PhonePage>,
}

/// Acts the overlay handles itself (never dispatched): `"phone.sub" {sub}`
/// opens an app's sub-page (`state.sub`, `sel = 0`), `"phone.app" {app}`
/// switches app, `"phone.back"`, `"phone.close"`.
pub const ACT_SUB: &str = "phone.sub";
pub const ACT_APP: &str = "phone.app";
pub const ACT_BACK: &str = "phone.back";
pub const ACT_CLOSE: &str = "phone.close";
