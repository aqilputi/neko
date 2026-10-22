//! The core's sessions as villagers (§6.1): `nk_cats::model::Office::build`
//! puts them in tree order, each gets a stable `CatId` for the run, a coat
//! seed (FNV-1a of the session id), a collar (its first group's colour), a
//! pose (`model::pose_for`) and its kittens; the contexts in tree order are
//! what `World::sync_contexts` lays the rooms out from.

use std::collections::{BTreeMap, HashMap};

use nk_cats::model::{self, Palette, Pose, Row, Sess};
use nk_client::{Session, Snapshot, Sub, TreeRow};
use nkw_core::{CatId, Time};

/// The reception's cat (toasts, confirmations): never a session.
pub const RECEPTIONIST: CatId = CatId(1);
/// The first id a session gets (the receptionist keeps 1).
pub const FIRST_ID: u32 = 2;
/// Kittens drawn per desk; the rest is a "+N" tag.
pub const MAX_KITTENS: usize = 4;

/// FNV-1a, 64 bits: the coat seed of a session id.
pub fn fnv1a(s: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// One session's cat.
#[derive(Clone, Debug, PartialEq)]
pub struct Villager {
    pub cat: CatId,
    /// the core's session, as last seen
    pub sess: Session,
    pub ctx: String,
    pub pose: Pose,
    /// when `pose` began (the puff runs once from here)
    pub pose_since: Time,
    pub seed: u64,
    /// the first group's colour
    pub collar: Option<[u8; 3]>,
    /// the context's colour (name plate, badge)
    pub ctx_rgb: [u8; 3],
}

impl Villager {
    pub fn label(&self) -> &str {
        &self.sess.label
    }

    /// The kittens shown on the desk (running first) and how many more.
    pub fn kittens(&self) -> (Vec<&Sub>, usize) {
        let mut k: Vec<&Sub> = self.sess.subs.iter().collect();
        k.sort_by_key(|s| s.done);
        let more = k.len().saturating_sub(MAX_KITTENS);
        k.truncate(MAX_KITTENS);
        (k, more)
    }
}

/// What an `apply` changed.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Change {
    pub added: Vec<CatId>,
    pub removed: Vec<CatId>,
    /// cats whose pose changed
    pub posed: Vec<CatId>,
    /// the contexts (rooms) or their order changed
    pub layout: bool,
}

/// Every villager, in tree order.
#[derive(Clone, Debug)]
pub struct Office {
    ids: BTreeMap<String, CatId>,
    next: u32,
    pub cats: Vec<Villager>,
    pub palette: Palette,
}

impl Default for Office {
    fn default() -> Office {
        Office::new()
    }
}

/// The core session → what `nk_cats::model` reads.
pub fn sess_of(s: &Session) -> Sess {
    Sess {
        id: s.session_id.clone(),
        short: s.short_id.clone(),
        label: if s.label.is_empty() {
            s.session_id.chars().take(8).collect()
        } else {
            s.label.clone()
        },
        ctx: s.context.clone(),
        state: s.state.clone(),
        attention: s.attention,
        last_at_ms: model::parse_rfc3339(&s.last_at),
        needs: s.needs.clone(),
        running_subs: s.subs.iter().filter(|x| !x.done).count(),
        groups: vec![],
        dim: s.stale || s.ended || s.kind == "history",
        archived: s.archived,
        ended: s.ended,
        finished: s.finished,
        cost: s.cost,
    }
}

pub fn row_of(r: &TreeRow) -> Row {
    Row {
        kind: r.kind.clone(),
        ctx: r.ctx.clone(),
        color: r.color,
        session_id: r.session_id.clone(),
        groups: r.groups.iter().map(|g| (g.tag.clone(), g.color)).collect(),
    }
}

impl Office {
    pub fn new() -> Office {
        Office {
            ids: BTreeMap::new(),
            next: FIRST_ID,
            cats: Vec::new(),
            palette: Palette::default(),
        }
    }

    /// The id of a session (a new one the first time it is seen).
    pub fn id_for(&mut self, session: &str) -> CatId {
        if let Some(c) = self.ids.get(session) {
            return *c;
        }
        let c = CatId(self.next);
        self.next += 1;
        self.ids.insert(session.to_string(), c);
        c
    }

    /// The session of a cat (also of one that left).
    pub fn session_of(&self, cat: CatId) -> Option<&str> {
        self.ids
            .iter()
            .find(|(_, c)| **c == cat)
            .map(|(s, _)| s.as_str())
    }

    pub fn get(&self, cat: CatId) -> Option<&Villager> {
        self.cats.iter().find(|v| v.cat == cat)
    }

    pub fn by_session(&self, id: &str) -> Option<&Villager> {
        self.cats.iter().find(|v| v.sess.session_id == id)
    }

    /// Bring the villagers in line with a snapshot and the tree's rows.
    pub fn apply(&mut self, snap: &Snapshot, rows: &[TreeRow], now: Time) -> Change {
        let sess: Vec<Sess> = snap.sessions.iter().map(sess_of).collect();
        let rows: Vec<Row> = rows.iter().map(row_of).collect();
        let built = model::Office::build(&sess, &rows);
        let before: HashMap<CatId, (Pose, String)> = self
            .cats
            .iter()
            .map(|v| (v.cat, (v.pose, v.ctx.clone())))
            .collect();
        let old_layout = self.contexts();
        let mut cats = Vec::new();
        let mut change = Change::default();
        for col in &built.columns {
            let Some(full) = snap.session(&col.sess.id) else {
                continue;
            };
            let cat = self.id_for(&col.sess.id);
            let pose = col.sess.pose();
            let pose_since = match before.get(&cat) {
                None => {
                    change.added.push(cat);
                    now
                }
                Some((p, _)) if *p != pose => {
                    change.posed.push(cat);
                    now
                }
                Some(_) => self.get(cat).map_or(now, |v| v.pose_since),
            };
            let mut sess = full.clone();
            sess.label = col.sess.label.clone();
            cats.push(Villager {
                cat,
                ctx: col.sess.ctx.clone(),
                pose,
                pose_since,
                seed: fnv1a(&col.sess.id),
                collar: col.collar.map(|i| self.palette.context(i)),
                ctx_rgb: self.palette.context(col.ctx_color),
                sess,
            });
        }
        change.removed = before
            .keys()
            .filter(|c| !cats.iter().any(|v: &Villager| v.cat == **c))
            .copied()
            .collect();
        change.removed.sort_by_key(|c| c.0);
        self.cats = cats;
        change.layout = self.contexts() != old_layout;
        change
    }

    /// The rooms: each context with its cats, both in tree order.
    pub fn contexts(&self) -> Vec<(String, Vec<CatId>)> {
        let mut out: Vec<(String, Vec<CatId>)> = Vec::new();
        for v in &self.cats {
            match out.iter_mut().find(|(c, _)| *c == v.ctx) {
                Some((_, cats)) => cats.push(v.cat),
                None => out.push((v.ctx.clone(), vec![v.cat])),
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cats::demo::{self, Fixture};

    fn office(f: Fixture) -> (Office, Change) {
        let (snap, tree) = demo::snapshot(f);
        let mut o = Office::new();
        let c = o.apply(&snap, &tree.rows, 0.0);
        (o, c)
    }

    #[test]
    fn fnv1a_is_the_standard_one() {
        assert_eq!(fnv1a(""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a("a"), 0xaf63_dc4c_8601_ec8c);
        assert_ne!(fnv1a("demo-acme"), fnv1a("demo-alpha"));
    }

    #[test]
    fn the_fixture_tree_makes_two_rooms_in_order() {
        let (o, c) = office(Fixture::Demo);
        let ctx = o.contexts();
        assert_eq!(
            ctx,
            [
                ("acme".to_string(), vec![CatId(2), CatId(3)]),
                ("demo".to_string(), vec![CatId(4), CatId(5), CatId(6)]),
            ]
        );
        assert_eq!(c.added.len(), 5);
        assert!(c.layout);
        let labels: Vec<_> = o.cats.iter().map(|v| v.label()).collect();
        assert_eq!(labels, ["acme", "api-gateway", "alpha", "bravo", "demo"]);
        let poses: Vec<_> = o.cats.iter().map(|v| v.pose).collect();
        assert_eq!(
            poses,
            [
                Pose::Typing,
                Pose::Alert,
                Pose::Sleeping,
                Pose::Idle,
                Pose::Puff
            ]
        );
        assert_eq!(o.cats[0].ctx_rgb, o.palette.context(0));
        assert_eq!(o.cats[2].ctx_rgb, o.palette.context(1));
    }

    #[test]
    fn ids_are_stable_and_never_reused() {
        let (mut o, _) = office(Fixture::Demo);
        let acme = o.by_session(&demo::id_of("acme")).unwrap().cat;
        // busy: three more; the old ones keep their ids
        let (snap, tree) = demo::snapshot(Fixture::Busy);
        let c = o.apply(&snap, &tree.rows, 1.0);
        assert_eq!(o.by_session(&demo::id_of("acme")).unwrap().cat, acme);
        assert_eq!(c.added, [CatId(7), CatId(8), CatId(9)]);
        assert!(c.removed.is_empty() && c.posed.is_empty() && c.layout);
        // back to demo: they leave; a new session never takes their ids
        let (snap, tree) = demo::snapshot(Fixture::Demo);
        let c = o.apply(&snap, &tree.rows, 2.0);
        assert_eq!(c.removed, [CatId(7), CatId(8), CatId(9)]);
        assert_eq!(o.session_of(CatId(7)), Some(demo::id_of("acme-2").as_str()));
        assert_eq!(o.id_for("new"), CatId(10));
        // the same snapshot again changes nothing
        let c = o.apply(&snap, &tree.rows, 3.0);
        assert_eq!(c, Change::default());
    }

    #[test]
    fn a_pose_change_restarts_its_clock() {
        let (mut o, _) = office(Fixture::Demo);
        let (mut snap, tree) = demo::snapshot(Fixture::Demo);
        snap.sessions[3].state = "busy".into();
        snap.sessions[3].attention = 0;
        let c = o.apply(&snap, &tree.rows, 5.0);
        let bravo = o.by_session(&demo::id_of("bravo")).unwrap();
        assert_eq!(c.posed, [bravo.cat]);
        assert_eq!((bravo.pose, bravo.pose_since), (Pose::Typing, 5.0));
        assert_eq!(o.cats[0].pose_since, 0.0, "the others keep theirs");
    }

    #[test]
    fn collars_and_kittens() {
        let (snap, mut tree) = demo::snapshot(Fixture::Demo);
        tree.rows[1].groups = vec![nk_client::RowGroup {
            tag: "t".into(),
            color: 4,
        }];
        let mut o = Office::new();
        o.apply(&snap, &tree.rows, 0.0);
        assert_eq!(o.cats[0].collar, Some(o.palette.context(4)));
        assert_eq!(o.cats[1].collar, None);
        let (k, more) = o.cats[0].kittens();
        assert_eq!((k.len(), more), (2, 0));
        let mut v = o.cats[0].clone();
        v.sess.subs = (0..6)
            .map(|i| Sub {
                id: format!("k{i}"),
                done: i % 2 == 0,
                ..Sub::default()
            })
            .collect();
        let (k, more) = v.kittens();
        assert_eq!((k.len(), more), (4, 2));
        assert!(!k[0].done && !k[2].done, "running ones first");
    }

    #[test]
    fn sessions_without_a_row_or_label() {
        let (mut snap, tree) = demo::snapshot(Fixture::Demo);
        snap.sessions[0].label.clear();
        snap.sessions.remove(4); // a row whose session is gone
        let mut o = Office::new();
        o.apply(&snap, &tree.rows, 0.0);
        assert_eq!(o.cats.len(), 4);
        assert_eq!(o.cats[0].label(), "demo-acm", "the id's first 8 chars");
    }
}
