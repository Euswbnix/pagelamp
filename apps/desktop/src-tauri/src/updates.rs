//! In-app updates (v0.3 M0.4). Rust only: the webview has no `updater:*` permission, so it can
//! neither install nor redirect an update; it calls the three commands below.
//!
//! - The channel (stable/beta) and whether a check is due come from the facade
//!   (`App::effective_update_channel`, `startup_tasks`); every check is recorded there (codes
//!   only) for the diagnostic report.
//! - Endpoints are static URLs from `tauri.conf.json` `plugins.pagelamp-updates` (no
//!   `{{target}}`/`{{arch}}`/`{{current_version}}` templating), and requests carry a minimal
//!   `User-Agent: PageLamp/<version>`. So GitHub sees the IP address and the app version, as
//!   with any download, and nothing else. The rehearsal overlay (`tauri.rehearsal.conf.json`)
//!   swaps the endpoints at build time; nothing can change them at runtime.
//! - Versions are compared with the real crate version (`CARGO_PKG_VERSION`, e.g.
//!   `0.3.0-beta.2`), not `tauri.conf.json`'s numeric one, so betas are offered the release.
//! - deb/rpm installs never auto-install: they read the AppImage entry only to learn the new
//!   version and get a link to the release page (the manifest has no deb/rpm keys, by design).

use std::sync::Mutex;
use std::time::Duration;

use pagelamp_app::{
    ActivityKind, AppError, AppErrorKind, UpdateChannel, UpdateCheckOutcome, UpdateCheckRecord,
};
use pagelamp_core::brand;
use semver::Version;
use serde::{Deserialize, Serialize};
use tauri::ipc::Channel;
use tauri::utils::config::BundleType;
use tauri::{AppHandle, Manager, Runtime, State, Url};
use tauri_plugin_updater::{Update, UpdaterExt};

use crate::backend::{Backend, internal};

type CmdResult<T> = Result<T, AppError>;

/// The manifest entry deb/rpm installs read to learn the new version (never installed).
const DOWNLOAD_ONLY_TARGET: &str = "linux-x86_64-appimage";
/// A check or download that hangs shouldn't hold the UI forever.
const CHECK_TIMEOUT: Duration = Duration::from_secs(30);

/// The real version of this build.
pub fn current_version() -> Version {
    Version::parse(env!("CARGO_PKG_VERSION")).expect("the crate version is valid semver")
}

/// The channel when the facade can't say (it can't open, e.g. a database written by a newer
/// PageLamp, which is exactly when an update is needed): the facade's own default rule (D3),
/// beta for a pre-release build, else stable.
pub fn default_channel(version: &Version) -> UpdateChannel {
    if version.pre.is_empty() {
        UpdateChannel::Stable
    } else {
        UpdateChannel::Beta
    }
}

/// Whether `remote` is newer than `current` by full semver, pre-releases included:
/// `0.3.0-beta.2 < 0.3.0`, and an equal version is not an update.
pub fn is_newer(current: &Version, remote: &Version) -> bool {
    remote > current
}

/// How this build updates.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InstallMode {
    InApp,
    /// deb/rpm: installing would need the package manager (and a password), so the app only
    /// links to the release page.
    DownloadOnly,
}

pub fn install_mode() -> InstallMode {
    match tauri::utils::platform::bundle_type() {
        Some(BundleType::Deb | BundleType::Rpm) => InstallMode::DownloadOnly,
        _ => InstallMode::InApp,
    }
}

#[derive(Debug, Serialize)]
pub struct UpdaterStatus {
    pub current_version: String,
    pub install: InstallMode,
    pub platform: &'static str,
}

#[derive(Debug, Serialize)]
pub struct AvailableUpdate {
    pub version: String,
    pub date: Option<String>,
    pub notes: Option<String>,
    /// The release page, for download-only installs.
    pub download_url: Option<String>,
}

/// Progress of "Install and restart", streamed to the UI.
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum UpdateEvent {
    DownloadStarted {
        total_bytes: Option<u64>,
    },
    Progress {
        downloaded_bytes: u64,
        total_bytes: Option<u64>,
    },
    Installing,
    Restarting,
}

/// The update the last check found (what "Install and restart" installs).
#[derive(Default)]
pub struct PendingUpdate(Mutex<Option<Update>>);

/// `tauri.conf.json` → `plugins.pagelamp-updates`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChannelsConfig {
    stable: Vec<Url>,
    beta: Vec<Url>,
    /// `{version}` is replaced by the new version.
    release_page: String,
}

fn channels_config<R: Runtime>(app: &AppHandle<R>) -> CmdResult<ChannelsConfig> {
    let value = app
        .config()
        .plugins
        .0
        .get("pagelamp-updates")
        .cloned()
        .ok_or_else(|| internal("this build has no update channels configured"))?;
    serde_json::from_value(value).map_err(|err| internal(format!("update channels: {err}")))
}

fn endpoints(config: ChannelsConfig, channel: &UpdateChannel) -> Vec<Url> {
    match channel {
        UpdateChannel::Stable => config.stable,
        UpdateChannel::Beta => config.beta,
    }
}

/// A short code for the diagnostic report (never a message or URL).
fn error_code(err: &tauri_plugin_updater::Error) -> &'static str {
    use tauri_plugin_updater::Error as E;
    match err {
        E::Reqwest(_) | E::Network(_) | E::Http(_) => "network",
        E::Minisign(_)
        | E::Base64(_)
        | E::SignatureUtf8(_)
        | E::SignedVersionMismatch { .. }
        | E::MissingSignedVersion => "signature",
        E::ReleaseNotFound
        | E::TargetNotFound(_)
        | E::TargetsNotFound(_)
        | E::Serialization(_)
        | E::Semver(_)
        | E::UrlParse(_)
        | E::EmptyEndpoints => "manifest",
        E::Io(_)
        | E::BinaryNotFoundInArchive
        | E::InvalidUpdaterFormat
        | E::TempDirNotFound
        | E::TempDirNotOnSameMountPoint
        | E::FailedToDetermineExtractPath => "install",
        _ => "other",
    }
}

fn app_error(err: &tauri_plugin_updater::Error) -> AppError {
    let kind = match error_code(err) {
        "network" => AppErrorKind::Network,
        _ => AppErrorKind::Internal,
    };
    AppError::new(kind, err.to_string())
}

#[tauri::command]
pub fn updates_status() -> UpdaterStatus {
    UpdaterStatus {
        current_version: env!("CARGO_PKG_VERSION").to_string(),
        install: install_mode(),
        platform: std::env::consts::OS,
    }
}

/// Checks the effective channel and records the outcome through the facade.
#[tauri::command]
pub async fn updates_check<R: Runtime>(
    app: AppHandle<R>,
    backend: State<'_, Backend>,
    pending: State<'_, PendingUpdate>,
) -> CmdResult<Option<AvailableUpdate>> {
    let channel = match backend
        .blocking(|facade| facade.effective_update_channel())
        .await
    {
        Ok(channel) => channel,
        Err(err) => {
            tracing::info!(target: "pagelamp::updates", "no stored channel ({}); using the default", err.message);
            default_channel(&current_version())
        }
    };
    let config = channels_config(&app)?;
    let release_page = config.release_page.clone();
    let mode = install_mode();
    let user_agent = format!("{}/{}", brand::PRODUCT_NAME, env!("CARGO_PKG_VERSION"));

    let mut builder = app
        .updater_builder()
        .endpoints(endpoints(config, &channel))
        .map_err(|err| app_error(&err))?
        .header("User-Agent", user_agent)
        .map_err(|err| app_error(&err))?
        .timeout(CHECK_TIMEOUT)
        // Windows: the installer takes over from here (Install is refused during syncs).
        .on_before_exit(
            || tracing::info!(target: "pagelamp::updates", "exiting for the installer"),
        );
    if mode == InstallMode::DownloadOnly {
        builder = builder.target(DOWNLOAD_ONLY_TARGET);
    }
    let result = match builder.build() {
        Ok(updater) => updater.check().await,
        Err(err) => Err(err),
    };

    let outcome = match &result {
        Ok(Some(update)) => UpdateCheckOutcome::Available {
            version: update.version.clone(),
        },
        Ok(None) => UpdateCheckOutcome::UpToDate,
        Err(err) => {
            tracing::warn!(target: "pagelamp::updates", code = error_code(err), "update check failed: {err}");
            UpdateCheckOutcome::Error {
                code: error_code(err).to_string(),
            }
        }
    };
    let record = UpdateCheckRecord {
        at: chrono::Utc::now(),
        channel,
        outcome,
    };
    if let Err(err) = backend
        .blocking(move |facade| facade.record_update_check(record))
        .await
    {
        tracing::warn!(target: "pagelamp::updates", "couldn't record the update check: {}", err.message);
    }

    let found = result.map_err(|err| app_error(&err))?;
    let mut slot = pending
        .0
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let Some(update) = found else {
        *slot = None;
        return Ok(None);
    };
    let available = AvailableUpdate {
        version: update.version.clone(),
        date: update
            .raw_json
            .get("pub_date")
            .and_then(|date| date.as_str())
            .map(str::to_string),
        notes: update.body.clone(),
        download_url: (mode == InstallMode::DownloadOnly)
            .then(|| release_page.replace("{version}", &update.version)),
    };
    *slot = (mode == InstallMode::InApp).then_some(update);
    Ok(Some(available))
}

/// Downloads, verifies and installs the update the last check found, then restarts. Only
/// called after the student confirmed; refused while anything runs (a sync, a download, a Codex
/// install, a model run).
#[tauri::command]
pub async fn updates_install<R: Runtime>(
    app: AppHandle<R>,
    backend: State<'_, Backend>,
    pending: State<'_, PendingUpdate>,
    on_event: Channel<UpdateEvent>,
) -> CmdResult<()> {
    if install_mode() == InstallMode::DownloadOnly {
        return Err(AppError::new(
            AppErrorKind::Invalid,
            "This install updates by downloading the new package.",
        ));
    }
    // Without an open core (e.g. its database is from a newer PageLamp) nothing of ours can be
    // syncing, and installing the update is the way out.
    if let Ok(facade) = backend.app() {
        let activity = facade.activity();
        if !activity.items.is_empty() || activity.other_process_syncing {
            let generating = activity
                .items
                .iter()
                .any(|item| item.kind == ActivityKind::Generation);
            return Err(AppError::new(
                AppErrorKind::Busy,
                if generating {
                    "The AI is still working (reading a syllabus or writing a study plan). \
                     Install the update when it finishes."
                } else {
                    "A sync is running. Install the update when it finishes."
                },
            ));
        }
    }
    let update = pending
        .0
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone()
        .ok_or_else(|| AppError::new(AppErrorKind::NotFound, "Check for updates first."))?;

    let mut downloaded: u64 = 0;
    let mut started = false;
    let progress = on_event.clone();
    let finished = on_event.clone();
    let result = update
        .download_and_install(
            move |chunk, total| {
                if !started {
                    started = true;
                    let _ = progress.send(UpdateEvent::DownloadStarted { total_bytes: total });
                }
                downloaded += chunk as u64;
                let _ = progress.send(UpdateEvent::Progress {
                    downloaded_bytes: downloaded,
                    total_bytes: total,
                });
            },
            move || {
                let _ = finished.send(UpdateEvent::Installing);
            },
        )
        .await;
    // On Windows the installer has already exited the process by now.
    if let Err(err) = result {
        tracing::warn!(target: "pagelamp::updates", code = error_code(&err), "update install failed: {err}");
        return Err(app_error(&err));
    }
    tracing::info!(target: "pagelamp::updates", version = %update.version, "update installed; restarting");
    let _ = on_event.send(UpdateEvent::Restarting);
    app.restart()
}

/// Windows: the NSIS pre-install hook (windows/hooks.nsh) renames a running `pagelamp.exe` (an
/// AI app's MCP server keeps it locked) to `pagelamp.exe.old` (or `.old2`). Once nothing runs it
/// any more, delete it.
pub fn remove_old_sidecar() {
    let binary = crate::backend::pagelamp_binary();
    let Some(name) = binary.file_name() else {
        return;
    };
    for suffix in [".old", ".old2"] {
        let mut old = name.to_os_string();
        old.push(suffix);
        let old = binary.with_file_name(old);
        if old.exists()
            && let Err(err) = std::fs::remove_file(&old)
        {
            // Still in use (an AI app hasn't been restarted yet): try again next launch.
            tracing::info!(target: "pagelamp::updates", "leftover sidecar not removed yet: {err}");
        }
    }
}

/// The updater plugin, comparing versions with the real crate version.
pub fn plugin<R: Runtime>() -> tauri::plugin::TauriPlugin<R, tauri_plugin_updater::Config> {
    tauri_plugin_updater::Builder::new()
        .default_version_comparator(|_, remote| is_newer(&current_version(), &remote.version))
        .build()
}

/// Registers what the updater commands need.
pub fn manage<R: Runtime>(app: &AppHandle<R>) {
    app.manage(PendingUpdate::default());
}

#[cfg(test)]
mod tests {
    use semver::Version;

    use super::{current_version, is_newer};

    fn v(s: &str) -> Version {
        Version::parse(s).expect("test version")
    }

    #[test]
    fn a_beta_is_offered_its_final_release() {
        assert!(is_newer(&v("0.3.0-beta.2"), &v("0.3.0")));
        assert!(is_newer(&v("0.3.0-alpha.1"), &v("0.3.0-alpha.2")));
        assert!(is_newer(&v("0.3.0-alpha.9"), &v("0.3.0-beta.1")));
        assert!(is_newer(&v("0.3.0"), &v("0.3.1")));
    }

    #[test]
    fn without_the_facade_a_pre_release_checks_beta() {
        use super::default_channel;
        use pagelamp_app::UpdateChannel;
        assert!(matches!(
            default_channel(&v("0.3.0-alpha.1")),
            UpdateChannel::Beta
        ));
        assert!(matches!(
            default_channel(&v("0.3.0")),
            UpdateChannel::Stable
        ));
    }

    #[test]
    fn the_same_or_an_older_version_is_not_an_update() {
        assert!(!is_newer(&v("0.3.0"), &v("0.3.0")));
        assert!(!is_newer(&v("0.3.0"), &v("0.3.0-beta.2")));
        assert!(!is_newer(&v("0.3.1"), &v("0.3.0")));
    }

    #[test]
    fn the_shipped_update_settings_are_valid_static_https_urls() {
        use super::ChannelsConfig;
        let config: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).expect("tauri.conf.json");
        let plugins = &config["plugins"];
        // What the plugin itself parses at startup (a bad section would stop the app).
        let _: tauri_plugin_updater::Config =
            serde_json::from_value(plugins["updater"].clone()).expect("plugins.updater");
        let channels: ChannelsConfig =
            serde_json::from_value(plugins["pagelamp-updates"].clone()).expect("channels");
        let urls: Vec<_> = channels.stable.iter().chain(&channels.beta).collect();
        assert!(!channels.stable.is_empty() && !channels.beta.is_empty());
        for url in &urls {
            assert_eq!(url.scheme(), "https", "{url}");
            // Static: no {{target}}/{{arch}}/{{current_version}} sent to the server.
            assert!(
                !url.as_str().contains("%7B%7B") && !url.as_str().contains("{{"),
                "{url}"
            );
        }
        assert!(channels.release_page.contains("{version}"));

        // The rehearsal overlay only swaps the endpoints, and only to the test manifest.
        let overlay: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.rehearsal.conf.json")).expect("overlay");
        let test = &overlay["plugins"]["pagelamp-updates"];
        for channel in ["stable", "beta"] {
            let list: Vec<tauri::Url> =
                serde_json::from_value(test[channel].clone()).expect("urls");
            assert!(
                list.iter()
                    .all(|u| u.as_str().ends_with("/updates/test.json")),
                "{list:?}"
            );
        }
    }

    #[test]
    fn the_running_version_is_the_crate_version() {
        assert_eq!(current_version().to_string(), env!("CARGO_PKG_VERSION"));
    }
}
