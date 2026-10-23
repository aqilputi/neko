//! Input: host events and script keys take the same path into the seat's
//! keyboard, whose filter asks the router what each key is (§3.6, S7).

use std::collections::BTreeMap;

use smithay::backend::input::InputTime;
use smithay::backend::input::{
    AbsolutePositionEvent, Axis, AxisSource, ButtonState, Event, InputBackend, InputEvent,
    KeyState, KeyboardKeyEvent, PointerAxisEvent, PointerButtonEvent, PointerMotionEvent,
};
use smithay::input::keyboard::{FilterResult, Keycode};
use smithay::input::pointer::{AxisFrame, ButtonEvent, MotionEvent};
use smithay::utils::{Logical, Point, SERIAL_COUNTER};

use nkw_core::{CatId, Target};

use crate::router::{KeyEv, KeyMods, Route};
use crate::state::Nkw;
use crate::world::Cmd;

/// What the keyboard filter took from the client.
enum Taken {
    Command(String, BTreeMap<String, String>),
    /// a held command's release (ANDAR): runs even when the press predates
    /// the mode (the machine's key filter is for presses)
    Release(String, BTreeMap<String, String>),
    Text(KeyEv),
    /// FOCO on a cat's terminal: the key is the pane's
    Pane(CatId, KeyEv),
}

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
            InputEvent::PointerMotion { event } => {
                // the host's relative pointer (the patched Smithay winit
                // backend; unpatched builds never see this variant)
                self.look(event.delta_x(), event.delta_y());
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

    /// ANDAR's mouse-look: a relative motion turns the walker's head.
    pub fn look(&mut self, dx: f64, dy: f64) {
        if self.world.walking.is_none() || !self.world.walk_config().mouse_look {
            return;
        }
        self.world.look_delta(dx as f32, dy as f32);
        self.stats.look_events += 1;
        self.world.want(nkw_core::Wish::world(nkw_core::Want::Once));
        self.wake();
    }

    pub fn pointer_button(&mut self, button: u32, state: ButtonState, time: InputTime) {
        if self.world.walking.is_some() && self.world.focus_2d().is_none() {
            // ANDAR: no client has the pointer; left interacts, right grabs
            if state == ButtonState::Pressed
                && let Some(cmd) = walk_button(button)
            {
                self.dispatch(cmd, &BTreeMap::new());
            }
            return;
        }
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
    /// router; what is forwarded reaches the focused client in this same
    /// loop iteration.
    pub fn key(&mut self, code: Keycode, state: KeyState, time: InputTime) {
        let kb = self.seat.get_keyboard().unwrap();
        let serial = SERIAL_COUNTER.next_serial();
        let pressed = state == KeyState::Pressed;
        let taken = kb.input(self, code, state, serial, time, |st, mods, h| {
            let sym = h.modified_sym().raw();
            st.last_sym = sym;
            let m = KeyMods {
                ctrl: mods.ctrl,
                alt: mods.alt,
                shift: mods.shift,
                logo: mods.logo,
            };
            let utf8 = char::from_u32(xkb_utf32(sym))
                .filter(|c| !c.is_control())
                .map(String::from);
            let ev = KeyEv {
                code: code.raw(),
                sym,
                name: crate::keys::key_name(sym, m),
                utf8,
                mods: m,
                pressed,
            };
            let pane = match st.world.mode().focus() {
                Some(Target::CatTerm(c)) => Some(c),
                _ => None,
            };
            if !pressed && st.world.mode().walking() {
                // ANDAR: a held walking command ends on its key's release
                // (S16: `{"pressed": "false"}`), rebound keys included
                let sub = st.ext.submode();
                if let Route::Command { name, args } =
                    st.ext.router.route(st.world.mode(), sub, &ev)
                {
                    st.intercepted.remove(&code);
                    return FilterResult::Intercept(Some(Taken::Release(name, args)));
                }
            }
            if !pressed {
                // a release follows its press
                return match (st.intercepted.remove(&code), pane) {
                    (true, Some(c)) => FilterResult::Intercept(Some(Taken::Pane(c, ev))),
                    (true, None) => FilterResult::Intercept(None),
                    (false, _) => FilterResult::Forward,
                };
            }
            let sub = st.ext.submode();
            let taken = match st.ext.router.route(st.world.mode(), sub, &ev) {
                Route::Forward => match pane {
                    Some(c) => Some(Taken::Pane(c, ev)),
                    None => return FilterResult::Forward,
                },
                Route::Swallow | Route::Pending => None,
                Route::Command { name, args } => Some(Taken::Command(name, args)),
                Route::Text(ev) => Some(Taken::Text(ev)),
            };
            st.intercepted.insert(code);
            FilterResult::Intercept(taken)
        });
        // keys held across a mode change count only once released (§3.2)
        let counts = self.world.machine.key(code.raw(), pressed);
        if let Some(Some(Taken::Release(name, args))) = &taken {
            self.dispatch(name, args);
            let _ = self.dh.flush_clients();
            return;
        }
        if let Some(Some(t)) = taken
            && counts
        {
            match t {
                Taken::Command(name, args) => self.dispatch(&name, &args),
                Taken::Text(ev) => {
                    let now = self.now();
                    let reqs = {
                        let ctx = crate::hook_ctx!(self, now);
                        self.ext.overlay.key(&ev, &ctx)
                    };
                    self.apply_reqs(reqs);
                    self.wake();
                }
                Taken::Pane(c, ev) => self.ext.cats.key_to_pane(c, &ev),
                Taken::Release(..) => {}
            }
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

/// What a pointer button press does in ANDAR (evdev codes).
pub fn walk_button(button: u32) -> Option<&'static str> {
    match button {
        0x110 => Some("world.interact"),
        0x111 => Some("world.grab"),
        _ => None,
    }
}

fn xkb_utf32(sym: u32) -> u32 {
    use smithay::input::keyboard::xkb;
    xkb::keysym_to_utf32(xkb::Keysym::new(sym))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn walk_buttons() {
        assert_eq!(walk_button(0x110), Some("world.interact"));
        assert_eq!(walk_button(0x111), Some("world.grab"));
        assert_eq!(walk_button(0x112), None, "middle: nothing");
    }
}
