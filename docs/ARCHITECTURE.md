# PageLamp v0.1 — Architecture & team contract

Status: agreed baseline (leader session, 2026-09-25). Changes to anything in §3–§5 go through the leader.

## 1. Product scope (v0.1 = "MCP-first")

PageLamp syncs a student's **own** course data to a local SQLite knowledge base and serves it over
**MCP** to the AI app the student already pays for (Claude Desktop, Claude Code, Codex, …). The
student's subscription supplies the model; PageLamp runs no model and no server.

In scope: study plans, weekly content explanations, "where is each course this week", deadlines for
planning, catching up. **Out of scope:** fetching assignment instructions to solve them, submitting
anything, any write to the LMS, a chat UI of our own, our own backend server.

Target users: students outside mainland China (Canvas first; folder + calendar-feed sources make it
work for Brightspace/Moodle schools too). UI English-first, zh-CN second.

## 2. Components

```
                        ┌──────────── student's AI app (their subscription) ────────────┐
                        │ Claude Desktop      Claude Code        Codex        Cursor …  │
                        └──────┬──────────────────┬─────────────────┬───────────────────┘
                     spawns    │ stdio MCP        │                 │   (one process per client)
                               ▼                  ▼                 ▼
                        pagelamp mcp       pagelamp mcp     pagelamp mcp
                               │   read-only, per-request connection (WAL)
                               ▼
   pagelamp sync ───►  <data_dir>/pagelamp.db  (+ <data_dir>/files/ cache)
   (CLI or desktop app,        ▲   single writer, advisory sync.lock
    the only network user)     │
   Canvas API (GET only) ──────┤
   Local course folder ────────┤
   iCal feed (deadlines) ──────┘
```

Rust workspace:

| Crate | Purpose | Owner |
|---|---|---|
| `crates/pagelamp-core` | model, store (SQLite + FTS5), ingest, timeline, views, paths, secrets | backend |
| `crates/pagelamp-extract` | PDF/PPTX/DOCX/ipynb/HTML/text extraction + chunking | backend |
| `crates/pagelamp-canvas` | read-only Canvas sync (personal token) | backend |
| `crates/pagelamp-local` | folder source + iCal source | backend |
| `crates/pagelamp-mcp` | rmcp 3.4 stdio server: tools + prompts | backend |
| `crates/pagelamp-app` | **the facade** used by CLI and desktop app | backend |
| `apps/pagelamp-cli` | `pagelamp` binary | backend |
| `apps/desktop` (+ `src-tauri`) | Tauri 2 + React desktop shell | frontend |
| root `Cargo.toml`, `docs/` | workspace + contracts | leader |
| root `package.json`, `pnpm-workspace.yaml` (if any) | JS tooling | frontend |

## 3. Hard rules (product/policy requirements)

1. **Canvas access is GET-only**, from `pagelamp-canvas` only, triggered by sync only. The MCP server
   never touches the network (Canvas API Policy §3(i) forbids accessing Canvas APIs via unapproved
   MCP servers).
2. **Personal token = personal use.** Instructure: asking other users to manually generate a token
   for your app violates the API Policy. UI and CLI must say so when adding a Canvas source; the
   folder + calendar-feed path is the shareable one. Tokens expire (the maximum is set per school/role and shown by Canvas, e.g. 90 days at UofT for this user; Instructure caps student-only users at 30) — surface
   expiry/401 clearly.
3. **Secrets** (Canvas token, calendar-feed URL) live only in the OS keychain (`core::secrets`);
   never in the DB, logs, MCP output, frontend state beyond the input field, or test fixtures.
4. **No assignment solving.** Assignments are recorded as title + due date + link only.
5. **Untrusted content.** Course text returned over MCP is wrapped in `<course_material …>` tags and
   the server instructions say it is data, not instructions.
6. **AI disclosure** (Canvas API Policy §2E): UI states that course text is sent to whatever AI app
   the student connects, and that PageLamp itself stores nothing remotely.
7. **Test data is synthetic.** No real course materials, names, or tokens anywhere in the repo.
8. **AI access to course materials is the student's choice, on by default** (decided 2026-09-25).
   - One-time disclosure at onboarding (and printed by `pagelamp mcp-config`): when the student
     asks their AI app about a course, the app reads that course's materials from PageLamp and
     sends them to the AI provider under the student's own account; the student is responsible
     for following each course's AI policy; sharing can be turned off per course.
   - Per-course switch `ai_access` ("Let my AI app read this course's materials"), default **on**.
   - If the student marks a course `ai_policy = prohibited`, its material **text** is withheld from
     MCP regardless of the switch (the switch keeps its stored value and applies again if the policy
     changes). Structure, titles, deadlines and the study plan remain available for planning.
   - Effective state is computed, never stored: `AiMaterialsState = readable | turned_off |
     withheld_by_policy`.
   - "Material text" = `read_material` content, `search_materials` snippets, announcement bodies,
     syllabus text, and any text a prompt would inline. Titles, kinds, dates, week numbers, URLs and
     counts are structure and stay available.

## 4. Concurrency (why there is no connection pool)

- stdio MCP is 1:1 — each AI client spawns its own `pagelamp mcp` process; nothing is shared.
- The DB is SQLite in WAL mode: many readers, one writer, readers never block each other.
- MCP opens a fresh read-only connection per request inside `spawn_blocking` (sub-ms). No global
  mutex, never hold a connection across `.await`.
- Heavy work (download, extract, chunk, index) happens at sync time only; MCP tools are indexed reads.
- `save_study_plan` is the only MCP write: short read-write transaction + `busy_timeout=5000`.
- `pagelamp mcp` must stay lightweight at startup (no model loading, no network).

## 5. App facade API (`pagelamp-app`) — the backend ⇄ frontend contract

Backend implements; frontend's Tauri commands are thin 1:1 wrappers. Names below are agreed; field
details may be refined by the backend, who then regenerates schemas and tells the frontend.
All return types are `Serialize + JsonSchema`; `pagelamp schema` prints them as JSON Schema →
frontend generates TS with `json-schema-to-typescript` (no hand-written duplicate types).

```rust
pub struct App { /* data_dir */ }
impl App {
    pub fn open() -> Result<App>;                          // default data dir (PAGELAMP_HOME respected)
    pub fn open_at(data_dir: PathBuf) -> Result<App>;

    // status & sources
    pub fn status(&self) -> Result<AppStatus>;            // data_dir, db_path, sources, counts, last sync
    pub fn list_sources(&self) -> Result<Vec<SourceRecord>>;
    pub async fn add_canvas_source(&self, base_url: &str, token: &str) -> Result<SourceRecord>; // validates token
    pub fn add_folder_source(&self, path: &Path, term_start: Option<NaiveDate>, label: Option<&str>) -> Result<SourceRecord>;
    pub async fn add_ical_source(&self, feed_url: &str, label: Option<&str>) -> Result<SourceRecord>; // validates by fetching
    pub fn remove_source(&self, source_id: &str) -> Result<()>;  // also deletes its secret
    /// Replace an expired/revoked Canvas token or a changed feed URL WITHOUT removing the
    /// source (remove cascades to courses + user overrides). Validates exactly like add_*,
    /// then overwrites the keychain entry and clears last_error/last_error_kind.
    pub async fn update_source_secret(&self, source_id: &str, secret: &str) -> Result<SourceRecord>;

    // sync (progress streamed to the UI; desktop forwards via tauri::ipc::Channel)
    pub async fn sync_all(&self, req: SyncRequest, on_event: impl Fn(SyncEvent) + Send + Sync) -> Result<SyncSummary>;
    pub async fn sync_source(&self, source_id: &str, req: SyncRequest, on_event: impl Fn(SyncEvent) + Send + Sync) -> Result<SourceSyncResult>;

    // read views (same functions back the MCP tools)
    pub fn list_courses(&self) -> Result<Vec<CourseSummary>>;           // course + timeline + counts + next deadline
    pub fn course_overview(&self, course: &str) -> Result<CourseOverview>;
    pub fn week_materials(&self, course: &str, week: Option<u32>) -> Result<WeekMaterials>;
    pub fn list_deadlines(&self, course: Option<&str>, days_ahead: u32, days_back: u32) -> Result<Vec<Event>>;
    pub fn search(&self, query: &str, course: Option<&str>, limit: u32) -> Result<Vec<SearchHit>>;
    pub fn latest_study_plan(&self) -> Result<Option<StoredStudyPlan>>;

    // course settings
    pub fn set_course_policy(&self, course: &str, policy: AiPolicy, note: Option<&str>) -> Result<()>;
    pub fn set_course_term(&self, course: &str, start: Option<NaiveDate>, end: Option<NaiveDate>) -> Result<()>;
    pub fn set_course_hidden(&self, course: &str, hidden: bool) -> Result<()>;
    pub fn set_course_ai_access(&self, course: &str, allowed: bool) -> Result<()>;  // §3 rule 8

    // "connect your AI app"
    pub fn mcp_client_configs(&self, pagelamp_binary: &Path) -> Vec<McpClientConfig>;
}

pub enum SyncEvent {            // serde tag = "type"
    SourceStarted { source_id, label },
    Progress { source_id, message, current: Option<u32>, total: Option<u32> },
    Warning { source_id, message },
    SourceFinished { source_id, ok: bool, error: Option<String>, error_kind: Option<SourceErrorKind> },
}

/// Structured failure classes — the UI branches on these, never on message strings.
/// Persisted on the source row as `last_error_kind` next to `last_error`
/// (`SourceRecord.last_error_kind`, schema v1 column `sources.last_error_kind TEXT`).
pub enum SourceErrorKind {      // serde snake_case
    AuthExpiredOrRevoked,       // Canvas 401 / invalid_token; feed URL 401/403
    Network,                    // DNS, TLS, timeout, connection refused
    NotFound,                   // folder missing, feed 404, Canvas host 404
    RateLimited,                // Canvas throttling exhausted retries
    Other,
}

/// Every facade method returns `Result<T, AppError>`; Tauri commands serialise it as-is.
pub struct AppError {           // Serialize + JsonSchema
    kind: AppErrorKind,         // auth | network | invalid | not_found | ambiguous | busy | internal
    message: String,            // user-presentable, never contains secrets
}
// `busy` = another process holds sync.lock (CLI vs desktop).
pub struct McpClientConfig {    // one per client: claude_desktop | claude_code | codex | generic
    client, title, install_kind /* json_snippet | shell_command | toml_snippet */,
    config_path_hint: Option<String>, content: String, notes: Vec<String>,
}
```

Read-view types (`CourseSummary`, `CourseOverview`, `WeekMaterials`, `MaterialView`) live in
`pagelamp_core::views` so the MCP server can use them over a read-only store.

Additions agreed 2026-09-25 (after the first draft of this section):
- `Course.term_source: TermSource` = `user | synced | none`.
- `Course.ai_access: bool` (stored, default true; schema v1 column `courses.ai_access INTEGER NOT
  NULL DEFAULT 1`) and `CourseSummary.ai_materials` / `CourseOverview.ai_materials:
  AiMaterialsState` (computed: `readable | turned_off | withheld_by_policy`; `prohibited` policy
  wins over the switch). "Materials readable by your AI app" counts are 0 unless `readable`.
- `WeekMaterials.note_kind: Option<WeekNoteKind>` and `McpClientConfig.note_codes: Vec<McpNoteCode>`
  — stable codes next to the English text so the UI can localise.

## 6. Desktop app (frontend) baseline

- `apps/desktop`: pnpm, Vite, React 19, TypeScript strict, Tailwind v4 + shadcn/ui, lucide icons,
  React Router, TanStack Query (command calls/caching) + Zustand (UI state), react-i18next (en default,
  zh-CN), Biome (lint+format), Vitest + Testing Library.
- `src/api/`: one typed interface with two implementations — `tauri` (invoke/Channel) and `mock`
  (synthetic "DEMO101 — Intro to Demo Studies" fixture), selected by `VITE_API=mock`, so the UI is
  built and tested in a plain browser before the Rust facade lands.
- `src-tauri`: Tauri 2; commands = thin wrappers over `pagelamp_app::App`; no business logic;
  no `shell:*` permissions granted to the frontend. Once it compiles, add
  `"apps/desktop/src-tauri"` to root workspace `members` (the only root-file edit allowed).
- Screens v0.1: Welcome/onboarding (pick source: Folder+Calendar feed [recommended, shareable] /
  Canvas token [personal use notice]) · Sources & Sync (status, progress, errors, token-expiry
  hints) · Courses (code, name, current week + confidence, next deadline, AI-policy badge) ·
  Course detail (timeline evidence, this week's materials, deadlines, AI-policy editor, term
  override, hide) · Connect your AI app (cards from `mcp_client_configs`, copy buttons) ·
  Settings/About (data dir, privacy + AI disclosure). **No chat UI.**

## 7. Roadmap (decided 2026-09-25)

License: **Apache-2.0** (root `LICENSE`; `license.workspace = true` in every crate, `"license":
"Apache-2.0"` in package.json). Dependencies must be Apache-2.0/MIT/BSD/ISC/Zlib/Unicode-compatible —
no GPL/AGPL/SSPL/BUSL/FSL crates or npm packages (a `cargo deny` license check will enforce this in CI).

| Version | Scope |
|---|---|
| **v0.1 MCP-first** (now) | core + sources + MCP server + CLI; desktop shell for onboarding/sources/courses/connect |
| v0.2 | local deterministic reminders (deadlines ≤ 48h, Monday "this week"), Claude Desktop `.mcpb` one-click extension, CI + signed releases + Tauri updater (GitHub Releases), bilingual README/CONTRIBUTING, text extraction in a resource-limited child process (`pagelamp extract-worker`: PDF decompression-bomb / parser-crash isolation), browser Connector decision (see spike), "Past courses" + explicit cleanup for Canvas courses no longer active, Canvas downloads via `files/:id/public_url` + signed URL if verified not to complete "must view" requirements |
| **v0.3 "full" PageLamp (embedded model access)** | PageLamp generates study plans / weekly explanations itself, via a provider abstraction: ChatGPT subscription through the bundled **unmodified official Codex** (`codex exec` stable / app-server experimental, Codex-managed "Sign in with ChatGPT"); Claude subscription through the student's **unmodified Claude Code** in headless mode (paid plans only; requires accepting Anthropic Commercial Terms + written confirmation first); BYOK API keys; local models. Never proxy/resell usage, never handle subscription tokens. |
| after v0.3 | branded distributions (first: UTMCSSA Academic Dept) via brand config — no fork |
| later (macOS) | native Swift companion (menu bar, WidgetKit "this week" widget, scheduled notifications, Shortcuts/App Intents) and possibly a full SwiftUI app — **over the same Rust core via UniFFI**, never a Swift re-implementation of core/sync/policy logic. Keep `pagelamp-app` FFI-friendly (plain serialisable types, no Tauri types in its API). Prefer direct distribution + notarization over the Mac App Store (sandbox vs course folders and AI-app-launched MCP; review of third-party-service access). |

## 8. Collaboration rules

- Stay inside your owned paths (§2). Need a change elsewhere → message the owner (cc leader for
  contract changes in §3–§5).
- Definition of done — backend: `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace` green; frontend: `pnpm run typecheck && pnpm run lint && pnpm run test && pnpm run build`
  green, `pnpm tauri dev` launches.

### Git workflow (user decision 2026-09-25: commit locally, push every 5 commits)

All sessions share ONE working tree and ONE local `main`. The repo is public.
- **Commit** each finished logical change yourself (small, focused commits). Before committing,
  the checks of your area must pass (DoD above, scoped to what you changed at minimum).
- **Commit with a pathspec, never a bare `git commit`** (the index is shared; a bare commit takes
  whatever anyone staged — this happened once in 810e415). Owned paths: backend `crates/
  apps/pagelamp-cli/`; frontend `apps/desktop/`; leader: root files, `docs/`, `spikes/`.
  Recipe: `git add -N <new files in your paths>` (intent-to-add, so pathspec commits see them),
  then `git commit -F msg -- <your paths>`. A pathspec commit records only those paths (their
  working-tree content) and leaves anything else in the index untouched. Don't leave files staged.
  Never `git add -A`, `git add .`, `git commit -a`. Verify with `git show --stat HEAD` right after.
- **`Cargo.lock` is committed by whoever's manifest change caused the lock change, in the same
  pathspec commit** (`git commit -- crates/… Cargo.lock` or `-- apps/desktop/ Cargo.lock`), so no
  commit has a manifest/lock mismatch. Before including it, check `git diff Cargo.lock` contains
  only your change; if it also has the other side's uncommitted entries, message them and agree who
  commits first.
- **Push when ≥ 5 local commits are ahead**: `git fetch origin && git rev-list --count origin/main..main`;
  if ≥ 5 → run the full DoD for your area once more, scan staged history for secrets, then
  `git push origin main`. Whoever makes the 5th commit pushes (including others' commits — they
  were checked by their owners). Never force-push; never rewrite pushed history.
- `.git/index.lock` exists → another session is committing; wait a few seconds and retry. Never
  delete the lock file.
- Commit messages: imperative subject ≤ 72 chars with an area prefix (`core:`, `extract:`,
  `canvas:`, `local:`, `mcp:`, `app:`, `cli:`, `desktop:`, `docs:`, `spike:`), body explains why.
  **No `Co-Authored-By: Claude` trailers and no "Generated with Claude Code" lines** (explicit
  user preference). Commits are authored by the configured git identity; don't change git config.
- Tooling note: `/usr/local/bin/gh` is an x86_64 build that breaks when it shells out to git — use
  plain `git push` (credential helper works) and `gh api` for GitHub API calls.
- Report milestones to the leader with what changed, how it was verified, and open questions.
