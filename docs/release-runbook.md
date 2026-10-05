# Release runbook

How PageLamp releases are built, published and rolled back, from v0.3 on (the updater). The
workflows, in `.github/workflows/`, are `release.yml` (build, sign, draft release),
`channels.yml` (update channels on the `gh-pages` branch) and the `release-config` job in `ci.yml`. The
design is in `docs/design/v0.3-plan.md` (M0.1, M0.3, M0.4, M0.6, M0.7).

## What runs where

| Piece | What it does |
|---|---|
| `release.yml` on a `v*` tag | `preflight` (release configuration, strict) and `deny` (advisories and licences) first; then the signing jobs build, sign and check macOS (one universal app) and Windows; Linux is built in `app-linux-build` (no secret, no cache) and uploaded, with the updater on first signed, by `app-linux`; `updater-manifest` writes `latest.json`; `checksums` checks that the draft holds exactly the checked files, writes `SHA256SUMS` and attests every file. Result: a **draft** release |
| `release.yml` with "rehearsal" | the same builds and checks from `main`, kept as workflow artifacts for 7 days; no release |
| Publishing the draft (by hand) | `channels.yml` checks the release's `latest.json` against its `SHA256SUMS` and copies it to `updates/beta.json`, and for a full release also to `updates/stable.json`, each only if it is newer than what that channel serves |
| `channels.yml` by hand | `promote` (point a channel at a published release: the rollback), `test-publish` / `test-delete` (the test channel) |
| `gh-pages` branch | only what Pages serves: `.nojekyll`, `updates/*.json`, `updates/test/`. Each change replaces it with one commit |

The update channels (D5): `updates/stable.json`, `updates/beta.json` (default for pre-release
installs, D3) and `updates/test.json` (only builds made with the rehearsal overlay read it) on
the `gh-pages` branch. The app asks
`https://raw.githubusercontent.com/Euswbnix/pagelamp/gh-pages/updates/<channel>.json` (the
branch itself; answers are cached for 5 minutes); stable then falls back to
`releases/latest/download/latest.json`. The updater tries the next endpoint after a network
error or an error status but stops at an answer that isn't JSON. A released app asks no other
address for updates: both are on GitHub's own hosts. GitHub Pages serves the same branch, and
only the test channel still uses it (a rehearsal build downloads its test update from there).

**The updater is on** in a build when `apps/desktop/src-tauri/tauri.conf.json` has
`bundle.createUpdaterArtifacts: true` and a real `plugins.updater.pubkey`. Until then a release
builds what v0.1.0 did, apart from the one universal `.dmg` and no `.msi` (D4): no updater files,
no `latest.json`, the Linux packages under the same names as before, and the channels don't
move.

## Owner: one-time setup

1. **The updater key** (A4, by 2026-10-09). On your Mac, in the repo:

   ```sh
   pnpm --dir apps/desktop tauri signer generate -w ~/.tauri/pagelamp-updater.key
   ```

   Choose a strong password when asked. This writes `~/.tauri/pagelamp-updater.key` (the private
   key, encrypted with that password) and `~/.tauri/pagelamp-updater.key.pub` (the public key).
   - Keep both files and the password in your password manager, plus one offline copy. If the
     private key is lost, installed apps can never be updated again; if it leaks, someone with
     write access to a release could push an update to every install.
   - GitHub → Settings → Environments → `release` → Environment secrets → Add secret:
     - `TAURI_SIGNING_PRIVATE_KEY`: the whole content of `pagelamp-updater.key`;
     - `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`: the password.

     Only in the environment `release`, never as repository secrets. The workflows give them only
     to the steps that sign updater files (the macOS bundle step, which makes the updater archive,
     and one `tauri signer sign` step per platform), and every signing job checks first that both
     exist.
   - Send the content of `pagelamp-updater.key.pub` (public, one line) to the frontend: it goes into
     `tauri.conf.json` → `plugins.updater.pubkey`.
2. **GitHub Pages** (A10). The branch `gh-pages` has to exist first: run Actions → **Update
   channels** → Run workflow → "Use workflow from": `main`, action `test-delete` (it creates
   `gh-pages` with `.nojekyll` when it is missing; any later channel change works too). Then
   Settings → Pages → Build and deployment → Source: **Deploy from a branch** → Branch:
   **`gh-pages`**, folder **`/ (root)`** → Save. Pages is needed for the test channel only. After
   the first channel update,
   `https://raw.githubusercontent.com/Euswbnix/pagelamp/gh-pages/updates/beta.json` answers:
   that is the address the app asks.
3. The rest of A10 (plan §5.1): turn on immutable releases, and add a ruleset on `main` that
   blocks force pushes and deletion. The `release` environment (deployment rules: branch `main`
   and tags `v*`; required reviewer: you) is already in place.

## Before a tag

- CI on `main` is green, including **Release config** and **Licences**. Release config warnings
  are errors for a tag: a `tauri.conf.json` version that doesn't fit the Cargo version, `msi` or
  `"all"` in `bundle.targets` (NSIS only, D4, with the updater on or off), and once the updater
  is on a placeholder `plugins.updater.pubkey`.
- `docs/release-notes/<tag>.md` exists (it is the release text and the update's notes).
- The versions agree: `[workspace.package] version` in the root `Cargo.toml` is the tag without
  its `v` (e.g. `0.3.0-alpha.1`); `tauri.conf.json`'s `version` is the same, its numeric part
  (`0.3.0`), or absent. `latest.json` announces the Cargo version, and the workflow signs every
  updater file for that version (the updater refuses a signature made for another version).
- **`Cargo.lock` is in the same commit as the version change.** It records every workspace
  crate's version, and release builds use `--locked`, so a commit that changes only `Cargo.toml`
  fails after you approved the deployments. After editing the version run
  `cargo update --workspace --offline` (or any `cargo metadata --offline >/dev/null`), then
  `git add Cargo.toml Cargo.lock`. Release config fails when they disagree.
- **What's new matches what ships.** Every feature this release turns on for students has its
  `WhatsNewTopic` row in `crates/pagelamp-app/src/updates.rs` (`WHATS_NEW`, introduced in this
  version) and its desktop copy, and no row names a feature that is still behind a switch
  (`REMOVAL_UI`, `CALENDAR_UI`, `AI_SETUP_ENABLED`, `REMINDERS_UI`…) in this build. A row lands in
  the same branch as its feature. A row that first ships in a later release than its
  `introduced_in` version is never shown to people upgrading from that version, so fix the
  version before tagging.
- A rehearsal of that commit passed (next section).

## Rehearsal from main

Actions → **Release** → Run workflow → "Use workflow from": **`main`**, leave "tag" empty, tick
**rehearsal** (or `gh workflow run release.yml --ref main -f rehearsal=true`). Approve the
`release` deployments when GitHub asks (the signing jobs wait for you).

It builds that commit's universal macOS app and `.dmg`, the macOS CLI for each architecture, the
Windows installer and CLI and the Linux packages (with the updater on, also the signed AppImage),
then signs, notarizes and checks everything exactly as a release does. The files are
workflow artifacts for 7 days; their names say "rehearsal". If
`apps/desktop/src-tauri/tauri.rehearsal.conf.json` exists, the apps are built with it, so their
updater reads the test channel. With the updater on, the run
also keeps `updater-test-<commit>-attempt<n>`: the test channel's `latest.json` with the macOS
and Windows updater files.

`preflight` isn't strict in a rehearsal (warnings only), and `deny` doesn't hold the signing jobs
back (a red `deny` still marks the run failed). Nothing is created or changed in any release.

## Making a release

1. Push the tag of the commit that passed the rehearsal (`git tag v0.3.0-alpha.1 <commit>` then
   `git push origin v0.3.0-alpha.1`). Tags with a `-` (alpha, beta, rc) become pre-releases.
2. Approve the `release` deployments. `preflight` and `deny` must pass before anything is built.
3. Check the draft on GitHub → Releases. With the updater on it holds (`<v>` is
   `tauri.conf.json`'s version, e.g. `0.3.0` for `v0.3.0-alpha.1` while it stays numeric):

   | File | For |
   |---|---|
   | `PageLamp_<v>_universal.dmg` | macOS (Apple silicon and Intel) |
   | `PageLamp_<v>_universal.app.tar.gz` + `.sig` | macOS updates |
   | `PageLamp_<v>_x64-setup.exe` + `.sig` | Windows install and updates |
   | `PageLamp_<v>_amd64.AppImage` + `.sig` | Linux, updates itself |
   | `.deb`, `.rpm` | Linux packages (download link in the app, D7) |
   | `pagelamp-<tag>-<target>.tar.gz` / `.zip` + `.sha256` | the CLI alone (4 targets) |
   | `latest.json` | the update manifest |
   | `SHA256SUMS` | every file above (and attested) |

   With the updater off the same list without the `.app.tar.gz`, the `.sig` files and
   `latest.json`. Nothing else can be there: `checksums` refuses to write `SHA256SUMS` (and
   attests nothing) if a checked file changed or any other file is in the draft.
4. **Wait for `SHA256SUMS` in the draft** (the `checksums` job uploads it last, after the
   attestation), then publish the draft. Never publish earlier: `channels.yml` refuses a release
   without `SHA256SUMS`, and once immutable releases are on, nothing can be added after
   publishing. `channels.yml` runs: check that
   `https://raw.githubusercontent.com/Euswbnix/pagelamp/gh-pages/updates/beta.json` (and for a
   full release `stable.json`) shows the new version, at most 5 minutes later. A channel that
   already serves a newer version stays where it is (e.g. `beta` on `0.4.0-alpha.2` when you
   publish the patch `0.3.1`); the run says so.

A failed job: "Re-run failed jobs" on the same run. A new run for an existing tag only with the
tag as ref: `gh workflow run release.yml --ref v0.3.0-alpha.1`.

## The test channel (before alpha.1 is published)

The update loop is tested end to end before any student gets an updater build (plan M0.4 "Test
channel", M0 definition of done 2). You need: the updater on (key and pubkey), Pages on, and the
frontend's `tauri.rehearsal.conf.json` (updater endpoint `updates/test.json`) on `main`.

Use test versions below the coming tag, so the test can be repeated without using up a real
version: N = `0.3.0-alpha.0.1`, N+1 = `0.3.0-alpha.0.2` (both come before `0.3.0-alpha.1`; a
second round goes on with `.0.3` and `.0.4`).

1. **Build N:** set the Cargo workspace version (and `tauri.conf.json`'s, if it has the full
   version) to N, update `Cargo.lock` (`cargo update --workspace --offline`) and commit both, then
   run a rehearsal from `main`. Install its `.dmg` on a Mac and its `-setup.exe` on a Windows PC
   (from the run's artifacts). Connect Claude Desktop so it runs `pagelamp mcp`.
2. **Raise the version on `main`** to N+1 the same way (`Cargo.toml`, `Cargo.lock` and, if it
   has the full version, `tauri.conf.json`, in one commit). Push.
3. **Build N+1:** a second rehearsal from `main`. Note its run ID (the number in the run's URL).
4. **Publish it:** Actions → **Update channels** → Run workflow (from `main`) → action
   **`test-publish`**, `run_id` = N+1's run ID. It refuses anything but a finished rehearsal of
   `release.yml` from `main`, checks the manifest and every signature against the committed public
   key, and serves `updates/test.json` plus the files under `updates/test/<commit>/`. Do this within
   7 days of the rehearsal (the artifact expires).
5. **Update:** on each machine, with Claude Desktop running, open N → Settings → Updates → Check
   now → Install and restart. Check the new version, the post-update banner, and that Claude
   Desktop works again after you quit and reopen it (owner test A11). The link under the release
   notes leads to a page that doesn't exist: no release has a test version.
6. **Clean up:** Update channels → action **`test-delete`**. The files leave `gh-pages` and its
   history. The test machines keep the rehearsal build, whose updater reads only `test.json`
   (now gone: each check reports an error) and never moves to a real channel. Uninstall it and
   install the published release (alpha.1) by hand once it is out.
7. **Before the tag**, set the version to the real one (`0.3.0-alpha.1`), with `Cargo.lock`, as
   in "Before a tag".

Linux isn't on the test channel: the AppImage (~95 MB) is too big to keep in a branch. Its signature is
still checked in every rehearsal.

## Bad release

1. **Stop the spread.** Point each affected channel back at the last good release: Actions →
   **Update channels** → Run workflow (from `main`) → action **`promote`**, `tag` = the last good
   release, `channel` = `beta`, and again with `stable` if the bad release was a full release (a
   full release is on both channels). This stops further updates to the bad version; it can't roll
   anyone back (the updater only moves to a newer version). For a bad full release also run
   `gh release edit <last good tag> --latest`, because the stable channel falls back to
   `releases/latest/download/latest.json` when the channel's own address doesn't answer. Leave
   the bad release and its files in place and add a warning to its notes.
2. **Ship the fix from a new tag** (N+1, e.g. `v0.3.1` or the next `-beta.N`). Never move or reuse
   a tag: immutable releases forbid replacing files in place. Publishing it moves the channels
   forward again.
3. **A broken database migration.** Before every schema migration PageLamp copies the database to
   `pagelamp.db.v<N>.bak` next to `pagelamp.db` (N = the schema version before the migration; only
   the newest copy is kept). The data folder is `~/Library/Application Support/dev.PageLamp.PageLamp`
   on macOS, `%APPDATA%\PageLamp\PageLamp\data` on Windows, `$XDG_DATA_HOME/pagelamp` (usually
   `~/.local/share/pagelamp`) on Linux, or `PAGELAMP_HOME` if set. To go back:
   1. Quit PageLamp and every AI app that runs `pagelamp mcp` (Claude Desktop, Codex, …).
   2. Install the last good release by hand (after step 1 of this list, it won't be updated to the
      bad one again).
   3. In the data folder, rename `pagelamp.db` to `pagelamp.db.broken` (and `pagelamp.db-wal`,
      `pagelamp.db-shm` if they exist), then copy `pagelamp.db.v<N>.bak` to `pagelamp.db`.
   4. Open PageLamp and sync. What Canvas has comes back with the sync; what the student entered
      after the update is lost. Keep `pagelamp.db.broken` for diagnosis, but never attach it to an
      issue: it holds course material.
4. **Find out what happened.** Updater errors go to the PageLamp log (the `logs/` folder of the
   data folder, kept 7 days), and the diagnostic report shows the last update check's result
   (counts and codes only): Settings → Help & feedback → Copy diagnostic report, or
   `pagelamp report`.

## When a check fails

- **preflight:** the message names the file and the rule. Secrets or `github.token` outside a
  step's `env:`/`with:`, unpinned actions, macOS entitlements, a `build.beforeBundleCommand`
  (it would run next to the signing secrets) and a `Cargo.lock` that doesn't carry the workspace
  version always fail; for a tag, so do the version, updater-key and MSI rules above.
- **deny:** a known vulnerability in a dependency. Update the crate (`cargo update -p <crate>`,
  then commit `Cargo.lock`), or, if the advisory can't affect PageLamp, add it to `deny.toml`
  under `[advisories] ignore` with the reason. Unmaintained crates only warn.
- **updater-manifest:** "made for version X … announces Y" means an updater file wasn't signed
  again for the Cargo version (a `tauri signer sign --app-version` step was skipped or changed);
  "another key" means the secret and `plugins.updater.pubkey` aren't a pair.
- **app-linux-build, "Pin the AppImage tools":** a pinned linuxdeploy tool changed upstream (the
  AppImage plugin comes from upstream's `continuous` release, which is replaced now and then).
  Find out why before updating its SHA-256 in `release.yml`; the step's comment says where the
  new digest comes from. A download from `tool-downloads-disabled.invalid` in the bundle step
  means a newer Tauri CLI wants a tool that isn't pinned yet.
- **checksums:** a file in the draft changed after its job checked it, or a file no job checked
  is in the draft (a manual upload, a copy such as `… (1).tar.gz`, a leftover from an earlier
  run). Don't publish; find out who uploaded it, delete it from the draft, re-run `checksums`.
- **Update channels after publishing:** "no SHA256SUMS" means the draft was published before
  `checksums` finished or after it failed, and "built with the updater on but … no latest.json"
  that `updater-manifest` failed or the draft was published too early: no channel moved. If the
  run's jobs can still finish (re-run them; a published release accepts files only while
  immutable releases are off), move the channels with `promote` afterwards; otherwise ship the
  fix from the next tag.

How anyone can check a download: [SECURITY.md → Verifying downloads](../SECURITY.md#verifying-downloads).
