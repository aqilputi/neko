//! nkw-sys: the thin, event-driven system adapters of the world (docs/plano-mundo-objetos.md §2.6, §4.6).
//!
//! Every adapter runs on its own thread and sleeps in `epoll` until the kernel, the bus or PipeWire
//! has something new; decoded changes cross to the caller as [`SysEvent`]s on a channel, each followed
//! by a call to the caller's `wake` (the compositor ties it to its event loop). Nothing here polls,
//! nothing here knows about Wayland or the event loop, and no event carries a personal identifier
//! (SSIDs and device names are never read).

use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, Sender};
use serde::{Deserialize, Serialize};

pub mod clock;
mod dbus;
pub mod demo;
pub mod pidmap;
mod psi;
#[cfg(feature = "pipewire")]
mod pw;

/// One decoded change from the system.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub enum SysEvent {
    /// MPRIS: the selected player ("the last one that went Playing wins") changed state.
    Media(MediaState),
    /// MPRIS: no player is left on the bus.
    MediaGone,
    /// UPower `DisplayDevice`.
    Power(PowerState),
    /// NetworkManager: state, active connection kind, AP strength. The SSID is never in it.
    Net(NetState),
    /// On each wall-clock minute, at start, and after resume or a clock change.
    Clock(ClockTick),
    /// PipeWire streams and the default sink volume.
    Audio(AudioEvent),
    /// A PSI trigger fired (the kernel only signals the rising edge; the caller decays).
    Pressure { cpu: bool, mem: bool },
    /// One per adapter that failed to start; the others keep running.
    Error { source: &'static str, msg: String },
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MediaState {
    pub playing: bool,
    pub title: String,
    pub artist: String,
    pub art_url: Option<String>,
    pub volume: f32,
    pub position_us: Option<i64>,
    pub length_us: Option<i64>,
    /// The bus name suffix after `org.mpris.MediaPlayer2.`.
    pub player: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PowerState {
    pub present: bool,
    pub pct: u8,
    pub charging: bool,
    pub time_to_empty_s: Option<u32>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetState {
    pub connected: bool,
    pub wifi: bool,
    /// Bars, 0..=4.
    pub strength: u8,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClockTick {
    pub unix_s: i64,
    pub hour: u8,
    pub minute: u8,
    /// Day band 0..=7 (addendum §2.1, [`clock::band_of`]).
    pub band: u8,
}

/// A PipeWire node id.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct StreamId(pub u32);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StreamInfo {
    pub id: StreamId,
    /// `application.process.id` (self-declared, but filled by libpulse/libpipewire from `getpid()`).
    pub pid: Option<u32>,
    pub binary: Option<String>,
    /// `pipewire.access.portal.app_id` (Flatpak).
    pub app_id: Option<String>,
    /// `media.class` is `Stream/Output/*`.
    pub output: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum AudioEvent {
    Added(StreamInfo),
    Removed(StreamId),
    /// Perceptual (cubic) volume of the default sink, `wpctl`'s scale: 1.0 = 100 %, may exceed it.
    DefaultSinkVolume(f32),
}

pub type Wake = Arc<dyn Fn() + Send + Sync>;

/// Fills interleaved stereo f32; `false` means "silent".
pub type Render = Box<dyn FnMut(&mut [f32]) -> bool + Send>;

pub struct SysOpts {
    pub session_bus: bool,
    pub system_bus: bool,
    pub mpris: bool,
    pub upower: bool,
    pub nm: bool,
    pub clock: bool,
    pub pipewire: bool,
    pub psi: bool,
    pub wake: Option<Wake>,
    /// Tests: connect both "session" and "system" adapters to this bus address instead.
    #[doc(hidden)]
    pub bus_override: Option<String>,
    /// Tests: tick every N seconds (aligned to multiples of N) instead of every minute.
    #[doc(hidden)]
    pub clock_period_s: u32,
    /// Tests: the PipeWire remote (socket name or absolute path) instead of the default one.
    #[doc(hidden)]
    pub pw_remote: Option<String>,
}

impl Default for SysOpts {
    fn default() -> Self {
        Self {
            session_bus: true,
            system_bus: true,
            mpris: true,
            upower: true,
            nm: true,
            clock: true,
            pipewire: true,
            psi: true,
            wake: None,
            bus_override: None,
            clock_period_s: 60,
            pw_remote: None,
        }
    }
}

impl SysOpts {
    /// Everything off; turn on what you need.
    pub fn none() -> Self {
        Self {
            session_bus: false,
            system_bus: false,
            mpris: false,
            upower: false,
            nm: false,
            clock: false,
            pipewire: false,
            psi: false,
            ..Self::default()
        }
    }
}

pub trait MediaControl: Send + Sync {
    fn play_pause(&self);
    fn next(&self);
    fn previous(&self);
    fn set_volume(&self, v: f32);
}

pub trait StreamVolume: Send + Sync {
    /// Linear per-channel gains (the acoustic model's output), `[1, 1]` = untouched.
    fn set_channel_volumes(&self, id: StreamId, lr: [f32; 2]);
    /// Perceptual (cubic) volume, `wpctl`'s scale, clamped to 0..=1.5.
    fn set_default_sink_volume(&self, v: f32);
}

pub trait Playback: Send + Sync {
    /// `render` fills interleaved stereo f32 at `rate`; returning false means "silent": the stream is
    /// disconnected after 200 ms of silence and reconnected on the next `wake()`.
    fn set_render(&self, render: Render);
    fn rate(&self) -> u32;
    fn wake(&self);
}

/// The sending half every adapter gets: send, then wake the caller.
#[derive(Clone)]
pub(crate) struct Out {
    tx: Sender<SysEvent>,
    wake: Option<Wake>,
}

impl Out {
    pub(crate) fn send(&self, ev: SysEvent) {
        if self.tx.send(ev).is_ok()
            && let Some(w) = &self.wake
        {
            w();
        }
    }

    pub(crate) fn error(&self, source: &'static str, msg: impl ToString) {
        self.send(SysEvent::Error {
            source,
            msg: msg.to_string(),
        });
    }
}

/// One adapter thread plus the way to ask it to stop.
pub(crate) struct Worker {
    pub(crate) stop: Box<dyn FnOnce() + Send>,
    pub(crate) handle: JoinHandle<()>,
}

struct NoAudio;
impl StreamVolume for NoAudio {
    fn set_channel_volumes(&self, _: StreamId, _: [f32; 2]) {}
    fn set_default_sink_volume(&self, _: f32) {}
}

pub struct Sys {
    rx: Receiver<SysEvent>,
    media: Arc<dyn MediaControl>,
    audio: Arc<dyn StreamVolume>,
    playback: Option<Arc<dyn Playback>>,
    workers: Vec<Worker>,
}

/// Starts every adapter `opts` asks for. Never panics and never blocks on a bus: a missing bus or
/// daemon becomes one [`SysEvent::Error`] and the rest runs.
pub fn start(opts: SysOpts) -> Sys {
    let (tx, rx) = crossbeam_channel::unbounded();
    let out = Out {
        tx,
        wake: opts.wake.clone(),
    };
    let mut workers = Vec::new();

    let (media, session_worker) = dbus::start_session(&opts, &out);
    workers.extend(session_worker);
    workers.extend(dbus::start_system(&opts, &out));

    if opts.clock {
        match clock::spawn(opts.clock_period_s.max(1), out.clone()) {
            Ok(w) => workers.push(w),
            Err(e) => out.error("clock", e),
        }
    }
    if opts.psi {
        match psi::spawn(out.clone()) {
            Ok(w) => workers.push(w),
            Err(e) => out.error("psi", e),
        }
    }

    #[cfg_attr(not(feature = "pipewire"), allow(unused_mut))]
    let mut audio: Arc<dyn StreamVolume> = Arc::new(NoAudio);
    #[cfg_attr(not(feature = "pipewire"), allow(unused_mut))]
    let mut playback: Option<Arc<dyn Playback>> = None;
    if opts.pipewire {
        #[cfg(feature = "pipewire")]
        match pw::spawn(out.clone(), opts.pw_remote.clone()) {
            Ok((handle, w)) => {
                audio = handle.clone();
                playback = Some(handle);
                workers.push(w);
            }
            Err(e) => out.error("pipewire", e),
        }
        #[cfg(not(feature = "pipewire"))]
        out.error("pipewire", "built without the pipewire feature");
    }

    Sys {
        rx,
        media,
        audio,
        playback,
        workers,
    }
}

impl Sys {
    pub fn events(&self) -> &Receiver<SysEvent> {
        &self.rx
    }

    pub fn media(&self) -> &dyn MediaControl {
        &*self.media
    }

    pub fn audio(&self) -> &dyn StreamVolume {
        &*self.audio
    }

    /// The same, shared: for a caller that must reach PipeWire from elsewhere (a panic hook
    /// restoring the volumes it changed).
    pub fn audio_handle(&self) -> Arc<dyn StreamVolume> {
        self.audio.clone()
    }

    /// Some only with the pipewire feature and a running daemon.
    pub fn playback(&self) -> Option<&dyn Playback> {
        self.playback.as_deref()
    }

    /// The same, shared (the caller keeps it to `wake()` the stream later).
    pub fn playback_handle(&self) -> Option<Arc<dyn Playback>> {
        self.playback.clone()
    }

    /// Stops and joins every adapter thread, waiting at most 1 s in total; a thread still stuck
    /// after that is left detached.
    pub fn shutdown(self) {
        let deadline = Instant::now() + Duration::from_secs(1);
        let mut handles = Vec::new();
        for w in self.workers {
            (w.stop)();
            handles.push(w.handle);
        }
        for h in handles {
            while !h.is_finished() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(2));
            }
            if h.is_finished() {
                let _ = h.join();
            }
        }
    }
}
