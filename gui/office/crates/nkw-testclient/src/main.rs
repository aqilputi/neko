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
    Args, CHILD_AFTER_MS, Event, Fill, KeyState, Log, MOTHER_FILL, TEXT_MIMES, USAGE, child_argv,
    damage_rect, paint, paint_rect, parse_args, physical_size, pick_text_mime,
};
use smithay_client_toolkit::activation::{ActivationHandler, ActivationState, RequestData};
use smithay_client_toolkit::reexports::protocols::wp::fractional_scale::v1::client::{
    wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1,
    wp_fractional_scale_v1::{self, WpFractionalScaleV1},
};
use smithay_client_toolkit::reexports::protocols::wp::viewporter::client::{
    wp_viewport::WpViewport, wp_viewporter::WpViewporter,
};
use smithay_client_toolkit::reexports::protocols::xdg::shell::client::xdg_positioner;
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
        wl_data_source::WlDataSource, wl_keyboard, wl_output, wl_seat, wl_shm, wl_surface,
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
}

fn run(
    args: Args,
    argv: &[String],
    token: Option<Option<String>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let log = Log::open(&args.log)?;
    let conn = Connection::connect_to_env()?;
    let (globals, queue) = registry_queue_init(&conn)?;
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
    let (fs_mgr, viewporter) = if args.fractional {
        bind_fractional(&globals, &qh)
    } else {
        (None, None)
    };

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
        }
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
            self.seat = Some(seat);
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
