//! The worker side: `pagelamp extract-worker --protocol 1` (see the module docs of `worker`).

use std::io::{Read, Write};
use std::path::Path;

use super::protocol::{Outcome, Request, Response};
use super::{CPU_EXIT_CODE, PROTOCOL, WorkerLimits, alloc, exit_now};
use crate::ExtractError;

/// Largest request we read (a path and a few numbers).
const MAX_REQUEST_BYTES: u64 = 64 * 1024;

/// Run the worker: read one request from stdin, extract, write one response to stdout.
/// Returns the process exit code (0 whenever a response was written). `protocol` is the
/// version the parent asked for on the command line.
pub fn serve_stdio(protocol: u32) -> i32 {
    let respond = |outcome: Outcome| -> i32 {
        let response = Response {
            protocol: PROTOCOL,
            outcome,
        };
        let mut stdout = std::io::stdout().lock();
        match serde_json::to_writer(&mut stdout, &response)
            .and_then(|()| stdout.flush().map_err(serde_json::Error::io))
        {
            Ok(()) => 0,
            Err(_) => 1,
        }
    };
    if protocol != PROTOCOL {
        return respond(Outcome::ProtocolMismatch);
    }
    let mut input = String::new();
    if std::io::stdin()
        .take(MAX_REQUEST_BYTES)
        .read_to_string(&mut input)
        .is_err()
    {
        return respond(Outcome::BadRequest);
    }
    let Ok(request) = serde_json::from_str::<Request>(&input) else {
        return respond(Outcome::BadRequest);
    };
    if request.protocol != PROTOCOL {
        return respond(Outcome::ProtocolMismatch);
    }
    enforce(WorkerLimits {
        memory_bytes: request.memory_bytes,
        cpu_seconds: request.cpu_seconds,
    });
    #[cfg(debug_assertions)]
    if let Some(fault) = request.debug_fault.as_deref() {
        // Report the environment's variable names (so tests can see it is empty).
        if fault == "env" {
            let mut names: Vec<String> = std::env::vars_os()
                .map(|(name, _)| name.to_string_lossy().into_owned())
                .collect();
            names.sort();
            return respond(Outcome::Failed {
                message: names.join(","),
            });
        }
        // Report whether this process has a console window (Windows: `CREATE_NO_WINDOW`).
        if fault == "console" {
            return respond(Outcome::Failed {
                message: console_window().to_string(),
            });
        }
        run_fault(fault);
    }
    let outcome = match crate::extract_file(Path::new(&request.path), request.mime.as_deref()) {
        Ok(segments) => Outcome::Ok {
            segments: segments.into_iter().map(Into::into).collect(),
        },
        Err(ExtractError::Unsupported(message)) => Outcome::Unsupported { message },
        Err(ExtractError::Failed(message)) => Outcome::Failed { message },
        Err(ExtractError::Io(err)) => Outcome::Io {
            message: err.to_string(),
        },
    };
    respond(outcome)
}

/// Put this process under `limits` (see the module docs of `worker`).
fn enforce(limits: WorkerLimits) {
    alloc::set_cap(limits.memory_bytes);
    #[cfg(unix)]
    super::limits_unix::apply(limits);
    #[cfg(windows)]
    super::limits_windows::no_crash_dialogs();
    let budget = std::time::Duration::from_secs(limits.cpu_seconds);
    let _ = std::thread::Builder::new()
        .name("cpu-watchdog".into())
        .spawn(move || {
            loop {
                std::thread::sleep(std::time::Duration::from_millis(100));
                if process_cpu_time().is_some_and(|used| used > budget) {
                    exit_now(CPU_EXIT_CODE);
                }
            }
        });
}

/// CPU time this process has used (user + system), if the platform tells us.
pub(crate) fn process_cpu_time() -> Option<std::time::Duration> {
    #[cfg(unix)]
    {
        super::limits_unix::process_cpu_time()
    }
    #[cfg(windows)]
    {
        super::limits_windows::process_cpu_time()
    }
    #[cfg(not(any(unix, windows)))]
    {
        None
    }
}

/// Debug builds only: "window" when this process has a console window, else "no window".
#[cfg(debug_assertions)]
fn console_window() -> &'static str {
    #[cfg(windows)]
    let window = super::limits_windows::has_console_window();
    #[cfg(not(windows))]
    let window = false;
    if window { "window" } else { "no window" }
}

/// Debug builds only: misbehave on purpose, so tests can check how the parent classifies it.
#[cfg(debug_assertions)]
fn run_fault(fault: &str) {
    match fault {
        // Never answer: the parent's wall clock ends it (`timed_out`).
        "hang" => std::thread::sleep(std::time::Duration::from_secs(3600)),
        // Burn CPU: the watchdog ends it (`cpu_limit`).
        "spin" => {
            let mut x: u64 = 0;
            loop {
                x = std::hint::black_box(x.wrapping_mul(31).wrapping_add(7));
            }
        }
        // Allocate past the cap: the allocator ends it (`memory_limit`).
        "allocate" => {
            let mut blocks: Vec<Vec<u8>> = Vec::new();
            loop {
                blocks.push(vec![1u8; 16 * 1024 * 1024]);
                std::hint::black_box(&blocks);
            }
        }
        // Die (`crashed`).
        "abort" => std::process::abort(),
        // Answer nonsense (`bad_output`).
        "garbage" => {
            let _ = std::io::stdout().write_all(b"this is not json");
            exit_now(0);
        }
        _ => {}
    }
}
