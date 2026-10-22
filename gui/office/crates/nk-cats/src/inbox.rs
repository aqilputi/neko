//! The inbox cats' queue (port of gui/web/src/inbox/queue.ts, in world
//! units along the aisle): which walkers are out, where each walks to and
//! when it gets there. Pure — every function takes `now` (ms) — so the whole
//! lifecycle (enter → walking → arrived → dismissed) is table-tested.

use std::collections::HashMap;

use crate::model::{ATT_BLOCKED, ATT_RETURNED};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    Blocked,
    Returned,
    Mail,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Queued,
    Waiting,
    Walking,
    Arrived,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Walker {
    /// `att:<id>` or `mail:<id>`
    pub key: String,
    pub session_id: String,
    pub label: String,
    pub kind: Kind,
    pub activity_at: i64,
    pub text: String,
    /// changes when what it brings changes
    pub sig: String,
    pub expires_at: Option<i64>,
    pub from: f32,
    pub to: f32,
    /// None = not out (in the "+N" badge)
    pub start_at: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Queue {
    pub walkers: Vec<Walker>,
    pub dismissed: HashMap<String, String>,
    pub texts: HashMap<String, String>,
    pub highlight: Option<(String, i64)>,
    pub last_start: Option<i64>,
}

/// Walking speed (world units per ms): ~1.7 units/s, a calm trot.
pub const SPEED: f32 = 0.0017;
/// Gap between two walkers entering (ms): single file.
pub const STAGGER: i64 = 450;
pub const MAX_VISIBLE: usize = 5;
pub const MAIL_FRESH_MS: i64 = 90_000;
pub const HIGHLIGHT_MS: i64 = 1500;
/// Two walkers of the same session stand this far apart.
pub const SIDE_STEP: f32 = 0.55;

/// What the queue reads from a session.
#[derive(Clone, Debug, PartialEq)]
pub struct Source {
    pub session_id: String,
    pub label: String,
    pub attention: i64,
    pub archived: bool,
    pub last_at_ms: i64,
    pub needs: String,
}

/// A mail cue: (at ms, sent).
pub type Mail = HashMap<String, (i64, bool)>;

fn rank(k: Kind) -> u8 {
    match k {
        Kind::Blocked => 0,
        Kind::Returned => 1,
        Kind::Mail => 2,
    }
}

/// blocked > returned > ✉, then most recent activity, then key.
pub fn compare(a: &Walker, b: &Walker) -> std::cmp::Ordering {
    rank(a.kind)
        .cmp(&rank(b.kind))
        .then(b.activity_at.cmp(&a.activity_at))
        .then(a.key.cmp(&b.key))
}

impl Queue {
    pub fn ordered(&self) -> Vec<&Walker> {
        let mut v: Vec<&Walker> = self.walkers.iter().collect();
        v.sort_by(|a, b| compare(a, b));
        v
    }

    pub fn visible(&self) -> Vec<&Walker> {
        let mut v = self.ordered();
        v.truncate(MAX_VISIBLE);
        v
    }

    pub fn overflow(&self) -> usize {
        self.walkers.len().saturating_sub(MAX_VISIBLE)
    }
}

pub fn arrive_at(w: &Walker) -> Option<i64> {
    w.start_at
        .map(|s| s + ((w.to - w.from).abs() / SPEED).round() as i64)
}

pub fn x_at(w: &Walker, now: i64) -> f32 {
    match w.start_at {
        None => w.from,
        Some(s) if now <= s => w.from,
        Some(s) => {
            let d = (now - s) as f32 * SPEED;
            let span = w.to - w.from;
            if span.abs() <= d {
                w.to
            } else {
                w.from + span.signum() * d
            }
        }
    }
}

pub fn phase_of(w: &Walker, now: i64) -> Phase {
    match w.start_at {
        None => Phase::Queued,
        Some(s) if now < s => Phase::Waiting,
        Some(_) if now < arrive_at(w).unwrap() => Phase::Walking,
        Some(_) => Phase::Arrived,
    }
}

fn wanted(src: &[Source], mail: &Mail, texts: &HashMap<String, String>, now: i64) -> Vec<Walker> {
    let mut out = Vec::new();
    let mut labels = HashMap::new();
    for s in src {
        labels.insert(s.session_id.clone(), s.label.clone());
        let kind = match s.attention {
            ATT_BLOCKED => Kind::Blocked,
            ATT_RETURNED => Kind::Returned,
            _ => continue,
        };
        if s.archived {
            continue;
        }
        out.push(Walker {
            key: format!("att:{}", s.session_id),
            session_id: s.session_id.clone(),
            label: s.label.clone(),
            kind,
            activity_at: s.last_at_ms,
            text: s.needs.clone(),
            sig: format!("{}|{}", s.attention, s.last_at_ms),
            expires_at: None,
            from: 0.0,
            to: 0.0,
            start_at: None,
        });
    }
    let mut ids: Vec<&String> = mail.keys().collect();
    ids.sort();
    for id in ids {
        let (at, sent) = mail[id];
        if sent || now - at >= MAIL_FRESH_MS {
            continue;
        }
        out.push(Walker {
            key: format!("mail:{id}"),
            session_id: id.clone(),
            label: labels
                .get(id)
                .cloned()
                .unwrap_or_else(|| id.chars().take(8).collect()),
            kind: Kind::Mail,
            activity_at: at,
            text: texts.get(id).cloned().unwrap_or_default(),
            sig: at.to_string(),
            expires_at: Some(at + MAIL_FRESH_MS),
            from: 0.0,
            to: 0.0,
            start_at: None,
        });
    }
    out
}

/// Where each visible walker stops: in front of its session's column
/// (`x_of`), a second walker of the same session one step to the side.
pub fn stops(vis: &[&Walker], x_of: &dyn Fn(&str) -> Option<f32>) -> HashMap<String, f32> {
    let mut out = HashMap::new();
    let mut seen: HashMap<&str, usize> = HashMap::new();
    for w in vis {
        let n = seen.entry(w.session_id.as_str()).or_insert(0);
        let x = x_of(&w.session_id).unwrap_or(0.0) - 0.9 + *n as f32 * SIDE_STEP;
        *n += 1;
        out.insert(w.key.clone(), x);
    }
    out
}

/// Brings the queue in line with the sessions at `now`: new items enter
/// (unless dismissed with the same signature), resolved ones leave, the
/// summoned walker leaves after its glow, the visible ones get their stops.
/// `start_x`: where walkers come from (just off the left of the view).
pub fn step(
    q: &Queue,
    src: &[Source],
    mail: &Mail,
    now: i64,
    start_x: f32,
    x_of: &dyn Fn(&str) -> Option<f32>,
) -> Queue {
    let q = expire_highlight(q, now);
    let want = wanted(src, mail, &q.texts, now);
    let by_key: HashMap<&str, &Walker> = q.walkers.iter().map(|w| (w.key.as_str(), w)).collect();
    let mut walkers = Vec::new();
    for w in &want {
        if let Some(cur) = by_key.get(w.key.as_str()) {
            walkers.push(Walker {
                from: cur.from,
                to: cur.to,
                start_at: cur.start_at,
                ..w.clone()
            });
        } else if q.dismissed.get(&w.key) != Some(&w.sig) {
            walkers.push(Walker {
                from: start_x,
                to: start_x,
                start_at: None,
                ..w.clone()
            });
        }
    }
    let mut dismissed = HashMap::new();
    for (k, sig) in &q.dismissed {
        if want.iter().any(|w| &w.key == k && &w.sig == sig) {
            dismissed.insert(k.clone(), sig.clone());
        }
    }
    let out = Queue {
        walkers,
        dismissed,
        ..q
    };
    place(&out, now, start_x, x_of)
}

/// Starts, retargets or parks walkers: the MAX_VISIBLE most urgent walk to
/// their stops, entering single file STAGGER apart.
pub fn place(q: &Queue, now: i64, start_x: f32, x_of: &dyn Fn(&str) -> Option<f32>) -> Queue {
    let vis = q.visible();
    let targets = stops(&vis, x_of);
    let on: Vec<String> = vis.iter().map(|w| w.key.clone()).collect();
    let mut order: Vec<String> = on.clone();
    order.dedup();
    let mut last = q.last_start;
    let mut next: HashMap<String, Walker> = HashMap::new();
    for w in &vis {
        let to = targets.get(&w.key).copied().unwrap_or(w.to);
        let n = match w.start_at {
            None => {
                let s = match last {
                    Some(l) => now.max(l + STAGGER),
                    None => now,
                };
                last = Some(s);
                Walker {
                    from: start_x,
                    to,
                    start_at: Some(s),
                    ..(*w).clone()
                }
            }
            Some(s) if (to - w.to).abs() > 1e-4 => {
                if s > now {
                    Walker { to, ..(*w).clone() }
                } else {
                    Walker {
                        from: x_at(w, now),
                        to,
                        start_at: Some(now),
                        ..(*w).clone()
                    }
                }
            }
            _ => (*w).clone(),
        };
        next.insert(w.key.clone(), n);
    }
    let walkers = q
        .walkers
        .iter()
        .map(|w| {
            if let Some(n) = next.remove(&w.key) {
                n
            } else if w.start_at.is_none() {
                w.clone()
            } else {
                Walker {
                    from: start_x,
                    to: start_x,
                    start_at: None,
                    ..w.clone()
                }
            }
        })
        .collect();
    Queue {
        walkers,
        last_start: last,
        ..q.clone()
    }
}

fn without(q: &Queue, gone: impl Fn(&Walker) -> bool) -> Queue {
    let mut out = q.clone();
    out.walkers.clear();
    for w in &q.walkers {
        if gone(w) {
            out.dismissed.insert(w.key.clone(), w.sig.clone());
        } else {
            out.walkers.push(w.clone());
        }
    }
    out
}

#[cfg(test)]
pub fn dismiss(q: &Queue, key: &str) -> Queue {
    without(q, |w| w.key == key)
}

/// Every walker of a session goes (it was opened: all of it is seen).
pub fn dismiss_session(q: &Queue, id: &str) -> Queue {
    let mut out = without(q, |w| w.session_id == id);
    if out.highlight.as_ref().is_some_and(|h| h.0 == id) {
        out.highlight = None;
    }
    out
}

/// inbox.summoned picked this session: its walker glows, then leaves.
pub fn summon(q: &Queue, id: &str, now: i64) -> Queue {
    Queue {
        highlight: Some((id.to_string(), now + HIGHLIGHT_MS)),
        ..q.clone()
    }
}

fn expire_highlight(q: &Queue, now: i64) -> Queue {
    match &q.highlight {
        Some((id, until)) if now >= *until => {
            let mut out = dismiss_session(q, id);
            out.highlight = None;
            out
        }
        _ => q.clone(),
    }
}

/// A peer.message: remember its text for the recipient's ✉ walker.
pub fn note_peer(q: &Queue, to: &str, from: &str, text: &str) -> Queue {
    let t = if from.is_empty() {
        text.to_string()
    } else {
        format!("{from}: {text}")
    };
    let mut out = q.clone();
    for w in &mut out.walkers {
        if w.key == format!("mail:{to}") {
            w.text = t.clone();
        }
    }
    out.texts.insert(to.to_string(), t);
    out
}

/// The next moment the queue changes by itself, if any.
pub fn next_wake(q: &Queue, now: i64) -> Option<i64> {
    let mut best: Option<i64> = None;
    let mut consider = |t: Option<i64>| {
        if let Some(t) = t
            && t > now
            && best.is_none_or(|b| t < b)
        {
            best = Some(t);
        }
    };
    for w in &q.walkers {
        consider(w.start_at);
        consider(arrive_at(w));
        consider(w.expires_at);
    }
    consider(q.highlight.as_ref().map(|h| h.1));
    best
}

/// True while some walker is on its way (the scene must keep rendering).
pub fn moving(q: &Queue, now: i64) -> bool {
    q.walkers
        .iter()
        .any(|w| matches!(phase_of(w, now), Phase::Waiting | Phase::Walking))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn src(id: &str, att: i64, last: i64) -> Source {
        Source {
            session_id: id.into(),
            label: id.into(),
            attention: att,
            archived: false,
            last_at_ms: last,
            needs: String::new(),
        }
    }

    fn xs(id: &str) -> Option<f32> {
        Some(match id {
            "a" => 0.0,
            "b" => 10.0,
            "c" => 20.0,
            _ => 30.0,
        })
    }

    #[test]
    fn enter_walk_arrive() {
        let s = vec![src("a", ATT_BLOCKED, 5), src("b", 0, 9)];
        let q = step(&Queue::default(), &s, &Mail::new(), 1000, -10.0, &xs);
        assert_eq!(q.walkers.len(), 1);
        let w = &q.walkers[0];
        assert_eq!(w.kind, Kind::Blocked);
        assert_eq!(w.start_at, Some(1000));
        assert_eq!(w.from, -10.0);
        assert!((w.to - -0.9).abs() < 1e-5);
        assert_eq!(phase_of(w, 1000), Phase::Walking);
        let arr = arrive_at(w).unwrap();
        assert!(arr > 1000);
        assert_eq!(phase_of(w, arr), Phase::Arrived);
        assert!((x_at(w, arr + 5) - w.to).abs() < 1e-5);
        assert!(moving(&q, 1500));
        assert!(!moving(&q, arr));
        assert_eq!(next_wake(&q, 1000), Some(arr));
    }

    #[test]
    fn order_single_file_and_overflow() {
        let s = vec![
            src("r1", ATT_RETURNED, 100),
            src("b1", ATT_BLOCKED, 1),
            src("b2", ATT_BLOCKED, 50),
            src("r2", ATT_RETURNED, 200),
            src("b3", ATT_BLOCKED, 3),
            src("r3", ATT_RETURNED, 300),
        ];
        let mut mail = Mail::new();
        mail.insert("m".into(), (0, false));
        mail.insert("sent".into(), (0, true));
        let q = step(&Queue::default(), &s, &mail, 1000, -5.0, &xs);
        let keys: Vec<_> = q.ordered().iter().map(|w| w.key.clone()).collect();
        assert_eq!(
            keys,
            [
                "att:b2", "att:b3", "att:b1", "att:r3", "att:r2", "att:r1", "mail:m"
            ]
        );
        assert_eq!(q.overflow(), 2);
        let starts: Vec<_> = q.visible().iter().map(|w| w.start_at.unwrap()).collect();
        assert_eq!(starts, [1000, 1450, 1900, 2350, 2800]);
        assert!(q.walkers.iter().filter(|w| w.start_at.is_none()).count() == 2);
    }

    #[test]
    fn mail_goes_stale_and_dismissal_sticks_until_change() {
        let mut mail = Mail::new();
        mail.insert("a".into(), (0, false));
        let q = step(&Queue::default(), &[], &mail, 1000, 0.0, &xs);
        assert_eq!(q.walkers.len(), 1);
        assert_eq!(q.walkers[0].kind, Kind::Mail);
        let q2 = step(&q, &[], &mail, MAIL_FRESH_MS + 1, 0.0, &xs);
        assert!(q2.walkers.is_empty());

        let s = vec![src("a", ATT_BLOCKED, 5)];
        let q = step(&Queue::default(), &s, &Mail::new(), 0, 0.0, &xs);
        let q = dismiss(&q, "att:a");
        assert!(q.walkers.is_empty());
        let q = step(&q, &s, &Mail::new(), 10, 0.0, &xs);
        assert!(q.walkers.is_empty(), "same signature stays dismissed");
        let s2 = vec![src("a", ATT_BLOCKED, 6)];
        let q = step(&q, &s2, &Mail::new(), 20, 0.0, &xs);
        assert_eq!(q.walkers.len(), 1, "new activity brings it back");
    }

    #[test]
    fn attention_clearing_dismisses() {
        let s = vec![src("a", ATT_BLOCKED, 5)];
        let q = step(&Queue::default(), &s, &Mail::new(), 0, 0.0, &xs);
        let q = step(&q, &[src("a", 0, 5)], &Mail::new(), 10, 0.0, &xs);
        assert!(q.walkers.is_empty());
    }

    #[test]
    fn retarget_when_columns_move() {
        let s = vec![src("a", ATT_BLOCKED, 5)];
        let q = step(&Queue::default(), &s, &Mail::new(), 0, -10.0, &xs);
        let moved = |_: &str| Some(5.0);
        let q2 = step(&q, &s, &Mail::new(), 1000, -10.0, &moved);
        let w = &q2.walkers[0];
        assert_eq!(w.start_at, Some(1000));
        assert!((w.from - x_at(&q.walkers[0], 1000)).abs() < 1e-4);
        assert!((w.to - 4.1).abs() < 1e-4);
    }

    #[test]
    fn summon_glows_then_leaves() {
        let s = vec![src("a", ATT_BLOCKED, 5)];
        let q = step(&Queue::default(), &s, &Mail::new(), 0, 0.0, &xs);
        let q = summon(&q, "a", 100);
        assert!(next_wake(&q, 100).is_some());
        let q = step(&q, &s, &Mail::new(), 100 + HIGHLIGHT_MS, 0.0, &xs);
        assert!(q.walkers.is_empty());
        assert!(q.highlight.is_none());
    }

    #[test]
    fn peers_text_and_two_walkers_side_by_side() {
        let s = vec![src("a", ATT_BLOCKED, 5)];
        let mut mail = Mail::new();
        mail.insert("a".into(), (0, false));
        let q = step(&Queue::default(), &s, &mail, 10, 0.0, &xs);
        let q = note_peer(&q, "a", "api", "pronto");
        let m = q.walkers.iter().find(|w| w.kind == Kind::Mail).unwrap();
        assert_eq!(m.text, "api: pronto");
        let tos: Vec<f32> = q.ordered().iter().map(|w| w.to).collect();
        assert!((tos[1] - tos[0] - SIDE_STEP).abs() < 1e-5);
        let q = dismiss_session(&q, "a");
        assert!(q.walkers.is_empty());
    }
}
