//! The advisory `<data_dir>/sync.lock` that keeps the CLI and the desktop app from syncing at
//! the same time (docs/ARCHITECTURE.md §2). It is an OS file lock (`File::try_lock`), so it is
//! released automatically when the holder exits or crashes — a stale lock file is harmless.

use std::fs::{File, OpenOptions, TryLockError};
use std::path::Path;

use crate::{AppError, AppErrorKind};

/// Held while a sync runs; released on drop.
#[derive(Debug)]
pub(crate) struct SyncLock {
    _file: File,
}

impl SyncLock {
    /// Take the lock without waiting. `Busy` when another process (or another `SyncLock` in
    /// this process) holds it.
    pub(crate) fn acquire(path: &Path) -> Result<SyncLock, AppError> {
        let file = open(path)?;
        match file.try_lock() {
            Ok(()) => Ok(SyncLock { _file: file }),
            Err(TryLockError::WouldBlock) => Err(busy()),
            Err(TryLockError::Error(err)) => Err(AppError::new(
                AppErrorKind::Internal,
                format!("could not lock {}: {err}", path.display()),
            )),
        }
    }
}

/// True when someone holds the lock right now (probe: take and immediately release it).
pub(crate) fn is_locked(path: &Path) -> bool {
    let Ok(file) = open(path) else {
        return false;
    };
    matches!(file.try_lock(), Err(TryLockError::WouldBlock))
    // Dropping `file` releases a lock taken by the probe.
}

pub(crate) fn busy() -> AppError {
    AppError::new(
        AppErrorKind::Busy,
        format!(
            "Another {} window or command is syncing right now. Try again when it has finished.",
            pagelamp_core::brand::PRODUCT_NAME
        ),
    )
}

fn open(path: &Path) -> Result<File, AppError> {
    OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(path)
        .map_err(|err| {
            AppError::new(
                AppErrorKind::Internal,
                format!("could not open {}: {err}", path.display()),
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn second_holder_is_busy_until_the_first_drops() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sync.lock");
        assert!(!is_locked(&path));
        let first = SyncLock::acquire(&path).unwrap();
        assert!(is_locked(&path));
        let err = SyncLock::acquire(&path).unwrap_err();
        assert_eq!(err.kind, AppErrorKind::Busy);
        drop(first);
        assert!(!is_locked(&path));
        SyncLock::acquire(&path).unwrap();
    }
}
