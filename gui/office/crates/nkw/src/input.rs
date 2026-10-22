//! Input: host events and script keys take the same path into the seat's
//! keyboard, whose filter hands the world its keys (§3.6).

use smithay::backend::input::InputTime;
use smithay::backend::input::{
    AbsolutePositionEvent, Axis, AxisSource, ButtonState, Event, InputBackend, InputEvent,
    KeyState, KeyboardKeyEvent, PointerAxisEvent, PointerButtonEvent,
};
use smithay::input::keyboard::{FilterResult, Keycode};
use smithay::input::pointer::{AxisFrame, ButtonEvent, MotionEvent};
use smithay::utils::{Logical, Point, SERIAL_COUNTER};

use crate::state::Nkw;
use crate::world::{Cmd, KeyMods, Verdict, filter};

impl Nkw {
    pub fn process_input<I: InputBackend>(&mut self, event: InputEvent<I>) {
        match event {
            InputEvent::Keyboard { event } => {
                self.key(event.key_code(), event.state(), Event::time(&event));
            }
            InputEvent::PointerMotionAbsolute { event } => {
                let s = self.world.out.scale as f64;
                let size = (
                    (self.world.out.width_px as f64 / s) as i32,
                    (self.world.out.height_px as f64 / s) as i32,
                );
                let pos = event.position_transformed(size.into());
                self.pointer_motion(pos, Event::time(&event));
            }
            InputEvent::PointerButton { event } => {
                self.pointer_button(event.button_code(), event.state(), Event::time(&event));
            }
            InputEvent::PointerAxis { event } => {
                let source = event.source();
                let mut frame = AxisFrame::new(Event::time(&event)).source(source);
                for axis in [Axis::Horizontal, Axis::Vertical] {
                    let v120 = event.amount_v120(axis);
                    let amount = event
                        .amount(axis)
                        .unwrap_or_else(|| v120.unwrap_or(0.0) * 15.0 / 120.0);
                    if amount != 0.0 {
                        frame = frame.value(axis, amount);
                        if let Some(d) = v120 {
                            frame = frame.v120(axis, d as i32);
                        }
                    }
                    if source == AxisSource::Finger && event.amount(axis) == Some(0.0) {
                        frame = frame.stop(axis);
                    }
                }
                let p = self.seat.get_pointer().unwrap();
                p.axis(self, frame);
                p.frame(self);
            }
            _ => {}
        }
    }

    pub fn pointer_motion(&mut self, pos: Point<f64, Logical>, time: InputTime) {
        self.pointer = pos;
        let under = self.surface_under(pos);
        let p = self.seat.get_pointer().unwrap();
        p.motion(
            self,
            under,
            &MotionEvent {
                location: pos,
                serial: SERIAL_COUNTER.next_serial(),
                time,
            },
        );
        p.frame(self);
    }

    pub fn pointer_button(&mut self, button: u32, state: ButtonState, time: InputTime) {
        let p = self.seat.get_pointer().unwrap();
        // in FOCO a click on the dimmed world around the window leaves (§3.6)
        if state == ButtonState::Pressed
            && !p.is_grabbed()
            && self.world.focus_2d().is_some()
            && self.surface_under(self.pointer).is_none()
        {
            self.run_cmd(Cmd::Back);
            return;
        }
        p.button(
            self,
            &ButtonEvent {
                button,
                state,
                serial: SERIAL_COUNTER.next_serial(),
                time,
            },
        );
        p.frame(self);
    }

    /// A key, from the host or a script: through the seat's keyboard and the
    /// world's filter; what is forwarded reaches the focused client in this
    /// same loop iteration.
    pub fn key(&mut self, code: Keycode, state: KeyState, time: InputTime) {
        let kb = self.seat.get_keyboard().unwrap();
        let serial = SERIAL_COUNTER.next_serial();
        let pressed = state == KeyState::Pressed;
        let cmd = kb.input(self, code, state, serial, time, |st, mods, h| {
            st.last_sym = h.modified_sym().raw();
            if !pressed {
                // a release follows its press
                return if st.intercepted.remove(&code) {
                    FilterResult::Intercept(None)
                } else {
                    FilterResult::Forward
                };
            }
            let m = KeyMods {
                ctrl: mods.ctrl,
                alt: mods.alt,
                shift: mods.shift,
            };
            match filter(st.world.mode(), m, h.modified_sym().raw()) {
                Verdict::Forward => FilterResult::Forward,
                Verdict::Intercept(c) => {
                    st.intercepted.insert(code);
                    FilterResult::Intercept(c)
                }
            }
        });
        // keys held across a mode change count only once released (§3.2)
        let counts = self.world.machine.key(code.raw(), pressed);
        if let Some(Some(c)) = cmd
            && counts
        {
            self.run_cmd(c);
        } else if counts
            && self.world.mode().walking()
            && self.world.walk_key(self.last_sym, pressed)
        {
            self.wake();
        }
        let _ = self.dh.flush_clients();
    }

    pub fn run_cmd(&mut self, c: Cmd) {
        let now = self.now();
        let outs = self.world.command(c, now);
        self.apply(outs);
        self.wake();
    }
}
