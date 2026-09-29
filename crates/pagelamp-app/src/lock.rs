//! The advisory `<data_dir>/sync.lock` that keeps the CLI and the desktop app from syncing at
//! the same time (docs/ARCHITECTURE.md §2). It is an OS file lock (`File::try_lock`), so it is
//! released automatically when the holder exits or crashes — a stale lock file is harmless.

use std::fs::{File, OpenOptions, TryLockError};
use std::path::Path;
use std::time::{Duration, Instant};

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
        match try_lock_within(&file, ACQUIRE_GRACE) {
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
    matches!(
        try_lock_within(&file, PROBE_GRACE),
        Err(TryLockError::WouldBlock)
    )
    // Dropping `file` releases a lock taken by the probe.
}

/// How long `acquire` rides out a lock that was released a moment ago (see `try_lock_within`).
const ACQUIRE_GRACE: Duration = Duration::from_millis(250);
/// The same for the `is_locked` probe, kept short: status and activity poll it.
const PROBE_GRACE: Duration = Duration::from_millis(20);

/// `try_lock`, retried for up to `grace`. An `flock` belongs to the open file, and a child another
/// thread of this process is starting shares every open file between fork and exec, so a lock
/// released a moment ago can linger for a while; a real holder still gives `WouldBlock`.
fn try_lock_within(file: &File, grace: Duration) -> Result<(), TryLockError> {
    let started = Instant::now();
    loop {
        match file.try_lock() {
            Err(TryLockError::WouldBlock) if started.elapsed() < grace => {
                std::thread::sleep(Duration::from_millis(5));
            }
            other => return other,
        }
    }
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
    let mut options = OpenOptions::new();
    options.create(true).truncate(false).write(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    options.open(path).map_err(|err| {
        AppError::new(
            AppErrorKind::Internal,
            format!("could not open {}: {err}", path.display()),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn the_lock_file_is_private() {
        use std::os::unix::fs::PermissionsExt;
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("sync.lock");
        drop(SyncLock::acquire(&path).unwrap());
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o077, 0);
    }

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
        // Another test's children may hold a just-released lock for a moment.
        let deadline = Instant::now() + Duration::from_secs(2);
        while is_locked(&path) {
            assert!(Instant::now() < deadline, "still locked");
        }
        SyncLock::acquire(&path).unwrap();
    }

    /// An `flock` belongs to the open file, and a child another thread is starting shares every
    /// open file between fork and exec, so a lock released a moment ago can linger. Acquiring
    /// right after a release must still work while other threads start processes.
    #[cfg(unix)]
    #[test]
    fn a_just_released_lock_can_be_taken_while_other_threads_start_processes() {
        use std::sync::atomic::{AtomicBool, Ordering};
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sync.lock");
        let stop = std::sync::Arc::new(AtomicBool::new(false));
        let spawners: Vec<_> = (0..4)
            .map(|_| {
                let stop = stop.clone();
                std::thread::spawn(move || {
                    while !stop.load(Ordering::Relaxed) {
                        let _ = std::process::Command::new("/bin/sh")
                            .args(["-c", "exit 0"])
                            .status();
                    }
                })
            })
            .collect();
        let mut busy = 0;
        for _ in 0..400 {
            match SyncLock::acquire(&path) {
                Ok(lock) => drop(lock),
                Err(err) if err.kind == AppErrorKind::Busy => busy += 1,
                Err(err) => panic!("{err}"),
            }
        }
        stop.store(true, Ordering::Relaxed);
        for spawner in spawners {
            spawner.join().unwrap();
        }
        assert_eq!(busy, 0, "a released lock lingered past the grace period");
    }
}
