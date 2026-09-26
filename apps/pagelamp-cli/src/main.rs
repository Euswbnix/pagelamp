//! `pagelamp` — sync your courses and serve them to your own AI app over MCP.
//!
//! Every command is a thin wrapper over `pagelamp_app::App` (the same facade the desktop app
//! uses); `mcp` runs the stdio MCP server. Output: human-readable text on stdout, `--json` for
//! scripts; progress, notices and logs go to stderr — stdout is reserved for the MCP protocol
//! in `pagelamp mcp`.
//!
//! Secrets (Canvas token, calendar-feed URL) are read from the terminal without echo, or from
//! stdin when piped (`echo "$URL" | pagelamp ical add`), never from command-line arguments
//! (they would end up in shell history and process lists).

mod text;

use std::io::{BufRead, IsTerminal, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use chrono::NaiveDate;
use clap::{Parser, Subcommand, ValueEnum};
use pagelamp_app::diagnostics;
use pagelamp_app::{
    App, AppError, McpClient, McpClientConfig, SourceSyncResult, SyncEvent, SyncRequest,
    SyncSummary,
};
use pagelamp_core::brand;
use pagelamp_core::model::{AiMaterialsState, AiPolicy, SourceRecord};
use serde::Serialize;

#[derive(Parser)]
#[command(name = brand::CLI_NAME, version, about = brand::TAGLINE)]
struct Cli {
    /// Print machine-readable JSON instead of text (where supported).
    #[arg(long, global = true)]
    json: bool,
    /// Show detailed diagnostics (e.g. every Canvas request) on stderr and in the log file;
    /// same as PAGELAMP_LOG=debug. Never shows tokens, feed URLs or course text.
    #[arg(short, long, global = true)]
    verbose: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Add a Canvas LMS account (personal access token; personal use only).
    #[command(subcommand)]
    Canvas(CanvasCommand),
    /// Add a local folder of course materials (<folder>/<COURSE>/...).
    #[command(subcommand)]
    Folder(FolderCommand),
    /// Add a calendar feed (e.g. Canvas Calendar → "Calendar Feed") for deadlines.
    #[command(subcommand)]
    Ical(IcalCommand),
    /// List, remove or update data sources.
    Sources {
        #[command(subcommand)]
        command: Option<SourcesCommand>,
    },
    /// Sync every source (or one) into the local database.
    Sync {
        /// Only this source id (see `sources`).
        #[arg(long)]
        source: Option<String>,
        /// Only these courses (id or code); repeatable.
        #[arg(long = "course")]
        courses: Vec<String>,
        /// Download and index Canvas files (counts as viewing them in Canvas).
        #[arg(long)]
        download_files: bool,
        /// Skip Canvas files larger than this many MB.
        #[arg(long, default_value_t = 50)]
        max_file_mb: u32,
    },
    /// Data folder, sources, counts and last sync.
    Status,
    /// Your courses: current week, next deadline, AI policy and access.
    Courses,
    /// Change a course's settings.
    #[command(subcommand)]
    Course(CourseCommand),
    /// Search your course materials.
    Search {
        query: String,
        #[arg(long)]
        course: Option<String>,
        #[arg(long, default_value_t = 10)]
        limit: u32,
    },
    /// Run the MCP server on stdin/stdout (started by your AI app, not by you).
    Mcp,
    /// Print the snippet that connects your AI app.
    McpConfig {
        /// Which app (default: all).
        client: Option<ClientArg>,
    },
    /// Print JSON Schemas of the app facade types (for the desktop frontend).
    Schema,
    /// Check the setup: version, data folder, database, keychain, sources, AI apps.
    Doctor,
    /// Print a diagnostic report to paste into a GitHub issue (read it first).
    Report {
        /// Write the report to this file instead of stdout.
        #[arg(long)]
        out: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum CanvasCommand {
    /// Add a Canvas account. The token is read without echo (or from stdin when piped).
    Add {
        /// e.g. https://lms.example.edu
        #[arg(long)]
        base_url: String,
    },
}

#[derive(Subcommand)]
enum FolderCommand {
    /// Add a folder whose sub-folders are courses.
    Add {
        path: PathBuf,
        /// First day of the term (YYYY-MM-DD), used when a course has no course.toml.
        #[arg(long, value_parser = parse_date)]
        term_start: Option<NaiveDate>,
        #[arg(long)]
        label: Option<String>,
    },
}

#[derive(Subcommand)]
enum IcalCommand {
    /// Add a calendar feed. The URL is read without echo (or from stdin when piped).
    Add {
        #[arg(long)]
        label: Option<String>,
    },
}

#[derive(Subcommand)]
enum SourcesCommand {
    /// List sources (default).
    List,
    /// Remove a source and everything synced from it.
    Remove { source_id: String },
    /// Replace an expired Canvas token or a changed feed URL (read like `add`).
    UpdateSecret { source_id: String },
}

#[derive(Subcommand)]
enum CourseCommand {
    /// Record the course's generative-AI policy.
    Policy {
        course: String,
        policy: PolicyArg,
        #[arg(long)]
        note: Option<String>,
    },
    /// Override the term dates (or --clear to use the synced ones).
    Term {
        course: String,
        #[arg(long, value_parser = parse_date)]
        start: Option<NaiveDate>,
        #[arg(long, value_parser = parse_date)]
        end: Option<NaiveDate>,
        #[arg(long, conflicts_with_all = ["start", "end"])]
        clear: bool,
    },
    /// Hide a course everywhere (including from your AI app).
    Hide { course: String },
    /// Show a hidden course again.
    Show { course: String },
    /// Let your AI app read this course's materials (on) or not (off).
    AiAccess { course: String, access: OnOff },
}

#[derive(Clone, Copy, ValueEnum)]
enum PolicyArg {
    Unknown,
    Prohibited,
    // The stored/JSON spelling (`learning_aid`) works too.
    #[value(alias = "learning_aid")]
    LearningAid,
    #[value(alias = "allowed_with_citation")]
    AllowedWithCitation,
    Unrestricted,
}

impl From<PolicyArg> for AiPolicy {
    fn from(arg: PolicyArg) -> Self {
        match arg {
            PolicyArg::Unknown => AiPolicy::Unknown,
            PolicyArg::Prohibited => AiPolicy::Prohibited,
            PolicyArg::LearningAid => AiPolicy::LearningAid,
            PolicyArg::AllowedWithCitation => AiPolicy::AllowedWithCitation,
            PolicyArg::Unrestricted => AiPolicy::Unrestricted,
        }
    }
}

#[derive(Clone, Copy, ValueEnum)]
enum OnOff {
    On,
    Off,
}

#[derive(Clone, Copy, ValueEnum)]
enum ClientArg {
    ClaudeDesktop,
    ClaudeCode,
    Codex,
    Generic,
}

impl ClientArg {
    fn client(self) -> McpClient {
        match self {
            ClientArg::ClaudeDesktop => McpClient::ClaudeDesktop,
            ClientArg::ClaudeCode => McpClient::ClaudeCode,
            ClientArg::Codex => McpClient::Codex,
            ClientArg::Generic => McpClient::Generic,
        }
    }
}

fn parse_date(text: &str) -> Result<NaiveDate, String> {
    NaiveDate::parse_from_str(text, "%Y-%m-%d")
        .map_err(|_| format!("expected YYYY-MM-DD, got '{text}'"))
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let kind = if matches!(cli.command, Command::Mcp) {
        diagnostics::ProcessKind::Mcp
    } else {
        diagnostics::ProcessKind::App
    };
    diagnostics::init(kind, cli.verbose);
    let runtime = match tokio::runtime::Runtime::new() {
        Ok(runtime) => runtime,
        Err(err) => {
            eprintln!("error: could not start: {err}");
            return ExitCode::FAILURE;
        }
    };
    match runtime.block_on(run(cli)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::FAILURE
        }
    }
}

async fn run(cli: Cli) -> anyhow::Result<()> {
    let json = cli.json;
    match cli.command {
        Command::Mcp => {
            // Never create or migrate anything here: the server opens the DB read-only.
            let db = pagelamp_core::paths::db_path()?;
            pagelamp_mcp::serve_stdio(db).await
        }
        Command::Schema => print_json(&pagelamp_app::json_schema()),
        Command::Doctor => {
            // Works even when the database can't be opened.
            let doctor = match App::open() {
                Ok(app) => app.doctor()?,
                Err(_) => diagnostics::doctor()?,
            };
            if json {
                return print_json(&doctor);
            }
            print_doctor(&doctor);
            Ok(())
        }
        Command::Report { out } => {
            let report = match App::open() {
                Ok(app) => app.diagnostic_report()?,
                Err(_) => diagnostics::diagnostic_report()?,
            };
            match out {
                Some(path) => {
                    std::fs::write(&path, &report)?;
                    eprintln!(
                        "Wrote {}. Read it before sharing; it contains no tokens, feed links or course names.",
                        path.display()
                    );
                }
                None => print!("{report}"),
            }
            Ok(())
        }
        Command::McpConfig { client } => {
            let app = App::open()?;
            let binary = std::env::current_exe()?;
            let binary = std::fs::canonicalize(&binary).unwrap_or(binary);
            let configs: Vec<McpClientConfig> = app
                .mcp_client_configs(&binary)
                .into_iter()
                .filter(|c| client.is_none_or(|arg| arg.client() == c.client))
                .collect();
            if json {
                return print_json(&configs);
            }
            eprintln!("{}\n", text::ai_disclosure());
            for config in &configs {
                print_config(config, configs.len() > 1);
            }
            eprintln!("{}", text::restart_hint());
            Ok(())
        }
        Command::Canvas(CanvasCommand::Add { base_url }) => {
            let app = App::open()?;
            eprintln!("{}\n", text::canvas_personal_use());
            first_add_disclosure(&app)?;
            eprintln!("{}", text::CANVAS_TOKEN_HOWTO);
            let token = read_secret("Canvas access token: ")?;
            let source = app.add_canvas_source(&base_url, &token).await?;
            if json {
                return print_json(&source);
            }
            let name = source.config["account_name"]
                .as_str()
                .unwrap_or("your Canvas account");
            println!("Connected as {name}");
            eprintln!("Next: run `{} sync`.", brand::CLI_NAME);
            Ok(())
        }
        Command::Folder(FolderCommand::Add {
            path,
            term_start,
            label,
        }) => {
            let app = App::open()?;
            first_add_disclosure(&app)?;
            let source = app.add_folder_source(&path, term_start, label.as_deref())?;
            print_added(&source, json)
        }
        Command::Ical(IcalCommand::Add { label }) => {
            let app = App::open()?;
            first_add_disclosure(&app)?;
            let url = read_secret("Calendar feed URL: ")?;
            let source = app.add_ical_source(&url, label.as_deref()).await?;
            print_added(&source, json)
        }
        Command::Sources { command } => {
            let app = App::open()?;
            match command.unwrap_or(SourcesCommand::List) {
                SourcesCommand::List => {
                    let sources = app.list_sources()?;
                    if json {
                        return print_json(&sources);
                    }
                    if sources.is_empty() {
                        println!(
                            "No sources yet. Add one with `{} folder add <path>`.",
                            brand::CLI_NAME
                        );
                    }
                    for source in &sources {
                        println!("{}", source_line(source));
                    }
                    Ok(())
                }
                SourcesCommand::Remove { source_id } => {
                    app.remove_source(&source_id)?;
                    println!("Removed {source_id}.");
                    Ok(())
                }
                SourcesCommand::UpdateSecret { source_id } => {
                    let secret = read_secret("New token or feed URL: ")?;
                    let source = app.update_source_secret(&source_id, &secret).await?;
                    println!("Updated {}.", source.id);
                    Ok(())
                }
            }
        }
        Command::Sync {
            source,
            courses,
            download_files,
            max_file_mb,
        } => {
            let app = App::open()?;
            if download_files {
                eprintln!("{}", text::CANVAS_DOWNLOAD_NOTICE);
            }
            let req = SyncRequest {
                download_files,
                max_file_mb,
                only_courses: courses,
            };
            match source {
                Some(id) => {
                    let result = app.sync_source(&id, req, print_event).await?;
                    finish_sync(&[result], json)
                }
                None => {
                    let summary: SyncSummary = app.sync_all(req, print_event).await?;
                    if summary.results.is_empty() && !json {
                        println!(
                            "No sources yet. Add one with `{} folder add <path>`.",
                            brand::CLI_NAME
                        );
                        return Ok(());
                    }
                    finish_sync(&summary.results, json)
                }
            }
        }
        Command::Status => {
            let app = App::open()?;
            let status = app.status()?;
            if json {
                return print_json(&status);
            }
            println!("{} {}", brand::PRODUCT_NAME, status.version);
            println!("Data folder: {}", status.data_dir);
            let c = &status.counts;
            println!(
                "Courses: {} ({} hidden) · materials: {} ({} with text) · events: {}",
                c.courses, c.hidden_courses, c.materials, c.indexed_materials, c.events
            );
            match status.last_synced_at {
                Some(at) => println!(
                    "Last sync: {}",
                    at.with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M")
                ),
                None => println!("Last sync: never"),
            }
            if status.sync_in_progress {
                println!("A sync is running right now.");
            }
            for source in &status.sources {
                println!("  {}", source_line(source));
            }
            Ok(())
        }
        Command::Courses => {
            let app = App::open()?;
            let courses = app.list_courses()?;
            if json {
                return print_json(&courses);
            }
            if courses.is_empty() {
                println!(
                    "No courses yet: add a source, then run `{} sync`.",
                    brand::CLI_NAME
                );
            }
            for summary in &courses {
                let course = &summary.course;
                let week = match summary.timeline.current_week {
                    Some(n) => format!("week {n} ({})", confidence(summary.timeline.confidence)),
                    None => "week ?".to_string(),
                };
                let next = summary
                    .next_deadline
                    .as_ref()
                    .and_then(|d| {
                        d.event.when().map(|w| {
                            format!(
                                "next: {} {}",
                                d.event.title,
                                w.with_timezone(&chrono::Local).format("%b %-d")
                            )
                        })
                    })
                    .unwrap_or_default();
                println!(
                    "{:<40} {:<18} ai_policy={:<21} ai_materials={:<18} {}{}",
                    course.display_name(),
                    week,
                    course.ai_policy.as_str(),
                    ai_materials(summary.ai_materials),
                    next,
                    if course.hidden { "  [hidden]" } else { "" }
                );
            }
            Ok(())
        }
        Command::Course(command) => {
            let app = App::open()?;
            match command {
                CourseCommand::Policy {
                    course,
                    policy,
                    note,
                } => app.set_course_policy(&course, policy.into(), note.as_deref())?,
                CourseCommand::Term {
                    course,
                    start,
                    end,
                    clear,
                } => {
                    if !clear && start.is_none() && end.is_none() {
                        anyhow::bail!("give --start and/or --end, or --clear");
                    }
                    app.set_course_term(&course, start, end)?
                }
                CourseCommand::Hide { course } => app.set_course_hidden(&course, true)?,
                CourseCommand::Show { course } => app.set_course_hidden(&course, false)?,
                CourseCommand::AiAccess { course, access } => {
                    app.set_course_ai_access(&course, matches!(access, OnOff::On))?
                }
            }
            println!("Saved.");
            Ok(())
        }
        Command::Search {
            query,
            course,
            limit,
        } => {
            let app = App::open()?;
            let hits = app.search(&query, course.as_deref(), limit)?;
            if json {
                return print_json(&hits);
            }
            if hits.is_empty() {
                println!("No matches.");
            }
            for hit in &hits {
                println!(
                    "{} · {}{} · {}",
                    hit.course_code.as_deref().unwrap_or(&hit.course_id),
                    hit.material_title,
                    hit.locator
                        .as_deref()
                        .map(|l| format!(", {l}"))
                        .unwrap_or_default(),
                    hit.snippet.replace('\n', " ")
                );
            }
            Ok(())
        }
    }
}

// ----- helpers --------------------------------------------------------------------------------

/// Read a secret: from the terminal without echo, or one line from stdin when piped.
fn read_secret(prompt: &str) -> anyhow::Result<String> {
    let secret = if std::io::stdin().is_terminal() {
        rpassword::prompt_password(prompt)?
    } else {
        let mut line = String::new();
        std::io::stdin().lock().read_line(&mut line)?;
        line
    };
    Ok(secret.trim().to_string())
}

/// The AI-use disclosure, printed once: when the first source is added.
fn first_add_disclosure(app: &App) -> Result<(), AppError> {
    if app.list_sources()?.is_empty() {
        eprintln!("{}\n", text::ai_disclosure());
    }
    Ok(())
}

fn print_added(source: &SourceRecord, json: bool) -> anyhow::Result<()> {
    if json {
        return print_json(source);
    }
    println!("Added {} ({}).", source.label, source.id);
    println!("Next: run `{} sync`.", brand::CLI_NAME);
    Ok(())
}

fn print_event(event: SyncEvent) {
    match event {
        SyncEvent::SourceStarted { label, .. } => eprintln!("Syncing {label}…"),
        SyncEvent::Progress {
            message,
            current,
            total,
            ..
        } => match (current, total) {
            (Some(current), Some(total)) => eprintln!("  {message} ({current}/{total})"),
            _ => eprintln!("  {message}"),
        },
        SyncEvent::Warning { message, .. } => eprintln!("  warning: {message}"),
        SyncEvent::SourceFinished { .. } => {}
    }
}

fn finish_sync(results: &[SourceSyncResult], json: bool) -> anyhow::Result<()> {
    if json {
        print_json(&results)?;
    } else {
        for r in results {
            let seconds = (r.finished_at - r.started_at).num_milliseconds() as f64 / 1000.0;
            if r.ok {
                println!(
                    "{}: ok — {} courses, {} materials ({} newly indexed), {} events",
                    r.label, r.courses, r.materials, r.files_indexed, r.events
                );
            } else {
                println!(
                    "{}: FAILED — {}",
                    r.label,
                    r.error.as_deref().unwrap_or("unknown error")
                );
            }
            if !r.course_summaries.is_empty() {
                let width = r
                    .course_summaries
                    .iter()
                    .map(|c| c.course.chars().count())
                    .max()
                    .unwrap_or(0);
                for c in &r.course_summaries {
                    println!(
                        "  {:<width$}  {:>3} modules  {:>4} pages  {:>4} files  {:>3} events{}",
                        c.course,
                        c.modules,
                        c.pages,
                        c.files,
                        c.events,
                        match c.warnings {
                            0 => String::new(),
                            1 => "  1 warning".to_string(),
                            n => format!("  {n} warnings"),
                        }
                    );
                }
            }
            match r.requests {
                Some(requests) => println!("  {requests} requests · {seconds:.1} s"),
                None => println!("  {seconds:.1} s"),
            }
            if !r.warnings.is_empty() {
                println!(
                    "  {} warning(s) above; run with -v for request details",
                    r.warnings.len()
                );
            }
        }
    }
    if results.iter().any(|r| !r.ok) {
        anyhow::bail!("some sources failed to sync");
    }
    Ok(())
}

fn print_doctor(doctor: &diagnostics::DoctorReport) {
    let yes = |b: bool| if b { "yes" } else { "no" };
    println!(
        "{} {} on {} ({})",
        brand::PRODUCT_NAME,
        doctor.version,
        doctor.os,
        doctor.arch
    );
    println!("Data folder: {}", doctor.data_dir);
    println!("Logs folder: {}", doctor.logs_dir);
    match (&doctor.schema_version, &doctor.database_error) {
        (Some(v), _) => println!("Database: ok (schema {v})"),
        (None, Some(err)) => println!("Database: not readable — {err}"),
        (None, None) => println!("Database: unknown"),
    }
    match &doctor.keychain_error {
        None => println!("Keychain: available"),
        Some(err) => println!("Keychain: NOT available — {err}"),
    }
    println!(
        "Courses: {} ({} hidden) · materials: {} · events: {}",
        doctor.courses, doctor.hidden_courses, doctor.materials, doctor.events
    );
    if doctor.sources.is_empty() {
        println!(
            "Sources: none (add one with `{} folder add <path>`)",
            brand::CLI_NAME
        );
    }
    for source in &doctor.sources {
        println!(
            "Source {}: {}{}{}",
            source.kind.as_str(),
            if source.ok { "ok" } else { "not ok" },
            source
                .last_synced_at
                .map(|t| format!(
                    ", last synced {}",
                    t.with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M")
                ))
                .unwrap_or_default(),
            source
                .last_error_kind
                .map(|k| format!(", last error: {}", k.as_str()))
                .unwrap_or_default()
        );
    }
    println!(
        "AI apps with {} configured: Claude Desktop {} · Claude Code {} · Codex {}",
        brand::PRODUCT_NAME,
        yes(doctor.mcp_clients.claude_desktop),
        yes(doctor.mcp_clients.claude_code),
        yes(doctor.mcp_clients.codex)
    );
    if let Some(crash) = &doctor.last_crash {
        println!(
            "Last crash: {} ({}); run `{} report` to include it in an issue",
            crash
                .time
                .with_timezone(&chrono::Local)
                .format("%Y-%m-%d %H:%M"),
            crash.message,
            brand::CLI_NAME
        );
    }
}

fn print_config(config: &McpClientConfig, with_title: bool) {
    if with_title {
        println!("== {} ==", config.title);
    }
    if let Some(path) = &config.config_path_hint {
        eprintln!("Add to {path}:");
    }
    println!("{}", config.content);
    for note in &config.notes {
        eprintln!("  • {note}");
    }
    if with_title {
        println!();
    }
}

fn source_line(source: &SourceRecord) -> String {
    let synced = source
        .last_synced_at
        .map(|at| {
            at.with_timezone(&chrono::Local)
                .format("%Y-%m-%d %H:%M")
                .to_string()
        })
        .unwrap_or_else(|| "never synced".into());
    let error = source
        .last_error
        .as_deref()
        .map(|e| format!(" — last sync failed: {e}"))
        .unwrap_or_default();
    format!(
        "{} [{}] {} · {synced}{error}",
        source.id,
        source.kind.as_str(),
        source.label
    )
}

fn confidence(c: pagelamp_core::model::Confidence) -> &'static str {
    match c {
        pagelamp_core::model::Confidence::High => "high",
        pagelamp_core::model::Confidence::Medium => "medium",
        pagelamp_core::model::Confidence::Low => "low",
    }
}

fn ai_materials(state: AiMaterialsState) -> &'static str {
    match state {
        AiMaterialsState::Readable => "readable",
        AiMaterialsState::TurnedOff => "turned_off",
        AiMaterialsState::WithheldByPolicy => "withheld_by_policy",
    }
}

fn print_json(value: &impl Serialize) -> anyhow::Result<()> {
    let mut out = std::io::stdout().lock();
    serde_json::to_writer_pretty(&mut out, value)?;
    writeln!(out)?;
    Ok(())
}
