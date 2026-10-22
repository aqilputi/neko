//! MPRIS2 (`org.mpris.MediaPlayer2.Player`): players are found with `ListNames` plus
//! `NameOwnerChanged`; all their `PropertiesChanged` arrive on one subscription and are matched to the
//! player by sender. With several players, the last one that went Playing wins (waybar's rule).

use std::cell::RefCell;
use std::collections::HashMap;

use futures_util::future::Either;

use super::{
    Cmd, Props, decode_changed, get, get_all, next_of, props_stream, v_f64, v_i64, v_str, v_strs,
};
use crate::{MediaState, Out, SysEvent};

pub(crate) const PREFIX: &str = "org.mpris.MediaPlayer2.";
pub(crate) const PATH: &str = "/org/mpris/MediaPlayer2";
pub(crate) const IFACE: &str = "org.mpris.MediaPlayer2.Player";

#[zbus::proxy(
    interface = "org.mpris.MediaPlayer2.Player",
    default_path = "/org/mpris/MediaPlayer2",
    gen_blocking = false
)]
trait Player {
    fn play_pause(&self) -> zbus::Result<()>;
    fn next(&self) -> zbus::Result<()>;
    fn previous(&self) -> zbus::Result<()>;
    #[zbus(property)]
    fn set_volume(&self, v: f64) -> zbus::Result<()>;
}

/// A player's `Player` properties → the event. Missing keys are tolerated: a fresh player may have
/// no metadata yet.
pub(crate) fn media_from_props(player: &str, p: &Props) -> MediaState {
    let mut m = MediaState {
        playing: get(p, "PlaybackStatus").and_then(v_str) == Some("Playing"),
        volume: get(p, "Volume")
            .and_then(v_f64)
            .unwrap_or(1.0)
            .clamp(0.0, 1.0) as f32,
        position_us: get(p, "Position").and_then(v_i64),
        player: player.to_string(),
        ..MediaState::default()
    };
    if let Some(zbus::zvariant::Value::Dict(meta)) = get(p, "Metadata").map(super::peel) {
        for (k, v) in meta.iter() {
            match v_str(k) {
                Some("xesam:title") => m.title = v_str(v).unwrap_or_default().to_string(),
                Some("xesam:artist") => m.artist = v_strs(v).join(", "),
                Some("mpris:artUrl") => {
                    m.art_url = v_str(v).map(str::to_string).filter(|s| !s.is_empty())
                }
                Some("mpris:length") => m.length_us = v_i64(v),
                _ => {}
            }
        }
    }
    m
}

struct Entry {
    name: String,
    props: Props,
    /// When this player last went Playing (0 = never).
    went_playing: u64,
}

impl Entry {
    fn playing(&self) -> bool {
        get(&self.props, "PlaybackStatus").and_then(v_str) == Some("Playing")
    }
}

/// Every player on the bus and the selection rule.
#[derive(Default)]
pub(crate) struct PlayerSet {
    players: Vec<Entry>,
    seq: u64,
}

impl PlayerSet {
    fn slot(&mut self, name: &str) -> &mut Entry {
        if let Some(i) = self.players.iter().position(|p| p.name == name) {
            return &mut self.players[i];
        }
        self.players.push(Entry {
            name: name.to_string(),
            props: Props::new(),
            went_playing: 0,
        });
        self.players.last_mut().expect("just pushed")
    }

    fn apply(&mut self, name: &str, f: impl FnOnce(&mut Props)) {
        self.seq += 1;
        let seq = self.seq;
        let p = self.slot(name);
        let was = p.playing();
        f(&mut p.props);
        if p.playing() && !was {
            p.went_playing = seq;
        }
    }

    /// A full `GetAll`.
    pub(crate) fn replace(&mut self, name: &str, props: Props) {
        self.apply(name, |p| *p = props);
    }

    /// A `PropertiesChanged`.
    pub(crate) fn merge(&mut self, name: &str, changed: Props) {
        self.apply(name, |p| p.extend(changed));
    }

    pub(crate) fn remove(&mut self, name: &str) {
        self.players.retain(|p| p.name != name);
    }

    pub(crate) fn contains(&self, name: &str) -> bool {
        self.players.iter().any(|p| p.name == name)
    }

    /// The last player that went Playing among those playing now; else the last one that played;
    /// else the first one seen.
    fn active(&self) -> Option<&Entry> {
        let pick = |only_playing: bool| {
            let mut best: Option<&Entry> = None;
            for p in &self.players {
                if only_playing && !p.playing() {
                    continue;
                }
                if best.is_none_or(|b| p.went_playing > b.went_playing) {
                    best = Some(p);
                }
            }
            best
        };
        pick(true).or_else(|| pick(false))
    }

    pub(crate) fn active_name(&self) -> Option<&str> {
        self.active().map(|p| p.name.as_str())
    }

    pub(crate) fn state(&self) -> Option<MediaState> {
        self.active()
            .map(|p| media_from_props(p.name.strip_prefix(PREFIX).unwrap_or(&p.name), &p.props))
    }
}

async fn fetch(conn: &zbus::Connection, name: &str, set: &mut PlayerSet) {
    if let Ok(props) = get_all(conn, name, PATH, IFACE).await {
        set.replace(name, props);
    }
}

/// Runs one command against the active player.
pub(crate) async fn command(conn: &zbus::Connection, name: &str, c: Cmd) -> zbus::Result<()> {
    let p = PlayerProxy::builder(conn)
        .destination(name.to_string())?
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await?;
    match c {
        Cmd::PlayPause => p.play_pause().await,
        Cmd::Next => p.next().await,
        Cmd::Previous => p.previous().await,
        Cmd::Volume(v) => p.set_volume(v).await,
        Cmd::Stop => Ok(()),
    }
}

pub(crate) async fn run(
    conn: &zbus::Connection,
    out: &Out,
    active: &RefCell<Option<String>>,
) -> zbus::Result<()> {
    // Subscribe first, then list, so a player appearing in between is not missed.
    let owners_rule = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .sender("org.freedesktop.DBus")?
        .interface("org.freedesktop.DBus")?
        .member("NameOwnerChanged")?
        .arg0ns("org.mpris.MediaPlayer2")?
        .build();
    let mut owners_changed = zbus::MessageStream::for_match_rule(owners_rule, conn, None).await?;
    let mut changes = props_stream(conn, None, Some(PATH), Some(IFACE)).await?;

    let dbus = zbus::fdo::DBusProxy::new(conn).await?;
    let mut set = PlayerSet::default();
    // unique name → well-known name
    let mut owner_of: HashMap<String, String> = HashMap::new();
    for name in dbus.list_names().await? {
        let name = name.to_string();
        if !name.starts_with(PREFIX) {
            continue;
        }
        if let Ok(owner) = dbus.get_name_owner(name.as_str().try_into()?).await {
            owner_of.insert(owner.to_string(), name.clone());
        }
        fetch(conn, &name, &mut set).await;
    }

    let mut last: Option<MediaState> = None;
    let mut emit = |set: &PlayerSet| {
        *active.borrow_mut() = set.active_name().map(str::to_string);
        let now = set.state();
        if now != last {
            match &now {
                Some(m) => out.send(SysEvent::Media(m.clone())),
                None => out.send(SysEvent::MediaGone),
            }
            last = now;
        }
    };
    if set.active_name().is_some() {
        emit(&set);
    }

    loop {
        match next_of(&mut owners_changed, &mut changes).await {
            Either::Left(Some(Ok(msg))) => {
                let Ok((name, old, new)) = msg.body().deserialize::<(String, String, String)>()
                else {
                    continue;
                };
                if !name.starts_with(PREFIX) {
                    continue;
                }
                if !old.is_empty() {
                    owner_of.remove(&old);
                    set.remove(&name);
                }
                if !new.is_empty() {
                    owner_of.insert(new, name.clone());
                    fetch(conn, &name, &mut set).await;
                }
            }
            Either::Right(Some(Ok(msg))) => {
                let sender = msg.header().sender().map(|s| s.to_string());
                let Some(name) = sender.and_then(|s| owner_of.get(&s).cloned()) else {
                    continue;
                };
                let Some((_, changed, invalidated)) = decode_changed(&msg) else {
                    continue;
                };
                if invalidated.is_empty() && set.contains(&name) {
                    set.merge(&name, changed);
                } else {
                    fetch(conn, &name, &mut set).await;
                }
            }
            Either::Left(None) | Either::Right(None) => return Ok(()),
            _ => continue,
        }
        emit(&set);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dbus::props;
    use std::collections::HashMap;
    use zbus::zvariant::Value;

    fn meta(pairs: Vec<(&'static str, Value<'static>)>) -> Value<'static> {
        let m: HashMap<&str, Value> = pairs.into_iter().collect();
        Value::from(m)
    }

    fn status(s: &'static str) -> Props {
        props(vec![("PlaybackStatus", Value::from(s))])
    }

    #[test]
    fn metadata_to_media_state() {
        let p = props(vec![
            ("PlaybackStatus", Value::from("Playing")),
            ("Volume", Value::F64(0.5)),
            ("Position", Value::I64(1_000_000)),
            (
                "Metadata",
                meta(vec![
                    ("xesam:title", Value::from("Lo-fi for cats")),
                    ("xesam:artist", Value::from(vec!["acme radio", "bravo"])),
                    ("mpris:artUrl", Value::from("file:///tmp/demo.png")),
                    ("mpris:length", Value::I64(180_000_000)),
                    ("xesam:album", Value::from("ignored")),
                ]),
            ),
        ]);
        let m = media_from_props("demo", &p);
        assert_eq!(
            m,
            MediaState {
                playing: true,
                title: "Lo-fi for cats".into(),
                artist: "acme radio, bravo".into(),
                art_url: Some("file:///tmp/demo.png".into()),
                volume: 0.5,
                position_us: Some(1_000_000),
                length_us: Some(180_000_000),
                player: "demo".into(),
            }
        );
    }

    #[test]
    fn missing_keys_are_tolerated() {
        let m = media_from_props("alpha", &Props::new());
        assert_eq!(
            m,
            MediaState {
                volume: 1.0,
                player: "alpha".into(),
                ..MediaState::default()
            }
        );
        // length as u64 (some players), artist as a bare string, empty art url
        let p = props(vec![
            ("PlaybackStatus", Value::from("Paused")),
            (
                "Metadata",
                meta(vec![
                    ("xesam:artist", Value::from("acme")),
                    ("mpris:length", Value::U64(5)),
                    ("mpris:artUrl", Value::from("")),
                ]),
            ),
        ]);
        let m = media_from_props("alpha", &p);
        assert!(!m.playing);
        assert_eq!(
            (m.artist.as_str(), m.length_us, m.art_url),
            ("acme", Some(5), None)
        );
    }

    #[test]
    fn last_playing_wins() {
        let a = "org.mpris.MediaPlayer2.alpha";
        let b = "org.mpris.MediaPlayer2.bravo";
        let mut s = PlayerSet::default();
        assert_eq!(s.active_name(), None);
        s.replace(a, status("Paused"));
        assert_eq!(s.active_name(), Some(a), "only player, never played");
        s.replace(b, status("Paused"));
        assert_eq!(
            s.active_name(),
            Some(a),
            "first seen wins among never-played"
        );
        s.merge(b, status("Playing"));
        assert_eq!(s.active_name(), Some(b));
        s.merge(a, status("Playing"));
        assert_eq!(s.active_name(), Some(a), "a went Playing last");
        s.merge(a, status("Paused"));
        assert_eq!(s.active_name(), Some(b), "b still playing beats paused a");
        s.merge(b, status("Paused"));
        assert_eq!(
            s.active_name(),
            Some(a),
            "nobody playing: the last that played"
        );
        s.merge(b, status("Playing"));
        s.merge(b, status("Playing"));
        assert_eq!(
            s.active_name(),
            Some(b),
            "repeated Playing does not reorder"
        );
        s.remove(b);
        assert_eq!(s.active_name(), Some(a));
        assert_eq!(s.state().map(|m| m.player), Some("alpha".into()));
        s.remove(a);
        assert!(s.state().is_none());
    }
}
