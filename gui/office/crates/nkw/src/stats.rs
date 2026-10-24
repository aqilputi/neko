//! `NKW_STATS=1`: one JSON line per frame on stderr (prefix `nkw-stats `),
//! one per injected key, and a summary at exit (contract S2). Timestamps
//! are `CLOCK_MONOTONIC` µs, the testclient's clock.
//!
//! `NKW_STATS` is a comma list: `1` (every frame), `gpu` (GPU timer
//! queries), `ext` (per-plugin `drain`+`tick` µs, p95 in the summary),
//! `slow` (only the frames over [`SLOW_US`], with their stage split). Any of
//! them prints the summary. `NKW_STATS_WARMUP_MS=N`: the first N ms after
//! start do not count in the percentiles (`world-perf.sh` passes the
//! scenario's `#@warmup`).

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
    /// the shm/dmabuf imports of the world pass (`import_surface_tree`),
    /// split out of `cull_us`
    pub import_us: u64,
    pub cull_us: u64,
    pub submit_us: u64,
    pub compose_us: u64,
    /// the part of `compose_us` that gathers the HUD (`hud_input`, the
    /// overlay's elements, their render elements); the rest is
    /// `render_output` and the submit
    pub hud_us: u64,
    pub draws: u32,
    pub tris: u32,
    pub gpu_us: Option<u64>,
}

/// A frame over this many µs of CPU is "slow" (`NKW_STATS=slow`).
pub const SLOW_US: u64 = 2000;

/// The plugins in `Ext::plugins` order (per-plugin tick stats).
pub const PLUGINS: [&str; 4] = ["wm", "objects", "cats", "overlay"];

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct Summary {
    pub summary: bool,
    pub frames: u64,
    pub world_passes: u64,
    pub p50_us: u64,
    pub p95_us: u64,
    pub dropped: u64,
    pub first_frame_ms: Option<f64>,
    pub rss_kb: u64,
    /// the cost of all plugin `tick`s per frame, p95
    pub ext_tick_us_p95: u64,
    /// the most simultaneous `Want::Rate` wishes in a frame (objects: ≤ 4)
    pub rate_wishes_max: u64,
    /// composes forced by a HUD change with no wish (S14)
    pub compose_hud_changes: u64,
    /// relative pointer motions that reached the walker
    pub look_events: u64,
    /// the world pass's client imports per frame (0 on compose-only frames)
    pub import_us_p50: u64,
    pub import_us_p95: u64,
    /// the 2D composition (`render_output` + submit) per frame
    pub compose_us_p50: u64,
    pub compose_us_p95: u64,
    /// `NKW_STATS=ext`: each plugin's `drain`+`tick` per frame, p95 µs
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext_plugin_us_p95: Option<std::collections::BTreeMap<&'static str, u64>>,
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

/// Frames lost over two consecutive gaps, the first of them late: a late
/// frame that the next one makes up for (late then early, two frames in two
/// vblanks) lost nothing; slots beyond the two frames did.
pub fn missed_pair(g1: f64, g2: f64, period: f64) -> u64 {
    let slots = ((g1 + g2) / period).round();
    if slots > 2.0 { slots as u64 - 2 } else { 0 }
}

/// The collector.
#[derive(Debug, Default)]
pub struct Stats {
    pub enabled: bool,
    pub gpu: bool,
    pub frames: u64,
    pub world_passes: u64,
    cpu_us: Vec<u64>,
    import_us: Vec<u64>,
    compose_us: Vec<u64>,
    pub dropped: u64,
    pub first_frame_ms: Option<f64>,
    last_animated: Option<f64>,
    /// a late gap (`missed > 0`) waiting for the next frame's gap
    /// ([`missed_pair`]); flushed alone when the animation stops
    late_gap: Option<f64>,
    ext_tick_us: Vec<u64>,
    pub rate_wishes_max: u64,
    /// `NKW_MEM=1`: an RSS attribution line (`nkw-mem …`) at the first frame and at exit
    pub mem: bool,
    /// every frame line (`1`, `gpu`)
    pub every: bool,
    /// `slow`: the frames over `SLOW_US`
    pub slow: bool,
    /// `ext`: per-plugin tick µs
    pub ext_detail: bool,
    ext_plugin_us: [Vec<u64>; 4],
    /// samples before this instant (mono s) are the warm-up
    pub warmup_until: f64,
    pub compose_hud_changes: u64,
    pub look_events: u64,
    /// the vblank period of the last frame
    period: f64,
}

impl Stats {
    /// From `NKW_STATS` (a comma list of `1`, `gpu`, `ext`, `slow`).
    pub fn from_env(v: Option<&str>) -> Stats {
        let mut s = Stats::default();
        for t in v.unwrap_or("").split(',').map(str::trim) {
            match t {
                "1" | "true" => s.every = true,
                "gpu" => {
                    s.every = true;
                    s.gpu = true;
                }
                "ext" => s.ext_detail = true,
                "slow" => s.slow = true,
                _ => {}
            }
        }
        s.enabled = s.every || s.slow || s.ext_detail;
        s
    }

    /// `NKW_STATS_WARMUP_MS`: ignore the first `ms` after `now` (mono s).
    pub fn with_warmup(mut self, ms: Option<&str>, now: f64) -> Stats {
        if let Some(ms) = ms.and_then(|v| v.trim().parse::<u64>().ok()) {
            self.warmup_until = now + ms as f64 / 1000.0;
        }
        self
    }

    fn warm(&self, now: f64) -> bool {
        now >= self.warmup_until
    }

    /// `NKW_MEM=1`: print the RSS attribution at the first frame and at exit.
    pub fn with_mem(mut self, on: bool) -> Stats {
        self.mem = on;
        self
    }

    /// A frame was drawn; `animated`: something wanted every frame, so a
    /// gap since the previous animated frame counts as dropped frames.
    pub fn frame(&mut self, f: &Frame, now: f64, animated: bool, period: f64) {
        self.frames += 1;
        self.period = period;
        if f.pass == "world" {
            self.world_passes += 1;
        }
        let cpu = f.update_us + f.import_us + f.cull_us + f.submit_us + f.compose_us;
        if self.warm(now) {
            self.cpu_us.push(cpu);
            self.import_us.push(f.import_us);
            self.compose_us.push(f.compose_us);
        }
        if self.first_frame_ms.is_none() {
            self.first_frame_ms = since_process_start_ms();
            if self.mem {
                print_mem("first_frame");
            }
        }
        let mut dropped_here = false;
        if animated {
            // a gap that starts in the warm-up is the warm-up's (like its
            // frames in the percentiles)
            if let Some(prev) = self.last_animated
                && self.warm(prev)
            {
                let gap = now - prev;
                let n = match self.late_gap.take() {
                    Some(g1) => missed_pair(g1, gap, period),
                    None if missed(gap, period) > 0 => {
                        // judged with the next gap
                        self.late_gap = Some(gap);
                        0
                    }
                    None => 0,
                };
                self.dropped += n;
                dropped_here = n > 0;
                if n > 0 && (self.every || self.slow) {
                    // which frame ended the loss (its stage split follows)
                    let v = serde_json::json!({"drop": {
                        "frame": f.frame, "gap_us": (gap * 1e6) as u64, "missed": n,
                    }});
                    eprintln!("nkw-stats {v}");
                }
            }
            self.last_animated = Some(now);
        } else {
            self.flush_late(period);
            self.last_animated = None;
        }
        if self.every || (self.slow && (cpu > SLOW_US || dropped_here)) {
            eprintln!("nkw-stats {}", serde_json::to_string(f).unwrap());
        }
    }

    /// The plugins' ticks of one frame took `us`; `rate_wishes`: how many
    /// `Want::Rate` wishes the frame had.
    pub fn ext(&mut self, us: u64, rate_wishes: usize, now: f64) {
        if self.warm(now) {
            self.ext_tick_us.push(us);
        }
        self.rate_wishes_max = self.rate_wishes_max.max(rate_wishes as u64);
    }

    /// `NKW_STATS=ext`: plugin `i` (in [`PLUGINS`] order) took `us` this frame.
    pub fn ext_plugin(&mut self, i: usize, us: u64, now: f64) {
        if self.ext_detail && self.warm(now) {
            self.ext_plugin_us[i].push(us);
        }
    }

    pub fn inject(&self, key: &str) {
        if self.every {
            let v = serde_json::json!({"inject": {"key": key, "t_us": mono_us()}});
            eprintln!("nkw-stats {v}");
        }
    }

    /// A pending late gap with no next frame counts alone.
    fn flush_late(&mut self, period: f64) {
        if let Some(g) = self.late_gap.take() {
            self.dropped += missed(g, period);
        }
    }

    pub fn summary(&mut self) -> Summary {
        // the period the late gap was judged with (60 Hz unless one was seen)
        self.flush_late(self.period);
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
            ext_tick_us_p95: percentile(&mut self.ext_tick_us.clone(), 95.0),
            rate_wishes_max: self.rate_wishes_max,
            compose_hud_changes: self.compose_hud_changes,
            look_events: self.look_events,
            import_us_p50: percentile(&mut self.import_us.clone(), 50.0),
            import_us_p95: percentile(&mut self.import_us.clone(), 95.0),
            compose_us_p50: percentile(&mut self.compose_us.clone(), 50.0),
            compose_us_p95: percentile(&mut self.compose_us.clone(), 95.0),
            ext_plugin_us_p95: self.ext_detail.then(|| {
                PLUGINS
                    .iter()
                    .zip(&self.ext_plugin_us)
                    .map(|(n, v)| (*n, percentile(&mut v.clone(), 95.0)))
                    .collect()
            }),
        }
    }

    pub fn print_summary(&mut self) {
        if self.mem {
            print_mem("exit");
        }
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

/// Where a mapping's resident pages come from (the RSS attribution).
pub fn mem_kind(path: &str, exe: &str) -> &'static str {
    let lib = path.rsplit('/').next().unwrap_or(path);
    if path.is_empty() {
        "anon"
    } else if path == "[heap]" {
        "heap"
    } else if path.starts_with("[stack") {
        "stack"
    } else if path.starts_with('[') {
        "kernel"
    } else if !exe.is_empty() && (path == exe || path.strip_suffix(" (deleted)") == Some(exe)) {
        "binary"
    } else if path.starts_with("/dev/dri") || path.contains("i915") || path.contains("drm") {
        "gpu_buffers"
    } else if lib.contains("_dri")
        || lib.starts_with("libgallium")
        || lib.starts_with("libLLVM")
        || lib.starts_with("libEGL_mesa")
        || lib.starts_with("libGLX_mesa")
        || lib.starts_with("libglapi")
        || lib.starts_with("libdrm")
        || lib.starts_with("libvulkan")
        || lib.starts_with("libgbm")
    {
        "gpu_driver"
    } else if path.starts_with("/memfd:")
        || path.starts_with("/dev/shm")
        || path.starts_with("/SYSV")
    {
        "shm"
    } else if lib.contains(".so") {
        "libs"
    } else {
        "files"
    }
}

/// `Rss` of `/proc/PID/smaps`, summed by [`mem_kind`], kB.
pub fn mem_attribution(smaps: &str, exe: &str) -> std::collections::BTreeMap<&'static str, u64> {
    let mut by = std::collections::BTreeMap::new();
    let mut kind = "anon";
    for l in smaps.lines() {
        let mut f = l.split_whitespace();
        let Some(first) = f.next() else { continue };
        if first.contains('-')
            && first.chars().next().is_some_and(|c| c.is_ascii_hexdigit())
            && !first.ends_with(':')
        {
            // "start-end perms offset dev inode [path]": the path may hold spaces
            let path = f
                .nth(4)
                .map(|p| &l[l.find(p).unwrap_or(l.len())..])
                .unwrap_or("");
            kind = mem_kind(path.trim(), exe);
        } else if first == "Rss:" {
            let kb: u64 = f.next().and_then(|v| v.parse().ok()).unwrap_or(0);
            *by.entry(kind).or_insert(0) += kb;
        }
    }
    by
}

/// `nkw-mem {"at":…,"rss_kb":…,"hwm_kb":…,"by":{kind: kB}}` on stderr.
fn print_mem(at: &str) {
    let smaps = std::fs::read_to_string("/proc/self/smaps").unwrap_or_default();
    let status = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    let exe = std::fs::read_link("/proc/self/exe")
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    let v = serde_json::json!({
        "at": at,
        "rss_kb": parse_kb(&status, "VmRSS:"),
        "hwm_kb": parse_kb(&status, "VmHWM:"),
        "anon_kb": parse_kb(&status, "RssAnon:"),
        "file_kb": parse_kb(&status, "RssFile:"),
        "shmem_kb": parse_kb(&status, "RssShmem:"),
        "threads": parse_kb(&status, "Threads:"),
        "by": mem_attribution(&smaps, &exe),
    });
    eprintln!("nkw-mem {v}");
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
            "import_us",
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
        for (us, rates) in [(5, 0), (9, 3), (7, 1)] {
            s.ext(us, rates, 1.0);
        }
        let sum = s.summary();
        assert_eq!((sum.ext_tick_us_p95, sum.rate_wishes_max), (9, 3));
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
            "ext_tick_us_p95",
            "rate_wishes_max",
            "compose_hud_changes",
            "look_events",
            "import_us_p50",
            "import_us_p95",
            "compose_us_p50",
            "compose_us_p95",
        ] {
            assert!(v.get(k).is_some(), "{k}");
        }
        assert!(
            v.get("ext_plugin_us_p95").is_none(),
            "only with NKW_STATS=ext"
        );
    }

    #[test]
    fn import_and_compose_are_measured_apart() {
        let mut s = Stats::from_env(Some("slow")).with_warmup(Some("1000"), 0.0);
        let f = |import_us, compose_us| Frame {
            pass: "world",
            import_us,
            cull_us: 50,
            compose_us,
            ..Frame::default()
        };
        // the warm-up frame does not count
        s.frame(&f(9000, 9000), 0.5, false, 1.0 / 60.0);
        for (i, (imp, comp)) in [(100, 400), (300, 200), (200, 300)].into_iter().enumerate() {
            s.frame(&f(imp, comp), 2.0 + i as f64, false, 1.0 / 60.0);
        }
        let sum = s.summary();
        assert_eq!((sum.import_us_p50, sum.import_us_p95), (200, 300));
        assert_eq!((sum.compose_us_p50, sum.compose_us_p95), (300, 400));
        // the import is part of the frame's CPU
        assert_eq!(sum.p50_us, 200 + 50 + 300);
    }

    #[test]
    fn a_gap_inside_the_warmup_is_not_a_drop() {
        // the warm-up's frames do not count in the percentiles; a stall
        // there (first world pass, shader warm-up, spawns) is not a drop
        // of the measured window either
        let p = 1.0 / 60.0;
        let mut s = Stats::from_env(Some("1")).with_warmup(Some("1000"), 0.0);
        let f = Frame {
            pass: "world",
            ..Frame::default()
        };
        s.frame(&f, 0.90, true, p);
        s.frame(&f, 0.90 + 4.0 * p, true, p); // 3 missed, in the warm-up
        assert_eq!(s.dropped, 0);
        // the gap that crosses into the window counts from its first frame on
        s.frame(&f, 1.05, true, p);
        s.frame(&f, 1.05 + 3.0 * p, true, p);
        assert_eq!(s.summary().dropped, 2, "a late gap at the end still counts");
    }

    #[test]
    fn a_late_frame_the_next_one_makes_up_for_is_not_a_drop() {
        // S5, nested (wave 4): frame 149 came 26.0 ms after 148, frame 150
        // 11.4 ms after 149 — two frames in two vblanks, drawn late then
        // early. The old rule counted the 26 ms gap alone as one drop.
        let p = 1.0 / 60.0;
        let alone = missed(0.0260, p);
        assert_eq!(alone, 1, "alone, the late gap looks like a drop");
        assert_eq!(missed_pair(0.0260, 0.0114, p), 0);
        // a real loss: the next frame comes on time, the slot stays lost
        assert_eq!(missed_pair(2.0 * p, p, p), 1);
        assert_eq!(missed_pair(3.1 * p, 1.0 * p, p), 2);
        let mut s = Stats::from_env(Some("slow"));
        let f = Frame {
            pass: "world",
            ..Frame::default()
        };
        let mut t = 0.0;
        for gap in [p, p, 0.0260, 0.0114, p, p] {
            t += gap;
            s.frame(&f, t, true, p);
        }
        assert_eq!(s.summary().dropped, 0);
        for gap in [p, 2.0 * p, p, p] {
            t += gap;
            s.frame(&f, t, true, p);
        }
        assert_eq!(s.summary().dropped, 1, "a lost slot still counts");
        // a late gap right before the animation stops counts on its own
        t += 2.0 * p;
        s.frame(&f, t, true, p);
        s.frame(&f, t + 1.0, false, p);
        assert_eq!(s.summary().dropped, 2);
    }

    #[test]
    fn the_warmup_does_not_count() {
        let mut s = Stats::from_env(Some("1")).with_warmup(Some("1500"), 10.0);
        let f = |us| Frame {
            pass: "compose",
            update_us: us,
            ..Frame::default()
        };
        s.frame(&f(9000), 10.5, false, 1.0 / 60.0);
        s.ext(1900, 0, 10.5);
        s.frame(&f(100), 11.6, false, 1.0 / 60.0);
        s.ext(30, 0, 11.6);
        let sum = s.summary();
        assert_eq!(sum.frames, 2, "frames still counted");
        assert_eq!(
            (sum.p50_us, sum.p95_us, sum.ext_tick_us_p95),
            (100, 100, 30)
        );
        let none = Stats::from_env(Some("1")).with_warmup(None, 10.0);
        assert_eq!(none.warmup_until, 0.0);
    }

    #[test]
    fn per_plugin_ticks_with_ext() {
        let mut s = Stats::from_env(Some("ext"));
        assert!(s.enabled && s.ext_detail && !s.every);
        for (i, us) in [(0, 3), (1, 50), (2, 900), (3, 4), (2, 100)] {
            s.ext_plugin(i, us, 1.0);
        }
        let m = s.summary().ext_plugin_us_p95.unwrap();
        assert_eq!(
            (m["wm"], m["objects"], m["cats"], m["overlay"]),
            (3, 50, 900, 4)
        );
        let mut off = Stats::from_env(Some("1"));
        off.ext_plugin(0, 7, 1.0);
        assert!(off.summary().ext_plugin_us_p95.is_none());
    }

    #[test]
    fn env_values() {
        assert!(Stats::from_env(Some("1")).enabled);
        let g = Stats::from_env(Some("gpu"));
        assert!(g.enabled && g.gpu && g.every);
        assert!(!Stats::from_env(Some("0")).enabled);
        assert!(!Stats::from_env(None).enabled);
        let slow = Stats::from_env(Some("slow"));
        assert!(slow.enabled && slow.slow && !slow.every);
        let both = Stats::from_env(Some("1,ext, slow"));
        assert!(both.every && both.ext_detail && both.slow);
    }

    #[test]
    fn mem_kinds() {
        let exe = "/w/target/world/nkw";
        let cases = [
            ("", "anon"),
            ("[heap]", "heap"),
            ("[stack]", "stack"),
            ("[vdso]", "kernel"),
            (exe, "binary"),
            ("/dev/dri/renderD128", "gpu_buffers"),
            ("/usr/lib/dri/iris_dri.so", "gpu_driver"),
            ("/usr/lib/libgallium-25.2.so", "gpu_driver"),
            ("/usr/lib/libLLVM.so.20.1", "gpu_driver"),
            ("/usr/lib/libEGL_mesa.so.0.0.0", "gpu_driver"),
            ("/usr/lib/libc.so.6", "libs"),
            ("/memfd:wayland-shm (deleted)", "shm"),
            ("/usr/share/fonts/x.ttf", "files"),
        ];
        for (p, want) in cases {
            assert_eq!(mem_kind(p, exe), want, "{p}");
        }
        let smaps = "\
55d0-55e0 r-xp 00000000 103:02 123   /w/target/world/nkw
Size:                 64 kB
Rss:                  40 kB
7f00-7f10 rw-p 00000000 00:00 0
Rss:                   8 kB
7f10-7f20 rw-s 00000000 00:05 9   /memfd:wayland-shm (deleted)
Rss:                   4 kB
7f20-7f30 rw-p 00000000 00:00 0   [heap]
Rss:                  16 kB
7f30-7f40 rw-p 00000000 00:00 0
Rss:                   2 kB
";
        let by = mem_attribution(smaps, exe);
        assert_eq!(by.get("binary"), Some(&40));
        assert_eq!(by.get("anon"), Some(&10));
        assert_eq!(by.get("shm"), Some(&4));
        assert_eq!(by.get("heap"), Some(&16));
        let real = mem_attribution(&std::fs::read_to_string("/proc/self/smaps").unwrap(), "");
        assert!(real.values().sum::<u64>() > 0);
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
