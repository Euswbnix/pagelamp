//! Extraction in a separate, resource-limited process (v0.3 M0.5; plan §2 M0.5).
//!
//! One bad file must not take a sync down: a decompression bomb, a parser that loops or a
//! crash in C-free but pathological code costs the worker process, never the app. The parent
//! (`extract_in_worker`) starts `<pagelamp> extract-worker --protocol 1` for ONE file, with an
//! empty environment (no `PAGELAMP_SECRET_*` or anything else reaches it), writes one JSON
//! request to its stdin and reads one JSON response from its stdout.
//!
//! Limits (`limits`), enforced from both sides:
//! - **memory:** the worker's counting global allocator (`CountingAllocator`, the only cap that
//!   works on macOS, where the rlimits don't) exits with `MEMORY_EXIT_CODE` past the cap; the
//!   OS backstops are `RLIMIT_AS` on Linux and a Job Object on Windows; on macOS the parent
//!   also polls the worker's physical footprint;
//! - **CPU:** a watchdog thread in the worker exits with `CPU_EXIT_CODE` past the CPU budget;
//!   backstops `RLIMIT_CPU` (Unix) and the Job Object (Windows);
//! - **wall clock:** the parent kills the worker after `wall_timeout(size)`;
//! - no core dumps (`RLIMIT_CORE=0`), no files written (`RLIMIT_FSIZE=0`), and on Windows no
//!   console window (`CREATE_NO_WINDOW`) and no crash dialog.
//!
//! Failures the worker can't report itself become a `WorkerFailure` class, stored as the
//! material's `text_error_kind` (schema 3), so a file that exhausted a limit is not tried
//! again until it, the protocol or the app version changes.

mod alloc;
mod child;
#[cfg(unix)]
mod limits_unix;
#[cfg(windows)]
mod limits_windows;
mod parent;
mod protocol;

use std::path::Path;
use std::time::Duration;

use crate::format::FileFormat;

pub use alloc::CountingAllocator;
pub use child::serve_stdio;
#[doc(hidden)]
pub use parent::run as extract_in_worker_with;
pub use parent::{WorkerFailure, check, extract_in_worker, extract_in_worker_cancellable};

/// Version of the request/response protocol. The worker refuses other versions
/// (`WorkerFailure::ProtocolMismatch`), which happens when a stale binary is found.
pub const PROTOCOL: u32 = 1;

/// The subcommand that runs the worker: `<pagelamp> extract-worker --protocol 1`.
pub const SUBCOMMAND: &str = "extract-worker";

/// The worker's limits, in one place (shown by `doctor`).
pub mod limits {
    /// Heap the worker may allocate (its counting allocator).
    pub const MEMORY_BYTES: u64 = 1 << 30;
    /// CPU time the worker may use: generous, so a big but legitimate slide deck on an old
    /// laptop is not classified `cpu_limit` (and then skipped until the next app version).
    pub const CPU_SECONDS: u64 = 120;
    /// Wall clock: this, plus `WALL_SECONDS_PER_MB` per MB of the file, at most `WALL_MAX_SECONDS`.
    pub const WALL_BASE_SECONDS: u64 = 120;
    pub const WALL_SECONDS_PER_MB: u64 = 2;
    pub const WALL_MAX_SECONDS: u64 = 300;
    /// Linux `RLIMIT_AS` backstop (address space, far above the heap cap: mappings count).
    pub const LINUX_ADDRESS_SPACE_BYTES: u64 = 4 << 30;
    /// Windows Job Object commit limit: this many heap caps.
    pub const WINDOWS_JOB_MEMORY_FACTOR: f64 = 1.5;
    /// macOS: the parent kills a worker whose physical footprint passes this many heap caps.
    pub const MACOS_FOOTPRINT_FACTOR: f64 = 1.5;
    /// Largest file the app extracts in its own process when the worker can't start at all
    /// (non-PDF files only).
    pub const IN_PROCESS_FALLBACK_MAX_BYTES: u64 = 5 * 1024 * 1024;
}

/// Wall-clock budget for a file of `size` bytes (`limits`).
pub fn wall_timeout(size: u64) -> Duration {
    let mb = size.div_ceil(1024 * 1024);
    let secs =
        limits::WALL_BASE_SECONDS.saturating_add(mb.saturating_mul(limits::WALL_SECONDS_PER_MB));
    Duration::from_secs(secs.min(limits::WALL_MAX_SECONDS))
}

/// Whether a file may be extracted in the app's own process when the worker can't be started
/// (`WorkerFailure::SpawnFailed` / `ProtocolMismatch`, usually antivirus or Smart App Control):
/// only small files of a supported type other than PDF, the format whose parser is most
/// exposed to hostile input.
pub fn in_process_fallback_allowed(path: &Path, mime: Option<&str>, size: u64) -> bool {
    size <= limits::IN_PROCESS_FALLBACK_MAX_BYTES
        && FileFormat::detect(path, mime).is_some_and(|format| format != FileFormat::Pdf)
}

/// What the parent asks the worker to enforce (the wall clock stays with the parent).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorkerLimits {
    pub memory_bytes: u64,
    pub cpu_seconds: u64,
}

impl Default for WorkerLimits {
    fn default() -> Self {
        WorkerLimits {
            memory_bytes: limits::MEMORY_BYTES,
            cpu_seconds: limits::CPU_SECONDS,
        }
    }
}

/// The worker exits with these when it stops itself at a limit.
pub(crate) const MEMORY_EXIT_CODE: i32 = 97;
pub(crate) const CPU_EXIT_CODE: i32 = 98;

/// End the process now, without unwinding or running destructors (safe to call from inside the
/// allocator).
pub(crate) fn exit_now(code: i32) -> ! {
    #[cfg(unix)]
    // SAFETY: `_exit` only ends the process; it is async-signal-safe and allocates nothing.
    unsafe {
        libc::_exit(code)
    }
    #[cfg(windows)]
    // SAFETY: `ExitProcess` ends the process; the argument is a plain exit code.
    unsafe {
        windows_sys::Win32::System::Threading::ExitProcess(code as u32)
    }
    #[cfg(not(any(unix, windows)))]
    std::process::exit(code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_wall_clock_grows_with_the_file_and_is_capped() {
        assert_eq!(wall_timeout(0), Duration::from_secs(120));
        assert_eq!(wall_timeout(1), Duration::from_secs(122));
        assert_eq!(wall_timeout(10 * 1024 * 1024), Duration::from_secs(140));
        assert_eq!(wall_timeout(u64::MAX), Duration::from_secs(300));
    }

    #[test]
    fn only_small_non_pdf_files_fall_back_in_process() {
        let max = limits::IN_PROCESS_FALLBACK_MAX_BYTES;
        assert!(in_process_fallback_allowed(
            Path::new("notes.md"),
            None,
            max
        ));
        assert!(in_process_fallback_allowed(
            Path::new("deck.pptx"),
            None,
            10
        ));
        assert!(!in_process_fallback_allowed(
            Path::new("notes.md"),
            None,
            max + 1
        ));
        assert!(!in_process_fallback_allowed(
            Path::new("slides.pdf"),
            None,
            10
        ));
        // The MIME type wins over a misleading extension.
        assert!(!in_process_fallback_allowed(
            Path::new("download"),
            Some("application/pdf"),
            10
        ));
        assert!(!in_process_fallback_allowed(
            Path::new("clip.mp4"),
            None,
            10
        ));
    }
}
