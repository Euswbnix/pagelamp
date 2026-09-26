# Changelog

All notable changes are listed here. The project follows [Semantic Versioning](https://semver.org/)
(0.x: anything may change between minor versions).

## [0.1.0-beta.1] — unreleased

First public beta.

### Added
- Local course knowledge base (SQLite + full-text search) with text extraction for PDF, PowerPoint
  (.pptx), Word (.docx), Jupyter notebooks, Markdown, HTML, plain text and source code; citations point to page,
  slide, cell or section.
- Sources: course folder (one sub-folder per course, week folders, optional `course.toml`),
  calendar feed (iCal/webcal, e.g. Canvas Calendar Feed), and Canvas with a personal access token
  (read-only; personal use only; files downloaded only on request).
- "Which week is this course in" inference with confidence and evidence.
- MCP server (`pagelamp mcp`) with 9 read-only tools plus `save_study_plan` (saves only to your local database), and the
  `weekly_review`, `catch_up` and `study_plan` prompts, for Claude Desktop, ChatGPT desktop /
  Codex and Claude Code.
- Per-course AI policy and AI-access switch; material text is withheld for "No AI" courses.
- `pagelamp` CLI and the desktop app (onboarding, sources & sync, courses, course detail,
  "Connect your AI app", settings), in English and 简体中文, light and dark.

### Upgrading from an earlier build
- After installing a new version, open PageLamp once (or run `pagelamp sync`), then restart your
  AI app so it starts the new `pagelamp`.

### Known limitations
- Builds are not code-signed yet (see the README for first-launch steps).
- No reminders/notifications yet (planned for v0.2).
- Canvas access uses a personal token until an institution-approved sign-in is available.
- On macOS and Windows the desktop app doesn't put `pagelamp` on your PATH (use the full path shown
  in *Connect your AI app*); the Linux `.AppImage` can't serve your AI app — use the `.deb`/`.rpm`.
- Older `.ppt`/`.doc` files and scanned PDFs (no text layer) are listed but not searchable.
- Windows and Linux builds are x86_64 only.
- Text extraction runs in the sync process. PDFs are checked for decompression bombs first, but
  image streams, LZW-compressed and encrypted streams aren't measured, so a deliberately crafted
  file can still use a lot of memory (an isolated extraction worker is planned for v0.2).
- Installer file names and the OS-level app version show `0.1.0` for every 0.1.0 beta; the real
  version is in *Settings → About* or `pagelamp --version`.
