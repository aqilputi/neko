//! PSI triggers (`/proc/pressure/{cpu,memory}`): the kernel wakes us only when stall time crosses
//! 150 ms in a 2 s window (2 s is the smallest window an unprivileged user may ask for). The ambient
//! fire and fish tank use it; exact numbers are read elsewhere, only when looked at.

use std::io::{self, Write};
use std::os::fd::{AsRawFd, OwnedFd};
use std::os::unix::fs::OpenOptionsExt;

use crate::clock::{eventfd, ring};
use crate::{Out, SysEvent, Worker};

pub(crate) const TRIGGER: &str = "some 150000 2000000";

fn open_trigger(path: &str) -> io::Result<std::fs::File> {
    let mut f = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(libc::O_NONBLOCK | libc::O_CLOEXEC)
        .open(path)?;
    // The kernel wants the terminating NUL.
    f.write_all(format!("{TRIGGER}\0").as_bytes())?;
    Ok(f)
}

pub(crate) fn spawn(out: Out) -> io::Result<Worker> {
    let cpu = open_trigger("/proc/pressure/cpu")?;
    let mem = open_trigger("/proc/pressure/memory")?;
    let stop: std::sync::Arc<OwnedFd> = std::sync::Arc::new(eventfd()?);
    let stop_in = stop.clone();
    let handle = std::thread::Builder::new()
        .name("nkw-sys-psi".into())
        .spawn(move || {
            loop {
                let mut fds = [
                    libc::pollfd {
                        fd: cpu.as_raw_fd(),
                        events: libc::POLLPRI,
                        revents: 0,
                    },
                    libc::pollfd {
                        fd: mem.as_raw_fd(),
                        events: libc::POLLPRI,
                        revents: 0,
                    },
                    libc::pollfd {
                        fd: stop_in.as_raw_fd(),
                        events: libc::POLLIN,
                        revents: 0,
                    },
                ];
                // SAFETY: three valid pollfds.
                let r = unsafe { libc::poll(fds.as_mut_ptr(), 3, -1) };
                if r < 0 {
                    if io::Error::last_os_error().kind() == io::ErrorKind::Interrupted {
                        continue;
                    }
                    out.error("psi", io::Error::last_os_error());
                    return;
                }
                if fds[2].revents != 0 {
                    return;
                }
                if (fds[0].revents | fds[1].revents) & libc::POLLERR != 0 {
                    out.error("psi", "trigger closed by the kernel");
                    return;
                }
                let cpu_hit = fds[0].revents & libc::POLLPRI != 0;
                let mem_hit = fds[1].revents & libc::POLLPRI != 0;
                if cpu_hit || mem_hit {
                    out.send(SysEvent::Pressure {
                        cpu: cpu_hit,
                        mem: mem_hit,
                    });
                }
            }
        })?;
    Ok(Worker {
        stop: Box::new(move || ring(&stop)),
        handle,
    })
}
