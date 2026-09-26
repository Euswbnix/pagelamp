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
/// Course names remembered for the report's pseudonymisation (see `course_aliases_path_in`).
const COURSE_ALIASES_FILE: &str = "course-aliases.json";

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

/// `<dir>/course-aliases.json`: course names and codes seen recently, so diagnostic reports
/// can pseudonymise courses that were renamed or removed. Outside `logs/` on purpose (that
/// folder is what students are invited to share).
pub fn course_aliases_path_in(dir: &Path) -> PathBuf {
    dir.join(COURSE_ALIASES_FILE)
}

/// Create the data directory (and `files/`) if missing. Returns the data dir.
pub fn ensure_dirs() -> Result<PathBuf> {
    let dir = data_dir()?;
    ensure_dirs_in(&dir)?;
    Ok(dir)
}

/// Create `dir` and `dir/files` (and any missing parents) if missing, private to the user on
/// Unix (see `create_private_dir_all`). PageLamp's own `files/` and `logs/` are made private
/// if an older version created them readable, and so is a data dir that holds nothing but
/// PageLamp's own entries. Tightening is best effort: a file system without Unix permissions
/// (a FAT/exFAT stick as `PAGELAMP_HOME`) refuses it, which is logged, not an error.
pub fn ensure_dirs_in(dir: &Path) -> std::io::Result<()> {
    create_private_dir_all(dir)?;
    create_private_dir_all(&files_dir_in(dir))?;
    #[cfg(unix)]
    {
        for own in [files_dir_in(dir), crate::diagnostics::logs_dir_in(dir)] {
            if own.is_dir() {
                best_effort(&own, restrict(&own, 0o700));
            }
        }
        best_effort(dir, make_private_if_ours(dir));
    }
    Ok(())
}

/// Create `path` (mode 0700 on Unix: the data dir holds course materials, logs and the
/// database, which other users of the computer must not read) and its missing ancestors
/// with the default mode (they may be shared, e.g. the root of a USB drive). Elsewhere the
/// platform's per-user folder already is private.
pub fn create_private_dir_all(path: &Path) -> std::io::Result<()> {
    if path.is_dir() {
        return Ok(());
    }
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)?;
    }
    let mut builder = std::fs::DirBuilder::new();
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
    match builder.create(path) {
        Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists && path.is_dir() => Ok(()),
        other => other,
    }
}

/// Clear the group/other bits of `path` (keeps the owner's) when any are set. Unix only.
#[cfg(unix)]
pub fn restrict(path: &Path, mode: u32) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let current = std::fs::metadata(path)?.permissions().mode();
    if current & 0o077 == 0 {
        return Ok(());
    }
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(current & mode))
}

#[cfg(unix)]
fn best_effort(path: &Path, result: std::io::Result<()>) {
    if let Err(err) = result {
        tracing::warn!(
            "could not make {} private: {err}",
            crate::diagnostics::shorten_home(&path.display().to_string())
        );
    }
}

/// Entries PageLamp itself creates in its data dir (plus Finder's `.DS_Store`).
#[cfg(unix)]
const OWN_ENTRIES: &[&str] = &[
    DB_FILE,
    "pagelamp.db-wal",
    "pagelamp.db-shm",
    "pagelamp.db-journal",
    FILES_DIR,
    SYNC_LOCK_FILE,
    "logs",
    COURSE_ALIASES_FILE,
    "course-aliases.json.tmp",
    ".DS_Store",
];

#[cfg(unix)]
fn make_private_if_ours(dir: &Path) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let name = entry?.file_name();
        if !OWN_ENTRIES.iter().any(|own| name == *own) {
            return Ok(()); // shared with other things: not ours to lock down
        }
    }
    restrict(dir, 0o700)
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

    #[cfg(unix)]
    #[test]
    fn data_dirs_are_private_to_the_user() {
        use std::os::unix::fs::PermissionsExt;
        let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path().join("new").join("pagelamp");
        ensure_dirs_in(&dir).unwrap();
        assert_eq!(mode(&dir), 0o700);
        assert_eq!(mode(&files_dir_in(&dir)), 0o700);
        // Only the data dir itself is made private; missing ancestors get the default mode.
        assert_ne!(mode(&temp.path().join("new")) & 0o055, 0);

        // Shared data dir (other content): it stays as it is, but our own folders are
        // locked down even if an older version made them readable.
        let shared_home = temp.path().join("shared-home");
        std::fs::create_dir_all(files_dir_in(&shared_home)).unwrap();
        std::fs::create_dir_all(shared_home.join("logs")).unwrap();
        std::fs::write(shared_home.join("notes.txt"), b"not ours").unwrap();
        for p in [
            shared_home.clone(),
            files_dir_in(&shared_home),
            shared_home.join("logs"),
        ] {
            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        ensure_dirs_in(&shared_home).unwrap();
        assert_eq!(mode(&shared_home), 0o755);
        assert_eq!(mode(&files_dir_in(&shared_home)), 0o700);
        assert_eq!(mode(&shared_home.join("logs")), 0o700);

        // Made readable by an older version: locked down, but only when it is ours alone.
        let old = temp.path().join("old");
        std::fs::create_dir_all(files_dir_in(&old)).unwrap();
        std::fs::write(db_path_in(&old), b"").unwrap();
        std::fs::write(old.join(".DS_Store"), b"").unwrap();
        std::fs::write(old.join("pagelamp.db-journal"), b"").unwrap();
        std::fs::set_permissions(&old, std::fs::Permissions::from_mode(0o755)).unwrap();
        ensure_dirs_in(&old).unwrap();
        assert_eq!(mode(&old), 0o700);

        let shared = temp.path().join("shared");
        std::fs::create_dir_all(&shared).unwrap();
        std::fs::write(shared.join("notes.txt"), b"not ours").unwrap();
        std::fs::set_permissions(&shared, std::fs::Permissions::from_mode(0o755)).unwrap();
        ensure_dirs_in(&shared).unwrap();
        assert_eq!(mode(&shared), 0o755);
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
