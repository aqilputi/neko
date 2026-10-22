//! `NKW_STATS=1`: one JSON line per frame on stderr (prefix `nkw-stats `),
//! one per injected key, and a summary at exit (contract S2). Timestamps
//! are `CLOCK_MONOTONIC` µs, the testclient's clock.

use serde::Serialize;

/// `CLOCK_MONOTONIC`, µs.
pub fn mono_us() -> u64 {
    clock_us(libc::CLOCK_MONOTONIC)
}

/// `CLOCK_MONOTONIC`, seconds (the world's `Time`).
pub fn mono_s() -> f64 {
    mono_us() as f64 / 1e6
}

fn clock_us(clock: libc::clockid_t) -> u64 {
    let mut ts = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: valid pointer to a timespec on the stack.
    unsafe { libc::clock_gettime(clock, &mut ts) };
    ts.tv_sec as u64 * 1_000_000 + ts.tv_nsec as u64 / 1000
}

#[derive(Clone, Copy, Debug, Default, Serialize, PartialEq)]
pub struct Frame {
    pub t_us: u64,
    pub frame: u64,
    pub pass: &'static str,
    pub update_us: u64,
    pub cull_us: u64,
    pub submit_us: u64,
    pub compose_us: u64,
    pub draws: u32,
    pub tris: u32,
    pub gpu_us: Option<u64>,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq)]
pub struct Summary {
    pub summary: bool,
    pub frames: u64,
    pub world_passes: u64,
    pub p50_us: u64,
    pub p95_us: u64,
    pub dropped: u64,
    pub first_frame_ms: Option<f64>,
    pub rss_kb: u64,
}

/// The nearest-rank percentile of `v` (sorted in place); 0 when empty.
pub fn percentile(v: &mut [u64], p: f64) -> u64 {
    if v.is_empty() {
        return 0;
    }
    v.sort_unstable();
    let rank = ((p / 100.0) * v.len() as f64).ceil() as usize;
    v[rank.clamp(1, v.len()) - 1]
}

/// Frames missed between two presented frames `gap` seconds apart on a
/// `period` grid (only meaningful while something animates).
pub fn missed(gap: f64, period: f64) -> u64 {
    let n = (gap / period).round();
    if n > 1.0 && gap > 1.5 * period {
        n as u64 - 1
    } else {
        0
    }
}

/// The collector.
#[derive(Debug, Default)]
pub struct Stats {
    pub enabled: bool,
    pub gpu: bool,
    pub frames: u64,
    pub world_passes: u64,
    cpu_us: Vec<u64>,
    pub dropped: u64,
    pub first_frame_ms: Option<f64>,
    last_animated: Option<f64>,
}

impl Stats {
    /// From `NKW_STATS` (`1`, `gpu`).
    pub fn from_env(v: Option<&str>) -> Stats {
        Stats {
            enabled: matches!(v, Some("1" | "gpu" | "true")),
            gpu: v == Some("gpu"),
            ..Stats::default()
        }
    }

    /// A frame was drawn; `animated`: something wanted every frame, so a
    /// gap since the previous animated frame counts as dropped frames.
    pub fn frame(&mut self, f: &Frame, now: f64, animated: bool, period: f64) {
        self.frames += 1;
        if f.pass == "world" {
            self.world_passes += 1;
        }
        self.cpu_us
            .push(f.update_us + f.cull_us + f.submit_us + f.compose_us);
        if self.first_frame_ms.is_none() {
            self.first_frame_ms = since_process_start_ms();
        }
        if animated {
            if let Some(prev) = self.last_animated {
                self.dropped += missed(now - prev, period);
            }
            self.last_animated = Some(now);
        } else {
            self.last_animated = None;
        }
        if self.enabled {
            eprintln!("nkw-stats {}", serde_json::to_string(f).unwrap());
        }
    }

    pub fn inject(&self, key: &str) {
        if self.enabled {
            let v = serde_json::json!({"inject": {"key": key, "t_us": mono_us()}});
            eprintln!("nkw-stats {v}");
        }
    }

    pub fn summary(&mut self) -> Summary {
        let mut v = self.cpu_us.clone();
        Summary {
            summary: true,
            frames: self.frames,
            world_passes: self.world_passes,
            p50_us: percentile(&mut v, 50.0),
            p95_us: percentile(&mut v, 95.0),
            dropped: self.dropped,
            first_frame_ms: self.first_frame_ms,
            rss_kb: rss_kb().unwrap_or(0),
        }
    }

    pub fn print_summary(&mut self) {
        if self.enabled {
            let s = self.summary();
            eprintln!("nkw-stats {}", serde_json::to_string(&s).unwrap());
        }
    }
}

/// `VmRSS` of this process, kB.
pub fn rss_kb() -> Option<u64> {
    let s = std::fs::read_to_string("/proc/self/status").ok()?;
    parse_kb(&s, "VmRSS:")
}

fn parse_kb(status: &str, key: &str) -> Option<u64> {
    status
        .lines()
        .find_map(|l| l.strip_prefix(key))
        .and_then(|v| v.split_whitespace().next())
        .and_then(|v| v.parse().ok())
}

/// The process start time (`/proc/self/stat` field 22) to now, ms.
fn since_process_start_ms() -> Option<f64> {
    let stat = std::fs::read_to_string("/proc/self/stat").ok()?;
    let start_ticks = starttime_ticks(&stat)?;
    // SAFETY: sysconf has no preconditions.
    let hz = unsafe { libc::sysconf(libc::_SC_CLK_TCK) } as f64;
    let boot_now = clock_us(libc::CLOCK_BOOTTIME) as f64 / 1e6;
    Some((boot_now - start_ticks as f64 / hz) * 1000.0)
}

fn starttime_ticks(stat: &str) -> Option<u64> {
    // the comm field may hold spaces and parens: count from the last ')'
    let rest = &stat[stat.rfind(')')? + 1..];
    rest.split_whitespace().nth(19)?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentiles() {
        assert_eq!(percentile(&mut [], 50.0), 0);
        let mut v: Vec<u64> = (1..=100).rev().collect();
        assert_eq!(percentile(&mut v, 50.0), 50);
        assert_eq!(percentile(&mut v, 95.0), 95);
        assert_eq!(percentile(&mut v, 100.0), 100);
        assert_eq!(percentile(&mut [7], 95.0), 7);
    }

    #[test]
    fn missed_frames() {
        let p = 1.0 / 60.0;
        assert_eq!(missed(p, p), 0);
        assert_eq!(missed(p * 1.4, p), 0);
        assert_eq!(missed(p * 2.0, p), 1);
        assert_eq!(missed(p * 3.1, p), 2);
    }

    #[test]
    fn frame_line_has_the_contract_keys() {
        let f = Frame {
            pass: "world",
            ..Frame::default()
        };
        let v: serde_json::Value = serde_json::to_value(f).unwrap();
        for k in [
            "t_us",
            "frame",
            "pass",
            "update_us",
            "cull_us",
            "submit_us",
            "compose_us",
            "draws",
            "tris",
            "gpu_us",
        ] {
            assert!(v.get(k).is_some(), "{k}");
        }
        assert!(v["gpu_us"].is_null());
    }

    #[test]
    fn summary_counts() {
        let mut s = Stats::from_env(None);
        assert!(!s.enabled);
        let p = 1.0 / 60.0;
        for (i, pass) in ["world", "compose", "world"].iter().enumerate() {
            let f = Frame {
                pass,
                update_us: 100 * (i as u64 + 1),
                ..Frame::default()
            };
            s.frame(&f, i as f64 * p * 2.0, true, p);
        }
        let sum = s.summary();
        assert_eq!((sum.frames, sum.world_passes, sum.dropped), (3, 2, 2));
        assert_eq!((sum.p50_us, sum.p95_us), (200, 300));
        assert!(sum.summary);
        assert!(sum.first_frame_ms.is_some());
        let v = serde_json::to_value(sum).unwrap();
        for k in [
            "summary",
            "frames",
            "world_passes",
            "p50_us",
            "p95_us",
            "dropped",
            "first_frame_ms",
            "rss_kb",
        ] {
            assert!(v.get(k).is_some(), "{k}");
        }
    }

    #[test]
    fn env_values() {
        assert!(Stats::from_env(Some("1")).enabled);
        let g = Stats::from_env(Some("gpu"));
        assert!(g.enabled && g.gpu);
        assert!(!Stats::from_env(Some("0")).enabled);
    }

    #[test]
    fn proc_parsers() {
        assert_eq!(
            parse_kb("Name:\tx\nVmRSS:\t  1234 kB\n", "VmRSS:"),
            Some(1234)
        );
        let stat = "42 (a (b) c) S 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 99 20";
        assert_eq!(starttime_ticks(stat), Some(99));
        assert!(rss_kb().unwrap() > 0);
    }
}
