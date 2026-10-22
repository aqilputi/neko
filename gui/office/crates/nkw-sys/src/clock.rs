//! The minute clock: one `timerfd` on `CLOCK_REALTIME`, armed at the next wall-clock minute with
//! `TFD_TIMER_CANCEL_ON_SET`, so a clock change or a resume wakes us too (`ECANCELED`). One wakeup a
//! minute, nothing in between.

use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};

use crate::{ClockTick, Out, SysEvent, Worker};

/// The day band (8 bands of 3 hours, addendum §2.1): 0 = 00–03 … 7 = 21–24. The sky, fog and sun
/// only re-render when it changes. `minute` is accepted so band edges can move off the hour later.
pub fn band_of(hour: u8, minute: u8) -> u8 {
    let _ = minute;
    (hour.min(23)) / 3
}

/// The next multiple of `period_s` strictly after `now_s`.
pub fn next_boundary(now_s: i64, period_s: u32) -> i64 {
    let p = i64::from(period_s.max(1));
    (now_s.div_euclid(p) + 1) * p
}

/// A tick at `unix_s`, in local time.
pub fn tick_at(unix_s: i64) -> ClockTick {
    let (hour, minute) = local_hm(unix_s);
    ClockTick {
        unix_s,
        hour,
        minute,
        band: band_of(hour, minute),
    }
}

fn local_hm(unix_s: i64) -> (u8, u8) {
    let t: libc::time_t = unix_s as libc::time_t;
    // SAFETY: `tm` is plain old data; localtime_r only writes into it.
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    let ok = unsafe { !libc::localtime_r(&t, &mut tm).is_null() };
    if !ok {
        let s = unix_s.rem_euclid(86_400);
        return ((s / 3600) as u8, ((s % 3600) / 60) as u8);
    }
    (tm.tm_hour as u8, tm.tm_min as u8)
}

fn now_s() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn cvt(r: libc::c_int) -> io::Result<libc::c_int> {
    if r < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(r)
    }
}

fn arm(tfd: &OwnedFd, at_s: i64) -> io::Result<()> {
    let spec = libc::itimerspec {
        it_interval: libc::timespec {
            tv_sec: 0,
            tv_nsec: 0,
        },
        it_value: libc::timespec {
            tv_sec: at_s as libc::time_t,
            tv_nsec: 0,
        },
    };
    let flags = libc::TFD_TIMER_ABSTIME | libc::TFD_TIMER_CANCEL_ON_SET;
    // SAFETY: valid fd and a fully initialised itimerspec.
    cvt(unsafe { libc::timerfd_settime(tfd.as_raw_fd(), flags, &spec, std::ptr::null_mut()) })?;
    Ok(())
}

/// An eventfd used as the "please stop" doorbell of a polling thread.
pub(crate) fn eventfd() -> io::Result<OwnedFd> {
    // SAFETY: plain syscall; the fd is owned right away.
    let fd = cvt(unsafe { libc::eventfd(0, libc::EFD_CLOEXEC | libc::EFD_NONBLOCK) })?;
    Ok(unsafe { OwnedFd::from_raw_fd(fd) })
}

pub(crate) fn ring(fd: &OwnedFd) {
    let one: u64 = 1;
    // SAFETY: writes 8 bytes from a valid u64.
    unsafe { libc::write(fd.as_raw_fd(), (&one as *const u64).cast(), 8) };
}

pub(crate) fn spawn(period_s: u32, out: Out) -> io::Result<Worker> {
    // SAFETY: plain syscall; the fd is owned right away.
    let tfd = cvt(unsafe { libc::timerfd_create(libc::CLOCK_REALTIME, libc::TFD_CLOEXEC) })?;
    let tfd = unsafe { OwnedFd::from_raw_fd(tfd) };
    let stop = std::sync::Arc::new(eventfd()?);
    arm(&tfd, next_boundary(now_s(), period_s))?;
    let stop_in = stop.clone();
    let handle = std::thread::Builder::new()
        .name("nkw-sys-clock".into())
        .spawn(move || {
            out.send(SysEvent::Clock(tick_at(now_s())));
            loop {
                let mut fds = [
                    libc::pollfd {
                        fd: tfd.as_raw_fd(),
                        events: libc::POLLIN,
                        revents: 0,
                    },
                    libc::pollfd {
                        fd: stop_in.as_raw_fd(),
                        events: libc::POLLIN,
                        revents: 0,
                    },
                ];
                // SAFETY: two valid pollfds.
                let r = unsafe { libc::poll(fds.as_mut_ptr(), 2, -1) };
                if r < 0 {
                    if io::Error::last_os_error().kind() == io::ErrorKind::Interrupted {
                        continue;
                    }
                    out.error("clock", io::Error::last_os_error());
                    return;
                }
                if fds[1].revents != 0 {
                    return;
                }
                if fds[0].revents != 0 {
                    let mut buf = [0u8; 8];
                    // SAFETY: reads 8 bytes into a valid buffer. ECANCELED means the clock was set.
                    let n = unsafe { libc::read(tfd.as_raw_fd(), buf.as_mut_ptr().cast(), 8) };
                    if n < 0 && io::Error::last_os_error().raw_os_error() != Some(libc::ECANCELED) {
                        continue;
                    }
                    let now = now_s();
                    out.send(SysEvent::Clock(tick_at(now)));
                    if let Err(e) = arm(&tfd, next_boundary(now, period_s)) {
                        out.error("clock", e);
                        return;
                    }
                }
            }
        })?;
    Ok(Worker {
        stop: Box::new(move || ring(&stop)),
        handle,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bands_table() {
        let cases: &[(u8, u8, u8)] = &[
            (0, 0, 0),
            (2, 59, 0),
            (3, 0, 1),
            (5, 59, 1),
            (6, 0, 2),
            (9, 0, 3),
            (12, 30, 4),
            (15, 0, 5),
            (18, 0, 6),
            (20, 59, 6),
            (21, 0, 7),
            (23, 59, 7),
            (99, 0, 7), // out of range clamps
        ];
        for &(h, m, b) in cases {
            assert_eq!(band_of(h, m), b, "{h}:{m}");
        }
        let all: std::collections::BTreeSet<u8> = (0..24).map(|h| band_of(h, 0)).collect();
        assert_eq!(all.len(), 8);
    }

    #[test]
    fn next_boundary_table() {
        assert_eq!(next_boundary(0, 60), 60);
        assert_eq!(next_boundary(59, 60), 60);
        assert_eq!(next_boundary(60, 60), 120);
        assert_eq!(next_boundary(61, 60), 120);
        assert_eq!(next_boundary(10, 1), 11);
        assert_eq!(next_boundary(-1, 60), 0);
        assert_eq!(next_boundary(5, 0), 6, "period 0 behaves as 1");
    }

    #[test]
    fn tick_is_consistent() {
        let t = tick_at(1_700_000_000);
        assert!(t.hour < 24 && t.minute < 60);
        assert_eq!(t.band, band_of(t.hour, t.minute));
        assert_eq!(t.unix_s, 1_700_000_000);
    }
}
