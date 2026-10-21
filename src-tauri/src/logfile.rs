//! A small local log file, so problems in a release build can be found.
//!
//! Release builds have no console (Windows `windows_subsystem`, Finder-launched
//! macOS apps), so stderr alone is lost. Lines go to stderr and to
//! `butters.log` in the app's log folder; nothing leaves the machine.
//! A panic hook writes the panic there too before the process aborts.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

/// Past this size the log is moved to `butters.log.old` and started fresh.
const MAX_BYTES: u64 = 512 * 1024;

/// Set once at startup; the lock keeps lines from interleaving across threads.
static FILE: OnceLock<Mutex<PathBuf>> = OnceLock::new();

/// Starts logging into `dir` and installs the panic hook. Call once, early.
pub fn init(dir: &Path) {
    let path = dir.join("butters.log");
    if let Err(e) = std::fs::create_dir_all(dir) {
        eprintln!("couldn't create log folder {}: {e}", dir.display());
        return;
    }
    rotate_if_big(&path);
    if FILE.set(Mutex::new(path)).is_err() {
        return;
    }
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let thread = std::thread::current();
        let name = thread.name().unwrap_or("unnamed");
        let bt = std::backtrace::Backtrace::force_capture();
        append(&format!("panic in thread {name}: {info}\n{bt}"));
        default_hook(info);
    }));
    append(&format!("Butters {} started", env!("CARGO_PKG_VERSION")));
}

/// Logs one line to stderr and, once `init` ran, to the log file.
pub fn error(msg: &str) {
    eprintln!("{msg}");
    append(msg);
}

fn append(msg: &str) {
    let Some(lock) = FILE.get() else { return };
    // A poisoned lock still holds a usable path.
    let path = lock.lock().unwrap_or_else(|p| p.into_inner());
    rotate_if_big(&path);
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let line = format!("{} {msg}\n", utc_stamp(secs));
    // Best effort: there's nowhere else to report a failing log write.
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&*path) {
        let _ = f.write_all(line.as_bytes());
    }
}

fn rotate_if_big(path: &Path) {
    if std::fs::metadata(path).is_ok_and(|m| m.len() > MAX_BYTES) {
        let _ = std::fs::rename(path, path.with_extension("log.old"));
    }
}

/// `YYYY-MM-DD HH:MM:SSZ` for Unix seconds, without a date crate.
fn utc_stamp(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    // Howard Hinnant's civil_from_days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}:{:02}Z",
        rem / 3600,
        rem / 60 % 60,
        rem % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utc_stamp_formats_known_instants() {
        assert_eq!(utc_stamp(0), "1970-01-01 00:00:00Z");
        assert_eq!(utc_stamp(951_782_400), "2000-02-29 00:00:00Z");
        assert_eq!(utc_stamp(1_792_540_799), "2026-10-20 23:59:59Z");
    }

    #[test]
    fn big_log_is_moved_aside() {
        let dir = std::env::temp_dir().join(format!("butters-log-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("butters.log");
        std::fs::write(&path, vec![b'x'; MAX_BYTES as usize + 1]).unwrap();
        rotate_if_big(&path);
        assert!(!path.exists());
        assert!(dir.join("butters.log.old").exists());
        let _ = std::fs::remove_dir_all(dir);
    }
}
