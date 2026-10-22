//! PipeWire against a private daemon (its own socket in a temp dir, a null sink, no session
//! manager, so the test configures ports and links itself): the user's real server and volumes are
//! never touched. Skipped without `pipewire` and its CLI tools.
#![cfg(feature = "pipewire")]

mod common;

use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use common::wait_for;
use nkw_sys::{AudioEvent, SysEvent, SysOpts};

const T: Duration = Duration::from_secs(5);

const CONF: &str = r#"
context.properties = { core.daemon = true core.name = nkw-test default.clock.rate = 48000 support.dbus = false mem.allow-mlock = false }
context.spa-libs = { audio.convert.* = audioconvert/libspa-audioconvert support.* = support/libspa-support }
context.modules = [
    { name = libpipewire-module-protocol-native args = { sockets = [ { name = "nkw-test-0" } ] } }
    { name = libpipewire-module-access args = { } }
    { name = libpipewire-module-metadata }
    { name = libpipewire-module-spa-node-factory }
    { name = libpipewire-module-client-node }
    { name = libpipewire-module-adapter }
    { name = libpipewire-module-link-factory }
]
context.objects = [
    { factory = metadata args = { metadata.name = default } }
    { factory = spa-node-factory args = { factory.name = support.node.driver node.name = nkw-test-driver priority.driver = 20000 } }
    { factory = adapter args = { factory.name = support.null-audio-sink node.name = nkw-test-sink media.class = Audio/Sink audio.position = [ FL FR ] object.linger = true adapter.auto-port-config = { mode = dsp monitor = false position = preserve } } }
]
"#;

struct PrivatePw {
    child: Child,
    dir: tempfile::TempDir,
}

impl PrivatePw {
    fn remote(&self) -> String {
        self.dir.path().join("nkw-test-0").display().to_string()
    }

    /// A PipeWire tool pointed at the private daemon only.
    fn tool(&self, bin: &str) -> Command {
        let mut c = Command::new(bin);
        c.env("PIPEWIRE_REMOTE", self.remote())
            .env("PIPEWIRE_RUNTIME_DIR", self.dir.path())
            .env("XDG_RUNTIME_DIR", self.dir.path())
            .stdin(Stdio::null())
            .stderr(Stdio::null());
        c
    }

    fn output_of(&self, bin: &str, args: &[&str]) -> String {
        let out = self.tool(bin).args(args).output().expect(bin);
        String::from_utf8_lossy(&out.stdout).into_owned()
    }
}

impl Drop for PrivatePw {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn private_pw() -> Option<PrivatePw> {
    for bin in [
        "pipewire",
        "pw-cat",
        "pw-cli",
        "pw-link",
        "pw-dump",
        "pw-metadata",
    ] {
        if Command::new(bin)
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_err()
        {
            eprintln!("skipping: no {bin}");
            return None;
        }
    }
    let dir = tempfile::tempdir().ok()?;
    let conf = dir.path().join("nkw-test.conf");
    std::fs::write(&conf, CONF).ok()?;
    let child = Command::new("pipewire")
        .arg("-c")
        .arg(&conf)
        .env("PIPEWIRE_RUNTIME_DIR", dir.path())
        .env("PIPEWIRE_CONFIG_DIR", dir.path())
        .env("XDG_RUNTIME_DIR", dir.path())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let pw = PrivatePw { child, dir };
    let sock: PathBuf = pw.dir.path().join("nkw-test-0");
    let end = Instant::now() + T;
    while !sock.exists() && Instant::now() < end {
        std::thread::sleep(Duration::from_millis(10));
    }
    sock.exists().then_some(pw)
}

fn dump(pw: &PrivatePw) -> Vec<serde_json::Value> {
    serde_json::from_str::<serde_json::Value>(&pw.output_of("pw-dump", &[]))
        .ok()
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default()
}

fn node_id(pw: &PrivatePw, name: &str) -> Option<u64> {
    dump(pw)
        .iter()
        .find(|o| o["info"]["props"]["node.name"].as_str() == Some(name))
        .and_then(|o| o["id"].as_u64())
}

fn channel_volumes_of(pw: &PrivatePw, id: u32) -> Option<Vec<f64>> {
    let all = dump(pw);
    let node = all
        .iter()
        .find(|o| o["id"].as_u64() == Some(u64::from(id)))?;
    let props = node["info"]["params"]["Props"].as_array()?;
    props.iter().find_map(|p| {
        p["channelVolumes"]
            .as_array()
            .map(|a| a.iter().filter_map(|x| x.as_f64()).collect())
    })
}

fn wait_until(mut f: impl FnMut() -> bool) -> bool {
    let end = Instant::now() + T;
    while Instant::now() < end {
        if f() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    false
}

fn sink_volume(e: &SysEvent) -> Option<f32> {
    match e {
        SysEvent::Audio(AudioEvent::DefaultSinkVolume(v)) => Some(*v),
        _ => None,
    }
}

#[test]
fn streams_sink_volume_and_own_playback() {
    let Some(pw) = private_pw() else { return };
    pw.tool("pw-metadata")
        .args(["0", "default.audio.sink", r#"{"name":"nkw-test-sink"}"#])
        .stdout(Stdio::null())
        .status()
        .unwrap();

    let sys = nkw_sys::start(SysOpts {
        pipewire: true,
        pw_remote: Some(pw.remote()),
        ..SysOpts::none()
    });
    let playback = sys.playback().expect("playback with a running daemon");
    assert_eq!(playback.rate(), 48_000);

    // default sink volume, both ways (the null sink of the private daemon)
    let v = wait_for(sys.events(), T, sink_volume).expect("DefaultSinkVolume");
    assert!((v - 1.0).abs() < 1e-3, "{v}");
    sys.audio().set_default_sink_volume(0.5);
    let v = wait_for(sys.events(), T, |e| {
        sink_volume(e).filter(|v| (v - 1.0).abs() > 1e-3)
    })
    .expect("changed");
    assert!((v - 0.5).abs() < 1e-3, "{v}");

    // a client stream appears with its pid and binary
    let mut cat = pw
        .tool("pw-cat")
        .args([
            "--playback",
            "--raw",
            "--format",
            "f32",
            "--rate",
            "48000",
            "--channels",
            "2",
            "-",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .spawn()
        .unwrap();
    let info = wait_for(sys.events(), T, |e| match e {
        SysEvent::Audio(AudioEvent::Added(si)) => Some(si.clone()),
        _ => None,
    })
    .expect("Added");
    assert_eq!(info.pid, Some(cat.id()));
    assert_eq!(info.binary.as_deref(), Some("pw-cat"));
    assert!(info.output);

    sys.audio().set_channel_volumes(info.id, [0.5, 0.25]);
    assert!(
        wait_until(
            || channel_volumes_of(&pw, info.id.0).is_some_and(|v| v.len() == 2
                && (v[0] - 0.5).abs() < 1e-3
                && (v[1] - 0.25).abs() < 1e-3)
        ),
        "channelVolumes: {:?}",
        channel_volumes_of(&pw, info.id.0)
    );

    let _ = cat.kill();
    let _ = cat.wait();
    let gone = wait_for(sys.events(), T, |e| match e {
        SysEvent::Audio(AudioEvent::Removed(id)) => Some(*id),
        _ => None,
    });
    assert_eq!(gone, Some(info.id));

    // our own sounds: a 200 ms tone, then silence; the stream must stop asking
    let calls = Arc::new(AtomicUsize::new(0));
    let frames = Arc::new(AtomicUsize::new(0));
    let (c, f) = (calls.clone(), frames.clone());
    playback.set_render(Box::new(move |buf: &mut [f32]| {
        c.fetch_add(1, Ordering::SeqCst);
        let done = f.fetch_add(buf.len() / 2, Ordering::SeqCst);
        if done >= 9_600 {
            return false;
        }
        for (i, s) in buf.iter_mut().enumerate() {
            *s = ((done + i / 2) as f32 * 0.0577).sin() * 0.1;
        }
        true
    }));
    assert!(
        node_id(&pw, "nkw-sounds").is_none(),
        "not connected before the first wake"
    );
    playback.wake();
    assert!(
        wait_until(|| node_id(&pw, "nkw-sounds").is_some()),
        "own stream appears on wake"
    );
    // what a session manager would do: ports, then links
    let id = node_id(&pw, "nkw-sounds").unwrap().to_string();
    let port_config = "{ direction: Output, mode: dsp, format: { mediaType: audio, mediaSubtype: raw, \
                       format: F32P, rate: 48000, channels: 2, position: [ FL, FR ] } }";
    pw.tool("pw-cli")
        .args(["s", &id, "PortConfig", port_config])
        .stdout(Stdio::null())
        .status()
        .unwrap();
    let ports = || pw.output_of("pw-link", &["-o"]);
    assert!(
        wait_until(|| ports().contains("nkw-sounds:output_FR")),
        "ports: {}",
        ports()
    );
    for ch in ["FL", "FR"] {
        pw.tool("pw-link")
            .args([
                format!("nkw-sounds:output_{ch}"),
                format!("nkw-test-sink:playback_{ch}"),
            ])
            .status()
            .unwrap();
    }
    assert!(
        wait_until(|| calls.load(Ordering::SeqCst) > 0),
        "process never called; links: {}",
        pw.output_of("pw-link", &["-l"])
    );
    // 200 ms tone + 200 ms silence, then the disconnect
    assert!(
        wait_until(|| node_id(&pw, "nkw-sounds").is_none()),
        "stream still connected after silence"
    );
    let n = calls.load(Ordering::SeqCst);
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(
        calls.load(Ordering::SeqCst),
        n,
        "process called after the disconnect"
    );
    assert!(
        frames.load(Ordering::SeqCst) >= 9_600 + 9_600,
        "rendered the tone and 200 ms of silence"
    );

    // the next wake reconnects
    frames.store(0, Ordering::SeqCst);
    playback.wake();
    assert!(
        wait_until(|| node_id(&pw, "nkw-sounds").is_some()),
        "reconnects on wake"
    );

    let t0 = Instant::now();
    sys.shutdown();
    assert!(t0.elapsed() < Duration::from_secs(1));
}

/// Live, read-only: connects to the user's PipeWire and lists streams; sets nothing.
#[test]
#[ignore = "talks to the live PipeWire (read-only); run with NKW_SYS_PW=1 -- --ignored"]
fn live_daemon_enumerates_read_only() {
    if std::env::var("NKW_SYS_PW").as_deref() != Ok("1") {
        eprintln!("skipping: set NKW_SYS_PW=1");
        return;
    }
    let sys = nkw_sys::start(SysOpts {
        pipewire: true,
        ..SysOpts::none()
    });
    assert!(sys.playback().is_some());
    let mut streams = 0;
    let mut sink = None;
    let end = Instant::now() + Duration::from_secs(1);
    while let Some(left) = end.checked_duration_since(Instant::now()) {
        match sys.events().recv_timeout(left) {
            Ok(SysEvent::Audio(AudioEvent::Added(_))) => streams += 1,
            Ok(SysEvent::Audio(AudioEvent::DefaultSinkVolume(v))) => sink = Some(v),
            Ok(SysEvent::Error { source, msg }) => panic!("{source}: {msg}"),
            _ => {}
        }
    }
    eprintln!("live: {streams} streams, default sink volume {sink:?}");
    sys.shutdown();
}
