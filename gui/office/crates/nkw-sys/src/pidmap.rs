//! pid → known ancestor: an audio stream's pid is usually a child (a browser's audio process, an
//! Electron helper) of the process that owns the window, so we walk `ppid` up a few hops until we hit
//! a pid the caller knows (a Wayland client, a cat's session).

/// The two `/proc` facts the walk needs, behind a trait so tests use a fake tree.
pub trait ProcPort {
    fn ppid(&self, pid: u32) -> Option<u32>;
    fn comm(&self, pid: u32) -> Option<String>;
}

/// The real `/proc`.
pub struct ProcFs;

impl ProcPort for ProcFs {
    fn ppid(&self, pid: u32) -> Option<u32> {
        parse_stat(&read_stat(pid)?).map(|s| s.1)
    }

    fn comm(&self, pid: u32) -> Option<String> {
        parse_stat(&read_stat(pid)?).map(|s| s.0)
    }
}

fn read_stat(pid: u32) -> Option<String> {
    std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()
}

/// `(comm, ppid)` from a `/proc/<pid>/stat` line. `comm` may hold spaces and parens, so it runs to
/// the LAST `)`; the state and ppid (field 4) follow.
pub fn parse_stat(line: &str) -> Option<(String, u32)> {
    let open = line.find('(')?;
    let close = line.rfind(')')?;
    if close < open {
        return None;
    }
    let comm = line[open + 1..close].to_string();
    let mut rest = line[close + 1..].split_ascii_whitespace();
    let _state = rest.next()?;
    let ppid = rest.next()?.parse().ok()?;
    Some((comm, ppid))
}

/// The first pid in `pid`'s ancestry (itself included) for which `is_known` holds, walking at most
/// `max_hops` parent links (6 is plenty for browsers and sandboxes). Stops at pid 1 and at cycles.
pub fn anchor(
    pid: u32,
    is_known: &dyn Fn(u32) -> bool,
    proc_: &dyn ProcPort,
    max_hops: usize,
) -> Option<u32> {
    let mut cur = pid;
    let mut seen: Vec<u32> = Vec::with_capacity(max_hops + 1);
    for hop in 0..=max_hops {
        if is_known(cur) {
            return Some(cur);
        }
        if cur <= 1 || hop == max_hops || seen.contains(&cur) {
            return None;
        }
        seen.push(cur);
        cur = proc_.ppid(cur)?;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    struct Fake(HashMap<u32, u32>);
    impl ProcPort for Fake {
        fn ppid(&self, pid: u32) -> Option<u32> {
            self.0.get(&pid).copied()
        }
        fn comm(&self, _: u32) -> Option<String> {
            Some("demo".into())
        }
    }

    fn tree(edges: &[(u32, u32)]) -> Fake {
        Fake(edges.iter().copied().collect())
    }

    #[test]
    fn anchor_table() {
        // 500 = window owner; 510 its child; 520 a grandchild (browser audio helper).
        let t = tree(&[
            (520, 510),
            (510, 500),
            (500, 1),
            (1, 0),
            (700, 701),
            (701, 700),
        ]);
        let known = |p: u32| p == 500;
        let cases: &[(u32, usize, Option<u32>)] = &[
            (500, 6, Some(500)), // direct hit
            (520, 6, Some(500)), // 2-hop child
            (520, 2, Some(500)), // exactly max_hops
            (520, 1, None),      // more hops than allowed
            (900, 6, None),      // unknown pid, no parent
            (700, 6, None),      // cycle-safe
        ];
        for &(pid, hops, want) in cases {
            assert_eq!(anchor(pid, &known, &t, hops), want, "pid {pid} hops {hops}");
        }
    }

    #[test]
    fn anchor_stops_at_init() {
        let t = tree(&[(30, 1), (1, 0)]);
        let calls = std::cell::Cell::new(0);
        let known = |p: u32| {
            calls.set(calls.get() + 1);
            p == 0
        };
        assert_eq!(anchor(30, &known, &t, 6), None);
        assert_eq!(calls.get(), 2, "checks 30 and 1, never walks past init");
        assert_eq!(anchor(1, &|p| p == 1, &t, 6), Some(1));
    }

    #[test]
    fn stat_parser_table() {
        let cases: &[(&str, Option<(&str, u32)>)] = &[
            ("1234 (demo) S 1 1234 1234 0 -1", Some(("demo", 1))),
            ("42 (Web Content) R 7 42 7 0", Some(("Web Content", 7))),
            ("43 (a) (b)) S 9 1 1", Some(("a) (b)", 9))),
            ("44 (x y z) Z 10", Some(("x y z", 10))),
            ("45 () S 3", Some(("", 3))),
            ("garbage", None),
            ("46 (cut", None),
            ("47 (demo) S notanumber", None),
        ];
        for &(line, want) in cases {
            let got = parse_stat(line);
            assert_eq!(
                got.as_ref().map(|(c, p)| (c.as_str(), *p)),
                want,
                "{line:?}"
            );
        }
    }

    #[test]
    fn procfs_reads_self() {
        let me = std::process::id();
        let parent = ProcFs.ppid(me).expect("own ppid");
        assert!(parent >= 1);
        assert!(ProcFs.comm(me).is_some());
        // our own pid is an ancestor-or-self of itself
        assert_eq!(anchor(me, &|p| p == parent, &ProcFs, 6), Some(parent));
    }
}
