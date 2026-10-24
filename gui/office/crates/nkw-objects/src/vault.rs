//! The vault (addendum §5): the lock state machine (phrase, cooldown,
//! lockout, auto-lock) and the path policy that decides what goes in it.
//!
//! The machine only emits `AuditLine`s; `nkw` writes them. Audit lines
//! never carry the typed phrase nor file names.

use serde::{Deserialize, Serialize};

use crate::Time;
use crate::sound::Cue;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Params {
    pub phrase: String,
    pub auto_lock_s: f64,
    pub cooldown_s: f64,
    pub max_fails: u8,
    pub lockout_s: f64,
}

impl Default for Params {
    fn default() -> Params {
        Params {
            phrase: "destrancar".into(),
            auto_lock_s: 20.0,
            cooldown_s: 2.0,
            max_fails: 5,
            lockout_s: 60.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum State {
    Locked,
    Unlocked { since: Time, last_touch: Time },
    Lockout { until: Time },
}

#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    /// the phrase typed to the librarian
    Attempt(String),
    /// reading a card, copying a path: renews the auto-lock
    Touch,
    LeftRoom,
    FocusLost,
    EnteredFocus,
    CaptureStarted,
    Tick,
    Shot,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LockReason {
    Timeout,
    LeftRoom,
    FocusLost,
    EnteredFocus,
    Capture,
    Shot,
    Manual,
}

impl LockReason {
    /// The audit word.
    pub fn audit(self) -> &'static str {
        match self {
            LockReason::Timeout => "lock_timeout",
            LockReason::LeftRoom => "lock_left_room",
            LockReason::FocusLost => "lock_focus_lost",
            LockReason::EnteredFocus => "lock_entered_focus",
            LockReason::Capture => "lock_capture",
            LockReason::Shot => "lock_shot",
            LockReason::Manual => "lock_manual",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DenyReason {
    WrongPhrase { fails: u8 },
    Cooldown,
    Lockout,
}

/// One line of `vault.log`: `<at> <what>[ <detail>]`.
#[derive(Clone, Debug, PartialEq)]
pub struct AuditLine {
    pub at: Time,
    /// "unlock" | "unlock_denied" | "lockout" | "lockout_end" | "lock_*"
    pub what: &'static str,
    pub detail: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Effect {
    Unlocked,
    Locked(LockReason),
    Denied(DenyReason),
    Audit(AuditLine),
    Cue(Cue),
}

fn audit(at: Time, what: &'static str, detail: Option<String>) -> Effect {
    Effect::Audit(AuditLine { at, what, detail })
}

#[derive(Clone, Debug, PartialEq)]
pub struct Vault {
    state: State,
    fails: u8,
    last_attempt: Option<Time>,
}

impl Default for Vault {
    fn default() -> Vault {
        Vault::new()
    }
}

impl Vault {
    pub fn new() -> Vault {
        Vault {
            state: State::Locked,
            fails: 0,
            last_attempt: None,
        }
    }

    pub fn state(&self) -> &State {
        &self.state
    }

    pub fn unlocked(&self) -> bool {
        matches!(self.state, State::Unlocked { .. })
    }

    /// Wrong attempts since the last unlock or lockout.
    pub fn fails(&self) -> u8 {
        self.fails
    }

    pub fn apply(&mut self, ev: Event, now: Time, p: &Params) -> Vec<Effect> {
        // deadlines first: an expired unlock never takes one more touch
        let mut out = self.expire(now, p);
        match ev {
            Event::Attempt(phrase) => out.extend(self.attempt(&phrase, now, p)),
            Event::Touch => {
                if let State::Unlocked { since, .. } = self.state {
                    self.state = State::Unlocked {
                        since,
                        last_touch: now,
                    };
                }
            }
            Event::Tick => {}
            Event::LeftRoom => out.extend(self.lock(LockReason::LeftRoom, now)),
            Event::FocusLost => out.extend(self.lock(LockReason::FocusLost, now)),
            Event::EnteredFocus => out.extend(self.lock(LockReason::EnteredFocus, now)),
            Event::CaptureStarted => out.extend(self.lock(LockReason::Capture, now)),
            Event::Shot => out.extend(self.lock(LockReason::Shot, now)),
        }
        out
    }

    fn expire(&mut self, now: Time, p: &Params) -> Vec<Effect> {
        match self.state {
            State::Unlocked { last_touch, .. } if now - last_touch >= p.auto_lock_s => {
                self.lock(LockReason::Timeout, now)
            }
            State::Lockout { until } if now >= until => {
                self.state = State::Locked;
                self.fails = 0;
                vec![audit(now, "lockout_end", None)]
            }
            _ => Vec::new(),
        }
    }

    fn attempt(&mut self, phrase: &str, now: Time, p: &Params) -> Vec<Effect> {
        match self.state {
            State::Unlocked { since, .. } => {
                self.state = State::Unlocked {
                    since,
                    last_touch: now,
                };
                return Vec::new();
            }
            State::Lockout { .. } => {
                return vec![
                    Effect::Denied(DenyReason::Lockout),
                    audit(now, "unlock_denied", Some("lockout".into())),
                ];
            }
            State::Locked => {}
        }
        if self.last_attempt.is_some_and(|t| now - t < p.cooldown_s) {
            return vec![
                Effect::Denied(DenyReason::Cooldown),
                audit(now, "unlock_denied", Some("cooldown".into())),
            ];
        }
        self.last_attempt = Some(now);
        if phrase == p.phrase {
            self.fails = 0;
            self.state = State::Unlocked {
                since: now,
                last_touch: now,
            };
            return vec![
                Effect::Unlocked,
                audit(now, "unlock", None),
                Effect::Cue(Cue::VaultDial),
                Effect::Cue(Cue::VaultUnlock),
            ];
        }
        self.fails = self.fails.saturating_add(1);
        let fails = self.fails;
        let mut out = vec![
            Effect::Denied(DenyReason::WrongPhrase { fails }),
            audit(
                now,
                "unlock_denied",
                Some(format!("wrong_phrase fails={fails}")),
            ),
        ];
        if fails >= p.max_fails.max(1) {
            self.state = State::Lockout {
                until: now + p.lockout_s,
            };
            out.push(audit(now, "lockout", None));
        }
        out
    }

    /// When `Tick` has a deadline to enforce: the auto-lock or the end of
    /// the lockout.
    pub fn next_wake(&self, _now: Time, p: &Params) -> Option<Time> {
        match self.state {
            State::Locked => None,
            State::Unlocked { last_touch, .. } => Some(last_touch + p.auto_lock_s),
            State::Lockout { until } => Some(until),
        }
    }

    /// Locks an open vault; a no-op (no audit) otherwise.
    pub fn lock(&mut self, reason: LockReason, now: Time) -> Vec<Effect> {
        if !self.unlocked() {
            return Vec::new();
        }
        self.state = State::Locked;
        vec![
            Effect::Locked(reason),
            audit(now, reason.audit(), None),
            Effect::Cue(Cue::VaultLock),
        ]
    }
}

/// What goes in the vault, and the library entry it partitions: one
/// definition, in `nkw_core::library` (wave 4 moved the duplicate there).
/// `Policy::{default_policy, grow, is_vault}` come with the type.
pub use nkw_core::library::{Entry, Policy, partition};

#[cfg(test)]
mod tests {
    use super::*;

    fn p() -> Params {
        Params::default()
    }

    fn open(v: &mut Vault, now: Time) {
        let out = v.apply(Event::Attempt("destrancar".into()), now, &p());
        assert_eq!(out[0], Effect::Unlocked);
    }

    fn whats(out: &[Effect]) -> Vec<&'static str> {
        out.iter()
            .filter_map(|e| match e {
                Effect::Audit(a) => Some(a.what),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn starts_locked() {
        let v = Vault::new();
        assert_eq!(v.state(), &State::Locked);
        assert!(!v.unlocked());
        assert_eq!(Vault::default(), v);
        assert_eq!(v.next_wake(0.0, &p()), None);
    }

    #[test]
    fn correct_phrase_unlocks_with_audit_and_cues() {
        let mut v = Vault::new();
        let out = v.apply(Event::Attempt("destrancar".into()), 5.0, &p());
        assert_eq!(
            out,
            [
                Effect::Unlocked,
                Effect::Audit(AuditLine {
                    at: 5.0,
                    what: "unlock",
                    detail: None
                }),
                Effect::Cue(Cue::VaultDial),
                Effect::Cue(Cue::VaultUnlock),
            ]
        );
        assert_eq!(
            v.state(),
            &State::Unlocked {
                since: 5.0,
                last_touch: 5.0
            }
        );
        assert!(v.unlocked());
    }

    #[test]
    fn wrong_phrase_is_denied_audited_and_counted_without_the_phrase() {
        let mut v = Vault::new();
        let out = v.apply(Event::Attempt("abracadabra".into()), 0.0, &p());
        assert_eq!(out[0], Effect::Denied(DenyReason::WrongPhrase { fails: 1 }));
        assert_eq!(whats(&out), ["unlock_denied"]);
        assert!(
            !format!("{out:?}").contains("abracadabra"),
            "never the phrase"
        );
        assert_eq!(v.fails(), 1);
        let out = v.apply(Event::Attempt("Destrancar".into()), 3.0, &p());
        assert_eq!(
            out[0],
            Effect::Denied(DenyReason::WrongPhrase { fails: 2 }),
            "exact match"
        );
        let out = v.apply(Event::Attempt(" destrancar".into()), 6.0, &p());
        assert_eq!(
            out[0],
            Effect::Denied(DenyReason::WrongPhrase { fails: 3 }),
            "no trim"
        );
    }

    #[test]
    fn cooldown_rejects_even_the_right_phrase() {
        let mut v = Vault::new();
        v.apply(Event::Attempt("nope".into()), 10.0, &p());
        let out = v.apply(Event::Attempt("destrancar".into()), 11.5, &p());
        assert_eq!(out[0], Effect::Denied(DenyReason::Cooldown));
        assert!(!v.unlocked());
        assert_eq!(v.fails(), 1, "a cooldown is not a fail");
        open(&mut v, 12.0);
    }

    #[test]
    fn five_fails_lock_out_for_60_s_then_locked_again() {
        let mut v = Vault::new();
        let mut out = Vec::new();
        for i in 0..5 {
            out = v.apply(Event::Attempt("x".into()), i as f64 * 3.0, &p());
        }
        assert_eq!(out[0], Effect::Denied(DenyReason::WrongPhrase { fails: 5 }));
        assert_eq!(whats(&out), ["unlock_denied", "lockout"]);
        assert_eq!(v.state(), &State::Lockout { until: 72.0 });
        assert_eq!(v.next_wake(13.0, &p()), Some(72.0));
        let out = v.apply(Event::Attempt("destrancar".into()), 40.0, &p());
        assert_eq!(out[0], Effect::Denied(DenyReason::Lockout));
        assert!(!v.unlocked());
        let out = v.apply(Event::Tick, 72.0, &p());
        assert_eq!(whats(&out), ["lockout_end"]);
        assert_eq!(v.state(), &State::Locked);
        assert_eq!(v.fails(), 0);
        open(&mut v, 75.0);
    }

    #[test]
    fn lockout_ends_lazily_on_the_next_attempt() {
        let mut v = Vault::new();
        for i in 0..5 {
            v.apply(Event::Attempt("x".into()), i as f64 * 3.0, &p());
        }
        let out = v.apply(Event::Attempt("destrancar".into()), 100.0, &p());
        assert_eq!(whats(&out), ["lockout_end", "unlock"]);
        assert!(v.unlocked());
    }

    #[test]
    fn touch_extends_and_tick_times_out_after_20_s() {
        let mut v = Vault::new();
        open(&mut v, 0.0);
        assert_eq!(v.next_wake(0.0, &p()), Some(20.0));
        assert!(v.apply(Event::Tick, 15.0, &p()).is_empty());
        v.apply(Event::Touch, 15.0, &p());
        assert_eq!(v.next_wake(15.0, &p()), Some(35.0));
        assert!(v.apply(Event::Tick, 34.9, &p()).is_empty());
        let out = v.apply(Event::Tick, 35.0, &p());
        assert_eq!(
            out,
            [
                Effect::Locked(LockReason::Timeout),
                Effect::Audit(AuditLine {
                    at: 35.0,
                    what: "lock_timeout",
                    detail: None
                }),
                Effect::Cue(Cue::VaultLock),
            ]
        );
        assert!(!v.unlocked());
        assert_eq!(v.next_wake(35.0, &p()), None);
    }

    #[test]
    fn a_late_touch_cannot_revive_an_expired_unlock() {
        let mut v = Vault::new();
        open(&mut v, 0.0);
        let out = v.apply(Event::Touch, 25.0, &p());
        assert_eq!(out[0], Effect::Locked(LockReason::Timeout));
        assert!(!v.unlocked());
    }

    #[test]
    fn every_event_locks_with_its_reason() {
        for (ev, reason, word) in [
            (Event::LeftRoom, LockReason::LeftRoom, "lock_left_room"),
            (Event::FocusLost, LockReason::FocusLost, "lock_focus_lost"),
            (
                Event::EnteredFocus,
                LockReason::EnteredFocus,
                "lock_entered_focus",
            ),
            (Event::CaptureStarted, LockReason::Capture, "lock_capture"),
            (Event::Shot, LockReason::Shot, "lock_shot"),
        ] {
            let mut v = Vault::new();
            open(&mut v, 0.0);
            let out = v.apply(ev.clone(), 1.0, &p());
            assert_eq!(out[0], Effect::Locked(reason), "{ev:?}");
            assert_eq!(whats(&out), [word]);
            assert!(!v.unlocked());
            // already locked: no-op, no second audit line
            assert!(v.apply(ev, 2.0, &p()).is_empty());
        }
        let mut v = Vault::new();
        open(&mut v, 0.0);
        assert_eq!(whats(&v.lock(LockReason::Manual, 1.0)), ["lock_manual"]);
        assert!(v.lock(LockReason::Manual, 1.5).is_empty());
    }

    #[test]
    fn attempt_while_open_just_touches() {
        let mut v = Vault::new();
        open(&mut v, 0.0);
        assert!(
            v.apply(Event::Attempt("whatever".into()), 10.0, &p())
                .is_empty()
        );
        assert_eq!(
            v.state(),
            &State::Unlocked {
                since: 0.0,
                last_touch: 10.0
            }
        );
    }

    #[test]
    fn the_policy_is_the_core_one() {
        // re-exports, not copies: the same types and the same function
        let pol: nkw_core::library::Policy = Policy::default_policy();
        assert!(pol.is_vault(".ssh/id_demo"));
        assert!(!pol.is_vault("projeto/notas.md"));
        let entries: Vec<nkw_core::library::Entry> = [".env", "notas.md"]
            .map(|r| Entry {
                rel: r.into(),
                is_dir: false,
                size: 1,
                mtime: 0,
                hidden: r.starts_with('.'),
            })
            .to_vec();
        let (shelf, vault) = partition(&pol, &entries);
        assert_eq!((shelf.len(), vault.len()), (1, 1));
        assert_eq!(vault[0].rel, ".env");
    }
}
