//! `zwp_text_input_v3`, only with `--text-input` (an evaluation flag).
//!
//! The state is constructed so a client can bind it and enable/disable its
//! text fields; the focus follows the keyboard (Smithay). Nothing commits
//! text: there is no input method behind it. What a nested IME would need:
//! the host's IME runs against *our* winit window (winit's `Ime` events:
//! preedit + commit for the window as a whole), so nkw would forward winit's
//! `Ime::Preedit`/`Ime::Commit` to the focused client's text-input
//! (`preedit_string`/`commit_string` + `done`), report the client's cursor
//! rectangle back with `set_ime_cursor_area` (FOCO coordinates) and enable
//! `set_ime_allowed` only while a FOCO client has text-input enabled. The
//! fork's winit backend does not surface `Ime` events today (phase 6).

use smithay::wayland::text_input::TextInputActivation;

use crate::state::Nkw;

impl TextInputActivation for Nkw {}
