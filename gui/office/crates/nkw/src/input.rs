//! Input: host events and script keys take the same path into the seat's
//! keyboard, whose filter asks the router what each key is (§3.6, S7).

use std::collections::BTreeMap;

use glam::Vec2;
use smithay::backend::input::InputTime;
use smithay::backend::input::{
    AbsolutePositionEvent, Axis, AxisSource, ButtonState, Event, InputBackend, InputEvent,
    KeyState, KeyboardKeyEvent, PointerAxisEvent, PointerButtonEvent, PointerMotionEvent,
};
use smithay::input::keyboard::{FilterResult, Keycode};
use smithay::input::pointer::{AxisFrame, ButtonEvent, MotionEvent};
use smithay::utils::{Logical, Point, SERIAL_COUNTER};

use nkw_core::pointer::{self, PtrEv};
use nkw_core::{CatId, Dir, Kind, Target};

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
    /// `esc` while a mouse drag (`World::pointer_dragging`) is live: cancels
    /// it instead of whatever `esc` would otherwise do in this mode
    PointerCancel,
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
                // backend; unpatched builds never see this variant). A
                // client's pointer lock in FOCO takes it first (S23).
                let (dx, dy) = (event.delta_x(), event.delta_y());
                let (dxu, dyu) = (event.delta_x_unaccel(), event.delta_y_unaccel());
                let utime = Event::time(&event).micros();
                if !self.protos_relative_motion(dx, dy, dxu, dyu, utime) {
                    self.look(dx, dy);
                }
            }
            InputEvent::PointerButton { event } => {
                self.pointer_button(event.button_code(), event.state(), Event::time(&event));
            }
            InputEvent::PointerAxis { event } => {
                // niri parity: outside FOCO, the wheel steps to the next/
                // previous desk (`Mod+WheelScrollDown/Up`) instead of
                // reaching a client — there is no client under the pointer
                // there anyway (NAV/ANDAR/etc. render the 3D world, not a 2D
                // surface). The sign is a best guess (the common "wheel down
                // = forward" convention); flip the comparison below if it
                // turns out backwards on real hardware. A 150 ms cooldown
                // (`World::wheel_nav_ready`) keeps one physical click to one
                // step.
                let vertical = event.amount(Axis::Vertical).unwrap_or_else(|| {
                    event.amount_v120(Axis::Vertical).unwrap_or(0.0) * 15.0 / 120.0
                });
                // the pointer machine's own wheel (plan §2.3/§2.4): depth or
                // scale while dragging a screen, or `alt`+wheel to scale one
                // outright. It only ever produces something in those two
                // cases (`nkw_core::pointer::Machine::step`'s `Wheel` arm),
                // so an empty result safely falls through to desk-nav/the
                // client exactly as before — this never changes FOCO's plain
                // scroll-to-the-client when no drag is live and `alt` is up.
                if self.world.walking.is_none()
                    && (self.world.mode().focus().is_none() || self.world.pointer_dragging())
                    && vertical.abs() > 0.1
                {
                    let steps = if vertical > 0.0 { 1 } else { -1 };
                    let px = self.pointer_px();
                    let hit = self.pointer_world_hit(px);
                    let now = self.now();
                    let outs = {
                        let cfg = self.pointer_cfg();
                        self.world
                            .pointer_step(PtrEv::Wheel { steps, hit }, &cfg, now)
                    };
                    if !outs.is_empty() {
                        let world_outs = self.world.pointer_apply(&outs, now);
                        self.apply(world_outs);
                        return;
                    }
                }
                if self.world.mode().focus().is_none() && vertical.abs() > 0.1 {
                    let now = self.now();
                    if self.world.wheel_nav_ready(now) {
                        let dir = if vertical > 0.0 {
                            Dir::Right
                        } else {
                            Dir::Left
                        };
                        self.run_cmd(Cmd::Move(dir));
                    }
                    return;
                }
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
        // the world pointer machine (plan §2.3/D15): hover (cursor shape,
        // not wired to a theme yet) and live drag moves. This only fires for
        // the host's absolute pointer, which ANDAR's mouse-look never emits
        // (it locks the cursor and uses `PointerMotion`/`look` instead), so
        // it is effectively NAV/ARRUMAR/FOCO/ANDAR-livre already.
        let px = self.pointer_px();
        let hit = self.pointer_world_hit(px);
        let now = self.now();
        self.pointer_ev(PtrEv::Motion { at: px, hit }, time, now);
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
            // ANDAR com mira: no client has the pointer; left interacts,
            // right grabs (existing, unchanged). The mouse's own
            // `ctrl+esquerdo segurado carrega` (plan §2.3) is not wired here
            // yet — see the final report.
            if state == ButtonState::Pressed
                && let Some(cmd) = walk_button(button)
            {
                self.dispatch(cmd, &BTreeMap::new());
            }
            return;
        }
        let p = self.seat.get_pointer().unwrap();
        if p.is_grabbed() {
            // a client's own grab (a popup, an interactive resize …): straight
            // through, exactly as before — the world pointer machine never
            // steals an event from a grab it does not own.
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
            return;
        }
        // in FOCO a click on the dimmed world around the window leaves (§3.6)
        if state == ButtonState::Pressed
            && self.world.focus_2d().is_some()
            && self.surface_under(self.pointer).is_none()
        {
            self.run_cmd(Cmd::Back);
            return;
        }
        let Some(btn) = ptr_button(button) else {
            // an unmapped button (side buttons …): the pointer machine only
            // knows left/middle/right, so this goes straight to the client
            // as it always did.
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
            return;
        };
        let px = self.pointer_px();
        let now = self.now();
        let ev = match state {
            ButtonState::Pressed => pointer::PtrEv::Press {
                button: btn,
                at: px,
                hit: self.pointer_world_hit(px),
            },
            ButtonState::Released => pointer::PtrEv::Release {
                button: btn,
                at: px,
            },
        };
        self.pointer_ev(ev, time, now);
    }

    /// What the pointer is over, for `nkw_core::pointer::Machine`'s own
    /// routing (D15): the focused client's surface or its dimmed
    /// surroundings in FOCO, the 2D overlay in CONVERSA/SALTO/COMANDOS (the
    /// balão/telefone/caixa are not ray-cast through — see the report), else
    /// `World`'s own 3D pick (`World::pointer_hit`).
    fn pointer_world_hit(&self, px: Vec2) -> pointer::Hit {
        if self.world.mode().focus().is_some() {
            return if self.surface_under(self.pointer).is_some() {
                pointer::Hit::Client
            } else {
                pointer::Hit::Outside
            };
        }
        if matches!(
            self.world.mode().kind(),
            Some(Kind::Talk { .. } | Kind::Hint { .. } | Kind::Palette)
        ) {
            return pointer::Hit::Ui;
        }
        self.world.pointer_hit(px)
    }

    /// The pointer's current position in physical px (the `[mouse]`/
    /// `arrange::View` convention; `self.pointer` is logical).
    fn pointer_px(&self) -> Vec2 {
        let s = self.world.out.scale;
        Vec2::new(self.pointer.x as f32 * s, self.pointer.y as f32 * s)
    }

    /// `[mouse]`/`world.toml`'s drag config, as `nkw_core::pointer::Config`
    /// (`world_mod` the same the router uses: the CLI flag over `[keys]`).
    fn pointer_cfg(&self) -> pointer::Config {
        let cfg = self.ext.wm.config();
        let world_mod = self.cli.world_mod_or(cfg.keys.world_mod.as_deref());
        ptr_cfg(&cfg.mouse, world_mod)
    }

    /// Feed one real pointer event to the world's pointer machine
    /// (`World::pointer_step`) and apply what it returns: `PassPress`/
    /// `PassRelease` reach the seat's pointer like a normal button (FOCO,
    /// mostly — the only case `nkw_core::pointer::passes` allows outside a
    /// live (1×) dock, not wired to the renderer yet); everything else is a
    /// world-side gesture `World::pointer_apply` turns into `Out`s this hub
    /// already knows how to run (`Nkw::apply`).
    fn pointer_ev(&mut self, ev: PtrEv, time: InputTime, now: f64) {
        let cfg = self.pointer_cfg();
        let outs = self.world.pointer_step(ev, &cfg, now);
        for o in &outs {
            match *o {
                pointer::PtrOut::PassPress(b) => {
                    self.pointer_seat_button(b, ButtonState::Pressed, time);
                }
                pointer::PtrOut::PassRelease(b) => {
                    self.pointer_seat_button(b, ButtonState::Released, time);
                }
                _ => {}
            }
        }
        let world_outs = self.world.pointer_apply(&outs, now);
        self.apply(world_outs);
    }

    fn pointer_seat_button(
        &mut self,
        button: pointer::Button,
        state: ButtonState,
        time: InputTime,
    ) {
        let p = self.seat.get_pointer().unwrap();
        p.button(
            self,
            &ButtonEvent {
                button: evdev_button(button),
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
            // the pointer machine's own modifier latch (plan §2.3's "trava"):
            // every key event, not just the ones the router takes, since a
            // live drag must see `ctrl`/`alt`/`shift` go up and down exactly
            // as the seat keyboard reports them
            {
                let cfg = st.pointer_cfg();
                let now = st.now();
                let pm = pointer::Mods {
                    ctrl: m.ctrl,
                    alt: m.alt,
                    shift: m.shift,
                    logo: m.logo,
                };
                let _ = st.world.pointer_step(PtrEv::Mods(pm), &cfg, now);
            }
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
            // `esc` while a mouse drag is live cancels it (plan §2.3)
            // instead of whatever `esc` would otherwise do in this mode; the
            // actual `PtrEv::Cancel`/`arrange_revert` happens after
            // `kb.input` returns, like `Taken::Release` below — mutating the
            // seat's own keyboard/pointer focus from inside this closure
            // would be re-entrant.
            if pressed && ev.name == "esc" && st.world.pointer_dragging() {
                st.intercepted.insert(code);
                return FilterResult::Intercept(Some(Taken::PointerCancel));
            }
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
        if let Some(Some(Taken::PointerCancel)) = &taken {
            let cfg = self.pointer_cfg();
            let now = self.now();
            let outs = self.world.pointer_step(PtrEv::Cancel, &cfg, now);
            let world_outs = self.world.pointer_apply(&outs, now);
            self.apply(world_outs);
            let _ = self.dh.flush_clients();
            return;
        }
        if let Some(Some(t)) = taken
            && counts
        {
            match t {
                Taken::Command(name, _args) if name == "world.screenshot" => {
                    self.take_screenshot();
                }
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
                // handled above, before `counts` is even read
                Taken::PointerCancel => {}
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

    /// `world.screenshot` (plan §1.3): saves the composed frame as a PNG,
    /// reusing `--shot`'s own mechanism (`Nkw::shots`, `frame.rs`'s
    /// read-back + `shot::write_png`). A simplification: this always
    /// captures the whole output, like `--shot`; cropping to just the
    /// selected screen's texture (the plan's "screen" target) is separate,
    /// bigger work — nothing exports a single screen's texture today, only
    /// the composed frame. No Go-side keybinding exists for this yet (it is
    /// phase B of the plan, not registered in `internal/app/world.go`): only
    /// the Rust fallback keymap (`router/table.rs`) binds a key to it so far.
    pub fn take_screenshot(&mut self) {
        let dir = std::env::var_os("HOME")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join("Imagens/nekuroddo");
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        let path = dir.join(format!("nkw-{stamp}.png"));
        self.shots.push(path.clone());
        let now = self.now();
        self.ext.overlay.toast(
            &format!("captura: {}", path.display()),
            crate::ext::ToastKind::Info,
            2000,
            now,
        );
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

/// An evdev button code as the pointer machine's own `Button` (left/middle/
/// right only — side buttons and the like are not gestures).
fn ptr_button(code: u32) -> Option<pointer::Button> {
    match code {
        0x110 => Some(pointer::Button::Left),
        0x111 => Some(pointer::Button::Right),
        0x112 => Some(pointer::Button::Middle),
        _ => None,
    }
}

/// The inverse of [`ptr_button`], for `PassPress`/`PassRelease`.
fn evdev_button(b: pointer::Button) -> u32 {
    match b {
        pointer::Button::Left => 0x110,
        pointer::Button::Right => 0x111,
        pointer::Button::Middle => 0x112,
    }
}

/// `[mouse]`'s `MouseConfig` + the resolved `world_mod`, as the pointer
/// machine's own `Config`.
fn ptr_cfg(
    mouse: &nkw_core::rules::MouseConfig,
    world_mod: crate::router::WorldMod,
) -> pointer::Config {
    pointer::Config {
        drag: mouse.drag,
        modifier: match mouse.drag_modifier {
            nkw_core::rules::DragModifier::Ctrl => pointer::DragMod::Ctrl,
            nkw_core::rules::DragModifier::Alt => pointer::DragMod::Alt,
            nkw_core::rules::DragModifier::World => pointer::DragMod::World,
            nkw_core::rules::DragModifier::None => pointer::DragMod::None,
        },
        world: match world_mod {
            crate::router::WorldMod::CtrlAlt => pointer::WorldKey::CtrlAlt,
            crate::router::WorldMod::Super => pointer::WorldKey::Super,
        },
        threshold_px: mouse.drag_threshold_px,
        double_s: mouse.double_click_ms as f64 / 1000.0,
    }
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
