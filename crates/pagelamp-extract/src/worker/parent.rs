//! The parent side: run one extraction in a worker process and classify how it ended.

use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

use super::protocol::{Outcome, Request, Response};
use super::{CPU_EXIT_CODE, MEMORY_EXIT_CODE, PROTOCOL, SUBCOMMAND, WorkerLimits, wall_timeout};
use crate::{ExtractError, Segment};

/// Largest response we accept (the text is capped at 5 MB; JSON escaping can grow it).
const MAX_RESPONSE_BYTES: u64 = 32 * 1024 * 1024;
/// How often the parent looks at the worker while it runs.
const POLL: Duration = Duration::from_millis(20);

/// How a worker failed when it couldn't report an extraction result itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkerFailure {
    /// Still running at the wall-clock deadline (killed).
    TimedOut,
    /// Used its CPU budget.
    CpuLimit,
    /// Used its memory budget.
    MemoryLimit,
    /// Died (a crash, an abort, a signal).
    Crashed,
    /// Answered something that isn't a response.
    BadOutput,
    /// Could not be started (missing, or blocked by antivirus / Smart App Control).
    SpawnFailed,
    /// A worker of another protocol version (a stale binary).
    ProtocolMismatch,
}

impl WorkerFailure {
    /// The stored name (`materials.text_error_kind`).
    pub fn as_str(self) -> &'static str {
        match self {
            WorkerFailure::TimedOut => "timed_out",
            WorkerFailure::CpuLimit => "cpu_limit",
            WorkerFailure::MemoryLimit => "memory_limit",
            WorkerFailure::Crashed => "crashed",
            WorkerFailure::BadOutput => "bad_output",
            WorkerFailure::SpawnFailed => "spawn_failed",
            WorkerFailure::ProtocolMismatch => "protocol_mismatch",
        }
    }

    /// A failure caused by the file (or this version's handling of it): not retried until the
    /// file, the worker protocol or the app version changes. The others are about the
    /// worker itself and are retried on the next sync.
    pub fn is_hard(self) -> bool {
        matches!(
            self,
            WorkerFailure::TimedOut
                | WorkerFailure::CpuLimit
                | WorkerFailure::MemoryLimit
                | WorkerFailure::Crashed
        )
    }
}

/// Extract `path` in a worker process started from `exe` (`<exe> extract-worker --protocol 1`),
/// under `limits` and a wall clock of `wall_timeout(size of the file)`.
///
/// `Ok(result)`: the worker answered, and `result` is what `extract_file` returned there.
/// `Err(failure)`: it didn't (see `WorkerFailure`).
pub fn extract_in_worker(
    exe: &Path,
    path: &Path,
    mime: Option<&str>,
    limits: WorkerLimits,
) -> Result<Result<Vec<Segment>, ExtractError>, WorkerFailure> {
    let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    run(exe, path, mime, limits, wall_timeout(size), None)
}

/// Wall clock of a `check`.
const CHECK_WALL: Duration = Duration::from_secs(15);

/// Start a worker from `exe` and have it answer one request, as `doctor` does: `Ok(time)` for
/// the round trip, `Err` when it could not be started (`SpawnFailed`), is from another version
/// (`ProtocolMismatch`) or did not answer properly. The request names a file of an unsupported
/// type, which the worker declines before touching the disk, so nothing is read.
pub fn check(exe: &Path) -> Result<Duration, WorkerFailure> {
    let started = Instant::now();
    let probe = Path::new("pagelamp-worker-check.unsupported");
    match run(exe, probe, None, WorkerLimits::default(), CHECK_WALL, None)? {
        Err(ExtractError::Unsupported(_)) => Ok(started.elapsed()),
        _ => Err(WorkerFailure::BadOutput),
    }
}

/// `extract_in_worker` with an explicit wall clock and, for tests of debug builds, a fault the
/// worker should simulate (`hang`, `spin`, `allocate`, `abort`, `garbage`).
#[doc(hidden)]
pub fn run(
    exe: &Path,
    path: &Path,
    mime: Option<&str>,
    limits: WorkerLimits,
    wall: Duration,
    debug_fault: Option<&str>,
) -> Result<Result<Vec<Segment>, ExtractError>, WorkerFailure> {
    let request = Request {
        protocol: PROTOCOL,
        path: path.to_string_lossy().into_owned(),
        mime: mime.map(str::to_string),
        memory_bytes: limits.memory_bytes,
        cpu_seconds: limits.cpu_seconds,
        debug_fault: debug_fault.map(str::to_string),
    };
    let mut command = Command::new(exe);
    command
        .args([SUBCOMMAND, "--protocol", &PROTOCOL.to_string()])
        .env_clear()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // Windows itself needs this one to start a process; it holds no secret.
        if let Some(root) = std::env::var_os("SystemRoot") {
            command.env("SystemRoot", root);
        }
        command.creation_flags(windows_sys::Win32::System::Threading::CREATE_NO_WINDOW);
    }
    let mut child = command.spawn().map_err(|_| WorkerFailure::SpawnFailed)?;
    #[cfg(windows)]
    let job = super::limits_windows::Job::new(limits).ok();
    #[cfg(windows)]
    if let Some(job) = &job {
        let _ = job.assign(&child);
    }

    // The request is small; a worker that died already makes this fail, which the exit status
    // then explains.
    if let Some(mut stdin) = child.stdin.take() {
        let _ = serde_json::to_writer(&mut stdin, &request);
        let _ = stdin.flush();
    }
    let reader = child.stdout.take().map(|stdout| {
        std::thread::spawn(move || {
            let mut stdout = stdout;
            let mut kept = Vec::new();
            let _ = (&mut stdout)
                .take(MAX_RESPONSE_BYTES + 1)
                .read_to_end(&mut kept);
            // Keep draining so the worker never blocks on a full pipe.
            let _ = std::io::copy(&mut stdout, &mut std::io::sink());
            kept
        })
    });

    let deadline = Instant::now() + wall;
    let mut stopped: Option<WorkerFailure> = None;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) => {}
            Err(_) => {
                let _ = child.kill();
                stopped = Some(WorkerFailure::Crashed);
                break child.wait().ok();
            }
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            stopped = Some(WorkerFailure::TimedOut);
            break child.wait().ok();
        }
        #[cfg(target_os = "macos")]
        if footprint(child.id()).is_some_and(|bytes| {
            bytes as f64 > limits.memory_bytes as f64 * super::limits::MACOS_FOOTPRINT_FACTOR
        }) {
            let _ = child.kill();
            stopped = Some(WorkerFailure::MemoryLimit);
            break child.wait().ok();
        }
        std::thread::sleep(POLL);
    };
    let output = reader.and_then(|r| r.join().ok()).unwrap_or_default();
    if let Some(failure) = stopped {
        return Err(failure);
    }
    let Some(status) = status else {
        return Err(WorkerFailure::Crashed);
    };
    #[cfg(windows)]
    if !status.success() {
        return Err(classify_windows(&status, job.as_ref(), limits));
    }
    classify(status, &output)
}

/// Turn the worker's exit status and output into the result.
fn classify(
    status: ExitStatus,
    output: &[u8],
) -> Result<Result<Vec<Segment>, ExtractError>, WorkerFailure> {
    match status.code() {
        Some(0) => {}
        Some(MEMORY_EXIT_CODE) => return Err(WorkerFailure::MemoryLimit),
        Some(CPU_EXIT_CODE) => return Err(WorkerFailure::CpuLimit),
        // A binary that doesn't know `extract-worker` exits with a usage error and no output.
        Some(2) if output.is_empty() => return Err(WorkerFailure::ProtocolMismatch),
        Some(_) => return Err(WorkerFailure::Crashed),
        None => {
            #[cfg(unix)]
            {
                use std::os::unix::process::ExitStatusExt;
                if status.signal() == Some(libc::SIGXCPU) {
                    return Err(WorkerFailure::CpuLimit);
                }
            }
            return Err(WorkerFailure::Crashed);
        }
    }
    if output.len() as u64 > MAX_RESPONSE_BYTES {
        return Err(WorkerFailure::BadOutput);
    }
    let response: Response =
        serde_json::from_slice(output).map_err(|_| WorkerFailure::BadOutput)?;
    if response.protocol != PROTOCOL {
        return Err(WorkerFailure::ProtocolMismatch);
    }
    match response.outcome {
        Outcome::Ok { segments } => Ok(Ok(segments.into_iter().map(Into::into).collect())),
        Outcome::Unsupported { message } => Ok(Err(ExtractError::Unsupported(message))),
        Outcome::Failed { message } => Ok(Err(ExtractError::Failed(message))),
        Outcome::Io { message } => Ok(Err(ExtractError::Io(std::io::Error::other(message)))),
        Outcome::ProtocolMismatch => Err(WorkerFailure::ProtocolMismatch),
        Outcome::BadRequest => Err(WorkerFailure::BadOutput),
    }
}

/// A Windows worker that ended badly: the job's accounting says whether a limit did it.
#[cfg(windows)]
fn classify_windows(
    status: &ExitStatus,
    job: Option<&super::limits_windows::Job>,
    limits: WorkerLimits,
) -> WorkerFailure {
    match status.code() {
        Some(MEMORY_EXIT_CODE) => return WorkerFailure::MemoryLimit,
        Some(CPU_EXIT_CODE) => return WorkerFailure::CpuLimit,
        _ => {}
    }
    if let Some(job) = job {
        if job
            .peak_memory()
            .is_some_and(|(peak, limit)| limit > 0 && peak >= limit)
        {
            return WorkerFailure::MemoryLimit;
        }
        if job
            .cpu_time()
            .is_some_and(|used| used >= Duration::from_secs(limits.cpu_seconds))
        {
            return WorkerFailure::CpuLimit;
        }
    }
    WorkerFailure::Crashed
}

/// The worker's physical footprint (what Activity Monitor shows), macOS only.
#[cfg(target_os = "macos")]
fn footprint(pid: u32) -> Option<u64> {
    let mut info = std::mem::MaybeUninit::<libc::rusage_info_v2>::zeroed();
    let pid = i32::try_from(pid).ok()?;
    // SAFETY: `info` is large enough for RUSAGE_INFO_V2; the API takes it as `rusage_info_t *`.
    let ok = unsafe {
        libc::proc_pid_rusage(
            pid,
            libc::RUSAGE_INFO_V2,
            info.as_mut_ptr().cast::<libc::rusage_info_t>(),
        )
    } == 0;
    // SAFETY: filled in by the successful call above.
    ok.then(|| unsafe { info.assume_init() }.ri_phys_footprint)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    fn exited(code: i32) -> ExitStatus {
        use std::os::unix::process::ExitStatusExt;
        ExitStatus::from_raw(code << 8)
    }

    #[cfg(windows)]
    fn exited(code: i32) -> ExitStatus {
        use std::os::windows::process::ExitStatusExt;
        ExitStatus::from_raw(code as u32)
    }

    fn response(json: &str) -> Vec<u8> {
        json.as_bytes().to_vec()
    }

    #[test]
    fn exit_codes_and_answers_are_classified() {
        let ok = response(
            r#"{"protocol":1,"outcome":{"kind":"ok","segments":[{"locator":"p. 1","text":"Alpha"}]}}"#,
        );
        let segments = classify(exited(0), &ok).unwrap().unwrap();
        assert_eq!(segments[0].text, "Alpha");
        assert_eq!(segments[0].locator.as_deref(), Some("p. 1"));

        let unsupported =
            response(r#"{"protocol":1,"outcome":{"kind":"unsupported","message":"video"}}"#);
        assert!(matches!(
            classify(exited(0), &unsupported),
            Ok(Err(ExtractError::Unsupported(_)))
        ));
        let failures = [
            (
                exited(MEMORY_EXIT_CODE),
                Vec::new(),
                WorkerFailure::MemoryLimit,
            ),
            (exited(CPU_EXIT_CODE), Vec::new(), WorkerFailure::CpuLimit),
            (exited(101), Vec::new(), WorkerFailure::Crashed),
            (exited(2), Vec::new(), WorkerFailure::ProtocolMismatch),
            (exited(0), response("not json"), WorkerFailure::BadOutput),
            (
                exited(0),
                response(r#"{"protocol":2,"outcome":{"kind":"ok","segments":[]}}"#),
                WorkerFailure::ProtocolMismatch,
            ),
            (
                exited(0),
                response(r#"{"protocol":1,"outcome":{"kind":"protocol_mismatch"}}"#),
                WorkerFailure::ProtocolMismatch,
            ),
        ];
        for (status, output, expected) in failures {
            assert_eq!(
                classify(status, &output).unwrap_err(),
                expected,
                "{status:?}"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn signals_are_classified() {
        use std::os::unix::process::ExitStatusExt;
        assert_eq!(
            classify(ExitStatus::from_raw(libc::SIGXCPU), &[]).unwrap_err(),
            WorkerFailure::CpuLimit
        );
        assert_eq!(
            classify(ExitStatus::from_raw(libc::SIGSEGV), &[]).unwrap_err(),
            WorkerFailure::Crashed
        );
    }

    #[test]
    fn hard_failures_are_the_ones_the_file_causes() {
        let hard: Vec<_> = [
            WorkerFailure::TimedOut,
            WorkerFailure::CpuLimit,
            WorkerFailure::MemoryLimit,
            WorkerFailure::Crashed,
            WorkerFailure::BadOutput,
            WorkerFailure::SpawnFailed,
            WorkerFailure::ProtocolMismatch,
        ]
        .into_iter()
        .filter(|f| f.is_hard())
        .map(WorkerFailure::as_str)
        .collect();
        assert_eq!(hard, ["timed_out", "cpu_limit", "memory_limit", "crashed"]);
    }

    #[test]
    fn a_missing_worker_is_spawn_failed() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("notes.txt");
        std::fs::write(&file, "demo").unwrap();
        assert_eq!(
            extract_in_worker(
                &dir.path().join("no-such-pagelamp"),
                &file,
                None,
                WorkerLimits::default()
            )
            .unwrap_err(),
            WorkerFailure::SpawnFailed
        );
    }
}
