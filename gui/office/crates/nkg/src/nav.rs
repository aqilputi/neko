//! The view state machine: office ↔ zoomed ↔ insert, and the focused
//! column. Pure: actions in, effects out (the Bevy side opens/closes panes
//! and moves the camera).

/// Where the user is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// The whole office, the focused column centred.
    Office,
    /// Zoomed onto the focused monitor; keys are commands (NORMAL).
    Zoom,
    /// Zoomed, keys go to the terminal (INSERT); only ctrl+] gets out.
    Insert,
}

/// What the user asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Act {
    Left,
    Right,
    /// focus a column (click, inbox cat, summon)
    Focus(usize),
    /// focus a column and zoom in (click on a monitor/inbox cat, summoned)
    FocusZoom(usize),
    /// enter: zoom onto the focused monitor
    Zoom,
    /// i: zoom (if needed) and type into the terminal
    Insert,
    /// ctrl+]: back to NORMAL, staying zoomed
    Normal,
    /// esc: back to the office
    Back,
    /// w: close the focused pane (kill) and go back to the office
    Close,
}

/// What the Bevy side must do after an action.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect {
    /// make sure the column's session has a pane open
    OpenPane(usize),
    /// close (kill) the column's pane
    ClosePane(usize),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Nav {
    pub focus: usize,
    pub mode: Mode,
    pub len: usize,
}

impl Nav {
    pub fn new(len: usize) -> Nav {
        Nav {
            focus: 0,
            mode: Mode::Office,
            len,
        }
    }

    pub fn zoomed(&self) -> bool {
        self.mode != Mode::Office
    }

    /// The column count changed (sessions came or went): keep focus valid;
    /// with no columns there is nothing to zoom onto.
    pub fn resize(&mut self, len: usize, keep: Option<usize>) {
        self.len = len;
        if let Some(i) = keep {
            self.focus = i;
        }
        if len == 0 {
            self.focus = 0;
            self.mode = Mode::Office;
        } else if self.focus >= len {
            self.focus = len - 1;
        }
    }

    pub fn apply(&mut self, act: Act) -> Vec<Effect> {
        if self.len == 0 {
            return vec![];
        }
        let mut fx = vec![];
        match act {
            Act::Left | Act::Right => {
                if self.mode == Mode::Insert {
                    return fx;
                }
                let next = match act {
                    Act::Left => self.focus.saturating_sub(1),
                    _ => (self.focus + 1).min(self.len - 1),
                };
                if next != self.focus {
                    self.focus = next;
                    if self.mode == Mode::Zoom {
                        fx.push(Effect::OpenPane(next));
                    }
                }
            }
            Act::Focus(i) => {
                if i < self.len {
                    self.focus = i;
                    if self.zoomed() {
                        self.mode = Mode::Zoom;
                        fx.push(Effect::OpenPane(i));
                    }
                }
            }
            Act::FocusZoom(i) => {
                if i < self.len {
                    self.focus = i;
                    self.mode = Mode::Zoom;
                    fx.push(Effect::OpenPane(i));
                }
            }
            Act::Zoom => {
                if self.mode == Mode::Office {
                    self.mode = Mode::Zoom;
                    fx.push(Effect::OpenPane(self.focus));
                }
            }
            Act::Insert => {
                if self.mode == Mode::Office {
                    fx.push(Effect::OpenPane(self.focus));
                }
                self.mode = Mode::Insert;
            }
            Act::Normal => {
                if self.mode == Mode::Insert {
                    self.mode = Mode::Zoom;
                }
            }
            Act::Back => {
                if self.mode == Mode::Zoom {
                    self.mode = Mode::Office;
                }
            }
            Act::Close => {
                if self.mode == Mode::Zoom {
                    fx.push(Effect::ClosePane(self.focus));
                    self.mode = Mode::Office;
                }
            }
        }
        fx
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn office_navigation() {
        let mut n = Nav::new(3);
        assert!(n.apply(Act::Left).is_empty());
        assert_eq!(n.focus, 0);
        n.apply(Act::Right);
        n.apply(Act::Right);
        n.apply(Act::Right);
        assert_eq!(n.focus, 2);
        assert_eq!(n.mode, Mode::Office);
    }

    #[test]
    fn zoom_insert_and_back() {
        let mut n = Nav::new(3);
        assert_eq!(n.apply(Act::Zoom), vec![Effect::OpenPane(0)]);
        assert_eq!(n.mode, Mode::Zoom);
        assert!(n.apply(Act::Zoom).is_empty());
        // zoomed h/l: the neighbour's pane opens, previous keeps running
        assert_eq!(n.apply(Act::Right), vec![Effect::OpenPane(1)]);
        assert_eq!(n.focus, 1);
        n.apply(Act::Insert);
        assert_eq!(n.mode, Mode::Insert);
        // in insert, h/l/esc are the terminal's
        assert!(n.apply(Act::Right).is_empty());
        n.apply(Act::Back);
        assert_eq!(n.mode, Mode::Insert);
        n.apply(Act::Normal);
        assert_eq!(n.mode, Mode::Zoom);
        n.apply(Act::Back);
        assert_eq!(n.mode, Mode::Office);
        assert_eq!(n.focus, 1);
    }

    #[test]
    fn insert_from_office_opens_the_pane() {
        let mut n = Nav::new(2);
        assert_eq!(n.apply(Act::Insert), vec![Effect::OpenPane(0)]);
        assert_eq!(n.mode, Mode::Insert);
    }

    #[test]
    fn close_kills_and_returns() {
        let mut n = Nav::new(2);
        assert!(
            n.apply(Act::Close).is_empty(),
            "w in the office does nothing"
        );
        n.apply(Act::FocusZoom(1));
        assert_eq!(n.apply(Act::Close), vec![Effect::ClosePane(1)]);
        assert_eq!(n.mode, Mode::Office);
    }

    #[test]
    fn focus_while_zoomed_follows() {
        let mut n = Nav::new(3);
        n.apply(Act::Insert);
        assert_eq!(n.apply(Act::Focus(2)), vec![Effect::OpenPane(2)]);
        assert_eq!(n.mode, Mode::Zoom);
        assert!(n.apply(Act::Focus(9)).is_empty());
    }

    #[test]
    fn resize_keeps_focus_valid() {
        let mut n = Nav::new(3);
        n.apply(Act::FocusZoom(2));
        n.resize(2, None);
        assert_eq!(n.focus, 1);
        n.resize(0, None);
        assert_eq!(n.mode, Mode::Office);
        assert!(n.apply(Act::Zoom).is_empty());
        n.resize(4, Some(3));
        assert_eq!(n.focus, 3);
    }
}
