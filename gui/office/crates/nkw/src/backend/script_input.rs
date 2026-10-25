//! A synthetic input backend for `--script`'s `pointer` and `button` lines:
//! they go through `Nkw::process_input` exactly as the host's events do
//! (the patched Smithay winit backend delivers `PointerMotion` the same
//! way), so headless tests cover the real path.

use std::path::PathBuf;

use smithay::backend::input::{
    AbsolutePositionEvent, Axis, AxisRelativeDirection, AxisSource, ButtonState, Device,
    DeviceCapability, Event, InputBackend, InputEvent, InputTime, PointerAxisEvent,
    PointerButtonEvent, PointerMotionEvent, UnusedEvent,
};

/// The script's backend marker.
#[derive(Debug)]
pub struct ScriptInput;

/// The script's only device: a pointer.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ScriptDevice;

impl Device for ScriptDevice {
    fn id(&self) -> String {
        "script".into()
    }
    fn name(&self) -> String {
        "nkw script pointer".into()
    }
    fn has_capability(&self, c: DeviceCapability) -> bool {
        c == DeviceCapability::Pointer
    }
    fn usb_id(&self) -> Option<(u32, u32)> {
        None
    }
    fn syspath(&self) -> Option<PathBuf> {
        None
    }
}

/// `pointer <dx> <dy>`
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Motion {
    pub dx: f64,
    pub dy: f64,
}

impl Event<ScriptInput> for Motion {
    fn time(&self) -> InputTime {
        InputTime::now()
    }
    fn device(&self) -> ScriptDevice {
        ScriptDevice
    }
}

impl PointerMotionEvent<ScriptInput> for Motion {
    fn delta_x(&self) -> f64 {
        self.dx
    }
    fn delta_y(&self) -> f64 {
        self.dy
    }
    fn delta_x_unaccel(&self) -> f64 {
        self.dx
    }
    fn delta_y_unaccel(&self) -> f64 {
        self.dy
    }
}

/// `button left|right down|up`
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Button {
    pub code: u32,
    pub pressed: bool,
}

impl Event<ScriptInput> for Button {
    fn time(&self) -> InputTime {
        InputTime::now()
    }
    fn device(&self) -> ScriptDevice {
        ScriptDevice
    }
}

impl PointerButtonEvent<ScriptInput> for Button {
    fn button_code(&self) -> u32 {
        self.code
    }
    fn state(&self) -> ButtonState {
        if self.pressed {
            ButtonState::Pressed
        } else {
            ButtonState::Released
        }
    }
}

/// `pointer_to <x> <y>`: logical px of the output (the script already
/// speaks the output's logical space; clamped into it).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MotionTo {
    pub x: f64,
    pub y: f64,
}

impl Event<ScriptInput> for MotionTo {
    fn time(&self) -> InputTime {
        InputTime::now()
    }
    fn device(&self) -> ScriptDevice {
        ScriptDevice
    }
}

impl smithay::backend::input::PointerMotionAbsoluteEvent<ScriptInput> for MotionTo {}

impl AbsolutePositionEvent<ScriptInput> for MotionTo {
    fn x(&self) -> f64 {
        self.x
    }
    fn y(&self) -> f64 {
        self.y
    }
    fn x_transformed(&self, width: i32) -> f64 {
        self.x.clamp(0.0, width.max(1) as f64 - 1.0)
    }
    fn y_transformed(&self, height: i32) -> f64 {
        self.y.clamp(0.0, height.max(1) as f64 - 1.0)
    }
}

/// `scroll <steps>`: a vertical wheel of whole notches; `+` = away (wheel
/// up), which Wayland sends as a negative value (v120 = −steps·120).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Wheel {
    pub steps: i32,
}

impl Event<ScriptInput> for Wheel {
    fn time(&self) -> InputTime {
        InputTime::now()
    }
    fn device(&self) -> ScriptDevice {
        ScriptDevice
    }
}

impl PointerAxisEvent<ScriptInput> for Wheel {
    fn amount(&self, axis: Axis) -> Option<f64> {
        (axis == Axis::Vertical).then_some(-self.steps as f64 * 15.0)
    }
    fn amount_v120(&self, axis: Axis) -> Option<f64> {
        (axis == Axis::Vertical).then_some(-self.steps as f64 * 120.0)
    }
    fn source(&self) -> AxisSource {
        AxisSource::Wheel
    }
    fn relative_direction(&self, _axis: Axis) -> AxisRelativeDirection {
        AxisRelativeDirection::Identical
    }
}

impl InputBackend for ScriptInput {
    type Device = ScriptDevice;
    type KeyboardKeyEvent = UnusedEvent;
    type PointerAxisEvent = Wheel;
    type PointerButtonEvent = Button;
    type PointerMotionEvent = Motion;
    type PointerMotionAbsoluteEvent = MotionTo;
    type GestureSwipeBeginEvent = UnusedEvent;
    type GestureSwipeUpdateEvent = UnusedEvent;
    type GestureSwipeEndEvent = UnusedEvent;
    type GesturePinchBeginEvent = UnusedEvent;
    type GesturePinchUpdateEvent = UnusedEvent;
    type GesturePinchEndEvent = UnusedEvent;
    type GestureHoldBeginEvent = UnusedEvent;
    type GestureHoldEndEvent = UnusedEvent;
    type TouchDownEvent = UnusedEvent;
    type TouchUpEvent = UnusedEvent;
    type TouchMotionEvent = UnusedEvent;
    type TouchCancelEvent = UnusedEvent;
    type TouchFrameEvent = UnusedEvent;
    type TabletToolAxisEvent = UnusedEvent;
    type TabletToolProximityEvent = UnusedEvent;
    type TabletToolTipEvent = UnusedEvent;
    type TabletToolButtonEvent = UnusedEvent;
    type SwitchToggleEvent = UnusedEvent;
    type SpecialEvent = ();
}

/// The event of a `pointer` line.
pub fn motion(dx: f64, dy: f64) -> InputEvent<ScriptInput> {
    InputEvent::PointerMotion {
        event: Motion { dx, dy },
    }
}

/// The event of a `pointer_to` line.
pub fn motion_to(x: f64, y: f64) -> InputEvent<ScriptInput> {
    InputEvent::PointerMotionAbsolute {
        event: MotionTo { x, y },
    }
}

/// The event of a `scroll` line.
pub fn wheel(steps: i32) -> InputEvent<ScriptInput> {
    InputEvent::PointerAxis {
        event: Wheel { steps },
    }
}

/// The event of a `button` line.
pub fn button(code: u32, pressed: bool) -> InputEvent<ScriptInput> {
    InputEvent::PointerButton {
        event: Button { code, pressed },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_events_carry_their_values() {
        let InputEvent::PointerMotion { event } = motion(100.0, -2.0) else {
            panic!("motion");
        };
        assert_eq!((event.delta_x(), event.delta_y()), (100.0, -2.0));
        assert_eq!(event.delta_unaccel(), (100.0, -2.0).into());
        let InputEvent::PointerButton { event } = button(0x111, true) else {
            panic!("button");
        };
        assert_eq!(event.button_code(), 0x111);
        assert_eq!(event.state(), ButtonState::Pressed);
        assert_eq!(
            event.button(),
            Some(smithay::backend::input::MouseButton::Right)
        );
        let InputEvent::PointerMotionAbsolute { event } = motion_to(640.0, 900.0) else {
            panic!("motion_to");
        };
        let p = event.position_transformed((1280, 800).into());
        assert_eq!((p.x, p.y), (640.0, 799.0), "clamped to the output");
        let InputEvent::PointerAxis { event } = wheel(2) else {
            panic!("wheel");
        };
        assert_eq!(event.amount_v120(Axis::Vertical), Some(-240.0), "away = up");
        assert_eq!(event.amount_v120(Axis::Horizontal), None);
        assert_eq!(event.source(), AxisSource::Wheel);
        assert!(ScriptDevice.has_capability(DeviceCapability::Pointer));
        assert!(!ScriptDevice.has_capability(DeviceCapability::Keyboard));
    }
}
