# Facade requests from the macOS app

Changes to `pagelamp-app` / `pagelamp-core` that the Mac work would like. Nothing here is
implemented; each entry says what the Mac side does meanwhile. (Design-level additions are in
`docs/design/macos-shell.md` §13.)

## From the Rust↔Swift bridge (`crates/pagelamp-ffi`)

### F1. A typed error kind for database schema mismatches
- **Request:** `AppErrorKind::Schema` (or `DatabaseTooNew` / `NeedsMigration`) for
  `pagelamp_core::Error::SchemaTooNew` / `SchemaTooOld`, which `From<pagelamp_core::Error>`
  currently folds into `Internal`.
- **Why:** S2 ("backend unavailable") should say "this data was written by a newer PageLamp —
  update the app" instead of a generic failure.
- **Meanwhile:** `pagelamp-ffi` re-classifies `Internal` errors whose message matches core's own
  `Display` of those two variants (rendered at run time with sentinel numbers, pinned by the test
  `error::tests::schema_errors_from_core_become_schema`) as `PageLampError.Schema`.

### F2. `diagnostics::init_in(data_dir, kind, verbose)` in `pagelamp-app`
- **Request:** a facade twin of `pagelamp_core::diagnostics::init` for an explicit data dir, like
  the facade's other `*_in` helpers.
- **Why:** the Mac app can open a non-default data dir (`PageLamp.open(dataDir:)`); its logs must
  go to that dir.
- **Meanwhile:** `init_diagnostics(verbose, data_dir)` calls `pagelamp_core::diagnostics::init`
  directly when a dir is given, `pagelamp_app::diagnostics::init` otherwise.

### F3. Keep `Event.course_hint` out of the public facade type
- **Request:** move the sync-internal `course_hint` (`#[serde(skip)]`) out of the public `Event`
  (e.g. a separate upsert type), or document it as always `None` in facade results.
- **Why:** a UniFFI mirror must list every field, so Swift's `Event` has a `courseHint` that is
  always nil.
- **Meanwhile:** mirrored and documented as always nil.

### F4. Cancellation of a running sync
- **Request:** a way to stop `sync_all` / `sync_source` / `download_course_files` (e.g. a cancel
  token argument or `App::cancel_sync()`).
- **Why:** UniFFI cannot cancel a Rust future when the Swift `Task` is cancelled; quitting the app
  is the only way to stop a long Canvas download today.
- **Meanwhile:** no Stop button (not in the M1 design).

## From the M1 review

### F5. A read-only schema check before opening a data folder
- **Request:** `pagelamp_app::schema_status_in(data_dir) -> SchemaStatus` (or `App::check_at`),
  which opens `pagelamp.db` read-only and answers, without creating, migrating or writing
  anything: `Missing` (no database yet), `Current`, `NeedsMigration { found, supported }` (an older
  PageLamp wrote it; opening would upgrade it) or `TooNew { found, supported }` (a newer PageLamp
  wrote it). `pagelamp_core::store::Store::open_read_only` already tells these apart
  (`NotInitialised` / `SchemaTooOld` / `SchemaTooNew`); the facade only needs to expose it for
  an explicit dir and the default one.
- **Why:** Debug ▸ Data Source ▸ Live Data… opens the default data folder with `App::open`, which
  migrates the database on open (`Store::open` in `open_at_with_secrets`). If the installed
  PageLamp (the Tauri app, the CLI an AI app launches) is older than the preview, it then refuses
  that folder with `SchemaTooNew` until it is updated. The preview should say so before it happens,
  and only when it would ("opening will update your data folder's format: the installed
  PageLamp 0.x will ask to be updated"), or offer to stay on mock data.
- **Meanwhile:** the Live Data confirmation always warns that opening the folder may update its
  database format and that an older installed PageLamp would then ask to be updated
  (`mac.debug.live.message`, en + zh-CN). Mock data stays the default and live mode is never
  remembered across launches.
