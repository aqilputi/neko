//! SALTO labels (§3.8): one or two letters from `hint.alphabet` on every
//! visible interactive thing. No label is a prefix of another, the shortest
//! go to what is nearest the centre of the view, and a thing keeps its
//! label from frame to frame while it stays visible.

use std::collections::BTreeMap;

/// The default `hint.alphabet`.
pub const ALPHABET: &str = "asdfghjkl";

/// At least `n` prefix-free labels over `alphabet`, shortest first (the
/// vimium scheme: keep splitting the first label into one per letter).
/// May return more than `n`: the spares keep the same lengths.
/// An alphabet of fewer than two letters cannot make labels: `ALPHABET` is
/// used instead.
pub fn labels(alphabet: &str, n: usize) -> Vec<String> {
    let mut letters: Vec<char> = Vec::new();
    for c in alphabet.chars() {
        if !letters.contains(&c) {
            letters.push(c);
        }
    }
    if letters.len() < 2 {
        letters = ALPHABET.chars().collect();
    }
    let mut queue = vec![String::new()];
    let mut first = 0;
    while queue.len() - first < n || queue.len() == 1 {
        let head = queue[first].clone();
        first += 1;
        queue.extend(letters.iter().map(|c| format!("{head}{c}")));
    }
    queue.split_off(first)
}

/// What the typed letters so far point at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HintMatch {
    /// a whole label: jump there
    Hit(u64),
    /// the start of at least one label: keep typing
    Partial,
    /// nothing: the caller beeps or clears
    None,
}

/// The labels of one SALTO session, kept stable across frames.
#[derive(Clone, Debug)]
pub struct Hints {
    alphabet: String,
    /// every label this session can hand out, shortest first
    pool: Vec<String>,
    by_id: BTreeMap<u64, String>,
}

impl Hints {
    pub fn new(alphabet: &str) -> Hints {
        Hints {
            alphabet: alphabet.to_string(),
            pool: Vec::new(),
            by_id: BTreeMap::new(),
        }
    }

    /// This frame's visible things as (id, distance from the view centre).
    /// Things still visible keep their labels; new ones take the shortest
    /// free labels, nearest first; only when the pool runs out is
    /// everything relabelled.
    pub fn update(&mut self, visible: &[(u64, f32)]) {
        let mut sorted = visible.to_vec();
        sorted.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)));
        sorted.dedup_by_key(|t| t.0);
        self.by_id.retain(|id, _| sorted.iter().any(|t| t.0 == *id));
        let new: Vec<u64> = sorted
            .iter()
            .map(|t| t.0)
            .filter(|id| !self.by_id.contains_key(id))
            .collect();
        let free: Vec<String> = self
            .pool
            .iter()
            .filter(|l| !self.by_id.values().any(|u| u == *l))
            .cloned()
            .collect();
        if free.len() >= new.len() {
            self.by_id.extend(new.into_iter().zip(free));
        } else {
            self.pool = labels(&self.alphabet, sorted.len());
            self.by_id = sorted
                .iter()
                .map(|t| t.0)
                .zip(self.pool.iter().cloned())
                .collect();
        }
    }

    pub fn label(&self, id: u64) -> Option<&str> {
        self.by_id.get(&id).map(String::as_str)
    }

    /// All current labels.
    pub fn all(&self) -> &BTreeMap<u64, String> {
        &self.by_id
    }

    pub fn resolve(&self, typed: &str) -> HintMatch {
        if let Some((id, _)) = self.by_id.iter().find(|(_, l)| *l == typed) {
            HintMatch::Hit(*id)
        } else if self.by_id.values().any(|l| l.starts_with(typed)) {
            HintMatch::Partial
        } else {
            HintMatch::None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prefix_free(ls: &[&String]) -> bool {
        ls.iter().enumerate().all(|(i, a)| {
            ls.iter()
                .enumerate()
                .all(|(j, b)| i == j || !b.starts_with(a.as_str()))
        })
    }

    #[test]
    fn few_things_get_single_letters() {
        let l = labels(ALPHABET, 5);
        assert!(l.len() >= 5);
        assert!(l.iter().all(|s| s.len() == 1));
    }

    #[test]
    fn labels_are_prefix_free_and_minimal() {
        for n in [1, 9, 10, 17, 40, 81, 82, 200] {
            let l = labels(ALPHABET, n);
            assert!(l.len() >= n, "{n}");
            let used: Vec<&String> = l.iter().take(n).collect();
            assert!(prefix_free(&used), "{n}: {used:?}");
            let all: Vec<&String> = l.iter().collect();
            assert!(prefix_free(&all), "spares too, {n}");
            let lens: Vec<usize> = l.iter().map(String::len).collect();
            assert!(lens.windows(2).all(|w| w[0] <= w[1]), "shortest first");
            assert!(l.iter().all(|s| s.chars().all(|c| ALPHABET.contains(c))));
        }
        // 10 things over 9 letters: 8 singles + 9 doubles cover 17
        let l = labels(ALPHABET, 10);
        assert_eq!(l.iter().filter(|s| s.len() == 1).count(), 8);
        assert_eq!(labels(ALPHABET, 81).iter().map(String::len).max(), Some(2));
    }

    #[test]
    fn repeated_letters_count_once_and_a_lone_letter_falls_back() {
        assert_eq!(labels("abab", 2), ["a", "b"]);
        assert_eq!(labels("aaa", 2), labels(ALPHABET, 2));
    }

    #[test]
    fn nearest_the_centre_gets_the_shortest() {
        let mut h = Hints::new(ALPHABET);
        let things: Vec<(u64, f32)> = (0..12).map(|i| (i, 100.0 - i as f32)).collect();
        h.update(&things);
        assert_eq!(h.all().len(), 12);
        // 11 is nearest, 0 is farthest
        assert_eq!(h.label(11).unwrap().len(), 1);
        assert_eq!(h.label(0).unwrap().len(), 2);
        let ls: Vec<&String> = h.all().values().collect();
        assert!(prefix_free(&ls));
    }

    #[test]
    fn labels_are_stable_across_frames() {
        let mut h = Hints::new(ALPHABET);
        h.update(&[(1, 5.0), (2, 1.0), (3, 9.0)]);
        let before = h.all().clone();
        // things move around the view, one leaves, one comes
        h.update(&[(1, 0.5), (3, 2.0), (4, 0.1)]);
        assert_eq!(h.label(1), before.get(&1).map(String::as_str));
        assert_eq!(h.label(3), before.get(&3).map(String::as_str));
        assert_eq!(h.label(2), None);
        let l4 = h.label(4).unwrap().to_string();
        assert!(
            !before
                .values()
                .any(|v| *v == l4 && before.get(&2) != Some(v))
        );
        let ls: Vec<&String> = h.all().values().collect();
        assert!(prefix_free(&ls));
    }

    #[test]
    fn running_out_relabels_everything_prefix_free() {
        let mut h = Hints::new(ALPHABET);
        h.update(&[(1, 0.0), (2, 1.0)]);
        let many: Vec<(u64, f32)> = (1..30).map(|i| (i, i as f32)).collect();
        h.update(&many);
        assert_eq!(h.all().len(), 29);
        let ls: Vec<&String> = h.all().values().collect();
        assert!(prefix_free(&ls));
    }

    #[test]
    fn typing_resolves() {
        let mut h = Hints::new(ALPHABET);
        let things: Vec<(u64, f32)> = (0..12).map(|i| (i, i as f32)).collect();
        h.update(&things);
        let long = h.label(11).unwrap().to_string();
        assert_eq!(long.len(), 2);
        assert_eq!(h.resolve(&long[..1]), HintMatch::Partial);
        assert_eq!(h.resolve(&long), HintMatch::Hit(11));
        assert_eq!(h.resolve(h.label(0).unwrap()), HintMatch::Hit(0));
        assert_eq!(h.resolve("zz"), HintMatch::None);
        assert_eq!(h.resolve(""), HintMatch::Partial);
    }
}
