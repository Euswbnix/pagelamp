# Follow-ups after v0.1.0-beta.1

Status: open list (release audit, 2026-09-26). Everything here was found by the pre-release audit
and its verification rounds, judged **not** to block the beta, and deliberately deferred. Each item
names where it lives; pick one, fix it with a test, and strike it through here.

## Robustness

- **Extraction worker (v0.2).** Text extraction runs in the sync process. PDF image data is never
  decoded and Flate/ASCII chains are measured, but drawn Form XObjects and page content streams
  under the 64 MB per-stream cap are still parsed in memory (measured ≈110× growth), and LZW-only
  streams aren't measured. Move extraction into `pagelamp extract-worker` with a memory limit
  (ARCHITECTURE §7). `crates/pagelamp-extract/src/{pdf.rs,pdf_inflate.rs}`
- **Atomic source removal.** `remove_source` deletes downloaded files course by course before the
  rows; a failure part-way leaves earlier courses' files gone. Stage deletions with renames into
  `files/.removing-<hash>/`, then commit. `crates/pagelamp-app/src/lib.rs`
- **Old course-code folders during sync.** After a Canvas course code changes, sync keeps the old
  `files/<OLDCODE>-<id>/` copy until the source is removed; prune or move it during sync.
  `crates/pagelamp-canvas/src/sync.rs`
- **Relink on hide/show.** Calendar events are linked when courses sync; hiding or showing a course
  doesn't re-run the tie-break (visible course wins). `crates/pagelamp-core/src/store.rs`
- **`course.toml` TOCTOU.** The symlink check and the open are separate syscalls; open with
  `O_NOFOLLOW` (and non-blocking for FIFOs) instead. `crates/pagelamp-local/src/folder.rs`
- **stdout closed, stdin open.** If an MCP client closes the server's stdout but keeps stdin open,
  `pagelamp mcp` keeps handling requests and logs an error per request instead of stopping.

## Privacy hardening (defence in depth; nothing leaks today)

- `clip()` redacts before flattening control characters, so a control character inside a token
  defeats redaction of AI-app client names. Flatten → redact → clip. `crates/pagelamp-mcp/src/lib.rs`
- A span-only `RUST_LOG` directive (e.g. `[serve_inner]=trace`) still enables rmcp events in the
  file log; drop `[`-directives from the file filter. `crates/pagelamp-core/src/diagnostics.rs`
- Study-plan tag neutralisation ignores zero-width/format characters and homoglyphs.
  `crates/pagelamp-mcp/src/format.rs`
- Unknown `course.toml` keys are quoted (≤40 chars) in warnings and reports.

## Correctness and wording

- Prompt references aren't escaped: a folder course without a code whose directory name contains
  `"` breaks the `(course "…")` instruction. `crates/pagelamp-mcp/src/text.rs`
- The display-name rule in `resolve_course_with` wins over an exact-code tie when a display name
  equals a bare code; the Ambiguous prompt error tells students to use a code that is ambiguous.
- Canvas address typed with full-width or ideographic dots (`q。utoronto。ca`) is refused by the
  typed-host check; normalise those dots first. `crates/pagelamp-canvas/src/lib.rs`
- A read-only data **directory** (WAL can't create `-shm`) gives "Internal error" instead of the
  "open the app once" text, and the startup log promises that text.
- Canvas 500/503 while adding a source is reported as an internal error; say Canvas is unavailable.
- `course term X --end D` alone clears an existing `--start` override (and vice versa); say so or
  keep the other bound.
- `status`/`doctor` "Courses: 3 (1 hidden)" reads as 3 total; it means 3 visible plus 1 hidden.
- A `last-crash.json` written by an older build after a broken pipe is still shown as a crash.

## Desktop app

- Close confirmation while a sync runs (needs a manual GUI test; see the batch-1 discussion).
- CI doesn't build the desktop app on Windows (release builds do).
- The Tauri app build in `release.yml` isn't `--locked` (the sidecar is).

## Release pipeline (from the macOS signing review, 2026-09-27)

- **Split the macOS signing jobs.** `app-macos` and `cli-macos` build and sign on the same
  runner, so a build script or npm package could leave a process running or change
  `$GITHUB_PATH`/`$GITHUB_ENV` for the signing steps. Split each into a build job (no environment,
  `contents: read`) that uploads the built app, sidecar and CLI as an artifact, and a signing job
  (`environment: release`) that uses only first-party actions and the lockfile-pinned Tauri CLI.
  `.github/workflows/release.yml`
- **Pinned action commits.** The signing jobs pin `pnpm/action-setup` and `dtolnay/rust-toolchain`
  to a commit, and nothing updates those pins; bump them by hand (the tag's current commit) or add
  Dependabot for GitHub Actions.
- **Windows signing.** The `.msi`, `-setup.exe` and `pagelamp.exe` are unsigned (SmartScreen
  "More info → Run anyway"); Azure Artifact Signing is the plan. Linux packages stay unsigned and
  are covered by `SHA256SUMS`.
