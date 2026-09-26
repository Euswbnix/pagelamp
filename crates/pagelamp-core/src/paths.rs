//! On-disk locations. Everything lives under one data directory so that an MCP client config
//! only needs to know a single path (or nothing, when defaults are used).
//!
//! Resolution order for the data directory:
//! 1. `PAGELAMP_HOME` environment variable (tests, portable installs, MCP client configs),
//!    when set and non-empty.
//! 2. Platform data dir via `directories::ProjectDirs::from("dev", "PageLamp", "PageLamp")`
//!    (macOS: `~/Library/Application Support/dev.PageLamp.PageLamp`).
//!
//! If neither is available (the OS reports no home directory) resolution FAILS with
//! `Error::NoDataDir` instead of guessing: MCP servers are spawned by AI clients with an
//! arbitrary working directory (often `/`), so a cwd-relative fallback would silently create
//! a second, empty database.
//!
//! The `*_in(dir)` helpers compute the same layout for an explicit data directory (used by
//! `App::open_at` and by tests); the plain versions apply them to `data_dir()`.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use crate::{Error, Result};

pub const HOME_ENV: &str = "PAGELAMP_HOME";

/// Log level of PageLamp's own diagnostics (`debug` = what `pagelamp sync -v` prints).
pub const LOG_ENV: &str = "PAGELAMP_LOG";

const DB_FILE: &str = "pagelamp.db";
const FILES_DIR: &str = "files";
const SYNC_LOCK_FILE: &str = "sync.lock";

/// Root data directory (not created). Errors with `Error::NoDataDir` when neither
/// `PAGELAMP_HOME` nor a platform data directory is available.
pub fn data_dir() -> Result<PathBuf> {
    data_dir_from(std::env::var_os(HOME_ENV), platform_data_dir())
}

/// The platform default data directory, ignoring `PAGELAMP_HOME`
/// (`~/Library/Application Support/dev.PageLamp.PageLamp` and equivalents); None without a
/// home directory. MCP client configs only need `PAGELAMP_HOME` when the data dir differs
/// from this.
pub fn platform_data_dir() -> Option<PathBuf> {
    directories::ProjectDirs::from("dev", "PageLamp", "PageLamp")
        .map(|dirs| dirs.data_dir().to_path_buf())
}

/// The resolution rules of `data_dir` with their inputs passed in (`PAGELAMP_HOME` value,
/// platform dir) so they can be tested without touching the process environment.
fn data_dir_from(home_env: Option<OsString>, platform: Option<PathBuf>) -> Result<PathBuf> {
    match home_env {
        Some(dir) if !dir.is_empty() => Ok(PathBuf::from(dir)),
        _ => platform.ok_or(Error::NoDataDir),
    }
}

/// `<data_dir>/pagelamp.db`
pub fn db_path() -> Result<PathBuf> {
    Ok(db_path_in(&data_dir()?))
}

/// `<data_dir>/files` — cache of downloaded LMS files, laid out as `<course-dir>/<file>`.
pub fn files_dir() -> Result<PathBuf> {
    Ok(files_dir_in(&data_dir()?))
}

/// `<data_dir>/sync.lock` — advisory lock held by the one process that syncs.
pub fn sync_lock_path() -> Result<PathBuf> {
    Ok(sync_lock_path_in(&data_dir()?))
}

/// `<dir>/pagelamp.db`
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
pub fn ensure_dirs() -> Result<PathBuf> {
    let dir = data_dir()?;
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

    fn platform() -> Option<PathBuf> {
        Some(PathBuf::from("/demo/platform-data"))
    }

    #[test]
    fn env_value_wins_when_non_empty() {
        let dir = data_dir_from(Some(OsString::from("/demo/pagelamp-home")), platform()).unwrap();
        assert_eq!(dir, PathBuf::from("/demo/pagelamp-home"));
        // Even without a home directory.
        let dir = data_dir_from(Some(OsString::from("/demo/pagelamp-home")), None).unwrap();
        assert_eq!(dir, PathBuf::from("/demo/pagelamp-home"));
    }

    #[test]
    fn empty_env_value_is_ignored() {
        let dir = data_dir_from(Some(OsString::new()), platform()).unwrap();
        assert_eq!(dir, PathBuf::from("/demo/platform-data"));
    }

    #[test]
    fn no_env_and_no_home_is_a_hard_error() {
        let err = data_dir_from(None, None).unwrap_err();
        assert!(matches!(err, Error::NoDataDir));
        assert!(err.to_string().contains("set PAGELAMP_HOME"), "{err}");
        assert!(data_dir_from(Some(OsString::new()), None).is_err());
    }

    // The CLI, the desktop app and every `pagelamp mcp` launched by an AI app resolve the
    // data dir through `data_dir()`; without PAGELAMP_HOME they must all land here, so the
    // MCP server sees what the desktop app synced. These pin the per-OS location.
    #[cfg(target_os = "macos")]
    #[test]
    fn macos_default_is_application_support() {
        let dir = platform_data_dir().unwrap();
        assert!(dir.ends_with("Library/Application Support/dev.PageLamp.PageLamp"));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_default_is_roaming_app_data() {
        let dir = platform_data_dir().unwrap();
        assert!(
            dir.ends_with(r"PageLamp\PageLamp\data"),
            "{}",
            dir.display()
        );
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    #[test]
    fn linux_default_is_xdg_data_home() {
        let dir = platform_data_dir().unwrap();
        assert!(dir.ends_with("pagelamp"), "{}", dir.display());
    }

    #[test]
    fn default_is_the_platform_dir_without_pagelamp_home() {
        let platform = platform_data_dir();
        match data_dir_from(None, platform.clone()) {
            Ok(dir) => assert_eq!(Some(dir), platform),
            Err(err) => assert!(platform.is_none() && matches!(err, Error::NoDataDir)),
        }
    }

    #[test]
    fn layout_inside_explicit_dir() {
        let dir = Path::new("/demo/data");
        assert_eq!(db_path_in(dir), PathBuf::from("/demo/data/pagelamp.db"));
        assert_eq!(files_dir_in(dir), PathBuf::from("/demo/data/files"));
        assert_eq!(
            sync_lock_path_in(dir),
            PathBuf::from("/demo/data/sync.lock")
        );
    }

    #[test]
    fn default_layout_uses_data_dir() {
        // Whatever `data_dir()` resolves to on this machine; nothing is created.
        let dir = data_dir().unwrap();
        assert_eq!(db_path().unwrap(), db_path_in(&dir));
        assert_eq!(files_dir().unwrap(), files_dir_in(&dir));
        assert_eq!(sync_lock_path().unwrap(), sync_lock_path_in(&dir));
    }

    #[test]
    fn ensure_dirs_in_creates_data_and_files_dirs() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path().join("nested").join("pagelamp");
        ensure_dirs_in(&dir).unwrap();
        assert!(dir.is_dir());
        assert!(files_dir_in(&dir).is_dir());
        // Idempotent.
        ensure_dirs_in(&dir).unwrap();
    }
}
