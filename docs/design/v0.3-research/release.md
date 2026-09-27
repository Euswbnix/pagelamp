# v0.3 start: updater, signing and the extraction worker (research report)

Research date 2026-09-27. Every external fact below carries a source number (the list is in §12; all
were accessed 2026-09-27). **[unverified]** marks things I could not confirm from a primary source
today, or that must be proven in a test build. Repo facts come from reading `main` at 7fda49f
(read-only) and the `feat/macos-shell` worktree.

Owner context (relayed request): cost isn't a concern ("就100多刀"), people don't need to be
managed, the owner will spend time testing, and 0.1 has little in it. As of today the GitHub
releases API lists **no published release** for `Euswbnix/pagelamp` [S38]: `v0.1.0-beta.1` is still
a draft, so every 0.1 install that exists is the team's own. That takes most of the risk out of
migration.

---

## 0. Recommendation in one screen

1. **Updater.** Use `tauri-plugin-updater` from **Rust only**: commands in `src-tauri`, no
   `updater:*` permission for the webview, so the capability file stays small. Set
   `createUpdaterArtifacts: true`. Generate our own `latest.json` with plain
   `github.com/.../releases/download/<tag>/<file>` URLs and set `uploadUpdaterJson: false` in
   tauri-action. tauri-action v1 now writes `api.github.com` asset URLs, and unauthenticated API
   calls are limited to 60 per hour per IP; many students behind one campus NAT could hit that
   [S5][S6][S31]. Offer two channels. **stable** reads `releases/latest/download/latest.json`,
   which skips drafts and pre-releases. **beta** reads a `beta.json` on GitHub Pages; a
   `release: published` workflow updates it. The channel is picked at runtime with
   `updater_builder().endpoints()` [S1].
2. **Versioning fix, needed before the first beta that uses the updater.** `tauri.conf.json` holds
   only the numeric version (`0.1.0`) because MSI rejects non-numeric pre-release tags [S8].
   Tauri's comparator would therefore see `0.3.0-beta.2` and `0.3.0` as the same `0.3.0`. Install
   a `Builder::default_version_comparator` that compares against `env!("CARGO_PKG_VERSION")` (the
   real workspace version) [S3]. Also ship **NSIS only** on Windows: it installs per user with no
   admin rights, installs over old WiX installs by itself, and avoids the MSI version rules
   [S8][S9].
3. **macOS.** Sign with Developer ID and the hardened runtime, with **no entitlements** at first.
   Tauri signs the sidecar inside-out with the same settings and notarizes and staples the `.app`
   [S10][S11]. Use a **Team** App Store Connect API key for `notarytool`; individual keys can't use
   notaryTool [S14]. Ship one **universal** DMG so students don't have to know which chip their
   Mac has; `build-sidecar.mjs` already supports it. Sign and notarize the standalone CLI too. It
   can't be stapled, so Gatekeeper looks up the ticket online [S12].
4. **Windows.** Use **Azure Artifact Signing, Basic** (about US$9.99 a month, quota 5,000
   signatures a month). As of 2026-05-21, individual developers in Canada are eligible
   [S17][S18][S19]. It needs a paid Azure subscription; free, trial and sponsored ones are refused
   [S20]. Sign through `signtool` + the Microsoft dlib, called from Tauri's `signCommand`, which
   also signs the sidecar [S21][S9]. Apply to **SignPath Foundation** (free for OSS) in parallel as
   a fallback [S22][S23]. Unsigned is no longer tolerable. On Windows 11, Smart App Control blocks
   unsigned executables outright [S24], and that would include `pagelamp.exe` when the student's
   AI app starts it.
5. **Linux.** No code signing. Rely on `SHA256SUMS`, GitHub **artifact attestations**
   (`actions/attest@v4`, free for public repos) and **immutable releases** [S26][S27][S28]. Only
   the AppImage updates itself. For `.deb`/`.rpm`, show "update available" with a download link.
   Tauri's deb/rpm installer asks for root through pkexec, or else asks for the sudo password in
   its own dialog [S4].
6. **Extraction worker.** Run it as a hidden `pagelamp extract-worker` subcommand of the **existing
   sidecar**, one process per file, with JSON over stdin/stdout. Limits:
   - A **counting global allocator** in the worker, which caps memory the same way on every OS.
     This matters on macOS, where `setrlimit` memory limits don't work [S33][S34].
   - `RLIMIT_CPU`/`RLIMIT_CORE=0`/`RLIMIT_FSIZE=0` on Unix, plus `RLIMIT_AS` on Linux.
   - A Windows **Job Object** with the process-memory, process-time, kill-on-close and
     die-on-unhandled-exception limits [S35][S36].
   - A wall-clock timeout enforced by the parent.

   The worker is wired in through `pagelamp-app` (the facade), so the Tauri app, the Swift shell
   and the CLI all get it. OS sandboxing (Seatbelt, Landlock) is phase 2.
7. **Release pipeline.** Put signing secrets in a GitHub **environment** `release`, restricted to
   `v*` tags, with the owner as required reviewer [S29]. Pin every third-party action to a commit
   SHA, because these jobs will hold signing keys (see the tj-actions compromise [S30]). Stop
   deleting `.app.tar.gz`: the updater needs it. Add a manual release-rehearsal workflow.

Yearly cost: Apple US$99 [S13] + Artifact Signing ≈ US$120 → **≈ US$220 a year**.

---

## 1. What the repo has today (gaps that matter)

| Where | Today | Gap for v0.3 |
|---|---|---|
| `apps/desktop/src-tauri/tauri.conf.json` | `"signingIdentity": "-"` (ad hoc), `"targets": "all"`, `externalBin: ["binaries/pagelamp"]`, version `0.1.0` (numeric) | No updater plugin, no `createUpdaterArtifacts`, no pubkey, and both MSI and NSIS are built |
| `.github/workflows/release.yml` | Draft release created by `gh`; `tauri-action@v1` gets `releaseId` only; the `checksums` job **deletes every `.app.tar.gz`** ("without an updater it only duplicates the .dmg") | Deleting `.app.tar.gz` breaks the macOS updater. With no `tagName`, tauri-action would point `latest.json` at `releases/latest`, which is wrong for pre-releases [S5]. The CLI archives are unsigned. No secrets or environment. Actions are pinned to tags |
| `.github/workflows/ci.yml` | Rust on 3 OSes; desktop on Ubuntu + macOS | Desktop isn't built on Windows (already in post-beta-followups); nothing checks the release config |
| `docs/release-notes/v0.1.0-beta.1.md` | Tells macOS users to run `xattr -dr com.apple.quarantine` when "your AI app can't start PageLamp" | Notarization fixes this: the stapled bundle includes `Contents/MacOS/pagelamp` |
| `PRIVACY.md` | "PageLamp never contacts any other server." | The update check contacts GitHub. Update the text and add a setting |
| `crates/pagelamp-core` secrets | `keyring 4.2` → `apple-native-keyring-store`, **`keychain` module** (the legacy keychain, where each item has an access list of trusted apps) | When the signing identity changes from ad hoc to Developer ID, expect **one** "allow access" prompt per binary **[unverified, must test]** |
| `crates/pagelamp-core/src/ingest.rs` | `index_file_with(…, extract: impl FnOnce(&Path, Option<&str>) -> Result<Vec<Segment>, ExtractError>)` | This closure is the natural place to plug in the worker |
| `apps/desktop/src-tauri/src/backend.rs` | `pagelamp_binary()` = the file named `pagelamp` next to `current_exe()` | Reuse it as the worker path |
| `feat/macos-shell` `apps/macos` | Builds `Contents/MacOS/pagelamp`, signs inside-out with the hardened runtime, ad hoc | Same Developer ID and notarization. Its updates can't use Tauri's updater (use Sparkle 2 later) |
| `ARCHITECTURE.md` §7 | Updater, signing and the extract-worker are listed under v0.2 | Move them to the start of v0.3 (owner decision) |

---

## 2. Tauri 2 updater

### 2.1 Facts (current plugin 2.13.0)
- Signing can't be turned off. `tauri signer generate -w ~/.tauri/<name>.key` creates the key pair.
  CI reads `TAURI_SIGNING_PRIVATE_KEY` (a path or the key's content) and
  `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`; `.env` files are not read. "If you lose this key you will
  NOT be able to publish new updates to the users that have the app already installed." [S1]
- `bundle.createUpdaterArtifacts: true` produces `.app.tar.gz` + `.sig` on macOS, a signed
  `-setup.exe` / `.msi` + `.sig` on Windows, and an `.AppImage` + `.sig` on Linux.
  `"v1Compatible"` is only for apps migrating from Tauri v1 [S1].
- Static JSON needs `version` plus `platforms[<key>].{url, signature}`. The plugin looks up
  **`{os}-{arch}-{installer}` first, then `{os}-{arch}`**. `installer` is one of `app`, `msi`,
  `nsis`, `appimage`, `deb`, `rpm`, taken from the bundle type compiled into the binary [S3].
- Endpoints can be set at runtime from Rust (`updater_builder().endpoints(vec![…])`), and the docs
  show exactly this for channels. The pubkey can also be set at runtime (for key rotation) [S1].
- Windows: "the application is automatically exited when the install step is executed".
  `installMode` is `passive` (default), `basicUi` or `quiet`; `on_before_exit` is a hook [S1].
  Plugin 2.11 added `restart_after_install` [S4].
- Plugin 2.10 made **deb and rpm** updatable, installed through `pkexec`, then a zenity/kdialog
  password prompt, then terminal `sudo` [S3][S4]. Plugin 2.12 **moved `allowDowngrades` out of the
  JS `check()` call into the plugin config** (breaking), so a webview can no longer downgrade the
  app [S4].
- The download request sends `Accept: application/octet-stream`, so API asset URLs work [S3].
- macOS install: the current bundle is moved to a temp folder and the new one renamed into place.
  If that isn't allowed, it falls back to `osascript … with administrator privileges` [S3]. A
  rename gives the new files new inodes, so a `pagelamp mcp` process that is already running keeps
  its old binary. **[unverified in a live test]**
- An app updated this way gets no quarantine attribute, so **Gatekeeper never re-checks it**. The
  minisign key is the only thing that protects updates; notarization protects the first install.
  (My conclusion from how quarantine works; confirm by inspecting xattrs after an update
  **[unverified]**.)

### 2.2 Keys and who keeps them
- The owner generates the key on his own Mac with a strong password. Keep the key file and password
  in a password manager, plus one offline copy (encrypted USB or paper; minisign keys are short).
  In CI they live only as **`release` environment secrets**. The public key is committed in
  `tauri.conf.json` under `plugins.updater.pubkey`.
- There is no revocation. Rotation means shipping an update, signed with the old key, whose code
  contains the new pubkey [S1]. A compromised key plus write access to a release lets an attacker
  push malware to every install, so the environment's required-reviewer gate and SHA-pinned
  actions are what protect the key.
- The manifest itself is not signed, only the artifacts. Someone who can change `latest.json` can
  hold updates back, or offer an older signed artifact labelled with a higher version. The
  signature covers file bytes, not the version string. (My reading of the design
  **[unverified]**.) Mitigation: serve manifests only from GitHub infrastructure over HTTPS and
  keep them in immutable releases where possible.

### 2.3 Manifest on GitHub Releases
- tauri-action **v1.0.0 (2026-06-29)** changed several things [S5][S6]:
  - `.app.tar.gz` names now include the version.
  - URLs in `latest.json` now use the **GitHub API asset URL** (#1315).
  - The action fails if `releaseDraft` doesn't match the release.
  - `includeUpdaterJson` became `uploadUpdaterJson`, and `updaterJsonKeepUniversal` was removed.
- With `releaseId` set and no `tagName`, `latest.json` points at `releases/latest/download/…`
  [S5]. That breaks for pre-releases, and it's what our workflow does today.
- **Recommendation:** set `uploadUpdaterJson: false` and add a small Node script
  (`.github/scripts/updater-manifest.mjs`) in a job after the build. It:
  - lists the draft's assets and reads each `.sig`;
  - writes the keys `darwin-aarch64`, `darwin-x86_64`, `darwin-*-app`, `windows-x86_64`,
    `windows-x86_64-nsis` and `linux-x86_64-appimage`;
  - leaves out the generic `linux-x86_64` key on purpose (see §2.6);
  - uses `https://github.com/<repo>/releases/download/<tag>/<asset>` URLs;
  - **fails if any expected platform is missing**, and uploads `latest.json` to the draft.
  - Optional: check every signature with `minisign -V` before uploading **[unverified that Tauri's
    base64 `.sig` decodes to a plain minisign signature; likely, since it uses the minisign
    format]**.

### 2.4 Channels
- **stable**: endpoints `[pages/updates/stable.json, releases/latest/download/latest.json]`.
  GitHub's "latest" skips drafts and pre-releases, which is exactly the stable channel. Tauri
  tries endpoints in order [S3].
- **beta**: `pages/updates/beta.json`. Immutable releases forbid changing assets after publishing
  [S28], so a fixed "channel" release with an overwritten asset won't work. Use GitHub Pages
  instead.
- New workflow `channels.yml` on `release: [published]`:
  - For a pre-release, copy its `latest.json` to `beta.json`.
  - For a stable release, copy it to both `stable.json` and `beta.json`, so beta users also get
    the final release.
  - Channels move only when the owner publishes the draft, so the manual review step stays.
  - Editing `stable.json` back to an older version works as a kill switch: it stops further
    updates, but can't roll anyone back.
- Default channel: beta if the installed version has a pre-release tag, otherwise stable. Put a
  switch in Settings → Updates.
- Branded builds (UTMCSSA, after v0.3): keep endpoints and channel in the brand config so a brand
  can point at its own manifest.

### 2.5 The version-comparison problem (fix before the first updater beta)
- Tauri takes `current_version` from `package_info()`, i.e. `tauri.conf.json` [S3]. Ours holds the
  numeric `0.1.0`, because WiX turns `-beta.1` into a 4th number and refuses anything that isn't
  numeric [S8]. As a result a `0.3.0-beta.2` install sees `0.3.0` as equal to itself, and betas
  never offer the final release.
- **Fix (recommended, small):** `tauri_plugin_updater::Builder::new().default_version_comparator(|_, remote|
  semver::Version::parse(env!("CARGO_PKG_VERSION")).map_or(true, |me| remote.version > me))`
  [S3]. The desktop crate has `version.workspace = true`, so `CARGO_PKG_VERSION` is the real
  version.
- Also check: MSI allows same-version upgrades (`AllowSameVersionUpgrades="yes"` [S7]), and NSIS
  reinstalls the same version fine.
- Alternative for later: go NSIS-only and put the full semver into `tauri.conf.json`. NSIS ignores
  the pre-release part [S8], but the macOS `CFBundleShortVersionString` would then read
  `0.3.0-beta.1` [S11], which Apple documents as numeric. **[unverified whether notarization
  cares]**

### 2.6 How each OS behaves
- **macOS:** the updater installs `.app.tar.gz`. MCP configs point at
  `/Applications/PageLamp.app/Contents/MacOS/pagelamp`, which doesn't change across updates. AI
  apps must be restarted to pick up the new sidecar (the release notes already say so); add a
  post-update banner.
- **Windows (NSIS, per user):** `%LOCALAPPDATA%\PageLamp\pagelamp.exe` doesn't change across
  updates. **Problem:** the NSIS template only checks whether the *main* exe is running
  (`CheckIfAppIsRunning "$INSTDIR\${MAINBINARYNAME}.exe"`) [S9]. A `pagelamp.exe` that Claude
  Desktop keeps running as an MCP server is locked, so `File` can't overwrite it.
  - Fix: an NSIS pre-install hook (`bundle.windows.nsis.installerHooks` [S37]) that renames the
    running `pagelamp.exe` to `pagelamp.exe.old` (a running exe can be renamed but not
    overwritten), with the app deleting leftover `.old` files at startup **[unverified: test on
    Windows 11 with Claude Desktop running]**. Do the same before uninstalling.
- **Windows (MSI):** drop it. The NSIS installer finds and uninstalls an earlier WiX install
  itself [S9]. That moves the install from Program Files to `%LOCALAPPDATA%`, so MCP snippets must
  be copied again, once.
- **Linux AppImage:** the updater replaces the file in place. The sidecar inside the AppImage still
  can't be used by AI apps (existing `TemporaryLocation::AppImage` note).
- **Linux deb/rpm:** don't auto-install. Two reasons:
  1. Tauri may ask for the user's sudo password inside our process [S3].
  2. If the manifest lacks a `-deb`/`-rpm` key, the plugin falls back to `linux-x86_64` and would
     run `dpkg -i` on an AppImage.

  Do this instead: in the app, use `tauri::utils::platform::bundle_type()`. For deb/rpm, call
  `check()` with `.target("linux-x86_64-appimage")` only to learn the version, then show a
  download link. An APT/RPM repository can come later.
- **Standalone CLI archives:** they don't update themselves. Later: Homebrew tap, Scoop, winget.
  The MCP path must stay offline, so any version check must be an explicit command.

### 2.7 Updating the sidecar, and version skew
The sidecar is inside the bundle or installer, so it always updates with the app. While an AI app
keeps an older `pagelamp mcp` running, it reads a database the new app may have migrated. The
existing rule ("additive migrations only", and `mcp` migrates once at startup) keeps this safe.
Keep that rule and state it in ARCHITECTURE §5. After an update, the app says: "Quit and reopen
Claude Desktop / Codex so they use the new PageLamp."

### 2.8 Migration from unsigned 0.1
- **0.1 installs cannot update themselves.** They have no updater plugin and no pubkey. Everyone
  on 0.1 reinstalls by hand **once**, into the first signed build that has the updater. Every
  version after that updates itself.
- What they do once:
  - **macOS:** quit PageLamp, download the new DMG, drag it to Applications, choose Replace.
    Opening it no longer needs "Open Anyway": notarized apps pass Gatekeeper, and "Open Anyway"
    is the only bypass since Sequoia [S15]. Expect a possible keychain prompt when the new signed
    binaries first read the Canvas token or feed URL that 0.1 stored; answer "Always Allow"
    **[unverified, test it]**. The data directory and bundle id don't change. MCP configs don't
    change if the app stays in /Applications.
  - **Windows:** quit the AI app first (the sidecar may be locked), then run the new
    `-setup.exe`. Over a 0.1 NSIS install it's seamless. Over a 0.1 MSI install, NSIS removes the
    MSI and the path changes: re-copy the MCP snippet.
  - **Linux:** install the new `.deb`/`.rpm`, or use the new AppImage.
- The first public build is a draft today, so this affects essentially only the team. Put the
  updater in the first v0.3 pre-release, or in v0.1.0 final if the Apple enrolment arrives in time.
  There's no reason to hold v0.1.0 back for it.

### 2.9 UI and privacy
- Settings → Updates: channel, "Check automatically" (check at launch, then every 24 h), Check now,
  release notes, "Install and restart". On Windows, warn that PageLamp will close.
- Use a Rust command with a progress channel. No JS updater permissions.
- Update PRIVACY.md: the update check sends the IP address and a
  `tauri-plugin-updater/<ver>` User-Agent to GitHub. With static JSON no app version is sent
  [S3].

---

## 3. macOS: Developer ID, hardened runtime, notarization

### 3.1 Facts
- Individual enrolment costs **US$99 per membership year**, shown in local currency at
  enrolment. It needs the owner's legal name and an Apple Account with 2FA; the legal name is shown
  as the seller [S13].
- **Only the Account Holder can create Developer ID certificates** [S16][S11].
- Notarization requires [S12]:
  - a Developer ID certificate;
  - the **hardened runtime for app and command-line targets**;
  - a secure timestamp.
- Staple the ticket to the `.app`/`.dmg`/`.pkg`. **ZIP archives and standalone binaries can't be
  stapled**; Gatekeeper finds their ticket online [S12].
- Since macOS Sequoia, Control-click no longer bypasses Gatekeeper; users must go to System
  Settings → Privacy & Security [S15].
- Tauri bundler behaviour [S10][S11]:
  - It copies `externalBin` into `Contents/MacOS`, signs frameworks and sidecars first, then the
    app. Every target gets the same entitlements file, with `--options runtime` when
    `hardenedRuntime` is on (the default).
  - Then it zips the app with `ditto`, runs `notarytool submit --wait` and **staples the `.app`**.
  - It signs the DMG but doesn't notarize it.
  - `APPLE_SIGNING_IDENTITY` in the environment overrides `bundle.macOS.signingIdentity`
    (tauri-cli `rust.rs`). We can keep `"-"` in the committed config for local builds.
- Notarization credentials Tauri reads [S10][S11]:
  - Option 1: `APPLE_API_KEY` + `APPLE_API_ISSUER` + `APPLE_API_KEY_PATH`.
  - Option 2: `APPLE_ID` + `APPLE_PASSWORD` (an app-specific password) + `APPLE_TEAM_ID`.
  - Certificate: `APPLE_CERTIFICATE` (base64 .p12) + `APPLE_CERTIFICATE_PASSWORD`.

### 3.2 Entitlements
- **App: none to start with.** WKWebView's JIT runs in WebKit's own processes, the legacy keychain
  needs no entitlement, and dialog/opener don't need one either **[unverified: prove it with a
  notarized rehearsal build that covers launch, folder picker, links, keychain read/write,
  starting the sidecar, and the updater]**.
- **Sidecar (`pagelamp`): none.** Tauri gives every target the same entitlements file [S11], so
  anything added for the app would also reach the CLI. Keep the file empty or absent.
- Never ship `get-task-allow`. Don't add `disable-library-validation` or
  `allow-unsigned-executable-memory` unless a test proves they're needed.
- If PageLamp is ever sandboxed (App Store), a spawned helper needs `com.apple.security.inherit`.
  That doesn't apply to direct distribution.

### 3.3 notarytool in GitHub Actions: API key or app-specific password
- **Use a Team App Store Connect API key.**
  - Apple: individual keys can't use notaryTool [S14].
  - Generating a team key needs Admin (the Account Holder is Admin), and the `.p8` can be
    downloaded **only once** [S14].
  - The Developer role is the usual choice for notarization **[unverified in Apple docs today]**.
  - A key can be revoked on its own and doesn't depend on the owner's Apple ID password.
- An app-specific password stops working when the Apple ID password changes.
- The `app` job writes `APPLE_API_KEY_P8` to `$RUNNER_TEMP` and exports `APPLE_API_KEY_PATH`.

### 3.4 Stapling and the CLI archives
- The `.app` is stapled by Tauri. Optional: `notarytool submit` + `stapler staple` on the DMG
  after the build, then upload it again with `--clobber`.
- Standalone `pagelamp` binaries: `codesign --force --options runtime --timestamp --sign "$ID"`,
  then `ditto -c -k` into a zip for submission only, then `notarytool submit --wait`. Ship the
  signed binary in the existing tar.gz/zip; it can't be stapled [S12].
- Sign with the default identifier (`pagelamp`), the same as Tauri's sidecar. Both then have the
  same designated requirement (identifier plus Team ID), so the keychain treats them as one app
  **[unverified]**.

### 3.5 What the owner does after enrolling, step by step
1. Enrol as an **Individual** at developer.apple.com/programs/enroll: Apple Account with 2FA,
   legal name, US$99 [S13]. Accept any pending agreements in the developer account (notarization
   is blocked until they're accepted **[unverified]**).
2. Create the **Developer ID Application** certificate. Either Xcode → Settings → Accounts → Manage
   Certificates → "+" → Developer ID Application, or a CSR from Keychain Access uploaded under
   Certificates, IDs & Profiles [S11]. Check with `security find-identity -v -p codesigning`.
3. Export the certificate and its private key as a `.p12` with a strong password. Store the file
   and password in the password manager.
   - `APPLE_CERTIFICATE` = `base64 -i cert.p12`
   - `APPLE_CERTIFICATE_PASSWORD` = the export password
   - `APPLE_SIGNING_IDENTITY` = the exact string "Developer ID Application: <Name> (<TEAMID>)"
4. App Store Connect → Users and Access → Integrations → App Store Connect API → **Team Keys** →
   Generate (Developer role) [S14]. Store `APPLE_API_ISSUER` (issuer id), `APPLE_API_KEY` (key id)
   and `APPLE_API_KEY_P8` (the `.p8` content). Check locally with
   `xcrun notarytool history --key … --key-id … --issuer …`.
5. Run `pnpm tauri signer generate -w ~/.tauri/pagelamp-updater.key` with a password. Store the key
   and password, and give the **public** key to the frontend developer for `tauri.conf.json`.
6. GitHub → Settings → Environments → `release`:
   - deployment tags `v*`;
   - required reviewer = owner;
   - add all secrets **there**, not at repo level [S29].
   - Also turn on immutable releases [S28].
7. Run the new **release-rehearsal** workflow (§7.3), then test on a Mac:
   1. Install from a **browser download** in a fresh macOS user account.
   2. Open without Privacy & Security.
   3. Connect Claude Desktop and check that it starts the sidecar.
   4. Upgrade from the 0.1 draft build and note any keychain prompt.
   5. Update N→N+1 through the updater **while Claude Desktop is running `pagelamp mcp`**.
8. Swift shell: reuse the same certificate and API key in `apps/macos/scripts/build-app.sh`, adding
   `notarytool` and `stapler` there.

### 3.6 Universal build or one per architecture
- Rust moved `x86_64-apple-darwin` to Tier 2 in 1.90 because GitHub dropped its free Intel runners
  [S39]. macOS 26 is the last release for Intel Macs (secondary sources) [S40].
- A universal DMG is about twice the size, but it means one notarization, one DMG and no
  "aarch64 or x64?" question in the release notes.
- `latest.json` points both `darwin-*` keys at the one universal `.app.tar.gz`.

---

## 4. Windows: options for an individual in Canada (Sept 2026)

| Option | Cost | Eligible? | Notes |
|---|---|---|---|
| **Azure Artifact Signing** (formerly Trusted Signing), Basic | ≈ US$9.99 a month, 5,000 signatures a month [S18][S19] | **Yes.** Individuals in the US and Canada only (quickstart, 2026-05-21) [S17] | Needs a **paid** Azure subscription; free, trial and sponsored subscriptions are refused, which by my reading includes Azure for Students [S20]. The billing account must be type Individual, with legal name and sold-to address matching a government ID. Identity check through AU10TIX + Microsoft Authenticator Verified ID; the check has an expiry and must be renewed; three failed attempts ends onboarding [S17][S20]. The certificate shows your **legal name and city/province/country** [S17]. Certificates last 3 days, so always timestamp (`http://timestamp.acs.microsoft.com`) [S21]. No EV certificates [S20]. Works in CI through the signtool + dlib or `Azure/artifact-signing-action` v2.0.0 [S21][S41] |
| **SignPath Foundation** | Free for OSS [S23] | Probably. Apache-2.0 is OSI-approved; no proprietary parts | Requirements [S22]: MFA for all members, defined roles, a "code signing policy" page with a credit line, and privacy terms (show the privacy policy at install and let users turn data collection off). The Foundation's name is on the certificate [S23]. A third party is in the release path |
| Certum Open Source Code Signing (cloud) | ≈ US$49.99 a year (reseller page) [S42] **[unverified with Certum]** | Individuals only | The certificate subject reads "Open Source Developer, <name>". Not for commercial distribution. The key sits in a cloud HSM (SimplySign) that needs interactive 2FA, which is awkward in CI |
| OV certificate (DigiCert, Sectigo, …) | US$150–300 a year [S18] | Yes | HSM required since June 2023 [S18]; certificates last at most 460 days since 2026-03-01 (CA/B CSC-31) [S25] |
| EV certificate | US$400+ a year | — | **No SmartScreen advantage since 2024** [S18][S24] |
| Microsoft Store | Free developer account; the Store re-signs MSIX [S18] | Yes | Tauri's Store path uses a signed EXE/MSI, which brings us back to the options above [S43]. Whether AI apps can start a sidecar inside an MSIX install is **[unverified]**. Poor fit for the MCP model |
| Unsigned (today) | 0 | — | SmartScreen shows "More info → Run anyway"; **Smart App Control blocks unsigned files** outright [S24] |

SmartScreen: signing doesn't make the warning disappear. Reputation builds per file hash and per
publisher certificate. Signing every release with the same identity lets new releases inherit it,
but "it can take several weeks and hundreds of clean installs" [S24]. Beta users should be told
they may see one prompt.

**Recommendation.** Use Artifact Signing Basic, and apply to SignPath now as the free fallback.

CI:
- Run `azure/login` with OIDC (a federated credential for `environment:release`, so no client
  secret).
- Install SignTool from `Microsoft.Windows.SDK.BuildTools` and the dlib from
  `Microsoft.ArtifactSigning.Client` [S21].
- Add a CI-only config overlay (`tauri build --config src-tauri/tauri.ci-windows.conf.json`)
  whose `signCommand` runs:
  `signtool sign /fd SHA256 /tr http://timestamp.acs.microsoft.com /td SHA256 /dlib …\Azure.CodeSigning.Dlib.dll /dmdf …\metadata.json %1`
  Tauri then signs the sidecar, the NSIS plugins, the installer and the uninstaller [S9].
- Sign the standalone CLI `pagelamp.exe` in the `cli` job with the same command.
- **[unverified]:** that the dlib's `DefaultAzureCredential` picks up the `azure/login` session;
  check in the rehearsal.

---

## 5. Linux

- AppImage signatures (`SIGN=1`, gpg) are embedded but **not checked by the AppImage runtime**,
  so they add little [S44]. Tauri can sign RPMs (`TAURI_SIGNING_RPM_KEY`) [S45], but that only
  matters once there's a repository.
- **Do:**
  - keep `SHA256SUMS`;
  - add `actions/attest@v4` with `subject-checksums: SHA256SUMS` (free for public repos; needs
    `id-token`, `attestations` and `artifact-metadata` write) [S26][S27];
  - turn on immutable releases, which lock assets and tag and add a release attestation [S28].
  - Users verify with `gh attestation verify <file> --repo Euswbnix/pagelamp`.
- The updater's minisign signature already protects AppImage updates.

---

## 6. Extraction worker

### 6.1 Goal and threat model
Today extraction runs inside the sync process. It has caps (200 MB file, 64/256 MB inflate,
5,000 pages), but content streams under the cap can still grow ≈110× in memory, and LZW isn't
measured (post-beta-followups). The parsers (`pdf-extract`, `lopdf`, `zip`, `quick-xml`) are pure
Rust, so the realistic dangers are **memory blow-up, runaway CPU, stack overflow and abort**, not
code execution. Goal: one bad file can cost a failed material, never the sync, the desktop app or
the student's laptop.

### 6.2 How each OS can limit a child process
- **Linux:** `setrlimit(RLIMIT_AS | RLIMIT_CPU | RLIMIT_CORE | RLIMIT_FSIZE | RLIMIT_NOFILE)`
  works. Add `PR_SET_PDEATHSIG(SIGKILL)`. Landlock (`landlock` crate, MIT/Apache) and seccomp are
  optional hardening; Codex uses bwrap + seccomp, with Landlock as fallback [S46].
- **macOS:** `RLIMIT_AS` isn't enforced, and on arm64 `RLIMIT_DATA` returns EINVAL for anything
  below ≈418 GB [S33][S34]. The limits that do work are `RLIMIT_CPU` (SIGXCPU),
  `RLIMIT_CORE=0` and `RLIMIT_FSIZE=0` **[expected; prove in CI on macOS]**. For memory, use an
  **in-process allocator cap** and, as a backstop, have the parent poll the child's footprint with
  `proc_pid_rusage` and kill it above the cap.
  - Seatbelt (`sandbox-exec` / `sandbox_init`) is deprecated but still used in production by Codex
    [S46]. It's the phase-2 option (deny network, allow reading one file).
- **Windows:** Job Object [S35][S36]:
  - `JOB_OBJECT_LIMIT_PROCESS_MEMORY`: committing beyond the limit fails.
  - `JOB_OBJECT_LIMIT_PROCESS_TIME`: the process is terminated.
  - `KILL_ON_JOB_CLOSE`: children die with the parent.
  - `DIE_ON_UNHANDLED_EXCEPTION`: no crash dialog.
  - `ACTIVE_PROCESS=1`: the worker can't start other processes.

  Spawn with `CREATE_NO_WINDOW` so no console window flashes when the GUI app starts the worker.
  Crate: `win32job` (MIT/Apache), or `windows-sys` directly.
- Crates on crates.io today: `rlimit` 0.11 (MIT), `win32job` 2.0.3 (MIT/Apache), `landlock` 0.4.7,
  `process-wrap` 10.0.1 (Apache/MIT) [S47]. All pass the `cargo deny` licence policy.

### 6.3 Design
- **Process model:** one worker process per file. That gives the strongest isolation and a fresh
  budget per file. Start with sequential calls; allow 2 in parallel later. Spawning costs a few ms
  on macOS/Linux and more on Windows **[measure]**. If it adds up, add a batch mode later: the
  worker takes several requests, the parent enforces per-request timeouts and restarts it after
  any failure.
- **Program:** the hidden `pagelamp extract-worker --protocol 1` subcommand of the **existing
  sidecar**:
  - It's already built, signed, notarized and shipped next to every app: Tauri
    `Contents/MacOS/pagelamp` or `%LOCALAPPDATA%\PageLamp\pagelamp.exe`, the Swift shell's
    `Contents/MacOS/pagelamp`, and `/usr/bin/pagelamp` from deb/rpm.
  - The CLI starts `current_exe()`. Desktop reuses `backend::pagelamp_binary()`. The Swift shell
    passes `Bundle.main.url(forAuxiliaryExecutable: "pagelamp")`.
- **Environment:** `env_clear()`, then set only what's needed. This keeps the secret overrides
  (`PAGELAMP_SECRET_*`) away from the worker. Working directory is a temp dir.
- **Order of operations:** the worker blocks reading stdin. The parent applies the limits (Windows
  Job Object assignment) before it writes the request, so there's no race even though std can't
  create a suspended process.
- **Request** (stdin, one JSON line, ≤ 64 KB):
  `{"protocol":1,"path":"/abs/file.pdf","mime":"application/pdf","limits":{"heap_bytes":1073741824,"cpu_secs":120,"max_text_bytes":5242880}}`.
  The worker applies its own rlimits first, then opens the file read-only.
- **Response** (stdout, one JSON document, exit 0):
  `{"protocol":1,"worker_version":"0.3.0","ok":{"segments":[{"locator":"p. 1","text":"…"}]}}` or
  `{"protocol":1,"err":{"kind":"unsupported|failed|io","message":"…"}}`.
  - The parent caps stdout at ≈ 2 × `max_text_bytes` + 1 MB and keeps the last 64 KB of stderr.
  - A version mismatch falls back to in-process extraction, with a warning.
- **Memory:** a `#[global_allocator]` wrapper in `apps/pagelamp-cli` counts live bytes.
  - The limit is `usize::MAX` unless the binary runs as the worker, so the extra cost elsewhere
    is one atomic add per allocation.
  - Past the limit it returns null. Rust's default handler prints "memory allocation of N bytes
    failed" and aborts, and the parent classifies that as `memory_limit`. Stable Rust is enough.
  - Linux `RLIMIT_AS` (e.g. 4 GiB) and the Windows job commit limit (≈1.5× the cap) are the OS
    backstops.
  - **[unverified]:** whether `zlib-rs` (flate2's backend in our lockfile) allocates through the
    Rust allocator; if it uses libc malloc, only the OS backstops catch it.
- **Time:** `RLIMIT_CPU` / job time ≈ 120 s, plus a parent wall-clock timeout (120 s, scaled by file
  size) that kills the worker.
- **Stack:** run extraction on a thread with an explicit stack size. An overflow now kills only the
  worker, which later lets `pdf_check`'s heuristics be relaxed.
- **Failure classes** stored with the material:
  - `timed_out`, `cpu_limit`, `memory_limit` ("too complex to read")
  - `crashed` (signal or exception code)
  - `bad_output`
  - `spawn_failed` (antivirus, missing binary): falls back to in-process with the current caps,
    with a `SyncEvent::Warning` and a `doctor` check.

  Surface them as `text_status = error` plus a new `error_kind`, so the UI can say "This file is
  too complex to read — open it yourself."
- **Crash reporting (local only, no telemetry):**
  - The worker never writes `last-crash.json` or the log file. Its panics and aborts are expected.
  - The parent logs the class, exit status and a clipped, redacted stderr tail. No file paths or
    names go into reports ("Course N" rule).
  - `diagnostic_report` gains counters such as "3 files couldn't be read: 2 too complex,
    1 timed out".
  - Set `RLIMIT_CORE=0` so no core dump holds course material.
- **Facade API** (FFI-friendly, plain types): `App::set_extract_worker(path: Option<PathBuf>)` (or
  an `open_*` option), plus `ExtractWorker::discover()`, which returns the `pagelamp` next to
  `current_exe()`. `pagelamp-extract` gets `worker::{serve, Client}`; `Segment` gets serde
  derives. `pagelamp-core::ingest` passes the client in as the `extract` closure. MCP is
  unchanged: it never extracts.
- **Tests:** `apps/pagelamp-cli/tests/extract_worker.rs` starts `CARGO_BIN_EXE_pagelamp` with tiny
  limits (heap 64 MB, 5 s) on synthetic bombs built with `lopdf` (already a dev-dependency). It
  checks each classification. It runs in the existing Rust CI job on all 3 OSes.

---

## 7. Concrete changes

### 7.1 `release.yml`
1. Add `environment: release` to `app`, `cli` and the new manifest job. Keep permissions per job:
   `contents: write`, and `id-token: write` only where OIDC is used.
2. `app` matrix:
   - macOS `--target universal-apple-darwin` (toolchain targets
     `aarch64-apple-darwin,x86_64-apple-darwin`);
   - Windows `--bundles nsis --config src-tauri/tauri.ci-windows.conf.json`;
   - Linux `--bundles appimage,deb,rpm`.
   - Append `-- --locked` (post-beta follow-up) **[verify that tauri passes it through to
     cargo]**.
3. tauri-action changes:
   ```yaml
   - uses: tauri-apps/tauri-action@<full-sha>  # v1.0.0
     env:
       GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
       TAURI_SIGNING_PRIVATE_KEY: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY }}
       TAURI_SIGNING_PRIVATE_KEY_PASSWORD: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY_PASSWORD }}
       APPLE_CERTIFICATE: ${{ runner.os == 'macOS' && secrets.APPLE_CERTIFICATE || '' }}
       APPLE_CERTIFICATE_PASSWORD: ${{ runner.os == 'macOS' && secrets.APPLE_CERTIFICATE_PASSWORD || '' }}
       APPLE_SIGNING_IDENTITY: ${{ runner.os == 'macOS' && secrets.APPLE_SIGNING_IDENTITY || '' }}
       APPLE_API_ISSUER: ${{ runner.os == 'macOS' && secrets.APPLE_API_ISSUER || '' }}
       APPLE_API_KEY: ${{ runner.os == 'macOS' && secrets.APPLE_API_KEY || '' }}
       # APPLE_API_KEY_PATH exported by a previous step that writes APPLE_API_KEY_P8 to $RUNNER_TEMP
     with:
       projectPath: apps/desktop
       tauriScript: pnpm tauri
       releaseId: ${{ needs.create-release.outputs.release_id }}
       tagName: ${{ env.TAG }}
       releaseDraft: true
       uploadUpdaterJson: false
       args: ${{ matrix.args }}
   ```
   Earlier steps: on macOS, write the `.p8`; on Windows, `azure/login` (OIDC), fetch SignTool and
   the dlib, and write `metadata.json`. Check at run time that empty `APPLE_*` values don't
   confuse Tauri on Windows/Linux; if they do, split the step by OS.
4. Verification steps after the build (fail the job on error):
   - macOS: `codesign --verify --deep --strict`; `codesign -dv` on `Contents/MacOS/pagelamp`,
     checking for the Developer ID authority and the `runtime` flag; `xcrun stapler validate`;
     `spctl -a -vv -t exec`.
   - Windows: `signtool verify /pa /v` on the setup exe and on `pagelamp.exe` inside the install
     dir (or the staged sidecar).
5. `cli` job:
   - macOS: import the `.p12` into a temporary keychain with the `security` commands from the Tauri
     docs [S11], then codesign with runtime + timestamp, zip and `notarytool submit --wait`.
   - Windows: signtool + dlib.
   - Linux: unchanged.
6. New job `updater-manifest` (needs `app`): run `.github/scripts/updater-manifest.mjs`, which
   builds `latest.json` from the draft's `.sig` assets with browser download URLs and checks that
   every platform is present, then run `gh release upload "$TAG" latest.json --clobber`.
7. `checksums` job (needs `app`, `cli`, `updater-manifest`):
   - **remove the step that deletes `.app.tar.gz`**;
   - keep `SHA256SUMS`;
   - add `actions/attest@<sha>  # v4` with `subject-checksums: assets/SHA256SUMS`, and
     permissions `id-token`, `attestations`, `artifact-metadata: write` [S27].
8. Change the header comment ("Builds are unsigned for now…") and the release-notes template.
9. Pin `actions/*`, `pnpm/action-setup`, `dtolnay/rust-toolchain`, `Swatinem/rust-cache`,
   `tauri-apps/tauri-action`, `azure/login`, `actions/attest` and `EmbarkStudios/cargo-deny-action`
   to full SHAs, with a comment giving the version [S30].

### 7.2 New workflow `channels.yml`
`on: release: types: [published]`. Download the release's `latest.json`. For a pre-release,
deploy it to Pages as `updates/beta.json`. For a stable release, deploy it as both
`updates/stable.json` and `updates/beta.json`. Use `actions/deploy-pages` with
`pages: write, id-token: write`, and keep the published JSON files in a `pages` branch so each
deploy includes the other channel's file.

### 7.3 New workflow `release-rehearsal.yml`
`workflow_dispatch`, same environment. It builds signed, notarized artifacts as **workflow
artifacts** (`uploadWorkflowArtifacts`, no release) and runs every §7.1 verification. Run it
before each tag, and first as soon as the secrets exist.

### 7.4 `ci.yml`
1. Add `windows-latest` to the `desktop` matrix (post-beta follow-up).
2. New job `release-config` (Ubuntu) checks:
   - `plugins.updater.pubkey` is present;
   - `createUpdaterArtifacts === true`;
   - bundle targets contain no `msi`;
   - `bundle.macOS.entitlements` is absent or empty;
   - `node --test .github/scripts/updater-manifest.test.mjs` passes (fixture assets → expected
     keys, and a missing platform fails).
3. The worker integration tests run inside the existing `rust` job on all 3 OSes.
4. Pin actions to SHAs, as in release.yml.

### 7.5 Files owned by other people (for the leader to hand out)
- **Frontend** (`apps/desktop`):
  - `tauri-plugin-updater` (Rust only), commands `updates_check` and `updates_install` (progress
    channel), the comparator (§2.5), channel endpoints, Settings → Updates, post-update banner;
  - bundle-type gating for deb/rpm;
  - `src-tauri/windows/hooks.nsh` (renames a running sidecar) and `tauri.ci-windows.conf.json`;
  - `bundle.targets` without MSI; `createUpdaterArtifacts: true`; `plugins.updater.pubkey`.
- **Backend** (`crates/*`, CLI): `pagelamp-extract::worker`, the `extract-worker` subcommand, the
  counting allocator, limits for each OS, the facade `set_extract_worker`, failure kinds in the
  store (additive migration), diagnostics counters, tests.
- **Leader:**
  - workflows and scripts;
  - PRIVACY.md (update check);
  - README / release-notes install sections (remove "Open Anyway" and `xattr`, add the SmartScreen
    note);
  - ARCHITECTURE §5 (worker API, migration rule) and §7 (move items to v0.3);
  - SECURITY.md (verifying downloads: checksums, attestations).
  - If SignPath is chosen, a `CODE_SIGNING.md` page.

---

## 8. What changed since 2025

- **Tauri updater 2.10–2.13:**
  - deb/rpm/NSIS/MSI updates, with `{os}-{arch}-{installer}` manifest keys;
  - `dangerousAcceptInvalidCerts` and `no_proxy` options;
  - `restart_after_install` on Windows;
  - `allowDowngrades` moved into the config (breaking, 2.12);
  - minimum Rust version 1.90 [S4].
- **tauri-action v1.0.0 (2026-06-29):**
  - manifest URLs now use the API;
  - versioned `.app.tar.gz` names;
  - fails on a draft mismatch;
  - renamed and removed updater inputs [S6].
- **Azure "Trusted Signing" became "Artifact Signing":**
  - individuals in the US and Canada are supported;
  - organisations in the EU, UK, AU, NZ, JP, KR, SG, CH, NO and IL were added;
  - the GitHub Action is now `Azure/artifact-signing-action` (v2.0.0, 2026-05-14)
    [S17][S41].
- **CA/B Forum CSC-31:** public code signing certificates last at most **460 days** from
  2026-03-01 [S25].
- **Microsoft docs (2026):**
  - EV certificates bring no SmartScreen advantage;
  - Smart App Control blocks unsigned files;
  - Microsoft Store developer accounts are free, and the Store re-signs MSIX [S18][S24].
- **Apple:**
  - Sequoia removed the Control-click bypass [S15];
  - macOS 26 is the last release for Intel Macs [S40];
  - individual API keys can't use notarytool [S14].
- **Rust:** `x86_64-apple-darwin` is Tier 2 from 1.90 [S39].
- **GitHub:**
  - immutable releases, with release attestations [S28];
  - `actions/attest` v4 replaces `attest-build-provenance` (now a wrapper) [S26][S27].

---

## 9. Risks

| Risk | Impact | Mitigation |
|---|---|---|
| Updater key lost | No auto-updates for existing installs, ever | Password manager plus offline copy; write down the rotation procedure |
| Updater key or signing secrets leaked | Malware pushed to every install; Gatekeeper doesn't re-check updates | `release` environment with reviewer, SHA-pinned actions, no secrets in PR workflows, immutable releases |
| Versions compared on the numeric bundle version | Beta users never get the final release | Custom comparator (§2.5), with a test |
| Windows sidecar locked during an update | Installer error, or an old `pagelamp.exe` kept | NSIS rename hook; tell users to quit the AI app; test |
| deb/rpm falls back to the generic manifest key | `dpkg -i` run on an AppImage, or sudo prompts | Don't auto-install deb/rpm; don't publish a generic `linux-x86_64` key |
| API-URL manifest (tauri-action v1) and the 60/h/IP limit behind campus NAT **[unverified that asset downloads count]** | Update failures in bursts | Own manifest with browser download URLs |
| Install location changes (MSI→NSIS, the Swift shell alongside Tauri) | MCP configs point at a missing file | One-time re-copy; later a `doctor` check that reads client configs (read-only) for stale paths |
| Keychain prompts after the signing change | A confused student, or a failed sync in the CLI | Test; release-note line "choose Always Allow" |
| Artifact Signing identity check fails (3 attempts) or needs a subscription type the owner lacks | No Windows signing | SignPath application in parallel; Certum as last resort |
| SmartScreen warns even for signed builds at first | Support questions | Beta notes; consistent identity; reputation builds over weeks [S24] |
| Notarization delays, outages or unaccepted agreements | Release blocked | Rehearsal workflow; `--skip-stapling` pass as a manual fallback [S11] |
| Worker memory cap missed on macOS (non-Rust allocations) | The machine starts swapping on a hostile PDF | Parent footprint polling as backstop; keep today's inflate caps |
| Worker blocked by antivirus or Smart App Control (unsigned) | Extraction falls back to in-process | Sign the sidecar (it's the same binary) |
| Privacy promise broken by the update check | Wrong statement in PRIVACY.md | Update PRIVACY.md; add a toggle |

---

## 10. Order of work (start of v0.3)

1. **Owner (now; lead time days to weeks):**
   - Apple enrolment;
   - paid Azure subscription with an Individual billing account → Artifact Signing account →
     identity validation;
   - optional SignPath application;
   - updater key;
   - GitHub `release` environment; immutable releases.
2. **Leader:** pin actions and move secrets into the environment; macOS app + CLI signing and
   notarization with checks; universal DMG; rehearsal workflow; manifest script and tests; stop
   deleting `.app.tar.gz`; attestations.
3. **Frontend:** updater integration (comparator, channels, UI, deb/rpm gating), NSIS hook,
   drop MSI, pubkey. Then the first signed pre-release (e.g. `v0.3.0-alpha.1`); everyone
   reinstalls once.
4. **Backend (can run alongside step 3):** extraction worker behind the facade, then the Swift
   shell and desktop pass the path.
5. **Leader:** Windows signing once identity validation completes; channels.yml; docs (PRIVACY,
   README, SECURITY, ARCHITECTURE §5/§7).

---

## 11. Questions only the owner can answer

1. Are you comfortable with **your legal name** appearing publicly in both certificates? That's
   Apple's "Developer ID Application: <Name>" and Azure's certificate, which also shows
   city/province/country. The alternative is an organisation, which needs a legal entity (plus
   D-U-N-S for Apple).
2. **Azure eligibility:** can you open a **paid** Azure subscription whose Individual billing
   account has the same legal name and address as a government photo ID? Can you prove a Canadian
   address (bank statement or utility bill)? Azure for Students and trial credits don't qualify.
3. Or would you rather have **SignPath Foundation** (free; the Foundation's name as publisher;
   publish a code-signing-policy page)? Or both, with SignPath as fallback?
4. Should updates be **checked automatically by default**, with disclosure at onboarding, or off
   until turned on? Install automatically, or always ask?
5. Which **channel** should pre-release installs follow by default (proposed: beta), and who is on
   beta?
6. OK to **drop MSI** (NSIS per-user only) and ship a **single universal macOS DMG**?
7. Where should update manifests live: `euswbnix.github.io/pagelamp/updates/…`, or a domain you
   own (e.g. `pagelamp.dev`, if you have it)? A custom domain adds renewal risk.
8. Where will the **updater private key** and `.p12` be kept (which password manager, and is there
   an offline backup)?
9. Does the updater go into **v0.1.0 final** if enrolment arrives in time, or wait for the first
   v0.3 pre-release?
10. Who is the **required reviewer** for the `release` environment? Only you? Should "Prevent
    self-review" stay off, since you're the only maintainer?
11. **Linux:** keep `.rpm`? Is "AppImage updates itself; deb/rpm shows a download link" acceptable?
12. **Branded builds (UTMCSSA):** should they be signed with your Developer ID and use your update
    key, or does UTMCSSA plan its own accounts?

---

## 12. Sources (all accessed 2026-09-27)

- S1 Tauri docs, Updater plugin (source `plugin/updater.mdx`): https://v2.tauri.app/plugin/updater/ · https://github.com/tauri-apps/tauri-docs/blob/v2/src/content/docs/plugin/updater.mdx
- S3 tauri-plugin-updater source `src/updater.rs` and `src/lib.rs` (v2 branch): https://github.com/tauri-apps/plugins-workspace/blob/v2/plugins/updater/src/updater.rs · https://github.com/tauri-apps/plugins-workspace/blob/v2/plugins/updater/src/lib.rs
- S4 tauri-plugin-updater CHANGELOG (2.10.0–2.13.0): https://github.com/tauri-apps/plugins-workspace/blob/v2/plugins/updater/CHANGELOG.md
- S5 tauri-action README (inputs, `releaseId`/`tagName` notes): https://github.com/tauri-apps/tauri-action/blob/dev/README.md
- S6 tauri-action CHANGELOG v1.0.0 and releases list: https://github.com/tauri-apps/tauri-action/blob/dev/CHANGELOG.md · https://api.github.com/repos/tauri-apps/tauri-action/releases · PR/issue https://github.com/tauri-apps/tauri-action/issues/1297
- S7 Tauri WiX template `main.wxs` (MajorUpgrade): https://github.com/tauri-apps/tauri/blob/dev/crates/tauri-bundler/src/bundle/windows/msi/main.wxs
- S8 tauri-bundler version handling (`msi/mod.rs` `convert_version`, `nsis/mod.rs` `try_add_numeric_build_number`): https://github.com/tauri-apps/tauri/blob/dev/crates/tauri-bundler/src/bundle/windows/msi/mod.rs · https://github.com/tauri-apps/tauri/blob/dev/crates/tauri-bundler/src/bundle/windows/nsis/mod.rs
- S9 Tauri NSIS template and utils (`CheckIfAppIsRunning`, WiX migration) and sidecar signing in `bundle.rs`: https://github.com/tauri-apps/tauri/blob/dev/crates/tauri-bundler/src/bundle/windows/nsis/installer.nsi · https://github.com/tauri-apps/tauri/blob/dev/crates/tauri-bundler/src/bundle/windows/nsis/utils.nsh · https://github.com/tauri-apps/tauri/blob/dev/crates/tauri-bundler/src/bundle.rs
- S10 Tauri docs, macOS code signing: https://v2.tauri.app/distribute/sign/macos/ (source https://github.com/tauri-apps/tauri-docs/blob/v2/src/content/docs/distribute/Sign/macos.mdx)
- S11 tauri-bundler macOS signing and notarization (`macos/app.rs`, `macos/sign.rs`, `macos/dmg/mod.rs`, `tauri-macos-sign/src/lib.rs`), CLI identity override (`tauri-cli/src/interface/rust.rs` l.1446), config (`tauri-utils/src/config.rs`): https://github.com/tauri-apps/tauri/tree/dev/crates/tauri-bundler/src/bundle/macos · https://github.com/tauri-apps/tauri/blob/dev/crates/tauri-macos-sign/src/lib.rs · https://github.com/tauri-apps/tauri/blob/dev/crates/tauri-cli/src/interface/rust.rs · https://github.com/tauri-apps/tauri/blob/dev/crates/tauri-utils/src/config.rs
- S12 Apple, Notarizing macOS software before distribution, and Customizing the notarization workflow: https://developer.apple.com/documentation/security/notarizing-macos-software-before-distribution · https://developer.apple.com/documentation/security/customizing-the-notarization-workflow
- S13 Apple Developer Program enrolment (US$99; individual legal name): https://developer.apple.com/programs/enroll/
- S14 Apple, Creating API keys for App Store Connect API (team vs individual; individual keys can't use notaryTool; one-time download): https://developer.apple.com/documentation/appstoreconnectapi/creating-api-keys-for-app-store-connect-api
- S15 Apple Developer News, "Updates to runtime protection in macOS Sequoia" (2024-08-06): https://developer.apple.com/news/?id=saqachfa
- S16 Apple, Signing Mac Software with Developer ID (Account Holder only): https://developer.apple.com/developer-id/
- S17 Microsoft Learn, Artifact Signing quickstart (ms.date 2026-05-21; eligibility, individual validation): https://learn.microsoft.com/en-us/azure/artifact-signing/quickstart · https://github.com/MicrosoftDocs/azure-docs/blob/main/articles/artifact-signing/quickstart.md
- S18 Microsoft Learn, Code signing options for Windows app developers (2026-08-29): https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/code-signing-options
- S19 Azure pricing, Artifact Signing (Basic 5,000 / Premium 100,000 signatures a month): https://azure.microsoft.com/en-us/pricing/details/artifact-signing/
- S20 Microsoft Learn, Artifact Signing FAQ (updated 2026-08-14; paid subscription, no EV, identity validation): https://learn.microsoft.com/en-us/azure/artifact-signing/faq
- S21 Microsoft Learn, Artifact Signing signing integrations (SignTool + dlib, timestamp URL, 3-day certificates): https://github.com/MicrosoftDocs/azure-docs/blob/main/articles/artifact-signing/how-to-signing-integrations.md
- S22 SignPath Foundation terms: https://signpath.org/terms
- S23 SignPath Foundation home: https://signpath.org/
- S24 Microsoft Learn, SmartScreen reputation for Windows app developers (2026-05-04; Smart App Control): https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/smartscreen-reputation
- S25 CA/Browser Forum, Ballot CSC-31 Maximum Validity Reduction: https://cabforum.org/2025/11/17/ballot-csc-31-maximum-validity-reduction/
- S26 actions/attest-build-provenance README (v4 wraps actions/attest; public repos): https://github.com/actions/attest-build-provenance
- S27 actions/attest README (v4.2.2; permissions; `subject-checksums`): https://github.com/actions/attest
- S28 GitHub Docs, Immutable releases: https://docs.github.com/en/code-security/supply-chain-security/understanding-your-software-supply-chain/immutable-releases
- S29 GitHub Docs, Managing environments for deployment: https://docs.github.com/en/actions/how-tos/deploy/configure-and-manage-deployments/manage-environments
- S30 CISA alert, tj-actions/changed-files compromise (CVE-2025-30066): https://www.cisa.gov/news-events/alerts/2025/03/18/supply-chain-compromise-third-party-tj-actionschanged-files-cve-2025-30066-and-reviewdogaction
- S31 GitHub Docs, REST API rate limits (60 per hour unauthenticated, per IP): https://docs.github.com/en/rest/using-the-rest-api/rate-limits-for-the-rest-api
- S33 Apple Developer Forums thread 702803 (arm64 `RLIMIT_DATA` EINVAL below 418301149184): https://developer.apple.com/forums/thread/702803
- S34 avast/retdec issue #379 "Memory limiting does not work on macOS" (secondary): https://github.com/avast/retdec/issues/379
- S35 Microsoft Learn, JOBOBJECT_BASIC_LIMIT_INFORMATION: https://learn.microsoft.com/en-us/windows/win32/api/winnt/ns-winnt-jobobject_basic_limit_information
- S36 Microsoft Learn, JOBOBJECT_EXTENDED_LIMIT_INFORMATION: https://learn.microsoft.com/en-us/windows/win32/api/winnt/ns-winnt-jobobject_extended_limit_information
- S37 Tauri docs, Windows installer (NSIS hooks, install modes): https://github.com/tauri-apps/tauri-docs/blob/v2/src/content/docs/distribute/windows-installer.mdx
- S38 GitHub REST, releases of Euswbnix/pagelamp (empty list = no published release): https://api.github.com/repos/Euswbnix/pagelamp/releases
- S39 Rust blog, Demoting x86_64-apple-darwin to Tier 2 with host tools (2025-08-19): https://blog.rust-lang.org/2025/08/19/demoting-x86-64-apple-darwin-to-tier-2-with-host-tools/
- S40 Thurrott, "macOS 26 Tahoe Will Be the Last Release to Support Intel-Based Macs" (secondary; Apple WWDC25 statement): https://www.thurrott.com/apple/321965/macos-26-tahoe-will-be-the-last-release-to-support-intel-based-macs
- S41 Azure/artifact-signing-action (renamed from trusted-signing-action; v2.0.0 2026-05-14): https://github.com/Azure/artifact-signing-action
- S42 Certum Open Source Code Signing in the Cloud (store/reseller pages; price secondary): https://certum.store/open-source-code-signing-on-simplysign.html
- S43 Tauri docs, Microsoft Store: https://github.com/tauri-apps/tauri-docs/blob/v2/src/content/docs/distribute/microsoft-store.mdx
- S44 Tauri docs, Linux code signing (AppImage signature not checked at run time): https://github.com/tauri-apps/tauri-docs/blob/v2/src/content/docs/distribute/Sign/linux.mdx
- S45 Tauri docs, RPM (signing with `TAURI_SIGNING_RPM_KEY`): https://github.com/tauri-apps/tauri-docs/blob/v2/src/content/docs/distribute/rpm.mdx
- S46 OpenAI Codex docs, Sandbox (Seatbelt on macOS; bwrap/seccomp/Landlock on Linux): https://developers.openai.com/codex/concepts/sandboxing
- S47 crates.io API for rlimit, win32job, landlock, process-wrap (licences and versions): https://crates.io/api/v1/crates/rlimit · https://crates.io/api/v1/crates/win32job · https://crates.io/api/v1/crates/landlock · https://crates.io/api/v1/crates/process-wrap
