//! Undo/redo of layout operations (plan §2.10). The World's snapshot type
//! is `S`; an entry keeps the subjects' state before and after.

use std::collections::VecDeque;

use serde::Serialize;

use crate::Time;

pub const HISTORY_CAP: usize = 50;
pub const COALESCE_S: Time = 1.0;

/// What an entry did.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OpKind {
    Move,
    Resize,
    Scale,
    Rotate,
    Dock,
    Undock,
    Cluster,
    Uncluster,
    LayoutLoad,
    Pin,
    ToDesk,
    ToWall,
    ToAir,
    Frame,
}

/// One undoable step.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry<S> {
    pub kind: OpKind,
    /// pt-BR, "mover firefox"
    pub label: String,
    pub subjects: Vec<u64>,
    pub before: S,
    pub after: S,
    pub at: Time,
}

/// The undo and redo stacks.
#[derive(Clone, Debug)]
pub struct History<S> {
    cap: usize,
    /// (serial, entry), oldest first
    done: VecDeque<(u64, Entry<S>)>,
    undone: Vec<(u64, Entry<S>)>,
    serial: u64,
}

impl<S: Clone> History<S> {
    pub fn new(cap: usize) -> History<S> {
        History {
            cap: cap.max(1),
            done: VecDeque::new(),
            undone: Vec::new(),
            serial: 0,
        }
    }

    /// Same kind and subjects within COALESCE_S of the last entry: keep its
    /// `before`, take this `after` (ten `h` in a second = one op). Clears
    /// the redo stack. Beyond `cap`, the oldest goes.
    pub fn push(&mut self, e: Entry<S>) {
        self.undone.clear();
        if let Some((_, last)) = self.done.back_mut()
            && last.kind == e.kind
            && last.subjects == e.subjects
            && (e.at - last.at).abs() <= COALESCE_S
        {
            last.after = e.after;
            last.at = e.at;
            return;
        }
        self.serial += 1;
        self.done.push_back((self.serial, e));
        while self.done.len() > self.cap {
            self.done.pop_front();
        }
    }

    /// To the redo stack; the caller applies `before`.
    pub fn undo(&mut self) -> Option<&Entry<S>> {
        let e = self.done.pop_back()?;
        self.undone.push(e);
        self.undone.last().map(|(_, e)| e)
    }

    /// Back; the caller applies `after`.
    pub fn redo(&mut self) -> Option<&Entry<S>> {
        let e = self.undone.pop()?;
        self.done.push_back(e);
        self.done.back().map(|(_, e)| e)
    }

    /// ARRUMAR's start.
    pub fn mark(&self) -> u64 {
        self.serial
    }

    /// Newest first, every entry pushed after `mark`.
    pub fn undo_to(&mut self, mark: u64) -> Vec<Entry<S>> {
        let mut out = Vec::new();
        loop {
            match self.done.back() {
                Some((serial, _)) if *serial > mark => {}
                _ => break,
            }
            match self.undo() {
                Some(e) => out.push(e.clone()),
                None => break,
            }
        }
        out
    }

    pub fn len(&self) -> usize {
        self.done.len()
    }

    pub fn is_empty(&self) -> bool {
        self.done.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.undone.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(kind: OpKind, subjects: &[u64], before: i32, after: i32, at: Time) -> Entry<i32> {
        Entry {
            kind,
            label: format!("{kind:?}"),
            subjects: subjects.to_vec(),
            before,
            after,
            at,
        }
    }

    #[test]
    fn push_and_undo_redo_round_trip() {
        let mut h: History<i32> = History::new(HISTORY_CAP);
        assert!(h.is_empty());
        h.push(entry(OpKind::Move, &[1], 0, 1, 0.0));
        assert_eq!(h.len(), 1);
        let got = h.undo().cloned().unwrap();
        assert_eq!((got.before, got.after), (0, 1));
        assert!(h.is_empty());
        assert!(h.can_redo());
        let got = h.redo().cloned().unwrap();
        assert_eq!((got.before, got.after), (0, 1));
        assert_eq!(h.len(), 1);
        assert!(!h.can_redo());
    }

    #[test]
    fn undo_and_redo_are_none_when_there_is_nothing() {
        let mut h: History<i32> = History::new(HISTORY_CAP);
        assert_eq!(h.undo(), None);
        assert_eq!(h.redo(), None);
    }

    #[test]
    fn pushing_clears_the_redo_stack() {
        let mut h: History<i32> = History::new(HISTORY_CAP);
        h.push(entry(OpKind::Move, &[1], 0, 1, 0.0));
        h.undo();
        assert!(h.can_redo());
        // an unrelated push (far enough apart not to coalesce) wipes redo
        h.push(entry(OpKind::Resize, &[2], 0, 1, 10.0));
        assert!(!h.can_redo());
        assert_eq!(h.redo(), None);
    }

    #[test]
    fn same_kind_and_subject_within_the_window_coalesces() {
        let mut h: History<i32> = History::new(HISTORY_CAP);
        h.push(entry(OpKind::Move, &[1], 0, 1, 0.0));
        // nine more "nudges" inside one second: still a single entry
        for i in 1..10 {
            h.push(entry(OpKind::Move, &[1], i, i + 1, i as f64 * 0.1));
        }
        assert_eq!(h.len(), 1, "ten nudges in under a second coalesce to one");
        let only = &h.done[0].1;
        assert_eq!(only.before, 0, "keeps the very first before");
        assert_eq!(only.after, 10, "takes the latest after");
    }

    #[test]
    fn coalescing_needs_the_same_kind_subjects_and_a_tight_window() {
        let mut h: History<i32> = History::new(HISTORY_CAP);
        h.push(entry(OpKind::Move, &[1], 0, 1, 0.0));
        h.push(entry(OpKind::Resize, &[1], 1, 2, 0.1)); // different kind
        assert_eq!(h.len(), 2);
        h.push(entry(OpKind::Resize, &[2], 2, 3, 0.2)); // different subject
        assert_eq!(h.len(), 3);
        h.push(entry(OpKind::Resize, &[2], 3, 4, 0.2 + COALESCE_S + 0.01)); // too slow
        assert_eq!(h.len(), 4);
    }

    #[test]
    fn the_cap_drops_the_oldest() {
        let mut h: History<i32> = History::new(3);
        for i in 0..5 {
            // far enough apart (and different subjects) that nothing coalesces
            h.push(entry(
                OpKind::Move,
                &[i],
                i as i32,
                i as i32 + 1,
                i as f64 * 10.0,
            ));
        }
        assert_eq!(h.len(), 3);
        // the oldest two (subjects 0 and 1) are gone: undoing three times
        // reaches the bottom
        for _ in 0..3 {
            assert!(h.undo().is_some());
        }
        assert!(h.undo().is_none());
    }

    #[test]
    fn mark_and_undo_to_unwind_everything_since_arrumar_started() {
        let mut h: History<i32> = History::new(HISTORY_CAP);
        h.push(entry(OpKind::Move, &[1], 0, 1, 0.0));
        let mark = h.mark();
        h.push(entry(OpKind::Resize, &[2], 0, 1, 100.0));
        h.push(entry(OpKind::Rotate, &[3], 0, 1, 200.0));
        let undone = h.undo_to(mark);
        assert_eq!(undone.len(), 2);
        // newest first
        assert_eq!(undone[0].kind, OpKind::Rotate);
        assert_eq!(undone[1].kind, OpKind::Resize);
        // the entry from before the mark is untouched
        assert_eq!(h.len(), 1);
        // the serial counter keeps climbing (it is never reused): a fresh
        // push after unwinding still lands strictly after the old mark
        h.push(entry(OpKind::Move, &[1], 1, 2, 300.0));
        assert!(h.mark() > mark);
    }

    #[test]
    fn undo_to_the_current_mark_undoes_nothing() {
        let mut h: History<i32> = History::new(HISTORY_CAP);
        h.push(entry(OpKind::Move, &[1], 0, 1, 0.0));
        let mark = h.mark();
        assert_eq!(h.undo_to(mark), Vec::new());
        assert_eq!(h.len(), 1);
    }

    #[test]
    fn undo_to_a_mark_before_everything_undoes_all_of_it() {
        let mut h: History<i32> = History::new(HISTORY_CAP);
        let mark = h.mark();
        h.push(entry(OpKind::Move, &[1], 0, 1, 0.0));
        h.push(entry(OpKind::Resize, &[2], 0, 1, 100.0));
        let undone = h.undo_to(mark);
        assert_eq!(undone.len(), 2);
        assert!(h.is_empty());
    }
}
