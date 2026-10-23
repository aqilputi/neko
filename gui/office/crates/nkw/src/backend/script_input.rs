//! A synthetic input backend for `--script`'s `pointer` and `button` lines:
//! they go through `Nkw::process_input` exactly as the host's events do
//! (the patched Smithay winit backend delivers `PointerMotion` the same
//! way), so headless tests cover the real path.

use std::path::PathBuf;

use smithay::backend::input::{
    ButtonState, Device, DeviceCapability, Event, InputBackend, InputEvent, InputTime,
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

impl InputBackend for ScriptInput {
    type Device = ScriptDevice;
    type KeyboardKeyEvent = UnusedEvent;
    type PointerAxisEvent = UnusedEvent;
    type PointerButtonEvent = Button;
    type PointerMotionEvent = Motion;
    type PointerMotionAbsoluteEvent = UnusedEvent;
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
        assert!(ScriptDevice.has_capability(DeviceCapability::Pointer));
        assert!(!ScriptDevice.has_capability(DeviceCapability::Keyboard));
    }
}
