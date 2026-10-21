//! Reading the small JSON files Butters keeps next to its settings.
//!
//! A file that exists but can't be used (corrupt, too big, from a newer
//! version) is moved aside to `<name>.bad` before the caller starts over,
//! so the next save can't silently overwrite the only copy.

use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;

use crate::log::Log;

/// Reads `path` as JSON. `None` when the file is missing or unusable; an
/// unusable file is renamed to `<name>.bad` and the problem is logged.
pub fn read<T: DeserializeOwned>(path: &Path, max_bytes: usize, log: &Log) -> Option<T> {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
        Err(e) => {
            log.error(format!("couldn't read {}: {e}", path.display()));
            return None;
        }
    };
    let problem = if bytes.len() >= max_bytes {
        format!("too big ({} bytes)", bytes.len())
    } else {
        match serde_json::from_slice::<T>(&bytes) {
            Ok(v) => return Some(v),
            Err(e) => e.to_string(),
        }
    };
    let bad = bad_path(path);
    match std::fs::rename(path, &bad) {
        Ok(()) => log.error(format!(
            "{} unusable ({problem}); kept it as {} and started fresh",
            path.display(),
            bad.display()
        )),
        Err(e) => log.error(format!(
            "{} unusable ({problem}); couldn't move it aside: {e}",
            path.display()
        )),
    }
    None
}

/// Logs a failed save; used where a save is best-effort and nobody waits on it.
pub fn log_save(log: &Log, path: &Path, result: std::io::Result<()>) {
    if let Err(e) = result {
        log.error(format!("couldn't save {}: {e}", path.display()));
    }
}

fn bad_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".bad");
    path.with_file_name(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unusable_file_is_kept_aside_and_missing_file_is_none() {
        let log = &Log::disabled();
        let dir = std::env::temp_dir().join(format!("butters-json-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("x.json");
        assert_eq!(read::<u32>(&path, 64, log), None);

        std::fs::write(&path, b"{not json").unwrap();
        assert_eq!(read::<u32>(&path, 64, log), None);
        assert!(!path.exists());
        assert_eq!(std::fs::read(dir.join("x.json.bad")).unwrap(), b"{not json");

        std::fs::write(&path, b"7").unwrap();
        assert_eq!(read::<u32>(&path, 64, log), Some(7));
        assert_eq!(
            read::<u32>(&path, 1, log),
            None,
            "too big counts as unusable"
        );
        assert!(!path.exists());
        let _ = std::fs::remove_dir_all(dir);
    }
}
