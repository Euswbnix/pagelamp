//! Unix limits the worker puts on itself (backstops for the allocator cap and CPU watchdog).

use std::time::Duration;

use super::WorkerLimits;

/// `RLIMIT_CPU` a little above the watchdog's budget (SIGXCPU at the soft limit, SIGKILL at
/// the hard one), no core dumps, no files written, and on Linux an address-space ceiling.
pub(crate) fn apply(limits: WorkerLimits) {
    set(
        libc::RLIMIT_CPU,
        limits.cpu_seconds + 2,
        limits.cpu_seconds + 5,
    );
    set(libc::RLIMIT_CORE, 0, 0);
    set(libc::RLIMIT_FSIZE, 0, 0);
    #[cfg(target_os = "linux")]
    set(
        libc::RLIMIT_AS,
        super::limits::LINUX_ADDRESS_SPACE_BYTES,
        super::limits::LINUX_ADDRESS_SPACE_BYTES,
    );
}

/// Lower one limit (best effort: a limit that is already lower stays as it is).
fn set(resource: LimitResource, soft: u64, hard: u64) {
    let mut current = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    // SAFETY: `current` is a valid, writable rlimit.
    if unsafe { libc::getrlimit(resource, &mut current) } != 0 {
        return;
    }
    let cap = |wanted: u64, existing: libc::rlim_t| -> libc::rlim_t {
        let wanted = libc::rlim_t::try_from(wanted).unwrap_or(libc::RLIM_INFINITY);
        if existing == libc::RLIM_INFINITY {
            wanted
        } else {
            wanted.min(existing)
        }
    };
    let max = cap(hard, current.rlim_max);
    let limit = libc::rlimit {
        rlim_cur: cap(soft, current.rlim_cur).min(max),
        rlim_max: max,
    };
    // SAFETY: `limit` is a valid rlimit no higher than the current hard limit.
    unsafe {
        libc::setrlimit(resource, &limit);
    }
}

#[cfg(all(target_os = "linux", target_env = "gnu"))]
type LimitResource = libc::__rlimit_resource_t;
#[cfg(not(all(target_os = "linux", target_env = "gnu")))]
type LimitResource = libc::c_int;

/// CPU time of this process (all threads, user + system).
pub(crate) fn process_cpu_time() -> Option<Duration> {
    let mut spec = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: `spec` is a valid, writable timespec.
    let ok = unsafe { libc::clock_gettime(libc::CLOCK_PROCESS_CPUTIME_ID, &mut spec) } == 0;
    ok.then(|| {
        Duration::new(
            u64::try_from(spec.tv_sec).unwrap_or(0),
            u32::try_from(spec.tv_nsec).unwrap_or(0),
        )
    })
}
