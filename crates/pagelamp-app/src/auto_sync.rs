//! Automatic sync, the facade's side (v0.3 alpha.1). The setting, the attempts record and the
//! clock rule live in `pagelamp_core::auto_sync`; this module is what starts and ends a run.
//!
//! - A sync starts only from the student's action in PageLamp or the CLI, or from the app's own
//!   timer under the student's setting: the shell reads `StartupTasks.sync_due` (at launch and
//!   every hour while it runs) and, when it is true for the reason it asks (`AutoSyncTrigger`:
//!   the timer, or the student being at the app), calls `sync_all` with that trigger in
//!   `SyncRequest.automatic`. Nothing else starts one: no MCP tool call can cause, request or
//!   schedule a sync, and the MCP server never reads or writes anything this rule depends on.
//! - `sync_due` is false while this shell's What's new is waiting (an upgrader reads about
//!   automatic sync, and can turn it off there, before the first run), while a sync runs in
//!   any process, when the setting is off, and by the clock rule.
//! - An automatic `sync_all` checks the clock rule again, then counts the attempt before it
//!   does anything else, the lock included: a run that is refused (another sync runs, What's
//!   new is waiting) or fails waits like one that ran (1 h, 2 h, 4 h… up to the interval), and
//!   no source gets more than 6 attempts in 24 hours. Each source waits after its own
//!   attempts only, and each trigger too; the cap is shared. Two shells that both saw `sync_due`
//!   therefore start one run between them.
//! - What a run reads depends on why it started (`scope_of`): from the timer, with nobody known
//!   to be at the app, Canvas is asked only for the course list, planner items and
//!   announcements, since Canvas may record a request for a course's modules, files, pages or
//!   assignments as the student's activity there. Such a light run stamps `sync.light`, never
//!   the source's `last_synced_at`, and a course it finds for the first time is listed with
//!   `structure_pending` until a full sync reads it. When the student is at the app (it was
//!   just opened or brought to the front), a run reads everything, like pressing Sync.
//! - After the student stops a sync (any sync of this app), nothing starts by itself for an
//!   hour.
//! - It reads only the sources that are due, leaves out the ones only the student can fix,
//!   never downloads files, and keeps a failure that may pass by itself (the network,
//!   throttling, a server having trouble) to its attempts record: the source isn't marked
//!   failed, so nothing asks for the student's attention. Any other failure is recorded as
//!   always and shows on Sources; a source only the student can fix (an expired or revoked
//!   token or link, a missing folder) is then left alone until it is fixed.

use pagelamp_core::auto_sync::{
    self as rule, AUTO_SYNC_ATTEMPTS_KEY, AutoSyncTrigger, Clocks, LIGHT_SYNC_KEY, SYNC_PREFS_KEY,
    SyncPrefs, SyncScope,
};
use pagelamp_core::model::{SourceErrorKind, SourceRecord, Timestamp};
use pagelamp_core::paths;
use pagelamp_core::store::Store;

use crate::{App, AppError, AppErrorKind, Result, SourceSyncResult, SyncDue, lock};

/// What each trigger syncs (the shells only ever name the trigger). With nobody at the app,
/// Canvas is asked for nothing with a course in its path (only the course list, deadlines and
/// announcements); with the student at the app, a run reads everything, the same as pressing
/// Sync. Folder and calendar-feed sources are read in full by both. Each scope has
/// its own clock (`pagelamp_core::auto_sync`).
pub(crate) const fn scope_of(trigger: AutoSyncTrigger) -> SyncScope {
    match trigger {
        AutoSyncTrigger::Unattended => SyncScope::UserLevel,
        AutoSyncTrigger::Attended => SyncScope::Everything,
    }
}

impl App {
    /// The student's sync settings: how often PageLamp syncs by itself while it runs (twice a
    /// day unless chosen otherwise; a failed read is an error, never the default).
    pub fn sync_prefs(&self) -> Result<SyncPrefs> {
        Ok(rule::sync_prefs(&self.read_store()?)?)
    }

    /// Change them. Allowed at any time, also while What's new is waiting (its row carries the
    /// control).
    pub fn set_sync_prefs(&self, prefs: SyncPrefs) -> Result<()> {
        Ok(self.write_store()?.set_setting(SYNC_PREFS_KEY, &prefs)?)
    }

    /// `StartupTasks.sync_due` (see the module docs).
    pub(crate) fn sync_due(
        &self,
        store: &Store,
        now: Timestamp,
        whats_new_waiting: bool,
    ) -> Result<SyncDue> {
        if whats_new_waiting || lock::is_locked(&paths::sync_lock_path_in(self.data_dir())) {
            return Ok(SyncDue::default());
        }
        let setting = rule::auto_sync(store)?;
        let sources = store.list_sources()?;
        let attempts = rule::attempts(store)?;
        let light = rule::light_sync(store)?;
        let clocks = Clocks {
            sources: &sources,
            attempts: &attempts,
            light: &light,
        };
        let due = |trigger| rule::due_by_the_clock(setting, clocks, scope_of(trigger), now);
        Ok(SyncDue {
            unattended: due(AutoSyncTrigger::Unattended),
            attended: due(AutoSyncTrigger::Attended),
        })
    }

    /// The start of an automatic `sync_all`, before the lock: `None` when no run is due any
    /// more (nothing is counted); else the attempt is counted and the ids of the sources to
    /// read are returned (the ones that are due, not every source), or the refusal when
    /// What's new is waiting (counted too).
    pub(crate) fn begin_automatic_sync(
        &self,
        trigger: AutoSyncTrigger,
        now: Timestamp,
    ) -> Result<Option<Vec<String>>> {
        let store = self.write_store()?;
        let trying = store.in_transaction(|store| {
            let sources = store.list_sources()?;
            let mut attempts = rule::attempts(store)?;
            let light = rule::light_sync(store)?;
            let scope = scope_of(trigger);
            let ids: Vec<String> = {
                let clocks = Clocks {
                    sources: &sources,
                    attempts: &attempts,
                    light: &light,
                };
                rule::due_now(rule::auto_sync(store)?, clocks, scope, now)
                    .iter()
                    .map(|source| source.id.clone())
                    .collect()
            };
            if ids.is_empty() {
                return Ok(None);
            }
            let due: Vec<&SourceRecord> = sources
                .iter()
                .filter(|source| ids.contains(&source.id))
                .collect();
            attempts.count(&due, now, scope);
            store.set_setting(AUTO_SYNC_ATTEMPTS_KEY, &attempts)?;
            Ok(Some(ids))
        })?;
        drop(store);
        if trying.is_some() && self.whats_new_waiting()? {
            return Err(AppError::new(
                AppErrorKind::Invalid,
                "Automatic sync waits until What's new has been read.",
            ));
        }
        Ok(trying)
    }

    /// A light run of a Canvas source ended well: its deadlines and announcements are as of
    /// `at`, and `new_courses` wait for a full sync. The source's `last_synced_at` isn't
    /// touched (it keeps meaning the last full sync). An error an earlier light run left on
    /// the source is cleared; one a full sync left stays until a full sync succeeds.
    pub(crate) fn record_light_sync(&self, source_id: &str, at: Timestamp, new_courses: &[String]) {
        let recorded = self.write_store().and_then(|store| {
            Ok(store.in_transaction(|store| {
                let mut light = rule::light_sync(store)?;
                light.light_run_ended(source_id, at, new_courses);
                if light.errors.remove(source_id) {
                    store.clear_source_error(source_id)?;
                }
                store.set_setting(LIGHT_SYNC_KEY, &light)
            })?)
        });
        if let Err(err) = recorded {
            tracing::warn!("could not record the light sync: {err}");
        }
    }

    /// A light run of a Canvas source failed in a way that is shown (`automatic_run_records`).
    /// It is recorded on the source unless a full sync's failure is already there: that one
    /// says more, and only a full sync that succeeds may clear it.
    pub(crate) fn record_light_failure(
        &self,
        source_id: &str,
        at: Timestamp,
        error: (SourceErrorKind, &str),
    ) {
        let recorded = self.write_store().and_then(|store| {
            Ok(store.in_transaction(|store| {
                let mut light = rule::light_sync(store)?;
                let has_error = store
                    .get_source(source_id)?
                    .is_some_and(|source| source.last_error.is_some());
                if has_error && !light.errors.contains(source_id) {
                    return Ok(());
                }
                store.record_sync(source_id, at, Some(error))?;
                light.errors.insert(source_id.to_string());
                store.set_setting(LIGHT_SYNC_KEY, &light)
            })?)
        });
        if let Err(err) = recorded {
            tracing::warn!("could not record the light sync's failure: {err}");
        }
    }

    /// A sync that isn't a light run wrote the source's outcome: what is on the source now is
    /// that sync's, not a light run's.
    pub(crate) fn light_error_superseded(&self, source_id: &str) {
        let recorded = self.write_store().and_then(|store| {
            Ok(store.in_transaction(|store| {
                let mut light = rule::light_sync(store)?;
                if light.errors.remove(source_id) {
                    store.set_setting(LIGHT_SYNC_KEY, &light)?;
                }
                Ok(())
            })?)
        });
        if let Err(err) = recorded {
            tracing::warn!("could not update the light sync record: {err}");
        }
    }

    /// A full sync of a Canvas source ended well: its courses don't wait for their structure
    /// any more (`only`: the ones whose structure it read, when it was limited to some
    /// courses). `unread`: the ones it couldn't read this time; a waiting one keeps waiting
    /// but no longer makes a full sync due by itself (`LightSync::full_sync_read`).
    pub(crate) fn structure_was_read(
        &self,
        source_id: &str,
        only: Option<&[String]>,
        unread: &[String],
    ) {
        let recorded = self.write_store().and_then(|store| {
            Ok(store.in_transaction(|store| {
                let mut light = rule::light_sync(store)?;
                if light.full_sync_read(source_id, only, unread) {
                    store.set_setting(LIGHT_SYNC_KEY, &light)?;
                }
                Ok(())
            })?)
        });
        if let Err(err) = recorded {
            tracing::warn!("could not record which courses were read: {err}");
        }
    }

    /// A source was removed: what the automatic sync remembers of it goes too (the light runs'
    /// record, its retry wait and its attempts).
    pub(crate) fn forget_light_sync(&self, store: &Store, source_id: &str) -> Result<()> {
        Ok(store.in_transaction(|store| {
            let mut light = rule::light_sync(store)?;
            if light.forget_source(source_id) {
                store.set_setting(LIGHT_SYNC_KEY, &light)?;
            }
            let mut attempts = rule::attempts(store)?;
            if attempts.forget_source(source_id) {
                store.set_setting(AUTO_SYNC_ATTEMPTS_KEY, &attempts)?;
            }
            Ok(())
        })?)
    }

    /// The student stopped a sync (any sync of this app): nothing starts by itself for an hour.
    pub(crate) fn sync_was_stopped(&self, at: Timestamp) {
        let recorded = self.write_store().and_then(|store| {
            Ok(store.in_transaction(|store| {
                let mut attempts = rule::attempts(store)?;
                attempts.stopped_at = Some(at);
                store.set_setting(AUTO_SYNC_ATTEMPTS_KEY, &attempts)
            })?)
        });
        if let Err(err) = recorded {
            tracing::warn!("could not record that the sync was stopped: {err}");
        }
    }

    /// The end of an automatic run: the retry wait of every source that synced is over. (A
    /// source that didn't keeps its attempt counted as it was at the start.)
    pub(crate) fn finish_automatic_sync(
        &self,
        trigger: AutoSyncTrigger,
        results: &[SourceSyncResult],
        at: Timestamp,
    ) {
        let recorded = self.write_store().and_then(|store| {
            Ok(store.in_transaction(|store| {
                let mut attempts = rule::attempts(store)?;
                for result in results.iter().filter(|result| result.ok) {
                    attempts.succeeded(&result.source_id, scope_of(trigger));
                }
                if results.iter().all(|result| result.ok) {
                    attempts.last_ok_at = Some(at);
                }
                store.set_setting(AUTO_SYNC_ATTEMPTS_KEY, &attempts)
            })?)
        });
        if let Err(err) = recorded {
            tracing::warn!("could not record the automatic sync's outcome: {err}");
        }
    }
}

/// Whether an automatic run records `kind` on the source. Everything except a failure that
/// may pass by itself: the network, throttling, a server having trouble. (An expired token, a
/// missing folder, a database that can't be written or a feed that answers with something else
/// is shown: nothing would fix it but the student.)
pub(crate) fn automatic_run_records(kind: SourceErrorKind) -> bool {
    !matches!(
        kind,
        SourceErrorKind::Network | SourceErrorKind::RateLimited
    )
}
