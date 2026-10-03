//! Automatic sync: the student's setting and the record of automatic attempts.
//!
//! Only data and the rule's arithmetic live here, so the MCP server (which depends on this
//! crate alone and never syncs) can say whether PageLamp refreshes by itself and when it last
//! did. Starting a sync is the facade's job (`pagelamp-app`); nothing an MCP client can write
//! is read here.
//!
//! | key                  | value                                                        |
//! |----------------------|--------------------------------------------------------------|
//! | `sync.prefs`         | `SyncPrefs` (absent: automatic sync twice a day)             |
//! | `sync.auto_attempts` | `AutoSyncAttempts`                                           |

use std::collections::BTreeMap;

use chrono::TimeDelta;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::Result;
use crate::model::{SourceErrorKind, SourceRecord, Timestamp};
use crate::store::Store;
use crate::views::STALE_AFTER_HOURS;

/// The setting's key in the `settings` table.
pub const SYNC_PREFS_KEY: &str = "sync.prefs";
/// The attempts record's key in the `settings` table.
pub const AUTO_SYNC_ATTEMPTS_KEY: &str = "sync.auto_attempts";

/// The first retry after an automatic attempt that didn't finish: 1 hour, doubling with each
/// further one (1, 2, 4, 8 h…) up to the setting's interval.
pub const FIRST_RETRY: TimeDelta = TimeDelta::hours(1);
/// The most automatic attempts one source gets in any 24 hours, whatever the backoff says.
pub const MAX_ATTEMPTS_PER_SOURCE_PER_DAY: usize = 6;

/// How often PageLamp syncs by itself while it runs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AutoSync {
    /// Only when the student starts a sync.
    Off,
    /// When the last successful sync is 24 hours old.
    Daily,
    /// When the last successful sync is 12 hours old (the default).
    #[default]
    TwiceDaily,
}

impl AutoSync {
    pub fn as_str(self) -> &'static str {
        match self {
            AutoSync::Off => "off",
            AutoSync::Daily => "daily",
            AutoSync::TwiceDaily => "twice_daily",
        }
    }

    /// How old the last successful sync must be before the next automatic one; `None`: off.
    pub fn interval(self) -> Option<TimeDelta> {
        match self {
            AutoSync::Off => None,
            AutoSync::Daily => Some(TimeDelta::hours(24)),
            AutoSync::TwiceDaily => Some(TimeDelta::hours(12)),
        }
    }

    /// After how long without a successful sync the data counts as stale: the interval and
    /// half of it again (an automatic sync may be an hour late and retry), and never less than
    /// 24 hours. So "once a day" isn't stale every morning.
    pub fn stale_after(self) -> TimeDelta {
        let at_least = TimeDelta::hours(STALE_AFTER_HOURS);
        match self.interval() {
            Some(interval) => (interval + interval / 2).max(at_least),
            None => at_least,
        }
    }
}

/// Why PageLamp starts a sync by itself. The shell says which; what each one syncs is the
/// facade's decision (the shells never choose it).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AutoSyncTrigger {
    /// The app's timer, with nobody known to be at the app.
    Unattended,
    /// The student just opened PageLamp, brought its window to the front or closed What's new.
    Attended,
}

/// The student's sync settings.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct SyncPrefs {
    /// How often PageLamp syncs by itself while it runs. Default twice a day.
    pub auto_sync: AutoSync,
}

/// What PageLamp remembers about its automatic sync attempts. An attempt is counted when it
/// is asked for, before anything else: one that is refused (another sync runs, What's new is
/// waiting) or fails counts as much as one that ran, so nothing retries back to back.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AutoSyncAttempts {
    /// When the last automatic attempt was asked for.
    pub last_at: Option<Timestamp>,
    /// Attempts in a row that didn't end with every source it could sync synced.
    pub failed: u32,
    /// When an automatic run last ended with every source it could sync synced.
    pub last_ok_at: Option<Timestamp>,
    /// Per source id: when its automatic attempts of the last 24 hours started.
    pub by_source: BTreeMap<String, Vec<Timestamp>>,
}

impl AutoSyncAttempts {
    /// The earliest moment the next automatic attempt may start: after a failed or refused one,
    /// 1 h, 2 h, 4 h… later, at most the setting's interval; `None`: no wait.
    pub fn retry_not_before(&self, interval: TimeDelta) -> Option<Timestamp> {
        let last = self.last_at?;
        if self.failed == 0 {
            return None;
        }
        let doubled = FIRST_RETRY
            .checked_mul(1 << (self.failed - 1).min(16))
            .unwrap_or(interval);
        Some(last + doubled.min(interval))
    }

    /// How many automatic attempts `source_id` had in the 24 hours before `now`.
    pub fn attempts_in_last_day(&self, source_id: &str, now: Timestamp) -> usize {
        let since = now - TimeDelta::hours(24);
        self.by_source
            .get(source_id)
            .map_or(0, |starts| starts.iter().filter(|at| **at > since).count())
    }

    /// Count an attempt on `sources` at `now` (and forget starts older than 24 hours).
    pub fn count(&mut self, sources: &[&SourceRecord], now: Timestamp) {
        let since = now - TimeDelta::hours(24);
        self.by_source.retain(|_, starts| {
            starts.retain(|at| *at > since);
            !starts.is_empty()
        });
        for source in sources {
            self.by_source
                .entry(source.id.clone())
                .or_default()
                .push(now);
        }
        self.last_at = Some(now);
        self.failed = self.failed.saturating_add(1);
    }

    /// The attempt counted last ended with every source it could sync synced.
    pub fn succeeded(&mut self, at: Timestamp) {
        self.failed = 0;
        self.last_ok_at = Some(at);
    }
}

/// The student's sync settings (the defaults when never set or unreadable; a failed read is
/// an error, never the default).
pub fn sync_prefs(store: &Store) -> Result<SyncPrefs> {
    Ok(store.setting_or_absent(SYNC_PREFS_KEY)?.unwrap_or_default())
}

/// How often PageLamp syncs by itself (`sync_prefs`).
pub fn auto_sync(store: &Store) -> Result<AutoSync> {
    Ok(sync_prefs(store)?.auto_sync)
}

/// The attempts record (empty when never written or unreadable; a failed read is an error).
pub fn attempts(store: &Store) -> Result<AutoSyncAttempts> {
    Ok(store
        .setting_or_absent(AUTO_SYNC_ATTEMPTS_KEY)?
        .unwrap_or_default())
}

/// Whether an automatic sync may try `source` at all: not while its last sync failed for a
/// reason only the student can fix (an expired or revoked token or link, a missing folder or
/// address). Replacing the token, link or folder, or a sync the student starts, clears that.
pub fn needs_the_student(source: &SourceRecord) -> bool {
    matches!(
        source.last_error_kind,
        Some(SourceErrorKind::AuthExpiredOrRevoked | SourceErrorKind::NotFound)
    )
}

/// The sources an automatic sync would try at `now`: those that don't need the student and
/// are under the daily cap.
pub fn eligible<'a>(
    sources: &'a [SourceRecord],
    attempts: &AutoSyncAttempts,
    now: Timestamp,
) -> Vec<&'a SourceRecord> {
    sources
        .iter()
        .filter(|source| !needs_the_student(source))
        .filter(|source| {
            attempts.attempts_in_last_day(&source.id, now) < MAX_ATTEMPTS_PER_SOURCE_PER_DAY
        })
        .collect()
}

/// Whether an automatic sync is due at `now` by the clock alone (the facade adds what only it
/// knows: a sync running, What's new waiting): the setting is on, a source can be tried, the
/// stalest of those was last synced the interval ago or never, and no retry wait is running.
pub fn due_by_the_clock(
    setting: AutoSync,
    sources: &[SourceRecord],
    attempts: &AutoSyncAttempts,
    now: Timestamp,
) -> bool {
    let Some(interval) = setting.interval() else {
        return false;
    };
    let eligible = eligible(sources, attempts, now);
    if eligible.is_empty() {
        return false;
    }
    let old = eligible
        .iter()
        .any(|source| source.last_synced_at.is_none_or(|at| now - at >= interval));
    let waiting = attempts
        .retry_not_before(interval)
        .is_some_and(|not_before| now < not_before);
    old && !waiting
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::SourceKind;

    fn at(text: &str) -> Timestamp {
        text.parse().unwrap()
    }

    fn source(id: &str, synced: Option<&str>, error: Option<SourceErrorKind>) -> SourceRecord {
        SourceRecord {
            id: id.into(),
            kind: SourceKind::Canvas,
            label: id.into(),
            config: serde_json::json!({}),
            last_synced_at: synced.map(at),
            last_error: error.map(|_| "failed".to_string()),
            last_error_kind: error,
        }
    }

    #[test]
    fn the_setting_names_its_interval_and_when_data_is_stale() {
        assert_eq!(AutoSync::default(), AutoSync::TwiceDaily);
        assert_eq!(AutoSync::Off.interval(), None);
        assert_eq!(AutoSync::Daily.interval(), Some(TimeDelta::hours(24)));
        assert_eq!(AutoSync::TwiceDaily.interval(), Some(TimeDelta::hours(12)));
        // Never stale within the interval: once a day is stale after 36 h, not 24.
        assert_eq!(AutoSync::Off.stale_after(), TimeDelta::hours(24));
        assert_eq!(AutoSync::TwiceDaily.stale_after(), TimeDelta::hours(24));
        assert_eq!(AutoSync::Daily.stale_after(), TimeDelta::hours(36));
        for setting in [AutoSync::Off, AutoSync::Daily, AutoSync::TwiceDaily] {
            let json = serde_json::to_string(&setting).unwrap();
            assert_eq!(json, format!("\"{}\"", setting.as_str()));
        }
    }

    #[test]
    fn due_when_the_stalest_source_that_can_be_tried_is_an_interval_old() {
        let now = at("2026-10-03T12:00:00Z");
        let none = AutoSyncAttempts::default();
        let fresh = source("a", Some("2026-10-03T03:00:00Z"), None);
        let old = source("b", Some("2026-10-02T23:59:00Z"), None);
        let never = source("c", None, None);
        let due =
            |setting, sources: &[SourceRecord]| due_by_the_clock(setting, sources, &none, now);
        assert!(!due(AutoSync::TwiceDaily, &[]), "no sources");
        assert!(
            !due(AutoSync::TwiceDaily, std::slice::from_ref(&fresh)),
            "9 h old"
        );
        assert!(due(AutoSync::TwiceDaily, &[fresh.clone(), old.clone()]));
        assert!(!due(AutoSync::Daily, &[fresh.clone(), old.clone()]));
        assert!(
            due(AutoSync::Daily, std::slice::from_ref(&never)),
            "never synced"
        );
        assert!(!due(AutoSync::Off, &[old.clone(), never.clone()]));
        // A source only the student can fix is neither tried nor a reason to try.
        for kind in [
            SourceErrorKind::AuthExpiredOrRevoked,
            SourceErrorKind::NotFound,
        ] {
            let blocked = source("d", Some("2026-09-01T00:00:00Z"), Some(kind));
            assert!(needs_the_student(&blocked));
            assert!(!due(AutoSync::TwiceDaily, std::slice::from_ref(&blocked)));
            assert!(!due(AutoSync::TwiceDaily, &[blocked, fresh.clone()]));
        }
        // A failure that may pass by itself is tried again.
        let flaky = source(
            "e",
            Some("2026-09-01T00:00:00Z"),
            Some(SourceErrorKind::Network),
        );
        assert!(due(AutoSync::TwiceDaily, &[flaky]));
    }

    #[test]
    fn a_failed_or_refused_attempt_waits_longer_each_time_up_to_the_interval() {
        let old = [source("a", Some("2026-10-01T00:00:00Z"), None)];
        let refs: Vec<&SourceRecord> = old.iter().collect();
        let mut attempts = AutoSyncAttempts::default();
        let start = at("2026-10-03T00:00:00Z");
        let twice = AutoSync::TwiceDaily;
        let due = |attempts: &AutoSyncAttempts, hours: i64| {
            due_by_the_clock(twice, &old, attempts, start + TimeDelta::hours(hours))
        };
        assert!(due(&attempts, 0));
        // Counted when asked for: nothing retries back to back.
        attempts.count(&refs, start);
        assert_eq!(attempts.failed, 1);
        assert!(!due(&attempts, 0));
        assert!(
            !due(&attempts, 0) && due(&attempts, 1),
            "1 h after the first"
        );
        attempts.count(&refs, start + TimeDelta::hours(1));
        assert!(
            !due(&attempts, 2) && due(&attempts, 3),
            "2 h after the second"
        );
        attempts.count(&refs, start + TimeDelta::hours(3));
        assert!(
            !due(&attempts, 6) && due(&attempts, 7),
            "4 h after the third"
        );
        attempts.count(&refs, start + TimeDelta::hours(7));
        assert!(
            !due(&attempts, 14) && due(&attempts, 15),
            "8 h after the fourth"
        );
        attempts.count(&refs, start + TimeDelta::hours(15));
        // 16 h would be next: capped at the interval (12 h).
        assert!(!due(&attempts, 26) && due(&attempts, 27));
        // A run that ends well clears the wait.
        attempts.succeeded(start + TimeDelta::hours(27));
        assert_eq!(
            (
                attempts.failed,
                attempts.retry_not_before(TimeDelta::hours(12))
            ),
            (0, None)
        );
    }

    #[test]
    fn a_source_gets_at_most_six_automatic_attempts_a_day() {
        let sources = [source("a", None, None), source("b", None, None)];
        let only_a = [&sources[0]];
        let mut attempts = AutoSyncAttempts::default();
        let start = at("2026-10-03T00:00:00Z");
        for hour in 0..6 {
            attempts.count(&only_a, start + TimeDelta::hours(hour));
        }
        let now = start + TimeDelta::hours(6);
        assert_eq!(attempts.attempts_in_last_day("a", now), 6);
        let ids = |attempts: &AutoSyncAttempts, now| -> Vec<String> {
            eligible(&sources, attempts, now)
                .iter()
                .map(|s| s.id.clone())
                .collect()
        };
        assert_eq!(ids(&attempts, now), ["b"], "a is at its cap");
        // 24 h after its first attempt, a may be tried again.
        let later = start + TimeDelta::hours(24) + TimeDelta::minutes(1);
        assert_eq!(ids(&attempts, later), ["a", "b"]);
        // Counting forgets what is older than a day.
        attempts.count(&[], later);
        assert_eq!(attempts.by_source["a"].len(), 5);
    }
}
