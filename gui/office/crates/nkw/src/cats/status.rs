//! What the cats tell the status strip and the phone's header (S20/S24):
//! the inbox, the agents at work and blocked, today's cost and the 5-hour
//! usage. Pure: [`status_of`] over the office and what the last snapshot
//! carried ([`Ledger`]).

use nk_cats::model::Pose;

use super::office::Office;
use crate::ext::Status;

/// The money and the limits of the last snapshot (or the fixture's).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Ledger {
    /// the snapshot's `today_cost` ($); 0 when the core has no "today"
    pub today_cost: f64,
    /// the "Current session" line of `/usage` (the 5-hour window), %
    pub usage_5h: Option<u8>,
    /// a snapshot (or a fixture) was seen
    pub seen: bool,
}

impl Ledger {
    pub fn of(snap: &nk_client::Snapshot) -> Ledger {
        Ledger {
            today_cost: snap.today_cost,
            usage_5h: usage_5h(&snap.usage),
            seen: true,
        }
    }
}

/// The 5-hour window of a `/usage` report: the line named "…session…".
pub fn usage_5h(u: &nk_client::Usage) -> Option<u8> {
    u.limits
        .iter()
        .find(|l| l.name.to_lowercase().contains("session"))
        .map(|l| l.pct.clamp(0, 100) as u8)
}

/// Dollars to whole cents (rounded; never negative).
pub fn cents(usd: f64) -> u32 {
    (usd.max(0.0) * 100.0).round() as u32
}

/// The cats' part of the strip: `inbox` is the cats waiting for you (the
/// whole queue, overflow included), `agents` is (typing, blocked),
/// `cost_cents` is today's cost (the snapshot's `today_cost`, else the
/// sessions' `cost_today` summed), then `usage_5h_pct`. Nothing before a
/// snapshot.
pub fn status_of(office: &Office, queue: usize, ledger: &Ledger) -> Status {
    if !ledger.seen && office.cats.is_empty() {
        return Status::default();
    }
    let count = |p: Pose| office.cats.iter().filter(|v| v.pose == p).count() as u32;
    let today = match ledger.today_cost > 0.0 {
        true => ledger.today_cost,
        false => office.cats.iter().map(|v| v.sess.cost_today).sum(),
    };
    Status {
        inbox: Some(queue as u32),
        agents: Some((count(Pose::Typing), count(Pose::Alert))),
        cost_cents: Some(cents(today)),
        usage_5h_pct: ledger.usage_5h,
        ..Status::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cats::demo::{self, Fixture};

    fn office(f: Fixture) -> Office {
        let (snap, tree) = demo::snapshot(f);
        let mut o = Office::new();
        o.apply(&snap, &tree.rows, 0.0);
        o
    }

    #[test]
    fn the_status_table() {
        let demo_ledger = Ledger::of(&demo::snapshot(Fixture::Demo).0);
        let rows: Vec<(&str, Office, usize, Ledger, Status)> = vec![
            (
                "nothing yet",
                Office::new(),
                0,
                Ledger::default(),
                Status::default(),
            ),
            (
                "demo",
                office(Fixture::Demo),
                1,
                demo_ledger.clone(),
                Status {
                    inbox: Some(1),
                    agents: Some((1, 1)),
                    cost_cents: Some(cents(demo::TODAY_USD)),
                    usage_5h_pct: Some(demo::USAGE_5H),
                    ..Status::default()
                },
            ),
            (
                "busy",
                office(Fixture::Busy),
                4,
                Ledger::of(&demo::snapshot(Fixture::Busy).0),
                Status {
                    inbox: Some(4),
                    agents: Some((4, 1)),
                    cost_cents: Some(cents(demo::TODAY_USD)),
                    usage_5h_pct: Some(demo::USAGE_5H),
                    ..Status::default()
                },
            ),
            (
                "no today in the snapshot: the sessions' sum",
                office(Fixture::Demo),
                0,
                Ledger {
                    today_cost: 0.0,
                    usage_5h: None,
                    seen: true,
                },
                Status {
                    inbox: Some(0),
                    agents: Some((1, 1)),
                    cost_cents: Some(cents(
                        demo::sessions(Fixture::Demo)
                            .iter()
                            .map(|s| s.cost_today)
                            .sum(),
                    )),
                    ..Status::default()
                },
            ),
        ];
        for (name, o, q, l, want) in rows {
            assert_eq!(status_of(&o, q, &l), want, "{name}");
        }
    }

    #[test]
    fn usage_and_cents() {
        let u = nk_client::Usage {
            limits: vec![
                nk_client::Limit {
                    name: "Current week (all models)".into(),
                    pct: 34,
                    resets: String::new(),
                },
                nk_client::Limit {
                    name: "Current session".into(),
                    pct: 127,
                    resets: String::new(),
                },
            ],
            ..nk_client::Usage::default()
        };
        assert_eq!(usage_5h(&u), Some(100), "clamped");
        assert_eq!(usage_5h(&nk_client::Usage::default()), None);
        assert_eq!(
            (cents(4.5), cents(0.005), cents(-1.0), cents(1.239)),
            (450, 1, 0, 124)
        );
    }
}
