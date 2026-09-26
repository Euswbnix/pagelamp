//! On-disk locations. Everything lives under one data directory so that an MCP client config
//! only needs to know a single path (or nothing, when defaults are used).
//!
//! Resolution order for the data directory:
//! 1. `STUDENTOS_HOME` environment variable (tests, portable installs, MCP client configs).
//! 2. Platform data dir via `directories::ProjectDirs::from("dev", "StudentOS", "StudentOS")`
//!    (macOS: `~/Library/Application Support/dev.StudentOS.StudentOS`).

use std::path::PathBuf;

pub const HOME_ENV: &str = "STUDENTOS_HOME";

/// Root data directory (not created).
pub fn data_dir() -> PathBuf {
    todo!("resolve STUDENTOS_HOME or ProjectDirs data_dir")
}

/// `<data_dir>/studentos.db`
pub fn db_path() -> PathBuf {
    data_dir().join("studentos.db")
}

/// `<data_dir>/files` — cache of downloaded LMS files, laid out as `<course-dir>/<file>`.
pub fn files_dir() -> PathBuf {
    data_dir().join("files")
}

/// Create the data directory (and `files/`) if missing. Returns the data dir.
pub fn ensure_dirs() -> std::io::Result<PathBuf> {
    todo!("create data_dir and files_dir")
}
