//! Windows limits: a Job Object around the worker (parent side), no crash dialogs and the
//! CPU clock (worker side).

use std::os::windows::io::AsRawHandle;
use std::process::Child;
use std::time::Duration;

use windows_sys::Win32::Foundation::{CloseHandle, FILETIME, HANDLE};
use windows_sys::Win32::System::Diagnostics::Debug::{
    SEM_FAILCRITICALERRORS, SEM_NOGPFAULTERRORBOX, SetErrorMode,
};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_DIE_ON_UNHANDLED_EXCEPTION,
    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, JOB_OBJECT_LIMIT_PROCESS_MEMORY,
    JOB_OBJECT_LIMIT_PROCESS_TIME, JOBOBJECT_BASIC_ACCOUNTING_INFORMATION,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectBasicAccountingInformation,
    JobObjectExtendedLimitInformation, QueryInformationJobObject, SetInformationJobObject,
};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};

use super::WorkerLimits;

/// Worker side: no "program stopped working" dialog and no "insert disk" prompt.
pub(crate) fn no_crash_dialogs() {
    // SAFETY: plain flags; returns the previous mode.
    unsafe {
        SetErrorMode(SEM_FAILCRITICALERRORS | SEM_NOGPFAULTERRORBOX);
    }
}

/// CPU time of this process (user + kernel).
pub(crate) fn process_cpu_time() -> Option<Duration> {
    let zero = FILETIME {
        dwLowDateTime: 0,
        dwHighDateTime: 0,
    };
    let (mut creation, mut exit, mut kernel, mut user) = (zero, zero, zero, zero);
    // SAFETY: all four pointers are valid, writable FILETIMEs; the pseudo-handle needs no close.
    let ok = unsafe {
        GetProcessTimes(
            GetCurrentProcess(),
            &mut creation,
            &mut exit,
            &mut kernel,
            &mut user,
        )
    } != 0;
    let ticks = |t: FILETIME| (u64::from(t.dwHighDateTime) << 32) | u64::from(t.dwLowDateTime);
    ok.then(|| Duration::from_nanos((ticks(kernel) + ticks(user)).saturating_mul(100)))
}

/// Parent side: a Job Object holding one worker. Closing it (drop) kills the worker.
pub(crate) struct Job(HANDLE);

// SAFETY: a job handle may be used and closed from any thread.
unsafe impl Send for Job {}

impl Job {
    /// A job with a commit limit (`WINDOWS_JOB_MEMORY_FACTOR` × the heap cap), a CPU-time limit
    /// a little above the worker's own watchdog, kill-on-close and no crash dialogs.
    pub(crate) fn new(limits: WorkerLimits) -> std::io::Result<Job> {
        // SAFETY: no security attributes, no name.
        let handle = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if handle.is_null() {
            return Err(std::io::Error::last_os_error());
        }
        let job = Job(handle);
        let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_PROCESS_MEMORY
            | JOB_OBJECT_LIMIT_PROCESS_TIME
            | JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
            | JOB_OBJECT_LIMIT_DIE_ON_UNHANDLED_EXCEPTION;
        // 100 ns units.
        let cpu = (limits.cpu_seconds + 5).saturating_mul(10_000_000);
        info.BasicLimitInformation.PerProcessUserTimeLimit = i64::try_from(cpu).unwrap_or(i64::MAX);
        info.ProcessMemoryLimit = job_memory(limits);
        // SAFETY: `info` is a valid JOBOBJECT_EXTENDED_LIMIT_INFORMATION of the stated size.
        let ok = unsafe {
            SetInformationJobObject(
                job.0,
                JobObjectExtendedLimitInformation,
                (&info as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        } != 0;
        if !ok {
            return Err(std::io::Error::last_os_error());
        }
        Ok(job)
    }

    /// Put `child` in the job.
    pub(crate) fn assign(&self, child: &Child) -> std::io::Result<()> {
        // SAFETY: both handles are valid for the duration of the call.
        // `RawHandle` and `HANDLE` are both `*mut c_void`.
        let ok = unsafe { AssignProcessToJobObject(self.0, child.as_raw_handle()) } != 0;
        if ok {
            Ok(())
        } else {
            Err(std::io::Error::last_os_error())
        }
    }

    /// The job's peak process commit and its limit (bytes).
    pub(crate) fn peak_memory(&self) -> Option<(u64, u64)> {
        let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        // SAFETY: `info` is a writable buffer of the stated size.
        let ok = unsafe {
            QueryInformationJobObject(
                self.0,
                JobObjectExtendedLimitInformation,
                (&mut info as *mut JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                std::ptr::null_mut(),
            )
        } != 0;
        ok.then(|| {
            (
                info.PeakProcessMemoryUsed as u64,
                info.ProcessMemoryLimit as u64,
            )
        })
    }

    /// CPU time the job's processes used (user + kernel).
    pub(crate) fn cpu_time(&self) -> Option<Duration> {
        let mut info = JOBOBJECT_BASIC_ACCOUNTING_INFORMATION::default();
        // SAFETY: `info` is a writable buffer of the stated size.
        let ok = unsafe {
            QueryInformationJobObject(
                self.0,
                JobObjectBasicAccountingInformation,
                (&mut info as *mut JOBOBJECT_BASIC_ACCOUNTING_INFORMATION).cast(),
                std::mem::size_of::<JOBOBJECT_BASIC_ACCOUNTING_INFORMATION>() as u32,
                std::ptr::null_mut(),
            )
        } != 0;
        let ticks = info.TotalUserTime.saturating_add(info.TotalKernelTime);
        ok.then(|| Duration::from_nanos(u64::try_from(ticks).unwrap_or(0).saturating_mul(100)))
    }
}

impl Drop for Job {
    fn drop(&mut self) {
        // SAFETY: the handle is ours and closed once; kill-on-close ends the worker.
        unsafe {
            CloseHandle(self.0);
        }
    }
}

fn job_memory(limits: WorkerLimits) -> usize {
    let bytes = limits.memory_bytes as f64 * super::limits::WINDOWS_JOB_MEMORY_FACTOR;
    usize::try_from(bytes as u64).unwrap_or(usize::MAX)
}
