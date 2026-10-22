//! nkw-testclient: one xdg toplevel painted with a solid colour or a 1 px
//! checker, logging configure/enter/leave/key/modifiers/frame/popup events as
//! JSON lines (contract S3). Used by nkw's integration tests and perf runs.

use std::time::Duration;

use calloop::EventLoop;
use calloop::timer::{TimeoutAction, Timer};
use nkw_testclient::{Args, Event, Fill, KeyState, Log, USAGE, paint, parse_args};
use smithay_client_toolkit::reexports::protocols::xdg::shell::client::xdg_positioner;
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState, FrameCallbackData},
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
    Connection, QueueHandle,
    globals::registry_queue_init,
    protocol::{wl_keyboard, wl_output, wl_seat, wl_shm, wl_surface},
};

const POPUP_SIZE: (u32, u32) = (64, 32);

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
    if let Err(e) = run(args) {
        eprintln!("nkw-testclient: {e}");
        std::process::exit(1);
    }
}

struct Client {
    args: Args,
    log: Log,
    registry_state: RegistryState,
    seat_state: SeatState,
    output_state: OutputState,
    compositor: CompositorState,
    xdg_shell: XdgShell,
    shm: Shm,
    pool: SlotPool,
    window: Window,
    size: (u32, u32),
    buffer: Option<Buffer>,
    configured: bool,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    seat: Option<wl_seat::WlSeat>,
    popup: Option<Popup>,
    popup_opened: bool,
    exit: bool,
}

fn run(args: Args) -> Result<(), Box<dyn std::error::Error>> {
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

    let surface = compositor.create_surface(&qh);
    let window = xdg_shell.create_window(surface, WindowDecorations::ServerDefault, &qh);
    window.set_title(args.title.clone());
    window.set_app_id(args.app_id.clone());
    // The first commit carries no buffer; the configure answers it.
    window.commit();

    let (w, h) = args.size;
    let pool = SlotPool::new((w * h * 4) as usize, &shm)?;

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
        size: args.size,
        args,
        log,
        registry_state: RegistryState::new(&globals),
        seat_state: SeatState::new(&globals, &qh),
        output_state: OutputState::new(&globals, &qh),
        compositor,
        xdg_shell,
        shm,
        pool,
        window,
        buffer: None,
        configured: false,
        keyboard: None,
        seat: None,
        popup: None,
        popup_opened: false,
        exit: false,
    };
    while !client.exit {
        // No timeout: the client only wakes for Wayland events or its own timer.
        event_loop.dispatch(None, &mut client)?;
    }
    Ok(())
}

impl Client {
    fn draw(&mut self, qh: &QueueHandle<Self>) {
        let (w, h) = self.size;
        let stride = w as i32 * 4;
        let fill = self.args.fill;
        // Repaint only into a fresh buffer; a released one keeps its pixels.
        let reuse = self
            .buffer
            .as_ref()
            .is_some_and(|b| b.height() == h as i32 && b.stride() == stride)
            && self
                .buffer
                .as_mut()
                .is_some_and(|b| self.pool.canvas(b).is_some());
        if !reuse {
            let (buffer, canvas) = self
                .pool
                .create_buffer(w as i32, h as i32, stride, wl_shm::Format::Argb8888)
                .expect("create shm buffer");
            paint(canvas, w, h, fill);
            self.buffer = Some(buffer);
        }
        let surface = self.window.wl_surface();
        surface.damage_buffer(0, 0, w as i32, h as i32);
        if !self.args.no_frame_wait {
            surface.frame(qh, FrameCallbackData(surface.clone()));
        }
        self.buffer
            .as_ref()
            .unwrap()
            .attach_to(surface)
            .expect("attach buffer");
        self.window.commit();
    }

    fn open_popup(&mut self, qh: &QueueHandle<Self>, serial: u32) {
        self.popup_opened = true;
        let Ok(positioner) = XdgPositioner::new(&self.xdg_shell) else {
            return;
        };
        positioner.set_size(POPUP_SIZE.0 as i32, POPUP_SIZE.1 as i32);
        positioner.set_anchor_rect(0, 0, 1, 1);
        positioner.set_anchor(xdg_positioner::Anchor::BottomRight);
        positioner.set_gravity(xdg_positioner::Gravity::BottomRight);
        let surface = self.compositor.create_surface(qh);
        let Ok(popup) = Popup::from_surface(
            Some(self.window.xdg_surface()),
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
        if s != self.window.wl_surface() {
            return;
        }
        self.log.emit(Event::Frame);
        self.draw(qh);
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
    fn request_close(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &Window) {
        self.log.emit(Event::Close);
        self.exit = true;
    }

    fn configure(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        _: &Window,
        cfg: WindowConfigure,
        _: u32,
    ) {
        let w = cfg.new_size.0.map_or(self.args.size.0, |v| v.get());
        let h = cfg.new_size.1.map_or(self.args.size.1, |v| v.get());
        self.log.emit(Event::Configure {
            w,
            h,
            states: nkw_testclient::state_names(cfg.state.bits()),
        });
        let resized = (w, h) != self.size;
        self.size = (w, h);
        // With frame callbacks the loop is already running; draw only to start it,
        // or to show the new size now.
        if !self.configured || resized || self.args.no_frame_wait {
            self.configured = true;
            self.draw(qh);
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

impl KeyboardHandler for Client {
    fn enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        s: &wl_surface::WlSurface,
        _: u32,
        _: &[u32],
        _: &[Keysym],
    ) {
        if s == self.window.wl_surface() {
            self.log.emit(Event::Enter);
        }
    }

    fn leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        s: &wl_surface::WlSurface,
        _: u32,
    ) {
        if s == self.window.wl_surface() {
            self.log.emit(Event::Leave);
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
        self.log.emit(key_event(&e, KeyState::Pressed));
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
        self.log.emit(key_event(&e, KeyState::Released));
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
        self.log.emit(Event::Modifiers {
            ctrl: m.ctrl,
            alt: m.alt,
            shift: m.shift,
            logo: m.logo,
            caps_lock: m.caps_lock,
            num_lock: m.num_lock,
        });
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
