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

use std::path::PathBuf;

/// Run the MCP server over stdio until the client disconnects.
/// `db_path` is normally `studentos_core::paths::db_path()`.
pub async fn serve_stdio(db_path: PathBuf) -> anyhow::Result<()> {
    let _ = db_path;
    todo!()
}
