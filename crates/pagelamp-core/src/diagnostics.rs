//! Local diagnostics for beta testers: log files, redaction, crash capture.
//!
//! - Log files live ONLY under `<data_dir>/logs/` and never leave the device (a student can
//!   choose to paste a `pagelamp report` into an issue after reading it):
//!   `app-YYYY-MM-DD.log` (CLI, desktop app, sync) and `mcp-YYYY-MM-DD.log` (MCP server
//!   processes; several may append at once, so every line carries the pid). One file per day,
//!   kept `RETENTION_DAYS` days, each capped at `MAX_FILE_BYTES`.
//! - Levels: file `info`, stderr `warn`; `PAGELAMP_LOG=debug` (or `pagelamp … -v`) raises both
//!   for PageLamp's own targets (e.g. the Canvas request log `pagelamp_canvas::http`). Other
//!   crates stay at `warn`. `RUST_LOG` (developers) overrides everything.
//! - Redaction by construction (nothing logs secrets or course text; course codes/titles
//!   only at debug) PLUS a final safety filter on every line (`redact`): bearer tokens,
//!   Canvas token patterns, secret-looking query values, calendar-feed URLs, the home dir.
//! - A panic hook logs message + location + backtrace and writes `logs/last-crash.json`.
//!   Parser panics that extraction catches on purpose are not crashes and are skipped.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, OnceLock};

use chrono::{Local, NaiveDate, TimeDelta, Utc};
use regex::Regex;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer};

use crate::model::Timestamp;
use crate::paths;
use crate::{Error, Result};

/// Days of log files kept.
pub const RETENTION_DAYS: i64 = 7;
/// Size cap of one log file; further lines that day are dropped (one marker line says so).
pub const MAX_FILE_BYTES: u64 = 10 * 1024 * 1024;
const CRASH_FILE: &str = "last-crash.json";
const MAX_CRASH_MESSAGE_CHARS: usize = 500;

/// Which kind of process writes the log (selects the file name).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProcessKind {
    /// CLI commands, the desktop app and syncs.
    App,
    /// `pagelamp mcp` server processes started by AI apps.
    Mcp,
}

impl ProcessKind {
    fn prefix(self) -> &'static str {
        match self {
            ProcessKind::App => "app",
            ProcessKind::Mcp => "mcp",
        }
    }
}

/// The most recent crash (`logs/last-crash.json`), until cleared.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CrashReport {
    pub time: Timestamp,
    pub version: String,
    pub process: ProcessKind,
    /// Redacted, at most 500 characters.
    pub message: String,
    /// `file.rs:line:column` of the panic.
    pub location: Option<String>,
}

/// `<data_dir>/logs`
pub fn logs_dir_in(data_dir: &Path) -> PathBuf {
    data_dir.join("logs")
}

// ---------------------------------------------------------------------------------------------
// Redaction
// ---------------------------------------------------------------------------------------------

static BEARER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\bbearer\s+[A-Za-z0-9._~+/=-]+").expect("valid regex"));
/// Canvas access tokens look like `1234~AbCd…` (numeric id, tilde, long random part).
static CANVAS_TOKEN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b\d{1,8}~[A-Za-z0-9]{16,}").expect("valid regex"));
static SECRET_QUERY: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)\b(access_token|verifier|sig|signature|token|x-amz-[a-z0-9-]+|key-pair-id|policy|client_secret)=[^&\s'<>]+",
    )
    .expect("valid regex")
});
static SECRET_ASSIGNMENT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)\b(password|secret|api[_-]?key|authorization)\s*[:=]\s*["']?[^\s"',;]+"#)
        .expect("valid regex")
});
/// Calendar feed URLs embed a private token in the path: keep the host only.
static FEED_URL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)\b(?:webcal|https?)://([^/\s"'<>]+)/[^\s"'<>]*(?:\.ics|/feeds?/|/calendar)[^\s"'<>]*"#)
        .expect("valid regex")
});

/// The final safety filter applied to every log line (and to reports): removes secrets that
/// should never have been logged in the first place, and shortens the home dir to `~`.
pub fn redact(text: &str) -> String {
    let text = BEARER.replace_all(text, "Bearer <redacted>");
    let text = CANVAS_TOKEN.replace_all(&text, "<redacted-token>");
    let text = FEED_URL.replace_all(&text, "https://$1/…");
    let text = SECRET_QUERY.replace_all(&text, "$1=<redacted>");
    let text = SECRET_ASSIGNMENT.replace_all(&text, "$1=<redacted>");
    shorten_home(&text)
}

/// Replace the user's home directory with `~` (it contains the user name).
pub fn shorten_home(text: &str) -> String {
    match home_dir() {
        Some(home) if home.len() > 1 && text.contains(&home) => text.replace(&home, "~"),
        _ => text.to_string(),
    }
}

fn home_dir() -> Option<String> {
    static HOME: OnceLock<Option<String>> = OnceLock::new();
    HOME.get_or_init(|| {
        std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .map(|h| {
                h.to_string_lossy()
                    .trim_end_matches(['/', '\\'])
                    .to_string()
            })
            .filter(|h| !h.is_empty())
    })
    .clone()
}

// ---------------------------------------------------------------------------------------------
// Log files
// ---------------------------------------------------------------------------------------------

/// Appends redacted lines to `<logs>/<kind>-<date>.log`. The file is opened per event (in
/// append mode), which keeps rotation trivial and lets several processes share a file.
struct LogFiles {
    dir: PathBuf,
    kind: ProcessKind,
    pid: u32,
}

impl LogFiles {
    fn path_for(&self, day: NaiveDate) -> PathBuf {
        self.dir.join(format!(
            "{}-{}.log",
            self.kind.prefix(),
            day.format("%Y-%m-%d")
        ))
    }

    fn append(&self, text: &str) {
        let path = self.path_for(Local::now().date_naive());
        let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        if size >= MAX_FILE_BYTES {
            return;
        }
        let mut text = with_pid(&redact(text), self.pid);
        if size + text.len() as u64 > MAX_FILE_BYTES {
            text = format!(
                "{} pid={} WARN the log file reached its size limit; further lines are dropped until tomorrow\n",
                Utc::now().to_rfc3339(),
                self.pid
            );
        }
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
        {
            // One write call per event: lines of concurrent processes don't interleave.
            let _ = file.write_all(text.as_bytes());
        }
    }

    /// Delete this kind's log files older than `RETENTION_DAYS`.
    fn prune(&self) {
        let oldest = Local::now().date_naive() - TimeDelta::days(RETENTION_DAYS - 1);
        let prefix = format!("{}-", self.kind.prefix());
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            let date = name
                .strip_prefix(&prefix)
                .and_then(|rest| rest.strip_suffix(".log"))
                .and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok());
            if date.is_some_and(|d| d < oldest) {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }
}

/// Insert `pid=N` after the leading timestamp of the first line.
fn with_pid(text: &str, pid: u32) -> String {
    match text.split_once(' ') {
        Some((time, rest)) => format!("{time} pid={pid} {rest}"),
        None => format!("pid={pid} {text}"),
    }
}

/// Where one formatted event goes.
#[derive(Clone)]
enum Sink {
    File(Arc<LogFiles>),
    Stderr,
}

/// Collects one formatted event, then redacts and writes it when dropped.
struct EventBuffer {
    buf: Vec<u8>,
    sink: Sink,
}

impl Write for EventBuffer {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.buf.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Drop for EventBuffer {
    fn drop(&mut self) {
        if self.buf.is_empty() {
            return;
        }
        let text = String::from_utf8_lossy(&self.buf);
        match &self.sink {
            Sink::File(files) => files.append(&text),
            Sink::Stderr => {
                let _ = std::io::stderr().write_all(redact(&text).as_bytes());
            }
        }
    }
}

#[derive(Clone)]
struct SinkWriter(Sink);

impl<'a> MakeWriter<'a> for SinkWriter {
    type Writer = EventBuffer;
    fn make_writer(&'a self) -> EventBuffer {
        EventBuffer {
            buf: Vec::new(),
            sink: self.0.clone(),
        }
    }
}

/// Filter levels: `(file, stderr)` directives.
fn levels(verbose: bool) -> (String, String) {
    if let Ok(developer) = std::env::var("RUST_LOG")
        && !developer.trim().is_empty()
    {
        return (developer.clone(), developer);
    }
    let requested = std::env::var(paths::LOG_ENV)
        .ok()
        .map(|v| v.trim().to_ascii_lowercase())
        .filter(|v| matches!(v.as_str(), "error" | "warn" | "info" | "debug" | "trace"));
    let (file, stderr) = match (verbose, requested.as_deref()) {
        (true, _) | (_, Some("debug")) => ("debug", "debug"),
        (_, Some("trace")) => ("trace", "trace"),
        (_, Some(level)) => (level, "warn"),
        (false, None) => ("info", "warn"),
    };
    (
        format!("warn,pagelamp={file}"),
        format!("warn,pagelamp={stderr}"),
    )
}

/// Set up logging (log files under `<data_dir>/logs` when a data dir is known, redacted
/// stderr) and the panic hook. Safe to call more than once; only the first call counts.
pub fn init(data_dir: Option<&Path>, kind: ProcessKind, verbose: bool) {
    static DONE: OnceLock<()> = OnceLock::new();
    if DONE.set(()).is_err() {
        return;
    }
    let (file_level, stderr_level) = levels(verbose);
    let files = data_dir.and_then(|dir| {
        let logs = logs_dir_in(dir);
        std::fs::create_dir_all(&logs).ok()?;
        Some(Arc::new(LogFiles {
            dir: logs,
            kind,
            pid: std::process::id(),
        }))
    });
    let file_layer = files.clone().map(|files| {
        tracing_subscriber::fmt::layer()
            .with_ansi(false)
            .with_writer(SinkWriter(Sink::File(files)))
            .with_filter(EnvFilter::new(&file_level))
    });
    let stderr_layer = tracing_subscriber::fmt::layer()
        .with_ansi(false)
        .with_writer(SinkWriter(Sink::Stderr))
        .with_filter(EnvFilter::new(&stderr_level));
    let _ = tracing_subscriber::registry()
        .with(file_layer)
        .with(stderr_layer)
        .try_init();
    if let Some(files) = &files {
        files.prune();
    }
    install_panic_hook(data_dir.map(Path::to_path_buf), kind);
}

// ---------------------------------------------------------------------------------------------
// Crashes
// ---------------------------------------------------------------------------------------------

thread_local! {
    /// How many `expect_panics` scopes this thread is inside.
    static EXPECTING: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

/// Run `f` in a scope whose panics are caught by the caller on purpose (e.g. with
/// `catch_unwind`): such panics are logged, but not reported as a crash. Nests.
pub fn expect_panics<T>(f: impl FnOnce() -> T) -> T {
    struct Scope;
    impl Drop for Scope {
        fn drop(&mut self) {
            EXPECTING.with(|d| d.set(d.get().saturating_sub(1)));
        }
    }
    EXPECTING.with(|d| d.set(d.get() + 1));
    let _scope = Scope;
    f()
}

/// A future whose polls run inside `expect_panics` — for tasks whose panics the caller
/// catches at the `JoinHandle` (on another thread, where a plain scope wouldn't be seen).
pub fn expect_panics_in<F: std::future::Future>(future: F) -> ExpectPanics<F> {
    ExpectPanics {
        future: Box::pin(future),
    }
}

/// See `expect_panics_in`.
pub struct ExpectPanics<F> {
    future: std::pin::Pin<Box<F>>,
}

impl<F: std::future::Future> std::future::Future for ExpectPanics<F> {
    type Output = F::Output;
    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<F::Output> {
        let future = self.future.as_mut();
        expect_panics(|| future.poll(cx))
    }
}

/// Whether a panic on this thread right now is caught on purpose (an `expect_panics` scope,
/// or text extraction's own parser guard).
fn panic_is_expected() -> bool {
    EXPECTING.with(|d| d.get() > 0) || pagelamp_extract::panic_is_expected()
}

/// Log every panic (message + location; backtrace for real crashes). A real crash — one no
/// caller catches on purpose — also records `logs/last-crash.json` and runs the previous hook
/// (which prints it to stderr). Expected panics (see `expect_panics`, text extraction) are
/// logged as warnings only, so a bad PDF never shows up as "PageLamp crashed".
pub fn install_panic_hook(data_dir: Option<PathBuf>, kind: ProcessKind) {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let message = redact(&panic_message(info));
        let location = info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()));
        let at = location.as_deref().unwrap_or("?");
        if panic_is_expected() {
            tracing::warn!(target: "pagelamp::panic", "handled panic at {at}: {message}");
            return;
        }
        let backtrace = std::backtrace::Backtrace::force_capture();
        tracing::error!(target: "pagelamp::panic", "crash at {at}: {message}\n{backtrace}");
        if let Some(dir) = &data_dir {
            let report = CrashReport {
                time: Utc::now(),
                version: env!("CARGO_PKG_VERSION").to_string(),
                process: kind,
                message: message.chars().take(MAX_CRASH_MESSAGE_CHARS).collect(),
                location,
            };
            let _ = write_crash(dir, &report);
        }
        previous(info);
    }));
}

fn panic_message(info: &std::panic::PanicHookInfo<'_>) -> String {
    let payload = info.payload();
    payload
        .downcast_ref::<&str>()
        .map(|s| s.to_string())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "unknown panic".to_string())
}

fn crash_path(data_dir: &Path) -> PathBuf {
    logs_dir_in(data_dir).join(CRASH_FILE)
}

fn write_crash(data_dir: &Path, report: &CrashReport) -> Result<()> {
    std::fs::create_dir_all(logs_dir_in(data_dir))?;
    std::fs::write(crash_path(data_dir), serde_json::to_vec_pretty(report)?)?;
    Ok(())
}

/// The last recorded crash, if any (a damaged file counts as none).
pub fn last_crash(data_dir: &Path) -> Result<Option<CrashReport>> {
    match std::fs::read(crash_path(data_dir)) {
        Ok(bytes) => Ok(serde_json::from_slice(&bytes).ok()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(Error::Io(err)),
    }
}

pub fn clear_last_crash(data_dir: &Path) -> Result<()> {
    match std::fs::remove_file(crash_path(data_dir)) {
        Err(err) if err.kind() != std::io::ErrorKind::NotFound => Err(Error::Io(err)),
        _ => Ok(()),
    }
}

/// The last `max` log lines of both kinds (newest last), redacted again for safety. Lines are
/// ordered by their leading RFC 3339 timestamp; continuation lines (backtraces) stay with
/// their event.
pub fn recent_log_lines(data_dir: &Path, max: usize) -> Vec<String> {
    let dir = logs_dir_in(data_dir);
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "log"))
        .collect();
    files.sort();
    // Events: (timestamp, text); a line not starting with a digit continues the last event.
    let mut events: Vec<(String, String)> = Vec::new();
    for file in files.iter().rev().take(4) {
        let Ok(text) = std::fs::read_to_string(file) else {
            continue;
        };
        for line in text.lines() {
            if line.starts_with(|c: char| c.is_ascii_digit()) {
                let stamp = line.split(' ').next().unwrap_or("").to_string();
                events.push((stamp, line.to_string()));
            } else if let Some(last) = events.last_mut() {
                last.1.push('\n');
                last.1.push_str(line);
            }
        }
    }
    events.sort_by(|a, b| a.0.cmp(&b.0));
    let lines: Vec<String> = events.into_iter().map(|(_, text)| redact(&text)).collect();
    let skip = lines.len().saturating_sub(max);
    lines.into_iter().skip(skip).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redaction_removes_planted_secrets() {
        let planted = [
            (
                "Authorization: Bearer 1234~AbCdEfGhIjKlMnOpQrStUvWxYz0123456789",
                "1234~AbCd",
            ),
            (
                "token 7~abcdefghijklmnopqrstuvwxyz0123 in text",
                "7~abcdefghij",
            ),
            (
                "GET /files/1/download?download_frd=1&verifier=SeCrEtVeRiFiEr",
                "SeCrEtVeRiFiEr",
            ),
            (
                "https://bucket.example.com/x?X-Amz-Signature=deadbeef&X-Amz-Credential=AKIA",
                "deadbeef",
            ),
            (
                "https://calendar.example.edu/feeds/calendars/user_Pr1v4teFeedTok3n.ics",
                "Pr1v4te",
            ),
            (
                "webcal://calendar.example.edu/private-XYZSECRET/basic.ics",
                "XYZSECRET",
            ),
            ("?access_token=abc123secret&page=2", "abc123secret"),
            ("password=hunter2 and api_key: sk-demo", "hunter2"),
        ];
        for (line, secret) in planted {
            let redacted = redact(line);
            assert!(!redacted.contains(secret), "{line:?} → {redacted:?}");
        }
        assert_eq!(
            redact("https://calendar.example.edu/feeds/calendars/user_x.ics"),
            "https://calendar.example.edu/…"
        );
        assert_eq!(redact("page=2&per_page=100"), "page=2&per_page=100");
    }

    #[test]
    fn home_dir_is_shortened() {
        if let Some(home) = home_dir() {
            assert_eq!(
                shorten_home(&format!("{home}/Courses/x.pdf")),
                "~/Courses/x.pdf"
            );
        }
    }

    #[test]
    fn log_files_append_redact_cap_and_prune() {
        let temp = tempfile::tempdir().unwrap();
        let files = LogFiles {
            dir: temp.path().to_path_buf(),
            kind: ProcessKind::Mcp,
            pid: 4242,
        };
        files.append("2026-09-26T10:00:00Z INFO pagelamp: token 1234~AbCdEfGhIjKlMnOpQrStUv\n");
        let path = files.path_for(Local::now().date_naive());
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(
            text.starts_with("2026-09-26T10:00:00Z pid=4242 INFO"),
            "{text}"
        );
        assert!(!text.contains("AbCdEfGh"));

        // Size cap: a file at the cap gets nothing more.
        std::fs::write(&path, vec![b'x'; MAX_FILE_BYTES as usize]).unwrap();
        files.append("2026-09-26T10:00:01Z INFO pagelamp: more\n");
        assert_eq!(std::fs::metadata(&path).unwrap().len(), MAX_FILE_BYTES);

        // Retention: old files go, recent ones and other kinds stay.
        let old = temp.path().join("mcp-2000-01-01.log");
        let other_kind = temp.path().join("app-2000-01-01.log");
        std::fs::write(&old, "old").unwrap();
        std::fs::write(&other_kind, "old").unwrap();
        files.prune();
        assert!(!old.exists() && other_kind.exists() && path.exists());
    }

    #[test]
    fn expect_panics_scopes_nest_and_unwind() {
        assert!(!panic_is_expected());
        expect_panics(|| {
            assert!(panic_is_expected());
            expect_panics(|| assert!(panic_is_expected()));
            assert!(panic_is_expected());
        });
        assert!(!panic_is_expected());
        // The scope is left even when the closure panics.
        let caught = std::panic::catch_unwind(|| expect_panics(|| panic!("demo")));
        assert!(caught.is_err());
        assert!(!panic_is_expected());
    }

    #[test]
    fn crash_marker_round_trip() {
        let temp = tempfile::tempdir().unwrap();
        assert_eq!(last_crash(temp.path()).unwrap(), None);
        let report = CrashReport {
            time: Utc::now(),
            version: "0.1.0".into(),
            process: ProcessKind::App,
            message: "boom".into(),
            location: Some("src/x.rs:1:1".into()),
        };
        write_crash(temp.path(), &report).unwrap();
        assert_eq!(last_crash(temp.path()).unwrap(), Some(report));
        clear_last_crash(temp.path()).unwrap();
        clear_last_crash(temp.path()).unwrap();
        assert_eq!(last_crash(temp.path()).unwrap(), None);
    }

    #[test]
    fn recent_lines_merge_files_by_time_and_keep_continuations() {
        let temp = tempfile::tempdir().unwrap();
        let logs = logs_dir_in(temp.path());
        std::fs::create_dir_all(&logs).unwrap();
        std::fs::write(
            logs.join("app-2026-09-26.log"),
            "2026-09-26T10:00:00Z pid=1 INFO a\n2026-09-26T10:00:02Z pid=1 ERROR crash\n  at frame 1\n",
        )
        .unwrap();
        std::fs::write(
            logs.join("mcp-2026-09-26.log"),
            "2026-09-26T10:00:01Z pid=2 INFO b\n",
        )
        .unwrap();
        let lines = recent_log_lines(temp.path(), 2);
        assert_eq!(lines.len(), 2);
        assert!(lines[0].ends_with("INFO b"));
        assert!(lines[1].contains("ERROR crash\n  at frame 1"));
    }
}
