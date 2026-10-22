//! PipeWire (pipewire-rs 0.10): one thread with `MainLoop` + `Context` + `Core`. The registry tells us
//! about audio streams (pid, binary, Flatpak app id) and sinks; `set_param(Props { channelVolumes })`
//! moves per-stream gains and the default sink volume; our own sounds are one `Stream` that exists
//! connected only while something is audible (200 ms of silence disconnects it).
//!
//! Everything touching PipeWire runs on that thread; callers talk to it through `pipewire::channel`.
//! Volumes we changed are restored to what they were when we first saw them, on stream loss and exit.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::{Rc, Weak};
use std::time::Duration;

use pipewire as pw;
use pw::spa;
use pw::spa::pod::{self, Pod};

use crate::{
    AudioEvent, Out, Playback, Render, StreamId, StreamInfo, StreamVolume, SysEvent, Worker,
};

pub(crate) const RATE: u32 = 48_000;
const CHANNELS: usize = 2;
/// Frames per `process` call (1024 at 48 kHz ≈ 21 ms); also our `node.latency`.
const MAX_FRAMES: usize = 1024;
pub(crate) const OWN_NODE: &str = "nkw-sounds";

enum Msg {
    Quit,
    Volumes(u32, [f32; 2]),
    SinkVolume(f32),
    Render(Render),
    Wake,
    /// From our own `process` callback after 200 ms of silence.
    Silent,
}

/// A stream's props → `StreamInfo`; `None` unless `media.class` is an audio stream.
pub(crate) fn stream_info<'a>(
    id: u32,
    get: impl Fn(&str) -> Option<&'a str>,
) -> Option<StreamInfo> {
    let class = get("media.class")?;
    if !(class.starts_with("Stream/") && class.ends_with("/Audio")) {
        return None;
    }
    let s = |k: &str| get(k).filter(|v| !v.is_empty()).map(str::to_string);
    Some(StreamInfo {
        id: StreamId(id),
        pid: get("application.process.id").and_then(|p| p.parse().ok()),
        binary: s("application.process.binary"),
        app_id: s("pipewire.access.portal.app_id"),
        output: class.starts_with("Stream/Output/"),
    })
}

/// `{"name":"…"}` (the `default.audio.sink` metadata value) → the node name.
pub(crate) fn json_name(v: &str) -> Option<String> {
    let at = v.find("\"name\"")? + "\"name\"".len();
    let rest = v[at..]
        .trim_start()
        .strip_prefix(':')?
        .trim_start()
        .strip_prefix('"')?;
    let mut out = String::new();
    let mut chars = rest.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => return Some(out),
            '\\' => out.push(chars.next()?),
            c => out.push(c),
        }
    }
    None
}

/// `[l, r]` spread over `n` channels: mono gets the mean, more than two alternate left/right.
pub(crate) fn spread(lr: [f32; 2], n: usize) -> Vec<f32> {
    let [l, r] = lr.map(|g| g.clamp(0.0, 1.0));
    match n {
        0 | 2 => vec![l, r],
        1 => vec![(l + r) * 0.5],
        n => (0..n).map(|i| if i % 2 == 0 { l } else { r }).collect(),
    }
}

/// Above 100 % is allowed up to `wpctl`'s usual 150 %.
pub(crate) const MAX_SINK_VOLUME: f32 = 1.5;

/// Perceptual (cubic, as `wpctl` shows it) ↔ linear amplitude.
pub(crate) fn to_linear(v: f32) -> f32 {
    let v = v.clamp(0.0, MAX_SINK_VOLUME);
    v * v * v
}

pub(crate) fn to_perceptual(lin: &[f32]) -> f32 {
    if lin.is_empty() {
        return 0.0;
    }
    (lin.iter().sum::<f32>() / lin.len() as f32).max(0.0).cbrt()
}

pub(crate) fn props_pod(vols: &[f32]) -> Option<Vec<u8>> {
    let obj = pod::Value::Object(pod::Object {
        type_: spa::sys::SPA_TYPE_OBJECT_Props,
        id: spa::sys::SPA_PARAM_Props,
        properties: vec![pod::Property {
            key: spa::sys::SPA_PROP_channelVolumes,
            flags: pod::PropertyFlags::empty(),
            value: pod::Value::ValueArray(pod::ValueArray::Float(vols.to_vec())),
        }],
    });
    pod::serialize::PodSerializer::serialize(std::io::Cursor::new(Vec::new()), &obj)
        .ok()
        .map(|(c, _)| c.into_inner())
}

pub(crate) fn channel_volumes(bytes: &[u8]) -> Option<Vec<f32>> {
    let (_, v) = pod::deserialize::PodDeserializer::deserialize_any_from(bytes).ok()?;
    let pod::Value::Object(o) = v else {
        return None;
    };
    o.properties
        .into_iter()
        .find(|p| p.key == spa::sys::SPA_PROP_channelVolumes)
        .and_then(|p| match p.value {
            pod::Value::ValueArray(pod::ValueArray::Float(v)) => Some(v),
            _ => None,
        })
}

fn format_pod() -> Option<Vec<u8>> {
    let mut info = spa::param::audio::AudioInfoRaw::new();
    info.set_format(spa::param::audio::AudioFormat::F32LE);
    info.set_rate(RATE);
    info.set_channels(CHANNELS as u32);
    let mut pos = [0; spa::param::audio::MAX_CHANNELS];
    pos[0] = spa::sys::SPA_AUDIO_CHANNEL_FL;
    pos[1] = spa::sys::SPA_AUDIO_CHANNEL_FR;
    info.set_position(pos);
    let obj = pod::Value::Object(pod::Object {
        type_: spa::sys::SPA_TYPE_OBJECT_Format,
        id: spa::sys::SPA_PARAM_EnumFormat,
        properties: info.into(),
    });
    pod::serialize::PodSerializer::serialize(std::io::Cursor::new(Vec::new()), &obj)
        .ok()
        .map(|(c, _)| c.into_inner())
}

#[derive(PartialEq)]
enum Kind {
    Stream,
    Sink,
}

struct Tracked {
    node: pw::node::Node,
    _listener: pw::node::NodeListener,
    kind: Kind,
    name: Option<String>,
    client: Option<u32>,
    /// Node props kept until the stream is announced (needs its client's props too).
    pending: Option<HashMap<String, String>>,
    announced: bool,
    vols: Option<Vec<f32>>,
    /// The volumes before we first touched them, for the restore.
    original: Option<Vec<f32>>,
}

impl Tracked {
    fn set(&self, vols: &[f32]) {
        if let Some(bytes) = props_pod(vols)
            && let Some(p) = Pod::from_bytes(&bytes)
        {
            self.node.set_param(spa::param::ParamType::Props, 0, p);
        }
    }
}

/// The client props a stream inherits (native `pw_stream` nodes do not carry them themselves).
const CLIENT_KEYS: [&str; 3] = [
    "application.process.id",
    "application.process.binary",
    "pipewire.access.portal.app_id",
];

struct Client {
    _client: pw::client::Client,
    _listener: pw::client::ClientListener,
    props: Option<HashMap<String, String>>,
}

#[derive(Default)]
struct St {
    nodes: HashMap<u32, Tracked>,
    clients: HashMap<u32, Client>,
    default_sink: Option<String>,
    touched: HashSet<u32>,
    meta: Option<(pw::metadata::Metadata, pw::metadata::MetadataListener)>,
}

impl St {
    fn default_sink(&self) -> Option<&Tracked> {
        let name = self.default_sink.as_deref()?;
        self.nodes
            .values()
            .find(|t| t.kind == Kind::Sink && t.name.as_deref() == Some(name))
    }

    fn emit_sink_volume(&self, out: &Out) {
        if let Some(v) = self.default_sink().and_then(|t| t.vols.as_deref()) {
            out.send(SysEvent::Audio(AudioEvent::DefaultSinkVolume(
                to_perceptual(v),
            )));
        }
    }

    /// Announces every stream whose node and client props are both known.
    fn announce_ready(&mut self, out: &Out) {
        let St { nodes, clients, .. } = self;
        let mut ready: Vec<StreamInfo> = Vec::new();
        for (id, t) in nodes.iter_mut() {
            if t.kind != Kind::Stream || t.announced {
                continue;
            }
            let Some(node) = &t.pending else { continue };
            let entry = t.client.and_then(|c| clients.get(&c));
            if entry.is_some_and(|c| c.props.is_none()) {
                continue; // its client's info has not arrived yet
            }
            let client = entry.and_then(|c| c.props.as_ref());
            let get = |k: &str| {
                node.get(k)
                    .or_else(|| client.and_then(|c| c.get(k)))
                    .map(String::as_str)
            };
            if let Some(si) = stream_info(*id, get) {
                t.announced = true;
                t.pending = None;
                ready.push(si);
            }
        }
        ready.sort_by_key(|si| si.id);
        for si in ready {
            out.send(SysEvent::Audio(AudioEvent::Added(si)));
        }
    }

    fn restore(&mut self, id: u32) {
        if self.touched.remove(&id)
            && let Some(t) = self.nodes.get(&id)
        {
            let n = t.vols.as_ref().map_or(CHANNELS, Vec::len);
            let back = t.original.clone().unwrap_or_else(|| vec![1.0; n]);
            t.set(&back);
        }
    }
}

/// The caller's handle: every call is a message to the PipeWire thread.
pub(crate) struct PwHandle {
    tx: pw::channel::Sender<Msg>,
}

impl StreamVolume for PwHandle {
    fn set_channel_volumes(&self, id: StreamId, lr: [f32; 2]) {
        let _ = self.tx.send(Msg::Volumes(id.0, lr));
    }
    fn set_default_sink_volume(&self, v: f32) {
        let _ = self.tx.send(Msg::SinkVolume(v));
    }
}

impl Playback for PwHandle {
    fn set_render(&self, render: Render) {
        let _ = self.tx.send(Msg::Render(render));
    }
    fn rate(&self) -> u32 {
        RATE
    }
    fn wake(&self) {
        let _ = self.tx.send(Msg::Wake);
    }
}

/// Starts the thread and waits (≤ 250 ms) for the connection; a missing daemon is an `Err`.
pub(crate) fn spawn(
    out: Out,
    remote: Option<String>,
) -> Result<(std::sync::Arc<PwHandle>, Worker), String> {
    let (tx, rx) = pw::channel::channel::<Msg>();
    let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<(), String>>();
    let self_tx = tx.clone();
    let handle = std::thread::Builder::new()
        .name("nkw-sys-pipewire".into())
        .spawn(move || {
            if let Err(e) = run(out, remote, rx, self_tx, &ready_tx) {
                let _ = ready_tx.send(Err(e));
            }
        })
        .map_err(|e| e.to_string())?;
    match ready_rx.recv_timeout(Duration::from_millis(250)) {
        Ok(Ok(())) => {
            let stop_tx = tx.clone();
            let w = Worker {
                stop: Box::new(move || drop(stop_tx.send(Msg::Quit))),
                handle,
            };
            Ok((std::sync::Arc::new(PwHandle { tx }), w))
        }
        Ok(Err(e)) => Err(e),
        Err(_) => {
            let _ = tx.send(Msg::Quit);
            Err("timed out connecting to PipeWire".into())
        }
    }
}

fn run(
    out: Out,
    remote: Option<String>,
    rx: pw::channel::Receiver<Msg>,
    self_tx: pw::channel::Sender<Msg>,
    ready: &std::sync::mpsc::Sender<Result<(), String>>,
) -> Result<(), String> {
    pw::init();
    let mainloop = pw::main_loop::MainLoopRc::new(None).map_err(|e| e.to_string())?;
    let context = pw::context::ContextRc::new(&mainloop, None).map_err(|e| e.to_string())?;
    let props = remote.map(|r| pw::properties::properties! { *pw::keys::REMOTE_NAME => r });
    let core = context
        .connect_rc(props)
        .map_err(|e| format!("connect: {e}"))?;
    let registry = core.get_registry_rc().map_err(|e| e.to_string())?;
    let st: Rc<RefCell<St>> = Rc::new(RefCell::new(St::default()));

    // Quit = restore, then a core round trip so the restores reach the server, then leave.
    let quit_seq: Rc<Cell<Option<i32>>> = Rc::new(Cell::new(None));
    let _core_listener = core
        .add_listener_local()
        .done({
            let ml = mainloop.downgrade();
            let quit_seq = quit_seq.clone();
            move |id, seq| {
                if id == pw::core::PW_ID_CORE
                    && Some(seq.seq()) == quit_seq.get()
                    && let Some(ml) = ml.upgrade()
                {
                    ml.quit();
                }
            }
        })
        .error({
            let ml = mainloop.downgrade();
            let out = out.clone();
            move |id, _seq, _res, msg| {
                if id == pw::core::PW_ID_CORE {
                    out.error("pipewire", msg);
                    if let Some(ml) = ml.upgrade() {
                        ml.quit();
                    }
                }
            }
        })
        .register();

    let _registry_listener = registry
        .add_listener_local()
        .global({
            let st = Rc::downgrade(&st);
            let reg = registry.downgrade();
            let out = out.clone();
            move |g| on_global(g, &st, &reg, &out)
        })
        .global_remove({
            let st = Rc::downgrade(&st);
            let out = out.clone();
            move |id| {
                let Some(st) = st.upgrade() else { return };
                let mut st = st.borrow_mut();
                st.touched.remove(&id);
                st.clients.remove(&id);
                if let Some(t) = st.nodes.remove(&id)
                    && t.kind == Kind::Stream
                    && t.announced
                {
                    out.send(SysEvent::Audio(AudioEvent::Removed(StreamId(id))));
                }
            }
        })
        .register();

    // Our own sounds: created now, connected only on wake.
    let render: Rc<RefCell<Option<Render>>> = Rc::new(RefCell::new(None));
    let silent_frames = Rc::new(Cell::new(0usize));
    let silence_reported = Rc::new(Cell::new(false));
    let connected = Cell::new(false);
    let stream = pw::stream::StreamRc::new(
        core.clone(),
        OWN_NODE,
        pw::properties::properties! {
            *pw::keys::MEDIA_TYPE => "Audio",
            *pw::keys::MEDIA_CATEGORY => "Playback",
            *pw::keys::MEDIA_ROLE => "Game",
            *pw::keys::NODE_NAME => OWN_NODE,
            *pw::keys::NODE_LATENCY => format!("{MAX_FRAMES}/{RATE}"),
        },
    )
    .map_err(|e| e.to_string())?;
    let _stream_listener = stream
        .add_local_listener_with_user_data(Vec::<f32>::new())
        .process({
            let render = render.clone();
            let silent_frames = silent_frames.clone();
            let silence_reported = silence_reported.clone();
            let self_tx = self_tx.clone();
            move |s, scratch| {
                let Some(mut buf) = s.dequeue_buffer() else {
                    return;
                };
                let datas = buf.datas_mut();
                let Some(d) = datas.first_mut() else { return };
                let stride = CHANNELS * 4;
                let mut frames = 0;
                if let Some(bytes) = d.data() {
                    frames = (bytes.len() / stride).min(MAX_FRAMES);
                    scratch.clear();
                    scratch.resize(frames * CHANNELS, 0.0);
                    let audible = render.borrow_mut().as_mut().is_some_and(|r| r(scratch));
                    for (i, x) in scratch.iter().enumerate() {
                        bytes[i * 4..i * 4 + 4].copy_from_slice(&x.to_le_bytes());
                    }
                    if audible {
                        silent_frames.set(0);
                    } else {
                        silent_frames.set(silent_frames.get() + frames);
                        if silent_frames.get() >= RATE as usize / 5 && !silence_reported.get() {
                            silence_reported.set(true);
                            let _ = self_tx.send(Msg::Silent);
                        }
                    }
                }
                let chunk = d.chunk_mut();
                *chunk.offset_mut() = 0;
                *chunk.stride_mut() = stride as i32;
                *chunk.size_mut() = (frames * stride) as u32;
            }
        })
        .register()
        .map_err(|e| e.to_string())?;
    let format = format_pod().ok_or("format pod")?;

    let _rx = rx.attach(mainloop.loop_(), {
        let st = st.clone();
        let core = core.clone();
        let stream = stream.clone();
        let quit_seq = quit_seq.clone();
        let ml = mainloop.clone();
        move |msg| match msg {
            Msg::Quit => {
                let mut s = st.borrow_mut();
                let ids: Vec<u32> = s.touched.iter().copied().collect();
                for id in ids {
                    s.restore(id);
                }
                drop(s);
                if connected.get() {
                    let _ = stream.disconnect();
                    connected.set(false);
                }
                // a hung server leaves us here; `Sys::shutdown` bounds the join anyway
                match core.sync(0) {
                    Ok(seq) => quit_seq.set(Some(seq.seq())),
                    Err(_) => ml.quit(),
                }
            }
            Msg::Volumes(id, lr) => {
                let mut s = st.borrow_mut();
                let Some(t) = s.nodes.get(&id) else { return };
                if t.kind != Kind::Stream {
                    return;
                }
                let n = t.vols.as_ref().map_or(CHANNELS, Vec::len);
                t.set(&spread(lr, n));
                s.touched.insert(id);
            }
            Msg::SinkVolume(v) => {
                let s = st.borrow();
                if let Some(t) = s.default_sink() {
                    let n = t.vols.as_ref().map_or(CHANNELS, Vec::len);
                    t.set(&vec![to_linear(v); n.max(1)]);
                }
            }
            Msg::Render(r) => *render.borrow_mut() = Some(r),
            Msg::Wake => {
                silent_frames.set(0);
                silence_reported.set(false);
                if !connected.get() {
                    let Some(p) = Pod::from_bytes(&format) else {
                        return;
                    };
                    let flags =
                        pw::stream::StreamFlags::AUTOCONNECT | pw::stream::StreamFlags::MAP_BUFFERS;
                    if stream
                        .connect(spa::utils::Direction::Output, None, flags, &mut [p])
                        .is_ok()
                    {
                        connected.set(true);
                    }
                }
            }
            Msg::Silent => {
                // a wake since the report cancels it
                if silence_reported.get() && connected.get() {
                    let _ = stream.disconnect();
                    connected.set(false);
                }
            }
        }
    });

    let _ = ready.send(Ok(()));
    mainloop.run();
    // drop order: listeners before proxies before the core
    st.borrow_mut().nodes.clear();
    st.borrow_mut().clients.clear();
    st.borrow_mut().meta = None;
    Ok(())
}

fn on_global(
    g: &pw::registry::GlobalObject<&spa::utils::dict::DictRef>,
    st: &Weak<RefCell<St>>,
    reg: &pw::registry::RegistryWeak,
    out: &Out,
) {
    let (Some(st_rc), Some(reg)) = (st.upgrade(), reg.upgrade()) else {
        return;
    };
    let Some(props) = g.props else { return };
    match g.type_ {
        pw::types::ObjectType::Node => {
            let class = props.get("media.class").unwrap_or("");
            let kind = if class.starts_with("Stream/") && class.ends_with("/Audio") {
                Kind::Stream
            } else if class == "Audio/Sink" {
                Kind::Sink
            } else {
                return;
            };
            if props.get("node.name") == Some(OWN_NODE) {
                return;
            }
            let Ok(node) = reg.bind::<pw::node::Node, _>(g) else {
                return;
            };
            let id = g.id;
            let listener = node
                .add_listener_local()
                .info({
                    let st = st.clone();
                    let out = out.clone();
                    move |info| {
                        let (Some(st), Some(p)) = (st.upgrade(), info.props()) else {
                            return;
                        };
                        let mut st = st.borrow_mut();
                        let Some(t) = st.nodes.get_mut(&id) else {
                            return;
                        };
                        if t.name.is_none() {
                            t.name = p.get("node.name").map(str::to_string);
                        }
                        if t.client.is_none() {
                            t.client = p.get("client.id").and_then(|c| c.parse().ok());
                        }
                        if t.kind == Kind::Stream && !t.announced {
                            let keep = ["media.class"].into_iter().chain(CLIENT_KEYS);
                            t.pending = Some(
                                keep.filter_map(|k| Some((k.to_string(), p.get(k)?.to_string())))
                                    .collect(),
                            );
                            st.announce_ready(&out);
                        }
                    }
                })
                .param({
                    let st = st.clone();
                    let out = out.clone();
                    move |_seq, ty, _index, _next, param| {
                        if ty != spa::param::ParamType::Props {
                            return;
                        }
                        let (Some(st), Some(param)) = (st.upgrade(), param) else {
                            return;
                        };
                        let Some(v) = channel_volumes(param.as_bytes()) else {
                            return;
                        };
                        let mut s = st.borrow_mut();
                        let default = s.default_sink.clone();
                        let Some(t) = s.nodes.get_mut(&id) else {
                            return;
                        };
                        if t.original.is_none() {
                            t.original = Some(v.clone());
                        }
                        let changed = t.vols.as_ref() != Some(&v);
                        t.vols = Some(v);
                        let is_default =
                            t.kind == Kind::Sink && t.name.is_some() && t.name == default;
                        if changed && is_default {
                            s.emit_sink_volume(&out);
                        }
                    }
                })
                .register();
            node.subscribe_params(&[spa::param::ParamType::Props]);
            let name = props.get("node.name").map(str::to_string);
            let client = props.get("client.id").and_then(|c| c.parse().ok());
            st_rc.borrow_mut().nodes.insert(
                id,
                Tracked {
                    node,
                    _listener: listener,
                    kind,
                    name,
                    client,
                    pending: None,
                    announced: false,
                    vols: None,
                    original: None,
                },
            );
        }
        pw::types::ObjectType::Client => {
            let Ok(client) = reg.bind::<pw::client::Client, _>(g) else {
                return;
            };
            let id = g.id;
            let listener = client
                .add_listener_local()
                .info({
                    let st = st.clone();
                    let out = out.clone();
                    move |info| {
                        let (Some(st), Some(p)) = (st.upgrade(), info.props()) else {
                            return;
                        };
                        let mut st = st.borrow_mut();
                        let Some(c) = st.clients.get_mut(&id) else {
                            return;
                        };
                        c.props = Some(
                            CLIENT_KEYS
                                .iter()
                                .filter_map(|k| Some((k.to_string(), p.get(k)?.to_string())))
                                .collect(),
                        );
                        st.announce_ready(&out);
                    }
                })
                .register();
            st_rc.borrow_mut().clients.insert(
                id,
                Client {
                    _client: client,
                    _listener: listener,
                    props: None,
                },
            );
        }
        pw::types::ObjectType::Metadata => {
            if props.get("metadata.name") != Some("default") {
                return;
            }
            let Ok(meta) = reg.bind::<pw::metadata::Metadata, _>(g) else {
                return;
            };
            let listener = meta
                .add_listener_local()
                .property({
                    let st = st.clone();
                    let out = out.clone();
                    move |_subject, key, _type, value| {
                        if key == Some("default.audio.sink")
                            && let Some(st) = st.upgrade()
                        {
                            let mut s = st.borrow_mut();
                            s.default_sink = value.and_then(json_name);
                            s.emit_sink_volume(&out);
                        }
                        0
                    }
                })
                .register();
            st_rc.borrow_mut().meta = Some((meta, listener));
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stream_info_from_props() {
        let p: HashMap<&str, &str> = [
            ("media.class", "Stream/Output/Audio"),
            ("application.process.id", "4242"),
            ("application.process.binary", "demo-player"),
            ("pipewire.access.portal.app_id", "org.acme.Demo"),
        ]
        .into();
        let si = stream_info(7, |k| p.get(k).copied()).unwrap();
        assert_eq!(
            si,
            StreamInfo {
                id: StreamId(7),
                pid: Some(4242),
                binary: Some("demo-player".into()),
                app_id: Some("org.acme.Demo".into()),
                output: true
            }
        );
        let input: HashMap<&str, &str> = [
            ("media.class", "Stream/Input/Audio"),
            ("application.process.id", "x"),
        ]
        .into();
        let si = stream_info(8, |k| input.get(k).copied()).unwrap();
        assert!(!si.output && si.pid.is_none() && si.binary.is_none());
        for class in ["Audio/Sink", "Stream/Output/Video", "Video/Source"] {
            let p: HashMap<&str, &str> = [("media.class", class)].into();
            assert!(stream_info(9, |k| p.get(k).copied()).is_none(), "{class}");
        }
        assert!(stream_info(9, |_| None).is_none());
    }

    #[test]
    fn json_name_table() {
        let cases: &[(&str, Option<&str>)] = &[
            (r#"{"name":"acme-sink"}"#, Some("acme-sink")),
            (r#"{ "name" : "alpha.bravo" }"#, Some("alpha.bravo")),
            (r#"{"name":"we\"ird"}"#, Some("we\"ird")),
            (r#"{"other":"x"}"#, None),
            (r#"{"name":"cut"#, None),
            ("", None),
        ];
        for &(s, want) in cases {
            assert_eq!(json_name(s).as_deref(), want, "{s}");
        }
    }

    #[test]
    fn spread_table() {
        assert_eq!(spread([0.2, 0.8], 2), vec![0.2, 0.8]);
        assert_eq!(spread([0.2, 0.8], 0), vec![0.2, 0.8]);
        assert_eq!(spread([0.2, 0.8], 1), vec![0.5]);
        assert_eq!(spread([0.2, 0.8], 6), vec![0.2, 0.8, 0.2, 0.8, 0.2, 0.8]);
        assert_eq!(
            spread([-1.0, 3.0], 2),
            vec![0.0, 1.0],
            "clamped: the world only lowers"
        );
    }

    #[test]
    fn cubic_volume() {
        assert_eq!(to_linear(1.0), 1.0);
        assert!((to_linear(2.0) - 3.375).abs() < 1e-5, "clamped at 150 %");
        assert_eq!(to_linear(0.0), 0.0);
        assert!((to_linear(0.5) - 0.125).abs() < 1e-6);
        assert!((to_perceptual(&[0.125, 0.125]) - 0.5).abs() < 1e-5);
        assert_eq!(to_perceptual(&[]), 0.0);
        assert!((to_perceptual(&[to_linear(0.7); 2]) - 0.7).abs() < 1e-5);
    }

    #[test]
    fn props_pod_round_trip() {
        let bytes = props_pod(&[0.25, 0.75]).unwrap();
        assert_eq!(channel_volumes(&bytes), Some(vec![0.25, 0.75]));
        assert!(format_pod().is_some());
        assert_eq!(channel_volumes(&format_pod().unwrap()), None);
    }
}
