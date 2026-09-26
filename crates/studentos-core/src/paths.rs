//! On-disk locations. Everything lives under one data directory so that an MCP client config
//! only needs to know a single path (or nothing, when defaults are used).
//!
//! Resolution order for the data directory:
//! 1. `STUDENTOS_HOME` environment variable (tests, portable installs, MCP client configs),
//!    when set and non-empty.
//! 2. Platform data dir via `directories::ProjectDirs::from("dev", "StudentOS", "StudentOS")`
//!    (macOS: `~/Library/Application Support/dev.StudentOS.StudentOS`).
//! 3. Only if the OS reports no home directory at all: `FALLBACK_DATA_DIR`, relative to the
//!    current working directory.
//!
//! The `*_in(dir)` helpers compute the same layout for an explicit data directory (used by
//! `App::open_at` and by tests); the plain versions apply them to `data_dir()`.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

pub const HOME_ENV: &str = "STUDENTOS_HOME";

/// Data directory used when neither `STUDENTOS_HOME` nor a platform data dir is available
/// (no home directory). Relative to the current working directory.
pub const FALLBACK_DATA_DIR: &str = ".studentos";

const DB_FILE: &str = "studentos.db";
const FILES_DIR: &str = "files";
const SYNC_LOCK_FILE: &str = "sync.lock";

/// Root data directory (not created).
pub fn data_dir() -> PathBuf {
    data_dir_from(std::env::var_os(HOME_ENV))
}

/// The resolution rules of `data_dir`, with the value of `STUDENTOS_HOME` passed in
/// (`None` = unset) so they can be tested without touching the process environment.
fn data_dir_from(home_env: Option<OsString>) -> PathBuf {
    match home_env {
        Some(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => directories::ProjectDirs::from("dev", "StudentOS", "StudentOS")
            .map(|dirs| dirs.data_dir().to_path_buf())
            .unwrap_or_else(|| PathBuf::from(FALLBACK_DATA_DIR)),
    }
}

/// `<data_dir>/studentos.db`
pub fn db_path() -> PathBuf {
    db_path_in(&data_dir())
}

/// `<data_dir>/files` — cache of downloaded LMS files, laid out as `<course-dir>/<file>`.
pub fn files_dir() -> PathBuf {
    files_dir_in(&data_dir())
}

/// `<data_dir>/sync.lock` — advisory lock held by the one process that syncs.
pub fn sync_lock_path() -> PathBuf {
    sync_lock_path_in(&data_dir())
}

/// `<dir>/studentos.db`
pub fn db_path_in(dir: &Path) -> PathBuf {
    dir.join(DB_FILE)
}

/// `<dir>/files`
pub fn files_dir_in(dir: &Path) -> PathBuf {
    dir.join(FILES_DIR)
}

/// `<dir>/sync.lock`
pub fn sync_lock_path_in(dir: &Path) -> PathBuf {
    dir.join(SYNC_LOCK_FILE)
}

/// Create the data directory (and `files/`) if missing. Returns the data dir.
pub fn ensure_dirs() -> std::io::Result<PathBuf> {
    let dir = data_dir();
    ensure_dirs_in(&dir)?;
    Ok(dir)
}

/// Create `dir` and `dir/files` (and any missing parents) if missing.
pub fn ensure_dirs_in(dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(files_dir_in(dir))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_value_wins_when_non_empty() {
        let dir = data_dir_from(Some(OsString::from("/demo/studentos-home")));
        assert_eq!(dir, PathBuf::from("/demo/studentos-home"));
    }

    #[test]
    fn empty_env_value_is_ignored() {
        assert_eq!(data_dir_from(Some(OsString::new())), data_dir_from(None));
    }

    #[test]
    fn default_is_platform_dir_or_fallback() {
        let expected = directories::ProjectDirs::from("dev", "StudentOS", "StudentOS")
            .map(|dirs| dirs.data_dir().to_path_buf())
            .unwrap_or_else(|| PathBuf::from(FALLBACK_DATA_DIR));
        assert_eq!(data_dir_from(None), expected);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_default_is_application_support() {
        let dir = data_dir_from(None);
        assert!(dir.ends_with("Library/Application Support/dev.StudentOS.StudentOS"));
    }

    #[test]
    fn layout_inside_explicit_dir() {
        let dir = Path::new("/demo/data");
        assert_eq!(db_path_in(dir), PathBuf::from("/demo/data/studentos.db"));
        assert_eq!(files_dir_in(dir), PathBuf::from("/demo/data/files"));
        assert_eq!(
            sync_lock_path_in(dir),
            PathBuf::from("/demo/data/sync.lock")
        );
    }

    #[test]
    fn default_layout_uses_data_dir() {
        // Whatever `data_dir()` resolves to on this machine; nothing is created.
        let dir = data_dir();
        assert_eq!(db_path(), db_path_in(&dir));
        assert_eq!(files_dir(), files_dir_in(&dir));
        assert_eq!(sync_lock_path(), sync_lock_path_in(&dir));
    }

    #[test]
    fn ensure_dirs_in_creates_data_and_files_dirs() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path().join("nested").join("studentos");
        ensure_dirs_in(&dir).unwrap();
        assert!(dir.is_dir());
        assert!(files_dir_in(&dir).is_dir());
        // Idempotent.
        ensure_dirs_in(&dir).unwrap();
    }
}
