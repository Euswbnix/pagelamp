//! SQLite store.
//!
//! Concurrency model (deliberately simple — see docs/ARCHITECTURE.md "Concurrency"):
//! - One writer for synced data: the `studentos sync` process (CLI / future desktop app).
//! - Any number of MCP server processes (one per AI client) open the DB with
//!   `Store::open_read_only`. WAL mode lets readers proceed while a sync writes.
//! - The only write an MCP process performs is `save_study_plan`, via a short-lived
//!   read-write connection (`Store::open`) — short transactions + `busy_timeout` suffice.
//! - `Store` wraps a single `rusqlite::Connection` (not `Sync`). Async callers open one
//!   connection per request inside `spawn_blocking`; opening SQLite is sub-millisecond.
//!   There is intentionally no connection pool and no global mutex.
//!
//! Schema versioning: `PRAGMA user_version`. `open` migrates forward; `open_read_only`
//! refuses a DB whose version is newer than `SCHEMA_VERSION` (`Error::SchemaTooNew`) and
//! returns `Error::NotInitialised` for a missing file or version 0.

use std::path::Path;

use chrono::NaiveDate;
use rusqlite::Connection;

use crate::Result;
use crate::model::*;

pub const SCHEMA_VERSION: i64 = 1;

/// Version-1 schema. Applied by `open` when `user_version` is 0.
pub const SCHEMA_V1: &str = r#"
CREATE TABLE sources (
    id              TEXT PRIMARY KEY,
    kind            TEXT NOT NULL,              -- canvas | folder | ical
    label           TEXT NOT NULL,
    config_json     TEXT NOT NULL DEFAULT '{}',
    last_synced_at  TEXT,
    last_error      TEXT,
    last_error_kind TEXT                        -- SourceErrorKind (snake_case) or NULL
);

CREATE TABLE courses (
    id               TEXT PRIMARY KEY,
    source_id        TEXT NOT NULL REFERENCES sources(id) ON DELETE CASCADE,
    external_id      TEXT NOT NULL,
    code             TEXT,
    name             TEXT NOT NULL,
    term_start       TEXT,                      -- synced (YYYY-MM-DD)
    term_end         TEXT,
    user_term_start  TEXT,                      -- user override, wins over synced
    user_term_end    TEXT,
    url              TEXT,
    syllabus_text    TEXT,
    ai_policy        TEXT NOT NULL DEFAULT 'unknown',
    ai_policy_note   TEXT,
    hidden           INTEGER NOT NULL DEFAULT 0,
    updated_at       TEXT NOT NULL
);
CREATE INDEX courses_source ON courses(source_id);

CREATE TABLE modules (
    id          TEXT PRIMARY KEY,
    course_id   TEXT NOT NULL REFERENCES courses(id) ON DELETE CASCADE,
    name        TEXT NOT NULL,
    position    INTEGER,
    unlock_at   TEXT,
    week_hint   INTEGER
);
CREATE INDEX modules_course ON modules(course_id);

CREATE TABLE materials (
    id            TEXT PRIMARY KEY,
    course_id     TEXT NOT NULL REFERENCES courses(id) ON DELETE CASCADE,
    module_id     TEXT REFERENCES modules(id) ON DELETE SET NULL,
    kind          TEXT NOT NULL,
    title         TEXT NOT NULL,
    url           TEXT,
    local_path    TEXT,
    mime          TEXT,
    published_at  TEXT,
    week_hint     INTEGER,
    content_hash  TEXT,
    text_status   TEXT NOT NULL DEFAULT 'pending',
    text_error    TEXT,
    updated_at    TEXT NOT NULL
);
CREATE INDEX materials_course ON materials(course_id);

CREATE TABLE chunks (
    id           INTEGER PRIMARY KEY,
    material_id  TEXT NOT NULL REFERENCES materials(id) ON DELETE CASCADE,
    ord          INTEGER NOT NULL,
    locator      TEXT,
    text         TEXT NOT NULL,
    UNIQUE(material_id, ord)
);

-- External-content FTS5 index over chunks.text, kept in sync by triggers.
CREATE VIRTUAL TABLE chunks_fts USING fts5(
    text,
    content = 'chunks',
    content_rowid = 'id',
    tokenize = 'porter unicode61 remove_diacritics 2'
);
CREATE TRIGGER chunks_ai AFTER INSERT ON chunks BEGIN
    INSERT INTO chunks_fts(rowid, text) VALUES (new.id, new.text);
END;
CREATE TRIGGER chunks_ad AFTER DELETE ON chunks BEGIN
    INSERT INTO chunks_fts(chunks_fts, rowid, text) VALUES ('delete', old.id, old.text);
END;
CREATE TRIGGER chunks_au AFTER UPDATE ON chunks BEGIN
    INSERT INTO chunks_fts(chunks_fts, rowid, text) VALUES ('delete', old.id, old.text);
    INSERT INTO chunks_fts(rowid, text) VALUES (new.id, new.text);
END;

CREATE TABLE events (
    id          TEXT PRIMARY KEY,
    source_id   TEXT NOT NULL REFERENCES sources(id) ON DELETE CASCADE,
    course_id   TEXT REFERENCES courses(id) ON DELETE SET NULL,
    kind        TEXT NOT NULL,
    title       TEXT NOT NULL,
    starts_at   TEXT,
    ends_at     TEXT,
    due_at      TEXT,
    url         TEXT,
    updated_at  TEXT NOT NULL
);
CREATE INDEX events_course ON events(course_id);

CREATE TABLE study_plans (
    id          INTEGER PRIMARY KEY,
    created_at  TEXT NOT NULL,
    plan_json   TEXT NOT NULL
);
"#;

pub struct Store {
    conn: Connection,
}

impl Store {
    // ----- opening -----------------------------------------------------------------------

    /// Open read-write, creating the file if needed. Sets `journal_mode=WAL`,
    /// `foreign_keys=ON`, `busy_timeout=5000`, then migrates to `SCHEMA_VERSION`.
    pub fn open(path: &Path) -> Result<Self> {
        let _ = path;
        todo!()
    }

    /// Open read-only (`SQLITE_OPEN_READ_ONLY`), `busy_timeout=5000`, `query_only=ON`.
    /// Errors: `NotInitialised` if the file is missing or `user_version == 0`;
    /// `SchemaTooNew` if `user_version > SCHEMA_VERSION`.
    pub fn open_read_only(path: &Path) -> Result<Self> {
        let _ = path;
        todo!()
    }

    /// Fresh in-memory DB with the schema applied (tests).
    pub fn open_in_memory() -> Result<Self> {
        todo!()
    }

    /// Run `f` inside `BEGIN IMMEDIATE … COMMIT` on this connection (ROLLBACK on error).
    /// All `Store` methods used inside `f` share the transaction.
    pub fn in_transaction<T>(&self, f: impl FnOnce(&Store) -> Result<T>) -> Result<T> {
        let _ = f;
        todo!()
    }

    // ----- sources -----------------------------------------------------------------------

    /// Insert or update id/kind/label/config (does not touch last_synced_at/last_error/
    /// last_error_kind).
    pub fn upsert_source(&self, source: &SourceRecord) -> Result<()> {
        let _ = source;
        todo!()
    }
    pub fn get_source(&self, id: &str) -> Result<Option<SourceRecord>> {
        let _ = id;
        todo!()
    }
    pub fn list_sources(&self) -> Result<Vec<SourceRecord>> {
        todo!()
    }
    /// Deletes the source and (via cascade) its courses, materials, chunks, events.
    pub fn remove_source(&self, id: &str) -> Result<()> {
        let _ = id;
        todo!()
    }
    /// Record a sync attempt: `error = None` → success (sets last_synced_at, clears last_error
    /// and last_error_kind); `Some((kind, msg))` → failure (sets last_error + last_error_kind,
    /// leaves last_synced_at unchanged).
    pub fn record_sync(
        &self,
        id: &str,
        at: Timestamp,
        error: Option<(SourceErrorKind, &str)>,
    ) -> Result<()> {
        let _ = (id, at, error);
        todo!()
    }
    /// Clear last_error/last_error_kind without touching last_synced_at (used after the user
    /// replaced an expired token / feed URL).
    pub fn clear_source_error(&self, id: &str) -> Result<()> {
        let _ = id;
        todo!()
    }

    // ----- courses -----------------------------------------------------------------------

    /// Insert or update synced fields only; never touches ai_policy, ai_policy_note,
    /// user_term_*, hidden. Sets updated_at = now.
    pub fn upsert_course(&self, course: &CourseUpsert) -> Result<()> {
        let _ = course;
        todo!()
    }
    /// All courses (including hidden ones when `include_hidden`), ordered by code, name.
    pub fn list_courses(&self, include_hidden: bool) -> Result<Vec<Course>> {
        let _ = include_hidden;
        todo!()
    }
    pub fn get_course(&self, id: &str) -> Result<Option<Course>> {
        let _ = id;
        todo!()
    }
    /// Resolve a user/AI-supplied course reference. Tries, in order: exact id; exact code
    /// (case-insensitive, ignoring spaces); unique code prefix ("csc413" → "CSC413H1");
    /// unique case-insensitive substring of name. Hidden courses are excluded.
    /// Errors: `NotFound` (message lists available codes) or `Ambiguous`.
    pub fn resolve_course(&self, query: &str) -> Result<Course> {
        self.resolve_course_with(query, false)
    }
    /// Like `resolve_course`, optionally including hidden courses (course-settings commands
    /// must be able to address a hidden course, e.g. to un-hide it).
    pub fn resolve_course_with(&self, query: &str, include_hidden: bool) -> Result<Course> {
        let _ = (query, include_hidden);
        todo!()
    }
    /// Delete courses of `source_id` whose id is not in `keep` (course dropped/ended).
    pub fn prune_courses(&self, source_id: &str, keep: &[String]) -> Result<usize> {
        let _ = (source_id, keep);
        todo!()
    }
    pub fn set_course_policy(
        &self,
        course_id: &str,
        policy: AiPolicy,
        note: Option<&str>,
    ) -> Result<()> {
        let _ = (course_id, policy, note);
        todo!()
    }
    /// Set/clear user term overrides.
    pub fn set_course_term(
        &self,
        course_id: &str,
        start: Option<NaiveDate>,
        end: Option<NaiveDate>,
    ) -> Result<()> {
        let _ = (course_id, start, end);
        todo!()
    }
    pub fn set_course_hidden(&self, course_id: &str, hidden: bool) -> Result<()> {
        let _ = (course_id, hidden);
        todo!()
    }
    pub fn course_syllabus_text(&self, course_id: &str) -> Result<Option<String>> {
        let _ = course_id;
        todo!()
    }

    // ----- modules -----------------------------------------------------------------------

    /// Replace all modules of a course (delete + insert). Materials referencing removed
    /// modules get module_id = NULL via FK.
    pub fn replace_modules(&self, course_id: &str, modules: &[Module]) -> Result<()> {
        let _ = (course_id, modules);
        todo!()
    }
    /// Ordered by position (NULLs last), then name.
    pub fn list_modules(&self, course_id: &str) -> Result<Vec<Module>> {
        let _ = course_id;
        todo!()
    }

    // ----- materials & chunks ------------------------------------------------------------

    /// Insert (text_status = pending) or update the synced columns of a material.
    /// On update, content_hash/text_status/text_error are preserved — EXCEPT when
    /// `local_path` changed, in which case text_status is reset to pending.
    pub fn upsert_material(&self, material: &MaterialUpsert) -> Result<()> {
        let _ = material;
        todo!()
    }
    pub fn get_material(&self, id: &str) -> Result<Option<Material>> {
        let _ = id;
        todo!()
    }
    /// Ordered by week_hint (NULLs last), published_at, title.
    pub fn list_materials(&self, course_id: &str) -> Result<Vec<Material>> {
        let _ = course_id;
        todo!()
    }
    /// Delete materials of the course whose id is not in `keep` (chunks cascade).
    pub fn prune_materials(&self, course_id: &str, keep: &[String]) -> Result<usize> {
        let _ = (course_id, keep);
        todo!()
    }
    /// Update index state of one material. `content_hash = None` leaves the stored hash as is.
    pub fn set_text_state(
        &self,
        material_id: &str,
        status: TextStatus,
        error: Option<&str>,
        content_hash: Option<&str>,
    ) -> Result<()> {
        let _ = (material_id, status, error, content_hash);
        todo!()
    }
    /// Replace all chunks of a material (FTS kept in sync by triggers).
    pub fn replace_chunks(&self, material_id: &str, chunks: &[Chunk]) -> Result<()> {
        let _ = (material_id, chunks);
        todo!()
    }
    /// Chunks ordered by ord, optionally a window [from_ord, from_ord+limit).
    pub fn get_chunks(
        &self,
        material_id: &str,
        from_ord: u32,
        limit: Option<u32>,
    ) -> Result<Vec<Chunk>> {
        let _ = (material_id, from_ord, limit);
        todo!()
    }
    pub fn chunk_count(&self, material_id: &str) -> Result<u32> {
        let _ = material_id;
        todo!()
    }

    // ----- events --------------------------------------------------------------------------

    /// Upsert the given events of a source and delete that source's other events
    /// whose id is not in the list (the list is the source's complete current set).
    pub fn replace_events(&self, source_id: &str, events: &[Event]) -> Result<()> {
        let _ = (source_id, events);
        todo!()
    }
    /// Events whose `when()` (due_at, else starts_at) falls in [from, to], optionally for one
    /// course, ordered by that instant. Events with neither date are excluded.
    pub fn list_events(
        &self,
        from: Timestamp,
        to: Timestamp,
        course_id: Option<&str>,
    ) -> Result<Vec<Event>> {
        let _ = (from, to, course_id);
        todo!()
    }

    // ----- search --------------------------------------------------------------------------

    /// Full-text search over chunks. `query` is free text from a user/AI: it must be
    /// sanitised into a safe FTS5 MATCH expression (quote each term, OR them; never pass
    /// raw user syntax to MATCH). Hidden courses excluded. Ordered by bm25, then limited.
    pub fn search(
        &self,
        query: &str,
        course_id: Option<&str>,
        limit: u32,
    ) -> Result<Vec<SearchHit>> {
        let _ = (query, course_id, limit);
        todo!()
    }

    // ----- study plans ---------------------------------------------------------------------

    pub fn save_study_plan(&self, plan: &StudyPlan) -> Result<StoredStudyPlan> {
        let _ = plan;
        todo!()
    }
    pub fn latest_study_plan(&self) -> Result<Option<StoredStudyPlan>> {
        todo!()
    }

    // ----- statistics ----------------------------------------------------------------------

    pub fn counts(&self) -> Result<StoreCounts> {
        todo!()
    }

    /// Escape hatch for tests/migrations.
    pub fn conn(&self) -> &Connection {
        &self.conn
    }
}
