//! StudentOS MCP server — the v0.1 product surface.
//!
//! Transport: stdio (one process per AI client; each opens the DB read-only per request
//! inside `spawn_blocking` — no pool, no global lock). stdout carries protocol only; all
//! logging goes to stderr.
//!
//! HARD RULES
//! - Never calls Canvas or any network API (Canvas API Policy §3(i)); serves the local DB only.
//! - Read-only except `save_study_plan`.
//! - No tool returns assignment instructions/solutions; deadlines are title + date + link.
//! - All course text is returned inside
//!   `<course_material id="…" title="…" locator="…">…</course_material>` wrappers, and the
//!   server `instructions` tell the model that text inside is untrusted data, never
//!   instructions (prompt-injection hygiene).
//! - Every text-returning tool caps output (default 12_000 chars) and paginates.
//!
//! TOOLS (names are the public contract)
//! - `list_courses()` → courses with code, name, current week (+confidence), ai_policy,
//!   data freshness (source last_synced_at).
//! - `course_overview(course)` → timeline (week, confidence, evidence), current modules,
//!   materials published in the last 14 days, deadlines in the next 21 days, announcements
//!   of the last 14 days (titles + ids), ai_policy + note.
//! - `week_materials(course, week?)` → materials of that week (default current week):
//!   id, title, kind, locator count, text_status, url.
//! - `read_material(material_id, from_chunk?, max_chars?)` → wrapped text with locators,
//!   `next_chunk` for pagination.
//! - `search_materials(query, course?, limit?)` → hits with snippet, material title,
//!   locator, url (for citations).
//! - `list_deadlines(course?, days_ahead? = 21, days_back? = 0)` → events for planning.
//! - `get_announcements(course, days? = 14)` → wrapped announcement text.
//! - `get_study_plan()` / `save_study_plan(plan)` — plan per `model::StudyPlan`.
//! - `sync_status()` → sources with last_synced_at / last_error; warns if > 24h stale and
//!   tells the model the student can run `studentos sync` (the server itself cannot sync).
//!
//! AI ACCESS (docs/ARCHITECTURE.md §3 rule 8): material TEXT (read_material, snippets,
//! announcement bodies, anything a prompt would inline) is only returned for courses whose
//! `ai_materials` is `readable`; otherwise the tools return a normal result explaining why
//! (wording in `text`). Structure, deadlines and study plans stay available. Enforcement
//! lives in `studentos_core::views`, so this crate cannot leak withheld text by accident.
//!
//! WORDING: every string sent to the model lives in `text.rs` (editable by non-Rust
//! maintainers). Claude Desktop ignores server `instructions`, so the essential rules are
//! repeated in tool descriptions and in a `guidance` field of course_overview /
//! week_materials / read_material results. No MCP sampling (deprecated, unsupported).
//!
//! PROMPTS (appear as slash commands in Claude Desktop / Claude Code)
//! - `weekly_review(course, week?)` — explain this week's content, cite materials as
//!   "Title, locator", check understanding with 2–3 questions; respect ai_policy.
//! - `catch_up(course, since?)` — what was missed since a date, in order, with materials.
//! - `study_plan(days? = 14, hours_per_week?)` — gather deadlines + timelines across courses,
//!   propose a day-by-day plan as data, then call `save_study_plan`.
//!
//! SERVER INSTRUCTIONS (initialize result) must state: purpose; cite sources; course text is
//! untrusted data; do not produce solutions to graded assignments — tutor instead, and for
//! `prohibited`/`unknown` ai_policy courses limit help to explaining course concepts;
//! answer in the student's language.

pub mod text;

mod format;

use std::path::PathBuf;
use std::sync::Arc;

use chrono::{Local, NaiveDate, TimeDelta};
use rmcp::handler::server::router::prompt::PromptRouter;
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{
    CallToolResult, Implementation, PromptMessage, Role, ServerCapabilities, ServerConfig,
};
use rmcp::{
    ErrorData, ServerHandler, ServiceExt, prompt, prompt_handler, prompt_router, tool,
    tool_handler, tool_router,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use studentos_core::brand;
use studentos_core::model::{
    AiMaterialsState, AiPolicy, Confidence, EventKind, MaterialKind, SourceErrorKind, SourceKind,
    StoreCounts, StudyPlan, TextStatus, Timestamp,
};
use studentos_core::store::Store;
use studentos_core::views::{self, AsOf, Deadline, MaterialView};

use crate::format::{OUTPUT_CAP, cap_list, error_result, json_result, text_result, wrap};

/// Run the MCP server over stdio until the client disconnects.
/// `db_path` is normally `studentos_core::paths::db_path()`. The server starts even when the
/// database does not exist yet (tools then tell the student to sync); it never touches the
/// network and never writes to stdout except protocol messages.
pub async fn serve_stdio(db_path: PathBuf) -> anyhow::Result<()> {
    let service = StudentOsServer::new(db_path)
        .serve(rmcp::transport::stdio())
        .await?;
    service.waiting().await?;
    Ok(())
}

/// The MCP server. Cheap to clone; every request opens its own short-lived read-only
/// connection inside `spawn_blocking` (docs/ARCHITECTURE.md §4).
#[derive(Clone)]
pub struct StudentOsServer {
    db_path: Arc<PathBuf>,
    tool_router: ToolRouter<Self>,
    prompt_router: PromptRouter<Self>,
}

// ----- limits ---------------------------------------------------------------------------------

const DEFAULT_SEARCH_LIMIT: u32 = 8;
const MAX_SEARCH_LIMIT: u32 = 25;
const MIN_READ_CHARS: u32 = 500;
const MAX_DAYS: u32 = 365;
const MAX_ANNOUNCEMENT_CHARS: usize = 4_000;
const MAX_LISTED_MATERIALS: usize = 60;
const MAX_LISTED_DEADLINES: usize = 60;
const MAX_PLAN_DAYS: u32 = 120;

// ----- tool parameters ------------------------------------------------------------------------

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CourseArgs {
    #[schemars(description = text::PARAM_COURSE)]
    pub course: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct WeekArgs {
    #[schemars(description = text::PARAM_COURSE)]
    pub course: String,
    #[schemars(description = text::PARAM_WEEK)]
    pub week: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReadArgs {
    #[schemars(description = text::PARAM_MATERIAL_ID)]
    pub material_id: String,
    #[schemars(description = text::PARAM_FROM_CHUNK)]
    pub from_chunk: Option<u32>,
    #[schemars(description = text::PARAM_MAX_CHARS)]
    pub max_chars: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SearchArgs {
    #[schemars(description = text::PARAM_QUERY)]
    pub query: String,
    #[schemars(description = text::PARAM_COURSE_OPTIONAL)]
    pub course: Option<String>,
    #[schemars(description = text::PARAM_LIMIT)]
    pub limit: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DeadlineArgs {
    #[schemars(description = text::PARAM_COURSE_OPTIONAL)]
    pub course: Option<String>,
    #[schemars(description = text::PARAM_DAYS_AHEAD)]
    pub days_ahead: Option<u32>,
    #[schemars(description = text::PARAM_DAYS_BACK)]
    pub days_back: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct AnnouncementArgs {
    #[schemars(description = text::PARAM_COURSE)]
    pub course: String,
    #[schemars(description = text::PARAM_DAYS)]
    pub days: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SavePlanArgs {
    #[schemars(description = text::PARAM_PLAN)]
    pub plan: StudyPlan,
}

// ----- prompt arguments (MCP prompt arguments are always strings) -----------------------------

#[derive(Debug, Deserialize, JsonSchema)]
pub struct WeeklyReviewArgs {
    #[schemars(description = text::ARG_COURSE)]
    pub course: String,
    #[schemars(description = text::ARG_WEEK)]
    pub week: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CatchUpArgs {
    #[schemars(description = text::ARG_COURSE)]
    pub course: String,
    #[schemars(description = text::ARG_SINCE)]
    pub since: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct StudyPlanArgs {
    #[schemars(description = text::ARG_DAYS)]
    pub days: Option<String>,
    #[schemars(description = text::ARG_HOURS)]
    pub hours_per_week: Option<String>,
}

// ----- tools ----------------------------------------------------------------------------------

#[tool_router]
impl StudentOsServer {
    #[tool(description = text::LIST_COURSES, annotations(read_only_hint = true))]
    async fn list_courses(&self) -> CallToolResult {
        let result = self
            .read(|store| {
                let courses = views::list_courses(store, false, AsOf::now_local())?;
                let stale = views::sync_status(store, AsOf::now_local())?.stale;
                Ok((courses, stale))
            })
            .await;
        match result {
            Ok((courses, stale)) => json_result(&CourseList {
                courses: courses.iter().map(CourseLine::from).collect(),
                hint: stale.then(text::stale_hint),
            }),
            Err(error) => error,
        }
    }

    #[tool(description = text::COURSE_OVERVIEW, annotations(read_only_hint = true))]
    async fn course_overview(&self, Parameters(args): Parameters<CourseArgs>) -> CallToolResult {
        let result = self
            .read(move |store| {
                views::course_overview(store, &args.course, false, AsOf::now_local())
            })
            .await;
        match result {
            Ok(overview) => {
                let (recent_materials, _) =
                    cap_list(overview.recent_materials, MAX_LISTED_MATERIALS);
                json_result(&Overview {
                    guidance: text::guidance(),
                    course: CourseInfo::from(&overview.course),
                    ai_materials: overview.ai_materials,
                    note: withheld_note(overview.ai_materials),
                    timeline: TimelineInfo {
                        current_week: overview.timeline.current_week,
                        confidence: overview.timeline.confidence,
                        outside_term: overview.timeline.outside_term,
                        evidence: overview.timeline.evidence,
                    },
                    current_modules: overview
                        .current_modules
                        .iter()
                        .map(|m| ModuleInfo {
                            id: m.id.clone(),
                            name: m.name.clone(),
                            week: m.week_hint,
                        })
                        .collect(),
                    recent_materials: recent_materials.iter().map(MaterialInfo::from).collect(),
                    upcoming_deadlines: overview
                        .upcoming_deadlines
                        .iter()
                        .map(DeadlineInfo::from)
                        .collect(),
                    recent_announcements: overview
                        .recent_announcements
                        .iter()
                        .map(|a| AnnouncementInfo {
                            id: a.id.clone(),
                            title: a.title.clone(),
                            posted_at: a.published_at,
                        })
                        .collect(),
                    source: overview.source_label,
                    last_synced_at: overview.last_synced_at,
                })
            }
            Err(error) => error,
        }
    }

    #[tool(description = text::WEEK_MATERIALS, annotations(read_only_hint = true))]
    async fn week_materials(&self, Parameters(args): Parameters<WeekArgs>) -> CallToolResult {
        let result = self
            .read(move |store| {
                views::week_materials(store, &args.course, args.week, false, AsOf::now_local())
            })
            .await;
        match result {
            Ok(week) => {
                let (materials, omitted) = cap_list(week.materials, MAX_LISTED_MATERIALS);
                json_result(&Week {
                    guidance: text::guidance(),
                    course: CourseInfo::from(&week.course),
                    ai_materials: week.ai_materials,
                    week: week.week,
                    requested_week: week.requested_week,
                    available_weeks: week.available_weeks,
                    note: week.note,
                    modules: week
                        .modules
                        .iter()
                        .map(|m| ModuleInfo {
                            id: m.id.clone(),
                            name: m.name.clone(),
                            week: m.week_hint,
                        })
                        .collect(),
                    materials: materials.iter().map(MaterialInfo::from).collect(),
                    more: (omitted > 0).then(|| text::output_capped(omitted)),
                })
            }
            Err(error) => error,
        }
    }

    #[tool(description = text::READ_MATERIAL, annotations(read_only_hint = true))]
    async fn read_material(&self, Parameters(args): Parameters<ReadArgs>) -> CallToolResult {
        let max_chars = args
            .max_chars
            .unwrap_or(OUTPUT_CAP as u32)
            .clamp(MIN_READ_CHARS, OUTPUT_CAP as u32) as usize;
        let from = args.from_chunk.unwrap_or(0);
        let result = self
            .read(move |store| views::read_material(store, &args.material_id, from, max_chars))
            .await;
        let material = match result {
            Ok(material) => material,
            Err(error) => return error,
        };
        let view = &material.material;
        let mut out = vec![text::guidance()];
        out.push(format!(
            "Material: \"{}\" ({}), course {}, {} part(s){}",
            format::escape_attr(&view.title),
            kind_name(view.kind),
            material.course_code.as_deref().unwrap_or(&view.course_id),
            material.total_chunks,
            public_url(view.url.as_deref())
                .map(|u| format!(", link: {u}"))
                .unwrap_or_default()
        ));
        if !material.ai_materials.is_readable() {
            out.push(text::withheld(
                material.ai_materials == AiMaterialsState::TurnedOff,
            ));
            return text_result(out.join("\n"));
        }
        if material.total_chunks == 0 {
            out.push(text::NO_TEXT.to_string());
            return text_result(out.join("\n"));
        }
        for chunk in &material.chunks {
            out.push(wrap(
                &[
                    ("id", Some(&view.id)),
                    ("title", Some(&view.title)),
                    ("course", material.course_code.as_deref()),
                    ("locator", chunk.locator.as_deref()),
                    ("part", Some(&chunk.ord.to_string())),
                ],
                &chunk.text,
            ));
        }
        out.push(match material.next_chunk {
            Some(next) => text::read_more(next),
            None => text::END_OF_MATERIAL.to_string(),
        });
        text_result(out.join("\n"))
    }

    #[tool(description = text::SEARCH_MATERIALS, annotations(read_only_hint = true))]
    async fn search_materials(&self, Parameters(args): Parameters<SearchArgs>) -> CallToolResult {
        let limit = args
            .limit
            .unwrap_or(DEFAULT_SEARCH_LIMIT)
            .clamp(1, MAX_SEARCH_LIMIT);
        let query = args.query.clone();
        let result = self
            .read(move |store| {
                views::search_for_ai(store, &args.query, args.course.as_deref(), limit)
            })
            .await;
        let results = match result {
            Ok(results) => results,
            Err(error) => return error,
        };
        if let Some(state) = results.course_ai_materials
            && !state.is_readable()
        {
            return text_result(text::withheld(state == AiMaterialsState::TurnedOff));
        }
        let mut out = vec![format!("Search results for \"{}\":", query.trim())];
        let mut used = 0;
        let mut shown = 0;
        for hit in &results.hits {
            let block = wrap(
                &[
                    ("id", Some(&hit.material_id)),
                    ("title", Some(&hit.material_title)),
                    ("course", hit.course_code.as_deref()),
                    ("locator", hit.locator.as_deref()),
                    ("part", Some(&hit.chunk_ord.to_string())),
                    ("url", public_url(hit.url.as_deref())),
                ],
                &hit.snippet,
            );
            if used + block.len() > OUTPUT_CAP && shown > 0 {
                break;
            }
            used += block.len();
            shown += 1;
            out.push(block);
        }
        if results.hits.is_empty() {
            out.push(text::NO_HITS.to_string());
        } else if shown < results.hits.len() {
            out.push(text::output_capped(results.hits.len() - shown));
        }
        if !results.excluded_courses.is_empty() {
            out.push(text::excluded_courses(&results.excluded_courses.join(", ")));
        }
        text_result(out.join("\n"))
    }

    #[tool(description = text::LIST_DEADLINES, annotations(read_only_hint = true))]
    async fn list_deadlines(&self, Parameters(args): Parameters<DeadlineArgs>) -> CallToolResult {
        let ahead = args
            .days_ahead
            .unwrap_or(views::UPCOMING_DAYS)
            .min(MAX_DAYS);
        let back = args.days_back.unwrap_or(0).min(MAX_DAYS);
        let result = self
            .read(move |store| {
                views::deadlines(
                    store,
                    args.course.as_deref(),
                    ahead,
                    back,
                    false,
                    AsOf::now_local(),
                )
            })
            .await;
        match result {
            Ok(deadlines) => {
                let (deadlines, omitted) = cap_list(deadlines, MAX_LISTED_DEADLINES);
                json_result(&DeadlineList {
                    deadlines: deadlines.iter().map(DeadlineInfo::from).collect(),
                    more: (omitted > 0).then(|| text::output_capped(omitted)),
                })
            }
            Err(error) => error,
        }
    }

    #[tool(description = text::GET_ANNOUNCEMENTS, annotations(read_only_hint = true))]
    async fn get_announcements(
        &self,
        Parameters(args): Parameters<AnnouncementArgs>,
    ) -> CallToolResult {
        let days = args.days.unwrap_or(views::RECENT_DAYS).clamp(1, MAX_DAYS);
        let result = self
            .read(move |store| {
                views::announcements(
                    store,
                    &args.course,
                    days,
                    MAX_ANNOUNCEMENT_CHARS,
                    AsOf::now_local(),
                )
            })
            .await;
        let items = match result {
            Ok(items) => items,
            Err(error) => return error,
        };
        if items.is_empty() {
            return text_result(text::NO_ANNOUNCEMENTS);
        }
        let mut out = Vec::new();
        if let Some(first) = items.first()
            && !first.ai_materials.is_readable()
        {
            out.push(text::withheld(
                first.ai_materials == AiMaterialsState::TurnedOff,
            ));
            // Structure stays available: titles and dates only.
            for item in &items {
                out.push(format!(
                    "- {} ({})",
                    item.material.title,
                    item.material
                        .published_at
                        .map(|p| p.to_rfc3339())
                        .unwrap_or_default()
                ));
            }
            return text_result(out.join("\n"));
        }
        let mut used = 0;
        for (shown, item) in items.iter().enumerate() {
            let posted = item.material.published_at.map(|p| p.to_rfc3339());
            let block = wrap(
                &[
                    ("id", Some(&item.material.id)),
                    ("title", Some(&item.material.title)),
                    ("kind", Some("announcement")),
                    ("posted", posted.as_deref()),
                    ("url", public_url(item.material.url.as_deref())),
                ],
                &item.text,
            );
            if used + block.len() > OUTPUT_CAP && shown > 0 {
                out.push(text::output_capped(items.len() - shown));
                break;
            }
            used += block.len();
            out.push(block);
        }
        text_result(out.join("\n"))
    }

    #[tool(description = text::GET_STUDY_PLAN, annotations(read_only_hint = true))]
    async fn get_study_plan(&self) -> CallToolResult {
        match self.read(|store| store.latest_study_plan()).await {
            Ok(Some(plan)) => json_result(&plan),
            Ok(None) => text_result(text::NO_PLAN),
            Err(error) => error,
        }
    }

    #[tool(description = text::SAVE_STUDY_PLAN, annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = false))]
    async fn save_study_plan(&self, Parameters(args): Parameters<SavePlanArgs>) -> CallToolResult {
        let db = Arc::clone(&self.db_path);
        let saved = tokio::task::spawn_blocking(move || {
            // The one MCP write: a short read-write transaction (busy_timeout applies).
            if !db.is_file() {
                return Err(studentos_core::Error::NotInitialised(
                    db.display().to_string(),
                ));
            }
            Store::open(&db)?.save_study_plan(&args.plan)
        })
        .await;
        match saved {
            Ok(Ok(stored)) => json_result(&SavedPlan {
                saved: true,
                id: stored.id,
                created_at: stored.created_at,
                items: stored.plan.items.len(),
            }),
            Ok(Err(err)) => core_error(err),
            Err(join) => error_result(format!("internal error: {join}")),
        }
    }

    #[tool(description = text::sync_status_description(), annotations(read_only_hint = true))]
    async fn sync_status(&self) -> CallToolResult {
        match self
            .read(|store| views::sync_status(store, AsOf::now_local()))
            .await
        {
            Ok(status) => json_result(&SyncInfo {
                sources: status
                    .sources
                    .iter()
                    .map(|s| SourceInfo {
                        id: s.source.id.clone(),
                        label: s.source.label.clone(),
                        kind: s.source.kind,
                        last_synced_at: s.source.last_synced_at,
                        last_error: s.source.last_error.clone(),
                        last_error_kind: s.source.last_error_kind,
                        stale: s.stale,
                    })
                    .collect(),
                counts: status.counts,
                last_synced_at: status.last_synced_at,
                stale: status.stale,
                hint: status.stale.then(text::stale_hint),
            }),
            Err(error) => error,
        }
    }
}

// ----- prompts --------------------------------------------------------------------------------

#[prompt_router]
impl StudentOsServer {
    #[prompt(name = "weekly_review", description = text::PROMPT_WEEKLY_REVIEW)]
    async fn weekly_review(
        &self,
        Parameters(args): Parameters<WeeklyReviewArgs>,
    ) -> Result<Vec<PromptMessage>, ErrorData> {
        let week = parse_number(args.week.as_deref(), "week")?;
        let (label, state) = self.course_state(&args.course).await?;
        let mut message = text::weekly_review(&label, week);
        append_withheld(&mut message, &label, state);
        Ok(vec![PromptMessage::new_text(Role::User, message)])
    }

    #[prompt(name = "catch_up", description = text::PROMPT_CATCH_UP)]
    async fn catch_up(
        &self,
        Parameters(args): Parameters<CatchUpArgs>,
    ) -> Result<Vec<PromptMessage>, ErrorData> {
        let since = match args
            .since
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            Some(text) => NaiveDate::parse_from_str(text, "%Y-%m-%d").map_err(|_| {
                ErrorData::invalid_params(
                    format!("since must be a date like 2026-09-01, not '{text}'"),
                    None,
                )
            })?,
            None => Local::now().date_naive() - TimeDelta::days(i64::from(views::RECENT_DAYS)),
        };
        let (label, state) = self.course_state(&args.course).await?;
        let mut message = text::catch_up(&label, &since.format("%Y-%m-%d").to_string());
        append_withheld(&mut message, &label, state);
        Ok(vec![PromptMessage::new_text(Role::User, message)])
    }

    #[prompt(name = "study_plan", description = text::PROMPT_STUDY_PLAN)]
    async fn study_plan(
        &self,
        Parameters(args): Parameters<StudyPlanArgs>,
    ) -> Result<Vec<PromptMessage>, ErrorData> {
        let days = parse_number(args.days.as_deref(), "days")?
            .unwrap_or(14)
            .clamp(1, MAX_PLAN_DAYS);
        let hours = parse_number(args.hours_per_week.as_deref(), "hours_per_week")?;
        Ok(vec![PromptMessage::new_text(
            Role::User,
            text::study_plan(days, hours),
        )])
    }
}

#[tool_handler(router = self.tool_router)]
#[prompt_handler(router = self.prompt_router)]
impl ServerHandler for StudentOsServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_prompts()
                .build(),
        )
        .with_server_info(
            Implementation::new(brand::MCP_SERVER_KEY, env!("CARGO_PKG_VERSION"))
                .with_title(brand::PRODUCT_NAME)
                .with_description(brand::TAGLINE)
                .with_website_url(brand::HOMEPAGE),
        )
        .with_instructions(text::instructions())
    }
}

impl StudentOsServer {
    /// A server over the database at `db_path` (which may not exist yet).
    pub fn new(db_path: PathBuf) -> Self {
        StudentOsServer {
            db_path: Arc::new(db_path),
            tool_router: Self::tool_router(),
            prompt_router: Self::prompt_router(),
        }
    }

    /// Run `f` on a fresh read-only connection on the blocking pool. Errors become a
    /// ready-to-return tool error result.
    async fn read<T: Send + 'static>(
        &self,
        f: impl FnOnce(&Store) -> studentos_core::Result<T> + Send + 'static,
    ) -> Result<T, CallToolResult> {
        let db = Arc::clone(&self.db_path);
        match tokio::task::spawn_blocking(move || f(&Store::open_read_only(&db)?)).await {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(err)) => Err(core_error(err)),
            Err(join) => Err(error_result(format!("internal error: {join}"))),
        }
    }

    /// Display label and AI-materials state of a course, for prompts. Before the first sync
    /// the prompt still works (the tools will explain the missing data).
    async fn course_state(&self, course: &str) -> Result<(String, AiMaterialsState), ErrorData> {
        let db = Arc::clone(&self.db_path);
        let query = course.to_string();
        let found =
            tokio::task::spawn_blocking(move || Store::open_read_only(&db)?.resolve_course(&query))
                .await
                .map_err(|err| ErrorData::internal_error(err.to_string(), None))?;
        match found {
            Ok(course) => Ok((course.display_name(), course.ai_materials())),
            Err(studentos_core::Error::NotInitialised(_)) => {
                Ok((course.to_string(), AiMaterialsState::Readable))
            }
            Err(err) => Err(ErrorData::invalid_params(err.to_string(), None)),
        }
    }
}

// ----- helpers --------------------------------------------------------------------------------

/// A core error as a tool error the model can explain to the student.
fn core_error(err: studentos_core::Error) -> CallToolResult {
    use studentos_core::Error as E;
    match err {
        E::NotInitialised(_) => error_result(text::not_initialised()),
        E::NotFound(what) => error_result(format!("Not found: {what}")),
        E::Invalid(what) => error_result(format!("Invalid input: {what}")),
        err @ E::Ambiguous { .. } => error_result(err.to_string()),
        err @ E::SchemaTooNew { .. } => error_result(err.to_string()),
        other => {
            tracing::error!("tool failed: {other}");
            error_result(format!("Internal error: {other}"))
        }
    }
}

fn append_withheld(message: &mut String, label: &str, state: AiMaterialsState) {
    if !state.is_readable() {
        message.push_str("\n\n");
        message.push_str(&text::prompt_withheld(
            label,
            state == AiMaterialsState::TurnedOff,
        ));
    }
}

/// Parse an optional numeric prompt argument ("" = absent).
fn parse_number(value: Option<&str>, name: &str) -> Result<Option<u32>, ErrorData> {
    match value.map(str::trim).filter(|v| !v.is_empty()) {
        None => Ok(None),
        Some(text) => text.parse().map(Some).map_err(|_| {
            ErrorData::invalid_params(format!("{name} must be a whole number, not '{text}'"), None)
        }),
    }
}

/// Withheld explanation for overview-style results.
fn withheld_note(state: AiMaterialsState) -> Option<String> {
    (!state.is_readable()).then(|| text::withheld(state == AiMaterialsState::TurnedOff))
}

/// URLs worth giving the AI app: LMS links yes, `file://` paths no (they reveal local paths,
/// e.g. the student's user name, and the AI app can't open them anyway).
fn public_url(url: Option<&str>) -> Option<&str> {
    url.filter(|u| !u.trim_start().to_ascii_lowercase().starts_with("file:"))
}

fn kind_name(kind: MaterialKind) -> &'static str {
    kind.as_str()
}

// ----- compact output shapes ------------------------------------------------------------------
// Tool results use these instead of the full view types: fewer tokens, and nothing private
// (source configs, local paths) reaches the AI provider.

#[derive(Serialize)]
struct CourseList {
    courses: Vec<CourseLine>,
    #[serde(skip_serializing_if = "Option::is_none")]
    hint: Option<String>,
}

#[derive(Serialize)]
struct CourseLine {
    id: String,
    code: Option<String>,
    name: String,
    current_week: Option<u32>,
    week_confidence: Confidence,
    ai_policy: AiPolicy,
    #[serde(skip_serializing_if = "Option::is_none")]
    ai_policy_note: Option<String>,
    ai_materials: AiMaterialsState,
    materials: u32,
    readable_materials: u32,
    next_deadline: Option<DeadlineInfo>,
    source: String,
    last_synced_at: Option<Timestamp>,
}

impl From<&views::CourseSummary> for CourseLine {
    fn from(summary: &views::CourseSummary) -> Self {
        CourseLine {
            id: summary.course.id.clone(),
            code: summary.course.code.clone(),
            name: summary.course.name.clone(),
            current_week: summary.timeline.current_week,
            week_confidence: summary.timeline.confidence,
            ai_policy: summary.course.ai_policy,
            ai_policy_note: summary.course.ai_policy_note.clone(),
            ai_materials: summary.ai_materials,
            materials: summary.counts.materials,
            readable_materials: summary.counts.indexed_materials,
            next_deadline: summary.next_deadline.as_ref().map(DeadlineInfo::from),
            source: summary.source_label.clone(),
            last_synced_at: summary.last_synced_at,
        }
    }
}

#[derive(Serialize)]
struct CourseInfo {
    id: String,
    code: Option<String>,
    name: String,
    term_start: Option<NaiveDate>,
    term_end: Option<NaiveDate>,
    url: Option<String>,
    ai_policy: AiPolicy,
    #[serde(skip_serializing_if = "Option::is_none")]
    ai_policy_note: Option<String>,
}

impl From<&studentos_core::model::Course> for CourseInfo {
    fn from(course: &studentos_core::model::Course) -> Self {
        CourseInfo {
            id: course.id.clone(),
            code: course.code.clone(),
            name: course.name.clone(),
            term_start: course.term_start,
            term_end: course.term_end,
            url: public_url(course.url.as_deref()).map(str::to_string),
            ai_policy: course.ai_policy,
            ai_policy_note: course.ai_policy_note.clone(),
        }
    }
}

#[derive(Serialize)]
struct TimelineInfo {
    current_week: Option<u32>,
    confidence: Confidence,
    outside_term: bool,
    /// Reasons for the week (may quote module/material titles — course data).
    evidence: Vec<String>,
}

#[derive(Serialize)]
struct ModuleInfo {
    id: String,
    name: String,
    week: Option<u32>,
}

#[derive(Serialize)]
struct MaterialInfo {
    id: String,
    title: String,
    kind: MaterialKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    week: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    module: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    published_at: Option<Timestamp>,
    #[serde(skip_serializing_if = "Option::is_none")]
    url: Option<String>,
    /// Parts of text available to read_material (0 = no text).
    parts: u32,
    text: TextStatus,
}

impl From<&MaterialView> for MaterialInfo {
    fn from(view: &MaterialView) -> Self {
        MaterialInfo {
            id: view.id.clone(),
            title: view.title.clone(),
            kind: view.kind,
            week: view.week_hint,
            module: view.module_name.clone(),
            published_at: view.published_at,
            url: public_url(view.url.as_deref()).map(str::to_string),
            parts: view.chunk_count,
            text: view.text_status,
        }
    }
}

#[derive(Serialize)]
struct DeadlineInfo {
    title: String,
    kind: EventKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    due_at: Option<Timestamp>,
    #[serde(skip_serializing_if = "Option::is_none")]
    starts_at: Option<Timestamp>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ends_at: Option<Timestamp>,
    #[serde(skip_serializing_if = "Option::is_none")]
    course: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    url: Option<String>,
}

impl From<&Deadline> for DeadlineInfo {
    fn from(deadline: &Deadline) -> Self {
        let event = &deadline.event;
        DeadlineInfo {
            title: event.title.clone(),
            kind: event.kind,
            due_at: event.due_at,
            starts_at: event.starts_at,
            ends_at: event.ends_at,
            course: deadline
                .course_code
                .clone()
                .or_else(|| deadline.course_name.clone()),
            url: event.url.clone(),
        }
    }
}

#[derive(Serialize)]
struct AnnouncementInfo {
    id: String,
    title: String,
    posted_at: Option<Timestamp>,
}

#[derive(Serialize)]
struct Overview {
    guidance: String,
    course: CourseInfo,
    ai_materials: AiMaterialsState,
    #[serde(skip_serializing_if = "Option::is_none")]
    note: Option<String>,
    timeline: TimelineInfo,
    current_modules: Vec<ModuleInfo>,
    recent_materials: Vec<MaterialInfo>,
    upcoming_deadlines: Vec<DeadlineInfo>,
    recent_announcements: Vec<AnnouncementInfo>,
    source: String,
    last_synced_at: Option<Timestamp>,
}

#[derive(Serialize)]
struct Week {
    guidance: String,
    course: CourseInfo,
    ai_materials: AiMaterialsState,
    week: Option<u32>,
    requested_week: Option<u32>,
    available_weeks: Vec<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    note: Option<String>,
    modules: Vec<ModuleInfo>,
    materials: Vec<MaterialInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    more: Option<String>,
}

#[derive(Serialize)]
struct DeadlineList {
    deadlines: Vec<DeadlineInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    more: Option<String>,
}

#[derive(Serialize)]
struct SavedPlan {
    saved: bool,
    id: i64,
    created_at: Timestamp,
    items: usize,
}

#[derive(Serialize)]
struct SyncInfo {
    sources: Vec<SourceInfo>,
    counts: StoreCounts,
    last_synced_at: Option<Timestamp>,
    stale: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    hint: Option<String>,
}

#[derive(Serialize)]
struct SourceInfo {
    id: String,
    label: String,
    kind: SourceKind,
    last_synced_at: Option<Timestamp>,
    last_error: Option<String>,
    last_error_kind: Option<SourceErrorKind>,
    stale: bool,
}
