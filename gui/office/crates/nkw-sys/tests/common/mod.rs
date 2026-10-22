//! Test fixtures: a private `dbus-daemon` (never the user's session bus) and fake services on it.
#![allow(dead_code)]

use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crossbeam_channel::Receiver;
use nkw_sys::SysEvent;
use zbus::zvariant::{OwnedValue, Value};

pub struct PrivateBus {
    child: Child,
    pub addr: String,
    _dir: tempfile::TempDir,
}

impl Drop for PrivateBus {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// A throwaway bus with no service activation. `None` (and a printed reason) without `dbus-daemon`.
pub fn private_bus() -> Option<PrivateBus> {
    let dir = tempfile::tempdir().ok()?;
    let conf = dir.path().join("bus.conf");
    std::fs::write(
        &conf,
        format!(
            r#"<!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-BUS Bus Configuration 1.0//EN"
 "http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">
<busconfig>
  <type>session</type>
  <listen>unix:dir={}</listen>
  <auth>EXTERNAL</auth>
  <policy context="default">
    <allow send_destination="*" eavesdrop="true"/>
    <allow eavesdrop="true"/>
    <allow own="*"/>
  </policy>
</busconfig>
"#,
            dir.path().display()
        ),
    )
    .ok()?;
    let child = Command::new("dbus-daemon")
        .arg(format!("--config-file={}", conf.display()))
        .args(["--print-address", "--nofork", "--nopidfile"])
        .env_remove("DBUS_SESSION_BUS_ADDRESS")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn();
    let mut child = match child {
        Ok(c) => c,
        Err(e) => {
            eprintln!("skipping: no dbus-daemon ({e})");
            return None;
        }
    };
    let mut line = String::new();
    BufReader::new(child.stdout.take()?)
        .read_line(&mut line)
        .ok()?;
    let addr = line.trim().to_string();
    if addr.is_empty() {
        eprintln!("skipping: dbus-daemon printed no address");
        let _ = child.kill();
        return None;
    }
    Some(PrivateBus {
        child,
        addr,
        _dir: dir,
    })
}

/// A connection builder for a fake service. Connects the socket itself (as the crate does) so the
/// fakes do not leave a `blocking` pool thread ticking in the idle measurement.
pub fn conn(addr: &str) -> zbus::blocking::connection::Builder<'static> {
    let path = addr
        .strip_prefix("unix:path=")
        .map(|r| r.split(',').next().unwrap_or(r))
        .expect("dbus-daemon prints unix:path=…");
    let s = std::os::unix::net::UnixStream::connect(path).expect("connect to the private bus");
    zbus::blocking::connection::Builder::async_io_unix_stream(s)
}

/// Waits up to `timeout` for an event matching `f`, returning it.
pub fn wait_for<T>(
    rx: &Receiver<SysEvent>,
    timeout: Duration,
    mut f: impl FnMut(&SysEvent) -> Option<T>,
) -> Option<T> {
    let end = Instant::now() + timeout;
    while let Some(left) = end.checked_duration_since(Instant::now()) {
        match rx.recv_timeout(left) {
            Ok(ev) => {
                if let Some(t) = f(&ev) {
                    return Some(t);
                }
            }
            Err(_) => return None,
        }
    }
    None
}

fn ov(v: Value<'_>) -> OwnedValue {
    OwnedValue::try_from(v).expect("owned")
}

/// A fake MPRIS player; `calls` records the methods that reached it.
pub struct FakePlayer {
    pub status: String,
    pub title: String,
    pub volume: f64,
    pub calls: Arc<Mutex<Vec<String>>>,
}

#[zbus::interface(name = "org.mpris.MediaPlayer2.Player")]
impl FakePlayer {
    fn play_pause(&mut self) {
        self.calls.lock().unwrap().push("PlayPause".into());
    }
    fn next(&self) {
        self.calls.lock().unwrap().push("Next".into());
    }
    fn previous(&self) {
        self.calls.lock().unwrap().push("Previous".into());
    }
    #[zbus(property)]
    fn playback_status(&self) -> String {
        self.status.clone()
    }
    #[zbus(property)]
    fn metadata(&self) -> HashMap<String, OwnedValue> {
        HashMap::from([
            (
                "xesam:title".to_string(),
                ov(Value::from(self.title.clone())),
            ),
            (
                "xesam:artist".to_string(),
                ov(Value::from(vec!["acme radio"])),
            ),
            ("mpris:length".to_string(), ov(Value::I64(180_000_000))),
        ])
    }
    #[zbus(property)]
    fn volume(&self) -> f64 {
        self.volume
    }
    #[zbus(property)]
    fn set_volume(&mut self, v: f64) {
        self.volume = v;
        self.calls.lock().unwrap().push(format!("Volume={v:.2}"));
    }
}

/// A fake UPower `DisplayDevice`.
pub struct FakeBattery {
    pub pct: f64,
    pub state: u32,
}

#[zbus::interface(name = "org.freedesktop.UPower.Device")]
impl FakeBattery {
    #[zbus(property)]
    fn is_present(&self) -> bool {
        true
    }
    #[zbus(property)]
    fn percentage(&self) -> f64 {
        self.pct
    }
    #[zbus(property)]
    fn state(&self) -> u32 {
        self.state
    }
    #[zbus(property)]
    fn time_to_empty(&self) -> i64 {
        if self.state == 2 { 3600 } else { 0 }
    }
}

/// A fake NetworkManager root object on a wired link (no access point chain to follow).
pub struct FakeNm {
    pub state: u32,
}

#[zbus::interface(name = "org.freedesktop.NetworkManager")]
impl FakeNm {
    #[zbus(property)]
    fn state(&self) -> u32 {
        self.state
    }
    #[zbus(property)]
    fn primary_connection(&self) -> zbus::zvariant::OwnedObjectPath {
        zbus::zvariant::OwnedObjectPath::try_from(
            "/org/freedesktop/NetworkManager/ActiveConnection/1",
        )
        .unwrap()
    }
    #[zbus(property)]
    fn primary_connection_type(&self) -> String {
        "802-3-ethernet".into()
    }
}

/// Voluntary + involuntary context switches per thread (`tid → (name, count)`) of this process,
/// except the caller's.
pub fn ctx_switches_except_me() -> HashMap<i32, (String, u64)> {
    let me = unsafe { libc::gettid() };
    let mut out = HashMap::new();
    let Ok(dir) = std::fs::read_dir("/proc/self/task") else {
        return out;
    };
    for e in dir.flatten() {
        let Some(tid) = e.file_name().to_str().and_then(|s| s.parse::<i32>().ok()) else {
            continue;
        };
        if tid == me {
            continue;
        }
        let Ok(s) = std::fs::read_to_string(e.path().join("status")) else {
            continue;
        };
        let mut name = String::new();
        let mut n = 0;
        for l in s.lines() {
            if let Some(v) = l.strip_prefix("Name:") {
                name = v.trim().to_string();
            }
            if let Some(v) = l
                .strip_prefix("voluntary_ctxt_switches:")
                .or_else(|| l.strip_prefix("nonvoluntary_ctxt_switches:"))
            {
                n += v.trim().parse::<u64>().unwrap_or(0);
            }
        }
        out.insert(tid, (name, n));
    }
    out
}

/// Switches between two snapshots, total and per thread name.
pub fn ctx_delta(
    a: &HashMap<i32, (String, u64)>,
    b: &HashMap<i32, (String, u64)>,
) -> (u64, Vec<(String, u64)>) {
    let mut per: Vec<(String, u64)> = b
        .iter()
        .map(|(tid, (name, n))| (name.clone(), n - a.get(tid).map_or(0, |x| x.1.min(*n))))
        .filter(|x| x.1 > 0)
        .collect();
    per.sort();
    (per.iter().map(|x| x.1).sum(), per)
}
