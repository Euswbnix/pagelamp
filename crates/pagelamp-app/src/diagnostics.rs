//! Diagnostics for beta testers: logging setup, crash records, `doctor` and the shareable
//! report. The free functions resolve the default data dir themselves, so they also work
//! when `App::open()` failed (a locked or damaged database) — exactly when a report is
//! needed. `App` has methods of the same names for an explicit data dir.
//!
//! Nothing here sends anything anywhere: the report is text the student reads and may
//! paste into an issue. It never contains tokens, feed URLs, course material text, course
//! codes or names (pseudonymised as "Course 1", "Course 2", …), or the user name in paths.

use std::path::{Path, PathBuf};

use pagelamp_core::brand;
use pagelamp_core::diagnostics as core_diag;
use pagelamp_core::model::{SourceErrorKind, SourceKind, Timestamp};
use pagelamp_core::paths;
use pagelamp_core::secrets::{KeychainSecrets, SecretBackend};
use pagelamp_core::store::Store;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub use pagelamp_core::diagnostics::{
    CrashReport, ExpectPanics, ProcessKind, expect_panics, expect_panics_in,
};

use crate::{AppError, AppErrorKind, Result};

/// Log lines included in a report.
const REPORT_LOG_LINES: usize = 200;
const MAX_UI_MESSAGE_CHARS: usize = 2_000;
const MAX_UI_STACK_CHARS: usize = 8_000;

/// A source as `doctor` shows it: no ids, URLs or labels.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct DoctorSource {
    pub kind: SourceKind,
    /// The last sync succeeded (false when it failed or never ran).
    pub ok: bool,
    pub last_synced_at: Option<Timestamp>,
    pub last_error_kind: Option<SourceErrorKind>,
}

/// Whether each AI app's config has a PageLamp entry (presence only; nothing else is read
/// out of those files).
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct McpClientPresence {
    pub claude_desktop: bool,
    pub claude_code: bool,
    pub codex: bool,
}

/// `pagelamp doctor`: the facts a maintainer needs to help, and nothing personal.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct DoctorReport {
    pub version: String,
    pub os: String,
    pub arch: String,
    /// With the home directory shortened to `~`.
    pub data_dir: String,
    pub logs_dir: String,
    pub schema_version: Option<i64>,
    /// Why the database could not be read (then the counts are 0).
    pub database_error: Option<String>,
    pub keychain_available: bool,
    pub keychain_error: Option<String>,
    pub sources: Vec<DoctorSource>,
    pub courses: u32,
    pub hidden_courses: u32,
    pub materials: u32,
    pub events: u32,
    pub mcp_clients: McpClientPresence,
    pub last_crash: Option<CrashReport>,
}

fn data_dir() -> Result<PathBuf> {
    Ok(paths::data_dir()?)
}

/// Set up log files, redacted stderr logging and the panic hook for this process. Call once,
/// first thing (before `App::open`). `verbose` = `PAGELAMP_LOG=debug`.
pub fn init(kind: ProcessKind, verbose: bool) {
    let dir = paths::data_dir().ok();
    core_diag::init(dir.as_deref(), kind, verbose);
}

/// `<default data dir>/logs` (created if missing).
pub fn logs_dir() -> Result<PathBuf> {
    logs_dir_in(&data_dir()?)
}

pub fn last_crash() -> Result<Option<CrashReport>> {
    Ok(core_diag::last_crash(&data_dir()?)?)
}

pub fn clear_last_crash() -> Result<()> {
    Ok(core_diag::clear_last_crash(&data_dir()?)?)
}

pub fn doctor() -> Result<DoctorReport> {
    Ok(doctor_in(&data_dir()?, &KeychainSecrets))
}

/// Markdown for an issue: doctor + last crash + the last ~200 log lines, redacted and with
/// course codes/names pseudonymised.
pub fn diagnostic_report() -> Result<String> {
    Ok(report_in(&data_dir()?, &KeychainSecrets))
}

/// One ERROR line from the desktop UI (message + optional stack, capped and redacted).
pub fn log_ui_error(message: &str, stack: Option<&str>) {
    let message: String = message.chars().take(MAX_UI_MESSAGE_CHARS).collect();
    match stack {
        Some(stack) => {
            let stack: String = stack.chars().take(MAX_UI_STACK_CHARS).collect();
            tracing::error!(target: "pagelamp::ui", "{message}\n{stack}");
        }
        None => tracing::error!(target: "pagelamp::ui", "{message}"),
    }
}

pub(crate) fn logs_dir_in(data_dir: &Path) -> Result<PathBuf> {
    let dir = core_diag::logs_dir_in(data_dir);
    std::fs::create_dir_all(&dir).map_err(|err| {
        AppError::new(
            AppErrorKind::Internal,
            format!("could not create the logs folder: {err}"),
        )
    })?;
    Ok(dir)
}

pub(crate) fn doctor_in(data_dir: &Path, secrets: &dyn SecretBackend) -> DoctorReport {
    let keychain = secrets.check();
    let mut report = DoctorReport {
        version: env!("CARGO_PKG_VERSION").to_string(),
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        data_dir: core_diag::shorten_home(&data_dir.display().to_string()),
        logs_dir: core_diag::shorten_home(&core_diag::logs_dir_in(data_dir).display().to_string()),
        schema_version: None,
        database_error: None,
        keychain_available: keychain.is_ok(),
        keychain_error: keychain.err().map(|e| core_diag::redact(&e)),
        sources: Vec::new(),
        courses: 0,
        hidden_courses: 0,
        materials: 0,
        events: 0,
        mcp_clients: mcp_client_presence(),
        last_crash: core_diag::last_crash(data_dir).ok().flatten(),
    };
    let db = paths::db_path_in(data_dir);
    let read = Store::open_read_only(&db).and_then(|store| {
        Ok((
            store.schema_version()?,
            store.counts()?,
            store.list_sources()?,
        ))
    });
    match read {
        Ok((version, counts, sources)) => {
            report.schema_version = Some(version);
            report.courses = counts.courses;
            report.hidden_courses = counts.hidden_courses;
            report.materials = counts.materials;
            report.events = counts.events;
            report.sources = sources
                .into_iter()
                .map(|s| DoctorSource {
                    kind: s.kind,
                    ok: s.last_synced_at.is_some() && s.last_error.is_none(),
                    last_synced_at: s.last_synced_at,
                    last_error_kind: s.last_error_kind,
                })
                .collect();
        }
        Err(err) => report.database_error = Some(core_diag::redact(&err.to_string())),
    }
    report
}

pub(crate) fn report_in(data_dir: &Path, secrets: &dyn SecretBackend) -> String {
    let doctor = doctor_in(data_dir, secrets);
    let names = course_names(data_dir);
    let yes = |b: bool| if b { "yes" } else { "no" };
    let mut out = String::new();
    out.push_str(&format!("# {} diagnostic report\n\n", brand::PRODUCT_NAME));
    out.push_str(&format!(
        "Generated {}. Please read it before sharing: it contains no tokens, calendar-feed \
         links, course material text or course names, but check anyway.\n\n",
        chrono::Utc::now().format("%Y-%m-%d %H:%M UTC")
    ));
    out.push_str("## System\n\n");
    out.push_str(&format!("- Version: {}\n", doctor.version));
    out.push_str(&format!("- OS: {} ({})\n", doctor.os, doctor.arch));
    out.push_str(&format!("- Data folder: {}\n", doctor.data_dir));
    match (&doctor.schema_version, &doctor.database_error) {
        (Some(v), _) => out.push_str(&format!("- Database: schema {v}\n")),
        (None, Some(err)) => out.push_str(&format!("- Database: not readable ({err})\n")),
        (None, None) => out.push_str("- Database: unknown\n"),
    }
    match &doctor.keychain_error {
        None => out.push_str("- Keychain: available\n"),
        Some(err) => out.push_str(&format!("- Keychain: NOT available ({err})\n")),
    }
    out.push_str(&format!(
        "- Courses: {} ({} hidden) · materials: {} · events: {}\n",
        doctor.courses, doctor.hidden_courses, doctor.materials, doctor.events
    ));
    out.push_str(&format!(
        "- AI apps configured: Claude Desktop {} · Claude Code {} · Codex {}\n",
        yes(doctor.mcp_clients.claude_desktop),
        yes(doctor.mcp_clients.claude_code),
        yes(doctor.mcp_clients.codex)
    ));
    out.push_str("\n## Sources\n\n");
    if doctor.sources.is_empty() {
        out.push_str("- none\n");
    }
    for source in &doctor.sources {
        out.push_str(&format!(
            "- {}: {}{}{}\n",
            source.kind.as_str(),
            if source.ok { "ok" } else { "not ok" },
            source
                .last_synced_at
                .map(|t| format!(", last synced {}", t.format("%Y-%m-%d %H:%M UTC")))
                .unwrap_or_default(),
            source
                .last_error_kind
                .map(|k| format!(", last error: {}", k.as_str()))
                .unwrap_or_default(),
        ));
    }
    out.push_str("\n## Last crash\n\n");
    match &doctor.last_crash {
        None => out.push_str("none\n"),
        Some(crash) => out.push_str(&format!(
            "{} · version {} · {} process · {}{}\n",
            crash.time.format("%Y-%m-%d %H:%M UTC"),
            crash.version,
            match crash.process {
                ProcessKind::App => "app",
                ProcessKind::Mcp => "MCP",
            },
            pseudonymise(&crash.message, &names),
            crash
                .location
                .as_deref()
                .map(|l| format!(" (at {l})"))
                .unwrap_or_default()
        )),
    }
    out.push_str(&format!(
        "\n## Recent log (last {REPORT_LOG_LINES} lines)\n\n```text\n"
    ));
    let lines = core_diag::recent_log_lines(data_dir, REPORT_LOG_LINES);
    if lines.is_empty() {
        out.push_str("(no log lines yet)\n");
    }
    for line in lines {
        // Keep the fence intact whatever the line contains.
        out.push_str(&pseudonymise(&line, &names).replace("```", "'''"));
        out.push('\n');
    }
    out.push_str("```\n");
    core_diag::redact(&out)
}

/// Course codes and names, ordered, for pseudonymisation ("Course 1", "Course 2", …).
fn course_names(data_dir: &Path) -> Vec<(String, String)> {
    let Ok(store) = Store::open_read_only(&paths::db_path_in(data_dir)) else {
        return Vec::new();
    };
    let Ok(courses) = store.list_courses(true) else {
        return Vec::new();
    };
    let mut pairs = Vec::new();
    for (index, course) in courses.iter().enumerate() {
        let alias = format!("Course {}", index + 1);
        pairs.push((course.display_name(), alias.clone()));
        pairs.push((course.name.clone(), alias.clone()));
        if let Some(code) = &course.code {
            pairs.push((code.clone(), alias));
        }
    }
    // Longest first, so "DEMO101 — Intro" is replaced before "DEMO101".
    pairs.retain(|(name, _)| name.trim().len() >= 3);
    pairs.sort_by_key(|pair| std::cmp::Reverse(pair.0.len()));
    pairs
}

fn pseudonymise(text: &str, names: &[(String, String)]) -> String {
    let mut text = text.to_string();
    for (name, alias) in names {
        if text.contains(name.as_str()) {
            text = text.replace(name.as_str(), alias);
        }
    }
    text
}

// ----- AI app config presence ------------------------------------------------------------------

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

/// Whether each AI app's config file mentions a `pagelamp` server entry. Only the presence
/// of that key is checked; nothing else from these files is kept or shown.
fn mcp_client_presence() -> McpClientPresence {
    let key = brand::MCP_SERVER_KEY;
    let json_has_server = |path: PathBuf, check_projects: bool| -> bool {
        let Ok(text) = std::fs::read_to_string(path) else {
            return false;
        };
        let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) else {
            return false;
        };
        let top = json.get("mcpServers").and_then(|s| s.get(key)).is_some();
        let in_project = check_projects
            && json
                .get("projects")
                .and_then(|p| p.as_object())
                .is_some_and(|projects| {
                    projects
                        .values()
                        .any(|p| p.get("mcpServers").and_then(|s| s.get(key)).is_some())
                });
        top || in_project
    };
    let desktop_config = if cfg!(target_os = "macos") {
        home().map(|h| h.join("Library/Application Support/Claude/claude_desktop_config.json"))
    } else if cfg!(windows) {
        std::env::var_os("APPDATA").map(|a| {
            PathBuf::from(a)
                .join("Claude")
                .join("claude_desktop_config.json")
        })
    } else {
        home().map(|h| h.join(".config/Claude/claude_desktop_config.json"))
    };
    let codex_config = std::env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .or_else(|| home().map(|h| h.join(".codex")))
        .map(|dir| dir.join("config.toml"));
    McpClientPresence {
        claude_desktop: desktop_config.is_some_and(|p| json_has_server(p, false)),
        claude_code: home().is_some_and(|h| json_has_server(h.join(".claude.json"), true)),
        codex: codex_config.is_some_and(|p| {
            std::fs::read_to_string(p).is_ok_and(|text| {
                text.lines().any(|line| {
                    let line = line.trim();
                    line == format!("[mcp_servers.{key}]")
                        || line == format!("[mcp_servers.\"{key}\"]")
                })
            })
        }),
    }
}

#[cfg(test)]
mod tests {
    use pagelamp_core::model::*;
    use pagelamp_core::secrets::MemorySecrets;
    use serde_json::json;

    use super::*;

    fn seeded(dir: &Path) {
        let store = Store::open(&paths::db_path_in(dir)).unwrap();
        store
            .upsert_source(&SourceRecord {
                id: "ical:secretid".into(),
                kind: SourceKind::Ical,
                label: "Demo calendar https://calendar.example.edu/feeds/x.ics".into(),
                config: json!({}),
                last_synced_at: None,
                last_error: None,
                last_error_kind: None,
            })
            .unwrap();
        store
            .record_sync(
                "ical:secretid",
                chrono::Utc::now(),
                Some((SourceErrorKind::AuthExpiredOrRevoked, "feed rejected")),
            )
            .unwrap();
        let folder = SourceRecord {
            id: "folder:x".into(),
            kind: SourceKind::Folder,
            label: "Courses".into(),
            config: json!({}),
            last_synced_at: None,
            last_error: None,
            last_error_kind: None,
        };
        store.upsert_source(&folder).unwrap();
        store
            .upsert_course(&CourseUpsert {
                id: "folder:x/course/DEMO101".into(),
                source_id: "folder:x".into(),
                external_id: "DEMO101".into(),
                code: Some("DEMO101".into()),
                name: "Intro to Demo Studies".into(),
                term_start: None,
                term_end: None,
                url: None,
                syllabus_text: None,
            })
            .unwrap();
    }

    #[test]
    fn doctor_reports_facts_but_no_urls_ids_or_labels() {
        let temp = tempfile::tempdir().unwrap();
        seeded(temp.path());
        let doctor = doctor_in(temp.path(), &MemorySecrets::new());
        assert_eq!(
            doctor.schema_version,
            Some(pagelamp_core::store::SCHEMA_VERSION)
        );
        assert!(doctor.keychain_available);
        assert_eq!(doctor.courses, 1);
        assert_eq!(doctor.sources.len(), 2);
        let ical = doctor
            .sources
            .iter()
            .find(|s| s.kind == SourceKind::Ical)
            .unwrap();
        assert!(!ical.ok);
        assert_eq!(
            ical.last_error_kind,
            Some(SourceErrorKind::AuthExpiredOrRevoked)
        );
        let json = serde_json::to_string(&doctor).unwrap();
        assert!(
            !json.contains("secretid") && !json.contains("calendar.example.edu"),
            "{json}"
        );

        // Without a database it still answers, and says why.
        let empty = tempfile::tempdir().unwrap();
        let doctor = doctor_in(empty.path(), &MemorySecrets::new());
        assert!(doctor.database_error.is_some() && doctor.schema_version.is_none());
    }

    #[test]
    fn report_is_redacted_and_pseudonymised() {
        let temp = tempfile::tempdir().unwrap();
        seeded(temp.path());
        let logs = core_diag::logs_dir_in(temp.path());
        std::fs::create_dir_all(&logs).unwrap();
        std::fs::write(
            logs.join("app-2026-09-26.log"),
            "2026-09-26T10:00:00Z pid=1 INFO pagelamp: synced DEMO101 — Intro to Demo Studies\n\
             2026-09-26T10:00:01Z pid=1 DEBUG pagelamp_canvas::http: GET /files/1/download?verifier=S3CR3T → 302\n\
             2026-09-26T10:00:02Z pid=1 WARN pagelamp: Bearer 1234~AbCdEfGhIjKlMnOpQrStUvWx rejected\n\
             2026-09-26T10:00:03Z pid=1 INFO pagelamp: feed https://calendar.example.edu/feeds/calendars/user_T0K3N.ics\n",
        )
        .unwrap();
        let report = report_in(temp.path(), &MemorySecrets::new());
        for secret in [
            "S3CR3T",
            "AbCdEfGh",
            "T0K3N",
            "DEMO101",
            "Intro to Demo Studies",
            "secretid",
        ] {
            assert!(!report.contains(secret), "{secret} leaked:\n{report}");
        }
        assert!(report.contains("synced Course 1"), "{report}");
        assert!(report.contains("- ical: not ok"), "{report}");
        assert!(report.contains("## Recent log"));
    }
}
