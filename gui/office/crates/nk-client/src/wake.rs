//! Blocking waits without periodic wakeups: an eventfd another thread can
//! poke, and `poll(2)` over a few fds.

use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::time::Duration;

/// A Linux eventfd used as a doorbell: `ring` from any thread, the waiting
/// thread polls it for `POLLIN` and `clear`s it.
#[derive(Debug)]
pub(crate) struct Bell(OwnedFd);

impl Bell {
    pub fn new() -> io::Result<Bell> {
        // SAFETY: eventfd takes no pointers; a non-negative result is a fresh fd we own.
        let fd = unsafe { libc::eventfd(0, libc::EFD_NONBLOCK | libc::EFD_CLOEXEC) };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: fd was just returned by eventfd and is owned by nobody else.
        Ok(Bell(unsafe { OwnedFd::from_raw_fd(fd) }))
    }

    /// Wakes the poller (coalesces: many rings, one wakeup).
    pub fn ring(&self) {
        let one: u64 = 1;
        // SAFETY: writes 8 bytes from a live u64 to our own eventfd.
        unsafe {
            libc::write(self.0.as_raw_fd(), (&one as *const u64).cast(), 8);
        }
    }

    /// Resets the counter after a wakeup.
    pub fn clear(&self) {
        let mut n: u64 = 0;
        // SAFETY: reads 8 bytes into a live u64 from our own (non-blocking) eventfd.
        unsafe {
            libc::read(self.0.as_raw_fd(), (&mut n as *mut u64).cast(), 8);
        }
    }

    pub fn fd(&self) -> RawFd {
        self.0.as_raw_fd()
    }
}

/// A pidfd for our child `pid`: readable once it exits (Linux >= 5.3).
pub(crate) fn pidfd(pid: u32) -> io::Result<OwnedFd> {
    // SAFETY: pidfd_open takes a pid and flags, no pointers.
    let fd = unsafe { libc::syscall(libc::SYS_pidfd_open, pid as libc::pid_t, 0) };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: a non-negative result is a fresh fd we own.
    Ok(unsafe { OwnedFd::from_raw_fd(fd as RawFd) })
}

/// `poll(2)` on `fds` (fd, events); returns the `revents` of each, in order.
/// `None` waits forever. EINTR is retried (with the full timeout again,
/// which is fine for our uses).
pub(crate) fn poll(fds: &[(RawFd, i16)], timeout: Option<Duration>) -> io::Result<Vec<i16>> {
    let mut p: Vec<libc::pollfd> = fds.iter().map(|&(fd, events)| libc::pollfd { fd, events, revents: 0 }).collect();
    let ms = timeout.map_or(-1, |d| d.as_millis().min(i32::MAX as u128) as libc::c_int);
    loop {
        // SAFETY: p is a live, correctly sized array of pollfd.
        let r = unsafe { libc::poll(p.as_mut_ptr(), p.len() as libc::nfds_t, ms) };
        if r >= 0 {
            return Ok(p.iter().map(|x| x.revents).collect());
        }
        let e = io::Error::last_os_error();
        if e.kind() != io::ErrorKind::Interrupted {
            return Err(e);
        }
    }
}

/// Context switches (voluntary + not) of this process's threads named
/// exactly `name` (as /proc shows it: at most 15 bytes).
#[cfg(test)]
pub(crate) fn ctx_switches(name: &str) -> u64 {
    let mut total = 0;
    for t in std::fs::read_dir("/proc/self/task").unwrap().flatten() {
        let Ok(comm) = std::fs::read_to_string(t.path().join("comm")) else { continue };
        if comm.trim() != name {
            continue;
        }
        let Ok(status) = std::fs::read_to_string(t.path().join("status")) else { continue };
        total += status
            .lines()
            .filter(|l| l.starts_with("voluntary_ctxt_switches") || l.starts_with("nonvoluntary_ctxt_switches"))
            .filter_map(|l| l.split_whitespace().last()?.parse::<u64>().ok())
            .sum::<u64>();
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bell_wakes_poll_and_clears() {
        let b = Bell::new().unwrap();
        let r = poll(&[(b.fd(), libc::POLLIN)], Some(Duration::ZERO)).unwrap();
        assert_eq!(r[0] & libc::POLLIN, 0);
        b.ring();
        b.ring();
        let r = poll(&[(b.fd(), libc::POLLIN)], None).unwrap();
        assert_ne!(r[0] & libc::POLLIN, 0);
        b.clear();
        let r = poll(&[(b.fd(), libc::POLLIN)], Some(Duration::ZERO)).unwrap();
        assert_eq!(r[0] & libc::POLLIN, 0);
    }
}
