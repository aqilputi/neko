//! The vault's audit log (addendum §5.7): `<state-dir>/vault.log`, created
//! 0600 in a 0700 directory, one line per event, append + fsync, rotated
//! to `vault.log.1` above 256 KB. Never the phrase, never a name: the pure
//! machine's `AuditLine`s carry neither. Never written under `--demo`.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use nkw_objects::vault::AuditLine;

/// Rotation threshold.
pub const MAX_BYTES: u64 = 256 * 1024;

#[derive(Clone, Debug, Default)]
pub struct AuditLog {
    path: Option<PathBuf>,
    /// lines written (the dump)
    pub written: usize,
}

impl AuditLog {
    /// `None`: no file (`--demo`, no state dir).
    pub fn new(path: Option<PathBuf>) -> AuditLog {
        AuditLog { path, written: 0 }
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Appends one line, stamped with the wall clock now.
    pub fn write(&mut self, a: &AuditLine) {
        let Some(p) = self.path.clone() else { return };
        let line = format_line(&stamp(SystemTime::now()), a);
        match append(&p, &line) {
            Ok(()) => self.written += 1,
            // the error kind only, never the path
            Err(e) => tracing::warn!(kind = ?e.kind(), "vault.log: not written"),
        }
    }
}

/// `<stamp> <what>[ <detail>]\n`.
pub fn format_line(stamp: &str, a: &AuditLine) -> String {
    match &a.detail {
        Some(d) => format!("{stamp} {} {d}\n", a.what),
        None => format!("{stamp} {}\n", a.what),
    }
}

fn append(p: &Path, line: &str) -> std::io::Result<()> {
    if let Some(dir) = p.parent()
        && !dir.exists()
    {
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir)?;
    }
    if let Ok(m) = std::fs::metadata(p)
        && m.len() + line.len() as u64 > MAX_BYTES
    {
        let mut old = p.as_os_str().to_owned();
        old.push(".1");
        std::fs::rename(p, PathBuf::from(old))?;
    }
    let mut f: File = OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(p)?;
    // an existing file made by someone else keeps 0600 too
    f.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    f.write_all(line.as_bytes())?;
    f.sync_all()
}

/// RFC 3339 local time with the offset: `2026-10-21T20:14:03-03:00`.
pub fn stamp(t: SystemTime) -> String {
    let secs = t
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let (tm, off) = local(secs);
    let sign = if off < 0 { '-' } else { '+' };
    let off = off.abs();
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}{sign}{:02}:{:02}",
        tm.0,
        tm.1,
        tm.2,
        tm.3,
        tm.4,
        tm.5,
        off / 3600,
        (off % 3600) / 60
    )
}

/// (y, mon, d, h, min, s) and the UTC offset in seconds.
fn local(secs: i64) -> ((i32, u32, u32, u32, u32, u32), i64) {
    // SAFETY: localtime_r writes into our tm; a null return leaves UTC
    unsafe {
        let t: libc::time_t = secs as libc::time_t;
        let mut tm: libc::tm = std::mem::zeroed();
        if libc::localtime_r(&t, &mut tm).is_null() {
            return (civil(secs), 0);
        }
        (
            (
                tm.tm_year + 1900,
                (tm.tm_mon + 1) as u32,
                tm.tm_mday as u32,
                tm.tm_hour as u32,
                tm.tm_min as u32,
                tm.tm_sec as u32,
            ),
            tm.tm_gmtoff,
        )
    }
}

/// UTC civil time from Unix seconds (days-from-civil, inverted).
fn civil(secs: i64) -> (i32, u32, u32, u32, u32, u32) {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = if m <= 2 { y + 1 } else { y };
    (
        y as i32,
        m,
        d,
        (rem / 3600) as u32,
        ((rem % 3600) / 60) as u32,
        (rem % 60) as u32,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(what: &'static str, detail: Option<&str>) -> AuditLine {
        AuditLine {
            at: 0.0,
            what,
            detail: detail.map(String::from),
        }
    }

    #[test]
    fn lines_have_the_plan_s_shape() {
        assert_eq!(
            format_line("2026-10-21T20:14:03-03:00", &line("unlock", None)),
            "2026-10-21T20:14:03-03:00 unlock\n"
        );
        assert_eq!(
            format_line("T", &line("unlock_denied", Some("wrong_phrase fails=1"))),
            "T unlock_denied wrong_phrase fails=1\n"
        );
        assert_eq!(civil(0), (1970, 1, 1, 0, 0, 0));
        assert_eq!(civil(1_792_624_443), (2026, 10, 21, 23, 14, 3));
        let s = stamp(SystemTime::now());
        assert_eq!(s.len(), 25, "{s}");
        assert_eq!(&s[4..5], "-");
        assert_eq!(&s[10..11], "T");
    }

    #[test]
    fn the_file_is_0600_appended_and_rotated_above_256_kb() {
        let t = tempfile::tempdir().unwrap();
        let dir = t.path().join("estado");
        let p = dir.join("vault.log");
        let mut log = AuditLog::new(Some(p.clone()));
        log.write(&line("unlock", None));
        log.write(&line("lock_timeout", None));
        assert_eq!(log.written, 2);
        let meta = std::fs::metadata(&p).unwrap();
        assert_eq!(meta.permissions().mode() & 0o777, 0o600);
        assert_eq!(
            std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777,
            0o700
        );
        let text = std::fs::read_to_string(&p).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].ends_with(" unlock"));
        assert!(lines[1].ends_with(" lock_timeout"));
        // fill past the limit: one rotation to .1, a fresh file after
        std::fs::write(&p, vec![b'x'; MAX_BYTES as usize - 10]).unwrap();
        log.write(&line("unlock", None));
        let rotated = t.path().join("estado/vault.log.1");
        assert!(rotated.exists());
        assert_eq!(std::fs::metadata(&rotated).unwrap().len(), MAX_BYTES - 10);
        let fresh = std::fs::read_to_string(&p).unwrap();
        assert_eq!(fresh.lines().count(), 1);
        assert_eq!(
            std::fs::metadata(&p).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    #[test]
    fn no_path_no_file() {
        let mut log = AuditLog::new(None);
        log.write(&line("unlock", None));
        assert_eq!(log.written, 0);
        assert!(log.path().is_none());
    }
}
