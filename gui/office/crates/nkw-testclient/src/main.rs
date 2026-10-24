//! nkw-testclient: one xdg toplevel painted with a solid colour or a 1 px
//! checker, logging configure/enter/leave/key/modifiers/frame/popup events as
//! JSON lines (contract S3, wave-2 additions in lib.rs). Used by nkw's
//! integration tests and perf runs.

use std::io::{Read, Write};
use std::os::fd::OwnedFd;
use std::time::Duration;

use calloop::timer::{TimeoutAction, Timer};
use calloop::{EventLoop, LoopHandle, PostAction};
use nkw_testclient::{
    Args, CHILD_AFTER_MS, Event, Fill, KeyState, Log, MOTHER_FILL, TEXT_MIMES, USAGE, channel_u32,
    child_argv, cursor_shape_code, damage_rect, paint, paint_rect, parse_args, physical_size,
    pick_text_mime, presented,
};
use smithay_client_toolkit::activation::{ActivationHandler, ActivationState, RequestData};
use smithay_client_toolkit::reexports::protocols::wp::fractional_scale::v1::client::{
    wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1,
    wp_fractional_scale_v1::{self, WpFractionalScaleV1},
};
use smithay_client_toolkit::reexports::protocols::wp::viewporter::client::{
    wp_viewport::WpViewport, wp_viewporter::WpViewporter,
};
use smithay_client_toolkit::primary_selection::{
    PrimarySelectionManagerState,
    device::{PrimarySelectionDevice, PrimarySelectionDeviceHandler},
    selection::{PrimarySelectionSource, PrimarySelectionSourceHandler},
};
use smithay_client_toolkit::reexports::protocols::wp::content_type::v1::client::{
    wp_content_type_manager_v1::WpContentTypeManagerV1,
    wp_content_type_v1::{self, WpContentTypeV1},
};
use smithay_client_toolkit::reexports::protocols::wp::cursor_shape::v1::client::{
    wp_cursor_shape_device_v1::{self, WpCursorShapeDeviceV1},
    wp_cursor_shape_manager_v1::WpCursorShapeManagerV1,
};
use smithay_client_toolkit::reexports::protocols::wp::idle_inhibit::zv1::client::{
    zwp_idle_inhibit_manager_v1::ZwpIdleInhibitManagerV1, zwp_idle_inhibitor_v1::ZwpIdleInhibitorV1,
};
use smithay_client_toolkit::reexports::protocols::wp::pointer_constraints::zv1::client::{
    zwp_confined_pointer_v1::ZwpConfinedPointerV1, zwp_locked_pointer_v1::ZwpLockedPointerV1,
    zwp_pointer_constraints_v1::Lifetime,
};
use smithay_client_toolkit::reexports::protocols::wp::presentation_time::client::{
    wp_presentation::WpPresentation,
    wp_presentation_feedback::{self, WpPresentationFeedback},
};
use smithay_client_toolkit::reexports::protocols::wp::relative_pointer::zv1::client::zwp_relative_pointer_v1::ZwpRelativePointerV1;
use smithay_client_toolkit::reexports::protocols::wp::single_pixel_buffer::v1::client::wp_single_pixel_buffer_manager_v1::WpSinglePixelBufferManagerV1;
use smithay_client_toolkit::reexports::protocols::xdg::foreign::zv2::client::{
    zxdg_exported_v2::{self, ZxdgExportedV2},
    zxdg_exporter_v2::ZxdgExporterV2,
};
use smithay_client_toolkit::reexports::protocols::xdg::shell::client::xdg_positioner;
use smithay_client_toolkit::seat::pointer::{PointerEvent, PointerEventKind, PointerHandler};
use smithay_client_toolkit::seat::pointer_constraints::{
    PointerConstraintsHandler, PointerConstraintsState,
};
use smithay_client_toolkit::seat::relative_pointer::{
    RelativeMotionEvent, RelativePointerHandler, RelativePointerState,
};
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState, FrameCallbackData},
    data_device_manager::{
        DataDeviceManagerState, WritePipe,
        data_device::{DataDevice, DataDeviceHandler},
        data_offer::{DataOfferHandler, DragOffer},
        data_source::{CopyPasteSource, DataSourceHandler},
    },
    dispatch2::Dispatch2,
    output::{OutputHandler, OutputState},
    reexports::calloop_wayland_source::WaylandSource,
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{
        Capability, SeatHandler, SeatState,
        keyboard::{KeyEvent, KeyboardHandler, Keysym, Modifiers, RawModifiers},
    },
    shell::{
        WaylandSurface,
        xdg::{
            XdgPositioner, XdgShell, XdgSurface,
            popup::{Popup, PopupConfigure, PopupHandler},
            window::{Window, WindowConfigure, WindowDecorations, WindowHandler},
        },
    },
    shm::{
        Shm, ShmHandler,
        slot::{Buffer, SlotPool},
    },
};
use wayland_client::{
    Connection, Proxy, QueueHandle,
    globals::{GlobalList, registry_queue_init},
    protocol::{
        wl_data_device::WlDataDevice, wl_data_device_manager::DndAction,
        wl_buffer::WlBuffer, wl_data_source::WlDataSource, wl_keyboard, wl_output, wl_pointer,
        wl_seat, wl_shm, wl_surface,
    },
};

const POPUP_SIZE: (u32, u32) = (64, 32);
const PARENT: Option<&str> = Some("parent");

fn main() {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    if argv.iter().any(|a| a == "-h" || a == "--help") {
        println!("{USAGE}");
        return;
    }
    let args = match parse_args(&argv) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("nkw-testclient: {e}\n{USAGE}");
            std::process::exit(2);
        }
    };
    // the token is one-shot and must not leak into children (xdg-activation)
    let token = if args.activation_token {
        let t = std::env::var("XDG_ACTIVATION_TOKEN")
            .ok()
            .filter(|t| !t.is_empty());
        // SAFETY: single-threaded, before any thread or child exists
        unsafe { std::env::remove_var("XDG_ACTIVATION_TOKEN") };
        Some(t)
    } else {
        None
    };
    if let Err(e) = run(args, &argv, token) {
        eprintln!("nkw-testclient: {e}");
        std::process::exit(1);
    }
}

/// One toplevel: the main window, or the `--parent-of` mother.
struct Win {
    window: Window,
    /// `None` = the main window; `Some("parent")` = the mother.
    tag: Option<&'static str>,
    fill: Fill,
    /// Logical size (the configure's).
    size: (u32, u32),
    buffer: Option<Buffer>,
    configured: bool,
    frames: u64,
    /// The next commit damages the whole buffer (first draw, resize, rescale).
    full_damage: bool,
    /// `--single-pixel`: the one 1×1 buffer, attached at every draw.
    pixel: Option<WlBuffer>,
}

/// The wave-4 protocol objects (each `Some` only when its flag asked for it
/// and the compositor has the global).
#[derive(Default)]
struct Extra {
    pointer: Option<wl_pointer::WlPointer>,
    cursor_mgr: Option<WpCursorShapeManagerV1>,
    cursor_dev: Option<WpCursorShapeDeviceV1>,
    primary_mgr: Option<PrimarySelectionManagerState>,
    primary_dev: Option<PrimarySelectionDevice>,
    primary_src: Option<PrimarySelectionSource>,
    primary_done: bool,
    presentation: Option<WpPresentation>,
    constraints: Option<PointerConstraintsState>,
    relative: Option<RelativePointerState>,
    locked: Option<ZwpLockedPointerV1>,
    rel_ptr: Option<ZwpRelativePointerV1>,
    single_pixel: Option<WpSinglePixelBufferManagerV1>,
    content_mgr: Option<WpContentTypeManagerV1>,
    content: Option<WpContentTypeV1>,
    exporter: Option<ZxdgExporterV2>,
    exported: Option<ZxdgExportedV2>,
    idle_mgr: Option<ZwpIdleInhibitManagerV1>,
    inhibitor: Option<ZwpIdleInhibitorV1>,
}

/// User data of the protocol objects SCTK has no helper for.
struct Proto;

struct Client {
    args: Args,
    log: Log,
    lh: LoopHandle<'static, Client>,
    registry_state: RegistryState,
    seat_state: SeatState,
    output_state: OutputState,
    compositor: CompositorState,
    xdg_shell: XdgShell,
    shm: Shm,
    pool: SlotPool,
    /// `None` while a `--parent-of` mother waits for its first configure.
    main: Option<Win>,
    mother: Option<Win>,
    data_mgr: Option<DataDeviceManagerState>,
    data_device: Option<DataDevice>,
    copy_source: Option<CopyPasteSource>,
    copy_done: bool,
    fs_mgr: Option<WpFractionalScaleManagerV1>,
    viewporter: Option<WpViewporter>,
    viewport: Option<WpViewport>,
    fractional: Option<WpFractionalScaleV1>,
    scale_120: u32,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    /// Whether keyboard focus is on the mother (key lines get tagged).
    kb_on_mother: bool,
    seat: Option<wl_seat::WlSeat>,
    popup: Option<Popup>,
    popup_opened: bool,
    exit: bool,
    /// `--activation-token`: the global, and the env's token (`Some(None)`:
    /// the flag without a token); taken at the first configure
    activation: Option<ActivationState>,
    token: Option<Option<String>>,
    /// `--spawn-child`
    child: Option<std::process::Child>,
    x: Extra,
}

fn run(
    args: Args,
    argv: &[String],
    token: Option<Option<String>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let log = Log::open(&args.log)?;
    let conn = Connection::connect_to_env()?;
    let (globals, queue) = registry_queue_init(&conn)?;
    if args.list_globals {
        let mut log = log;
        let mut all = globals.contents().clone_list();
        all.sort_by(|a, b| a.interface.cmp(&b.interface));
        for g in all {
            log.emit(Event::Global {
                name: g.interface,
                version: g.version,
            });
        }
        return Ok(());
    }
    let qh = queue.handle();
    let mut event_loop: EventLoop<Client> = EventLoop::try_new()?;
    WaylandSource::new(conn.clone(), queue)
        .insert(event_loop.handle())
        .map_err(|e| e.error)?;

    let compositor = CompositorState::bind(&globals, &qh)?;
    let xdg_shell = XdgShell::bind(&globals, &qh)?;
    let shm = Shm::bind(&globals, &qh)?;
    let data_mgr = (args.copy.is_some() || args.log_selection)
        .then(|| DataDeviceManagerState::bind(&globals, &qh).ok())
        .flatten();
    let (fs_mgr, mut viewporter) = if args.fractional {
        bind_fractional(&globals, &qh)
    } else {
        (None, None)
    };
    let x = bind_extra(&args, &globals, &qh);
    if args.single_pixel.is_some() && viewporter.is_none() {
        // the 1×1 buffer is stretched to the window by a viewport
        viewporter = globals.bind(&qh, 1..=1, Proto).ok();
    }

    let activation = token
        .is_some()
        .then(|| ActivationState::bind(&globals, &qh).ok())
        .flatten();
    let (w, h) = args.size;
    let pool = SlotPool::new((w * h * 4) as usize, &shm)?;
    if args.spawn_child {
        let child = child_argv(argv);
        event_loop
            .handle()
            .insert_source(
                Timer::from_duration(Duration::from_millis(CHILD_AFTER_MS)),
                move |_, _, c: &mut Client| {
                    c.spawn_child(&child);
                    TimeoutAction::Drop
                },
            )
            .map_err(|e| e.error)?;
    }

    if let Some(ms) = args.exit_after_ms {
        event_loop
            .handle()
            .insert_source(
                Timer::from_duration(Duration::from_millis(ms)),
                |_, _, c| {
                    c.exit = true;
                    TimeoutAction::Drop
                },
            )
            .map_err(|e| e.error)?;
    }

    let mut client = Client {
        lh: event_loop.handle(),
        log,
        registry_state: RegistryState::new(&globals),
        seat_state: SeatState::new(&globals, &qh),
        output_state: OutputState::new(&globals, &qh),
        compositor,
        xdg_shell,
        shm,
        pool,
        main: None,
        mother: None,
        data_mgr,
        data_device: None,
        copy_source: None,
        copy_done: false,
        fs_mgr,
        viewporter,
        viewport: None,
        fractional: None,
        scale_120: 120,
        keyboard: None,
        kb_on_mother: false,
        seat: None,
        popup: None,
        popup_opened: false,
        exit: false,
        activation,
        token,
        child: None,
        x,
        args,
    };
    match client.args.parent_of.clone() {
        Some(app_id) => client.make_mother(&qh, &app_id),
        None => client.make_main(&qh, None),
    }
    while !client.exit {
        // No timeout: the client only wakes for Wayland events or its own timer.
        event_loop.dispatch(None, &mut client)?;
    }
    if let Some(mut c) = client.child.take() {
        let _ = c.kill();
        let _ = c.wait();
    }
    Ok(())
}

fn bind_fractional(
    globals: &GlobalList,
    qh: &QueueHandle<Client>,
) -> (Option<WpFractionalScaleManagerV1>, Option<WpViewporter>) {
    let fs = globals.bind(qh, 1..=1, Proto).ok();
    let vp = globals.bind(qh, 1..=1, Proto).ok();
    match (fs, vp) {
        (Some(fs), Some(vp)) => (Some(fs), Some(vp)),
        _ => {
            // Without both globals the client renders at scale 1, like any old client.
            eprintln!("nkw-testclient: --fractional: no wp_fractional_scale_v1/wp_viewporter");
            (None, None)
        }
    }
}

/// The wave-4 globals the flags ask for (a missing one is reported once and
/// the flag does nothing, like `--fractional` without its globals). Nothing
/// is bound without its flag.
fn bind_extra(args: &Args, globals: &GlobalList, qh: &QueueHandle<Client>) -> Extra {
    fn want<T>(flag: bool, name: &str, bind: impl FnOnce() -> Option<T>) -> Option<T> {
        if !flag {
            return None;
        }
        let r = bind();
        if r.is_none() {
            eprintln!("nkw-testclient: no {name}");
        }
        r
    }
    let primary = args.primary.is_some() || args.log_primary;
    Extra {
        cursor_mgr: want(
            args.cursor_shape.is_some(),
            "wp_cursor_shape_manager_v1",
            || globals.bind(qh, 1..=2, Proto).ok(),
        ),
        primary_mgr: want(primary, "zwp_primary_selection_device_manager_v1", || {
            PrimarySelectionManagerState::bind(globals, qh).ok()
        }),
        presentation: want(args.presentation, "wp_presentation", || {
            globals.bind(qh, 1..=2, Proto).ok()
        }),
        single_pixel: want(
            args.single_pixel.is_some(),
            "wp_single_pixel_buffer_manager_v1",
            || globals.bind(qh, 1..=1, Proto).ok(),
        ),
        content_mgr: want(
            args.content_type.is_some(),
            "wp_content_type_manager_v1",
            || globals.bind(qh, 1..=1, Proto).ok(),
        ),
        exporter: want(args.export, "zxdg_exporter_v2", || {
            globals.bind(qh, 1..=1, Proto).ok()
        }),
        idle_mgr: want(args.idle_inhibit, "zwp_idle_inhibit_manager_v1", || {
            globals.bind(qh, 1..=1, Proto).ok()
        }),
        // SCTK's states keep "not present" themselves; locking reports it
        constraints: args
            .lock_pointer
            .then(|| PointerConstraintsState::bind(globals, qh)),
        relative: args
            .lock_pointer
            .then(|| RelativePointerState::bind(globals, qh)),
        ..Extra::default()
    }
}

fn new_win(window: Window, tag: Option<&'static str>, fill: Fill, size: (u32, u32)) -> Win {
    Win {
        window,
        tag,
        fill,
        size,
        buffer: None,
        configured: false,
        frames: 0,
        full_damage: true,
        pixel: None,
    }
}

impl Client {
    /// `--spawn-child`: this binary again, as our child; it dies with us.
    fn spawn_child(&mut self, argv: &[String]) {
        let Ok(exe) = std::env::current_exe() else {
            return;
        };
        let mut cmd = std::process::Command::new(exe);
        cmd.args(argv);
        // SAFETY: prctl is async-signal-safe
        unsafe {
            std::os::unix::process::CommandExt::pre_exec(&mut cmd, || {
                libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
                Ok(())
            });
        }
        match cmd.spawn() {
            Ok(c) => {
                self.log.emit(Event::SpawnedChild { pid: c.id() });
                self.child = Some(c);
            }
            Err(e) => eprintln!("nkw-testclient: --spawn-child: {e}"),
        }
    }

    /// `--activation-token`: activate the main window once it is mapped.
    fn activate_once(&mut self) {
        let Some(token) = self.token.take() else {
            return;
        };
        if let (Some(t), Some(a), Some(main)) = (&token, &self.activation, &self.main) {
            a.activate::<Client>(main.window.wl_surface(), t.clone());
        }
        self.log.emit(Event::Activation {
            token: token.is_some() && self.activation.is_some(),
        });
    }

    fn make_mother(&mut self, qh: &QueueHandle<Self>, app_id: &str) {
        let surface = self.compositor.create_surface(qh);
        let window = self
            .xdg_shell
            .create_window(surface, WindowDecorations::ServerDefault, qh);
        window.set_title(app_id.to_string());
        window.set_app_id(app_id.to_string());
        window.commit();
        self.mother = Some(new_win(window, PARENT, MOTHER_FILL, self.args.size));
    }

    fn make_main(&mut self, qh: &QueueHandle<Self>, parent: Option<&Window>) {
        let surface = self.compositor.create_surface(qh);
        if let (Some(fs), Some(vp)) = (&self.fs_mgr, &self.viewporter) {
            self.fractional = Some(fs.get_fractional_scale(&surface, qh, Proto));
            self.viewport = Some(vp.get_viewport(&surface, qh, Proto));
        } else if let (Some(_), Some(vp)) = (self.args.single_pixel, &self.viewporter) {
            self.viewport = Some(vp.get_viewport(&surface, qh, Proto));
        }
        self.surface_extras(qh, &surface);
        let window = self
            .xdg_shell
            .create_window(surface, WindowDecorations::ServerDefault, qh);
        window.set_title(self.args.title.clone());
        window.set_app_id(self.args.app_id.clone());
        if parent.is_some() {
            window.set_parent(parent);
        }
        if self.args.fullscreen {
            window.set_fullscreen(None);
        }
        // The first commit carries no buffer; the configure answers it.
        window.commit();
        self.main = Some(new_win(window, None, self.args.fill, self.args.size));
    }

    /// `--content-type`, `--idle-inhibit`: state of the main surface, set
    /// before its first commit (double-buffered: the first buffer carries it).
    fn surface_extras(&mut self, qh: &QueueHandle<Self>, surface: &wl_surface::WlSurface) {
        if let (Some(kind), Some(mgr)) = (self.args.content_type, &self.x.content_mgr) {
            let ct = mgr.get_surface_content_type(surface, qh, Proto);
            if let Ok(t) = wp_content_type_v1::Type::try_from(kind.code()) {
                ct.set_content_type(t);
            }
            self.x.content = Some(ct);
            self.log.emit(Event::ContentType { kind: kind.code() });
        }
        if let Some(mgr) = &self.x.idle_mgr {
            self.x.inhibitor = Some(mgr.create_inhibitor(surface, qh, Proto));
            self.log.emit(Event::IdleInhibit);
        }
    }

    /// `--export`: once the toplevel exists (after its first configure).
    fn export_once(&mut self, qh: &QueueHandle<Self>) {
        if self.x.exported.is_some() {
            return;
        }
        if let (Some(e), Some(main)) = (&self.x.exporter, &self.main) {
            self.x.exported = Some(e.export_toplevel(main.window.wl_surface(), qh, Proto));
        }
    }

    /// `--primary`: offer the text with the serial of the first keyboard enter.
    fn offer_primary(&mut self, qh: &QueueHandle<Self>, serial: u32) {
        if self.x.primary_done || self.args.primary.is_none() {
            return;
        }
        let (Some(mgr), Some(dev)) = (&self.x.primary_mgr, &self.x.primary_dev) else {
            return;
        };
        let src = mgr.create_selection_source(qh, TEXT_MIMES);
        src.set_selection(dev, serial);
        self.x.primary_src = Some(src);
        self.x.primary_done = true;
        self.log.emit(Event::PrimarySet);
    }

    /// The pointer entered the main window: `--cursor-shape`, `--lock-pointer`.
    fn on_pointer_enter(&mut self, qh: &QueueHandle<Self>, serial: u32) {
        self.log.emit(Event::PointerEnter);
        if let (Some(name), Some(dev)) = (&self.args.cursor_shape, &self.x.cursor_dev) {
            let code = cursor_shape_code(name).unwrap_or(1);
            if let Ok(shape) = wp_cursor_shape_device_v1::Shape::try_from(code) {
                dev.set_shape(serial, shape);
                self.log.emit(Event::CursorShapeSet { shape: code });
            }
        }
        if self.args.lock_pointer && self.x.locked.is_none() {
            let (Some(c), Some(p), Some(main)) = (&self.x.constraints, &self.x.pointer, &self.main)
            else {
                return;
            };
            match c.lock_pointer(main.window.wl_surface(), p, None, Lifetime::Persistent, qh) {
                Ok(l) => self.x.locked = Some(l),
                Err(_) => eprintln!("nkw-testclient: no zwp_pointer_constraints_v1"),
            }
        }
    }

    fn is_main(&self, s: &wl_surface::WlSurface) -> bool {
        self.main
            .as_ref()
            .is_some_and(|w| w.window.wl_surface() == s)
    }

    fn is_mother(&self, s: &wl_surface::WlSurface) -> bool {
        self.mother
            .as_ref()
            .is_some_and(|w| w.window.wl_surface() == s)
    }

    /// Draws the main window (`mother = false`) or the mother.
    fn draw(&mut self, qh: &QueueHandle<Self>, mother: bool) {
        let slot = if mother {
            &mut self.mother
        } else {
            &mut self.main
        };
        let Some(mut win) = slot.take() else { return };
        self.draw_win(qh, &mut win);
        let slot = if mother {
            &mut self.mother
        } else {
            &mut self.main
        };
        *slot = Some(win);
    }

    fn draw_win(&mut self, qh: &QueueHandle<Self>, win: &mut Win) {
        let is_main = win.tag.is_none();
        if is_main && let Some(fb) = &self.x.presentation {
            // one feedback per commit: `presented` or `discarded` comes back
            fb.feedback(win.window.wl_surface(), qh, Proto);
        }
        if is_main
            && let (Some([r, g, b, a]), Some(spb), Some(vp)) =
                (self.args.single_pixel, &self.x.single_pixel, &self.viewport)
        {
            let surface = win.window.wl_surface().clone();
            let buf = win.pixel.get_or_insert_with(|| {
                spb.create_u32_rgba_buffer(
                    channel_u32(r),
                    channel_u32(g),
                    channel_u32(b),
                    channel_u32(a),
                    qh,
                    Proto,
                )
            });
            surface.attach(Some(buf), 0, 0);
            surface.damage_buffer(0, 0, 1, 1);
            vp.set_destination(win.size.0 as i32, win.size.1 as i32);
            win.frames += 1;
            if !self.args.no_frame_wait {
                surface.frame(qh, FrameCallbackData(surface.clone()));
            }
            win.window.commit();
            return;
        }
        let viewport = if is_main {
            self.viewport.as_ref()
        } else {
            None
        };
        let (w, h) = match viewport {
            Some(_) => physical_size(win.size, self.scale_120),
            None => win.size,
        };
        let stride = w as i32 * 4;
        let damage = if is_main { self.args.damage } else { None };
        // Repaint only into a fresh buffer; a released one keeps its pixels.
        let reuse = win
            .buffer
            .as_ref()
            .is_some_and(|b| b.height() == h as i32 && b.stride() == stride)
            && win
                .buffer
                .as_mut()
                .is_some_and(|b| self.pool.canvas(b).is_some());
        if !reuse {
            let (buffer, canvas) = self
                .pool
                .create_buffer(w as i32, h as i32, stride, wl_shm::Format::Argb8888)
                .expect("create shm buffer");
            paint(canvas, w, h, win.fill);
            win.buffer = Some(buffer);
        }
        let surface = win.window.wl_surface().clone();
        match damage {
            Some(d) => {
                let odd = win.frames % 2 == 1;
                let buffer = win.buffer.as_mut().unwrap();
                if let Some(canvas) = self.pool.canvas(buffer) {
                    paint_rect(canvas, w, h, d, win.fill, odd);
                }
                if win.full_damage {
                    surface.damage_buffer(0, 0, w as i32, h as i32);
                } else {
                    let (rw, rh) = damage_rect(d, (w, h));
                    surface.damage_buffer(0, 0, rw as i32, rh as i32);
                }
            }
            None => surface.damage_buffer(0, 0, w as i32, h as i32),
        }
        win.full_damage = false;
        win.frames += 1;
        if is_main && !self.args.no_frame_wait {
            surface.frame(qh, FrameCallbackData(surface.clone()));
        }
        if let Some(vp) = viewport {
            vp.set_destination(win.size.0 as i32, win.size.1 as i32);
        }
        win.buffer
            .as_ref()
            .unwrap()
            .attach_to(&surface)
            .expect("attach buffer");
        win.window.commit();
    }

    fn open_popup(&mut self, qh: &QueueHandle<Self>, serial: u32) {
        self.popup_opened = true;
        let Some(main) = &self.main else { return };
        let Ok(positioner) = XdgPositioner::new(&self.xdg_shell) else {
            return;
        };
        positioner.set_size(POPUP_SIZE.0 as i32, POPUP_SIZE.1 as i32);
        positioner.set_anchor_rect(0, 0, 1, 1);
        positioner.set_anchor(xdg_positioner::Anchor::BottomRight);
        positioner.set_gravity(xdg_positioner::Gravity::BottomRight);
        let surface = self.compositor.create_surface(qh);
        let Ok(popup) = Popup::from_surface(
            Some(main.window.xdg_surface()),
            &positioner,
            qh,
            surface,
            &self.xdg_shell,
        ) else {
            return;
        };
        // A grabbed popup is the kind compositors dismiss when focus leaves.
        if let Some(seat) = &self.seat {
            popup.xdg_popup().grab(seat, serial);
        }
        popup.wl_surface().commit();
        self.popup = Some(popup);
    }

    /// `--copy`: offer the text with the serial of the first keyboard enter.
    fn offer_copy(&mut self, qh: &QueueHandle<Self>, serial: u32) {
        if self.copy_done || self.args.copy.is_none() {
            return;
        }
        let (Some(mgr), Some(dev)) = (&self.data_mgr, &self.data_device) else {
            return;
        };
        let source = mgr.create_copy_paste_source(qh, TEXT_MIMES);
        source.set_selection(dev, serial);
        self.copy_source = Some(source);
        self.copy_done = true;
        self.log.emit(Event::SelectionSet);
    }

    fn on_scale(&mut self, qh: &QueueHandle<Self>, scale_120: u32) {
        if scale_120 == self.scale_120 || scale_120 == 0 {
            return;
        }
        self.scale_120 = scale_120;
        self.log.emit(Event::Scale { scale_120 });
        if let Some(main) = &mut self.main
            && main.configured
        {
            main.full_damage = true;
            self.draw(qh, false);
        }
    }
}

impl Dispatch2<WpFractionalScaleManagerV1, Client> for Proto {
    fn event(
        &self,
        _: &mut Client,
        _: &WpFractionalScaleManagerV1,
        _: <WpFractionalScaleManagerV1 as Proxy>::Event,
        _: &Connection,
        _: &QueueHandle<Client>,
    ) {
    }
}

impl Dispatch2<WpFractionalScaleV1, Client> for Proto {
    fn event(
        &self,
        state: &mut Client,
        _: &WpFractionalScaleV1,
        ev: wp_fractional_scale_v1::Event,
        _: &Connection,
        qh: &QueueHandle<Client>,
    ) {
        if let wp_fractional_scale_v1::Event::PreferredScale { scale } = ev {
            state.on_scale(qh, scale);
        }
    }
}

impl Dispatch2<WpViewporter, Client> for Proto {
    fn event(
        &self,
        _: &mut Client,
        _: &WpViewporter,
        _: <WpViewporter as Proxy>::Event,
        _: &Connection,
        _: &QueueHandle<Client>,
    ) {
    }
}

impl Dispatch2<WpViewport, Client> for Proto {
    fn event(
        &self,
        _: &mut Client,
        _: &WpViewport,
        _: <WpViewport as Proxy>::Event,
        _: &Connection,
        _: &QueueHandle<Client>,
    ) {
    }
}

impl CompositorHandler for Client {
    fn scale_factor_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: i32,
    ) {
    }

    fn transform_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: wl_output::Transform,
    ) {
    }

    fn frame(&mut self, _: &Connection, qh: &QueueHandle<Self>, s: &wl_surface::WlSurface, _: u32) {
        if !self.is_main(s) {
            return;
        }
        self.log.emit(Event::Frame);
        self.draw(qh, false);
    }

    fn surface_enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }

    fn surface_leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }
}

impl OutputHandler for Client {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }
    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
}

impl WindowHandler for Client {
    fn request_close(&mut self, _: &Connection, _: &QueueHandle<Self>, w: &Window) {
        if self.is_mother(w.wl_surface()) {
            // Closing the mother closes the whole client (its dialog goes with it).
            self.log.emit_win(Event::Close, PARENT);
        } else {
            self.log.emit(Event::Close);
        }
        self.exit = true;
    }

    fn configure(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        window: &Window,
        cfg: WindowConfigure,
        _: u32,
    ) {
        let mother = self.is_mother(window.wl_surface());
        if !mother && !self.is_main(window.wl_surface()) {
            return;
        }
        let w = cfg.new_size.0.map_or(self.args.size.0, |v| v.get());
        let h = cfg.new_size.1.map_or(self.args.size.1, |v| v.get());
        let ev = Event::Configure {
            w,
            h,
            states: nkw_testclient::state_names(cfg.state.bits()),
        };
        let no_frame_wait = self.args.no_frame_wait;
        let win = if mother {
            self.log.emit_win(ev, PARENT);
            self.mother.as_mut()
        } else {
            self.log.emit(ev);
            self.main.as_mut()
        };
        let Some(win) = win else { return };
        let resized = (w, h) != win.size;
        win.size = (w, h);
        // With frame callbacks the loop is already running; draw only to start it,
        // or to show the new size now. The mother never asks for frames.
        let first = !win.configured;
        if first || resized || no_frame_wait || mother {
            win.configured = true;
            win.full_damage = true;
            self.draw(qh, mother);
        }
        if !mother && first {
            self.activate_once();
            self.export_once(qh);
        }
        if mother && first && self.main.is_none() {
            let parent = self.mother.as_ref().map(|m| m.window.clone());
            self.make_main(qh, parent.as_ref());
        }
    }
}

impl PopupHandler for Client {
    fn configure(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        popup: &Popup,
        c: PopupConfigure,
    ) {
        let (w, h) = (c.width.max(1) as u32, c.height.max(1) as u32);
        let Ok((buffer, canvas)) =
            self.pool
                .create_buffer(w as i32, h as i32, w as i32 * 4, wl_shm::Format::Argb8888)
        else {
            return;
        };
        paint(canvas, w, h, Fill::Color(0xffcc00));
        let s = popup.wl_surface();
        s.damage_buffer(0, 0, w as i32, h as i32);
        if buffer.attach_to(s).is_ok() {
            s.commit();
        }
    }

    fn done(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &Popup) {
        self.log.emit(Event::PopupDone);
        self.popup = None;
    }
}

impl SeatHandler for Client {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat_state
    }

    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}

    fn new_capability(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        cap: Capability,
    ) {
        if cap == Capability::Keyboard && self.keyboard.is_none() {
            // No repeat source: the log must hold exactly the keys the compositor sent.
            self.keyboard = self.seat_state.get_keyboard(qh, &seat, None).ok();
            // The data device exists before the first enter, so `--copy` has a serial.
            if self.data_device.is_none()
                && let Some(mgr) = &self.data_mgr
            {
                self.data_device = Some(mgr.get_data_device(qh, &seat));
            }
            if self.x.primary_dev.is_none()
                && let Some(mgr) = &self.x.primary_mgr
            {
                self.x.primary_dev = Some(mgr.get_selection_device(qh, &seat));
            }
            self.seat = Some(seat);
        } else if cap == Capability::Pointer
            && self.x.pointer.is_none()
            && (self.args.cursor_shape.is_some() || self.args.lock_pointer)
        {
            let Ok(p) = self.seat_state.get_pointer(qh, &seat) else {
                return;
            };
            if let Some(m) = &self.x.cursor_mgr {
                self.x.cursor_dev = Some(m.get_pointer(&p, qh, Proto));
            }
            if let Some(r) = &self.x.relative {
                self.x.rel_ptr = r.get_relative_pointer(&p, qh).ok();
            }
            self.x.pointer = Some(p);
        }
    }

    fn remove_capability(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: wl_seat::WlSeat,
        cap: Capability,
    ) {
        if cap == Capability::Keyboard
            && let Some(k) = self.keyboard.take()
        {
            k.release();
        }
    }

    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}

fn key_event(e: &KeyEvent, state: KeyState) -> Event {
    let sym: Keysym = e.keysym;
    Event::Key {
        keycode: e.raw_code,
        sym: nkw_testclient::sym_name(sym.name(), sym.raw()),
        state,
        utf8: e.utf8.clone().filter(|s| !s.is_empty()),
    }
}

impl Client {
    fn kb_tag(&self) -> Option<&'static str> {
        if self.kb_on_mother { PARENT } else { None }
    }
}

impl KeyboardHandler for Client {
    fn enter(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        s: &wl_surface::WlSurface,
        serial: u32,
        _: &[u32],
        _: &[Keysym],
    ) {
        if self.is_main(s) {
            self.kb_on_mother = false;
            self.log.emit(Event::Enter);
        } else if self.is_mother(s) {
            self.kb_on_mother = true;
            self.log.emit_win(Event::Enter, PARENT);
        } else {
            return;
        }
        self.offer_copy(qh, serial);
        self.offer_primary(qh, serial);
    }

    fn leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        s: &wl_surface::WlSurface,
        _: u32,
    ) {
        if self.is_main(s) {
            self.log.emit(Event::Leave);
        } else if self.is_mother(s) {
            self.log.emit_win(Event::Leave, PARENT);
        }
    }

    fn press_key(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        serial: u32,
        e: KeyEvent,
    ) {
        let tag = self.kb_tag();
        self.log.emit_win(key_event(&e, KeyState::Pressed), tag);
        if self.args.popup_on_key && !self.popup_opened {
            self.open_popup(qh, serial);
        }
    }

    fn repeat_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        _: KeyEvent,
    ) {
    }

    fn release_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        e: KeyEvent,
    ) {
        let tag = self.kb_tag();
        self.log.emit_win(key_event(&e, KeyState::Released), tag);
    }

    fn update_modifiers(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        m: Modifiers,
        _: RawModifiers,
        _: u32,
    ) {
        let tag = self.kb_tag();
        self.log.emit_win(
            Event::Modifiers {
                ctrl: m.ctrl,
                alt: m.alt,
                shift: m.shift,
                logo: m.logo,
                caps_lock: m.caps_lock,
                num_lock: m.num_lock,
            },
            tag,
        );
    }
}

impl DataDeviceHandler for Client {
    fn enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlDataDevice,
        _: f64,
        _: f64,
        _: &wl_surface::WlSurface,
    ) {
    }

    fn leave(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataDevice) {}

    fn motion(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataDevice, _: f64, _: f64) {}

    fn selection(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataDevice) {
        if !self.args.log_selection {
            return;
        }
        let Some(offer) = self
            .data_device
            .as_ref()
            .and_then(|d| d.data().selection_offer())
        else {
            return;
        };
        let Some(mime) = offer.with_mime_types(pick_text_mime) else {
            return;
        };
        let Ok(pipe) = offer.receive(mime.to_string()) else {
            return;
        };
        let mut data = Vec::new();
        let _ = self.lh.insert_source(pipe, move |_, f, state| {
            let _keep = &offer;
            // SAFETY: the file is only read, never closed or replaced.
            let f: &mut std::fs::File = unsafe { f.get_mut() };
            let mut buf = [0u8; 4096];
            match f.read(&mut buf) {
                Ok(0) => {
                    let text = String::from_utf8_lossy(&data).into_owned();
                    state.log.emit(Event::Selection {
                        mime: mime.to_string(),
                        text,
                    });
                    PostAction::Remove
                }
                Ok(n) => {
                    data.extend_from_slice(&buf[..n]);
                    PostAction::Continue
                }
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => PostAction::Continue,
                Err(_) => PostAction::Remove,
            }
        });
    }

    fn drop_performed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataDevice) {}
}

impl DataOfferHandler for Client {
    fn source_actions(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &mut DragOffer,
        _: DndAction,
    ) {
    }

    fn selected_action(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &mut DragOffer,
        _: DndAction,
    ) {
    }
}

impl DataSourceHandler for Client {
    fn accept_mime(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlDataSource,
        _: Option<String>,
    ) {
    }

    fn send_request(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        source: &WlDataSource,
        mime: String,
        fd: WritePipe,
    ) {
        let ours = self
            .copy_source
            .as_ref()
            .is_some_and(|s| s.inner() == source);
        if let (true, Some(text)) = (ours && TEXT_MIMES.contains(&mime.as_str()), &self.args.copy) {
            let mut f = std::fs::File::from(OwnedFd::from(fd));
            let _ = f.write_all(text.as_bytes());
        }
    }

    fn cancelled(&mut self, _: &Connection, _: &QueueHandle<Self>, source: &WlDataSource) {
        if self
            .copy_source
            .as_ref()
            .is_some_and(|s| s.inner() == source)
        {
            self.copy_source = None;
            self.log.emit(Event::SelectionCancelled);
        }
    }

    fn dnd_dropped(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataSource) {}

    fn dnd_finished(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataSource) {}

    fn action(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataSource, _: DndAction) {}
}

impl ActivationHandler for Client {
    type RequestUdata = ();
    // we only activate with a token we were given; never request one
    fn new_token(&mut self, _: String, _: &RequestData<()>) {}
}

impl PointerHandler for Client {
    fn pointer_frame(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        _: &wl_pointer::WlPointer,
        events: &[PointerEvent],
    ) {
        for e in events {
            if let PointerEventKind::Enter { serial } = e.kind
                && self.is_main(&e.surface)
            {
                self.on_pointer_enter(qh, serial);
            }
        }
    }
}

impl PointerConstraintsHandler for Client {
    fn confined(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &ZwpConfinedPointerV1,
        _: &wl_surface::WlSurface,
        _: &wl_pointer::WlPointer,
    ) {
    }

    fn unconfined(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &ZwpConfinedPointerV1,
        _: &wl_surface::WlSurface,
        _: &wl_pointer::WlPointer,
    ) {
    }

    fn locked(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &ZwpLockedPointerV1,
        _: &wl_surface::WlSurface,
        _: &wl_pointer::WlPointer,
    ) {
        self.log.emit(Event::Locked);
    }

    fn unlocked(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &ZwpLockedPointerV1,
        _: &wl_surface::WlSurface,
        _: &wl_pointer::WlPointer,
    ) {
        self.log.emit(Event::Unlocked);
    }
}

impl RelativePointerHandler for Client {
    fn relative_pointer_motion(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &ZwpRelativePointerV1,
        _: &wl_pointer::WlPointer,
        e: RelativeMotionEvent,
    ) {
        self.log.emit(Event::RelativeMotion {
            dx: e.delta.0,
            dy: e.delta.1,
        });
    }
}

impl PrimarySelectionDeviceHandler for Client {
    fn selection(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &smithay_client_toolkit::reexports::protocols::wp::primary_selection::zv1::client::zwp_primary_selection_device_v1::ZwpPrimarySelectionDeviceV1,
    ) {
        if !self.args.log_primary {
            return;
        }
        let Some(offer) = self
            .x
            .primary_dev
            .as_ref()
            .and_then(|d| d.data().selection_offer())
        else {
            return;
        };
        let Some(mime) = offer.with_mime_types(pick_text_mime) else {
            return;
        };
        let Ok(pipe) = offer.receive(mime.to_string()) else {
            return;
        };
        let mut data = Vec::new();
        let _ = self.lh.insert_source(pipe, move |_, f, state| {
            let _keep = &offer;
            // SAFETY: the file is only read, never closed or replaced.
            let f: &mut std::fs::File = unsafe { f.get_mut() };
            let mut buf = [0u8; 4096];
            match f.read(&mut buf) {
                Ok(0) => {
                    let text = String::from_utf8_lossy(&data).into_owned();
                    state.log.emit(Event::Primary {
                        mime: mime.to_string(),
                        text,
                    });
                    PostAction::Remove
                }
                Ok(n) => {
                    data.extend_from_slice(&buf[..n]);
                    PostAction::Continue
                }
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => PostAction::Continue,
                Err(_) => PostAction::Remove,
            }
        });
    }
}

impl PrimarySelectionSourceHandler for Client {
    fn send_request(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        source: &smithay_client_toolkit::reexports::protocols::wp::primary_selection::zv1::client::zwp_primary_selection_source_v1::ZwpPrimarySelectionSourceV1,
        mime: String,
        fd: WritePipe,
    ) {
        let ours = self
            .x
            .primary_src
            .as_ref()
            .is_some_and(|s| s.inner() == source);
        if let (true, Some(text)) = (
            ours && TEXT_MIMES.contains(&mime.as_str()),
            &self.args.primary,
        ) {
            let mut f = std::fs::File::from(OwnedFd::from(fd));
            let _ = f.write_all(text.as_bytes());
        }
    }

    fn cancelled(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        source: &smithay_client_toolkit::reexports::protocols::wp::primary_selection::zv1::client::zwp_primary_selection_source_v1::ZwpPrimarySelectionSourceV1,
    ) {
        if self
            .x
            .primary_src
            .as_ref()
            .is_some_and(|s| s.inner() == source)
        {
            self.x.primary_src = None;
            self.log.emit(Event::PrimaryCancelled);
        }
    }
}

/// The wave-4 objects with no SCTK helper: requests only, or events we log.
macro_rules! no_events {
    ($($t:ty),* $(,)?) => {$(
        impl Dispatch2<$t, Client> for Proto {
            fn event(
                &self,
                _: &mut Client,
                _: &$t,
                _: <$t as Proxy>::Event,
                _: &Connection,
                _: &QueueHandle<Client>,
            ) {
            }
        }
    )*};
}
no_events!(
    WpCursorShapeManagerV1,
    WpCursorShapeDeviceV1,
    WpPresentation,
    WpSinglePixelBufferManagerV1,
    WlBuffer,
    WpContentTypeManagerV1,
    WpContentTypeV1,
    ZxdgExporterV2,
    ZwpIdleInhibitManagerV1,
    ZwpIdleInhibitorV1,
);

impl Dispatch2<WpPresentationFeedback, Client> for Proto {
    fn event(
        &self,
        state: &mut Client,
        _: &WpPresentationFeedback,
        ev: wp_presentation_feedback::Event,
        _: &Connection,
        _: &QueueHandle<Client>,
    ) {
        match ev {
            wp_presentation_feedback::Event::Presented {
                tv_sec_hi,
                tv_sec_lo,
                tv_nsec,
                refresh,
                seq_hi,
                seq_lo,
                flags,
            } => {
                let flags = flags.into_result().map(|f| f.bits()).unwrap_or(0);
                state.log.emit(presented(
                    tv_sec_hi, tv_sec_lo, tv_nsec, refresh, seq_hi, seq_lo, flags,
                ));
            }
            wp_presentation_feedback::Event::Discarded => state.log.emit(Event::Discarded),
            _ => {}
        }
    }
}

impl Dispatch2<ZxdgExportedV2, Client> for Proto {
    fn event(
        &self,
        state: &mut Client,
        _: &ZxdgExportedV2,
        ev: zxdg_exported_v2::Event,
        _: &Connection,
        _: &QueueHandle<Client>,
    ) {
        if let zxdg_exported_v2::Event::Handle { handle } = ev {
            state.log.emit(Event::Exported { handle });
        }
    }
}

impl ShmHandler for Client {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

impl ProvidesRegistryState for Client {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    registry_handlers![OutputState, SeatState];
}

smithay_client_toolkit::delegate_registry!(Client);
smithay_client_toolkit::delegate_dispatch2!(Client);
