//! Update preferences, the first-run / upgrade state and update-check records (v0.3 M0.4).
//!
//! The logic lives here, in the facade (plan §1.3, rule 12): the desktop app, the Swift shell
//! and the CLI only render `startup_tasks` and call the setters. Everything is stored in the
//! schema-3 `settings` table:
//!
//! | key                          | value                                                    |
//! |------------------------------|----------------------------------------------------------|
//! | `updates.prefs`              | `UpdatePrefs`                                            |
//! | `updates.disclosure_acknowledged` | `true` once the student saw what the update check sends |
//! | `updates.last_check`         | `UpdateCheckRecord`                                      |
//! | `app.last_run_version`       | the version that last ran `startup_tasks`                |
//! | `app.whats_new_acknowledged` | the version whose What's new the student closed          |
//!
//! `startup_tasks(now)` classifies the launch once per process (fresh install, upgrade and
//! from which version) and records the running version; everything else is recomputed on every
//! call, because the app may run for days and callers poll it on a timer.

use chrono::TimeDelta;
use pagelamp_core::model::Timestamp;
use pagelamp_core::store::Store;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{App, Result};

const PREFS_KEY: &str = "updates.prefs";
const DISCLOSURE_KEY: &str = "updates.disclosure_acknowledged";
const LAST_CHECK_KEY: &str = "updates.last_check";
const LAST_RUN_KEY: &str = "app.last_run_version";
const WHATS_NEW_ACK_KEY: &str = "app.whats_new_acknowledged";

/// How often the automatic update check runs.
const CHECK_INTERVAL: TimeDelta = TimeDelta::hours(24);

/// This build's What's new topics, each with the version that introduced it. A student coming
/// from an older version sees the topics introduced after that version.
const WHATS_NEW: &[(WhatsNewTopic, &str)] = &[
    (WhatsNewTopic::UpdateCheck, "0.3.0-alpha.1"),
    (WhatsNewTopic::CourseWeeks, "0.3.0-alpha.1"),
    (WhatsNewTopic::CourseRemoval, "0.3.0-alpha.2"),
];

/// Where updates come from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum UpdateChannel {
    Stable,
    Beta,
}

/// The student's update settings.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct UpdatePrefs {
    /// Check for updates automatically (daily). Default on (D2).
    pub auto_check: bool,
    /// The chosen channel; `None` = the default for this build (`effective_update_channel`).
    pub channel: Option<UpdateChannel>,
}

impl Default for UpdatePrefs {
    fn default() -> Self {
        UpdatePrefs {
            auto_check: true,
            channel: None,
        }
    }
}

/// A topic of the one-time What's new sheet shown after an update.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WhatsNewTopic {
    /// PageLamp now checks for updates (what it sends, how to turn it off).
    UpdateCheck,
    /// Course weeks, phases and the Past group.
    CourseWeeks,
    /// Removing finished courses: 7 days to undo, the student's own folders untouched.
    CourseRemoval,
}

/// What's new since `since` (`None`: an update from 0.1, which didn't record its version).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct WhatsNew {
    pub since: Option<String>,
    pub topics: Vec<WhatsNewTopic>,
}

/// What the app should do now (M0 subset; `startup_tasks`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct StartupTasks {
    /// Show What's new (upgraders only) until `acknowledge_whats_new`.
    pub whats_new: Option<WhatsNew>,
    /// Run the automatic update check now: it is on, the student saw the disclosure (onboarding
    /// or What's new), no What's new is waiting, and the last check is 24 h or more ago.
    pub update_check_due: bool,
    /// The version this launch updated from (`None`: not an update, or an update from 0.1).
    /// Shows the "quit and reopen your AI app" banner.
    pub updated_from: Option<String>,
}

/// One update check and how it ended (`record_update_check`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct UpdateCheckRecord {
    pub at: Timestamp,
    pub channel: UpdateChannel,
    pub outcome: UpdateCheckOutcome,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum UpdateCheckOutcome {
    UpToDate,
    Available {
        version: String,
    },
    /// A short code (e.g. `network`, `signature`, `manifest`), never a message or URL.
    Error {
        code: String,
    },
}

/// How this process's launch was classified (computed once, by the first `startup_tasks`).
#[derive(Clone, Debug)]
pub(crate) struct LaunchClass {
    /// An update from an older version (including 0.1).
    upgrade: bool,
    /// The version it updated from; `None` for 0.1 or when not an update.
    updated_from: Option<String>,
}

impl App {
    /// The student's update settings (defaults when never set, or unreadable).
    pub fn update_prefs(&self) -> Result<UpdatePrefs> {
        Ok(self
            .read_store()?
            .setting(PREFS_KEY)
            .unwrap_or(None)
            .unwrap_or_default())
    }

    pub fn set_update_prefs(&self, prefs: UpdatePrefs) -> Result<()> {
        Ok(self.write_store()?.set_setting(PREFS_KEY, &prefs)?)
    }

    /// The channel updates come from: the student's choice, else Beta for a pre-release build
    /// (e.g. 0.3.0-alpha.1) and Stable otherwise.
    pub fn effective_update_channel(&self) -> Result<UpdateChannel> {
        Ok(self
            .update_prefs()?
            .channel
            .unwrap_or_else(|| default_channel(env!("CARGO_PKG_VERSION"))))
    }

    /// What to do at launch and on the app's timer (see the module docs). `now` is the caller's
    /// clock, so the answer is testable and follows a long-running app.
    pub fn startup_tasks(&self, now: Timestamp) -> Result<StartupTasks> {
        let launch = self.launch_class()?;
        let store = self.read_store()?;
        let whats_new = if launch.upgrade && !whats_new_acknowledged(&store) {
            let topics = topics_since(launch.updated_from.as_deref());
            (!topics.is_empty()).then(|| WhatsNew {
                since: launch.updated_from.clone(),
                topics,
            })
        } else {
            None
        };
        let prefs: UpdatePrefs = store.setting(PREFS_KEY).unwrap_or(None).unwrap_or_default();
        let disclosed: bool = store
            .setting(DISCLOSURE_KEY)
            .unwrap_or(None)
            .unwrap_or(false);
        let last_check: Option<UpdateCheckRecord> = store.setting(LAST_CHECK_KEY).unwrap_or(None);
        let check_is_old = last_check.is_none_or(|check| now - check.at >= CHECK_INTERVAL);
        Ok(StartupTasks {
            update_check_due: prefs.auto_check && disclosed && whats_new.is_none() && check_is_old,
            whats_new,
            updated_from: launch.updated_from.clone(),
        })
    }

    /// The student closed What's new. Its update-check topic counts as the update disclosure.
    pub fn acknowledge_whats_new(&self) -> Result<()> {
        let launch = self.launch_class()?;
        let store = self.write_store()?;
        store.set_setting(WHATS_NEW_ACK_KEY, &env!("CARGO_PKG_VERSION"))?;
        if topics_since(launch.updated_from.as_deref()).contains(&WhatsNewTopic::UpdateCheck) {
            store.set_setting(DISCLOSURE_KEY, &true)?;
        }
        Ok(())
    }

    /// The student saw what the update check sends (onboarding, fresh installs).
    pub fn acknowledge_update_disclosure(&self) -> Result<()> {
        Ok(self.write_store()?.set_setting(DISCLOSURE_KEY, &true)?)
    }

    /// Remember how an update check ended (for `startup_tasks` and diagnostic reports).
    pub fn record_update_check(&self, record: UpdateCheckRecord) -> Result<()> {
        Ok(self.write_store()?.set_setting(LAST_CHECK_KEY, &record)?)
    }

    pub fn last_update_check(&self) -> Result<Option<UpdateCheckRecord>> {
        Ok(self.read_store()?.setting(LAST_CHECK_KEY).unwrap_or(None))
    }

    /// Classify this launch once per process: a fresh install gets no What's new (its
    /// acknowledgement is set to this version), an upgrade does; the running version is
    /// recorded for the next launch.
    fn launch_class(&self) -> Result<LaunchClass> {
        let mut cached = self.state.launch.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(launch) = cached.as_ref() {
            return Ok(launch.clone());
        }
        let current = env!("CARGO_PKG_VERSION");
        let store = self.write_store()?;
        let last_run: Option<String> = store.setting(LAST_RUN_KEY).unwrap_or(None);
        let launch = match last_run {
            Some(last) if last == current => LaunchClass {
                upgrade: false,
                updated_from: None,
            },
            Some(last) => LaunchClass {
                upgrade: true,
                updated_from: Some(last),
            },
            // 0.1 never recorded a version: it is an update if 0.1 left data behind, as it
            // was when this app opened the data dir (a new user's onboarding adds a source
            // before the shell asks for its startup tasks).
            None => {
                let used_before = self.state.used_before_at_open;
                if !used_before {
                    store.set_setting(WHATS_NEW_ACK_KEY, &current)?;
                }
                LaunchClass {
                    upgrade: used_before,
                    updated_from: None,
                }
            }
        };
        store.set_setting(LAST_RUN_KEY, &current)?;
        *cached = Some(launch.clone());
        Ok(launch)
    }
}

/// Beta for pre-release builds, Stable otherwise.
fn default_channel(version: &str) -> UpdateChannel {
    match semver::Version::parse(version) {
        Ok(v) if v.pre.is_empty() => UpdateChannel::Stable,
        Ok(_) => UpdateChannel::Beta,
        Err(_) => UpdateChannel::Stable,
    }
}

/// The What's new topics introduced after `since` (`None`: from 0.1, i.e. all of them).
fn topics_since(since: Option<&str>) -> Vec<WhatsNewTopic> {
    let since = since.and_then(|v| semver::Version::parse(v).ok());
    WHATS_NEW
        .iter()
        .filter(|(_, introduced)| {
            let introduced = semver::Version::parse(introduced).expect("valid version");
            since.as_ref().is_none_or(|since| *since < introduced)
        })
        .map(|(topic, _)| *topic)
        .collect()
}

fn whats_new_acknowledged(store: &Store) -> bool {
    store
        .setting::<String>(WHATS_NEW_ACK_KEY)
        .unwrap_or(None)
        .is_some_and(|version| version == env!("CARGO_PKG_VERSION"))
}

/// "2026-10-01 14:03 UTC (beta): up_to_date | available 0.3.0-alpha.2 | error network".
pub(crate) fn describe_check(record: &UpdateCheckRecord) -> String {
    let channel = match record.channel {
        UpdateChannel::Stable => "stable",
        UpdateChannel::Beta => "beta",
    };
    let outcome = match &record.outcome {
        UpdateCheckOutcome::UpToDate => "up_to_date".to_string(),
        UpdateCheckOutcome::Available { version } => format!("available {version}"),
        UpdateCheckOutcome::Error { code } => format!("error {code}"),
    };
    format!(
        "{} ({channel}): {outcome}",
        record.at.format("%Y-%m-%d %H:%M UTC")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pre_release_builds_default_to_beta() {
        assert_eq!(default_channel("0.3.0-alpha.1"), UpdateChannel::Beta);
        assert_eq!(default_channel("0.3.0"), UpdateChannel::Stable);
        assert_eq!(default_channel("not a version"), UpdateChannel::Stable);
    }

    #[test]
    fn topics_are_the_ones_introduced_after_the_old_version() {
        use WhatsNewTopic::*;
        let all = [UpdateCheck, CourseWeeks, CourseRemoval];
        assert_eq!(topics_since(None), all);
        assert_eq!(topics_since(Some("0.1.0")), all);
        assert_eq!(topics_since(Some("0.3.0-alpha.1")), [CourseRemoval]);
        assert!(topics_since(Some("0.3.0-alpha.2")).is_empty());
        assert!(topics_since(Some("0.3.0")).is_empty());
    }

    /// Every topic has a row (the version that introduced it) and desktop copy in both
    /// languages: a build never announces a topic without words, nor forgets one.
    #[test]
    fn every_topic_has_a_row_and_desktop_copy() {
        fn names(value: &serde_json::Value, out: &mut Vec<String>) {
            match value {
                serde_json::Value::Object(map) => {
                    for (key, value) in map {
                        match (key.as_str(), value) {
                            ("const", serde_json::Value::String(name)) => out.push(name.clone()),
                            ("enum", serde_json::Value::Array(list)) => out
                                .extend(list.iter().filter_map(|v| v.as_str().map(str::to_string))),
                            _ => names(value, out),
                        }
                    }
                }
                serde_json::Value::Array(items) => items.iter().for_each(|item| names(item, out)),
                _ => {}
            }
        }
        let schema = serde_json::to_value(schemars::schema_for!(WhatsNewTopic)).unwrap();
        let mut topics = Vec::new();
        names(&schema, &mut topics);
        topics.sort();
        topics.dedup();
        assert!(topics.len() >= 3, "{schema}");
        let mut rows: Vec<String> = WHATS_NEW
            .iter()
            .map(|(topic, _)| {
                serde_json::to_value(topic)
                    .unwrap()
                    .as_str()
                    .unwrap()
                    .to_string()
            })
            .collect();
        rows.sort();
        assert_eq!(rows, topics, "one WHATS_NEW row per topic");
        for language in ["en", "zh-CN"] {
            let path = format!(
                "{}/../../apps/desktop/src/i18n/locales/{language}/updates.json",
                env!("CARGO_MANIFEST_DIR")
            );
            let copy: serde_json::Value =
                serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
            for topic in &topics {
                for field in ["title", "body"] {
                    let text = copy["whatsNew"]["topics"][topic][field].as_str();
                    assert!(
                        text.is_some_and(|text| !text.trim().is_empty()),
                        "{language}: whatsNew.topics.{topic}.{field}"
                    );
                }
            }
        }
    }
}
