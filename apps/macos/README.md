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
apps/macos/scripts/perf-probe.sh all   # frame timing of the panel animations and page switches (opens a window, ~1 min)
apps/macos/scripts/perf-probe.sh capsule  # the sidebar's selection capsule under real clicks and keys (see No jank)
PAGELAMP_PERF_PRESS=0 apps/macos/scripts/perf-probe.sh segment  # the section picker's glass thumb on every kind of change (0 or 50 ms clicks; unset: both)
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
| **Live** | `LiveService`: the Rust facade over the **default data folder** (`~/Library/Application Support/dev.PageLamp.PageLamp`) and the keychain. | Debug ▸ Data Source ▸ Live Data…, after a confirmation: it shares data, secrets and the sync lock with the installed PageLamp app and its CLI; syncing here changes that data, and opening the folder may update its database format (an older installed PageLamp then asks to be updated; FACADE-REQUESTS F5 asks for a read-only check first). Not remembered across launches. |

Debug ▸ Run Mock Sync / Run Mock Sync with a Rejected Token exercise the capsule. The Debug menu
exists only with the `PAGELAMP_PREVIEW` compile flag (set in Package.swift for the UI targets);
a student build drops it.

## Layout

| Target | Folder | What |
|---|---|---|
| `PageLampKit` | `Sources/PageLampKit` | UniFFI bindings (generated) + `SyncEventStream` (sync callbacks → `AsyncStream`) |
| `PageLampModel` | `Sources/PageLampModel` | `PageLampService` (the calls M1 needs; typed `throws(PageLampFailure)`), `LiveService`, `UnavailableService` (S2: diagnostics only), `MockService` + `FixtureService` (snapshots/tests: the mock with answers replaced); `AppModel` (`@Observable @MainActor`: shell data, navigation, sync + capsule, data mode, language); `L10n` + code → words helpers (`L10n+Formatting`); `Model/SidebarLayout`, `SidebarNavigation`, `SidebarMotionGate` (the sidebar's geometry, list behaviour and motion order, §2.3); `Model/SegmentedLayout`, `SegmentedNavigation` (the course section picker's geometry and its pointer, key and VoiceOver rules, §3.2.1); per screen the logic without views: `ThisWeek/` (`ThisWeekDigest`, the port of Tauri `thisWeek.ts` and M1 stand-in for `this_week`; sections, text), `Course/` (page model, presentation), `Setup/` (source rows, Connect steps and snippets, settings); `PrimaryActionArbiter` |
| `PageLamp` | `Sources/PageLamp` | every view, MainActor by default. `Shell/` scenes, root split view, the custom sidebar (§2.3), commands, S1/S2 · `Chrome/` the functional layer: accessory bar, status capsule, toolbars, the shared glass thumb (`GlassThumb`: `NSGlassEffectView` + an additive Core Animation spring) that is the sidebar's selection capsule and the course section picker's thumb (`GlassSegmentedControl`, `NSControl`, its VoiceOver tree in `GlassSegmentedAccessibility`) — **the only folder with glass** · `Components/` the content layer: LampBand/LampWash, ReadingColumn/ReadingPage (`readingMeasure()`), PageHeader, SectionHeader, Callout/CalloutNote, CodeBlock/CopyButton, QuietState/SectionError, EmptyState, row and link button styles, arbiter styles, diagnostic preview, the component gallery (snapshots only) · `Support/` environment, strings, pasteboard and links, window metrics for the snapshots · `Views/<Screen>/` ThisWeek, Course, Sources, Connect, Settings · `Generated/`, `Resources/`. The page views the snapshots render are `package`, nothing else is |
| `PageLampApp` | `Sources/PageLampApp` | `@main`: owns the `AppModel` and the app delegate. Executable name `PageLampApp` (never `pagelamp`: the sidecar is `Contents/MacOS/pagelamp` on a case-insensitive disk) |
| `PageLampSnapshots` | `Sources/PageLampSnapshots` | the snapshot catalogue (`SnapshotCatalog.pages`, one file per screen) and the PNG writer (`SnapshotRenderer`, `ImageRenderer`): `swift run PageLampSnapshots <dir> [name-prefix …]`. Never linked into the app; the tests import it |

**Writing a screen.** A screen is `Views/<Screen>/<Screen>View.swift` (the `ScrollView` with
`.accessoryBar()` and its title; its toolbar items are in the one `WindowToolbar`, shown per page
with `.hidden(_:)`) plus a `<Screen>Page` document view (a `ReadingPage`: the
`LampBand` first, then `ReadingColumn`). Read shell data from `@Environment(AppModel.self)`
(`courses`, `sources`, `thisWeek`, `capsule`, …), load screen data through `model.service`
(course detail: `model.weekMaterials(for:)` also feeds Go ▸ Previous/Next Week), and strings from
`@Environment(\.l10n)`. Add the page's states to the snapshot catalogue (`Sources/PageLampSnapshots/<Screen>Snapshots.swift`; the page view is `package`):
each `SnapshotPage` sets up its model (`SnapshotSetup`: mock scenario, moment, a `FixtureService`
for states the demo data lacks) and loads its data in `make` (`.task` never runs offscreen).

## Rules

- **Glass only in `Sources/PageLamp/Chrome/`** (§1.3): `glassEffect*`, `GlassEffectContainer`,
  `.buttonStyle(.glass/.glassProminent)`, AppKit's `NSGlassEffectView` / `NSGlassEffectContainerView`. Content (bands, headers, callouts, rows, forms) never
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
  system colour), `PLSpace`, `PLLayout`, `PLSize`, `PLRadius`, `PLType`, `PLMotion`. `SidebarMetrics`
  is the one place for sidebar geometry (it mirrors the system source list), `SegmentedMetrics` for
  the section picker's (it mirrors the system's 27 tabs control).
- **One light, one tinted action**: `LampBand(lit:)` only for *now*; prominence through
  `.primaryActionCandidates([...])` + `.arbitratedButtonStyle(_:)` (§3.0).
- **Honour the system**: Reduce Motion (`PLMotion.reduced` / no animation), Reduce Transparency and
  Increase Contrast (LampBand's rule, Callout borders), `appearsActive` (capsule dims; inactive window:
  sidebar icons `.secondary`).
- **No jank** (M1 Macs included). Measure layout changes with `scripts/perf-probe.sh` before and
  after (hitch time < 5 ms/s on the sidebar and inspector animations). What cost us frames:
  a content root hosted in its own `NSHostingView` (the detail column, both sides of `.inspector`,
  the page under `.accessoryBar()`) without `.minimumSizeShield()` lays out all of its text at
  zero width on every animation frame; `.id` or a per-page `.toolbar` above the inspector rebuilds
  the split view and toolbar on every page switch; a custom `Layout` around a whole page measures
  it twice; `String(localized:)` with a bundle re-reads the table (L10n caches it); a model write
  that only flashes a loading state renders the page again. Chrome motion never shares frames with a
  page switch (SwiftUI animations are advanced on the main thread and stutter while a page builds):
  set the model first; start Core Animation motion (`PLMotion.<token>Spring`) after the page's first
  drawn frame (`SidebarMotionGate`); measure with `perf-probe.sh capsule`. One exception: the course
  section picker's thumb starts in the input's own handler (a model change: in the update that carries
  it), with no gate, because it slides within the page and the render server keeps drawing it while
  the new section builds (~20 ms); the control writes the model on the next run-loop turn, so the slide
  is committed before that build (never `CATransaction.flush()`: it stalls the content's crossfade on
  back-to-back switches); measure with `perf-probe.sh segment`.
- Swift 6 language mode, strict concurrency, warnings are errors. No Swift package dependencies.
- **Tests never touch the real data folder, keychain or preferences**: models use
  `InMemorySettingsStore`, a private `NotificationCenter`, `MockService`, or `LiveService` over
  `PageLamp.openWithMemorySecrets` in a temp folder.
- **Tests wait on events, never on time**: a `SyncStepGate` (`MockService.Timing.gate`) holds the
  mock's sync before each progress step, `AppModel.Timing.sleep` lets a test fire the model's timers
  (`ManualTimers`), and `until { … }` wakes on Observation changes (its timeout only ends a hang).

## Snapshots

`swift run PageLampSnapshots <dir>` (or `PAGELAMP_SNAPSHOT_DIR=<dir> swift test --filter SnapshotRenderTests`,
optionally with `PAGELAMP_SNAPSHOT_FILTER=this-week,course-DEMO101`) renders every page of
`SnapshotCatalog.pages` in light/dark × en/zh-Hans, on mock data at Friday 2026-09-25 10:00: This Week
(11 states), course detail (16 states + 5 inspector views), Sources & Sync, Connect and Settings (20
states), the sidebar (6 states) and the component gallery — 59 pages, 236 PNGs. A plain `swift test`
renders a small subset (This Week, a course page, Sources, the sidebar, the gallery; English light and
Chinese dark) into a temporary folder. `ImageRenderer` has limits: AppKit-backed controls (segmented/tabs pickers, borderless
buttons, progress bars) draw as yellow placeholders, except the course section picker, which draws a
stand-in (`\.drawsControlStandIns`): its real track, geometry and labels (`SegmentedLayout`, so the
narrow fallback to a pop-up menu shows at the app's widths) with a flat thumb for the glass; `Form`
draws blank (Settings and the inspector render a stand-in of the grouped style); and window chrome
(sidebar column, toolbar, inspector column, glass capsule) is not drawn; the sidebar's glass selection
capsule draws as a flat stand-in.

## Known limits (M1)

- **M2 and later, by design:** welcome window, Add Source / Replace / Remove sheets (the capsule's fix
  bubble and the course header's Replace Token… open Sources & Sync scrolled to the source, which is
  highlighted for a moment, and the problem callout explains the fix; the Replace button there is
  shown disabled and plain, with a "next update" help tag, and never takes the tint from Sync All),
  downloads, inspector editing, week scrubber and term strip, hidden and past course sections,
  search, capsule fuse/split, S5/S10/S13/S16 (S6, the crash notice, shows on This Week only), the Connect running check and access
  popover, the Privacy table, Week starts on, Reduce Highlighting Effects, the menu bar extra, Quick
  Look. The disclosure acknowledgement is not recorded on the Mac before M2 (spec §13 #1), so Connect
  and Privacy say "You haven't confirmed this yet".
- **Needs an on-device check** (offscreen snapshots can't show it; spec [verify]): the lamp's spill
  under the toolbar, sidebar and inspector (§6.1), the capsule's glass and fix bubble (§6.2), `.tabs`
  pickers on 27, grouped forms (Settings tab heights, inspector scrolling to a section), material row
  focus/double-click/Return/⌘C, VoiceOver (rotors, the adjustable week line, English-tagged backend
  text in zh-CN), Reopen Now's relaunch, and every Reduce Transparency / Increase Contrast / Show
  Borders / Reduce Motion path.
- **Sidebar (custom source list, §2.3), on device:** row tops, title x, icon centres, headers and the
  trailing edge beside a native list at small / medium / large (`-NSTableViewDefaultSizeMode 1|3` as a
  launch argument; small rows are a uniform 24 pt on purpose where the native ones are 25–27 pt),
  including the wide `folder.badge.gearshape` against the title at large;
  `\.sidebarRowSize` following System Settings live; keys (↑/↓ with ⇧⌃⌘, ⌥↑/↓, Home/End scroll only,
  Page Up/Down, Return, Space, ←, →, Esc passed on, type-select timing, one Tab / ⇧Tab / ⌃F6 stop
  with Full Keyboard Access off and on, `onKeyPress` delivering `.repeat` and `.up` for arrows);
  pointer (select on mouse-down, a trackpad tap slides, drag and release, cancel by dragging out or
  ⌘-Tab mid-press, the first click in an inactive window, control-click); the focus ring (after Tab
  or keys only, hidden in an inactive window, following an accent change); VoiceOver in English and
  Chinese ("Sidebar, list", static-text rows with "selected" whose outline covers the whole row,
  headings, the course tooltip (not an accessibility help), the Sources and Connect values, VO-Space,
  announcements without double speech, including the one when Tab brings focus to the list, the
  footer after the list, the capsule never focused); the capsule's look beside the course section
  picker's thumb (light, dark, several accents, window inactive, over the lamp spill, under the
  toolbar edge with 12+ courses at 760×520, Reduce Transparency, Increase Contrast, Show Borders,
  the 27 glass slider, Reduce Motion); accent icons active / secondary inactive; least-amount scrolling for
  arrows and ⌘1/2/3; column resize during a slide; collapsing the sidebar or closing the window
  mid-slide or mid-preview (keys still switch pages at once afterwards); macOS 26.x (floating
  sidebar); `perf-probe.sh capsule` on a quiet machine with the app frontmost (clicks only register
  while it is active) and on an M1.
- **Section picker (custom glass control, §3.2.1), on device, each beside the system control (Preview
  builds: Debug ▸ Course Pages Use the System Section Picker; removed after sign-off):** U1 Keyboard
  navigation on: Tab / ⇧Tab reach the picker, ←/→/Space, a click doesn't focus it (all measured per
  process like the system control's; AppKit 27.2 ignores the `-AppleKeyboardUIMode 2` launch
  argument, as it reads the preference with `CFPreferencesCopyValue`, so a probe needs a
  `DYLD_INSERT_LIBRARIES` library that answers 2 for that key); U2 the focus ring's look; U3 VoiceOver in English and Chinese ("Course sections, tab group", "This Week,
  selected, tab, 1 of 3", VO-arrows, VO-Space, ←/→ speech, no double speech after Space or when focus
  arrives); U4 real Increase Contrast, Reduce Transparency and Show Borders; U5 real Reduce Motion (the
  thumb jumps); U6 the glass thumb beside the system one in light, dark and an inactive window, on the
  lamp band, with the 27 glass slider (Clear / Tinted); U7 120 Hz smoothness; U8 macOS 26, where the
  custom control replaces the accent-filled `.segmented` one.
- The xcframework and bindings are git-ignored, so a fresh checkout runs `build-ffi.sh` (or
  `build-app.sh`) before `swift build`.
