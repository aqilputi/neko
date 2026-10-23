//! The vault (addendum §5): in the library, guarded by the librarian. `t`
//! or `interact` before it opens a CONVERSA with purpose `VaultUnlock`; the
//! phrase typed on the dialogue's line goes to `nkw_objects::vault::Vault`
//! (never to the core, never echoed, never logged). Open, it shows the HUD
//! badge and its cards (names only, read at unlock, dropped at lock); it
//! locks again on its own — `Timeout`, `LeftRoom`, `FocusLost`,
//! `EnteredFocus` (any FOCO), `Shot` — or by hand (`Manual`). Every event is
//! one line of `vault.log` (0600, never under `--demo`).

use nkw_core::{Action, CatId, Effect, TalkPurpose, Time};
use nkw_objects::sound::Cue;
use nkw_objects::vault::{DenyReason, Effect as VEffect, Event, LockReason, Params, State, Vault};

use super::audit::AuditLog;
use crate::ext::{DialogView, LineView, Req, ToastKind};

pub const ASK: &str = "Esse é o cofre. Se é você mesmo, diga a palavra.";
pub const OPENED: &str = "Pronto. Eu fecho daqui a pouco.";
pub const WRONG: &str = "Hmm, não foi isso.";
pub const WAIT: &str = "Calma, um instante.";
pub const LOCKOUT: &str = "Volte daqui a pouco.";
/// The librarian's name tag in the dialogue box.
pub const NAME: &str = "bibliotecária";
const RGB: [u8; 3] = [0xc8, 0xa8, 0xe0];
/// The door: open 420 ms + 60 ms settle, shut 320 ms (§5.10).
pub const OPEN_S: f64 = 0.48;
pub const SHUT_S: f64 = 0.32;
/// After the phrase, the CONVERSA ends on its own.
pub const END_AFTER_S: f64 = 1.4;

/// `[vault]` → the machine's parameters.
pub fn params(c: &crate::objects::library::model::VaultConf) -> Params {
    Params {
        phrase: c.phrase.clone(),
        auto_lock_s: c.auto_lock_after_s,
        cooldown_s: c.cooldown_s,
        max_fails: c.max_fails,
        lockout_s: c.lockout_s,
    }
}

/// What changed for the library (the cards).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
    Unlocked,
    Locked(LockReason),
}

#[derive(Default)]
pub struct VaultGlue {
    pub vault: Vault,
    pub p: Params,
    /// the CONVERSA is ours
    open: bool,
    line: bool,
    text: &'static str,
    pub log: AuditLog,
    /// the door: open?, since when
    door: (bool, Time),
    /// end our CONVERSA at
    end_at: Option<Time>,
    /// transitions since the last `take_changes`
    changes: Vec<Change>,
    /// the last lock's reason (the dump)
    pub last_lock: Option<LockReason>,
}

impl VaultGlue {
    pub fn new(p: Params, log: AuditLog) -> VaultGlue {
        VaultGlue {
            p,
            log,
            door: (false, f64::NEG_INFINITY),
            ..VaultGlue::default()
        }
    }

    pub fn unlocked(&self) -> bool {
        self.vault.unlocked()
    }

    /// "locked" | "unlocked" | "lockout"
    pub fn state_name(&self) -> &'static str {
        match self.vault.state() {
            State::Locked => "locked",
            State::Unlocked { .. } => "unlocked",
            State::Lockout { .. } => "lockout",
        }
    }

    pub fn take_changes(&mut self) -> Vec<Change> {
        std::mem::take(&mut self.changes)
    }

    /// `Interact` on the vault: open → lock it (`Manual`); shut → talk to
    /// the librarian about it.
    pub fn prompt(&mut self, librarian: Option<CatId>, now: Time) -> (Vec<Req>, Vec<Cue>) {
        if self.unlocked() {
            return (Vec::new(), self.lock(now));
        }
        let r = match librarian {
            Some(cat) => vec![Req::Action(Action::Talk {
                cat,
                purpose: TalkPurpose::VaultUnlock,
            })],
            None => vec![Req::Toast {
                text: "a bibliotecária não está: o cofre fica fechado".into(),
                kind: ToastKind::Info,
                ms: 1800,
            }],
        };
        (r, Vec::new())
    }

    /// A CONVERSA effect routed to the objects (purpose `VaultUnlock`);
    /// `line`: the text sent, on `SendLine`. Returns the requests and the
    /// cues to play at the vault.
    pub fn effect(&mut self, e: &Effect, line: Option<&str>, now: Time) -> (Vec<Req>, Vec<Cue>) {
        let mut reqs = Vec::new();
        let mut cues = Vec::new();
        match e {
            Effect::FrameFace(_) => {
                self.open = true;
                self.line = false;
                self.text = ASK;
                self.end_at = None;
            }
            Effect::OpenLine => self.line = true,
            Effect::CloseLine => self.line = false,
            Effect::SendLine => {
                self.line = false;
                cues.extend(self.attempt(line.unwrap_or(""), now));
            }
            // the next key after the answer ends the talk
            Effect::TalkNext => reqs.push(Req::Action(Action::Back)),
            _ => {}
        }
        (reqs, cues)
    }

    /// The phrase, as the line would send it (also `inject objects
    /// {"vault":{"phrase":…}}`).
    pub fn attempt(&mut self, phrase: &str, now: Time) -> Vec<Cue> {
        let fx = self
            .vault
            .apply(Event::Attempt(phrase.to_string()), now, &self.p);
        self.absorb(fx, now)
    }

    fn absorb(&mut self, fx: Vec<VEffect>, now: Time) -> Vec<Cue> {
        let mut cues = Vec::new();
        for e in fx {
            match e {
                VEffect::Unlocked => {
                    self.text = OPENED;
                    self.door = (true, now);
                    self.end_at = Some(now + END_AFTER_S);
                    self.changes.push(Change::Unlocked);
                }
                VEffect::Denied(DenyReason::WrongPhrase { .. }) => self.text = WRONG,
                VEffect::Denied(DenyReason::Cooldown) => self.text = WAIT,
                VEffect::Denied(DenyReason::Lockout) => self.text = LOCKOUT,
                VEffect::Locked(r) => {
                    // a picture is about to be taken: shut at once, no swing
                    let since = if matches!(r, LockReason::Shot | LockReason::Capture) {
                        f64::NEG_INFINITY
                    } else {
                        now
                    };
                    self.door = (false, since);
                    self.last_lock = Some(r);
                    self.changes.push(Change::Locked(r));
                }
                VEffect::Audit(a) => self.log.write(&a),
                VEffect::Cue(c) => cues.push(c),
            }
        }
        cues
    }

    /// Every frame: `talking` = the machine is still in our CONVERSA;
    /// `in_room` = the camera is in the vault's room; `focus` = some FOCO is
    /// on. Returns the cues and the requests (the CONVERSA's own end).
    pub fn tick(
        &mut self,
        now: Time,
        talking: bool,
        in_room: bool,
        focus: bool,
    ) -> (Vec<Cue>, Vec<Req>) {
        let mut reqs = Vec::new();
        if !talking {
            self.open = false;
            self.line = false;
            self.end_at = None;
        } else if self.end_at.is_some_and(|t| now >= t) {
            self.end_at = None;
            reqs.push(Req::Action(Action::Back));
        }
        let mut fx = Vec::new();
        if self.vault.unlocked() && focus {
            fx.extend(self.vault.apply(Event::EnteredFocus, now, &self.p));
        }
        if self.vault.unlocked() && !in_room {
            fx.extend(self.vault.apply(Event::LeftRoom, now, &self.p));
        }
        fx.extend(self.vault.apply(Event::Tick, now, &self.p));
        (self.absorb(fx, now), reqs)
    }

    /// A screenshot is about to be taken: lock first.
    pub fn shot(&mut self, now: Time) -> Vec<Cue> {
        let fx = self.vault.apply(Event::Shot, now, &self.p);
        self.absorb(fx, now)
    }

    /// The host window lost the keyboard.
    pub fn focus_lost(&mut self, now: Time) -> Vec<Cue> {
        let fx = self.vault.apply(Event::FocusLost, now, &self.p);
        self.absorb(fx, now)
    }

    /// Reading a card, copying a path: the auto-lock starts over.
    pub fn touch(&mut self, now: Time) {
        let fx = self.vault.apply(Event::Touch, now, &self.p);
        self.absorb(fx, now);
    }

    pub fn lock(&mut self, now: Time) -> Vec<Cue> {
        let fx = self.vault.lock(LockReason::Manual, now);
        self.absorb(fx, now)
    }

    pub fn next_wake(&self, now: Time) -> Option<Time> {
        let mut t = self.vault.next_wake(now, &self.p);
        if let Some(e) = self.end_at {
            t = Some(t.map_or(e, |x| x.min(e)));
        }
        t
    }

    /// The door's openness 0..1 (a cut under reduced motion).
    pub fn openness(&self, now: Time, reduced: bool) -> f32 {
        let (open, since) = self.door;
        if reduced {
            return if open { 1.0 } else { 0.0 };
        }
        let t = now - since;
        if open {
            // 420 ms out with a small overshoot settled in the last 60 ms
            let k = (t / 0.42).clamp(0.0, 1.0) as f32;
            let ease = 1.0 - (1.0 - k).powi(3);
            let settle = if (0.42..OPEN_S).contains(&t) {
                0.04 * (1.0 - ((t - 0.42) / 0.06) as f32)
            } else {
                0.0
            };
            (ease + settle).min(1.04)
        } else {
            let k = (t / SHUT_S).clamp(0.0, 1.0) as f32;
            1.0 - k * k
        }
    }

    /// The door is moving.
    pub fn animating(&self, now: Time) -> bool {
        let (open, since) = self.door;
        now - since < if open { OPEN_S } else { SHUT_S }
    }

    pub fn dialog(&self) -> Option<DialogView> {
        self.open.then(|| DialogView {
            name: NAME.into(),
            rgb: RGB,
            text: self.text.into(),
            more: false,
            choices: Vec::new(),
            selected: 0,
            line: self.line.then(|| LineView {
                text: String::new(),
                cursor: 0,
            }),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn glue() -> VaultGlue {
        VaultGlue::new(Params::default(), AuditLog::default())
    }

    #[test]
    fn the_prompt_talks_to_the_librarian_or_locks_an_open_vault() {
        let mut v = glue();
        assert_eq!(
            v.prompt(Some(CatId(999_000)), 0.0).0,
            [Req::Action(Action::Talk {
                cat: CatId(999_000),
                purpose: TalkPurpose::VaultUnlock
            })]
        );
        assert!(matches!(&v.prompt(None, 0.0).0[..], [Req::Toast { .. }]));
        v.attempt("destrancar", 1.0);
        assert!(v.unlocked());
        let (r, cues) = v.prompt(Some(CatId(999_000)), 2.0);
        assert!(r.is_empty());
        assert!(!cues.is_empty());
        assert!(!v.unlocked());
        assert_eq!(v.last_lock, Some(LockReason::Manual));
    }

    #[test]
    fn the_phrase_opens_it_a_wrong_one_does_not_and_the_talk_ends_alone() {
        let mut v = glue();
        v.effect(&Effect::FrameFace(CatId(1)), None, 0.0);
        assert_eq!(v.dialog().unwrap().text, ASK);
        assert_eq!(v.dialog().unwrap().name, NAME);
        v.effect(&Effect::OpenLine, None, 0.0);
        assert!(v.dialog().unwrap().line.is_some());
        let line = v.dialog().unwrap().line.unwrap();
        assert!(line.text.is_empty(), "never echoed");
        v.effect(&Effect::SendLine, Some("abre-te sésamo"), 1.0);
        assert!(!v.unlocked());
        assert_eq!(v.dialog().unwrap().text, WRONG);
        let phrase = v.p.phrase.clone();
        let (_, cues) = v.effect(&Effect::SendLine, Some(&phrase), 10.0);
        assert!(v.unlocked());
        assert!(cues.contains(&Cue::VaultDial) && cues.contains(&Cue::VaultUnlock));
        assert_eq!(v.dialog().unwrap().text, OPENED);
        assert_eq!(v.take_changes(), [Change::Unlocked]);
        assert_eq!(v.next_wake(10.0), Some(10.0 + END_AFTER_S));
        let (_, r) = v.tick(10.5, true, true, false);
        assert!(r.is_empty());
        let (_, r) = v.tick(10.0 + END_AFTER_S, true, true, false);
        assert_eq!(r, [Req::Action(Action::Back)]);
        v.tick(11.5, false, true, false);
        assert!(v.dialog().is_none(), "the talk ended");
        assert!(v.unlocked(), "and the vault stays open");
    }

    #[test]
    fn every_auto_lock_reason_locks_and_reports() {
        let open = |v: &mut VaultGlue, t| {
            let phrase = v.p.phrase.clone();
            v.attempt(&phrase, t);
            assert!(v.unlocked());
            v.take_changes();
        };
        let mut v = glue();
        open(&mut v, 0.0);
        v.tick(1.0, false, false, false);
        assert_eq!(v.take_changes(), [Change::Locked(LockReason::LeftRoom)]);
        open(&mut v, 5.0);
        v.tick(6.0, false, true, true);
        assert_eq!(v.take_changes(), [Change::Locked(LockReason::EnteredFocus)]);
        open(&mut v, 10.0);
        v.tick(10.0 + v.p.auto_lock_s + 0.1, false, true, false);
        assert_eq!(v.take_changes(), [Change::Locked(LockReason::Timeout)]);
        open(&mut v, 40.0);
        v.touch(55.0);
        v.tick(65.0, false, true, false);
        assert!(v.unlocked(), "a touch renews");
        v.shot(66.0);
        assert_eq!(v.take_changes(), [Change::Locked(LockReason::Shot)]);
        assert_eq!(
            v.openness(66.0, false),
            0.0,
            "a shot shuts the door at once"
        );
        open(&mut v, 70.0);
        v.focus_lost(71.0);
        assert_eq!(v.take_changes(), [Change::Locked(LockReason::FocusLost)]);
        assert!(v.focus_lost(72.0).is_empty(), "already shut: nothing");
        assert!(v.take_changes().is_empty());
    }

    #[test]
    fn the_door_opens_in_420_ms_settles_and_shuts_in_320() {
        let mut v = glue();
        assert_eq!(v.openness(0.0, false), 0.0);
        v.attempt("destrancar", 1.0);
        assert!(v.animating(1.2));
        assert!(v.openness(1.2, false) > 0.3 && v.openness(1.2, false) < 1.0);
        assert!(
            v.openness(1.43, false) > 1.0,
            "the settle overshoots a hair"
        );
        assert!(!v.animating(1.0 + OPEN_S));
        assert!((v.openness(2.0, false) - 1.0).abs() < 1e-6);
        assert_eq!(v.openness(1.01, true), 1.0, "a cut under reduced motion");
        v.lock(3.0);
        assert!(v.openness(3.16, false) > 0.0);
        assert_eq!(v.openness(3.0 + SHUT_S, false), 0.0);
    }

    #[test]
    fn audit_lines_reach_the_file_without_the_phrase() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("vault.log");
        let mut v = VaultGlue::new(Params::default(), AuditLog::new(Some(p.clone())));
        v.attempt("palavra-errada", 0.0);
        v.attempt("destrancar", 5.0);
        v.lock(6.0);
        let text = std::fs::read_to_string(&p).unwrap();
        let whats: Vec<&str> = text.lines().map(|l| l.split(' ').nth(1).unwrap()).collect();
        assert_eq!(whats, ["unlock_denied", "unlock", "lock_manual"]);
        assert!(!text.contains("palavra-errada") && !text.contains("destrancar"));
    }
}
