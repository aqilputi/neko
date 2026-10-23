//! The scan thread (plan §7.2): one thread owns the file-system port and
//! serves bounded requests — a shelf, a search within its budget, the vault's
//! cards, a preview — and wakes the loop when a result is ready. `inline`
//! serves on the caller's thread (unit tests, deterministic).
//!
//! The thread is started on the first request, never at startup (no scan at
//! rest, S1).

use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::Instant;

use super::model::{
    Config, FsError, FsPort, PREVIEW_MAX, Search, Shelf, read_shelf, read_vault, search_tree,
};

/// A port the thread can own.
pub type Port = Arc<dyn FsPort + Send + Sync>;
/// Wakes the event loop (calloop's `Ping`).
pub type Wake = Arc<dyn Fn() + Send + Sync>;

#[derive(Clone, Debug, PartialEq)]
pub enum Request {
    /// a shelf; `current`: it becomes the one you look at
    Open {
        root: usize,
        rel: String,
        current: bool,
    },
    Search {
        root: usize,
        rel: String,
        q: String,
    },
    /// the vault's cards (only after an unlock)
    Vault {
        root: usize,
        rel: String,
    },
    /// the lectern (≤ 64 KB of UTF-8)
    Preview {
        root: usize,
        rel: String,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum Response {
    Shelf {
        root: usize,
        rel: String,
        current: bool,
        result: Result<Shelf, FsError>,
        ms: f64,
    },
    Search {
        search: Search,
        ms: f64,
    },
    Vault {
        rel: String,
        result: Result<Shelf, FsError>,
    },
    Preview {
        rel: String,
        text: Option<String>,
    },
}

/// Serve one request (on whatever thread).
pub fn serve(port: &dyn FsPort, cfg: &Config, r: Request) -> Response {
    let t0 = Instant::now();
    let ms = |t0: Instant| t0.elapsed().as_secs_f64() * 1000.0;
    match r {
        Request::Open { root, rel, current } => {
            let result = read_shelf(cfg, port, root, &rel);
            Response::Shelf {
                root,
                rel,
                current,
                result,
                ms: ms(t0),
            }
        }
        Request::Search { root, rel, q } => {
            let mut clock = || t0.elapsed().as_millis().min(u32::MAX as u128) as u32;
            let search = search_tree(cfg, port, root, &rel, &q, &mut clock);
            Response::Search { search, ms: ms(t0) }
        }
        Request::Vault { root, rel } => Response::Vault {
            result: read_vault(cfg, port, root, &rel),
            rel,
        },
        Request::Preview { root, rel } => Response::Preview {
            text: if cfg.policy.is_vault(&rel) {
                None
            } else {
                port.read_text(root, &rel, PREVIEW_MAX)
            },
            rel,
        },
    }
}

enum Mode {
    Inline {
        port: Port,
        cfg: Config,
        done: VecDeque<Response>,
    },
    Thread {
        tx: Sender<Request>,
        rx: Receiver<Response>,
    },
}

pub struct Scanner {
    mode: Mode,
    /// requests sent and not answered yet
    pending: usize,
}

impl Scanner {
    pub fn inline(port: Port, cfg: Config) -> Scanner {
        Scanner {
            mode: Mode::Inline {
                port,
                cfg,
                done: VecDeque::new(),
            },
            pending: 0,
        }
    }

    /// The thread; `wake` after every result.
    pub fn thread(port: Port, cfg: Config, wake: Wake) -> Scanner {
        let (tx, rx_req) = channel::<Request>();
        let (tx_res, rx) = channel::<Response>();
        let spawned = std::thread::Builder::new()
            .name("nkw-library".into())
            .spawn(move || {
                while let Ok(r) = rx_req.recv() {
                    if tx_res.send(serve(&*port, &cfg, r)).is_err() {
                        break;
                    }
                    wake();
                }
            });
        if let Err(e) = spawned {
            tracing::warn!(%e, "library: no scan thread");
        }
        Scanner {
            mode: Mode::Thread { tx, rx },
            pending: 0,
        }
    }

    pub fn request(&mut self, r: Request) {
        self.pending += 1;
        match &mut self.mode {
            Mode::Inline { port, cfg, done } => done.push_back(serve(&**port, cfg, r)),
            Mode::Thread { tx, .. } => {
                if tx.send(r).is_err() {
                    self.pending -= 1;
                }
            }
        }
    }

    /// Everything answered so far, without blocking.
    pub fn poll(&mut self) -> Vec<Response> {
        let out: Vec<Response> = match &mut self.mode {
            Mode::Inline { done, .. } => done.drain(..).collect(),
            Mode::Thread { rx, .. } => rx.try_iter().collect(),
        };
        self.pending = self.pending.saturating_sub(out.len());
        out
    }

    /// Requests in flight.
    pub fn pending(&self) -> usize {
        self.pending
    }

    /// Wait for the next answer (tests of the thread only).
    pub fn wait(&mut self, ms: u64) -> Option<Response> {
        match &mut self.mode {
            Mode::Inline { done, .. } => done.pop_front(),
            Mode::Thread { rx, .. } => rx.recv_timeout(std::time::Duration::from_millis(ms)).ok(),
        }
        .inspect(|_| self.pending = self.pending.saturating_sub(1))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::super::fixture;
    use super::*;

    #[test]
    fn the_thread_round_trips_and_wakes() {
        let woke = Arc::new(AtomicUsize::new(0));
        let w = woke.clone();
        let fs = Arc::new(fixture::demo());
        let mut s = Scanner::thread(
            fs.clone(),
            Config::default(),
            Arc::new(move || {
                w.fetch_add(1, Ordering::Relaxed);
            }),
        );
        s.request(Request::Open {
            root: 0,
            rel: "projeto".into(),
            current: true,
        });
        assert_eq!(s.pending(), 1);
        let Some(Response::Shelf {
            rel,
            result,
            current,
            ..
        }) = s.wait(5000)
        else {
            panic!("no shelf");
        };
        assert_eq!(rel, "projeto");
        assert!(current);
        assert!(result.unwrap().entries.iter().any(|e| e.name() == "src"));
        assert_eq!(s.pending(), 0);
        // the wake follows the send
        let t0 = Instant::now();
        while woke.load(Ordering::Relaxed) == 0 && t0.elapsed().as_secs() < 5 {
            std::thread::yield_now();
        }
        assert!(woke.load(Ordering::Relaxed) >= 1);
        s.request(Request::Search {
            root: 0,
            rel: String::new(),
            q: "lib".into(),
        });
        let Some(Response::Search { search, .. }) = s.wait(5000) else {
            panic!("no search");
        };
        assert!(!search.hits.is_empty());
    }

    #[test]
    fn inline_serves_at_once_and_the_preview_never_reads_the_vault() {
        let fs = Arc::new(fixture::demo());
        let mut s = Scanner::inline(fs.clone(), Config::default());
        s.request(Request::Preview {
            root: 0,
            rel: "notas.md".into(),
        });
        s.request(Request::Preview {
            root: 0,
            rel: ".ssh/config".into(),
        });
        let r = s.poll();
        assert_eq!(r.len(), 2);
        assert!(matches!(&r[0], Response::Preview { text: Some(t), .. } if t.contains("notas")));
        assert!(matches!(&r[1], Response::Preview { text: None, .. }));
        assert!(s.poll().is_empty());
    }

    #[test]
    fn a_1k_shelf_and_the_first_shelf_are_fast() {
        let mut m = super::super::model::MemFs::new(1);
        for i in 0..1000 {
            m.file(0, &format!("mil/f{i:04}.txt"), i);
        }
        let fs: Port = Arc::new(m);
        let cfg = Config::default();
        let Response::Shelf { ms, result, .. } = serve(
            &*fs,
            &cfg,
            Request::Open {
                root: 0,
                rel: "mil".into(),
                current: true,
            },
        ) else {
            panic!()
        };
        assert_eq!(result.unwrap().more, 600);
        // plan §10 fase 4: ≤ 20 ms (debug build: ≤ 5×)
        assert!(ms <= 100.0, "1k entries in {ms} ms");
        let demo: Port = Arc::new(fixture::demo());
        let Response::Shelf { ms, .. } = serve(
            &*demo,
            &cfg,
            Request::Open {
                root: 0,
                rel: String::new(),
                current: true,
            },
        ) else {
            panic!()
        };
        assert!(ms <= 500.0, "first shelf in {ms} ms");
    }

    #[test]
    fn a_50k_search_stays_within_a_second() {
        let fs: Port = Arc::new(fixture::big());
        let Response::Search { search, ms } = serve(
            &*fs,
            &Config::default(),
            Request::Search {
                root: 0,
                rel: String::new(),
                q: "arquivo 7".into(),
            },
        ) else {
            panic!()
        };
        assert!(search.scanned >= 49_000, "{}", search.scanned);
        assert!(!search.hits.is_empty());
        // ≤ 1 s (debug: ≤ 5×, and the 2 s budget would cut it anyway)
        assert!(ms <= 5000.0, "50k search in {ms} ms");
    }
}
