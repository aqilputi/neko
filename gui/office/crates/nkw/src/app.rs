//! Start-up, the event loop and its wakeups: the world socket, the host
//! (winit nested, or surfaceless EGL headless with a software vblank), the
//! frame scheduler's timers, `--script`, `--seconds`, signals.

use std::collections::HashSet;
use std::os::fd::{AsFd, AsRawFd, FromRawFd, OwnedFd};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use calloop::generic::Generic;
use calloop::timer::{TimeoutAction, Timer};
use calloop::{EventLoop, Interest, Mode as CMode, PostAction};
use smithay::backend::allocator::Fourcc;
use smithay::backend::egl::native::EGLSurfacelessDisplay;
use smithay::backend::egl::{EGLContext, EGLDevice, EGLDisplay};
use smithay::backend::input::{InputTime, KeyState};
use smithay::backend::renderer::damage::OutputDamageTracker;
use smithay::backend::renderer::glow::GlowRenderer;
use smithay::backend::renderer::{ImportDma, ImportMemWl, Offscreen};
use smithay::backend::winit::{self, WinitEvent};
use smithay::desktop::PopupManager;
use smithay::input::SeatState;
use smithay::input::keyboard::{Keycode, XkbConfig};
use smithay::output::{Mode, Output, PhysicalProperties, Scale, Subpixel};
use smithay::reexports::wayland_server::Display;
use smithay::reexports::winit::dpi::LogicalSize;
use smithay::reexports::winit::window::WindowAttributes;
use smithay::utils::Transform;
use smithay::wayland::compositor::CompositorState;
use smithay::wayland::dmabuf::{DmabufFeedbackBuilder, DmabufState};
use smithay::wayland::output::OutputManagerState;
use smithay::wayland::selection::data_device::DataDeviceState;
use smithay::wayland::shell::xdg::XdgShellState;
use smithay::wayland::shell::xdg::decoration::XdgDecorationState;
use smithay::wayland::shm::ShmState;
use smithay::wayland::socket::ListeningSocketSource;

use nkw_core::Time;

use crate::cli::{Cli, exit};
use crate::frame::{Gfx, Host};
use crate::keys::KeyTable;
use crate::script::{Cmd, Line, mode_name};
use crate::state::{Nkw, Sched, client_data};
use crate::stats::Stats;
use crate::world::{Opts, World};

/// A failure at start-up: exit code and message.
pub type Fail = (i32, String);

/// The `--script` being run.
pub struct Script {
    pub lines: Vec<Line>,
    pub pc: usize,
    pub keys: Option<KeyTable>,
    /// waiting for: a frame (after `shot`), a new window (after `spawn`)
    pub wait: Wait,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Wait {
    None,
    /// a `wait` timer is pending
    Timer,
    /// the camera to settle before a shot
    Settle,
    /// the shot to be written
    Shot,
    /// a new window to map (with a deadline)
    Window {
        had: usize,
        until: Time,
    },
}

impl Nkw {
    /// A frame at the next host vblank (winit) or software vblank (headless).
    pub fn request_frame(&mut self) {
        if self.sched.frame_requested {
            return;
        }
        self.sched.frame_requested = true;
        match &self.gfx.host {
            Host::Winit(b) => b.window().request_redraw(),
            Host::Headless { .. } => {
                let now = self.now();
                let at = self.grid(now).at_or_after(now);
                let dur = Duration::from_secs_f64((at - now).max(0.0));
                let r = self
                    .lh
                    .insert_source(Timer::from_duration(dur), |_, _, st: &mut Nkw| {
                        st.frame();
                        st.after_frame();
                        TimeoutAction::Drop
                    });
                if r.is_err() {
                    self.sched.frame_requested = false;
                }
            }
        }
    }

    /// A wakeup at `t` (the scheduler's `next_wakeup`); nothing when `None`.
    pub fn arm(&mut self, t: Option<Time>) {
        let Some(t) = t else { return };
        let now = self.now();
        if t <= now + self.sched.period * 0.5 {
            self.request_frame();
            return;
        }
        if let Some((tok, at)) = self.sched.timer {
            if at <= t + 1e-4 {
                return;
            }
            self.lh.remove(tok);
            self.sched.timer = None;
        }
        let dur = Duration::from_secs_f64(t - now);
        if let Ok(tok) = self
            .lh
            .insert_source(Timer::from_duration(dur), |_, _, st: &mut Nkw| {
                st.sched.timer = None;
                st.request_frame();
                TimeoutAction::Drop
            })
        {
            self.sched.timer = Some((tok, t));
        }
    }

    /// After every frame: script barriers and the final `--shot`.
    pub fn after_frame(&mut self) {
        let Some(s) = self.script.as_mut() else {
            return;
        };
        if s.wait == Wait::Shot && self.shots.is_empty() {
            s.wait = Wait::None;
            self.script_step();
        }
    }

    pub fn spawn(&mut self, args: &[String]) {
        let Some((cmd, rest)) = args.split_first() else {
            return;
        };
        let prog = resolve(cmd);
        let mut c = std::process::Command::new(&prog);
        // the world's display only: no X11 fallback to the host (no xwayland yet)
        c.args(rest)
            .env("WAYLAND_DISPLAY", &self.socket)
            .env_remove("DISPLAY");
        // SAFETY: the closure only calls async-signal-safe functions.
        unsafe {
            std::os::unix::process::CommandExt::pre_exec(&mut c, || {
                unblock_signals();
                Ok(())
            });
        }
        let r = c.spawn();
        match r {
            Ok(c) => self.children.push(c),
            Err(e) => eprintln!("nkw: spawn {}: {e}", prog.display()),
        }
    }

    /// Run script lines until one has to wait.
    pub fn script_step(&mut self) {
        loop {
            let Some(s) = self.script.as_mut() else {
                return;
            };
            if s.wait != Wait::None {
                return;
            }
            let Some(line) = s.lines.get(s.pc).cloned() else {
                self.script = None;
                self.finish();
                return;
            };
            s.pc += 1;
            let no = line.no;
            match line.cmd {
                Cmd::Wait(ms) => {
                    s.wait = Wait::Timer;
                    let _ = self.lh.insert_source(
                        Timer::from_duration(Duration::from_millis(ms)),
                        |_, _, st: &mut Nkw| {
                            if let Some(s) = st.script.as_mut() {
                                s.wait = Wait::None;
                            }
                            st.script_step();
                            TimeoutAction::Drop
                        },
                    );
                    return;
                }
                Cmd::Key(ref c) | Cmd::Down(ref c) | Cmd::Up(ref c) => {
                    let keys = self.script.as_ref().unwrap().keys.as_ref();
                    let ev = match (&line.cmd, keys) {
                        (_, None) => Err("no keymap".to_string()),
                        (Cmd::Key(_), Some(k)) => k.key(c),
                        (Cmd::Down(_), Some(k)) => k.down(c),
                        (_, Some(k)) => k.up(c),
                    };
                    match ev {
                        Ok(ev) => {
                            self.stats.inject(&c.key);
                            self.inject(&ev);
                        }
                        Err(e) => return self.script_fail(no, &e),
                    }
                }
                Cmd::Text(ref t) => {
                    let ev = self
                        .script
                        .as_ref()
                        .unwrap()
                        .keys
                        .as_ref()
                        .map(|k| k.text(t));
                    match ev {
                        Some(Ok(ev)) => {
                            self.stats.inject("text");
                            self.inject(&ev);
                        }
                        Some(Err(e)) => return self.script_fail(no, &e),
                        None => return self.script_fail(no, "no keymap"),
                    }
                }
                Cmd::Spawn(args) => {
                    let had = self.mapped();
                    self.spawn(&args);
                    let until = self.now() + 5.0;
                    self.script.as_mut().unwrap().wait = Wait::Window { had, until };
                    self.poll_window_wait();
                    return;
                }
                Cmd::Shot(p) => {
                    self.script.as_mut().unwrap().wait = Wait::Settle;
                    self.shot_when_settled(p);
                    return;
                }
                Cmd::Dump(p) => {
                    let v = self.world.dump(&self.world.wishes());
                    let r = std::fs::write(&p, serde_json::to_vec_pretty(&v).unwrap());
                    if let Err(e) = r {
                        eprintln!("nkw: dump {}: {e}", p.display());
                    }
                }
                Cmd::ExpectMode(m) => {
                    let actual = mode_name(self.world.mode().label()).to_string();
                    if actual != m {
                        eprintln!("nkw: script line {no}: expected mode {m}, actual {actual}");
                        self.quit(exit::EXPECT);
                        return;
                    }
                }
                Cmd::ExpectFocus(app) => {
                    let actual = self
                        .world
                        .focus_target()
                        .and_then(|id| self.world.screens.get(&id))
                        .map(|s| s.app_id.clone())
                        .unwrap_or_else(|| "<none>".into());
                    if actual != app {
                        eprintln!("nkw: script line {no}: expected focus {app}, actual {actual}");
                        self.quit(exit::EXPECT);
                        return;
                    }
                }
                Cmd::Quit => {
                    self.script = None;
                    self.finish();
                    return;
                }
            }
        }
    }

    fn script_fail(&mut self, no: usize, msg: &str) {
        eprintln!("nkw: script line {no}: {msg}");
        self.quit(exit::EXPECT);
    }

    /// Script over (or `quit`): the final `--shot`, then exit unless
    /// `--seconds` keeps the world up (no script: winit keeps running).
    fn finish(&mut self) {
        if let Some(p) = self.cli.shot.take() {
            self.script = Some(Script {
                lines: vec![Line {
                    no: 0,
                    cmd: Cmd::Quit,
                }],
                pc: 0,
                keys: None,
                wait: Wait::Settle,
            });
            self.shot_when_settled(p);
            return;
        }
        if self.cli.seconds.is_none() {
            self.quit(exit::OK);
        }
    }

    fn inject(&mut self, ev: &[(u32, bool)]) {
        for &(kc, down) in ev {
            let st = if down {
                KeyState::Pressed
            } else {
                KeyState::Released
            };
            self.key(Keycode::new(kc), st, InputTime::now());
        }
    }

    /// Windows that have a buffer.
    fn mapped(&self) -> usize {
        self.world
            .screens
            .values()
            .filter(|s| s.size.x > 0.0)
            .count()
    }

    /// Re-check the `spawn` barrier (on commits, and on its own timer).
    pub fn poll_window_wait(&mut self) {
        let Some(Wait::Window { had, until }) = self.script.as_ref().map(|s| s.wait) else {
            return;
        };
        let now = self.now();
        if self.mapped() > had || now >= until {
            if now >= until {
                eprintln!("nkw: spawn: no new window after 5 s, going on");
            }
            self.script.as_mut().unwrap().wait = Wait::None;
            // let the new window's first frame land before the next line
            let _ = self.lh.insert_source(
                Timer::from_duration(Duration::from_millis(50)),
                |_, _, st: &mut Nkw| {
                    st.script_step();
                    TimeoutAction::Drop
                },
            );
            return;
        }
        let _ = self.lh.insert_source(
            Timer::from_duration(Duration::from_millis(20)),
            |_, _, st: &mut Nkw| {
                st.poll_window_wait();
                TimeoutAction::Drop
            },
        );
    }

    /// Take a shot once the camera stopped gliding.
    fn shot_when_settled(&mut self, p: PathBuf) {
        if self.world.gliding() {
            let _ = self.lh.insert_source(
                Timer::from_duration(Duration::from_millis(16)),
                move |_, _, st: &mut Nkw| {
                    st.shot_when_settled(p.clone());
                    TimeoutAction::Drop
                },
            );
            return;
        }
        if let Some(s) = self.script.as_mut() {
            s.wait = Wait::Shot;
        }
        self.shots.push(p);
        self.request_frame();
    }
}

/// `testclient` names the sibling `nkw-testclient` binary when it exists.
fn resolve(cmd: &str) -> PathBuf {
    if cmd == "testclient" || cmd == "nkw-testclient" {
        if let Some(p) = std::env::current_exe()
            .ok()
            .and_then(|e| e.parent().map(|d| d.join("nkw-testclient")))
            .filter(|p| p.exists())
        {
            return p;
        }
        return "nkw-testclient".into();
    }
    cmd.into()
}

/// SIGINT/SIGTERM as a file descriptor (so the loop exits cleanly and the
/// stats summary is printed).
fn signal_fd() -> Option<OwnedFd> {
    // SAFETY: plain libc calls on a local sigset.
    unsafe {
        let mut set: libc::sigset_t = std::mem::zeroed();
        libc::sigemptyset(&mut set);
        libc::sigaddset(&mut set, libc::SIGINT);
        libc::sigaddset(&mut set, libc::SIGTERM);
        libc::pthread_sigmask(libc::SIG_BLOCK, &set, std::ptr::null_mut());
        let fd = libc::signalfd(-1, &set, libc::SFD_CLOEXEC | libc::SFD_NONBLOCK);
        (fd >= 0).then(|| OwnedFd::from_raw_fd(fd))
    }
}

/// In a child, before exec: undo the blocked mask `signal_fd` set up.
fn unblock_signals() {
    // SAFETY: async-signal-safe libc calls on a local sigset.
    unsafe {
        let mut set: libc::sigset_t = std::mem::zeroed();
        libc::sigemptyset(&mut set);
        libc::sigaddset(&mut set, libc::SIGINT);
        libc::sigaddset(&mut set, libc::SIGTERM);
        libc::pthread_sigmask(libc::SIG_UNBLOCK, &set, std::ptr::null_mut());
    }
}

struct Started {
    host: Host,
    output: Output,
    size: (u32, u32),
    scale: f64,
}

fn output_for(size: (i32, i32), scale: f64, transform: Transform) -> Output {
    let output = Output::new(
        "nkw".to_string(),
        PhysicalProperties {
            size: (0, 0).into(),
            subpixel: Subpixel::Unknown,
            make: "nekuroddo".into(),
            model: "world".into(),
            serial_number: "0".into(),
        },
    );
    let mode = Mode {
        size: size.into(),
        refresh: 60_000,
    };
    output.change_current_state(
        Some(mode),
        Some(transform),
        Some(Scale::Fractional(scale)),
        Some((0, 0).into()),
    );
    output.set_preferred(mode);
    output
}

fn start_winit(cli: &Cli) -> Result<(Started, winit::WinitEventLoop), Fail> {
    let (w, h) = cli.size.unwrap_or((1280, 800));
    let attrs = WindowAttributes::default()
        .with_title("nkw")
        .with_surface_size(LogicalSize::new(w as f64, h as f64));
    let (backend, events) = winit::init_from_attributes::<GlowRenderer>(attrs)
        .map_err(|e| (exit::GL, format!("winit/EGL: {e}")))?;
    let size = backend.window_size();
    let scale = cli.scale.unwrap_or_else(|| backend.scale_factor());
    let output = output_for((size.w, size.h), scale, Transform::Flipped180);
    Ok((
        Started {
            host: Host::Winit(Box::new(backend)),
            output,
            size: (size.w as u32, size.h as u32),
            scale,
        },
        events,
    ))
}

fn start_headless(cli: &Cli) -> Result<Started, Fail> {
    let gl = |e: &dyn std::fmt::Display| (exit::GL, format!("headless EGL: {e}"));
    let (w, h) = cli.size_or_default();
    let display = unsafe { EGLDisplay::new(EGLSurfacelessDisplay) }.map_err(|e| gl(&e))?;
    let ctx = EGLContext::new(&display).map_err(|e| gl(&e))?;
    let mut renderer = unsafe { GlowRenderer::new(ctx) }.map_err(|e| gl(&e))?;
    let target = Offscreen::<smithay::backend::renderer::gles::GlesTexture>::create_buffer(
        &mut renderer,
        Fourcc::Abgr8888,
        (w as i32, h as i32).into(),
    )
    .map_err(|e| gl(&e))?;
    let scale = cli.scale.unwrap_or(1.0);
    let output = output_for((w as i32, h as i32), scale, Transform::Normal);
    Ok(Started {
        host: Host::Headless {
            renderer: Box::new(renderer),
            target,
        },
        output,
        size: (w, h),
        scale,
    })
}

pub fn run(cli: Cli, script: Option<Vec<Line>>) -> Result<i32, Fail> {
    let mut event_loop: EventLoop<'static, Nkw> =
        EventLoop::try_new().map_err(|e| (exit::GL, format!("event loop: {e}")))?;
    let display: Display<Nkw> =
        Display::new().map_err(|e| (exit::SOCKET, format!("display: {e}")))?;
    let dh = display.handle();

    if std::env::var_os("XDG_RUNTIME_DIR").is_none() {
        return Err((exit::SOCKET, "XDG_RUNTIME_DIR is not set".into()));
    }
    let listening = ListeningSocketSource::with_name(&cli.socket)
        .map_err(|e| (exit::SOCKET, format!("socket {}: {e}", cli.socket)))?;
    let socket = listening.socket_name().to_os_string();

    let (started, winit_events) = if cli.headless {
        (start_headless(&cli)?, None)
    } else {
        let (s, e) = start_winit(&cli)?;
        (s, Some(e))
    };
    let Started {
        mut host,
        output,
        size,
        scale,
    } = started;
    let _output_global = output.create_global::<Nkw>(&dh);

    let renderer: &mut GlowRenderer = match &mut host {
        Host::Winit(b) => b.renderer(),
        Host::Headless { renderer, .. } => renderer,
    };
    // linux-dmabuf v4 with the render node's default feedback (else v3)
    let mut dmabuf_state = DmabufState::new();
    let formats = renderer.dmabuf_formats();
    let node = EGLDevice::device_for_display(renderer.egl_context().display())
        .ok()
        .and_then(|d| d.render_device_path().or_else(|_| d.drm_device_path()).ok())
        .and_then(|p| std::fs::metadata(p).ok())
        .map(|m| std::os::unix::fs::MetadataExt::rdev(&m) as libc::dev_t);
    let dmabuf_global =
        match node.and_then(|n| DmabufFeedbackBuilder::new(n, formats.clone()).build().ok()) {
            Some(fb) => dmabuf_state.create_global_with_default_feedback::<Nkw>(&dh, &fb),
            None => dmabuf_state.create_global::<Nkw>(&dh, formats),
        };
    let shm_formats: Vec<_> = renderer.shm_formats().collect();

    let damage = OutputDamageTracker::from_output(&output);
    let gfx = Gfx::new(host, damage, cli.gl_debug, cli.preset64).map_err(|e| (exit::GL, e))?;

    let mut seat_state = SeatState::new();
    let mut seat = seat_state.new_wl_seat(&dh, "nkw");
    seat.add_keyboard(XkbConfig::default(), 200, 25)
        .map_err(|e| (exit::USAGE, format!("keymap (XKB_DEFAULT_*): {e:?}")))?;
    seat.add_pointer();
    let mut shm_state = ShmState::new::<Nkw>(&dh, vec![]);
    shm_state.update_formats(shm_formats);

    let out = nkw_core::Output {
        width_px: size.0,
        height_px: size.1,
        scale: scale as f32,
    };
    let opts = Opts {
        reduced_motion: cli.reduced_motion,
    };
    let world = if cli.demo {
        crate::demo::world(out, opts)
    } else {
        World::new(out, opts)
    };

    let lh = event_loop.handle();
    let mut st = Nkw {
        dh: dh.clone(),
        lh: lh.clone(),
        signal: event_loop.get_signal(),
        socket,
        clock0: Instant::now(),
        compositor_state: CompositorState::new::<Nkw>(&dh),
        xdg_shell_state: XdgShellState::new::<Nkw>(&dh),
        shm_state,
        output_manager_state: OutputManagerState::new_with_xdg_output::<Nkw>(&dh),
        seat_state,
        data_device_state: DataDeviceState::new::<Nkw>(&dh),
        decoration_state: XdgDecorationState::new::<Nkw>(&dh),
        viewporter_state: smithay::wayland::viewporter::ViewporterState::new::<Nkw>(&dh),
        dmabuf: Some((dmabuf_state, dmabuf_global)),
        popups: PopupManager::default(),
        seat,
        windows: Vec::new(),
        world,
        output,
        gfx,
        sched: Sched {
            period: 1.0 / 60.0,
            ..Sched::default()
        },
        intercepted: HashSet::new(),
        stats: Stats::from_env(std::env::var("NKW_STATS").ok().as_deref()),
        exit: None,
        shots: Vec::new(),
        children: Vec::new(),
        pointer: (0.0, 0.0).into(),
        script: None,
        last_sym: 0,
        cli: cli.clone(),
    };

    lh.insert_source(listening, |stream, _, st: &mut Nkw| {
        if let Err(e) = st.dh.insert_client(stream, client_data()) {
            eprintln!("nkw: client: {e}");
        }
    })
    .map_err(|e| (exit::SOCKET, format!("socket source: {e}")))?;
    lh.insert_source(
        Generic::new(display, Interest::READ, CMode::Level),
        |_, display, st: &mut Nkw| {
            // SAFETY: the display is not dropped while the source lives
            let r = unsafe { display.get_mut().dispatch_clients(st) };
            if let Err(e) = r {
                eprintln!("nkw: dispatch: {e}");
            }
            st.poll_window_wait();
            Ok(PostAction::Continue)
        },
    )
    .map_err(|e| (exit::SOCKET, format!("display source: {e}")))?;

    if let Some(events) = winit_events {
        lh.insert_source(events, |event, _, st: &mut Nkw| match event {
            WinitEvent::Resized { size, scale_factor } => {
                let scale = st.cli.scale.unwrap_or(scale_factor);
                let mode = Mode {
                    size,
                    refresh: 60_000,
                };
                st.output.change_current_state(
                    Some(mode),
                    None,
                    Some(Scale::Fractional(scale)),
                    None,
                );
                st.world.out = nkw_core::Output {
                    width_px: size.w as u32,
                    height_px: size.h as u32,
                    scale: scale as f32,
                };
                st.sched.full = true;
                st.world.want(nkw_core::Wish::world(nkw_core::Want::Once));
                st.request_frame();
            }
            WinitEvent::Input(ev) => st.process_input(ev),
            WinitEvent::Redraw => {
                st.frame();
                st.after_frame();
            }
            WinitEvent::CloseRequested => st.quit(exit::OK),
            WinitEvent::Focus(_) => {}
        })
        .map_err(|e| (exit::GL, format!("winit source: {e}")))?;
    }

    if let Some(fd) = signal_fd() {
        lh.insert_source(
            Generic::new(fd, Interest::READ, CMode::Level),
            |_, fd, st: &mut Nkw| {
                let mut buf = [0u8; 128];
                // SAFETY: reading a signalfd_siginfo into a local buffer
                unsafe { libc::read(fd.as_fd().as_raw_fd(), buf.as_mut_ptr().cast(), buf.len()) };
                st.quit(exit::OK);
                Ok(PostAction::Continue)
            },
        )
        .map_err(|e| (exit::SOCKET, format!("signals: {e}")))?;
    }

    if let Some(s) = cli.seconds {
        lh.insert_source(
            Timer::from_duration(Duration::from_secs_f64(s)),
            |_, _, st: &mut Nkw| {
                st.quit(exit::OK);
                TimeoutAction::Drop
            },
        )
        .map_err(|e| (exit::SOCKET, format!("timer: {e}")))?;
    }

    let keymap = {
        let kb = st.seat.get_keyboard().unwrap();
        kb.with_xkb_state(&mut st, |ctx| ctx.xkb().lock().unwrap().keymap_as_string())
    };
    if let Some(lines) = script {
        st.script = Some(Script {
            lines,
            pc: 0,
            keys: KeyTable::from_string(&keymap),
            wait: Wait::None,
        });
    }

    if !st.cli.cmd.is_empty() {
        let cmd = st.cli.cmd.clone();
        st.spawn(&cmd);
    }

    // the first frame, then the script
    st.world.want(nkw_core::Wish::world(nkw_core::Want::Once));
    st.request_frame();
    if st.script.is_some() {
        st.script_step();
    } else if st.cli.headless && st.cli.shot.is_some() {
        st.finish();
    }

    let signal = event_loop.get_signal();
    event_loop
        .run(None, &mut st, |st| {
            // every reply of this loop iteration goes out now
            let _ = st.dh.flush_clients();
            if st.exit.is_some() {
                signal.stop();
            }
        })
        .map_err(|e| (exit::SOCKET, format!("event loop: {e}")))?;

    st.stats.print_summary();
    for c in &mut st.children {
        let _ = c.kill();
        let _ = c.wait();
    }
    if st.gfx.gl_errors > 0 {
        eprintln!("nkw: {} GL errors", st.gfx.gl_errors);
    }
    Ok(st.exit.unwrap_or(exit::OK))
}
