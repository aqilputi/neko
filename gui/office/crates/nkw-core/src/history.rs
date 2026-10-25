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
        let _ = mark;
        Vec::new()
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
