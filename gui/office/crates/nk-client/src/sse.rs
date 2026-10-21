//! Server-sent events: the line parser and the mapping of its frames to
//! [`Event`]s.

use crate::proto::WireEvent;

/// The state of the events connection, reported as [`Event::Status`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnStatus {
    /// (Re)connecting.
    Connecting,
    /// The stream is open.
    Connected,
    /// The stream ended or could not open; it retries by itself.
    Disconnected(String),
}

/// What `Core::events()` delivers.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// An event of the core's bus (`snapshot.changed`, `session.state`,
    /// `session.tail`, `confirm.requested`…), with its JSON payload.
    Core { seq: u64, name: String, data: serde_json::Value },
    /// Events may have been lost (a `gap`, or a fresh connection that could
    /// not resume): re-read the snapshot (and whatever else is cached).
    Resync,
    /// The connection changed state.
    Status(ConnStatus),
}

/// One dispatched SSE message.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SseFrame {
    pub id: Option<String>,
    pub event: String,
    pub data: String,
}

/// Incremental SSE parser (WHATWG event-stream rules): feed it lines without
/// their line terminator.
#[derive(Debug, Default)]
pub struct SseParser {
    id: Option<String>,
    event: String,
    data: String,
    has_data: bool,
    /// The last event id seen (what to resume from with Last-Event-ID).
    pub last_id: Option<String>,
    /// The reconnection delay the server asked for, in ms.
    pub retry: Option<u64>,
}

impl SseParser {
    pub fn new() -> SseParser {
        SseParser::default()
    }

    /// Feeds one line; returns a frame when the line ends one.
    pub fn line(&mut self, line: &str) -> Option<SseFrame> {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if line.is_empty() {
            return self.dispatch();
        }
        if line.starts_with(':') {
            return None; // comment (": ok", ": ping")
        }
        let (field, value) = match line.split_once(':') {
            Some((f, v)) => (f, v.strip_prefix(' ').unwrap_or(v)),
            None => (line, ""),
        };
        match field {
            "event" => self.event = value.to_string(),
            "data" => {
                if self.has_data {
                    self.data.push('\n');
                }
                self.data.push_str(value);
                self.has_data = true;
            }
            "id" if !value.contains('\0') => {
                self.id = Some(value.to_string());
                self.last_id = Some(value.to_string());
            }
            "retry" => {
                if let Ok(ms) = value.parse() {
                    self.retry = Some(ms);
                }
            }
            _ => {}
        }
        None
    }

    fn dispatch(&mut self) -> Option<SseFrame> {
        let id = self.id.take();
        let event = std::mem::take(&mut self.event);
        if !self.has_data {
            return None;
        }
        self.has_data = false;
        Some(SseFrame {
            id,
            event: if event.is_empty() { "message".into() } else { event },
            data: std::mem::take(&mut self.data),
        })
    }
}

/// Maps a frame to an [`Event`]: `gap` → `Resync`, anything else must carry
/// a `proto.Event` JSON (else it is dropped).
pub fn decode_frame(f: &SseFrame) -> Option<Event> {
    if f.event == "gap" {
        return Some(Event::Resync);
    }
    let w: WireEvent = serde_json::from_str(&f.data).ok()?;
    if w.name == "gap" {
        return Some(Event::Resync);
    }
    let seq = if w.seq != 0 { w.seq } else { f.id.as_deref().and_then(|s| s.parse().ok()).unwrap_or(0) };
    let name = if w.name.is_empty() { f.event.clone() } else { w.name };
    Some(Event::Core { seq, name, data: w.data })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed(p: &mut SseParser, text: &str) -> Vec<SseFrame> {
        text.split('\n').filter_map(|l| p.line(l)).collect()
    }

    #[test]
    fn frames_with_id_event_data() {
        let mut p = SseParser::new();
        let got = feed(&mut p, "retry: 1000\n: ok\n\nid: 42\nevent: session.state\ndata: {\"seq\":42}\n\n");
        assert_eq!(p.retry, Some(1000));
        assert_eq!(
            got,
            vec![SseFrame { id: Some("42".into()), event: "session.state".into(), data: "{\"seq\":42}".into() }]
        );
        assert_eq!(p.last_id.as_deref(), Some("42"));
    }

    #[test]
    fn multiline_data_crlf_and_defaults() {
        let mut p = SseParser::new();
        let got = feed(&mut p, "data:a\r\ndata: b\r\ndata\r\n\r\n");
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].event, "message");
        assert_eq!(got[0].data, "a\nb\n");
        assert_eq!(got[0].id, None);
    }

    #[test]
    fn no_data_no_frame_but_id_sticks() {
        let mut p = SseParser::new();
        assert!(feed(&mut p, "id: 7\nevent: x\n\n").is_empty());
        assert_eq!(p.last_id.as_deref(), Some("7"));
        // the event name does not leak into the next frame
        let got = feed(&mut p, "data: y\n\n");
        assert_eq!(got[0].event, "message");
        assert_eq!(got[0].id, None);
    }

    #[test]
    fn gap_is_resync() {
        let mut p = SseParser::new();
        let got = feed(&mut p, "event: gap\ndata: {\"name\":\"gap\",\"data\":{\"since\":42,\"oldest\":950}}\n\n");
        assert_eq!(decode_frame(&got[0]), Some(Event::Resync));
        assert_eq!(p.last_id, None, "a gap carries no id");
    }

    #[test]
    fn decodes_core_events() {
        let f = SseFrame {
            id: Some("4".into()),
            event: "session.tail".into(),
            data: r#"{"seq":4,"at":"x","name":"session.tail","data":{"id":"abc","n_new":1}}"#.into(),
        };
        match decode_frame(&f) {
            Some(Event::Core { seq, name, data }) => {
                assert_eq!(seq, 4);
                assert_eq!(name, "session.tail");
                assert_eq!(data["n_new"], 1);
            }
            other => panic!("{other:?}"),
        }
        let junk = SseFrame { id: None, event: "x".into(), data: "not json".into() };
        assert_eq!(decode_frame(&junk), None);
    }

    #[test]
    fn real_stream_fixture() {
        let text = include_str!("../tests/fixtures/events.sse");
        let mut p = SseParser::new();
        let events: Vec<Event> = feed(&mut p, text).iter().filter_map(decode_frame).collect();
        assert!(events.len() >= 5, "{}", events.len());
        let mut last = 0;
        for e in &events {
            let Event::Core { seq, name, .. } = e else { panic!("{e:?}") };
            assert!(*seq > last, "seq goes up");
            last = *seq;
            assert!(!name.is_empty());
        }
        assert!(events.iter().any(|e| matches!(e, Event::Core { name, .. } if name == "snapshot.changed")));
        assert!(events.iter().any(|e| matches!(e, Event::Core { name, data, .. } if name == "session.tail" && data["n_new"].as_u64() >= Some(1))));
        assert_eq!(p.retry, Some(1000));
    }
}
