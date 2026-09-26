# PageLamp for Mac (preview)

The native macOS app: SwiftUI with Liquid Glass, macOS 26+, Apple silicon, over the same Rust
core as the CLI and the Tauri app (UniFFI, `crates/pagelamp-ffi`). The design is
[`docs/design/macos-shell.md`](../../docs/design/macos-shell.md) ("Lamplight"); section numbers
below (§…) refer to it. This is milestone M1: shell, This Week, course detail (read-only),
Connect, Sources list, Settings basics. It runs on **mock data by default**.

## Build and test

Needs Xcode 27 (licence accepted), rustup with `aarch64-apple-darwin`, Node ≥ 24. If
`xcode-select` points at the Command Line Tools, prefix Swift commands with
`DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer` (the scripts do this themselves).

```sh
apps/macos/scripts/build-ffi.sh        # Rust core → Frameworks/PageLampFFI.xcframework + Swift bindings (git-ignored)
cd apps/macos
swift build                            # debug build of every target
swift test                             # PageLampKitTests (the facade) + PageLampModelTests (model, mock, strings)
swift run PageLampSnapshots /tmp/snaps # PNGs of every page and state: light/dark × en/zh-Hans (headless)
cd ../..
apps/macos/scripts/build-app.sh        # → apps/macos/dist/PageLamp Preview.app (ad-hoc signed)
apps/macos/scripts/lint.sh             # glass only in Chrome/, generated files current, string keys exist
```

- **build-app.sh** rebuilds the Rust core when the xcframework is missing or older than any crate
  source, builds the CLI sidecar (`Contents/MacOS/pagelamp`, what AI apps launch), builds
  `PageLampApp` in release with the real SDK stamped (`-Xlinker -platform_version macos 26.0
  <sdk>`: SwiftPM otherwise records sdk = 26.0 and AppKit's behaviour differs), assembles the
  bundle (`dev.pagelamp.mac-preview`, "PageLamp Preview", en + zh-Hans), signs inside out with the
  hardened runtime, verifies with `codesign --verify --deep --strict` and checks the stamp with
  `vtool`. It never launches the app. Ad-hoc signed apps open only on the Mac that built them.
- **Generated files.** `Sources/PageLampKit/Generated/` and `Frameworks/` come from build-ffi.sh
  (git-ignored). `Sources/PageLamp/Generated/` (PLTokens.swift, L10nKeys.swift) and
  `Sources/PageLamp/Resources/*.lproj` are committed and come from
  `node design/tokens/gen-tokens.mjs` and `node apps/macos/scripts/gen-strings.mjs`; lint fails
  when they are stale.

## Data modes

| Mode | What | How |
|---|---|---|
| **Mock** (default) | `MockService`: synthetic demo data ported from the Tauri mock (DEMO101/205/310/099, three sources, a study plan, sync events streamed with delays). Never touches the data folder, keychain or network. | Every launch starts here, scenario **Expired Canvas Token** (S7 visible). Debug ▸ Data Source ▸ Mock Data switches between Demo, No Sources, Expired Canvas Token, Missing Course Folder, Another Sync Running, Recovered from a Crash. |
| **Live** | `LiveService`: the Rust facade over the **default data folder** (`~/Library/Application Support/dev.PageLamp.PageLamp`) and the keychain. | Debug ▸ Data Source ▸ Live Data…, after a confirmation: it shares data, secrets and the sync lock with the installed PageLamp app and its CLI; syncing here changes that data. Not remembered across launches. |

Debug ▸ Run Mock Sync / Run Mock Sync with a Rejected Token exercise the capsule. The Debug menu
exists only with the `PAGELAMP_PREVIEW` compile flag (set in Package.swift for the UI targets);
a student build drops it.

## Layout

| Target | Folder | What |
|---|---|---|
| `PageLampKit` | `Sources/PageLampKit` | UniFFI bindings (generated) + `SyncEventStream` (sync callbacks → `AsyncStream`) |
| `PageLampModel` | `Sources/PageLampModel` | `PageLampService` (the calls M1 needs; typed `throws(PageLampFailure)`), `LiveService`, `UnavailableService` (S2: diagnostics only), `MockService` + `FixtureService` (snapshots/tests: the mock with answers replaced); `AppModel` (`@Observable @MainActor`: shell data, navigation, sync + capsule, data mode, language); `L10n` + code → words helpers (`L10n+Formatting`); per screen the logic without views: `ThisWeek/` (`ThisWeekDigest`, the port of Tauri `thisWeek.ts` and M1 stand-in for `this_week`; sections, text), `Course/` (page model, presentation), `Setup/` (source rows, Connect steps and snippets, settings); `PrimaryActionArbiter` |
| `PageLamp` | `Sources/PageLamp` | every view, MainActor by default. `Shell/` scenes, root split view, sidebar, commands, S1/S2 · `Chrome/` the functional layer: accessory bar, status capsule, toolbars — **the only folder with glass** · `Components/` the content layer: LampBand/LampWash, ReadingColumn/ReadingPage (`ReadingMeasure` layout), PageHeader, SectionHeader, Callout/CalloutNote, CodeBlock/CopyButton, QuietState/SectionError, EmptyState, row and link button styles, arbiter styles, diagnostic preview · `Support/` environment, strings, pasteboard and links · `Views/<Screen>/` ThisWeek, Course, Sources, Connect, Settings · `Snapshots/` the snapshot catalogue (one file per screen) · `Generated/`, `Resources/` |
| `PageLampApp` | `Sources/PageLampApp` | `@main`: owns the `AppModel` and the app delegate. Executable name `PageLampApp` (never `pagelamp`: the sidecar is `Contents/MacOS/pagelamp` on a case-insensitive disk) |
| `PageLampSnapshots` | `Sources/PageLampSnapshots` | renders `SnapshotCatalog.pages` with `ImageRenderer` (`swift run PageLampSnapshots <dir> [name-prefix …]`) |

**Writing a screen.** A screen is `Views/<Screen>/<Screen>View.swift` (the `ScrollView` with
`.accessoryBar()`, title and toolbar) plus a `<Screen>Page` document view (a `ReadingPage`: the
`LampBand` first, then `ReadingColumn`). Read shell data from `@Environment(AppModel.self)`
(`courses`, `sources`, `thisWeek`, `capsule`, …), load screen data through `model.service`
(course detail: `model.weekMaterials(for:)` also feeds Go ▸ Previous/Next Week), and strings from
`@Environment(\.l10n)`. Add the page's states to the snapshot catalogue (`Snapshots/<Screen>Snapshots.swift`):
each `SnapshotPage` sets up its model (`SnapshotSetup`: mock scenario, moment, a `FixtureService`
for states the demo data lacks) and loads its data in `make` (`.task` never runs offscreen).

## Rules

- **Glass only in `Sources/PageLamp/Chrome/`** (§1.3): `glassEffect*`, `GlassEffectContainer`,
  `.buttonStyle(.glass/.glassProminent)`. Content (bands, headers, callouts, rows, forms) never
  uses glass. `lint.sh` enforces it.
- **No hard-coded user-visible text.** Every string is `l10n("key")` / `l10n.plural("key", count:)`
  with keys from the shared i18n (`apps/desktop/src/i18n/locales`) or `mac.*` fragments in
  `Localization/` (English **and** Simplified Chinese; see `Localization/README.md`), then
  `node apps/macos/scripts/gen-strings.mjs`. Arguments are named (`["source": label]`); numbers are
  pre-formatted with `l10n.number(_:)`. Unknown keys assert in debug builds; `lint.sh` checks every
  key literal and flags string literals in `Text(…)`, `Button(…)`, `.help(…)` and friends
  (`Text(verbatim:)` for text that is not localizable, such as course codes). Menus use
  `model.menuL10n` (the launch language, like the system's items); content uses `\.l10n` and
  switches live.
- **Tokens, not numbers**: `PLColor` (lamp wash/rule and status glyphs only; everything else is a
  system colour), `PLSpace`, `PLLayout`, `PLSize`, `PLRadius`, `PLType`, `PLMotion`.
- **One light, one tinted action**: `LampBand(lit:)` only for *now*; prominence through
  `.primaryActionCandidates([...])` + `.arbitratedButtonStyle(_:)` (§3.0).
- **Honour the system**: Reduce Motion (`PLMotion.reduced` / no animation), Reduce Transparency and
  Increase Contrast (LampBand's rule, Callout borders), `appearsActive` (capsule dims).
- Swift 6 language mode, strict concurrency, warnings are errors. No Swift package dependencies.
- **Tests never touch the real data folder, keychain or preferences**: models use
  `InMemorySettingsStore`, a private `NotificationCenter`, `MockService`, or `LiveService` over
  `PageLamp.openWithMemorySecrets` in a temp folder.

## Snapshots

`swift run PageLampSnapshots <dir>` (or `PAGELAMP_SNAPSHOT_DIR=<dir> swift test --filter SnapshotRenderTests`,
optionally with `PAGELAMP_SNAPSHOT_FILTER=this-week,course-DEMO101`) renders every page of
`SnapshotCatalog.pages` in light/dark × en/zh-Hans, on mock data at Friday 2026-09-25 10:00: This Week
(10 states), course detail (16 states + 5 inspector views), Sources & Sync, Connect and Settings (18
states) and the component gallery — 50 pages, 200 PNGs. `ImageRenderer` has limits: AppKit-backed
controls (segmented/tabs pickers, borderless buttons, progress bars) draw as yellow placeholders,
`Form` draws blank (Settings and the inspector render a stand-in of the grouped style), and window
chrome (sidebar, toolbar, inspector column, glass capsule) is not drawn.

## Known limits (M1)

- **M2 and later, by design:** welcome window, Add Source / Replace / Remove sheets (the fix buttons
  open Sources & Sync, where the problem callout explains the fix; the Replace button there is shown
  disabled), downloads, inspector editing, week scrubber and term strip, hidden and past course
  sections, search, capsule fuse/split, S5/S6/S10/S13/S16, the Connect running check and access
  popover, the Privacy table, Week starts on, Reduce Highlighting Effects, the menu bar extra, Quick
  Look. The disclosure acknowledgement is not recorded on the Mac before M2 (spec §13 #1), so Connect
  and Privacy say "You haven't confirmed this yet".
- **Needs an on-device check** (offscreen snapshots can't show it; spec [verify]): the lamp's spill
  under the toolbar, sidebar and inspector (§6.1), the capsule's glass and fix bubble (§6.2), `.tabs`
  pickers on 27, grouped forms (Settings tab heights, inspector scrolling to a section), material row
  focus/double-click/Return/⌘C, VoiceOver (rotors, the adjustable week line, English-tagged backend
  text in zh-CN), Reopen Now's relaunch, and every Reduce Transparency / Increase Contrast / Show
  Borders / Reduce Motion path.
- The xcframework and bindings are git-ignored, so a fresh checkout runs `build-ffi.sh` (or
  `build-app.sh`) before `swift build`.
