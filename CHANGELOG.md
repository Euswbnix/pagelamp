# Changelog

All notable changes are listed here. The project follows [Semantic Versioning](https://semver.org/)
(0.x: anything may change between minor versions).

## [0.1.0-beta.1] — unreleased

First public beta.

### Added
- Local course knowledge base (SQLite + full-text search) with text extraction for PDF, PowerPoint,
  Word, Jupyter notebooks, Markdown, HTML, plain text and source code; citations point to page,
  slide, cell or section.
- Sources: course folder (one sub-folder per course, week folders, optional `course.toml`),
  calendar feed (iCal/webcal, e.g. Canvas Calendar Feed), and Canvas with a personal access token
  (read-only; personal use only; files downloaded only on request).
- "Which week is this course in" inference with confidence and evidence.
- MCP server (`pagelamp mcp`) with 10 read-only tools plus `save_study_plan`, and the
  `weekly_review`, `catch_up` and `study_plan` prompts, for Claude Desktop, ChatGPT desktop /
  Codex and Claude Code.
- Per-course AI policy and AI-access switch; material text is withheld for "No AI" courses.
- `pagelamp` CLI and the desktop app (onboarding, sources & sync, courses, course detail,
  "Connect your AI app", settings), in English and 简体中文, light and dark.

### Known limitations
- Builds are not code-signed yet (see the README for first-launch steps).
- No reminders/notifications yet (planned for v0.2).
- Canvas access uses a personal token until an institution-approved sign-in is available.
