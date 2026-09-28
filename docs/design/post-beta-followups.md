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

## Release pipeline (from the macOS and Windows signing work, 2026-09-27/28)

- **Split the signing jobs.** `app-macos`, `cli-macos`, `app-windows` and `cli-windows` build and
  sign on the same runner, so a build script or npm package could leave a process running or
  change `$GITHUB_PATH`/`$GITHUB_ENV` for the signing steps. On Windows it is worse: every step of
  a job with `id-token: write` has `ACTIONS_ID_TOKEN_REQUEST_URL/TOKEN` in its environment, so a
  build script could get the job's OIDC token and exchange it for an Azure token that can sign
  anything, anywhere, until it expires (60–90 minutes); signing out of Azure for the build doesn't
  prevent that. Split each into a build job (no environment, `contents: read`, no `id-token`)
  that uploads the built app, sidecar and CLI as an artifact, and a signing job
  (`environment: release`) that uses only first-party actions, the lockfile-pinned Tauri CLI and
  the pinned signing tools. `.github/workflows/release.yml`. Things to get right:
  - Split both Windows jobs together: the app's `beforeBuildCommand` builds the sidecar with the
    same `cargo build --release --locked -p pagelamp-cli --target x86_64-pc-windows-msvc` as
    `cli-windows`, so splitting only `cli-windows` keeps every one of those build scripts in a job
    that can get the token.
  - Artifact names are shared by every job in the run, including the Linux jobs (caches,
    unpinned actions). The signing job should download by the ID the build job passes on as a
    job output (`upload-artifact`'s `artifact-id`, `download-artifact`'s `artifact-ids`) and
    check the files against hashes from the build job's outputs before signing.
  - With a required reviewer, each signing job asks for approval when it starts, i.e. after its
    build; approving at the start of the run no longer covers it. The preflight and the Azure
    sign-in check would also move after the build, unless a small check job runs them first.
- **Pinned action commits and tools.** The signing jobs pin `pnpm/action-setup`,
  `dtolnay/rust-toolchain` and `azure/login` (v3.1.0) to a commit, and
  `.github/scripts/windows-signing.ps1` pins SignTool (`Microsoft.Windows.SDK.BuildTools`
  10.0.26100.4188) and the Artifact Signing dlib (`Microsoft.ArtifactSigning.Client` 1.0.128) by
  version, size and SHA-512 (the pair Microsoft's `Azure/artifact-signing-action` pins). Nothing
  updates those pins; bump them by hand (the tag's current commit; for NuGet, both packages
  together, with size and hash from nuget.org's catalog entry) or add Dependabot for GitHub
  Actions.
- **makensis doesn't check the uninstaller signature.** Tauri's NSIS template runs
  `!uninstfinalize` without a return-value check, so a failed signature would leave an unsigned
  `uninstall.exe` inside a signed installer. `app-windows` catches that by installing the
  `-setup.exe` on the runner and checking every installed file; worth an upstream fix in Tauri
  (`!uninstfinalize '…' = 0`).
- **What else Tauri signs.** With a signCommand, Tauri also signs the NSIS plugin DLLs (unpacked
  to a temporary folder while the installer runs) and the WiX extension DLLs it uses locally, with
  our certificate. Each signature is checked as it is made, but the workflow doesn't unpack the
  plugins from the finished installer. A release run uses about 15 signatures (Basic: 5,000 a
  month).
- **Linux jobs can write to the release.** `app` and `cli` (Linux) restore caches (pnpm,
  `Swatinem/rust-cache`, keys that `ci.yml` also writes on main), use unpinned third-party actions
  and have `contents: write`. `checksums` now refuses to write SHA256SUMS unless the draft holds
  exactly the signed files the signing jobs checked (each passes its hashes on as a job output) and
  every `.sha256` matches its archive, so a replaced signed file stops the release. A poisoned
  Linux job can still ship poisoned Linux files or add extra assets to the draft (check the asset
  list before publishing). Fix: no caches in the Linux jobs, or build them without the token and
  upload from a separate job.
- **Windows install check and WebView2.** The silent install downloads the WebView2 bootstrapper
  if the runner image lacks the runtime; a network error there fails the check (re-run).
- **MSI or NSIS only.** `tauri.conf.json` still builds both a `.msi` and a `-setup.exe` on Windows;
  the v0.3 research recommends NSIS only (frontend-owned file). The workflow signs and checks
  whichever it builds.
- **Windows publisher name.** Every Windows signature is checked against the variable
  `AZURE_ARTIFACT_SIGNING_PUBLISHER` (the certificate's CN). Update it if the validated identity
  changes; renew the identity validation in the Azure portal before it expires, or signing stops.
