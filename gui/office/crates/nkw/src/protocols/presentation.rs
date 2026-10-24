//! `wp_presentation` (presentation-time): after every submit the hub hands
//! over the surfaces it composed; their feedbacks are `presented` with the
//! submit instant (`CLOCK_MONOTONIC`), the host's period and a running
//! sequence. Nested, the host compositor paces us at its vblank, so the
//! flags say `VSYNC`; headless the software vblank does the same.

use std::time::Duration;

use smithay::desktop::utils::{
    OutputPresentationFeedback, take_presentation_feedback_surface_tree,
};
use smithay::output::Output;
use smithay::reexports::wayland_protocols::wp::presentation_time::server::wp_presentation_feedback;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{Monotonic, Time};
use smithay::wayland::presentation::Refresh;

/// The flags every feedback carries (no zero-copy: we always compose).
pub fn flags() -> wp_presentation_feedback::Kind {
    wp_presentation_feedback::Kind::Vsync
}

/// The output's refresh as the protocol wants it (unknown when zero).
pub fn refresh(period: Duration) -> Refresh {
    if period.is_zero() {
        Refresh::Unknown
    } else {
        Refresh::fixed(period)
    }
}

/// Every pending feedback of `surfaces` (whole trees: subsurfaces too) is
/// presented now. Surfaces not composed keep theirs for a later frame.
pub fn presented(
    output: &Output,
    surfaces: &[WlSurface],
    at: Duration,
    period: Duration,
    seq: u64,
) {
    let mut fb = OutputPresentationFeedback::new(output);
    for s in surfaces {
        take_presentation_feedback_surface_tree(
            s,
            &mut fb,
            |_, _| Some(output.clone()),
            |_, _| wp_presentation_feedback::Kind::empty(),
        );
    }
    fb.presented::<Time<Monotonic>, Monotonic>(at.into(), refresh(period), seq, flags());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refresh_and_flags() {
        assert_eq!(refresh(Duration::ZERO), Refresh::Unknown);
        let p = Duration::from_nanos(16_666_667);
        assert_eq!(refresh(p), Refresh::Fixed(p));
        assert!(flags().contains(wp_presentation_feedback::Kind::Vsync));
        assert!(!flags().contains(wp_presentation_feedback::Kind::ZeroCopy));
    }
}
