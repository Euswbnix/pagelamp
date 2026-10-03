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
//!   no source gets more than 6 attempts in 24 hours. Two shells that both saw `sync_due`
//!   therefore start one run between them.
//! - It leaves out the sources only the student can fix, never downloads files, and keeps a
//!   failure that may pass by itself (network, throttling) to its attempts record: the source
//!   isn't marked failed, so nothing asks for the student's attention. A failure the student
//!   must fix (an expired or revoked token or link, a missing folder) is recorded as always
//!   and shows on Sources; that source is then left alone until it is fixed.

use pagelamp_core::auto_sync::{
    self as rule, AUTO_SYNC_ATTEMPTS_KEY, AutoSyncTrigger, SYNC_PREFS_KEY, SyncPrefs,
};
use pagelamp_core::model::{SourceErrorKind, Timestamp};
use pagelamp_core::paths;
use pagelamp_core::store::Store;

use crate::{App, AppError, AppErrorKind, Result, SourceSyncResult, SyncDue, lock};

/// What an automatic run syncs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AutoSyncScope {
    /// Everything a sync the student starts does, except downloading files.
    Everything,
}

/// The scope each trigger runs, with a clock of its own per scope. Both triggers run everything
/// today; if an unattended run is to do less in Canvas than an attended one, this is where
/// that is decided (the shells only ever name the trigger).
pub(crate) const fn scope_of(trigger: AutoSyncTrigger) -> AutoSyncScope {
    match trigger {
        AutoSyncTrigger::Unattended | AutoSyncTrigger::Attended => AutoSyncScope::Everything,
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
        let due = |trigger| match scope_of(trigger) {
            AutoSyncScope::Everything => rule::due_by_the_clock(setting, &sources, &attempts, now),
        };
        Ok(SyncDue {
            unattended: due(AutoSyncTrigger::Unattended),
            attended: due(AutoSyncTrigger::Attended),
        })
    }

    /// The start of an automatic `sync_all`, before the lock: `None` when no run is due any
    /// more (nothing is counted); else the attempt is counted and the ids of the sources to
    /// try are returned, or the refusal when What's new is waiting (counted too).
    pub(crate) fn begin_automatic_sync(
        &self,
        trigger: AutoSyncTrigger,
        now: Timestamp,
    ) -> Result<Option<Vec<String>>> {
        let AutoSyncScope::Everything = scope_of(trigger);
        let store = self.write_store()?;
        let trying = store.in_transaction(|store| {
            let sources = store.list_sources()?;
            let mut attempts = rule::attempts(store)?;
            if !rule::due_by_the_clock(rule::auto_sync(store)?, &sources, &attempts, now) {
                return Ok(None);
            }
            let eligible = rule::eligible(&sources, &attempts, now);
            let ids: Vec<String> = eligible.iter().map(|source| source.id.clone()).collect();
            attempts.count(&eligible, now);
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

    /// The end of an automatic run: when every source it tried synced, the retry wait is over.
    /// (Otherwise the attempt stays counted as it was at the start.)
    pub(crate) fn finish_automatic_sync(&self, results: &[SourceSyncResult], at: Timestamp) {
        if !results.iter().all(|result| result.ok) {
            return;
        }
        let recorded = self.write_store().and_then(|store| {
            Ok(store.in_transaction(|store| {
                let mut attempts = rule::attempts(store)?;
                attempts.succeeded(at);
                store.set_setting(AUTO_SYNC_ATTEMPTS_KEY, &attempts)
            })?)
        });
        if let Err(err) = recorded {
            tracing::warn!("could not record the automatic sync's outcome: {err}");
        }
    }
}

/// Whether an automatic run records `kind` on the source: only what the student must fix.
pub(crate) fn automatic_run_records(kind: SourceErrorKind) -> bool {
    matches!(
        kind,
        SourceErrorKind::AuthExpiredOrRevoked | SourceErrorKind::NotFound
    )
}
