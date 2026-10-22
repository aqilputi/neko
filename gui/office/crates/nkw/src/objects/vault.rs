//! The vault (addendum §5, the stretch of P5): a CONVERSA with the
//! receptionist whose purpose is `VaultUnlock`; the phrase typed on the
//! dialogue's line goes to `nkw_objects::vault::Vault`. Unlocked shows the
//! HUD badge; it locks again on its own (timeout), when you leave the
//! reception, and before a screenshot. No files, no library yet (phase 4):
//! the audit lines stay in memory.

use nkw_core::{Action, CatId, Effect, TalkPurpose, Time};
use nkw_objects::sound::Cue;
use nkw_objects::vault::{
    AuditLine, DenyReason, Effect as VEffect, Event, LockReason, Params, Vault,
};

use crate::ext::{DialogView, LineView, Req, ToastKind};

pub const ASK: &str = "Qual é a frase?";
pub const OPENED: &str = "Pronto, está aberto. Ele tranca sozinho.";
pub const WRONG: &str = "Não é essa.";
pub const WAIT: &str = "Calma, um instante.";
pub const LOCKOUT: &str = "Trancado por um minuto.";
/// The receptionist's name tag in the dialogue box.
pub const NAME: &str = "recepcionista";
const RGB: [u8; 3] = [0xf2, 0xc8, 0x8a];

#[derive(Default)]
pub struct VaultGlue {
    pub vault: Vault,
    pub p: Params,
    /// the CONVERSA is ours
    open: bool,
    line: bool,
    text: &'static str,
    /// in memory only (phase 4 writes `vault.log`)
    pub audit: Vec<AuditLine>,
}

impl VaultGlue {
    pub fn unlocked(&self) -> bool {
        self.vault.unlocked()
    }

    /// `Interact` on the vault: talk to the receptionist about it.
    pub fn prompt(&mut self, receptionist: Option<CatId>) -> Vec<Req> {
        match receptionist {
            Some(cat) => vec![Req::Action(Action::Talk {
                cat,
                purpose: TalkPurpose::VaultUnlock,
            })],
            None => vec![Req::Toast {
                text: "não há quem abra o cofre".into(),
                kind: ToastKind::Info,
                ms: 1500,
            }],
        }
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
            }
            Effect::OpenLine => self.line = true,
            Effect::CloseLine => self.line = false,
            Effect::SendLine => {
                self.line = false;
                let fx =
                    self.vault
                        .apply(Event::Attempt(line.unwrap_or("").to_string()), now, &self.p);
                cues.extend(self.absorb(fx));
            }
            // the next key after the answer ends the talk
            Effect::TalkNext => reqs.push(Req::Action(Action::Back)),
            _ => {}
        }
        (reqs, cues)
    }

    fn absorb(&mut self, fx: Vec<VEffect>) -> Vec<Cue> {
        let mut cues = Vec::new();
        for e in fx {
            match e {
                VEffect::Unlocked => self.text = OPENED,
                VEffect::Denied(DenyReason::WrongPhrase { .. }) => self.text = WRONG,
                VEffect::Denied(DenyReason::Cooldown) => self.text = WAIT,
                VEffect::Denied(DenyReason::Lockout) => self.text = LOCKOUT,
                VEffect::Locked(_) => {}
                VEffect::Audit(a) => self.audit.push(a),
                VEffect::Cue(c) => cues.push(c),
            }
        }
        cues
    }

    /// Every frame: `talking` = the machine is still in our CONVERSA;
    /// `in_reception` = the camera is in the vault's room.
    pub fn tick(&mut self, now: Time, talking: bool, in_reception: bool) -> Vec<Cue> {
        if !talking {
            self.open = false;
            self.line = false;
        }
        let mut fx = Vec::new();
        if self.vault.unlocked() && !in_reception {
            fx.extend(self.vault.apply(Event::LeftRoom, now, &self.p));
        }
        fx.extend(self.vault.apply(Event::Tick, now, &self.p));
        self.absorb(fx)
    }

    /// A screenshot is about to be taken: lock first.
    pub fn shot(&mut self, now: Time) -> Vec<Cue> {
        let fx = self.vault.apply(Event::Shot, now, &self.p);
        self.absorb(fx)
    }

    pub fn lock(&mut self, now: Time) -> Vec<Cue> {
        let fx = self.vault.lock(LockReason::Manual, now);
        self.absorb(fx)
    }

    pub fn next_wake(&self, now: Time) -> Option<Time> {
        self.vault.next_wake(now, &self.p)
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

    #[test]
    fn the_prompt_talks_to_the_receptionist() {
        let mut v = VaultGlue::default();
        assert_eq!(
            v.prompt(Some(CatId(1))),
            [Req::Action(Action::Talk {
                cat: CatId(1),
                purpose: TalkPurpose::VaultUnlock
            })]
        );
        assert!(matches!(&v.prompt(None)[..], [Req::Toast { .. }]));
    }

    #[test]
    fn the_phrase_opens_it_a_wrong_one_does_not() {
        let mut v = VaultGlue::default();
        v.effect(&Effect::FrameFace(CatId(1)), None, 0.0);
        assert_eq!(v.dialog().unwrap().text, ASK);
        v.effect(&Effect::OpenLine, None, 0.0);
        assert!(v.dialog().unwrap().line.is_some());
        let (_, _) = v.effect(&Effect::SendLine, Some("abre-te sésamo"), 1.0);
        assert!(!v.unlocked());
        assert_eq!(v.dialog().unwrap().text, WRONG);
        assert!(v.dialog().unwrap().line.is_none());
        let phrase = v.p.phrase.clone();
        let (_, cues) = v.effect(&Effect::SendLine, Some(&phrase), 10.0);
        assert!(v.unlocked());
        assert!(!cues.is_empty());
        assert_eq!(v.dialog().unwrap().text, OPENED);
        assert!(v.audit.iter().any(|a| a.what == "unlock"));
        let (r, _) = v.effect(&Effect::TalkNext, None, 11.0);
        assert_eq!(r, [Req::Action(Action::Back)]);
        v.tick(11.0, false, true);
        assert!(v.dialog().is_none(), "the talk ended");
    }

    #[test]
    fn it_locks_on_leaving_the_room_on_timeout_and_before_a_shot() {
        let open = |v: &mut VaultGlue, t| {
            let phrase = v.p.phrase.clone();
            v.effect(&Effect::SendLine, Some(&phrase), t);
            assert!(v.unlocked());
        };
        let mut v = VaultGlue::default();
        open(&mut v, 0.0);
        v.tick(1.0, false, false);
        assert!(!v.unlocked(), "left the reception");
        open(&mut v, 5.0);
        assert!(v.next_wake(5.0).is_some());
        v.tick(5.0 + v.p.auto_lock_s + 0.1, false, true);
        assert!(!v.unlocked(), "timeout");
        open(&mut v, 40.0);
        v.shot(41.0);
        assert!(!v.unlocked(), "a screenshot never shows it open");
    }
}
