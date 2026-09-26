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
