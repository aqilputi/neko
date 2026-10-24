# winit: emit `InputEvent::PointerMotion` (relative) from `DeviceEvent::PointerMotion`

PR text for upstream Smithay. The diff is `smithay-winit-relative-pointer.patch` next to this file, against
rev `24647fc8d00131eb7f388efed26e35ea316b0018`; `scripts/smithay-fork.sh` turns it into one commit on a branch.

> Note on naming: winit 0.30 called the device event `DeviceEvent::MouseMotion`; the winit 0.31 that Smithay
> uses at this rev calls it `DeviceEvent::PointerMotion { delta }`. Same event: the raw/relative motion of the
> pointer device, not the cursor position.

## Why

The winit backend declares `type PointerMotionEvent = UnusedEvent`, so a compositor running nested (inside
another Wayland compositor or X11) never receives `InputEvent::PointerMotion`. It only sees
`PointerMotionAbsolute`, i.e. the cursor's position inside the host window.

That is enough for a desktop, but not for a first-person view or a game-like camera. Mouse-look works like
this: lock the host cursor with `Window::set_cursor_grab(CursorGrabMode::Locked)`, hide it, and turn the
camera by the relative deltas. Once the cursor is locked, the absolute position stops changing, so the only
signal left is the relative motion. winit already receives it (on Wayland it binds `zwp_relative_pointer_v1`
for every seat and reports the unaccelerated deltas as `DeviceEvent::PointerMotion`), but the backend drops
it because `WinitEventLoopApp` does not implement `ApplicationHandler::device_event`.

Nested compositors are also where you develop and test a compositor. With this patch, a client that locks
the pointer (`zwp_pointer_constraints_v1` plus `zwp_relative_pointer_v1`, e.g. a game or a 3D viewer) can be
tested under winit the same way it runs under udev/libinput.

## What

About 60 lines in `src/backend/winit/{input.rs,mod.rs}`, with no new feature flag and no API removed:

- `WinitMouseRelativeEvent { time, delta }`, implementing `Event<WinitInput>` and
  `PointerMotionEvent<WinitInput>`. winit only gives the unaccelerated delta, so `delta_*_unaccel()` return
  the same values as `delta_*()`, and a comment says so.
- `type PointerMotionEvent = WinitMouseRelativeEvent` in `impl InputBackend for WinitInput`. It was
  `UnusedEvent`.
- `WinitEventLoopApp::device_event` maps `DeviceEvent::PointerMotion { delta }` to
  `InputEvent::PointerMotion`, using the backend's usual timestamp (`self.timestamp()`, the same clock as the
  other winit input events). Every other device event is still ignored.

A compositor that does not care about relative motion only gets one more `InputEvent` variant from the winit
backend. Anyone who matched `InputEvent::PointerMotion` before was matching an event that could never arrive.

## When the window is not focused

winit's default device-event filter is `DeviceEvents::WhenFocused`, and the patch does not change it.

- **Wayland host:** relative motion is delivered only while the host gives our surface pointer focus. That is
  how the protocol works, since `zwp_relative_pointer_v1` events follow `wl_pointer` focus. When the pointer
  leaves the window, or another window takes focus, the events stop. They never leak from other clients.
- **X11 host:** XInput2 raw motion is global, but `WhenFocused` filters it to the times when our window has
  focus.

A compositor that wants to stop reacting while it is unfocused can still check
`WindowEvent::Focused` / `WinitEvent::Focus(false)`. The patch does not do that check for you.

## `set_cursor_grab(Locked)` and the compositor's own pointer lock

The two are independent, and the patch needs neither:

- The relative events arrive with or without the grab. Without it, the host cursor also moves, so the
  compositor still sees `PointerMotionAbsolute` next to `PointerMotion`. With `CursorGrabMode::Locked` (winit
  uses `zwp_pointer_constraints_v1.lock_pointer` on Wayland), the absolute events stop and only the relative
  ones keep coming. That is the mouse-look case.
- `Locked` can fail: some X11 setups, and hosts without `pointer-constraints`. A compositor should then fall
  back to `CursorGrabMode::Confined`. Relative events still flow, but the hidden cursor can reach the edge of
  the window.
- If the nested compositor in turn gives its *own* clients pointer constraints (Smithay's
  `PointerConstraintsState` and `RelativePointerManagerState`), it forwards these host deltas to the client
  that holds the lock through `RelativePointerHandle::relative_motion`. The host lock (winit) and the client
  lock (Smithay) are separate. The compositor decides when to take the host lock: while its client holds a
  lock, or while it does its own mouse-look.
- Releasing the host lock with `CursorGrabMode::None` and `set_cursor_visible(true)` does not stop the
  relative events. The compositor ignores them when it does not need them.

## How it was tested

- In the downstream compositor (nkw, a nested Smithay compositor with a first-person "walk" mode), the stream
  is fed through the same `process_input` path as a real host:
  `cargo test -p nkw --test walk_headless` (relative motions turn the camera, clamped pitch, no roll) and the
  `pointer <dx> <dy>` script command in the headless e2e tests.
- Nested in niri (Wayland) on Intel iris: 10 minutes of walking with mouse-look, by hand, following this
  checklist (fill in before opening the PR: date, host compositor version, result):
  - entering the walk mode locks and hides the host cursor, and leaving it releases and shows it;
  - turning by mouse never rolls the camera and never changes the FOV;
  - pitch stops at its limits, and yaw wraps without a jump;
  - snap turning still works with the keyboard while the mouse is locked;
  - moving the pointer out of the window, or focusing another host window, stops the motion (no events
    while unfocused), and focusing the window again brings it back;
  - the comfort vignette (optional) appears while turning and fades out afterwards;
  - leaving the walk mode lands on a station with the cursor back.
- Before opening: `cargo check --features backend_winit` and `cargo test` in the Smithay checkout that
  `scripts/smithay-fork.sh` builds, plus `cargo clippy` on the touched files. No existing test changes,
  since nothing covered the unused event.

## Not in this PR

- Accelerated deltas, which winit does not expose.
- A way to turn off device events from the backend's builder. Compositors can ignore them, and
  `DeviceEvents::Never` is reachable through winit if anyone ever needs it.
